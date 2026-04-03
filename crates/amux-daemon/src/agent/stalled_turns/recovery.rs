use super::runtime::StalledTurnCandidate;
use super::*;

impl AgentEngine {
    pub(super) async fn perform_stalled_turn_retry(
        &self,
        candidate: &StalledTurnCandidate,
    ) -> Result<()> {
        let attempt = candidate.retries_sent.saturating_add(1);
        let recovery_message = stalled_turn_system_message(candidate, attempt);

        {
            let mut threads = self.threads.write().await;
            let Some(thread) = threads.get_mut(&candidate.thread_id) else {
                anyhow::bail!(
                    "thread {} disappeared before stalled-turn retry",
                    candidate.thread_id
                );
            };
            let mut msg = AgentMessage::user(recovery_message, now_millis());
            msg.role = MessageRole::System;
            thread.messages.push(msg);
            thread.updated_at = now_millis();
        }
        self.persist_thread_by_id(&candidate.thread_id).await;

        self.emit_workflow_notice(
            &candidate.thread_id,
            "stalled-turn-retry",
            format!("WELES requested continuation for an unfinished turn (attempt {attempt}/3)."),
            Some(
                serde_json::json!({
                    "attempt": attempt,
                    "stall_class": candidate.class.as_str(),
                    "task_id": candidate.task_id,
                    "goal_run_id": candidate.goal_run_id,
                })
                .to_string(),
            ),
        );

        self.send_message_inner(
            Some(&candidate.thread_id),
            "continue",
            candidate.task_id.as_deref(),
            None,
            None,
            None,
            None,
            None,
            false,
        )
        .await?;
        Ok(())
    }

    pub(super) async fn perform_stalled_turn_escalation(&self, candidate: &StalledTurnCandidate) {
        let message = format!(
            "Thread marked as stuck_needs_recovery after 3 stalled-turn retries. Last unfinished message: {}",
            candidate.last_message_excerpt
        );
        self.emit_workflow_notice(
            &candidate.thread_id,
            "stuck-needs-recovery",
            message.clone(),
            Some(
                serde_json::json!({
                    "stall_class": candidate.class.as_str(),
                    "task_id": candidate.task_id,
                    "goal_run_id": candidate.goal_run_id,
                    "retries_sent": candidate.retries_sent,
                })
                .to_string(),
            ),
        );

        if let Some(task_id) = candidate.task_id.as_deref() {
            let updated = {
                let mut tasks = self.tasks.lock().await;
                if let Some(task) = tasks.iter_mut().find(|task| task.id == task_id) {
                    task.blocked_reason = Some("stuck_needs_recovery".to_string());
                    task.logs.push(make_task_log_entry(
                        task.retry_count,
                        TaskLogLevel::Warn,
                        "stalled-turn-recovery",
                        "automatic stalled-turn recovery exhausted",
                        Some(message.clone()),
                    ));
                    Some(task.clone())
                } else {
                    None
                }
            };
            if let Some(task) = updated {
                self.persist_tasks().await;
                self.emit_task_update(
                    &task,
                    Some("Automatic stalled-turn recovery exhausted".into()),
                );
            }
        }
    }
}

fn stalled_turn_system_message(candidate: &StalledTurnCandidate, attempt: u32) -> String {
    match attempt {
        1 => "WELES stalled-turn recovery: Continue from your last unfinished action.".to_string(),
        2 => format!(
            "WELES stalled-turn recovery: You said \"{}\" but no concrete follow-through happened. Continue with that unfinished work now.",
            candidate.last_message_excerpt
        ),
        _ => format!(
            "WELES stalled-turn recovery: Your previous turn stopped after promising work but before taking action. Resume immediately from this unfinished step: {}",
            candidate.last_message_excerpt
        ),
    }
}
