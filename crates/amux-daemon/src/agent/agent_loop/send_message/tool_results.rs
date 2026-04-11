use super::*;

const OFFLOAD_SUMMARY_KEY_FINDING_LINES: usize = 3;
const OFFLOAD_SUMMARY_LINE_CHAR_LIMIT: usize = 160;

fn summarize_offloaded_tool_result(
    result: &ToolResult,
    byte_size: usize,
    payload_id: &str,
) -> String {
    let status = if result.is_error { "error" } else { "done" };
    let key_findings = extract_offload_key_findings(&result.content);
    let findings_block = key_findings
        .into_iter()
        .map(|line| format!("  - {line}"))
        .collect::<Vec<_>>()
        .join("\n");

    format!(
        "Tool result offloaded\n- tool: {}\n- status: {}\n- bytes: {}\n- payload_id: {}\n- key findings:\n{}",
        result.name, status, byte_size, payload_id, findings_block
    )
}

fn extract_offload_key_findings(raw_payload: &str) -> Vec<String> {
    let findings = raw_payload
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(|line| {
            let truncated: String = line.chars().take(OFFLOAD_SUMMARY_LINE_CHAR_LIMIT).collect();
            if line.chars().count() > OFFLOAD_SUMMARY_LINE_CHAR_LIMIT {
                format!("{truncated}...")
            } else {
                truncated
            }
        })
        .take(OFFLOAD_SUMMARY_KEY_FINDING_LINES)
        .collect::<Vec<_>>();

    if findings.is_empty() {
        vec!["(no non-empty lines)".to_string()]
    } else {
        findings
    }
}

fn build_handoff_restart_user_message(
    previous_agent_name: &str,
    next_agent_name: &str,
    original_user_message: &str,
    tool_arguments: &str,
) -> Option<String> {
    let args: serde_json::Value = serde_json::from_str(tool_arguments).ok()?;
    let reason = args
        .get("reason")
        .and_then(|value| value.as_str())
        .unwrap_or("");
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PreparedToolResultThreadMessage {
    pub content: String,
    pub offloaded_payload_id: Option<String>,
}

pub(crate) async fn prepare_tool_result_thread_message(
    engine: &AgentEngine,
    thread_id: &str,
    result: &ToolResult,
    created_at: u64,
) -> PreparedToolResultThreadMessage {
    let threshold_bytes = {
        engine
            .config
            .read()
            .await
            .offload_tool_result_threshold_bytes
    };
    let raw_payload = result.content.clone();
    let byte_size = raw_payload.as_bytes().len();

    if thread_id.trim().is_empty()
        || threshold_bytes == 0
        || byte_size <= threshold_bytes
        || result.name == "read_offloaded_payload"
    {
        return PreparedToolResultThreadMessage {
            content: raw_payload,
            offloaded_payload_id: None,
        };
    }

    let payload_id = uuid::Uuid::new_v4().to_string();
    let summary = summarize_offloaded_tool_result(result, byte_size, &payload_id);
    let payload_path = engine
        .history
        .offloaded_payload_path(thread_id, &payload_id);
    let persist_result: Result<()> = async {
        let parent = payload_path
            .parent()
            .context("offloaded payload path missing parent directory")?;
        tokio::fs::create_dir_all(parent)
            .await
            .with_context(|| format!("create offloaded payload directory {}", parent.display()))?;
        tokio::fs::write(&payload_path, raw_payload.as_bytes())
            .await
            .with_context(|| format!("write offloaded payload file {}", payload_path.display()))?;
        if let Err(error) = engine
            .history
            .upsert_offloaded_payload_metadata(
                &payload_id,
                thread_id,
                &result.name,
                Some(result.tool_call_id.as_str()),
                "text/plain",
                byte_size as u64,
                &summary,
                created_at,
            )
            .await
        {
            let _ = tokio::fs::remove_file(&payload_path).await;
            return Err(error).context("persist offloaded payload metadata");
        }
        Ok(())
    }
    .await;

    if let Err(error) = persist_result {
        tracing::warn!(
            thread_id = %thread_id,
            tool_name = %result.name,
            %error,
            "failed to offload tool result payload; keeping inline content"
        );
        return PreparedToolResultThreadMessage {
            content: raw_payload,
            offloaded_payload_id: None,
        };
    }

    PreparedToolResultThreadMessage {
        content: summary,
        offloaded_payload_id: Some(payload_id),
    }
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

        let prepared_tool_result =
            prepare_tool_result_thread_message(self.engine, &self.tid, result, now_epoch_secs)
                .await;
        let structural_refs = if result.is_error {
            Vec::new()
        } else {
            self.engine
                .enrich_thread_structural_memory_from_tool_result(
                    &self.tid,
                    &tc.function.name,
                    &tc.function.arguments,
                    Some(result.content.as_str()),
                )
                .await
        };
        let tool_result_content = prepared_tool_result.content.clone();
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
                    upstream_message: None,
                    provider_final_result: None,
                    author_agent_id: None,
                    author_agent_name: None,
                    reasoning: None,
                    message_kind: AgentMessageKind::Normal,
                    compaction_strategy: None,
                    compaction_payload: None,
                    offloaded_payload_id: prepared_tool_result.offloaded_payload_id.clone(),
                    structural_refs,
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
                upstream_message: None,
                provider_final_result: None,
            });
            return Ok(ToolCallDisposition::BreakLoop);
        }

        Ok(ToolCallDisposition::ContinueTools)
    }
}
