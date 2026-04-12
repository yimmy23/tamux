//! Message sending API — public interface, thread creation, and session routing.

use super::*;

mod concierge;
mod direct_messages;

impl AgentEngine {
    async fn prepare_internal_dm_thread(
        &self,
        sender: &str,
        recipient: &str,
        wrapped_content: &str,
    ) -> String {
        let dm_thread_id = internal_dm_thread_id(sender, recipient);
        let _ = self
            .get_or_create_thread_with_target(Some(&dm_thread_id), wrapped_content, Some(recipient))
            .await;
        self.set_thread_handoff_state(
            &dm_thread_id,
            initial_thread_handoff_state(
                &dm_thread_id,
                Some(canonical_agent_name(recipient)),
                now_millis(),
            ),
        )
        .await;
        dm_thread_id
    }

    pub async fn delete_thread_messages(
        &self,
        thread_id: &str,
        message_ids: &[String],
    ) -> Result<usize> {
        if message_ids.is_empty() {
            return Ok(0);
        }

        let id_set: std::collections::HashSet<&str> =
            message_ids.iter().map(String::as_str).collect();

        let removed = {
            let threads = self.threads.read().await;
            threads
                .get(thread_id)
                .map(|thread| {
                    thread
                        .messages
                        .iter()
                        .filter(|message| id_set.contains(message.id.as_str()))
                        .count()
                })
                .unwrap_or(0)
        };

        // Also delete from SQLite (by synthetic ID or direct ID).
        let id_refs: Vec<&str> = message_ids.iter().map(String::as_str).collect();
        let db_removed = self
            .history
            .delete_messages(thread_id, &id_refs)
            .await
            .unwrap_or(0);

        let total = removed.max(db_removed);
        if total > 0 {
            let existing_pinned = {
                let threads = self.threads.read().await;
                threads.get(thread_id).map(|thread| thread.pinned)
            };

            if let Some(mut restored) = self.restore_thread_from_db(thread_id).await {
                if let Some(pinned) = existing_pinned {
                    restored.pinned = pinned;
                }

                let mut threads = self.threads.write().await;
                threads.insert(thread_id.to_string(), restored);
            } else {
                let mut threads = self.threads.write().await;
                if let Some(thread) = threads.get_mut(thread_id) {
                    thread
                        .messages
                        .retain(|message| !id_set.contains(message.id.as_str()));
                    thread.updated_at = now_millis();
                    thread.total_input_tokens = thread
                        .messages
                        .iter()
                        .map(|message| message.input_tokens)
                        .sum();
                    thread.total_output_tokens = thread
                        .messages
                        .iter()
                        .map(|message| message.output_tokens)
                        .sum();
                }
            }

            self.repair_tool_call_sequence(thread_id).await;
            self.clear_thread_continuation_state(thread_id).await;
            let _ = self.event_tx.send(AgentEvent::ThreadReloadRequired {
                thread_id: thread_id.to_string(),
            });
            tracing::info!(
                thread_id,
                in_memory = removed,
                sqlite = db_removed,
                "deleted messages and reconciled thread state"
            );
        }
        Ok(total)
    }

    pub async fn seed_thread_context(
        &self,
        thread_id: Option<&str>,
        context: &[amux_protocol::AgentDbMessage],
    ) {
        let tid = match thread_id {
            Some(id) => id.to_string(),
            None => return, // Can't seed without a thread ID
        };

        let mut threads = self.threads.write().await;
        // Only seed if the thread doesn't exist yet or has no messages
        let needs_seeding = match threads.get(&tid) {
            None => true,
            Some(t) => t.messages.is_empty(),
        };
        if !needs_seeding || context.is_empty() {
            return;
        }

        let messages: Vec<AgentMessage> = context
            .iter()
            .filter_map(|msg| {
                let role = match msg.role.as_str() {
                    "user" => MessageRole::User,
                    "assistant" => MessageRole::Assistant,
                    "tool" => MessageRole::Tool,
                    "system" => MessageRole::System,
                    _ => return None,
                };
                let tool_calls: Option<Vec<ToolCall>> = msg
                    .tool_calls_json
                    .as_deref()
                    .and_then(|json| serde_json::from_str(json).ok());
                let metadata = parse_message_metadata(msg.metadata_json.as_deref());
                Some(AgentMessage {
                    id: msg.id.clone(),
                    role,
                    content: msg.content.clone(),
                    tool_calls,
                    tool_call_id: metadata.tool_call_id,
                    tool_name: metadata.tool_name,
                    tool_arguments: metadata.tool_arguments,
                    tool_status: metadata.tool_status,
                    weles_review: metadata.weles_review.clone(),
                    input_tokens: msg.input_tokens.unwrap_or(0) as u64,
                    output_tokens: msg.output_tokens.unwrap_or(0) as u64,
                    provider: msg.provider.clone(),
                    model: msg.model.clone(),
                    api_transport: metadata.api_transport,
                    response_id: metadata.response_id,
                    upstream_message: metadata.upstream_message,
                    provider_final_result: metadata.provider_final_result,
                    author_agent_id: metadata.author_agent_id,
                    author_agent_name: metadata.author_agent_name,
                    reasoning: msg.reasoning.clone(),
                    message_kind: metadata.message_kind,
                    compaction_strategy: metadata.compaction_strategy,
                    compaction_payload: metadata.compaction_payload,
                    offloaded_payload_id: metadata.offloaded_payload_id,
                    structural_refs: metadata.structural_refs,
                    timestamp: msg.created_at as u64,
                })
            })
            .collect();

        if messages.is_empty() {
            return;
        }

        let total_in: u64 = messages.iter().map(|m| m.input_tokens).sum();
        let total_out: u64 = messages.iter().map(|m| m.output_tokens).sum();
        let title = messages
            .iter()
            .find(|m| m.role == MessageRole::User)
            .map(|m| m.content.chars().take(50).collect::<String>())
            .unwrap_or_else(|| "Continued conversation".into());

        tracing::info!(thread_id = %tid, context_messages = messages.len(), "seeding thread with frontend context");

        let active_agent_id = current_agent_scope_id();
        let created_at = now_millis();
        threads.insert(
            tid.clone(),
            AgentThread {
                id: tid.clone(),
                agent_name: Some(canonical_agent_name(&active_agent_id).to_string()),
                title,
                messages,
                pinned: false,
                upstream_thread_id: None,
                upstream_transport: None,
                upstream_provider: None,
                upstream_model: None,
                upstream_assistant_id: None,
                created_at,
                updated_at: created_at,
                total_input_tokens: total_in,
                total_output_tokens: total_out,
            },
        );
        drop(threads);
        self.thread_handoff_states.write().await.insert(
            tid.clone(),
            initial_thread_handoff_state(
                &tid,
                Some(canonical_agent_name(&active_agent_id)),
                created_at,
            ),
        );
    }

    /// Get or create a thread, returning the thread ID and whether it was newly created.
    pub(super) async fn get_or_create_thread(
        &self,
        thread_id: Option<&str>,
        content: &str,
    ) -> (String, bool) {
        self.get_or_create_thread_with_target(thread_id, content, None)
            .await
    }

    pub(super) async fn get_or_create_thread_with_target(
        &self,
        thread_id: Option<&str>,
        content: &str,
        target_agent_id: Option<&str>,
    ) -> (String, bool) {
        let given_id = thread_id.map(|s| s.to_string());
        let id = given_id.unwrap_or_else(|| format!("thread_{}", Uuid::new_v4()));
        let title = content.chars().take(50).collect::<String>();
        let mut created = false;

        let mut threads = self.threads.write().await;
        if !threads.contains_key(&id) {
            // Try to restore the thread from the database (history continuation)
            if let Some(restored) = self.restore_thread_from_db(&id).await {
                tracing::info!(thread_id = %id, messages = restored.messages.len(), "restored thread from history");
                threads.insert(id.clone(), restored);
            } else {
                created = true;
                let created_at = now_millis();
                let active_agent_id = match target_agent_id {
                    Some(target_agent_id) => canonical_agent_id(target_agent_id).to_string(),
                    None => current_agent_scope_id(),
                };
                threads.insert(
                    id.clone(),
                    AgentThread {
                        id: id.clone(),
                        agent_name: Some(canonical_agent_name(&active_agent_id).to_string()),
                        title: title.clone(),
                        messages: Vec::new(),
                        pinned: false,
                        upstream_thread_id: None,
                        upstream_transport: None,
                        upstream_provider: None,
                        upstream_model: None,
                        upstream_assistant_id: None,
                        created_at,
                        updated_at: created_at,
                        total_input_tokens: 0,
                        total_output_tokens: 0,
                    },
                );
                self.thread_handoff_states.write().await.insert(
                    id.clone(),
                    initial_thread_handoff_state(
                        &id,
                        Some(canonical_agent_name(&active_agent_id)),
                        created_at,
                    ),
                );
                let _ = self.event_tx.send(AgentEvent::ThreadCreated {
                    thread_id: id.clone(),
                    title,
                    agent_name: Some(canonical_agent_name(&active_agent_id).to_string()),
                });
            }
        }
        drop(threads);
        (id, created)
    }

    /// Attempt to restore a thread and its messages from the SQLite history database.
    async fn restore_thread_from_db(&self, thread_id: &str) -> Option<AgentThread> {
        let db_thread = self.history.get_thread(thread_id).await.ok().flatten()?;
        let db_messages = self
            .history
            .list_messages(thread_id, Some(500))
            .await
            .ok()?;
        let thread_metadata = parse_thread_metadata(db_thread.metadata_json.as_deref());
        if let Some(client_surface) = thread_metadata.client_surface {
            self.thread_client_surfaces
                .write()
                .await
                .insert(thread_id.to_string(), client_surface);
        }
        let handoff_state = normalized_thread_handoff_state(
            thread_id,
            db_thread.agent_name.as_deref(),
            db_thread.created_at as u64,
            thread_metadata.handoff_state,
        );
        self.thread_handoff_states
            .write()
            .await
            .insert(thread_id.to_string(), handoff_state.clone());

        let messages: Vec<AgentMessage> = db_messages
            .into_iter()
            .filter_map(|msg| {
                let role = match msg.role.as_str() {
                    "user" => MessageRole::User,
                    "assistant" => MessageRole::Assistant,
                    "tool" => MessageRole::Tool,
                    "system" => MessageRole::System,
                    _ => return None,
                };

                let tool_calls: Option<Vec<ToolCall>> = msg
                    .tool_calls_json
                    .as_deref()
                    .and_then(|json| serde_json::from_str(json).ok());

                let metadata = parse_message_metadata(msg.metadata_json.as_deref());

                Some(AgentMessage {
                    id: msg.id.clone(),
                    role,
                    content: msg.content,
                    tool_calls,
                    tool_call_id: metadata.tool_call_id,
                    tool_name: metadata.tool_name,
                    tool_arguments: metadata.tool_arguments,
                    tool_status: metadata.tool_status,
                    weles_review: metadata.weles_review,
                    input_tokens: msg.input_tokens.unwrap_or(0) as u64,
                    output_tokens: msg.output_tokens.unwrap_or(0) as u64,
                    provider: msg.provider,
                    model: msg.model,
                    api_transport: metadata.api_transport,
                    response_id: metadata.response_id,
                    upstream_message: metadata.upstream_message,
                    provider_final_result: metadata.provider_final_result,
                    author_agent_id: metadata.author_agent_id,
                    author_agent_name: metadata.author_agent_name,
                    reasoning: msg.reasoning,
                    message_kind: metadata.message_kind,
                    compaction_strategy: metadata.compaction_strategy,
                    compaction_payload: metadata.compaction_payload,
                    offloaded_payload_id: metadata.offloaded_payload_id,
                    structural_refs: metadata.structural_refs,
                    timestamp: msg.created_at as u64,
                })
            })
            .collect();

        let total_input: u64 = messages.iter().map(|m| m.input_tokens).sum();
        let total_output: u64 = messages.iter().map(|m| m.output_tokens).sum();

        Some(AgentThread {
            id: thread_id.to_string(),
            agent_name: Some(canonical_agent_name(&handoff_state.active_agent_id).to_string()),
            title: db_thread.title,
            messages,
            pinned: false,
            upstream_thread_id: thread_metadata.upstream_thread_id,
            upstream_transport: thread_metadata.upstream_transport,
            upstream_provider: thread_metadata.upstream_provider,
            upstream_model: thread_metadata.upstream_model,
            upstream_assistant_id: thread_metadata.upstream_assistant_id,
            created_at: db_thread.created_at as u64,
            updated_at: db_thread.updated_at as u64,
            total_input_tokens: total_input,
            total_output_tokens: total_output,
        })
    }

    // -----------------------------------------------------------------------
    // Agent turn (send message → LLM → tool loop → done)
    // -----------------------------------------------------------------------

    /// Run a complete agent turn in a thread.
    pub async fn send_message(&self, thread_id: Option<&str>, content: &str) -> Result<String> {
        Ok(Box::pin(
            self.send_message_inner(thread_id, content, None, None, None, None, None, None, true),
        )
        .await?
        .thread_id)
    }

    pub(super) async fn send_internal_message(
        &self,
        thread_id: Option<&str>,
        content: &str,
    ) -> Result<String> {
        Ok(Box::pin(self.send_message_inner(
            thread_id, content, None, None, None, None, None, None, false,
        ))
        .await?
        .thread_id)
    }

    pub async fn send_message_with_ephemeral_user_override(
        &self,
        thread_id: Option<&str>,
        stored_content: &str,
        llm_user_override: &str,
        stream_chunk_timeout: std::time::Duration,
    ) -> Result<String> {
        Ok(Box::pin(self.send_message_inner(
            thread_id,
            stored_content,
            None,
            None,
            None,
            Some(llm_user_override),
            Some(stream_chunk_timeout),
            None,
            true,
        ))
        .await?
        .thread_id)
    }

    pub async fn send_message_with_session(
        &self,
        thread_id: Option<&str>,
        preferred_session_hint: Option<&str>,
        content: &str,
    ) -> Result<String> {
        self.send_message_with_session_and_surface(thread_id, preferred_session_hint, content, None)
            .await
    }

    pub async fn send_message_with_session_and_surface(
        &self,
        thread_id: Option<&str>,
        preferred_session_hint: Option<&str>,
        content: &str,
        client_surface: Option<amux_protocol::ClientSurface>,
    ) -> Result<String> {
        let outcome = Box::pin(self.send_message_inner(
            thread_id,
            content,
            None,
            preferred_session_hint,
            None,
            None,
            None,
            client_surface,
            true,
        ))
        .await?;
        Ok(outcome.thread_id)
    }

    pub async fn send_message_with_session_surface_and_target(
        &self,
        thread_id: Option<&str>,
        preferred_session_hint: Option<&str>,
        content: &str,
        client_surface: Option<amux_protocol::ClientSurface>,
        target_agent_id: Option<&str>,
    ) -> Result<String> {
        let effective_thread_id = if target_agent_id.is_some() {
            let (thread_id, _) = self
                .get_or_create_thread_with_target(thread_id, content, target_agent_id)
                .await;
            Some(thread_id)
        } else {
            thread_id.map(str::to_string)
        };

        self.send_message_with_session_and_surface(
            effective_thread_id.as_deref(),
            preferred_session_hint,
            content,
            client_surface,
        )
        .await
    }

    pub(super) async fn send_task_message(
        &self,
        task_id: &str,
        thread_id: Option<&str>,
        preferred_session_hint: Option<&str>,
        backend_override: Option<&str>,
        content: &str,
    ) -> Result<SendMessageOutcome> {
        let client_surface = if let Some(thread_id) = thread_id {
            self.get_thread_client_surface(thread_id).await
        } else {
            let goal_run_id = {
                let tasks = self.tasks.lock().await;
                tasks
                    .iter()
                    .find(|task| task.id == task_id)
                    .and_then(|task| task.goal_run_id.clone())
            };
            match goal_run_id {
                Some(goal_run_id) => self.get_goal_run_client_surface(&goal_run_id).await,
                None => None,
            }
        };
        Box::pin(self.send_message_inner(
            thread_id,
            content,
            Some(task_id),
            preferred_session_hint,
            backend_override,
            None,
            None,
            client_surface,
            false,
        ))
        .await
    }

    pub(super) async fn send_internal_task_message(
        &self,
        sender: &str,
        recipient: &str,
        task_id: &str,
        preferred_session_hint: Option<&str>,
        backend_override: Option<&str>,
        content: &str,
    ) -> Result<SendMessageOutcome> {
        let wrapped = wrap_internal_message(sender, recipient, content);
        let dm_thread_id = self
            .prepare_internal_dm_thread(sender, recipient, &wrapped)
            .await;
        let outcome = Box::pin(self.send_message_inner(
            Some(&dm_thread_id),
            &wrapped,
            Some(task_id),
            preferred_session_hint,
            backend_override,
            None,
            None,
            None,
            false,
        ))
        .await?;
        self.ensure_thread_identity(
            &dm_thread_id,
            &internal_dm_thread_title(sender, recipient),
            false,
        )
        .await;
        Ok(outcome)
    }

    pub(super) async fn ensure_thread_identity(&self, thread_id: &str, title: &str, pinned: bool) {
        let mut threads = self.threads.write().await;
        if let Some(thread) = threads.get_mut(thread_id) {
            thread.title = title.to_string();
            thread.pinned = pinned;
            thread.updated_at = now_millis();
        }
        drop(threads);
        self.persist_thread_by_id(thread_id).await;
    }
}

#[cfg(test)]
#[path = "tests/messaging/mod.rs"]
mod tests;
