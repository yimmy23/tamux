//! Message sending API — public interface, thread creation, and session routing.

use super::*;

impl AgentEngine {
    pub async fn delete_thread_messages(&self, thread_id: &str, message_ids: &[String]) -> Result<usize> {
        if message_ids.is_empty() {
            return Ok(0);
        }

        let id_set: std::collections::HashSet<&str> =
            message_ids.iter().map(String::as_str).collect();

        let removed = {
            let mut threads = self.threads.write().await;
            if let Some(thread) = threads.get_mut(thread_id) {
                let before = thread.messages.len();
                thread.messages.retain(|msg| !id_set.contains(msg.id.as_str()));
                let removed = before.saturating_sub(thread.messages.len());
                if removed > 0 {
                    thread.updated_at = now_millis();
                    thread.total_input_tokens = thread.messages.iter().map(|m| m.input_tokens).sum();
                    thread.total_output_tokens =
                        thread.messages.iter().map(|m| m.output_tokens).sum();
                }
                removed
            } else {
                0
            }
        };

        // Also delete from SQLite (by synthetic ID or direct ID).
        let id_refs: Vec<&str> = message_ids.iter().map(String::as_str).collect();
        let db_removed = self.history.delete_messages(thread_id, &id_refs).await.unwrap_or(0);

        let total = removed.max(db_removed);
        if total > 0 {
            // Re-persist the thread to sync SQLite with in-memory state.
            self.persist_thread_by_id(thread_id).await;
            tracing::info!(thread_id, in_memory = removed, sqlite = db_removed, "deleted messages and persisted");
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
                    input_tokens: msg.input_tokens.unwrap_or(0) as u64,
                    output_tokens: msg.output_tokens.unwrap_or(0) as u64,
                    provider: msg.provider.clone(),
                    model: msg.model.clone(),
                    api_transport: metadata.api_transport,
                    response_id: metadata.response_id,
                    reasoning: msg.reasoning.clone(),
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

        threads.insert(
            tid.clone(),
            AgentThread {
                id: tid,
                title,
                messages,
                pinned: false,
                upstream_thread_id: None,
                upstream_transport: None,
                upstream_provider: None,
                upstream_model: None,
                upstream_assistant_id: None,
                created_at: now_millis(),
                updated_at: now_millis(),
                total_input_tokens: total_in,
                total_output_tokens: total_out,
            },
        );
    }

    /// Get or create a thread, returning the thread ID and whether it was newly created.
    pub(super) async fn get_or_create_thread(
        &self,
        thread_id: Option<&str>,
        content: &str,
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
                threads.insert(
                    id.clone(),
                    AgentThread {
                        id: id.clone(),
                        title: title.clone(),
                        messages: Vec::new(),
                        pinned: false,
                        upstream_thread_id: None,
                        upstream_transport: None,
                        upstream_provider: None,
                        upstream_model: None,
                        upstream_assistant_id: None,
                        created_at: now_millis(),
                        updated_at: now_millis(),
                        total_input_tokens: 0,
                        total_output_tokens: 0,
                    },
                );
                let _ = self.event_tx.send(AgentEvent::ThreadCreated {
                    thread_id: id.clone(),
                    title,
                });
            }
        }
        drop(threads);
        (id, created)
    }

    /// Attempt to restore a thread and its messages from the SQLite history database.
    async fn restore_thread_from_db(&self, thread_id: &str) -> Option<AgentThread> {
        let db_thread = self.history.get_thread(thread_id).await.ok().flatten()?;
        let db_messages = self.history.list_messages(thread_id, Some(500)).await.ok()?;
        let thread_metadata = parse_thread_metadata(db_thread.metadata_json.as_deref());

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
                    input_tokens: msg.input_tokens.unwrap_or(0) as u64,
                    output_tokens: msg.output_tokens.unwrap_or(0) as u64,
                    provider: msg.provider,
                    model: msg.model,
                    api_transport: metadata.api_transport,
                    response_id: metadata.response_id,
                    reasoning: msg.reasoning,
                    timestamp: msg.created_at as u64,
                })
            })
            .collect();

        let total_input: u64 = messages.iter().map(|m| m.input_tokens).sum();
        let total_output: u64 = messages.iter().map(|m| m.output_tokens).sum();

        Some(AgentThread {
            id: thread_id.to_string(),
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
        Ok(self
            .send_message_inner(thread_id, content, None, None, None)
            .await?
            .thread_id)
    }

    pub async fn send_message_with_session(
        &self,
        thread_id: Option<&str>,
        preferred_session_hint: Option<&str>,
        content: &str,
    ) -> Result<String> {
        Ok(self
            .send_message_inner(thread_id, content, None, preferred_session_hint, None)
            .await?
            .thread_id)
    }

    pub(super) async fn send_task_message(
        &self,
        task_id: &str,
        thread_id: Option<&str>,
        preferred_session_hint: Option<&str>,
        backend_override: Option<&str>,
        content: &str,
    ) -> Result<SendMessageOutcome> {
        self.send_message_inner(
            thread_id,
            content,
            Some(task_id),
            preferred_session_hint,
            backend_override,
        )
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session_manager::SessionManager;
    use tempfile::tempdir;

    #[tokio::test]
    async fn delete_thread_messages_updates_live_thread_and_persisted_history() {
        let root = tempdir().unwrap();
        let manager = SessionManager::new_test(root.path()).await;
        let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
        let thread_id = "thread_test";

        {
            let mut threads = engine.threads.write().await;
            threads.insert(
                thread_id.to_string(),
                AgentThread {
                    id: thread_id.to_string(),
                    title: "Test".to_string(),
                    created_at: 1,
                    updated_at: 1,
                    pinned: false,
                    upstream_thread_id: None,
                    upstream_transport: None,
                    upstream_provider: None,
                    upstream_model: None,
                    upstream_assistant_id: None,
                    total_input_tokens: 0,
                    total_output_tokens: 0,
                    messages: vec![
                        AgentMessage::user("first", 1),
                        AgentMessage::user("second", 2),
                        AgentMessage::user("third", 3),
                    ],
                },
            );
        }
        engine.persist_thread_by_id(thread_id).await;

        // Get the actual UUID of the second message to delete.
        let msg_id = {
            let threads = engine.threads.read().await;
            threads.get(thread_id).unwrap().messages[1].id.clone()
        };
        let deleted = engine
            .delete_thread_messages(thread_id, &[msg_id])
            .await
            .expect("delete should succeed");
        assert_eq!(deleted, 1);

        let live = engine.threads.read().await;
        let thread = live.get(thread_id).expect("thread should still exist");
        assert_eq!(thread.messages.len(), 2);
        assert_eq!(thread.messages[0].content, "first");
        assert_eq!(thread.messages[1].content, "third");
        drop(live);

        let persisted = engine.history.list_messages(thread_id, Some(10)).await.unwrap();
        assert_eq!(persisted.len(), 2);
        assert_eq!(persisted[0].content, "first");
        assert_eq!(persisted[1].content, "third");
    }
}
