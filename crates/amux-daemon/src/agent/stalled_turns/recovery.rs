use super::runtime::StalledTurnCandidate;
use super::types::StalledTurnClass;
use super::*;

impl AgentEngine {
    pub(super) async fn perform_stalled_turn_retry(
        &self,
        candidate: &StalledTurnCandidate,
    ) -> Result<()> {
        let attempt = candidate.retries_sent.saturating_add(1);
        let recovery_message = stalled_turn_system_message(candidate, attempt);
        let prior_user_message = {
            let threads = self.threads.read().await;
            let Some(thread) = threads.get(&candidate.thread_id) else {
                anyhow::bail!(
                    "thread {} disappeared before stalled-turn retry",
                    candidate.thread_id
                );
            };
            thread
                .messages
                .iter()
                .rev()
                .find(|message| message.role == MessageRole::User)
                .map(|message| message.content.clone())
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "thread {} has no prior user message for stalled-turn retry",
                        candidate.thread_id
                    )
                })?
        };

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
        let _ = self.event_tx.send(AgentEvent::ThreadReloadRequired {
            thread_id: candidate.thread_id.clone(),
        });

        let responder_agent_id = self
            .active_agent_id_for_thread(&candidate.thread_id)
            .await
            .unwrap_or_else(|| crate::agent::agent_identity::MAIN_AGENT_ID.to_string());
        if crate::agent::agent_identity::canonical_agent_id(&responder_agent_id)
            != crate::agent::agent_identity::WELES_AGENT_ID
        {
            let internal_message = stalled_turn_internal_dm_message(
                candidate,
                attempt,
                &prior_user_message,
                &responder_agent_id,
            );
            if let Err(error) = self
                .send_internal_agent_message(
                    crate::agent::agent_identity::WELES_AGENT_ID,
                    &responder_agent_id,
                    &internal_message,
                    None,
                )
                .await
            {
                tracing::warn!(
                    thread_id = %candidate.thread_id,
                    recipient = %responder_agent_id,
                    error = %error,
                    "failed to send stalled-turn internal DM"
                );
            }
        }

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

        self.resend_existing_user_message(&candidate.thread_id, &prior_user_message)
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
    if candidate.class == StalledTurnClass::ActiveStreamIdle {
        return match attempt {
            1 => "WELES stalled-turn recovery: Your stream went idle before completion. Resume the unfinished turn now.".to_string(),
            2 => format!(
                "WELES stalled-turn recovery: Your stream went idle after partial output (\"{}\"). Continue the unfinished turn now.",
                candidate.last_message_excerpt
            ),
            _ => format!(
                "WELES stalled-turn recovery: The stream kept stalling before completion. Resume immediately from this unfinished point: {}",
                candidate.last_message_excerpt
            ),
        };
    }

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

fn stalled_turn_internal_dm_message(
    candidate: &StalledTurnCandidate,
    attempt: u32,
    prior_user_message: &str,
    responder_agent_id: &str,
) -> String {
    let responder_name = crate::agent::agent_identity::canonical_agent_name(responder_agent_id);
    match candidate.class {
        StalledTurnClass::ActiveStreamIdle => format!(
            "WELES stalled-turn recovery for thread `{}`.\nAttempt {attempt}/3.\nYou are the active responder ({responder_name}). Your stream went idle before completion after partial output: \"{}\"\nResume the original operator request immediately: {}",
            candidate.thread_id,
            candidate.last_message_excerpt,
            prior_user_message,
        ),
        _ => format!(
            "WELES stalled-turn recovery for thread `{}`.\nAttempt {attempt}/3.\nYou are the active responder ({responder_name}). Your last unfinished message was: \"{}\"\nContinue the original operator request immediately: {}",
            candidate.thread_id,
            candidate.last_message_excerpt,
            prior_user_message,
        ),
    }
}
