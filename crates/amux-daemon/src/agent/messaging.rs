//! Message sending API — public interface, thread creation, and session routing.

use super::*;

impl AgentEngine {
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
            let mut threads = self.threads.write().await;
            if let Some(thread) = threads.get_mut(thread_id) {
                let before = thread.messages.len();
                thread
                    .messages
                    .retain(|msg| !id_set.contains(msg.id.as_str()));
                let removed = before.saturating_sub(thread.messages.len());
                if removed > 0 {
                    thread.updated_at = now_millis();
                    thread.total_input_tokens =
                        thread.messages.iter().map(|m| m.input_tokens).sum();
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
        let db_removed = self
            .history
            .delete_messages(thread_id, &id_refs)
            .await
            .unwrap_or(0);

        let total = removed.max(db_removed);
        if total > 0 {
            // Re-persist the thread to sync SQLite with in-memory state.
            self.persist_thread_by_id(thread_id).await;
            tracing::info!(
                thread_id,
                in_memory = removed,
                sqlite = db_removed,
                "deleted messages and persisted"
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
        let db_messages = self
            .history
            .list_messages(thread_id, Some(500))
            .await
            .ok()?;
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
            .send_message_inner(thread_id, content, None, None, None, true)
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
            .send_message_inner(thread_id, content, None, preferred_session_hint, None, true)
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
            true,
        )
        .await
    }

    pub async fn send_direct_message(
        &self,
        target: &str,
        thread_id: Option<&str>,
        preferred_session_hint: Option<&str>,
        content: &str,
    ) -> Result<(String, String)> {
        if is_concierge_target(target)
            || thread_id == Some(crate::agent::concierge::CONCIERGE_THREAD_ID)
        {
            let target_thread_id = thread_id
                .unwrap_or(crate::agent::concierge::CONCIERGE_THREAD_ID)
                .to_string();
            self.send_concierge_message_on_thread(
                &target_thread_id,
                content,
                preferred_session_hint,
                true,
                true,
            )
            .await?;
            let response = self
                .latest_assistant_message_text(&target_thread_id)
                .await
                .unwrap_or_default();
            return Ok((target_thread_id, response));
        }

        let target_thread_id = self
            .send_message_inner(thread_id, content, None, preferred_session_hint, None, true)
            .await?
            .thread_id;
        let response = self
            .latest_assistant_message_text(&target_thread_id)
            .await
            .unwrap_or_default();
        Ok((target_thread_id, response))
    }

    pub(super) async fn send_internal_agent_message(
        &self,
        sender: &str,
        recipient: &str,
        content: &str,
        preferred_session_hint: Option<&str>,
    ) -> Result<(String, String)> {
        let dm_thread_id = internal_dm_thread_id(sender, recipient);
        let wrapped = wrap_internal_message(sender, recipient, content);
        if is_concierge_target(recipient) {
            Box::pin(self.send_concierge_message_on_thread(
                &dm_thread_id,
                &wrapped,
                preferred_session_hint,
                false,
                false,
            ))
            .await?;
        } else {
            Box::pin(self.send_message_inner(
                Some(&dm_thread_id),
                &wrapped,
                None,
                preferred_session_hint,
                None,
                false,
            ))
            .await?;
        }
        self.ensure_thread_identity(
            &dm_thread_id,
            &internal_dm_thread_title(sender, recipient),
            false,
        )
        .await;
        let response = self
            .latest_assistant_message_text(&dm_thread_id)
            .await
            .unwrap_or_default();
        Ok((dm_thread_id, response))
    }

    pub(super) async fn latest_assistant_message_text(&self, thread_id: &str) -> Option<String> {
        let threads = self.threads.read().await;
        threads.get(thread_id).and_then(|thread| {
            thread
                .messages
                .iter()
                .rev()
                .find(|message| {
                    message.role == MessageRole::Assistant && !message.content.trim().is_empty()
                })
                .map(|message| message.content.clone())
        })
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

    pub(super) async fn send_concierge_message_on_thread(
        &self,
        thread_id: &str,
        content: &str,
        preferred_session_hint: Option<&str>,
        operator_origin: bool,
        allow_escalation: bool,
    ) -> Result<()> {
        let (tid, _) = self.get_or_create_thread(Some(thread_id), content).await;
        let pinned = tid == crate::agent::concierge::CONCIERGE_THREAD_ID;
        let title = if pinned {
            "Concierge".to_string()
        } else {
            internal_dm_thread_title(CONCIERGE_AGENT_ID, MAIN_AGENT_ID)
        };
        self.ensure_thread_identity(&tid, &title, pinned).await;

        {
            let mut threads = self.threads.write().await;
            if let Some(thread) = threads.get_mut(&tid) {
                thread
                    .messages
                    .push(AgentMessage::user(content.trim(), now_millis()));
                thread.updated_at = now_millis();
            }
        }
        self.persist_thread_by_id(&tid).await;

        if operator_origin {
            self.record_operator_message(&tid, content, false).await?;
        }

        let reply = if allow_escalation && concierge_should_escalate(content) {
            let (_internal_thread_id, response) = Box::pin(self.send_internal_agent_message(
                CONCIERGE_AGENT_ID,
                MAIN_AGENT_ID,
                content,
                preferred_session_hint,
            ))
            .await?;
            format!("I checked with {}. {}", MAIN_AGENT_NAME, response.trim())
        } else {
            self.generate_concierge_reply(&tid).await?
        };

        self.add_assistant_message(
            &tid,
            &reply,
            0,
            0,
            None,
            Some("concierge".to_string()),
            None,
            None,
            None,
        )
        .await;
        let _ = self.event_tx.send(AgentEvent::Delta {
            thread_id: tid.clone(),
            content: reply.clone(),
        });
        let _ = self.event_tx.send(AgentEvent::Done {
            thread_id: tid,
            input_tokens: 0,
            output_tokens: 0,
            cost: None,
            provider: Some("concierge".to_string()),
            model: None,
            tps: None,
            generation_ms: None,
            reasoning: None,
        });
        Ok(())
    }

    async fn generate_concierge_reply(&self, thread_id: &str) -> Result<String> {
        let config = self.config.read().await.clone();
        let provider_config = crate::agent::concierge::fast_concierge_provider_config(
            &crate::agent::concierge::resolve_concierge_provider(&config)?,
        );
        let provider_id = config
            .concierge
            .provider
            .as_deref()
            .unwrap_or(&config.provider)
            .to_string();
        drop(config);

        let memory_dir = active_memory_dir(&self.data_dir);
        let memory = self.memory.read().await;
        let mut system_prompt = format!(
            "{}\n\n{} is the main builder agent. You are {}, the concierge. If you need deeper implementation work, summarize what {} should handle.\nPersistent memory files:\n- MEMORY.md: {}\n- SOUL.md: {}\n- USER.md: {}",
            crate::agent::concierge::concierge_system_prompt(),
            MAIN_AGENT_NAME,
            CONCIERGE_AGENT_NAME,
            MAIN_AGENT_NAME,
            memory_dir.join("MEMORY.md").display(),
            memory_dir.join("SOUL.md").display(),
            memory_dir.join("USER.md").display(),
        );
        system_prompt.push_str("\n\n");
        system_prompt.push_str(&build_concierge_runtime_identity_prompt(
            &provider_id,
            &provider_config.model,
        ));
        if !memory.soul.trim().is_empty() {
            system_prompt.push_str("\n\n## Identity Notes\n");
            system_prompt.push_str(memory.soul.trim());
        }
        if !memory.memory.trim().is_empty() {
            system_prompt.push_str("\n\n## Persistent Memory\n");
            system_prompt.push_str(memory.memory.trim());
        }
        if !memory.user_profile.trim().is_empty() {
            system_prompt.push_str("\n\n## Operator Profile\n");
            system_prompt.push_str(memory.user_profile.trim());
        }
        drop(memory);
        if is_internal_dm_thread(thread_id) {
            system_prompt.push_str(
                "\n\nThis thread is an internal agent-to-agent DM. Reply to another agent, not directly to the operator.",
            );
        }

        let messages = {
            let threads = self.threads.read().await;
            let history = threads
                .get(thread_id)
                .map(|thread| {
                    let start = thread.messages.len().saturating_sub(16);
                    thread.messages[start..].to_vec()
                })
                .unwrap_or_default();
            super::llm_client::messages_to_api_format(&history)
        };

        let stream = send_completion_request(
            &self.http_client,
            &provider_id,
            &provider_config,
            &system_prompt,
            &messages,
            &[],
            provider_config.api_transport,
            None,
            None,
            RetryStrategy::Bounded {
                max_retries: 1,
                retry_delay_ms: 500,
            },
        );
        let mut full_content = String::new();
        let mut stream = std::pin::pin!(stream);
        while let Some(chunk) = stream.next().await {
            match chunk {
                Ok(CompletionChunk::Delta { content, .. }) => full_content.push_str(&content),
                Ok(CompletionChunk::Done { content, .. }) => {
                    if !content.is_empty() {
                        full_content = content;
                    }
                    break;
                }
                Ok(CompletionChunk::Error { message }) => anyhow::bail!(message),
                Err(error) => return Err(error),
                Ok(_) => {}
            }
        }
        Ok(full_content.trim().to_string())
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

        let persisted = engine
            .history
            .list_messages(thread_id, Some(10))
            .await
            .unwrap();
        assert_eq!(persisted.len(), 2);
        assert_eq!(persisted[0].content, "first");
        assert_eq!(persisted[1].content, "third");
    }
}
