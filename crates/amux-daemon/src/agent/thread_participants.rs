use super::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThreadParticipantStatus {
    Active,
    Inactive,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThreadParticipantCommandAction {
    Upsert,
    Deactivate,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThreadParticipantState {
    pub agent_id: String,
    pub agent_name: String,
    pub instruction: String,
    pub status: ThreadParticipantStatus,
    pub created_at: u64,
    pub updated_at: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deactivated_at: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_contribution_at: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThreadParticipantSuggestionStatus {
    Queued,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThreadParticipantSuggestion {
    pub id: String,
    pub target_agent_id: String,
    pub target_agent_name: String,
    pub instruction: String,
    #[serde(default)]
    pub force_send: bool,
    pub status: ThreadParticipantSuggestionStatus,
    pub created_at: u64,
    pub updated_at: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

pub(super) fn normalize_thread_participants(
    participants: Vec<ThreadParticipantState>,
) -> Vec<ThreadParticipantState> {
    participants
        .into_iter()
        .map(|mut participant| {
            let canonical_id = canonical_agent_id(&participant.agent_id).to_string();
            participant.agent_id = canonical_id.clone();
            if participant.agent_name.trim().is_empty() {
                participant.agent_name = canonical_agent_name(&canonical_id).to_string();
            }
            participant
        })
        .collect()
}

fn is_known_main_agent_alias(alias: &str) -> bool {
    matches!(
        alias.trim().to_ascii_lowercase().as_str(),
        MAIN_AGENT_ID | MAIN_AGENT_ALIAS | MAIN_AGENT_LEGACY_ALIAS | MAIN_AGENT_FALLBACK_ALIAS
    )
}

fn is_known_builtin_agent_alias(alias: &str) -> bool {
    let trimmed = alias.trim();
    !trimmed.is_empty()
        && (canonical_agent_id(trimmed) != MAIN_AGENT_ID || is_known_main_agent_alias(trimmed))
}

fn is_participant_deactivation_action(action: &str) -> bool {
    matches!(
        action.trim().to_ascii_lowercase().as_str(),
        "deactivate" | "leave" | "stop" | "done" | "return"
    )
}

impl AgentEngine {
    pub(in crate::agent) async fn resolve_thread_participant_target(
        &self,
        alias: &str,
    ) -> Result<(String, String)> {
        let trimmed = alias.trim();
        if trimmed.is_empty() {
            anyhow::bail!("target agent cannot be empty");
        }

        if is_known_builtin_agent_alias(trimmed) {
            let canonical_id = canonical_agent_id(trimmed).to_string();
            return Ok((
                canonical_id.clone(),
                canonical_agent_name(&canonical_id).to_string(),
            ));
        }

        let normalized = trimmed.to_ascii_lowercase();
        if let Some(sub_agent) = self.list_sub_agents().await.into_iter().find(|entry| {
            entry.id.eq_ignore_ascii_case(&normalized) || entry.name.eq_ignore_ascii_case(trimmed)
        }) {
            return Ok((sub_agent.id, sub_agent.name));
        }

        anyhow::bail!("unknown agent target: {trimmed}");
    }

    async fn latest_visible_user_message_content(&self, thread_id: &str) -> Option<String> {
        let threads = self.threads.read().await;
        threads.get(thread_id).and_then(|thread| {
            thread
                .messages
                .iter()
                .rev()
                .find(|message| message.role == MessageRole::User)
                .map(|message| message.content.clone())
        })
    }

    pub(in crate::agent) async fn build_internal_delegate_payload(
        &self,
        thread_id: Option<&str>,
        content: &str,
        request_visible_thread_continuation: bool,
    ) -> String {
        let mut payload = String::new();
        if let Some(thread_id) = thread_id {
            payload.push_str(&format!(
                "Visible thread id: {thread_id}\nThread delegation mode: internal_hidden\n"
            ));
            payload.push_str(&format!(
                "Continuation requested on visible thread: {}\n",
                if request_visible_thread_continuation {
                    "yes"
                } else {
                    "no"
                }
            ));
            if request_visible_thread_continuation {
                payload.push_str("Do not continue work in this internal DM thread.\n");
            }
            payload.push('\n');
            if let Some(thread) = self.get_thread(thread_id).await {
                payload.push_str(&format!("Visible thread title: {}\n", thread.title.trim()));
                let recent_messages = thread
                    .messages
                    .iter()
                    .rev()
                    .take(8)
                    .cloned()
                    .collect::<Vec<_>>()
                    .into_iter()
                    .rev()
                    .collect::<Vec<_>>();
                if !recent_messages.is_empty() {
                    payload.push_str("Recent visible thread messages:\n");
                    for message in recent_messages {
                        let role = match message.role {
                            MessageRole::Assistant => "assistant",
                            MessageRole::System => "system",
                            MessageRole::Tool => "tool",
                            _ => "user",
                        };
                        payload.push_str(&format!("- {role}: {}\n", message.content.trim()));
                    }
                    payload.push('\n');
                }
            }
        }
        payload.push_str("Delegation request:\n");
        payload.push_str(content.trim());
        payload
    }

    pub(in crate::agent) async fn build_visible_thread_continuation_prompt(
        &self,
        thread_id: &str,
        sender: &str,
        target_agent_id: &str,
        content: &str,
    ) -> String {
        let sender_name = canonical_agent_name(sender);
        let target_agent_name = canonical_agent_name(target_agent_id);
        let latest_operator_request = self
            .latest_visible_user_message_content(thread_id)
            .await
            .unwrap_or_default();
        if latest_operator_request.trim().is_empty() {
            format!(
                "Continue the visible operator thread as {}. This continuation was explicitly requested in an internal DM from {}. Internal DMs are discussion-only; do not continue work there.\n\nInternal delegation request:\n{}",
                target_agent_name,
                sender_name,
                content.trim()
            )
        } else {
            format!(
                "Continue the visible operator thread as {}. This continuation was explicitly requested in an internal DM from {}. Internal DMs are discussion-only; do not continue work there.\n\nInternal delegation request:\n{}\n\nLatest operator request already on this thread:\n{}",
                target_agent_name,
                sender_name,
                content.trim(),
                latest_operator_request.trim()
            )
        }
    }

    pub(in crate::agent) async fn enqueue_visible_thread_continuation(
        &self,
        thread_id: &str,
        continuation: DeferredVisibleThreadContinuation,
    ) {
        let mut queued = self.deferred_visible_thread_continuations.lock().await;
        queued
            .entry(thread_id.to_string())
            .or_default()
            .push(continuation);
    }

    pub(in crate::agent) async fn flush_deferred_visible_thread_continuations(
        &self,
        thread_id: &str,
    ) -> Result<()> {
        let continuations = {
            let mut queued = self.deferred_visible_thread_continuations.lock().await;
            queued.remove(thread_id).unwrap_or_default()
        };
        for continuation in continuations {
            self.continue_visible_thread_as_agent(
                thread_id,
                &continuation.agent_id,
                continuation.preferred_session_hint.as_deref(),
                &continuation.llm_user_content,
            )
            .await?;
        }
        Ok(())
    }

    async fn continue_visible_thread_as_agent(
        &self,
        thread_id: &str,
        agent_id: &str,
        preferred_session_hint: Option<&str>,
        llm_user_content: &str,
    ) -> Result<SendMessageOutcome> {
        if !self.threads.read().await.contains_key(thread_id) {
            anyhow::bail!("thread not found: {thread_id}");
        }

        let stored_user_content = self
            .latest_visible_user_message_content(thread_id)
            .await
            .unwrap_or_else(|| llm_user_content.to_string());
        let mut current_thread_id = thread_id.to_string();
        let mut current_llm_user_content = llm_user_content.to_string();
        let mut current_agent_scope_id = canonical_agent_id(agent_id).to_string();

        loop {
            let thread_for_turn = current_thread_id.clone();
            let stored_user_content_for_turn = stored_user_content.clone();
            let llm_user_content_for_turn = current_llm_user_content.clone();
            let client_surface_for_turn = self.get_thread_client_surface(&thread_for_turn).await;
            let outcome = Box::pin(run_with_agent_scope(current_agent_scope_id.clone(), async move {
                self.run_internal_send_loop(
                    Some(thread_for_turn.as_str()),
                    &stored_user_content_for_turn,
                    &llm_user_content_for_turn,
                    None,
                    preferred_session_hint,
                    None,
                    client_surface_for_turn,
                    false,
                    true,
                )
                .await
            }))
            .await?;

            if let Some(restart) = outcome.handoff_restart.clone() {
                current_thread_id = outcome.thread_id.clone();
                current_llm_user_content = restart.llm_user_content;
                current_agent_scope_id = self
                    .agent_scope_id_for_turn(Some(&current_thread_id), None)
                    .await;
                continue;
            }

            if !outcome.interrupted_for_approval {
                Box::pin(
                    self.maybe_auto_send_next_thread_participant_suggestion(&outcome.thread_id),
                )
                .await?;
            }
            return Ok(outcome);
        }
    }

    pub async fn list_thread_participants(&self, thread_id: &str) -> Vec<ThreadParticipantState> {
        self.thread_participants
            .read()
            .await
            .get(thread_id)
            .cloned()
            .unwrap_or_default()
    }

    pub async fn list_thread_participant_suggestions(
        &self,
        thread_id: &str,
    ) -> Vec<ThreadParticipantSuggestion> {
        self.thread_participant_suggestions
            .read()
            .await
            .get(thread_id)
            .cloned()
            .unwrap_or_default()
    }

    pub async fn queue_thread_participant_suggestion(
        &self,
        thread_id: &str,
        target_agent_id: &str,
        instruction: &str,
        force_send: bool,
    ) -> Result<ThreadParticipantSuggestion> {
        if !self.threads.read().await.contains_key(thread_id) {
            anyhow::bail!("thread not found: {thread_id}");
        }

        let trimmed_instruction = instruction.trim();
        if trimmed_instruction.is_empty() {
            anyhow::bail!("participant suggestion instruction cannot be empty");
        }

        let (agent_id, agent_name) = self
            .resolve_thread_participant_target(target_agent_id)
            .await?;
        let now = now_millis();
        let suggestion = ThreadParticipantSuggestion {
            id: uuid::Uuid::new_v4().to_string(),
            target_agent_id: agent_id,
            target_agent_name: agent_name,
            instruction: trimmed_instruction.to_string(),
            force_send,
            status: ThreadParticipantSuggestionStatus::Queued,
            created_at: now,
            updated_at: now,
            error: None,
        };

        self.thread_participant_suggestions
            .write()
            .await
            .entry(thread_id.to_string())
            .or_default()
            .push(suggestion.clone());

        self.persist_thread_by_id(thread_id).await;
        let _ = self.event_tx.send(AgentEvent::ParticipantSuggestion {
            thread_id: thread_id.to_string(),
            suggestion: suggestion.clone(),
        });
        let _ = self.event_tx.send(AgentEvent::ThreadReloadRequired {
            thread_id: thread_id.to_string(),
        });
        let _ = self
            .record_behavioral_event(
                "participant_suggestion",
                BehavioralEventContext {
                    thread_id: Some(thread_id),
                    task_id: None,
                    goal_run_id: None,
                    approval_id: None,
                },
                serde_json::json!({
                    "action": "queued",
                    "suggestion": suggestion,
                }),
            )
            .await;

        if suggestion.force_send {
            self.send_thread_participant_suggestion(thread_id, &suggestion.id, None)
                .await?;
        } else {
            let _ = self
                .maybe_auto_send_next_thread_participant_suggestion(thread_id)
                .await?;
        }

        Ok(suggestion)
    }

    pub async fn dismiss_thread_participant_suggestion(
        &self,
        thread_id: &str,
        suggestion_id: &str,
    ) -> Result<bool> {
        if !self.threads.read().await.contains_key(thread_id) {
            anyhow::bail!("thread not found: {thread_id}");
        }

        let mut suggestions = self.thread_participant_suggestions.write().await;
        let (changed, removed_suggestion, remove_entry) = {
            let Some(entry) = suggestions.get_mut(thread_id) else {
                return Ok(false);
            };
            let initial_len = entry.len();
            let removed_suggestion = entry
                .iter()
                .find(|suggestion| suggestion.id == suggestion_id)
                .cloned();
            entry.retain(|suggestion| suggestion.id != suggestion_id);
            (
                entry.len() != initial_len,
                removed_suggestion,
                entry.is_empty(),
            )
        };
        if remove_entry {
            suggestions.remove(thread_id);
        }
        drop(suggestions);

        if changed {
            self.persist_thread_by_id(thread_id).await;
            let _ = self.event_tx.send(AgentEvent::ThreadReloadRequired {
                thread_id: thread_id.to_string(),
            });
            if let Some(suggestion) = removed_suggestion {
                let _ = self
                    .record_behavioral_event(
                        "participant_suggestion",
                        BehavioralEventContext {
                            thread_id: Some(thread_id),
                            task_id: None,
                            goal_run_id: None,
                            approval_id: None,
                        },
                        serde_json::json!({
                            "action": "dismissed",
                            "suggestion": suggestion,
                        }),
                    )
                    .await;
            }
        }

        Ok(changed)
    }

    pub async fn fail_thread_participant_suggestion(
        &self,
        thread_id: &str,
        suggestion_id: &str,
        error: &str,
    ) -> Result<Option<ThreadParticipantSuggestion>> {
        if !self.threads.read().await.contains_key(thread_id) {
            anyhow::bail!("thread not found: {thread_id}");
        }

        let trimmed_error = error.trim();
        let mut suggestions = self.thread_participant_suggestions.write().await;
        let updated = suggestions
            .get_mut(thread_id)
            .and_then(|entry| {
                entry
                    .iter_mut()
                    .find(|suggestion| suggestion.id == suggestion_id)
            })
            .map(|suggestion| {
                suggestion.status = ThreadParticipantSuggestionStatus::Failed;
                suggestion.updated_at = now_millis();
                suggestion.error = (!trimmed_error.is_empty()).then(|| trimmed_error.to_string());
                suggestion.clone()
            });
        drop(suggestions);

        if updated.is_some() {
            self.persist_thread_by_id(thread_id).await;
            if let Some(suggestion) = updated.clone() {
                let _ = self.event_tx.send(AgentEvent::ParticipantSuggestion {
                    thread_id: thread_id.to_string(),
                    suggestion: suggestion.clone(),
                });
                let _ = self
                    .record_behavioral_event(
                        "participant_suggestion",
                        BehavioralEventContext {
                            thread_id: Some(thread_id),
                            task_id: None,
                            goal_run_id: None,
                            approval_id: None,
                        },
                        serde_json::json!({
                            "action": "failed",
                            "suggestion": suggestion,
                        }),
                    )
                    .await;
            }
            let _ = self.event_tx.send(AgentEvent::ThreadReloadRequired {
                thread_id: thread_id.to_string(),
            });
        }

        Ok(updated)
    }

    pub async fn send_thread_participant_suggestion(
        &self,
        thread_id: &str,
        suggestion_id: &str,
        preferred_session_hint: Option<&str>,
    ) -> Result<bool> {
        if !self.threads.read().await.contains_key(thread_id) {
            anyhow::bail!("thread not found: {thread_id}");
        }

        let suggestion = self
            .list_thread_participant_suggestions(thread_id)
            .await
            .into_iter()
            .find(|entry| entry.id == suggestion_id)
            .ok_or_else(|| anyhow::anyhow!("participant suggestion not found: {suggestion_id}"))?;

        match self
            .append_visible_thread_participant_message(
                thread_id,
                &suggestion.target_agent_id,
                &suggestion.instruction,
            )
            .await
        {
            Ok(()) => {
                let _ = self
                    .dismiss_thread_participant_suggestion(thread_id, suggestion_id)
                    .await?;
                let _ = self
                    .record_behavioral_event(
                        "participant_suggestion",
                        BehavioralEventContext {
                            thread_id: Some(thread_id),
                            task_id: None,
                            goal_run_id: None,
                            approval_id: None,
                        },
                        serde_json::json!({
                            "action": "sent",
                            "suggestion": suggestion,
                        }),
                    )
                    .await;
                self.continue_thread_after_participant_post_or_notice(thread_id)
                    .await;
                Ok(true)
            }
            Err(error) => {
                let _ = self
                    .fail_thread_participant_suggestion(
                        thread_id,
                        suggestion_id,
                        &error.to_string(),
                    )
                    .await?;
                Err(error)
            }
        }
    }

    pub async fn agent_thread_detail_json(&self, thread_id: &str) -> String {
        let thread = self.get_thread(thread_id).await;
        let mut value = serde_json::to_value(thread).unwrap_or(serde_json::Value::Null);

        if let Some(detail) = value.as_object_mut() {
            let participants = self.list_thread_participants(thread_id).await;
            let suggestions = self.list_thread_participant_suggestions(thread_id).await;
            detail.insert(
                "thread_participants".to_string(),
                serde_json::to_value(participants).unwrap_or(serde_json::Value::Array(Vec::new())),
            );
            detail.insert(
                "queued_participant_suggestions".to_string(),
                serde_json::to_value(suggestions).unwrap_or(serde_json::Value::Array(Vec::new())),
            );
        }

        serde_json::to_string(&value).unwrap_or_else(|_| "null".to_string())
    }

    pub(crate) async fn continue_thread_after_participant_post_or_notice(&self, thread_id: &str) {
        let (prior_user_message, latest_participant_author_id) =
            self.threads.read().await.get(thread_id).map_or((None, None), |thread| {
                let prior_user_message = thread
                    .messages
                    .iter()
                    .rev()
                    .find(|message| message.role == MessageRole::User)
                    .map(|message| message.content.clone());
                let latest_participant_author_id = thread.messages.last().and_then(|message| {
                    (message.role == MessageRole::Assistant)
                        .then(|| message.author_agent_id.clone())
                        .flatten()
                });
                (prior_user_message, latest_participant_author_id)
            });
        let Some(prior_user_message) = prior_user_message.filter(|value| !value.trim().is_empty())
        else {
            return;
        };

        if let Some(latest_participant_author_id) = latest_participant_author_id {
            if self
                .active_agent_id_for_thread(thread_id)
                .await
                .as_deref()
                == Some(latest_participant_author_id.as_str())
            {
                tracing::info!(
                    thread_id = %thread_id,
                    participant = %latest_participant_author_id,
                    "skipping participant follow-up continuation because the participant is already the active responder"
                );
                return;
            }
        }

        if let Err(error) = self
            .continue_existing_user_message_without_queue_drain(thread_id, &prior_user_message)
            .await
        {
            tracing::warn!(
                thread_id = %thread_id,
                error = %error,
                "participant follow-up continuation failed"
            );
            let _ = self.event_tx.send(AgentEvent::WorkflowNotice {
                thread_id: thread_id.to_string(),
                kind: "participant_follow_up_error".to_string(),
                message: "participant follow-up failed".to_string(),
                details: Some(error.to_string()),
            });
        }
    }

    pub(crate) async fn maybe_auto_send_next_thread_participant_suggestion(
        &self,
        thread_id: &str,
    ) -> Result<bool> {
        {
            let streams = self.stream_cancellations.lock().await;
            if streams.contains_key(thread_id) {
                return Ok(false);
            }
        }

        let next_suggestion = self
            .list_thread_participant_suggestions(thread_id)
            .await
            .into_iter()
            .find(|suggestion| suggestion.status == ThreadParticipantSuggestionStatus::Queued);
        let Some(next_suggestion) = next_suggestion else {
            return Ok(false);
        };

        tracing::info!(
            thread_id = %thread_id,
            participant = %next_suggestion.target_agent_id,
            suggestion_id = %next_suggestion.id,
            "auto-sending queued participant suggestion after thread became idle"
        );
        self.send_thread_participant_suggestion(thread_id, &next_suggestion.id, None)
            .await
    }

    pub async fn upsert_thread_participant(
        &self,
        thread_id: &str,
        target_agent_id: &str,
        instruction: &str,
    ) -> Result<ThreadParticipantState> {
        if !self.threads.read().await.contains_key(thread_id) {
            anyhow::bail!("thread not found: {thread_id}");
        }

        let trimmed_instruction = instruction.trim();
        if trimmed_instruction.is_empty() {
            anyhow::bail!("participant instruction cannot be empty");
        }

        let (agent_id, agent_name) = self
            .resolve_thread_participant_target(target_agent_id)
            .await?;
        let now = now_millis();
        let mut participants = self.thread_participants.write().await;
        let entry = participants.entry(thread_id.to_string()).or_default();
        let updated = if let Some(existing) = entry
            .iter_mut()
            .find(|participant| participant.agent_id.eq_ignore_ascii_case(&agent_id))
        {
            existing.agent_id = agent_id.clone();
            existing.agent_name = agent_name.clone();
            existing.instruction = trimmed_instruction.to_string();
            existing.status = ThreadParticipantStatus::Active;
            existing.updated_at = now;
            existing.deactivated_at = None;
            existing.clone()
        } else {
            let state = ThreadParticipantState {
                agent_id: agent_id.clone(),
                agent_name: agent_name.clone(),
                instruction: trimmed_instruction.to_string(),
                status: ThreadParticipantStatus::Active,
                created_at: now,
                updated_at: now,
                deactivated_at: None,
                last_contribution_at: None,
            };
            entry.push(state.clone());
            state
        };
        *entry = normalize_thread_participants(entry.clone());
        drop(participants);

        self.persist_thread_by_id(thread_id).await;
        let _ = self.event_tx.send(AgentEvent::ThreadReloadRequired {
            thread_id: thread_id.to_string(),
        });

        Ok(updated)
    }

    pub async fn deactivate_thread_participant(
        &self,
        thread_id: &str,
        target_agent_id: &str,
    ) -> Result<Option<ThreadParticipantState>> {
        if !self.threads.read().await.contains_key(thread_id) {
            anyhow::bail!("thread not found: {thread_id}");
        }

        let (agent_id, _) = self
            .resolve_thread_participant_target(target_agent_id)
            .await?;
        let now = now_millis();
        let mut participants = self.thread_participants.write().await;
        let Some(entry) = participants.get_mut(thread_id) else {
            return Ok(None);
        };
        let updated = entry
            .iter_mut()
            .find(|participant| participant.agent_id.eq_ignore_ascii_case(&agent_id))
            .map(|participant| {
                participant.status = ThreadParticipantStatus::Inactive;
                participant.updated_at = now;
                participant.deactivated_at = Some(now);
                participant.clone()
            });
        drop(participants);

        if updated.is_some() {
            self.persist_thread_by_id(thread_id).await;
            let _ = self.event_tx.send(AgentEvent::ThreadReloadRequired {
                thread_id: thread_id.to_string(),
            });
        }

        Ok(updated)
    }

    pub async fn apply_thread_participant_command(
        &self,
        thread_id: &str,
        target_agent_id: &str,
        action: &str,
        instruction: Option<&str>,
    ) -> Result<()> {
        if is_participant_deactivation_action(action) {
            let _ = self
                .deactivate_thread_participant(thread_id, target_agent_id)
                .await?;
            return Ok(());
        }

        self.upsert_thread_participant(
            thread_id,
            target_agent_id,
            instruction.unwrap_or("").trim(),
        )
        .await?;
        if let Err(error) = self.run_participant_observers(thread_id).await {
            tracing::warn!(
                thread_id = %thread_id,
                participant = %target_agent_id,
                error = %error,
                "participant initial observer failed"
            );
            let _ = self.event_tx.send(AgentEvent::WorkflowNotice {
                thread_id: thread_id.to_string(),
                kind: "participant_observer_error".to_string(),
                message: "participant observers failed".to_string(),
                details: Some(error.to_string()),
            });
        }
        Ok(())
    }

    pub async fn send_internal_delegate_message(
        &self,
        thread_id: Option<&str>,
        target_agent_id: &str,
        preferred_session_hint: Option<&str>,
        content: &str,
    ) -> Result<()> {
        if let Some(thread_id) = thread_id {
            if is_internal_dm_thread(thread_id) || is_internal_handoff_thread(thread_id) {
                anyhow::bail!(
                    "internal delegate continuation requires a visible operator thread, not an internal thread"
                );
            }
        }
        let (resolved_target_id, _) = self
            .resolve_thread_participant_target(target_agent_id)
            .await?;
        let sender = match thread_id {
            Some(thread_id) => self
                .thread_handoff_state(thread_id)
                .await
                .map(|state| state.active_agent_id)
                .unwrap_or_else(|| MAIN_AGENT_ID.to_string()),
            None => MAIN_AGENT_ID.to_string(),
        };
        let payload = self
            .build_internal_delegate_payload(thread_id, content, thread_id.is_some())
            .await;

        self.send_internal_agent_message(
            &sender,
            &resolved_target_id,
            &payload,
            preferred_session_hint,
        )
        .await?;

        if let Some(thread_id) = thread_id {
            let continuation_prompt = self
                .build_visible_thread_continuation_prompt(
                    thread_id,
                    &sender,
                    &resolved_target_id,
                    content,
                )
                .await;
            self.continue_visible_thread_as_agent(
                thread_id,
                &resolved_target_id,
                preferred_session_hint,
                &continuation_prompt,
            )
            .await?;
        }

        Ok(())
    }

    pub async fn append_visible_thread_participant_message(
        &self,
        thread_id: &str,
        target_agent_id: &str,
        content: &str,
    ) -> Result<()> {
        if !self.threads.read().await.contains_key(thread_id) {
            anyhow::bail!("thread not found: {thread_id}");
        }

        let trimmed_content = content.trim();
        if trimmed_content.is_empty() {
            anyhow::bail!("participant message content cannot be empty");
        }

        let (agent_id, agent_name) = self
            .resolve_thread_participant_target(target_agent_id)
            .await?;

        let participant_exists =
            self.list_thread_participants(thread_id)
                .await
                .iter()
                .any(|participant| {
                    participant.agent_id.eq_ignore_ascii_case(&agent_id)
                        && participant.status == ThreadParticipantStatus::Active
                });
        if !participant_exists {
            anyhow::bail!("participant is not active on thread: {agent_id}");
        }

        let now = now_millis();
        {
            let mut threads = self.threads.write().await;
            let thread = threads
                .get_mut(thread_id)
                .ok_or_else(|| anyhow::anyhow!("thread not found: {thread_id}"))?;
            thread.messages.push(AgentMessage {
                id: generate_message_id(),
                role: MessageRole::Assistant,
                content: trimmed_content.to_string(),
                tool_calls: None,
                tool_call_id: None,
                tool_name: None,
                tool_arguments: None,
                tool_status: None,
                weles_review: None,
                input_tokens: 0,
                output_tokens: 0,
                provider: None,
                model: None,
                api_transport: None,
                response_id: None,
                upstream_message: None,
                provider_final_result: None,
                author_agent_id: Some(agent_id.clone()),
                author_agent_name: Some(agent_name.clone()),
                reasoning: None,
                message_kind: AgentMessageKind::Normal,
                compaction_strategy: None,
                compaction_payload: None,
                offloaded_payload_id: None,
                structural_refs: Vec::new(),
                timestamp: now,
            });
            thread.updated_at = now;
        }

        {
            let mut participants = self.thread_participants.write().await;
            if let Some(entry) = participants.get_mut(thread_id) {
                if let Some(participant) = entry
                    .iter_mut()
                    .find(|participant| participant.agent_id.eq_ignore_ascii_case(&agent_id))
                {
                    participant.last_contribution_at = Some(now);
                    participant.updated_at = now;
                    participant.status = ThreadParticipantStatus::Active;
                }
            }
        }

        self.persist_thread_by_id(thread_id).await;
        let _ = self.event_tx.send(AgentEvent::ThreadReloadRequired {
            thread_id: thread_id.to_string(),
        });

        Ok(())
    }

    pub async fn send_visible_thread_participant_message(
        &self,
        thread_id: &str,
        target_agent_id: &str,
        preferred_session_hint: Option<&str>,
        content: &str,
    ) -> Result<()> {
        if !self.threads.read().await.contains_key(thread_id) {
            anyhow::bail!("thread not found: {thread_id}");
        }

        let trimmed_content = content.trim();
        if trimmed_content.is_empty() {
            anyhow::bail!("participant message content cannot be empty");
        }

        let (agent_id, agent_name) = self
            .resolve_thread_participant_target(target_agent_id)
            .await?;

        let participant_exists =
            self.list_thread_participants(thread_id)
                .await
                .iter()
                .any(|participant| {
                    participant.agent_id.eq_ignore_ascii_case(&agent_id)
                        && participant.status == ThreadParticipantStatus::Active
                });
        if !participant_exists {
            anyhow::bail!("participant is not active on thread: {agent_id}");
        }

        let existing_message_ids: HashSet<String> = {
            let threads = self.threads.read().await;
            threads
                .get(thread_id)
                .map(|thread| {
                    thread
                        .messages
                        .iter()
                        .map(|message| message.id.clone())
                        .collect()
                })
                .unwrap_or_default()
        };
        let client_surface = self.get_thread_client_surface(thread_id).await;

        Box::pin(self.send_message_inner(
            Some(thread_id),
            trimmed_content,
            None,
            preferred_session_hint,
            None,
            None,
            None,
            client_surface,
            true,
        ))
        .await?;

        let now = now_millis();
        let mut tagged_assistant = false;
        {
            let mut threads = self.threads.write().await;
            let thread = threads.get_mut(thread_id).ok_or_else(|| {
                anyhow::anyhow!("thread not found after participant send: {thread_id}")
            })?;

            thread.messages.retain(|message| {
                !(message.role == MessageRole::User && !existing_message_ids.contains(&message.id))
            });

            for message in thread.messages.iter_mut().rev() {
                if existing_message_ids.contains(&message.id) {
                    continue;
                }
                if message.role == MessageRole::Assistant {
                    message.author_agent_id = Some(agent_id.clone());
                    message.author_agent_name = Some(agent_name.clone());
                    tagged_assistant = true;
                    break;
                }
            }

            if !tagged_assistant {
                anyhow::bail!("participant send did not produce an assistant message");
            }

            thread.updated_at = now;
        }

        {
            let mut participants = self.thread_participants.write().await;
            if let Some(entry) = participants.get_mut(thread_id) {
                if let Some(participant) = entry
                    .iter_mut()
                    .find(|participant| participant.agent_id.eq_ignore_ascii_case(&agent_id))
                {
                    participant.last_contribution_at = Some(now);
                    participant.updated_at = now;
                    participant.status = ThreadParticipantStatus::Active;
                }
            }
        }

        self.persist_thread_by_id(thread_id).await;
        let _ = self.event_tx.send(AgentEvent::ThreadReloadRequired {
            thread_id: thread_id.to_string(),
        });
        self.continue_thread_after_participant_post_or_notice(thread_id)
            .await;

        Ok(())
    }
}
