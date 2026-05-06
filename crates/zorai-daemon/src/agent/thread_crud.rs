#![allow(dead_code)]

//! Thread CRUD operations — list, get, delete, planner detection.

use super::*;
use serde::{Deserialize, Serialize};

const SESSION_ABANDON_WINDOW_MS: u64 = 30_000;
const LAZY_CAPPED_IPC_MESSAGE_WINDOW: usize = 64;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct ThreadListFilter {
    pub created_after: Option<u64>,
    pub created_before: Option<u64>,
    pub updated_after: Option<u64>,
    pub updated_before: Option<u64>,
    pub agent_name: Option<String>,
    pub title_query: Option<String>,
    pub pinned: Option<bool>,
    pub include_internal: bool,
    pub limit: Option<usize>,
    pub offset: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ThreadDetailResult {
    pub thread: AgentThread,
    pub messages_truncated: bool,
    pub total_message_count: usize,
    pub loaded_message_start: usize,
    pub loaded_message_end: usize,
    pub active_context_window_start: usize,
    pub active_context_window_end: usize,
    pub active_context_window_tokens: u64,
    #[serde(default)]
    pub pinned_messages: Vec<PinnedThreadMessageSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct PinnedThreadMessageSummary {
    pub message_id: String,
    pub absolute_index: usize,
    pub role: MessageRole,
    pub content: String,
}

fn pinned_message_summaries(thread: &AgentThread) -> Vec<PinnedThreadMessageSummary> {
    thread
        .messages
        .iter()
        .enumerate()
        .filter(|(_, message)| message.pinned_for_compaction)
        .map(|(absolute_index, message)| PinnedThreadMessageSummary {
            message_id: message.id.clone(),
            absolute_index,
            role: message.role,
            content: message.content.clone(),
        })
        .collect()
}

fn thread_context_window_summary(messages: &[AgentMessage]) -> (usize, usize, u64) {
    let (start, active_messages) = active_compaction_window(messages);
    (
        start,
        messages.len(),
        estimate_message_tokens(active_messages) as u64,
    )
}

async fn persisted_pinned_message_summaries(
    engine: &AgentEngine,
    thread_id: &str,
) -> Vec<PinnedThreadMessageSummary> {
    let Ok(rows) = engine
        .history
        .list_pinned_messages_for_compaction(thread_id)
        .await
    else {
        return Vec::new();
    };

    rows.into_iter()
        .filter_map(|(absolute_index, row)| {
            super::messaging::agent_message_from_db(row).map(|message| PinnedThreadMessageSummary {
                message_id: message.id,
                absolute_index,
                role: message.role,
                content: message.content,
            })
        })
        .collect()
}

fn thread_detail_frame_fits_ipc(thread: &Option<AgentThread>) -> bool {
    let Ok(thread_json) = serde_json::to_string(thread) else {
        return false;
    };

    zorai_protocol::daemon_message_fits_ipc(&zorai_protocol::DaemonMessage::AgentThreadDetail {
        thread_json,
    })
}

fn cap_thread_detail_for_ipc(detail: ThreadDetailResult) -> ThreadDetailResult {
    if thread_detail_frame_fits_ipc(&Some(detail.thread.clone())) {
        return detail;
    }

    let mut low = 0usize;
    let mut high = detail.thread.messages.len();
    while low < high {
        let mid = low + (high - low) / 2;
        let mut candidate = detail.thread.clone();
        candidate.messages = candidate.messages[mid..].to_vec();
        if thread_detail_frame_fits_ipc(&Some(candidate)) {
            high = mid;
        } else {
            low = mid + 1;
        }
    }

    let mut thread = detail.thread;
    thread.messages = thread.messages[low..].to_vec();

    ThreadDetailResult {
        thread,
        messages_truncated: detail.messages_truncated || low > 0,
        total_message_count: detail.total_message_count,
        loaded_message_start: detail.loaded_message_start + low,
        loaded_message_end: detail.loaded_message_end,
        active_context_window_start: detail.active_context_window_start,
        active_context_window_end: detail.active_context_window_end,
        active_context_window_tokens: detail.active_context_window_tokens,
        pinned_messages: detail.pinned_messages,
    }
}

impl AgentEngine {
    pub(super) async fn append_system_thread_message(
        &self,
        thread_id: &str,
        content: impl Into<String>,
    ) -> bool {
        let content = content.into();
        let appended = {
            let mut threads = self.threads.write().await;
            let Some(thread) = threads.get_mut(thread_id) else {
                return false;
            };
            thread.messages.push(AgentMessage {
                id: generate_message_id(),
                role: MessageRole::System,
                content,
                content_blocks: Vec::new(),
                tool_calls: None,
                tool_call_id: None,
                tool_name: None,
                tool_arguments: None,
                tool_status: None,
                weles_review: None,
                input_tokens: 0,
                output_tokens: 0,
                cost: None,
                provider: None,
                model: None,
                api_transport: None,
                response_id: None,
                upstream_message: None,
                provider_final_result: None,
                author_agent_id: None,
                author_agent_name: None,
                reasoning: None,
                message_kind: crate::agent::types::AgentMessageKind::Normal,
                compaction_strategy: None,
                compaction_payload: None,
                offloaded_payload_id: None,
                tool_output_preview_path: None,
                structural_refs: Vec::new(),
                pinned_for_compaction: false,
                timestamp: now_millis(),
            });
            thread.updated_at = now_millis();
            true
        };

        if appended {
            self.persist_thread_by_id(thread_id).await;
            let _ = self.event_tx.send(AgentEvent::ThreadReloadRequired {
                thread_id: thread_id.to_string(),
            });
        }

        appended
    }

    pub(crate) async fn pin_thread_message_for_compaction(
        &self,
        thread_id: &str,
        message_id: &str,
    ) -> ThreadMessagePinMutationResult {
        self.ensure_thread_messages_loaded(thread_id).await;
        let config = self.config.read().await.clone();
        let provider_config = match resolve_active_provider_config(&config) {
            Ok(provider_config) => provider_config,
            Err(error) => {
                return ThreadMessagePinMutationResult::failure(
                    thread_id,
                    message_id,
                    format!("provider_config_unavailable:{error}"),
                    0,
                    0,
                    None,
                );
            }
        };

        let result = {
            let mut threads = self.threads.write().await;
            let Some(thread) = threads.get_mut(thread_id) else {
                return ThreadMessagePinMutationResult::failure(
                    thread_id,
                    message_id,
                    "thread_not_found",
                    0,
                    pinned_for_compaction_budget_chars(&config, &provider_config),
                    None,
                );
            };

            let result =
                pin_thread_message_for_compaction(thread, message_id, &config, &provider_config);
            if result.ok {
                thread.updated_at = now_millis();
            }
            result
        };

        if result.ok {
            self.persist_thread_by_id(thread_id).await;
            let _ = self.event_tx.send(AgentEvent::ThreadReloadRequired {
                thread_id: thread_id.to_string(),
            });
        }

        result
    }

    pub(crate) async fn unpin_thread_message_for_compaction(
        &self,
        thread_id: &str,
        message_id: &str,
    ) -> ThreadMessagePinMutationResult {
        self.ensure_thread_messages_loaded(thread_id).await;
        let config = self.config.read().await.clone();
        let provider_config = match resolve_active_provider_config(&config) {
            Ok(provider_config) => provider_config,
            Err(error) => {
                return ThreadMessagePinMutationResult::failure(
                    thread_id,
                    message_id,
                    format!("provider_config_unavailable:{error}"),
                    0,
                    0,
                    None,
                );
            }
        };

        let result = {
            let mut threads = self.threads.write().await;
            let Some(thread) = threads.get_mut(thread_id) else {
                return ThreadMessagePinMutationResult::failure(
                    thread_id,
                    message_id,
                    "thread_not_found",
                    0,
                    pinned_for_compaction_budget_chars(&config, &provider_config),
                    None,
                );
            };

            let result =
                unpin_thread_message_for_compaction(thread, message_id, &config, &provider_config);
            if result.ok {
                thread.updated_at = now_millis();
            }
            result
        };

        if result.ok {
            self.persist_thread_by_id(thread_id).await;
            let _ = self.event_tx.send(AgentEvent::ThreadReloadRequired {
                thread_id: thread_id.to_string(),
            });
        }

        result
    }

    pub async fn set_thread_client_surface(
        &self,
        thread_id: &str,
        client_surface: zorai_protocol::ClientSurface,
    ) {
        self.thread_client_surfaces
            .write()
            .await
            .insert(thread_id.to_string(), client_surface);
        let thread_exists = self.threads.read().await.contains_key(thread_id);
        if thread_exists {
            self.persist_thread_by_id(thread_id).await;
        }
    }

    pub async fn get_thread_client_surface(
        &self,
        thread_id: &str,
    ) -> Option<zorai_protocol::ClientSurface> {
        self.thread_client_surfaces
            .read()
            .await
            .get(thread_id)
            .copied()
    }

    pub async fn clear_thread_client_surface(&self, thread_id: &str) {
        self.thread_client_surfaces.write().await.remove(thread_id);
    }

    pub async fn set_thread_skill_discovery_state(
        &self,
        thread_id: &str,
        state: LatestSkillDiscoveryState,
    ) {
        self.thread_skill_discovery_states
            .write()
            .await
            .insert(thread_id.to_string(), state);
        let thread_exists = self.threads.read().await.contains_key(thread_id);
        if thread_exists {
            self.persist_thread_by_id(thread_id).await;
        }
    }

    pub async fn get_thread_skill_discovery_state(
        &self,
        thread_id: &str,
    ) -> Option<LatestSkillDiscoveryState> {
        self.thread_skill_discovery_states
            .read()
            .await
            .get(thread_id)
            .cloned()
    }

    pub async fn clear_thread_skill_discovery_state(&self, thread_id: &str) {
        self.thread_skill_discovery_states
            .write()
            .await
            .remove(thread_id);
        let thread_exists = self.threads.read().await.contains_key(thread_id);
        if thread_exists {
            self.persist_thread_by_id(thread_id).await;
        }
    }

    pub async fn set_thread_memory_injection_state(
        &self,
        thread_id: &str,
        state: PromptMemoryInjectionState,
    ) {
        self.thread_memory_injection_state_map()
            .write()
            .await
            .insert(thread_id.to_string(), state);
        let thread_exists = self.threads.read().await.contains_key(thread_id);
        if thread_exists {
            self.persist_thread_by_id(thread_id).await;
        }
    }

    pub async fn get_thread_memory_injection_state(
        &self,
        thread_id: &str,
    ) -> Option<PromptMemoryInjectionState> {
        self.thread_memory_injection_state_map()
            .read()
            .await
            .get(thread_id)
            .cloned()
    }

    pub async fn clear_thread_memory_injection_state(&self, thread_id: &str) {
        self.thread_memory_injection_state_map()
            .write()
            .await
            .remove(thread_id);
        let thread_exists = self.threads.read().await.contains_key(thread_id);
        if thread_exists {
            self.persist_thread_by_id(thread_id).await;
        }
    }

    pub async fn set_goal_run_client_surface(
        &self,
        goal_run_id: &str,
        client_surface: zorai_protocol::ClientSurface,
    ) {
        self.goal_run_client_surfaces
            .write()
            .await
            .insert(goal_run_id.to_string(), client_surface);
    }

    pub async fn get_goal_run_client_surface(
        &self,
        goal_run_id: &str,
    ) -> Option<zorai_protocol::ClientSurface> {
        self.goal_run_client_surfaces
            .read()
            .await
            .get(goal_run_id)
            .copied()
    }

    pub async fn list_threads(&self) -> Vec<AgentThread> {
        self.list_threads_filtered(&ThreadListFilter::default())
            .await
    }

    pub async fn list_threads_paginated(
        &self,
        limit: Option<usize>,
        offset: usize,
        include_internal: bool,
    ) -> Vec<AgentThread> {
        self.list_threads_filtered(&ThreadListFilter {
            limit,
            offset,
            include_internal,
            ..ThreadListFilter::default()
        })
        .await
    }

    pub(crate) async fn list_threads_filtered(
        &self,
        filter: &ThreadListFilter,
    ) -> Vec<AgentThread> {
        let config = self.config.read().await.clone();
        let query = persisted_thread_list_query(filter, &config);
        match self.history.list_threads_filtered(&query).await {
            Ok(rows) if !rows.is_empty() || self.threads.read().await.is_empty() => {
                return rows
                    .into_iter()
                    .map(summarize_persisted_thread_for_list)
                    .collect();
            }
            Ok(_) => {}
            Err(error) => {
                tracing::warn!(%error, "failed to list persisted thread summaries with SQL");
            }
        }

        self.list_threads_filtered_from_memory(filter).await
    }

    async fn list_threads_filtered_from_memory(
        &self,
        filter: &ThreadListFilter,
    ) -> Vec<AgentThread> {
        let threads = self.threads.read().await;
        let mut list: Vec<AgentThread> = threads
            .values()
            .filter(|thread| thread_matches_list_filter(thread, filter))
            .map(summarize_thread_for_list)
            .collect();

        list.sort_by(|a, b| {
            b.updated_at
                .cmp(&a.updated_at)
                .then_with(|| a.id.cmp(&b.id))
        });

        let limit = filter.limit.unwrap_or(usize::MAX);
        list.into_iter().skip(filter.offset).take(limit).collect()
    }

    pub async fn get_thread(&self, thread_id: &str) -> Option<AgentThread> {
        self.get_thread_filtered(thread_id, false, None, 0)
            .await
            .map(|result| result.thread)
    }

    pub(crate) async fn get_thread_filtered(
        &self,
        thread_id: &str,
        include_internal: bool,
        message_limit: Option<usize>,
        message_offset: usize,
    ) -> Option<ThreadDetailResult> {
        if let Some(limit) = message_limit {
            if let Some(detail) = self
                .get_lazy_persisted_thread_window(
                    thread_id,
                    include_internal,
                    limit,
                    message_offset,
                )
                .await
            {
                tracing::info!(
                    thread_id,
                    message_limit = limit,
                    message_offset,
                    loaded_message_start = detail.loaded_message_start,
                    loaded_message_end = detail.loaded_message_end,
                    total_message_count = detail.total_message_count,
                    returned_messages = detail.thread.messages.len(),
                    source = "db",
                    "daemon loaded thread message window"
                );
                return Some(detail);
            }
        }

        if let Some(detail) = self
            .get_authoritative_persisted_thread_detail(
                thread_id,
                include_internal,
                message_limit,
                message_offset,
            )
            .await
        {
            tracing::info!(
                thread_id,
                message_limit,
                message_offset,
                loaded_message_start = detail.loaded_message_start,
                loaded_message_end = detail.loaded_message_end,
                total_message_count = detail.total_message_count,
                returned_messages = detail.thread.messages.len(),
                source = "db",
                "daemon loaded thread message window"
            );
            return Some(detail);
        }

        self.ensure_thread_messages_loaded(thread_id).await;
        let mut thread = self.threads.read().await.get(thread_id).cloned()?;
        if !thread_is_query_visible(&thread, include_internal) {
            return None;
        }
        let pinned_messages = pinned_message_summaries(&thread);
        let (active_context_window_start, active_context_window_end, active_context_window_tokens) =
            thread_context_window_summary(&thread.messages);

        let total_messages = thread.messages.len();
        let end = total_messages.saturating_sub(message_offset);
        let start = message_limit
            .map(|limit| end.saturating_sub(limit))
            .unwrap_or(0);
        let messages_truncated = start > 0 || end < total_messages;

        if messages_truncated {
            thread.messages = thread
                .messages
                .into_iter()
                .skip(start)
                .take(end.saturating_sub(start))
                .collect();
        }

        let detail = ThreadDetailResult {
            thread,
            messages_truncated,
            total_message_count: total_messages,
            loaded_message_start: start,
            loaded_message_end: end,
            active_context_window_start,
            active_context_window_end,
            active_context_window_tokens,
            pinned_messages,
        };
        tracing::info!(
            thread_id,
            message_limit,
            message_offset,
            loaded_message_start = detail.loaded_message_start,
            loaded_message_end = detail.loaded_message_end,
            total_message_count = detail.total_message_count,
            returned_messages = detail.thread.messages.len(),
            source = "memory",
            "daemon loaded thread message window"
        );
        Some(detail)
    }

    async fn get_authoritative_persisted_thread_detail(
        &self,
        thread_id: &str,
        include_internal: bool,
        message_limit: Option<usize>,
        message_offset: usize,
    ) -> Option<ThreadDetailResult> {
        let existing_pinned = self
            .threads
            .read()
            .await
            .get(thread_id)
            .map(|thread| thread.pinned);
        let mut thread = self.restore_thread_with_messages_from_db(thread_id).await?;
        if let Some(pinned) = existing_pinned {
            thread.pinned = pinned;
        }

        let pinned_messages = persisted_pinned_message_summaries(self, thread_id).await;
        let (active_context_window_start, active_context_window_end, active_context_window_tokens) =
            match self.history.list_active_context_window(thread_id).await {
                Ok((rows, absolute_start, absolute_end)) => {
                    let messages = rows
                        .into_iter()
                        .filter_map(super::messaging::agent_message_from_db)
                        .collect::<Vec<_>>();
                    let (relative_start, _, tokens) = thread_context_window_summary(&messages);
                    (
                        absolute_start.saturating_add(relative_start),
                        absolute_end,
                        tokens,
                    )
                }
                Err(_) => thread_context_window_summary(&thread.messages),
            };

        let total_messages = thread.messages.len();
        {
            let mut threads = self.threads.write().await;
            threads.insert(thread_id.to_string(), thread.clone());
        }
        if !thread_is_query_visible(&thread, include_internal) {
            return None;
        }

        let end = total_messages.saturating_sub(message_offset);
        let start = message_limit
            .map(|limit| end.saturating_sub(limit))
            .unwrap_or(0);
        let messages_truncated = start > 0 || end < total_messages;
        if messages_truncated {
            thread.messages = thread
                .messages
                .into_iter()
                .skip(start)
                .take(end.saturating_sub(start))
                .collect();
        }

        Some(ThreadDetailResult {
            thread,
            messages_truncated,
            total_message_count: total_messages,
            loaded_message_start: start,
            loaded_message_end: end,
            active_context_window_start,
            active_context_window_end,
            active_context_window_tokens,
            pinned_messages,
        })
    }

    async fn get_lazy_persisted_thread_window(
        &self,
        thread_id: &str,
        include_internal: bool,
        message_limit: usize,
        message_offset: usize,
    ) -> Option<ThreadDetailResult> {
        if !self
            .thread_message_hydration_pending
            .read()
            .await
            .contains(thread_id)
        {
            return None;
        }

        let _guard = self.thread_message_hydration_lock.lock().await;
        if !self
            .thread_message_hydration_pending
            .read()
            .await
            .contains(thread_id)
        {
            return None;
        }

        let mut thread = self.threads.read().await.get(thread_id).cloned()?;
        if !thread_is_query_visible(&thread, include_internal) {
            return None;
        }

        let (db_messages, total_message_count, loaded_message_start, loaded_message_end) = self
            .history
            .list_message_window(thread_id, message_limit, message_offset)
            .await
            .ok()?;
        thread.messages = db_messages
            .into_iter()
            .filter_map(super::messaging::agent_message_from_db)
            .collect();

        let pinned_messages = persisted_pinned_message_summaries(self, thread_id).await;
        let (active_context_window_start, active_context_window_end, active_context_window_tokens) =
            match self.history.list_active_context_window(thread_id).await {
                Ok((rows, absolute_start, absolute_end)) => {
                    let messages = rows
                        .into_iter()
                        .filter_map(super::messaging::agent_message_from_db)
                        .collect::<Vec<_>>();
                    let (relative_start, _, tokens) = thread_context_window_summary(&messages);
                    (
                        absolute_start.saturating_add(relative_start),
                        absolute_end,
                        tokens,
                    )
                }
                Err(_) => thread_context_window_summary(&thread.messages),
            };

        let detail = ThreadDetailResult {
            thread,
            messages_truncated: loaded_message_start > 0
                || loaded_message_end < total_message_count,
            total_message_count,
            loaded_message_start,
            loaded_message_end,
            active_context_window_start,
            active_context_window_end,
            active_context_window_tokens,
            pinned_messages,
        };
        tracing::info!(
            thread_id,
            message_limit,
            message_offset,
            loaded_message_start = detail.loaded_message_start,
            loaded_message_end = detail.loaded_message_end,
            total_message_count = detail.total_message_count,
            returned_messages = detail.thread.messages.len(),
            source = "db",
            "daemon loaded thread message window"
        );
        Some(detail)
    }

    pub(crate) async fn get_thread_capped_for_ipc(
        &self,
        thread_id: &str,
        include_internal: bool,
    ) -> Option<ThreadDetailResult> {
        if self
            .thread_message_hydration_pending
            .read()
            .await
            .contains(thread_id)
        {
            if let Some(detail) = self
                .get_lazy_persisted_thread_window(
                    thread_id,
                    include_internal,
                    LAZY_CAPPED_IPC_MESSAGE_WINDOW,
                    0,
                )
                .await
            {
                return Some(cap_thread_detail_for_ipc(detail));
            }
        }

        let detail = self
            .get_thread_filtered(thread_id, include_internal, None, 0)
            .await?;

        Some(cap_thread_detail_for_ipc(detail))
    }

    pub async fn planner_required_for_thread(&self, thread_id: &str) -> bool {
        self.ensure_thread_messages_loaded(thread_id).await;
        let threads = self.threads.read().await;
        let Some(thread) = threads.get(thread_id) else {
            return false;
        };
        let latest_user_message = thread
            .messages
            .iter()
            .rev()
            .find(|message| message.role == MessageRole::User)
            .map(|message| message.content.as_str())
            .unwrap_or("");
        planner_required_for_message(latest_user_message)
    }

    pub async fn delete_thread(&self, thread_id: &str) -> bool {
        self.ensure_thread_messages_loaded(thread_id).await;
        let _ = self.stop_stream(thread_id).await;
        let thread_snapshot = self.threads.read().await.get(thread_id).cloned();
        let removed = self.threads.write().await.remove(thread_id).is_some();
        if removed {
            if let Some(thread) = thread_snapshot.as_ref() {
                self.maybe_record_session_abandon_on_thread_delete(thread)
                    .await;
            }
            self.clear_thread_client_surface(thread_id).await;
            self.clear_thread_skill_discovery_state(thread_id).await;
            self.clear_thread_memory_injection_state(thread_id).await;
            self.clear_thread_structural_memory(thread_id).await;
            self.thread_identity_metadata
                .write()
                .await
                .remove(thread_id);
            self.thread_handoff_states.write().await.remove(thread_id);
            self.thread_participants.write().await.remove(thread_id);
            self.thread_participant_suggestions
                .write()
                .await
                .remove(thread_id);
            self.clear_thread_message_hydration_pending(thread_id).await;
            self.remove_repo_watcher(thread_id).await;
            self.thread_todos.write().await.remove(thread_id);
            self.thread_work_contexts.write().await.remove(thread_id);
            if let Err(error) = self.history.delete_thread(thread_id).await {
                tracing::warn!(thread_id = %thread_id, %error, "failed to delete thread history");
            }
            self.persist_threads().await;
            self.persist_todos().await;
            self.persist_work_context().await;
        }
        removed
    }

    async fn maybe_record_session_abandon_on_thread_delete(&self, thread: &AgentThread) {
        let now = now_millis();
        let Some(last_assistant) = thread
            .messages
            .iter()
            .rev()
            .find(|message| message.role == MessageRole::Assistant)
        else {
            return;
        };

        if now.saturating_sub(last_assistant.timestamp) > SESSION_ABANDON_WINDOW_MS {
            return;
        }

        let last_user_after_assistant = thread
            .messages
            .iter()
            .rev()
            .find(|message| message.role == MessageRole::User)
            .is_some_and(|message| message.timestamp > last_assistant.timestamp);
        if last_user_after_assistant {
            return;
        }

        if self
            .history
            .implicit_signal_exists(&thread.id, "session_abandon")
            .await
            .unwrap_or(false)
        {
            return;
        }

        if let Err(error) = self
            .record_session_abandon_feedback(
                &thread.id,
                last_assistant.content.trim(),
                last_assistant.timestamp,
                now,
            )
            .await
        {
            tracing::warn!(
                thread_id = %thread.id,
                error = %error,
                "failed to record session abandonment feedback on thread delete"
            );
        }
    }
}

fn thread_is_visible_by_default(thread: &AgentThread) -> bool {
    !crate::agent::concierge::is_user_visible_thread(thread)
        && !crate::agent::agent_identity::is_participant_playground_thread(&thread.id)
        && !crate::agent::is_internal_handoff_thread(&thread.id)
}

fn thread_is_query_visible(thread: &AgentThread, include_internal: bool) -> bool {
    include_internal || thread_is_visible_by_default(thread)
}

fn list_filter_timestamp(value: Option<u64>) -> Option<i64> {
    value.map(|value| value.min(i64::MAX as u64) as i64)
}

fn persisted_thread_list_query(
    filter: &ThreadListFilter,
    config: &AgentConfig,
) -> crate::history::AgentThreadListQuery {
    let (agent_names, include_empty_agent_name) =
        persisted_thread_agent_name_filter(filter.agent_name.as_deref(), config);

    crate::history::AgentThreadListQuery {
        created_after: list_filter_timestamp(filter.created_after),
        created_before: list_filter_timestamp(filter.created_before),
        updated_after: list_filter_timestamp(filter.updated_after),
        updated_before: list_filter_timestamp(filter.updated_before),
        agent_names,
        include_empty_agent_name,
        title_query: filter
            .title_query
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned),
        title_excluded_prefixes: Vec::new(),
        pinned: filter.pinned,
        min_message_count: None,
        include_internal: filter.include_internal,
        excluded_ids: Vec::new(),
        hidden_id_prefixes: vec![
            crate::agent::agent_identity::PARTICIPANT_PLAYGROUND_THREAD_PREFIX.to_string(),
            crate::agent::thread_handoffs::INTERNAL_HANDOFF_THREAD_PREFIX.to_string(),
        ],
        hidden_message_substrings: vec![
            format!(
                "{} {}",
                crate::agent::agent_identity::PERSONA_ID_MARKER,
                crate::agent::agent_identity::WELES_AGENT_ID
            ),
            format!(
                "{} {}",
                crate::agent::agent_identity::PERSONA_ID_MARKER,
                crate::agent::agent_identity::WELES_GOVERNANCE_SCOPE
            ),
            format!(
                "{} {}",
                crate::agent::agent_identity::PERSONA_ID_MARKER,
                crate::agent::agent_identity::WELES_VITALITY_SCOPE
            ),
        ],
        limit: filter.limit,
        offset: filter.offset,
    }
}

fn persisted_thread_agent_name_filter(
    agent_name: Option<&str>,
    config: &AgentConfig,
) -> (Vec<String>, bool) {
    let Some(agent_name) = agent_name.map(str::trim).filter(|value| !value.is_empty()) else {
        return (Vec::new(), false);
    };

    let target = crate::agent::agent_identity::resolve_agent_target(agent_name, &config.sub_agents);
    let mut names = vec![target.agent_name, agent_name.to_string()];
    let canonical = canonical_thread_agent_name(Some(agent_name));
    names.push(canonical.to_string());

    let include_empty_agent_name =
        canonical.eq_ignore_ascii_case(crate::agent::agent_identity::MAIN_AGENT_NAME);
    if include_empty_agent_name {
        names.extend([
            crate::agent::agent_identity::MAIN_AGENT_ID.to_string(),
            crate::agent::agent_identity::MAIN_AGENT_PUBLIC_ALIAS.to_string(),
            crate::agent::agent_identity::MAIN_AGENT_ALIAS.to_string(),
            crate::agent::agent_identity::MAIN_AGENT_LEGACY_ALIAS.to_string(),
            crate::agent::agent_identity::MAIN_AGENT_FALLBACK_ALIAS.to_string(),
        ]);
    }

    names.sort_by_key(|name| name.to_ascii_lowercase());
    names.dedup_by(|a, b| a.eq_ignore_ascii_case(b));
    (names, include_empty_agent_name)
}

fn summarize_persisted_thread_for_list(thread: zorai_protocol::AgentDbThread) -> AgentThread {
    let metadata = parse_thread_metadata(thread.metadata_json.as_deref());
    AgentThread {
        id: thread.id,
        agent_name: thread
            .agent_name
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty()),
        title: thread.title,
        messages: Vec::new(),
        pinned: metadata.pinned,
        upstream_thread_id: metadata.upstream_thread_id,
        upstream_transport: metadata.upstream_transport,
        upstream_provider: metadata.upstream_provider,
        upstream_model: metadata.upstream_model,
        upstream_assistant_id: metadata.upstream_assistant_id,
        created_at: thread.created_at.max(0) as u64,
        updated_at: thread.updated_at.max(0) as u64,
        total_input_tokens: thread.total_tokens.max(0) as u64,
        total_output_tokens: 0,
    }
}

fn canonical_thread_agent_name(agent_name: Option<&str>) -> &'static str {
    let normalized = agent_name.unwrap_or("").trim();
    if normalized.is_empty() {
        return crate::agent::agent_identity::canonical_agent_name(
            crate::agent::agent_identity::MAIN_AGENT_ID,
        );
    }

    crate::agent::agent_identity::canonical_agent_name(normalized)
}

fn thread_matches_list_filter(thread: &AgentThread, filter: &ThreadListFilter) -> bool {
    if !thread_is_query_visible(thread, filter.include_internal) {
        return false;
    }

    if let Some(created_after) = filter.created_after {
        if thread.created_at < created_after {
            return false;
        }
    }

    if let Some(created_before) = filter.created_before {
        if thread.created_at > created_before {
            return false;
        }
    }

    if let Some(updated_after) = filter.updated_after {
        if thread.updated_at < updated_after {
            return false;
        }
    }

    if let Some(updated_before) = filter.updated_before {
        if thread.updated_at > updated_before {
            return false;
        }
    }

    if let Some(pinned) = filter.pinned {
        if thread.pinned != pinned {
            return false;
        }
    }

    if let Some(agent_name) = filter.agent_name.as_deref() {
        let expected = canonical_thread_agent_name(Some(agent_name));
        let actual = canonical_thread_agent_name(thread.agent_name.as_deref());
        if !actual.eq_ignore_ascii_case(expected) {
            return false;
        }
    }

    if let Some(title_query) = filter
        .title_query
        .as_deref()
        .map(str::trim)
        .filter(|query| !query.is_empty())
    {
        if !thread
            .title
            .to_ascii_lowercase()
            .contains(&title_query.to_ascii_lowercase())
        {
            return false;
        }
    }

    true
}

fn summarize_thread_for_list(thread: &AgentThread) -> AgentThread {
    AgentThread {
        id: thread.id.clone(),
        agent_name: thread.agent_name.clone(),
        title: thread.title.clone(),
        messages: Vec::new(),
        pinned: thread.pinned,
        upstream_thread_id: thread.upstream_thread_id.clone(),
        upstream_transport: thread.upstream_transport,
        upstream_provider: thread.upstream_provider.clone(),
        upstream_model: thread.upstream_model.clone(),
        upstream_assistant_id: thread.upstream_assistant_id.clone(),
        created_at: thread.created_at,
        updated_at: thread.updated_at,
        total_input_tokens: thread.total_input_tokens,
        total_output_tokens: thread.total_output_tokens,
    }
}

#[cfg(test)]
#[path = "tests/thread_crud.rs"]
mod tests;
