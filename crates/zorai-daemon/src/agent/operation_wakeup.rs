use super::*;

#[derive(Debug, Clone)]
pub(super) struct OperationWakeup {
    pub(super) operation_id: String,
    pub(super) thread_id: String,
    pub(super) tool_name: String,
    pub(super) registered_at: u64,
}

impl AgentEngine {
    pub(in crate::agent) async fn register_operation_wakeup(
        &self,
        thread_id: &str,
        tool_name: &str,
        operation_id: &str,
    ) {
        let thread_id = thread_id.trim();
        let operation_id = operation_id.trim();
        if thread_id.is_empty() || operation_id.is_empty() {
            return;
        }

        let mut wakeups = self.operation_wakeups.lock().await;
        wakeups
            .entry(operation_id.to_string())
            .or_insert_with(|| OperationWakeup {
                operation_id: operation_id.to_string(),
                thread_id: thread_id.to_string(),
                tool_name: tool_name.to_string(),
                registered_at: now_millis(),
            });
    }

    pub(in crate::agent) async fn register_operation_wakeups_from_tool_result(
        &self,
        thread_id: &str,
        tool_name: &str,
        content: &str,
    ) {
        if matches!(
            tool_name,
            zorai_protocol::tool_names::GET_OPERATION_STATUS
                | zorai_protocol::tool_names::GET_BACKGROUND_TASK_STATUS
        ) {
            return;
        }

        for operation_id in extract_operation_ids(content) {
            self.register_operation_wakeup(thread_id, tool_name, &operation_id)
                .await;
        }
    }

    #[cfg(test)]
    pub(in crate::agent) async fn pending_operation_wakeup_count(&self) -> usize {
        self.operation_wakeups.lock().await.len()
    }

    pub(in crate::agent) async fn supervise_operation_completion_wakeups(&self) -> Result<()> {
        let wakeups = {
            let wakeups = self.operation_wakeups.lock().await;
            wakeups.values().cloned().collect::<Vec<_>>()
        };

        for wakeup in wakeups {
            let Some(status) = self.operation_wakeup_status(&wakeup).await? else {
                continue;
            };
            if !operation_status_is_terminal(&status) {
                continue;
            }

            let claimed = {
                let mut wakeups = self.operation_wakeups.lock().await;
                wakeups.remove(&wakeup.operation_id).is_some()
            };
            if !claimed {
                continue;
            }

            self.perform_operation_completion_wakeup(&wakeup, status)
                .await;
        }

        Ok(())
    }

    async fn operation_wakeup_status(
        &self,
        wakeup: &OperationWakeup,
    ) -> Result<Option<serde_json::Value>> {
        if let Some(status) = self
            .session_manager
            .get_background_task_status(&wakeup.operation_id)
            .await?
        {
            let mut payload = serde_json::json!({
                "operation_id": status.background_task_id,
                "kind": status.kind,
                "state": status.state,
                "background_task_id": wakeup.operation_id,
            });
            if let Some(session_id) = status.session_id {
                payload["session_id"] = serde_json::Value::String(session_id);
            }
            if let Some(position) = status.position {
                payload["position"] = serde_json::Value::Number(position.into());
            }
            if let Some(command) = status.command {
                payload["command"] = serde_json::Value::String(command);
            }
            if let Some(exit_code) = status.exit_code {
                payload["exit_code"] = serde_json::Value::Number(exit_code.into());
            }
            if let Some(duration_ms) = status.duration_ms {
                payload["duration_ms"] = serde_json::Value::Number(duration_ms.into());
            }
            if let Some(snapshot_path) = status.snapshot_path {
                payload["snapshot_path"] = serde_json::Value::String(snapshot_path);
            }
            return Ok(Some(payload));
        }

        let Some(snapshot) = crate::server::operation_registry().snapshot(&wakeup.operation_id)
        else {
            tracing::warn!(
                operation_id = %wakeup.operation_id,
                thread_id = %wakeup.thread_id,
                "dropping operation wakeup for unknown operation"
            );
            self.operation_wakeups
                .lock()
                .await
                .remove(&wakeup.operation_id);
            return Ok(None);
        };

        let mut payload = serde_json::json!({
            "operation_id": snapshot.operation_id,
            "kind": snapshot.kind,
            "state": snapshot.state,
            "revision": snapshot.revision,
        });
        if let Some(dedup) = snapshot.dedup {
            payload["dedup"] = serde_json::Value::String(dedup);
        }
        if let Some(terminal_result) =
            crate::server::operation_registry().terminal_result(&wakeup.operation_id)
        {
            payload["terminal_result"] = terminal_result;
        }
        Ok(Some(payload))
    }

    async fn perform_operation_completion_wakeup(
        &self,
        wakeup: &OperationWakeup,
        status: serde_json::Value,
    ) {
        let state = status
            .get("state")
            .and_then(|value| value.as_str())
            .unwrap_or("unknown");
        let message = format!(
            "Background operation finished.\n\noperation_id: {}\ntool: {}\nstate: {}\nregistered_at: {}\n\nOperation status:\n{}",
            wakeup.operation_id, wakeup.tool_name, state, wakeup.registered_at, status
        );

        if !self
            .append_system_thread_message(&wakeup.thread_id, message.clone())
            .await
        {
            return;
        }

        self.emit_workflow_notice(
            &wakeup.thread_id,
            "operation-completion-wakeup",
            format!(
                "Background operation {} reached terminal state: {state}.",
                wakeup.operation_id
            ),
            Some(status.to_string()),
        );

        let prior_user_message = {
            let threads = self.threads.read().await;
            threads.get(&wakeup.thread_id).and_then(|thread| {
                thread
                    .messages
                    .iter()
                    .rev()
                    .find(|message| message.role == MessageRole::User)
                    .map(|message| message.content.clone())
            })
        };

        let Some(prior_user_message) = prior_user_message else {
            return;
        };

        let task_id = self
            .operation_wakeup_active_task_id(&wakeup.thread_id)
            .await;
        let agent_id = self
            .active_agent_id_for_thread(&wakeup.thread_id)
            .await
            .unwrap_or_else(|| MAIN_AGENT_ID.to_string());
        self.enqueue_visible_thread_continuation(
            &wakeup.thread_id,
            DeferredVisibleThreadContinuation {
                agent_id,
                task_id,
                preferred_session_hint: None,
                llm_user_content: prior_user_message,
                force_compaction: false,
                rerun_participant_observers_after_turn: true,
                internal_delegate_sender: None,
                internal_delegate_message: None,
            },
        )
        .await;

        if let Err(error) = self
            .flush_deferred_visible_thread_continuations(&wakeup.thread_id)
            .await
        {
            tracing::warn!(
                operation_id = %wakeup.operation_id,
                thread_id = %wakeup.thread_id,
                error = %error,
                "operation completion wakeup continuation flush failed"
            );
        }
    }

    async fn operation_wakeup_active_task_id(&self, thread_id: &str) -> Option<String> {
        let tasks = self.tasks.lock().await;
        tasks
            .iter()
            .rev()
            .find(|task| {
                task.thread_id.as_deref() == Some(thread_id)
                    && matches!(
                        task.status,
                        TaskStatus::InProgress | TaskStatus::Blocked | TaskStatus::AwaitingApproval
                    )
            })
            .map(|task| task.id.clone())
    }
}

fn extract_operation_ids(content: &str) -> Vec<String> {
    content
        .lines()
        .filter_map(|line| line.trim().strip_prefix("operation_id: "))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

fn operation_status_is_terminal(status: &serde_json::Value) -> bool {
    matches!(
        status.get("state").and_then(|value| value.as_str()),
        Some("completed" | "failed" | "cancelled")
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tempfile::tempdir;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::{TcpListener, TcpStream};
    use tokio::time::{timeout, Duration};
    use zorai_shared::providers::PROVIDER_ID_OPENAI;

    async fn read_http_request_body(socket: &mut TcpStream) -> std::io::Result<String> {
        let mut buffer = Vec::new();
        let mut temp = [0_u8; 1024];
        loop {
            let n = socket.read(&mut temp).await?;
            if n == 0 {
                break;
            }
            buffer.extend_from_slice(&temp[..n]);
            if let Some(header_end) = buffer.windows(4).position(|window| window == b"\r\n\r\n") {
                let headers = String::from_utf8_lossy(&buffer[..header_end]);
                let content_len = headers
                    .lines()
                    .find_map(|line| {
                        line.to_ascii_lowercase()
                            .strip_prefix("content-length:")
                            .and_then(|value| value.trim().parse::<usize>().ok())
                    })
                    .unwrap_or(0);
                let body_start = header_end + 4;
                while buffer.len().saturating_sub(body_start) < content_len {
                    let n = socket.read(&mut temp).await?;
                    if n == 0 {
                        break;
                    }
                    buffer.extend_from_slice(&temp[..n]);
                }
                return Ok(String::from_utf8_lossy(&buffer[body_start..]).to_string());
            }
        }
        Ok(String::new())
    }

    #[tokio::test]
    async fn completed_operation_appends_single_thread_wakeup_message() {
        let root = tempdir().expect("tempdir should succeed");
        let manager = SessionManager::new_test(root.path()).await;
        let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
        let thread_id = "thread-operation-wakeup";
        let now = now_millis();
        {
            let mut threads = engine.threads.write().await;
            threads.insert(
                thread_id.to_string(),
                AgentThread {
                    id: thread_id.to_string(),
                    agent_name: None,
                    title: "Operation wakeup".to_string(),
                    messages: vec![AgentMessage::user("run the long command", now)],
                    pinned: false,
                    upstream_thread_id: None,
                    upstream_transport: None,
                    upstream_provider: None,
                    upstream_model: None,
                    upstream_assistant_id: None,
                    created_at: now,
                    updated_at: now,
                    total_input_tokens: 0,
                    total_output_tokens: 0,
                },
            );
        }

        let operation = crate::server::operation_registry()
            .accept_operation(zorai_protocol::tool_names::BASH_COMMAND, None);
        crate::server::operation_registry().mark_started(&operation.operation_id);
        engine
            .register_operation_wakeup(
                thread_id,
                zorai_protocol::tool_names::BASH_COMMAND,
                &operation.operation_id,
            )
            .await;
        crate::server::operation_registry().mark_completed_with_terminal_result(
            &operation.operation_id,
            serde_json::json!({
                "command": "sleep 1 && echo done",
                "exit_code": 0,
                "duration_ms": 1000,
                "stdout": "done",
            }),
        );

        engine
            .supervise_operation_completion_wakeups()
            .await
            .expect("operation wakeup supervision should succeed");
        engine
            .supervise_operation_completion_wakeups()
            .await
            .expect("second wakeup supervision should be idempotent");

        let threads = engine.threads.read().await;
        let thread = threads
            .get(thread_id)
            .expect("thread should remain available");
        let wakeups = thread
            .messages
            .iter()
            .filter(|message| {
                message.role == MessageRole::System
                    && message.content.contains("Background operation finished")
                    && message.content.contains(&operation.operation_id)
                    && message.content.contains("\"state\":\"completed\"")
            })
            .count();
        assert_eq!(wakeups, 1);
        assert!(engine.pending_operation_wakeup_count().await == 0);
    }

    #[tokio::test]
    async fn completed_operation_wakeup_is_claimed_by_one_concurrent_supervisor() {
        let root = tempdir().expect("tempdir should succeed");
        let manager = SessionManager::new_test(root.path()).await;
        let engine =
            Arc::new(AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await);
        let thread_id = "thread-operation-wakeup-concurrent";
        let now = now_millis();
        {
            let mut threads = engine.threads.write().await;
            threads.insert(
                thread_id.to_string(),
                AgentThread {
                    id: thread_id.to_string(),
                    agent_name: None,
                    title: "Concurrent operation wakeup".to_string(),
                    messages: vec![AgentMessage::user("run the long command", now)],
                    pinned: false,
                    upstream_thread_id: None,
                    upstream_transport: None,
                    upstream_provider: None,
                    upstream_model: None,
                    upstream_assistant_id: None,
                    created_at: now,
                    updated_at: now,
                    total_input_tokens: 0,
                    total_output_tokens: 0,
                },
            );
        }

        let operation = crate::server::operation_registry()
            .accept_operation(zorai_protocol::tool_names::BASH_COMMAND, None);
        crate::server::operation_registry().mark_started(&operation.operation_id);
        engine
            .register_operation_wakeup(
                thread_id,
                zorai_protocol::tool_names::BASH_COMMAND,
                &operation.operation_id,
            )
            .await;
        crate::server::operation_registry().mark_completed_with_terminal_result(
            &operation.operation_id,
            serde_json::json!({
                "command": "sleep 1 && echo done",
                "exit_code": 0,
                "duration_ms": 1000,
                "stdout": "done",
            }),
        );

        let supervisors = 16;
        let barrier = Arc::new(tokio::sync::Barrier::new(supervisors));
        let handles = (0..supervisors).map(|_| {
            let engine = engine.clone();
            let barrier = barrier.clone();
            async move {
                barrier.wait().await;
                engine.supervise_operation_completion_wakeups().await
            }
        });

        for result in futures::future::join_all(handles).await {
            result.expect("operation wakeup supervision should succeed");
        }

        let threads = engine.threads.read().await;
        let thread = threads
            .get(thread_id)
            .expect("thread should remain available");
        let wakeups = thread
            .messages
            .iter()
            .filter(|message| {
                message.role == MessageRole::System
                    && message.content.contains("Background operation finished")
                    && message.content.contains(&operation.operation_id)
                    && message.content.contains("\"state\":\"completed\"")
            })
            .count();
        assert_eq!(
            wakeups, 1,
            "racing supervisors must not append duplicate completion wakeups for the same operation"
        );
        assert!(engine.pending_operation_wakeup_count().await == 0);
    }

    #[tokio::test]
    async fn completed_operation_wakeup_waits_for_active_stream_before_resending() {
        let root = tempdir().expect("tempdir should succeed");
        let manager = SessionManager::new_test(root.path()).await;
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind operation wakeup test server");
        let addr = listener.local_addr().expect("operation wakeup addr");
        let request_counter = Arc::new(AtomicUsize::new(0));
        let first_request_started = Arc::new(tokio::sync::Notify::new());
        let second_request_started = Arc::new(tokio::sync::Notify::new());
        let release_first_response = Arc::new(tokio::sync::Notify::new());

        tokio::spawn({
            let request_counter = request_counter.clone();
            let first_request_started = first_request_started.clone();
            let second_request_started = second_request_started.clone();
            let release_first_response = release_first_response.clone();
            async move {
                loop {
                    let Ok((mut socket, _)) = listener.accept().await else {
                        break;
                    };
                    let request_counter = request_counter.clone();
                    let first_request_started = first_request_started.clone();
                    let second_request_started = second_request_started.clone();
                    let release_first_response = release_first_response.clone();
                    tokio::spawn(async move {
                        let _body = read_http_request_body(&mut socket)
                            .await
                            .expect("read operation wakeup request");
                        let request_index = request_counter.fetch_add(1, Ordering::SeqCst);
                        let response_body = if request_index == 0 {
                            first_request_started.notify_waiters();
                            release_first_response.notified().await;
                            concat!(
                                "data: {\"choices\":[{\"delta\":{\"content\":\"first stream finished\"}}]}\n\n",
                                "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":7,\"completion_tokens\":3}}\n\n",
                                "data: [DONE]\n\n"
                            )
                        } else {
                            second_request_started.notify_waiters();
                            concat!(
                                "data: {\"choices\":[{\"delta\":{\"content\":\"wakeup continuation finished\"}}]}\n\n",
                                "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":8,\"completion_tokens\":4}}\n\n",
                                "data: [DONE]\n\n"
                            )
                        };
                        let response = format!(
                            "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncache-control: no-cache\r\nconnection: close\r\n\r\n{}",
                            response_body
                        );
                        socket
                            .write_all(response.as_bytes())
                            .await
                            .expect("write operation wakeup response");
                    });
                }
            }
        });

        let mut config = AgentConfig::default();
        config.provider = PROVIDER_ID_OPENAI.to_string();
        config.base_url = format!("http://{addr}/v1");
        config.model = "gpt-5.4-mini".to_string();
        config.api_key = "test-key".to_string();
        config.auth_source = AuthSource::ApiKey;
        config.api_transport = ApiTransport::ChatCompletions;
        config.auto_retry = false;
        config.max_retries = 0;
        config.max_tool_loops = 1;
        let engine = Arc::new(AgentEngine::new_test(manager, config, root.path()).await);
        let thread_id = "thread-operation-wakeup-active-stream";
        let now = now_millis();
        {
            let mut threads = engine.threads.write().await;
            threads.insert(
                thread_id.to_string(),
                AgentThread {
                    id: thread_id.to_string(),
                    agent_name: None,
                    title: "Operation wakeup active stream".to_string(),
                    messages: vec![AgentMessage::user("run the long command", now)],
                    pinned: false,
                    upstream_thread_id: None,
                    upstream_transport: None,
                    upstream_provider: None,
                    upstream_model: None,
                    upstream_assistant_id: None,
                    created_at: now,
                    updated_at: now,
                    total_input_tokens: 0,
                    total_output_tokens: 0,
                },
            );
        }

        let send_task = tokio::spawn({
            let engine = engine.clone();
            async move {
                engine
                    .resend_existing_user_message(thread_id, "run the long command")
                    .await
            }
        });

        timeout(Duration::from_secs(1), first_request_started.notified())
            .await
            .expect("first request should start");
        timeout(Duration::from_secs(1), async {
            loop {
                let streams = engine.stream_cancellations.lock().await;
                if streams.contains_key(thread_id) {
                    break;
                }
                drop(streams);
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .expect("active stream should be registered");

        let operation = crate::server::operation_registry()
            .accept_operation(zorai_protocol::tool_names::BASH_COMMAND, None);
        crate::server::operation_registry().mark_started(&operation.operation_id);
        engine
            .register_operation_wakeup(
                thread_id,
                zorai_protocol::tool_names::BASH_COMMAND,
                &operation.operation_id,
            )
            .await;
        crate::server::operation_registry().mark_completed_with_terminal_result(
            &operation.operation_id,
            serde_json::json!({
                "command": "sleep 1 && echo done",
                "exit_code": 0,
                "duration_ms": 1000,
                "stdout": "done",
            }),
        );

        engine
            .supervise_operation_completion_wakeups()
            .await
            .expect("operation wakeup supervision should succeed");

        assert!(
            timeout(
                Duration::from_millis(150),
                second_request_started.notified()
            )
            .await
            .is_err(),
            "operation wakeup must queue the continuation instead of opening a second stream while the first stream is active"
        );

        release_first_response.notify_waiters();
        timeout(Duration::from_secs(2), second_request_started.notified())
            .await
            .expect("queued operation wakeup should run after the active stream finishes");
        timeout(Duration::from_secs(2), send_task)
            .await
            .expect("send task should finish")
            .expect("send task should join")
            .expect("send task should succeed");

        assert_eq!(
            request_counter.load(Ordering::SeqCst),
            2,
            "expected the initial stream and one queued operation wakeup continuation"
        );
    }
}
