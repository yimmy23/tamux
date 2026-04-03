use super::*;

fn build_handoff_restart_user_message(
    previous_agent_name: &str,
    next_agent_name: &str,
    original_user_message: &str,
    tool_arguments: &str,
) -> Option<String> {
    let args: serde_json::Value = serde_json::from_str(tool_arguments).ok()?;
    let reason = args.get("reason").and_then(|value| value.as_str()).unwrap_or("");
    let summary = args
        .get("summary")
        .and_then(|value| value.as_str())
        .unwrap_or("");
    let action = args
        .get("action")
        .and_then(|value| value.as_str())
        .unwrap_or("push_handoff");

    let intro = if action == "return_handoff" {
        format!(
            "User requested you while talking to {previous_agent_name}. Control has returned to {next_agent_name}."
        )
    } else {
        format!(
            "User requested you while talking to {previous_agent_name}. You are now the active responder for this thread as {next_agent_name}."
        )
    };

    Some(format!(
        "{intro}\nHandoff reason: {reason}\nConversation summary from the previous responder: {summary}\nLatest operator request to answer now: {original_user_message}"
    ))
}

impl<'a> SendMessageRunner<'a> {
    pub(super) async fn finalize_tool_call_result(
        &mut self,
        tc: &ToolCall,
        result: &ToolResult,
        args_summary: &str,
        args_hash: &str,
        now_epoch_secs: u64,
        decision_reasoning: Option<&str>,
    ) -> Result<ToolCallDisposition> {
        self.engine
            .persist_tool_selection_causal_trace(
                &self.tid,
                self.current_task_snapshot
                    .as_ref()
                    .and_then(|task| task.goal_run_id.as_deref()),
                self.task_id,
                tc,
                decision_reasoning,
                result,
                &self.trace_collector,
                &self.config,
                &self.provider_config,
            )
            .await;
        self.engine
            .record_provenance_event(
                "tool_call",
                "agent executed tool call",
                serde_json::json!({
                    "tool": tc.function.name.as_str(),
                    "arguments": tc.function.arguments.as_str(),
                    "is_error": result.is_error,
                }),
                self.current_task_snapshot
                    .as_ref()
                    .and_then(|task| task.goal_run_id.as_deref()),
                self.task_id,
                Some(self.tid.as_str()),
                None,
                None,
            )
            .await;

        self.engine
            .update_counter_who_on_tool_result(
                &self.tid,
                &tc.function.name,
                args_summary,
                !result.is_error,
            )
            .await;

        if result.is_error {
            let scope_hint = self
                .current_task_snapshot
                .as_ref()
                .map(|task| task.title.as_str())
                .or_else(|| {
                    self.current_task_snapshot
                        .as_ref()
                        .and_then(|task| task.goal_run_title.as_deref())
                });
            if let Err(error) = self
                .engine
                .record_negative_knowledge_from_tool_failure(
                    scope_hint,
                    &tc.function.name,
                    args_summary,
                    &result.content,
                )
                .await
            {
                tracing::warn!(
                    thread_id = %self.tid,
                    tool_name = %tc.function.name,
                    error = %error,
                    "failed to record immediate negative knowledge from tool failure"
                );
            }
        }

        let tool_result_content = result.content.clone();
        let tool_result_name = result.name.clone();
        let tool_result_id = result.tool_call_id.clone();
        let tool_status = if result.is_error { "error" } else { "done" };

        let _ = self.engine.event_tx.send(AgentEvent::ToolResult {
            thread_id: self.tid.clone(),
            call_id: tool_result_id.clone(),
            name: tool_result_name.clone(),
            content: tool_result_content.clone(),
            is_error: result.is_error,
            weles_review: result.weles_review.clone(),
        });

        {
            let mut threads = self.engine.threads.write().await;
            if let Some(thread) = threads.get_mut(&self.tid) {
                self.tool_side_effect_committed = true;
                thread.messages.push(AgentMessage {
                    id: generate_message_id(),
                    role: MessageRole::Tool,
                    content: tool_result_content,
                    tool_calls: None,
                    tool_call_id: Some(tool_result_id),
                    tool_name: Some(tool_result_name),
                    tool_arguments: Some(tc.function.arguments.clone()),
                    tool_status: Some(tool_status.to_string()),
                    weles_review: result.weles_review.clone(),
                    input_tokens: 0,
                    output_tokens: 0,
                    provider: None,
                    model: None,
                    api_transport: None,
                    response_id: None,
                    reasoning: None,
                    message_kind: AgentMessageKind::Normal,
                    compaction_strategy: None,
                    compaction_payload: None,
                    timestamp: now_millis(),
                });
            }
        }

        let current_tokens = {
            let threads = self.engine.threads.read().await;
            threads
                .get(&self.tid)
                .map(|thread| estimate_message_tokens(&thread.messages))
                .unwrap_or(0) as u32
        };
        if let Some(task) = self.current_task_snapshot.as_ref() {
            self.engine
                .record_subagent_tool_result(
                    task,
                    &self.tid,
                    &tc.function.name,
                    result.is_error,
                    current_tokens,
                )
                .await;
            self.engine.persist_subagent_runtime_metrics(&task.id).await;
        }

        let is_progress = !result.is_error && result.content.len() > 50;
        self.engine
            .record_awareness_outcome(
                &self.tid,
                "thread",
                &tc.function.name,
                args_hash,
                !result.is_error,
                is_progress,
            )
            .await;
        self.engine
            .check_awareness_mode_shift(&self.tid, &self.tid)
            .await;

        if let Some(task_id) = self.task_id {
            let task_snapshot = {
                let tasks = self.engine.tasks.lock().await;
                tasks.iter().find(|task| task.id == task_id).cloned()
            };
            if let Some(task_snapshot) = task_snapshot {
                let scope = policy_scope_for_task(&self.tid, &task_snapshot);
                if result.is_error
                    && self
                        .engine
                        .is_retry_guard_active(&scope, args_hash, now_epoch_secs)
                        .await
                {
                    if let super::orchestrator_policy::PolicyLoopAction::AbortRetry = self
                        .engine
                        .enforce_orchestrator_retry_guard(
                            &self.tid,
                            Some(task_id),
                            &scope,
                            args_hash,
                            now_epoch_secs,
                        )
                        .await?
                    {
                        self.policy_aborted_retry = true;
                        return Ok(ToolCallDisposition::BreakLoop);
                    }
                }

                if let Some(action) = apply_post_tool_policy_checkpoint(
                    self.engine,
                    &self.tid,
                    task_id,
                    &task_snapshot,
                    args_hash,
                    self.recent_policy_tool_outcomes.make_contiguous(),
                    now_epoch_secs,
                )
                .await?
                {
                    match action {
                        super::orchestrator_policy::PolicyLoopAction::Continue => {}
                        super::orchestrator_policy::PolicyLoopAction::RestartLoop => {
                            return Ok(ToolCallDisposition::RestartLoop);
                        }
                        super::orchestrator_policy::PolicyLoopAction::InterruptForApproval => {
                            self.interrupted_for_approval = true;
                            return Ok(ToolCallDisposition::BreakLoop);
                        }
                        super::orchestrator_policy::PolicyLoopAction::AbortRetry => {
                            self.policy_aborted_retry = true;
                            return Ok(ToolCallDisposition::BreakLoop);
                        }
                    }
                }
            }
        }

        if let Some(pending_approval) = result.pending_approval.as_ref() {
            let _ = self
                .engine
                .record_operator_approval_requested(pending_approval)
                .await;
            self.engine
                .record_provenance_event(
                    "approval_requested",
                    "tool execution requested operator approval",
                    serde_json::json!({
                        "approval_id": pending_approval.approval_id,
                        "command": pending_approval.command,
                        "risk_level": pending_approval.risk_level,
                        "blast_radius": pending_approval.blast_radius,
                    }),
                    self.current_task_snapshot
                        .as_ref()
                        .and_then(|task| task.goal_run_id.as_deref()),
                    self.task_id,
                    Some(self.tid.as_str()),
                    Some(pending_approval.approval_id.as_str()),
                    None,
                )
                .await;
            self.interrupted_for_approval = true;
            if let Some(task_id) = self.task_id {
                self.engine
                    .mark_task_awaiting_approval(task_id, &self.tid, pending_approval)
                    .await;
            }
            return Ok(ToolCallDisposition::BreakLoop);
        }

        if self.stream_cancel_token.is_cancelled() {
            self.was_cancelled = true;
            return Ok(ToolCallDisposition::BreakLoop);
        }

        if !result.is_error && tc.function.name == "handoff_thread_agent" {
            let next_agent_name = self
                .engine
                .active_agent_id_for_thread(&self.tid)
                .await
                .map(|agent_id| canonical_agent_name(&agent_id).to_string())
                .unwrap_or_else(|| self.runtime_agent_name.clone());
            if let Some(llm_user_content) = build_handoff_restart_user_message(
                &self.runtime_agent_name,
                &next_agent_name,
                self.stored_user_content,
                &tc.function.arguments,
            ) {
                self.handoff_restart = Some(HandoffRestartRequest { llm_user_content });
            }
            let _ = self.engine.event_tx.send(AgentEvent::Done {
                thread_id: self.tid.clone(),
                input_tokens: 0,
                output_tokens: 0,
                cost: None,
                provider: None,
                model: None,
                tps: None,
                generation_ms: None,
                reasoning: None,
            });
            return Ok(ToolCallDisposition::BreakLoop);
        }

        Ok(ToolCallDisposition::ContinueTools)
    }
}
