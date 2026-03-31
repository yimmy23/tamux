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
        Ok(Box::pin(
            self.send_message_inner(thread_id, content, None, None, None, None, None, true),
        )
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
        Ok(Box::pin(self.send_message_inner(
            thread_id,
            content,
            None,
            preferred_session_hint,
            None,
            None,
            None,
            true,
        ))
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
        Box::pin(self.send_message_inner(
            thread_id,
            content,
            Some(task_id),
            preferred_session_hint,
            backend_override,
            None,
            None,
            true,
        ))
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

        let target_thread_id = Box::pin(self.send_message_inner(
            thread_id,
            content,
            None,
            preferred_session_hint,
            None,
            None,
            None,
            true,
        ))
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
                None,
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

        let memory_paths = memory_paths_for_scope(&self.data_dir, MAIN_AGENT_ID);
        let memory = self.memory_snapshot_for_scope(MAIN_AGENT_ID).await;
        let mut system_prompt = format!(
            "{}\n\n{} is the main builder agent. You are {}, the concierge. If you need deeper implementation work, summarize what {} should handle.\nPersistent memory files:\n- MEMORY.md: {}\n- SOUL.md: {}\n- USER.md: {}",
            crate::agent::concierge::concierge_system_prompt(),
            MAIN_AGENT_NAME,
            CONCIERGE_AGENT_NAME,
            MAIN_AGENT_NAME,
            memory_paths.memory_path.display(),
            memory_paths.soul_path.display(),
            memory_paths.user_path.display(),
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
    use std::collections::VecDeque;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::{Arc, Mutex as StdMutex};
    use tempfile::tempdir;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    fn repo_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("daemon crate dir")
            .parent()
            .expect("workspace root")
            .to_path_buf()
    }

    async fn spawn_recording_openai_server(
        recorded_bodies: Arc<StdMutex<VecDeque<String>>>,
    ) -> String {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind recording openai server");
        let addr = listener.local_addr().expect("recording server local addr");

        tokio::spawn(async move {
            loop {
                let Ok((mut socket, _)) = listener.accept().await else {
                    break;
                };
                let recorded_bodies = recorded_bodies.clone();
                tokio::spawn(async move {
                    let mut buffer = vec![0u8; 65536];
                    let read = socket
                        .read(&mut buffer)
                        .await
                        .expect("read request from test client");
                    let request = String::from_utf8_lossy(&buffer[..read]).to_string();
                    let body = request
                        .split("\r\n\r\n")
                        .nth(1)
                        .unwrap_or_default()
                        .to_string();
                    recorded_bodies
                        .lock()
                        .expect("lock request log")
                        .push_back(body);

                    let response = concat!(
                        "HTTP/1.1 200 OK\r\n",
                        "content-type: text/event-stream\r\n",
                        "cache-control: no-cache\r\n",
                        "connection: close\r\n",
                        "\r\n",
                        "data: {\"choices\":[{\"delta\":{\"content\":\"Gateway reply ok\"}}]}\n\n",
                        "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":7,\"completion_tokens\":3}}\n\n",
                        "data: [DONE]\n\n"
                    );
                    socket
                        .write_all(response.as_bytes())
                        .await
                        .expect("write response");
                });
            }
        });

        format!("http://{addr}/v1")
    }

    #[test]
    fn direct_message_entrypoints_box_large_send_message_futures() {
        let messaging_source =
            fs::read_to_string(repo_root().join("crates/amux-daemon/src/agent/messaging.rs"))
                .expect("read messaging.rs");
        let messaging_production = messaging_source
            .split("\n#[cfg(test)]")
            .next()
            .unwrap_or(messaging_source.as_str());
        let agent_loop_source =
            fs::read_to_string(repo_root().join("crates/amux-daemon/src/agent/agent_loop.rs"))
                .expect("read agent_loop.rs");
        let agent_loop_production = agent_loop_source
            .split("\n#[cfg(test)]")
            .next()
            .unwrap_or(agent_loop_source.as_str());

        for required in [
            "Box::pin(self.send_message_inner(",
            "let target_thread_id = Box::pin(self.send_message_inner(",
        ] {
            assert!(
                messaging_production.contains(required),
                "messaging entrypoint should box oversized future: {required}"
            );
        }

        assert!(
            agent_loop_production.contains("Box::pin(run_with_agent_scope(agent_scope_id, async {"),
            "send_message_inner should box the oversized agent loop future"
        );
    }

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

    #[tokio::test]
    async fn delete_thread_messages_rehydrates_and_clears_invalid_continuation() {
        let root = tempdir().unwrap();
        let manager = SessionManager::new_test(root.path()).await;
        let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
        let thread_id = "thread_continuation";

        let assistant_id = "assistant-anchor".to_string();
        {
            let mut threads = engine.threads.write().await;
            threads.insert(
                thread_id.to_string(),
                AgentThread {
                    id: thread_id.to_string(),
                    title: "Continuation".to_string(),
                    created_at: 1,
                    updated_at: 4,
                    pinned: false,
                    upstream_thread_id: Some("upstream-thread-1".to_string()),
                    upstream_transport: Some(ApiTransport::Responses),
                    upstream_provider: Some("github-copilot".to_string()),
                    upstream_model: Some("gpt-5.4".to_string()),
                    upstream_assistant_id: None,
                    total_input_tokens: 0,
                    total_output_tokens: 0,
                    messages: vec![
                        AgentMessage::user("first", 1),
                        AgentMessage {
                            id: assistant_id.clone(),
                            role: MessageRole::Assistant,
                            content: "answer".to_string(),
                            tool_calls: None,
                            tool_call_id: None,
                            tool_name: None,
                            tool_arguments: None,
                            tool_status: None,
                            input_tokens: 0,
                            output_tokens: 0,
                            provider: Some("github-copilot".to_string()),
                            model: Some("gpt-5.4".to_string()),
                            api_transport: Some(ApiTransport::Responses),
                            response_id: Some("resp_123".to_string()),
                            reasoning: None,
                            timestamp: 2,
                        },
                        AgentMessage::user("continue", 3),
                    ],
                },
            );
        }
        engine.persist_thread_by_id(thread_id).await;

        engine
            .delete_thread_messages(thread_id, std::slice::from_ref(&assistant_id))
            .await
            .expect("delete should succeed");

        let threads = engine.threads.read().await;
        let thread = threads.get(thread_id).expect("thread should exist");
        assert_eq!(thread.messages.len(), 2);
        assert!(thread
            .messages
            .iter()
            .all(|message| message.response_id.is_none()));
        assert!(thread.upstream_thread_id.is_none());
        assert!(thread.upstream_transport.is_none());
        assert!(thread.upstream_provider.is_none());
        assert!(thread.upstream_model.is_none());
        drop(threads);

        let persisted = engine
            .history
            .list_messages(thread_id, Some(10))
            .await
            .unwrap();
        assert_eq!(persisted.len(), 2);
        assert!(persisted.iter().all(|message| {
            !message
                .metadata_json
                .as_deref()
                .unwrap_or_default()
                .contains("resp_123")
        }));
    }

    #[tokio::test]
    async fn delete_thread_messages_removes_orphaned_tool_results_during_rebuild() {
        let root = tempdir().unwrap();
        let manager = SessionManager::new_test(root.path()).await;
        let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
        let thread_id = "thread_orphans";

        let assistant_id = "assistant-tool-turn".to_string();
        let tool_a_id = "tool-a".to_string();
        let tool_b_id = "tool-b".to_string();
        {
            let mut threads = engine.threads.write().await;
            threads.insert(
                thread_id.to_string(),
                AgentThread {
                    id: thread_id.to_string(),
                    title: "Orphans".to_string(),
                    created_at: 1,
                    updated_at: 6,
                    pinned: false,
                    upstream_thread_id: Some("upstream-thread-2".to_string()),
                    upstream_transport: Some(ApiTransport::Responses),
                    upstream_provider: Some("github-copilot".to_string()),
                    upstream_model: Some("gpt-5.4".to_string()),
                    upstream_assistant_id: None,
                    total_input_tokens: 0,
                    total_output_tokens: 0,
                    messages: vec![
                        AgentMessage::user("start", 1),
                        AgentMessage {
                            id: assistant_id.clone(),
                            role: MessageRole::Assistant,
                            content: "checking".to_string(),
                            tool_calls: Some(vec![
                                ToolCall {
                                    id: "call-a".to_string(),
                                    function: ToolFunction {
                                        name: "tool_a".to_string(),
                                        arguments: "{}".to_string(),
                                    },
                                },
                                ToolCall {
                                    id: "call-b".to_string(),
                                    function: ToolFunction {
                                        name: "tool_b".to_string(),
                                        arguments: "{}".to_string(),
                                    },
                                },
                            ]),
                            tool_call_id: None,
                            tool_name: None,
                            tool_arguments: None,
                            tool_status: None,
                            input_tokens: 0,
                            output_tokens: 0,
                            provider: Some("github-copilot".to_string()),
                            model: Some("gpt-5.4".to_string()),
                            api_transport: Some(ApiTransport::Responses),
                            response_id: Some("resp_456".to_string()),
                            reasoning: None,
                            timestamp: 2,
                        },
                        AgentMessage {
                            id: tool_a_id.clone(),
                            role: MessageRole::Tool,
                            content: "partial".to_string(),
                            tool_calls: None,
                            tool_call_id: Some("call-a".to_string()),
                            tool_name: Some("tool_a".to_string()),
                            tool_arguments: Some("{}".to_string()),
                            tool_status: Some("done".to_string()),
                            input_tokens: 0,
                            output_tokens: 0,
                            provider: None,
                            model: None,
                            api_transport: None,
                            response_id: None,
                            reasoning: None,
                            timestamp: 3,
                        },
                        AgentMessage {
                            id: tool_b_id.clone(),
                            role: MessageRole::Tool,
                            content: "done".to_string(),
                            tool_calls: None,
                            tool_call_id: Some("call-b".to_string()),
                            tool_name: Some("tool_b".to_string()),
                            tool_arguments: Some("{}".to_string()),
                            tool_status: Some("done".to_string()),
                            input_tokens: 0,
                            output_tokens: 0,
                            provider: None,
                            model: None,
                            api_transport: None,
                            response_id: None,
                            reasoning: None,
                            timestamp: 4,
                        },
                        AgentMessage {
                            id: "assistant-final".to_string(),
                            role: MessageRole::Assistant,
                            content: "final answer".to_string(),
                            tool_calls: None,
                            tool_call_id: None,
                            tool_name: None,
                            tool_arguments: None,
                            tool_status: None,
                            input_tokens: 0,
                            output_tokens: 0,
                            provider: Some("github-copilot".to_string()),
                            model: Some("gpt-5.4".to_string()),
                            api_transport: Some(ApiTransport::Responses),
                            response_id: None,
                            reasoning: None,
                            timestamp: 5,
                        },
                    ],
                },
            );
        }
        engine.persist_thread_by_id(thread_id).await;

        engine
            .delete_thread_messages(thread_id, std::slice::from_ref(&assistant_id))
            .await
            .expect("delete should succeed");

        let threads = engine.threads.read().await;
        let thread = threads.get(thread_id).expect("thread should exist");
        assert_eq!(thread.messages.len(), 2);
        assert_eq!(thread.messages[0].content, "start");
        assert_eq!(thread.messages[1].content, "final answer");
        assert!(thread
            .messages
            .iter()
            .all(|message| message.role != MessageRole::Tool));
        drop(threads);

        let persisted = engine
            .history
            .list_messages(thread_id, Some(10))
            .await
            .unwrap();
        assert_eq!(persisted.len(), 2);
        assert!(persisted.iter().all(|message| message.role != "tool"));
    }

    #[tokio::test]
    async fn delete_thread_messages_drops_incomplete_assistant_tool_turn_after_tool_delete() {
        let root = tempdir().unwrap();
        let manager = SessionManager::new_test(root.path()).await;
        let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
        let thread_id = "thread_incomplete_tool_turn";

        let tool_result_a_id = "tool-result-a".to_string();
        let tool_result_b_id = "tool-result-b".to_string();
        {
            let mut threads = engine.threads.write().await;
            threads.insert(
                thread_id.to_string(),
                AgentThread {
                    id: thread_id.to_string(),
                    title: "Incomplete tool turn".to_string(),
                    created_at: 1,
                    updated_at: 6,
                    pinned: false,
                    upstream_thread_id: Some("upstream-thread-3".to_string()),
                    upstream_transport: Some(ApiTransport::Responses),
                    upstream_provider: Some("github-copilot".to_string()),
                    upstream_model: Some("gpt-5.4".to_string()),
                    upstream_assistant_id: None,
                    total_input_tokens: 0,
                    total_output_tokens: 0,
                    messages: vec![
                        AgentMessage::user("start", 1),
                        AgentMessage {
                            id: "assistant-tool-turn".to_string(),
                            role: MessageRole::Assistant,
                            content: "checking".to_string(),
                            tool_calls: Some(vec![
                                ToolCall {
                                    id: "call-a".to_string(),
                                    function: ToolFunction {
                                        name: "tool_a".to_string(),
                                        arguments: "{}".to_string(),
                                    },
                                },
                                ToolCall {
                                    id: "call-b".to_string(),
                                    function: ToolFunction {
                                        name: "tool_b".to_string(),
                                        arguments: "{}".to_string(),
                                    },
                                },
                            ]),
                            tool_call_id: None,
                            tool_name: None,
                            tool_arguments: None,
                            tool_status: None,
                            input_tokens: 0,
                            output_tokens: 0,
                            provider: Some("github-copilot".to_string()),
                            model: Some("gpt-5.4".to_string()),
                            api_transport: Some(ApiTransport::Responses),
                            response_id: Some("resp_789".to_string()),
                            reasoning: None,
                            timestamp: 2,
                        },
                        AgentMessage {
                            id: tool_result_a_id.clone(),
                            role: MessageRole::Tool,
                            content: "result a".to_string(),
                            tool_calls: None,
                            tool_call_id: Some("call-a".to_string()),
                            tool_name: Some("tool_a".to_string()),
                            tool_arguments: Some("{}".to_string()),
                            tool_status: Some("done".to_string()),
                            input_tokens: 0,
                            output_tokens: 0,
                            provider: None,
                            model: None,
                            api_transport: None,
                            response_id: None,
                            reasoning: None,
                            timestamp: 3,
                        },
                        AgentMessage {
                            id: tool_result_b_id.clone(),
                            role: MessageRole::Tool,
                            content: "result b".to_string(),
                            tool_calls: None,
                            tool_call_id: Some("call-b".to_string()),
                            tool_name: Some("tool_b".to_string()),
                            tool_arguments: Some("{}".to_string()),
                            tool_status: Some("done".to_string()),
                            input_tokens: 0,
                            output_tokens: 0,
                            provider: None,
                            model: None,
                            api_transport: None,
                            response_id: None,
                            reasoning: None,
                            timestamp: 4,
                        },
                        AgentMessage::user("continue", 5),
                    ],
                },
            );
        }
        engine.persist_thread_by_id(thread_id).await;

        engine
            .delete_thread_messages(thread_id, std::slice::from_ref(&tool_result_b_id))
            .await
            .expect("delete should succeed");

        let threads = engine.threads.read().await;
        let thread = threads.get(thread_id).expect("thread should exist");
        assert_eq!(thread.messages.len(), 2);
        assert_eq!(thread.messages[0].content, "start");
        assert_eq!(thread.messages[1].content, "continue");
        assert!(thread
            .messages
            .iter()
            .all(|message| message.tool_calls.is_none()));
        assert!(thread
            .messages
            .iter()
            .all(|message| message.role != MessageRole::Tool));
    }

    #[tokio::test]
    async fn delete_thread_messages_emits_thread_reload_event_after_reconciliation() {
        let root = tempdir().unwrap();
        let manager = SessionManager::new_test(root.path()).await;
        let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
        let thread_id = "thread_reload_event";
        let assistant_id = "assistant-anchor".to_string();
        let mut events = engine.subscribe();

        {
            let mut threads = engine.threads.write().await;
            threads.insert(
                thread_id.to_string(),
                AgentThread {
                    id: thread_id.to_string(),
                    title: "Reload event".to_string(),
                    created_at: 1,
                    updated_at: 3,
                    pinned: false,
                    upstream_thread_id: Some("upstream-thread-4".to_string()),
                    upstream_transport: Some(ApiTransport::Responses),
                    upstream_provider: Some("github-copilot".to_string()),
                    upstream_model: Some("gpt-5.4".to_string()),
                    upstream_assistant_id: None,
                    total_input_tokens: 0,
                    total_output_tokens: 0,
                    messages: vec![
                        AgentMessage::user("first", 1),
                        AgentMessage {
                            id: assistant_id.clone(),
                            role: MessageRole::Assistant,
                            content: "answer".to_string(),
                            tool_calls: None,
                            tool_call_id: None,
                            tool_name: None,
                            tool_arguments: None,
                            tool_status: None,
                            input_tokens: 0,
                            output_tokens: 0,
                            provider: Some("github-copilot".to_string()),
                            model: Some("gpt-5.4".to_string()),
                            api_transport: Some(ApiTransport::Responses),
                            response_id: Some("resp_999".to_string()),
                            reasoning: None,
                            timestamp: 2,
                        },
                        AgentMessage::user("continue", 3),
                    ],
                },
            );
        }
        engine.persist_thread_by_id(thread_id).await;

        while events.try_recv().is_ok() {}

        engine
            .delete_thread_messages(thread_id, std::slice::from_ref(&assistant_id))
            .await
            .expect("delete should succeed");

        let mut saw_reload = false;
        while let Ok(event) = events.try_recv() {
            if let AgentEvent::ThreadReloadRequired {
                thread_id: event_thread_id,
            } = event
            {
                assert_eq!(event_thread_id, thread_id);
                saw_reload = true;
                break;
            }
        }

        assert!(saw_reload, "delete should emit thread reload event");
    }

    #[tokio::test]
    async fn send_message_with_ephemeral_user_override_keeps_thread_history_clean() {
        let recorded_bodies = Arc::new(StdMutex::new(VecDeque::new()));
        let server_url = spawn_recording_openai_server(recorded_bodies.clone()).await;
        let root = tempdir().expect("tempdir");
        let manager = SessionManager::new_test(root.path()).await;
        let mut config = AgentConfig::default();
        config.provider = "openai".to_string();
        config.base_url = server_url;
        config.model = "gpt-4o-mini".to_string();
        config.api_transport = ApiTransport::ChatCompletions;
        config.max_retries = 0;
        config.auto_retry = false;
        let engine = AgentEngine::new_test(manager, config, root.path()).await;

        let thread_id = engine
            .send_message_with_ephemeral_user_override(
                None,
                "What model are you bro?",
                "[discord message from mariuszkurman]: What model are you bro?\nYour final assistant response will be delivered back to the user automatically.",
                std::time::Duration::from_secs(120),
            )
            .await
            .expect("send message with ephemeral override");

        let messages = engine
            .history
            .list_messages(&thread_id, Some(10))
            .await
            .expect("load persisted messages");
        let stored_user = messages
            .iter()
            .find(|message| message.role == "user")
            .expect("stored user message");
        assert_eq!(stored_user.content, "What model are you bro?");

        let request_body = recorded_bodies
            .lock()
            .expect("lock request log")
            .pop_front()
            .expect("captured llm request");
        assert!(
            request_body.contains(
                "Your final assistant response will be delivered back to the user automatically."
            ),
            "LLM request should include the ephemeral gateway wrapper"
        );
        assert!(
            !request_body.contains("\"content\":\"What model are you bro?\""),
            "LLM request should replace the raw stored user text with the ephemeral override"
        );
    }
}
