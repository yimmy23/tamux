use super::*;

#[derive(Debug, Clone)]
pub(super) struct OperationWakeup {
    pub(super) operation_id: String,
    pub(super) thread_id: String,
    pub(super) tool_name: String,
    pub(super) registered_at: u64,
    pub(super) terminal_observed_at: Option<u64>,
}

struct OperationWakeupPayloadRef {
    payload_id: String,
    file_path: String,
    operation_count: usize,
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
                terminal_observed_at: None,
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
        let now = now_millis();
        let mut terminal_wakeups_by_thread: std::collections::BTreeMap<
            String,
            Vec<(OperationWakeup, serde_json::Value)>,
        > = std::collections::BTreeMap::new();
        let mut latest_terminal_observed_by_thread: std::collections::BTreeMap<String, u64> =
            std::collections::BTreeMap::new();

        for wakeup in wakeups {
            let Some(status) = self.operation_wakeup_status(&wakeup).await? else {
                continue;
            };
            if !operation_status_is_terminal(&status) {
                continue;
            }

            let Some(terminal_observed_at) = ({
                let mut wakeups = self.operation_wakeups.lock().await;
                wakeups
                    .get_mut(&wakeup.operation_id)
                    .map(|stored| *stored.terminal_observed_at.get_or_insert(now))
            }) else {
                continue;
            };

            latest_terminal_observed_by_thread
                .entry(wakeup.thread_id.clone())
                .and_modify(|latest| *latest = (*latest).max(terminal_observed_at))
                .or_insert(terminal_observed_at);
            terminal_wakeups_by_thread
                .entry(wakeup.thread_id.clone())
                .or_default()
                .push((wakeup, status));
        }

        for (thread_id, grouped_wakeups) in terminal_wakeups_by_thread {
            let latest_observed_at = latest_terminal_observed_by_thread
                .get(&thread_id)
                .copied()
                .unwrap_or(now);
            if latest_observed_at.saturating_add(operation_wakeup_debounce_ms()) > now {
                continue;
            }

            let claimed_wakeups = {
                let mut wakeups = self.operation_wakeups.lock().await;
                grouped_wakeups
                    .into_iter()
                    .filter(|(wakeup, _)| wakeups.remove(&wakeup.operation_id).is_some())
                    .collect::<Vec<_>>()
            };
            if claimed_wakeups.is_empty() {
                continue;
            }
            if self
                .perform_operation_completion_wakeup_batch(&thread_id, &claimed_wakeups)
                .await
            {
                self.enqueue_operation_completion_continuation(&thread_id)
                    .await;
                if let Err(error) = self
                    .flush_deferred_visible_thread_continuations(&thread_id)
                    .await
                {
                    tracing::warn!(
                        thread_id = %thread_id,
                        error = %error,
                        "operation completion wakeup continuation flush failed"
                    );
                }
            }
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

    async fn perform_operation_completion_wakeup_batch(
        &self,
        thread_id: &str,
        wakeups: &[(OperationWakeup, serde_json::Value)],
    ) -> bool {
        let payload_ref = match self
            .write_operation_wakeup_payload(thread_id, wakeups)
            .await
        {
            Ok(payload_ref) => payload_ref,
            Err(error) => {
                tracing::warn!(
                    thread_id = %thread_id,
                    %error,
                    "failed to offload operation wakeup payload"
                );
                let message = if wakeups.len() == 1 {
                    "Background operation finished.\n\nOperation result could not be saved to file; see daemon logs.".to_string()
                } else {
                    format!(
                        "Background operations finished.\n\n{} operation results could not be saved to file; see daemon logs.",
                        wakeups.len()
                    )
                };
                return self.append_system_thread_message(thread_id, message).await;
            }
        };

        let message = if wakeups.len() == 1 {
            let (_, status) = &wakeups[0];
            let state = status
                .get("state")
                .and_then(|value| value.as_str())
                .unwrap_or("unknown");
            format!(
                "Background operation finished.\n\nOperation result saved to file.\n- operations: {}\n- state: {}\n- payload_id: {}\n- file_path: {}\n- read_with: read_offloaded_payload {{\"payload_id\":\"{}\",\"full\":true}}",
                payload_ref.operation_count,
                state,
                payload_ref.payload_id,
                payload_ref.file_path,
                payload_ref.payload_id
            )
        } else {
            format!(
                "Background operations finished.\n\nOperation results saved to file.\n- operations: {}\n- payload_id: {}\n- file_path: {}\n- read_with: read_offloaded_payload {{\"payload_id\":\"{}\",\"full\":true}}",
                payload_ref.operation_count,
                payload_ref.payload_id,
                payload_ref.file_path,
                payload_ref.payload_id
            )
        };

        if !self.append_system_thread_message(thread_id, message).await {
            return false;
        }

        if wakeups.len() == 1 {
            let (wakeup, status) = &wakeups[0];
            let state = status
                .get("state")
                .and_then(|value| value.as_str())
                .unwrap_or("unknown");
            self.emit_workflow_notice(
                thread_id,
                "operation-completion-wakeup",
                format!(
                    "Background operation {} reached terminal state: {state}.",
                    wakeup.operation_id
                ),
                Some(operation_wakeup_payload_ref_json(&payload_ref).to_string()),
            );
            return true;
        }

        self.emit_workflow_notice(
            thread_id,
            "operation-completion-wakeup",
            format!(
                "{} background operations reached terminal states.",
                wakeups.len()
            ),
            Some(operation_wakeup_payload_ref_json(&payload_ref).to_string()),
        );
        true
    }

    async fn write_operation_wakeup_payload(
        &self,
        thread_id: &str,
        wakeups: &[(OperationWakeup, serde_json::Value)],
    ) -> Result<OperationWakeupPayloadRef> {
        let payload_id = uuid::Uuid::new_v4().to_string();
        let payload_path = self.history.offloaded_payload_path(thread_id, &payload_id);
        let results = wakeups
            .iter()
            .map(operation_wakeup_result_payload)
            .collect::<Vec<_>>();
        let raw_payload = format!(
            "{}\n",
            serde_json::to_string_pretty(&serde_json::Value::Array(results))?
        );
        let parent = payload_path
            .parent()
            .ok_or_else(|| anyhow::anyhow!("offloaded operation payload path missing parent"))?;
        tokio::fs::create_dir_all(parent)
            .await
            .with_context(|| format!("create operation payload directory {}", parent.display()))?;
        tokio::fs::write(&payload_path, raw_payload.as_bytes())
            .await
            .with_context(|| format!("write operation payload file {}", payload_path.display()))?;

        let summary = format!(
            "Background operation results offloaded\n- operations: {}\n- payload_id: {}",
            wakeups.len(),
            payload_id
        );
        if let Err(error) = self
            .history
            .upsert_offloaded_payload_metadata(
                &payload_id,
                thread_id,
                "operation_completion_wakeup",
                None,
                "application/json",
                raw_payload.len() as u64,
                &summary,
                now_millis(),
            )
            .await
        {
            let _ = tokio::fs::remove_file(&payload_path).await;
            return Err(error).context("persist operation wakeup payload metadata");
        }

        Ok(OperationWakeupPayloadRef {
            payload_id,
            file_path: payload_path.to_string_lossy().into_owned(),
            operation_count: wakeups.len(),
        })
    }

    async fn enqueue_operation_completion_continuation(&self, thread_id: &str) {
        let prior_user_message = {
            let threads = self.threads.read().await;
            threads.get(thread_id).and_then(|thread| {
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

        let task_id = self.operation_wakeup_active_task_id(thread_id).await;
        let agent_id = if let Some(task_id) = task_id.as_deref() {
            self.agent_scope_id_for_turn(Some(thread_id), Some(task_id))
                .await
        } else {
            self.active_agent_id_for_thread(thread_id)
                .await
                .unwrap_or_else(|| MAIN_AGENT_ID.to_string())
        };
        self.enqueue_visible_thread_continuation(
            thread_id,
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
    }
}

fn operation_wakeup_payload_ref_json(payload_ref: &OperationWakeupPayloadRef) -> serde_json::Value {
    serde_json::json!({
        "operations": payload_ref.operation_count,
        "payload_id": payload_ref.payload_id,
        "file_path": payload_ref.file_path,
    })
}

fn operation_wakeup_result_payload(
    (wakeup, status): &(OperationWakeup, serde_json::Value),
) -> serde_json::Value {
    let state = status
        .get("state")
        .and_then(|value| value.as_str())
        .unwrap_or("unknown");
    serde_json::json!({
        "operation_id": wakeup.operation_id,
        "tool": wakeup.tool_name,
        "state": state,
        "registered_at": wakeup.registered_at,
        "status": status,
    })
}

fn operation_wakeup_debounce_ms() -> u64 {
    30_000
}

impl AgentEngine {
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
    use tokio::time::{Duration, timeout};
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

    async fn mark_operation_wakeup_debounce_elapsed(engine: &AgentEngine, operation_id: &str) {
        let mut wakeups = engine.operation_wakeups.lock().await;
        let Some(wakeup) = wakeups.get_mut(operation_id) else {
            panic!("operation wakeup should be registered: {operation_id}");
        };
        wakeup.terminal_observed_at = Some(
            now_millis()
                .saturating_sub(operation_wakeup_debounce_ms())
                .saturating_sub(1),
        );
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
        mark_operation_wakeup_debounce_elapsed(&engine, &operation.operation_id).await;

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
                    && message.content.contains("Operation result saved to file.")
            })
            .collect::<Vec<_>>();
        assert_eq!(wakeups.len(), 1);
        assert!(
            !wakeups[0].content.contains(&operation.operation_id),
            "operation id should be in offloaded payload, not inline message"
        );
        let payload_id = wakeups[0]
            .content
            .lines()
            .find_map(|line| line.trim().strip_prefix("- payload_id: "))
            .expect("wakeup message should include payload id");
        let file_path = wakeups[0]
            .content
            .lines()
            .find_map(|line| line.trim().strip_prefix("- file_path: "))
            .expect("wakeup message should include file path");
        let payload = std::fs::read_to_string(file_path)
            .expect("offloaded operation wakeup payload should exist");
        assert!(payload.contains(&operation.operation_id));
        assert!(payload.contains("\"state\": \"completed\""));
        assert!(
            engine
                .history
                .get_offloaded_payload_metadata(payload_id)
                .await
                .expect("metadata lookup should succeed")
                .is_some()
        );
        assert!(engine.pending_operation_wakeup_count().await == 0);
    }

    #[tokio::test]
    async fn completed_operation_wakeup_waits_for_debounce_window() {
        let root = tempdir().expect("tempdir should succeed");
        let manager = SessionManager::new_test(root.path()).await;
        let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
        let thread_id = "thread-operation-wakeup-debounce";
        let now = now_millis();
        {
            let mut threads = engine.threads.write().await;
            threads.insert(
                thread_id.to_string(),
                AgentThread {
                    id: thread_id.to_string(),
                    agent_name: None,
                    title: "Operation wakeup debounce".to_string(),
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

        {
            let threads = engine.threads.read().await;
            let thread = threads
                .get(thread_id)
                .expect("thread should remain available");
            assert!(
                thread
                    .messages
                    .iter()
                    .all(|message| !message.content.contains("Background operation finished")),
                "terminal wakeup should wait for the debounce window before notifying the thread"
            );
        }
        assert_eq!(engine.pending_operation_wakeup_count().await, 1);

        mark_operation_wakeup_debounce_elapsed(&engine, &operation.operation_id).await;
        engine
            .supervise_operation_completion_wakeups()
            .await
            .expect("operation wakeup supervision should succeed after debounce");

        let threads = engine.threads.read().await;
        let thread = threads
            .get(thread_id)
            .expect("thread should remain available");
        assert!(
            thread
                .messages
                .iter()
                .any(|message| message.content.contains("Background operation finished")),
            "debounced wakeup should be appended once the window has elapsed"
        );
        assert_eq!(engine.pending_operation_wakeup_count().await, 0);
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
        mark_operation_wakeup_debounce_elapsed(&engine, &operation.operation_id).await;

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
                    && message.content.contains("Operation result saved to file.")
            })
            .count();
        assert_eq!(
            wakeups, 1,
            "racing supervisors must not append duplicate completion wakeups for the same operation"
        );
        assert!(engine.pending_operation_wakeup_count().await == 0);
    }

    #[tokio::test]
    async fn same_thread_operation_wakeups_coalesce_to_one_continuation() {
        let root = tempdir().expect("tempdir should succeed");
        let manager = SessionManager::new_test(root.path()).await;
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind operation wakeup test server");
        let addr = listener.local_addr().expect("operation wakeup addr");
        let request_counter = Arc::new(AtomicUsize::new(0));

        tokio::spawn({
            let request_counter = request_counter.clone();
            async move {
                loop {
                    let Ok((mut socket, _)) = listener.accept().await else {
                        break;
                    };
                    let request_counter = request_counter.clone();
                    tokio::spawn(async move {
                        let _body = read_http_request_body(&mut socket)
                            .await
                            .expect("read operation wakeup request");
                        request_counter.fetch_add(1, Ordering::SeqCst);
                        let response_body = concat!(
                            "data: {\"choices\":[{\"delta\":{\"content\":\"coalesced wakeup handled\"}}]}\n\n",
                            "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":8,\"completion_tokens\":4}}\n\n",
                            "data: [DONE]\n\n"
                        );
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
        let engine = AgentEngine::new_test(manager, config, root.path()).await;
        let thread_id = "thread-operation-wakeup-coalesce";
        let now = now_millis();
        {
            let mut threads = engine.threads.write().await;
            threads.insert(
                thread_id.to_string(),
                AgentThread {
                    id: thread_id.to_string(),
                    agent_name: None,
                    title: "Operation wakeup coalesce".to_string(),
                    messages: vec![AgentMessage::user("run both long commands", now)],
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

        let first_operation = crate::server::operation_registry()
            .accept_operation(zorai_protocol::tool_names::BASH_COMMAND, None);
        let second_operation = crate::server::operation_registry()
            .accept_operation(zorai_protocol::tool_names::BASH_COMMAND, None);
        for operation in [&first_operation, &second_operation] {
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
        }
        mark_operation_wakeup_debounce_elapsed(&engine, &first_operation.operation_id).await;
        mark_operation_wakeup_debounce_elapsed(&engine, &second_operation.operation_id).await;

        engine
            .supervise_operation_completion_wakeups()
            .await
            .expect("operation wakeup supervision should succeed");

        assert_eq!(
            request_counter.load(Ordering::SeqCst),
            1,
            "same-thread operation wakeups in one supervisor pass should produce one visible continuation"
        );

        let threads = engine.threads.read().await;
        let thread = threads
            .get(thread_id)
            .expect("thread should remain available");
        let wakeup_messages = thread
            .messages
            .iter()
            .filter(|message| {
                message.role == MessageRole::System
                    && message.content.contains("Background operations finished")
            })
            .collect::<Vec<_>>();
        assert_eq!(
            wakeup_messages.len(),
            1,
            "same-thread wakeups should be gathered into one visible result payload"
        );
        let message = &wakeup_messages[0].content;
        assert!(
            message.contains("Background operations finished"),
            "{message}"
        );
        assert!(
            message.contains("Operation results saved to file."),
            "{message}"
        );
        assert!(
            !message.contains(&first_operation.operation_id),
            "{message}"
        );
        assert!(
            !message.contains(&second_operation.operation_id),
            "{message}"
        );

        let payload_id = message
            .lines()
            .find_map(|line| line.trim().strip_prefix("- payload_id: "))
            .expect("batched wakeup message should include payload id");
        let file_path = message
            .lines()
            .find_map(|line| line.trim().strip_prefix("- file_path: "))
            .expect("batched wakeup message should include payload file path");
        let results_json = std::fs::read_to_string(file_path)
            .expect("offloaded operation result payload should exist");
        let results: serde_json::Value =
            serde_json::from_str(&results_json).expect("operation results should be JSON");
        assert_eq!(
            results.as_array().map(Vec::len),
            Some(2),
            "batched wakeup payload should include both operation results"
        );
        assert!(
            results_json.contains(&first_operation.operation_id)
                && results_json.contains(&second_operation.operation_id),
            "offloaded payload should contain both operation ids"
        );
        let metadata = engine
            .history
            .get_offloaded_payload_metadata(payload_id)
            .await
            .expect("metadata lookup should succeed")
            .expect("metadata should exist");
        assert_eq!(metadata.thread_id, thread_id);
        assert_eq!(metadata.content_type, "application/json");
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
        mark_operation_wakeup_debounce_elapsed(&engine, &operation.operation_id).await;

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

    #[tokio::test]
    async fn operator_message_clears_queued_operation_wakeup_continuations() {
        let root = tempdir().expect("tempdir should succeed");
        let manager = SessionManager::new_test(root.path()).await;
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind queued continuation test server");
        let addr = listener.local_addr().expect("queued continuation addr");
        let request_counter = Arc::new(AtomicUsize::new(0));

        tokio::spawn({
            let request_counter = request_counter.clone();
            async move {
                loop {
                    let Ok((mut socket, _)) = listener.accept().await else {
                        break;
                    };
                    let request_counter = request_counter.clone();
                    tokio::spawn(async move {
                        let _body = read_http_request_body(&mut socket)
                            .await
                            .expect("read queued continuation request");
                        let request_index = request_counter.fetch_add(1, Ordering::SeqCst);
                        let response_body = format!(
                            "data: {{\"choices\":[{{\"delta\":{{\"content\":\"operator response {}\"}}}}]}}\n\n\
                             data: {{\"choices\":[{{\"delta\":{{}},\"finish_reason\":\"stop\"}}],\"usage\":{{\"prompt_tokens\":7,\"completion_tokens\":3}}}}\n\n\
                             data: [DONE]\n\n",
                            request_index
                        );
                        let response = format!(
                            "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncache-control: no-cache\r\nconnection: close\r\n\r\n{}",
                            response_body
                        );
                        socket
                            .write_all(response.as_bytes())
                            .await
                            .expect("write queued continuation response");
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
        let engine = AgentEngine::new_test(manager, config, root.path()).await;
        let thread_id = "thread-operation-wakeup-operator-supersedes";
        let now = now_millis();
        {
            let mut threads = engine.threads.write().await;
            threads.insert(
                thread_id.to_string(),
                AgentThread {
                    id: thread_id.to_string(),
                    agent_name: None,
                    title: "Operator supersedes operation wakeup".to_string(),
                    messages: vec![AgentMessage::user("verify the background task", now)],
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

        for index in 0..2 {
            engine
                .enqueue_visible_thread_continuation(
                    thread_id,
                    DeferredVisibleThreadContinuation {
                        agent_id: MAIN_AGENT_ID.to_string(),
                        task_id: None,
                        preferred_session_hint: None,
                        llm_user_content: format!("background continuation {index}"),
                        force_compaction: false,
                        rerun_participant_observers_after_turn: true,
                        internal_delegate_sender: None,
                        internal_delegate_message: None,
                    },
                )
                .await;
        }
        assert_eq!(
            engine
                .deferred_visible_thread_continuations_for(thread_id)
                .await
                .len(),
            2
        );

        engine
            .send_message_inner(
                Some(thread_id),
                "it works, stop checking",
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                true,
            )
            .await
            .expect("operator turn should succeed");

        assert_eq!(
            request_counter.load(Ordering::SeqCst),
            1,
            "operator reply should supersede queued background continuations instead of draining them into repeated assistant turns"
        );
        assert!(
            engine
                .deferred_visible_thread_continuations_for(thread_id)
                .await
                .is_empty()
        );
    }

    #[tokio::test]
    async fn duplicate_visible_thread_continuations_are_coalesced() {
        let root = tempdir().expect("tempdir should succeed");
        let manager = SessionManager::new_test(root.path()).await;
        let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
        let thread_id = "thread-duplicate-visible-continuations";

        let continuation = DeferredVisibleThreadContinuation {
            agent_id: MAIN_AGENT_ID.to_string(),
            task_id: None,
            preferred_session_hint: None,
            llm_user_content: "continue after background completion".to_string(),
            force_compaction: false,
            rerun_participant_observers_after_turn: true,
            internal_delegate_sender: None,
            internal_delegate_message: None,
        };

        engine
            .enqueue_visible_thread_continuation(thread_id, continuation.clone())
            .await;
        engine
            .enqueue_visible_thread_continuation(thread_id, continuation)
            .await;

        assert_eq!(
            engine
                .deferred_visible_thread_continuations_for(thread_id)
                .await
                .len(),
            1,
            "identical deferred continuations should collapse to one queued turn"
        );
    }

    #[tokio::test]
    async fn completed_operation_wakeup_queues_subagent_continuation_for_task_owner() {
        let root = tempdir().expect("tempdir should succeed");
        let manager = SessionManager::new_test(root.path()).await;
        let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
        let thread_id = "thread-operation-wakeup-subagent";
        let now = now_millis();
        {
            let mut threads = engine.threads.write().await;
            threads.insert(
                thread_id.to_string(),
                AgentThread {
                    id: thread_id.to_string(),
                    agent_name: Some(crate::agent::agent_identity::DAZHBOG_AGENT_NAME.to_string()),
                    title: "Subagent operation wakeup".to_string(),
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
        let task = engine
            .enqueue_task(
                "Spawned worker".to_string(),
                "Run the long command".to_string(),
                "normal",
                None,
                None,
                Vec::new(),
                None,
                "subagent",
                None,
                Some("task-parent".to_string()),
                Some(thread_id.to_string()),
                Some("daemon".to_string()),
            )
            .await;
        {
            let mut tasks = engine.tasks.lock().await;
            let task = tasks
                .iter_mut()
                .find(|entry| entry.id == task.id)
                .expect("subagent task should exist");
            task.status = TaskStatus::InProgress;
            task.started_at = Some(now);
            task.thread_id = Some(thread_id.to_string());
            task.override_system_prompt = Some(
                "Agent persona: Dazhbog\nAgent persona id: dazhbog\nTask-owned builtin persona."
                    .to_string(),
            );
        }
        assert_eq!(
            engine
                .operation_wakeup_active_task_id(thread_id)
                .await
                .as_deref(),
            Some(task.id.as_str())
        );
        assert_eq!(
            engine
                .agent_scope_id_for_turn(Some(thread_id), Some(task.id.as_str()))
                .await,
            crate::agent::agent_identity::DAZHBOG_AGENT_ID
        );

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
        mark_operation_wakeup_debounce_elapsed(&engine, &operation.operation_id).await;

        let (_generation, _, _) = engine.begin_stream_cancellation(thread_id).await;
        engine
            .supervise_operation_completion_wakeups()
            .await
            .expect("operation wakeup supervision should succeed");

        let continuations = engine
            .deferred_visible_thread_continuations_for(thread_id)
            .await;
        assert_eq!(continuations.len(), 1);
        assert_eq!(
            continuations[0].agent_id,
            crate::agent::agent_identity::DAZHBOG_AGENT_ID,
            "operation wakeups for spawned task threads must resume the owning subagent"
        );
        assert_eq!(continuations[0].task_id.as_deref(), Some(task.id.as_str()));
    }
}
