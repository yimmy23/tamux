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

            {
                let mut wakeups = self.operation_wakeups.lock().await;
                wakeups.remove(&wakeup.operation_id);
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
            wakeup.operation_id,
            wakeup.tool_name,
            state,
            wakeup.registered_at,
            status
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

        if let Some(task_id) = self
            .operation_wakeup_active_task_id(&wakeup.thread_id)
            .await
        {
            if let Err(error) = self
                .resend_existing_user_message_for_task(
                    &wakeup.thread_id,
                    &prior_user_message,
                    &task_id,
                )
                .await
            {
                tracing::warn!(
                    operation_id = %wakeup.operation_id,
                    thread_id = %wakeup.thread_id,
                    task_id = %task_id,
                    error = %error,
                    "operation completion wakeup resend failed"
                );
            }
        } else if let Err(error) = self
            .resend_existing_user_message(&wakeup.thread_id, &prior_user_message)
            .await
        {
            tracing::warn!(
                operation_id = %wakeup.operation_id,
                thread_id = %wakeup.thread_id,
                error = %error,
                "operation completion wakeup resend failed"
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
    use tempfile::tempdir;

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
}
