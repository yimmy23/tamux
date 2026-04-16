use super::*;
use crate::agent::metacognitive::introspector::{
    introspect, IntrospectionInput, RecentToolOutcome,
};
use crate::agent::metacognitive::pattern_regulator::{regulate, InterventionAction};
use crate::agent::metacognitive::self_assessment::SelfAssessor;

pub(super) fn inter_tool_call_delay(
    index: usize,
    configured_delay_ms: u64,
) -> Option<std::time::Duration> {
    (index > 0 && configured_delay_ms > 0)
        .then(|| std::time::Duration::from_millis(configured_delay_ms))
}

fn skill_gate_exempt_tool(name: &str) -> bool {
    matches!(
        name,
        "discover_skills" | "list_skills" | "read_skill" | "justify_skill_skip"
    )
}

impl<'a> SendMessageRunner<'a> {
    async fn handle_metacognitive_intervention(
        &mut self,
        tc: &ToolCall,
    ) -> Option<LoopDisposition> {
        let decision_reasoning = {
            let threads = self.engine.threads.read().await;
            threads
                .get(&self.tid)
                .and_then(|thread| {
                    thread
                        .messages
                        .iter()
                        .rev()
                        .find(|message| message.role == MessageRole::Assistant)
                })
                .and_then(|message| message.reasoning.clone())
                .or_else(|| {
                    threads
                        .get(&self.tid)
                        .and_then(|thread| {
                            thread
                                .messages
                                .iter()
                                .rev()
                                .find(|message| message.role == MessageRole::Assistant)
                        })
                        .map(|message| message.content.clone())
                })
        };
        let current_tool_signature = normalized_tool_signature(tc);
        let predicted_repeat_count = if self
            .previous_tool_signature
            .as_deref()
            .is_some_and(|value| value == current_tool_signature.as_str())
        {
            self.consecutive_same_tool_calls.saturating_add(1)
        } else {
            1
        };
        let recent_tool_outcomes = self
            .recent_policy_tool_outcomes
            .iter()
            .cloned()
            .map(|summary| RecentToolOutcome {
                tool_name: summary.tool_name,
                outcome: summary.outcome,
                summary: summary.summary,
            })
            .collect::<Vec<_>>();
        let task_retry_count = self
            .current_task_snapshot
            .as_ref()
            .map(|task| task.retry_count)
            .unwrap_or(0);
        let input = IntrospectionInput {
            proposed_tool_name: tc.function.name.clone(),
            proposed_tool_arguments: tc.function.arguments.clone(),
            normalized_tool_signature: current_tool_signature.clone(),
            predicted_repeat_count,
            recent_tool_outcomes,
            task_retry_count,
            decision_reasoning,
        };
        let self_model = self.engine.meta_cognitive_self_model.read().await.clone();
        let outcome = introspect(&self_model, &input);
        if outcome.signals.is_empty() {
            return None;
        }

        let decision = regulate(&outcome, tc);
        self.engine.emit_workflow_notice(
            &self.tid,
            "metacognitive-check",
            decision.summary.clone(),
            serde_json::to_string(&outcome).ok(),
        );
        if let Some(adjustment) = decision.confidence_adjustment {
            let recent_success_rate = if input.recent_tool_outcomes.is_empty() {
                1.0
            } else {
                input
                    .recent_tool_outcomes
                    .iter()
                    .filter(|outcome| outcome.outcome.eq_ignore_ascii_case("success"))
                    .count() as f64
                    / input.recent_tool_outcomes.len() as f64
            };
            let assessment = SelfAssessor::default().assess(
                &crate::agent::metacognitive::self_assessment::AssessmentInput {
                    progress: crate::agent::metacognitive::self_assessment::ProgressMetrics {
                        goal_distance_pct: 0.0,
                        steps_completed: 0,
                        steps_total: 0,
                        estimated_remaining: 0,
                        momentum: if recent_success_rate >= 0.5 {
                            0.1
                        } else {
                            -0.2
                        },
                    },
                    efficiency: crate::agent::metacognitive::self_assessment::EfficiencyMetrics {
                        token_efficiency: 0.5,
                        tool_success_rate: recent_success_rate,
                        time_efficiency: 0.0,
                        tokens_consumed: 0,
                        elapsed_secs: input.task_retry_count as u64,
                    },
                    quality: crate::agent::metacognitive::self_assessment::QualityMetrics {
                        error_rate: 1.0 - recent_success_rate,
                        revision_count: input.task_retry_count,
                        user_feedback_score: None,
                    },
                },
            );
            let predicted_band = metacognitive_calibrated_band(&assessment, 0.0);
            self.engine
                .apply_meta_cognitive_calibration_adjustment(adjustment, predicted_band)
                .await;
        }
        for signal in &outcome.signals {
            self.engine
                .reinforce_meta_cognitive_bias_occurrence(&signal.bias_name)
                .await;
        }

        match decision.action {
            InterventionAction::Allow => None,
            InterventionAction::Warn => {
                if let Some(system_message) = decision.system_message.as_deref() {
                    self.engine
                        .append_metacognitive_system_message(&self.tid, system_message)
                        .await;
                }
                self.engine.persist_thread_by_id(&self.tid).await;
                Some(LoopDisposition::Continue)
            }
            InterventionAction::Block => {
                if let Some(system_message) = decision.system_message.as_deref() {
                    self.engine
                        .append_metacognitive_system_message(&self.tid, system_message)
                        .await;
                }
                let denied_content = format!(
                    "Tool call blocked by meta-cognitive regulator before execution. {}",
                    decision.summary
                );
                self.persist_denied_tool_result(tc, denied_content).await;
                self.engine.persist_thread_by_id(&self.tid).await;
                Some(LoopDisposition::Continue)
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) async fn handle_tool_calls_chunk(
        &mut self,
        _llm_started_at: Instant,
        _first_token_at: Option<Instant>,
        effective_transport_for_turn: ApiTransport,
        accumulated_content: String,
        accumulated_reasoning: String,
        tool_calls: Vec<ToolCall>,
        content: Option<String>,
        reasoning: Option<String>,
        input_tokens: Option<u64>,
        output_tokens: Option<u64>,
        response_id: Option<String>,
        upstream_message: Option<CompletionUpstreamMessage>,
        provider_final_result: Option<CompletionProviderFinalResult>,
        upstream_thread_id: Option<String>,
    ) -> Result<LoopDisposition> {
        self.emit_tool_ack_if_needed(&tool_calls, &accumulated_content);

        let msg_content = content.unwrap_or(accumulated_content);
        let msg_reasoning = reasoning.or(if accumulated_reasoning.is_empty() {
            None
        } else {
            Some(accumulated_reasoning)
        });
        let decision_reasoning = msg_reasoning
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .map(ToOwned::to_owned)
            .or_else(|| (!msg_content.trim().is_empty()).then_some(msg_content.clone()));

        self.persist_assistant_tool_calls_message(
            &tool_calls,
            msg_content,
            msg_reasoning,
            input_tokens,
            output_tokens,
            response_id,
            upstream_message,
            provider_final_result.clone(),
            effective_transport_for_turn,
        )
        .await;
        self.engine
            .accumulate_goal_run_cost(
                &self.tid,
                input_tokens.unwrap_or(0),
                output_tokens.unwrap_or(0),
                &self.config.provider,
                &self.provider_config.model,
            )
            .await;
        self.engine.persist_thread_by_id(&self.tid).await;
        Box::pin(
            self.engine
                .maybe_auto_send_gateway_thread_response(&self.tid),
        )
        .await;
        self.engine
            .update_thread_upstream_state(
                &self.tid,
                &self.config.provider,
                &self.provider_config.model,
                effective_transport_for_turn,
                Some(self.provider_config.assistant_id.as_str()),
                upstream_thread_id,
            )
            .await;
        self.provider_final_result = provider_final_result;

        for (index, tc) in tool_calls.iter().enumerate() {
            if self.stream_cancel_token.is_cancelled() {
                self.was_cancelled = true;
                break;
            }

            if let Some(delay) = inter_tool_call_delay(index, self.config.tool_call_delay_ms) {
                tokio::time::sleep(delay).await;
            }

            let args_summary = tool_args_summary(&tc.function.arguments);
            let args_hash = super::episodic::counter_who::compute_approach_hash(
                &tc.function.name,
                &args_summary,
            );
            let now_epoch_secs = current_epoch_secs();

            if let Some(task) = self.current_task_snapshot.as_ref() {
                let scope = policy_scope_for_task(&self.tid, task);
                if let super::orchestrator_policy::PolicyLoopAction::AbortRetry = self
                    .engine
                    .enforce_orchestrator_retry_guard(
                        &self.tid,
                        Some(task.id.as_str()),
                        &scope,
                        &args_hash,
                        now_epoch_secs,
                    )
                    .await?
                {
                    self.policy_aborted_retry = true;
                    return Ok(LoopDisposition::Break);
                }
            }

            let _ = self.engine.event_tx.send(AgentEvent::ToolCall {
                thread_id: self.tid.clone(),
                call_id: tc.id.clone(),
                name: tc.function.name.clone(),
                arguments: tc.function.arguments.clone(),
                weles_review: tc.weles_review.clone(),
            });

            if self.handle_tool_filter_denial(tc).await {
                continue;
            }
            if self.handle_skill_gate_denial(tc).await {
                continue;
            }
            if let Some(disposition) = self.handle_metacognitive_intervention(tc).await {
                return Ok(disposition);
            }

            let result = self.execute_tool_call(tc).await;
            self.record_tool_trace(tc, &result);

            if tc.function.name == "update_memory" && !result.is_error {
                self.engine.refresh_memory_cache().await;
            }

            if let Some((previous_tool_name, previous_was_error)) =
                self.previous_tool_outcome.as_ref()
            {
                if let Err(error) = self
                    .engine
                    .record_tool_hesitation(
                        previous_tool_name,
                        tc.function.name.as_str(),
                        *previous_was_error,
                        result.is_error,
                    )
                    .await
                {
                    tracing::warn!(error = %error, "failed to record implicit tool fallback feedback");
                }
            }
            self.previous_tool_outcome = Some((tc.function.name.clone(), result.is_error));
            if result.is_error {
                self.last_tool_error = Some((tc.function.name.clone(), result.content.clone()));
            } else {
                self.last_tool_error = None;
            }

            self.recent_policy_tool_outcomes
                .push_back(summarize_tool_result_for_policy(&tc.function.name, &result));
            while self.recent_policy_tool_outcomes.len() > POLICY_TOOL_OUTCOME_HISTORY_LIMIT {
                self.recent_policy_tool_outcomes.pop_front();
            }

            if !result.is_error {
                self.engine
                    .capture_tool_work_context(
                        &self.tid,
                        self.task_id,
                        tc.function.name.as_str(),
                        tc.function.arguments.as_str(),
                    )
                    .await;
            }

            match self
                .finalize_tool_call_result(
                    tc,
                    &result,
                    &args_summary,
                    &args_hash,
                    now_epoch_secs,
                    decision_reasoning.as_deref(),
                )
                .await?
            {
                ToolCallDisposition::ContinueTools => {}
                ToolCallDisposition::RestartLoop => return Ok(LoopDisposition::Continue),
                ToolCallDisposition::BreakLoop => return Ok(LoopDisposition::Break),
            }
        }

        if self.was_cancelled {
            return Ok(LoopDisposition::Break);
        }

        if self.should_break_for_termination().await {
            return Ok(LoopDisposition::Break);
        }

        Ok(LoopDisposition::Continue)
    }

    fn emit_tool_ack_if_needed(&mut self, tool_calls: &[ToolCall], accumulated_content: &str) {
        if self.tool_ack_emitted || !accumulated_content.trim().is_empty() {
            return;
        }
        self.tool_ack_emitted = true;
        let tool_names: Vec<&str> = tool_calls
            .iter()
            .map(|tool_call| tool_call.function.name.as_str())
            .collect();
        let ack = match tool_names.as_slice() {
            [single] => format!("On it - using {single}..."),
            names if names.len() <= 3 => {
                format!("Working on it - using {}...", names.join(", "))
            }
            names => format!("Working on it - running {} tools...", names.len()),
        };
        let _ = self.engine.event_tx.send(AgentEvent::WorkflowNotice {
            thread_id: self.tid.clone(),
            kind: "tool-ack".to_string(),
            message: ack,
            details: None,
        });
    }

    async fn persist_assistant_tool_calls_message(
        &mut self,
        tool_calls: &[ToolCall],
        msg_content: String,
        msg_reasoning: Option<String>,
        input_tokens: Option<u64>,
        output_tokens: Option<u64>,
        response_id: Option<String>,
        upstream_message: Option<CompletionUpstreamMessage>,
        provider_final_result: Option<CompletionProviderFinalResult>,
        effective_transport_for_turn: ApiTransport,
    ) {
        let mut threads = self.engine.threads.write().await;
        if let Some(thread) = threads.get_mut(&self.tid) {
            let author_agent_id = current_agent_scope_id();
            let author_agent_name = canonical_agent_name(&author_agent_id).to_string();
            self.assistant_output_visible = true;
            thread.messages.push(AgentMessage {
                id: generate_message_id(),
                role: MessageRole::Assistant,
                content: msg_content,
                tool_calls: Some(tool_calls.to_vec()),
                tool_call_id: None,
                tool_name: None,
                tool_arguments: None,
                tool_status: None,
                weles_review: None,
                input_tokens: input_tokens.unwrap_or(0),
                output_tokens: output_tokens.unwrap_or(0),
                cost: None,
                provider: Some(self.config.provider.clone()),
                model: Some(self.provider_config.model.clone()),
                api_transport: Some(effective_transport_for_turn),
                response_id,
                upstream_message,
                provider_final_result,
                author_agent_id: Some(author_agent_id),
                author_agent_name: Some(author_agent_name),
                reasoning: msg_reasoning,
                message_kind: AgentMessageKind::Normal,
                compaction_strategy: None,
                compaction_payload: None,
                offloaded_payload_id: None,
                structural_refs: Vec::new(),
                pinned_for_compaction: false,
                timestamp: now_millis(),
            });
            thread.total_input_tokens += input_tokens.unwrap_or(0);
            thread.total_output_tokens += output_tokens.unwrap_or(0);
        }
    }

    async fn persist_denied_tool_result(&mut self, tc: &ToolCall, content: String) {
        let _ = self.engine.event_tx.send(AgentEvent::ToolResult {
            thread_id: self.tid.clone(),
            call_id: tc.id.clone(),
            name: tc.function.name.clone(),
            content: content.clone(),
            is_error: true,
            weles_review: tc.weles_review.clone(),
        });
        let mut threads = self.engine.threads.write().await;
        if let Some(thread) = threads.get_mut(&self.tid) {
            self.tool_side_effect_committed = true;
            thread.messages.push(AgentMessage {
                id: generate_message_id(),
                role: MessageRole::Tool,
                content,
                tool_calls: None,
                tool_call_id: Some(tc.id.clone()),
                tool_name: Some(tc.function.name.clone()),
                tool_arguments: Some(tc.function.arguments.clone()),
                tool_status: Some("error".to_string()),
                weles_review: tc.weles_review.clone(),
                input_tokens: 0,
                output_tokens: 0,
                cost: None,
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
                offloaded_payload_id: None,
                structural_refs: Vec::new(),
                pinned_for_compaction: false,
                timestamp: now_millis(),
            });
        }
    }

    async fn handle_tool_filter_denial(&mut self, tc: &ToolCall) -> bool {
        let Some(filter) = self.task_tool_filter.as_ref() else {
            return false;
        };
        let Some(reason) = filter.deny_reason(&tc.function.name) else {
            return false;
        };

        let denied_content = format!("Tool call denied: {reason}");
        self.persist_denied_tool_result(tc, denied_content).await;
        true
    }

    async fn handle_skill_gate_denial(&mut self, tc: &ToolCall) -> bool {
        if skill_gate_exempt_tool(&tc.function.name) {
            return false;
        }
        let Some(state) = self
            .engine
            .get_thread_skill_discovery_state(&self.tid)
            .await
        else {
            return false;
        };
        if state.compliant {
            return false;
        }

        if state.is_discovery_pending() {
            self.engine.emit_workflow_notice(
                &self.tid,
                "skill-gate",
                format!(
                    "Skill discovery is still running asynchronously before `{}`; allowing the tool call to proceed for now.",
                    tc.function.name
                ),
                serde_json::to_string(&state).ok(),
            );
            return false;
        }

        if state.mesh_requires_approval {
            let denied_content = format!(
                "Tool call blocked by skill discovery governance. Before `{}` you must obtain approval for `{}`.",
                tc.function.name, state.recommended_action
            );
            self.engine.emit_workflow_notice(
                &self.tid,
                "skill-gate",
                format!(
                    "Skill discovery requires approval before `{}`. Required next step: {}.",
                    tc.function.name, state.recommended_action
                ),
                serde_json::to_string(&state).ok(),
            );
            self.persist_denied_tool_result(tc, denied_content).await;
            return true;
        }

        if state.requires_skill_read_before_progress() {
            self.engine.emit_workflow_notice(
                &self.tid,
                "skill-gate",
                format!(
                    "Skill discovery strongly recommends `{}` before `{}`; allowing the tool call to proceed.",
                    state.recommended_action,
                    tc.function.name
                ),
                serde_json::to_string(&state).ok(),
            );
            return false;
        }

        self.engine.emit_workflow_notice(
            &self.tid,
            "skill-gate",
            if state.has_advisory_skill_read() {
                format!(
                    "Weak skill discovery recommends `{}` before `{}`; allowing the tool call to proceed.",
                    state.recommended_action, tc.function.name
                )
            } else {
                format!(
                    "Skill discovery suggests `{}` before `{}`; allowing the tool call to proceed.",
                    state.recommended_action, tc.function.name
                )
            },
            serde_json::to_string(&state).ok(),
        );
        false
    }

    async fn execute_tool_call(&mut self, tc: &ToolCall) -> ToolResult {
        let current_tool_signature = normalized_tool_signature(tc);
        let repeated = self
            .previous_tool_signature
            .as_deref()
            .is_some_and(|value| value == current_tool_signature.as_str());
        let result = if repeated {
            self.consecutive_same_tool_calls = self.consecutive_same_tool_calls.saturating_add(1);
            if self.consecutive_same_tool_calls >= 3 {
                self.engine.emit_workflow_notice(
                    &self.tid,
                    "tool-stall",
                    "Repeated identical tool call suppressed; inspect fresh state or choose a different action.",
                    Some(format!(
                        "tool={} signature={}",
                        tc.function.name, current_tool_signature
                    )),
                );
                ToolResult {
                    tool_call_id: tc.id.clone(),
                    name: tc.function.name.clone(),
                    content: "Repeated identical tool call suppressed because the agent appears stuck. Inspect current state or continue with a different action instead of repeating the same tool input.".to_string(),
                    is_error: true,
                    weles_review: tc.weles_review.clone(),
                    pending_approval: None,
                }
            } else {
                Box::pin(execute_tool(
                    tc,
                    self.engine,
                    &self.tid,
                    self.task_id,
                    &self.engine.session_manager,
                    self.preferred_session_id,
                    &self.engine.event_tx,
                    &self.engine.data_dir,
                    &self.engine.http_client,
                    Some(self.stream_cancel_token.clone()),
                ))
                .await
            }
        } else {
            self.consecutive_same_tool_calls = 1;
            Box::pin(execute_tool(
                tc,
                self.engine,
                &self.tid,
                self.task_id,
                &self.engine.session_manager,
                self.preferred_session_id,
                &self.engine.event_tx,
                &self.engine.data_dir,
                &self.engine.http_client,
                Some(self.stream_cancel_token.clone()),
            ))
            .await
        };
        self.previous_tool_signature = Some(current_tool_signature);
        result
    }

    fn record_tool_trace(&mut self, tc: &ToolCall, result: &ToolResult) {
        self.termination_tool_calls += 1;
        if result.is_error {
            self.termination_consecutive_errors += 1;
            self.termination_total_errors += 1;
        } else {
            self.termination_consecutive_errors = 0;
            self.termination_tool_successes += 1;
        }
        if self.task_id.is_some() {
            self.trace_collector.record_step(
                &tc.function.name,
                &crate::agent::learning::traces::hash_arguments(&tc.function.arguments),
                !result.is_error,
                0,
                0,
                if result.is_error {
                    Some(result.content.clone())
                } else {
                    None
                },
                now_millis(),
            );
        }
    }

    async fn should_break_for_termination(&mut self) -> bool {
        if let Some(evaluator) = self.task_termination_eval.as_ref() {
            let elapsed = now_millis().saturating_sub(self.loop_started_at) / 1000;
            let metrics = crate::agent::subagent::termination::TerminationMetrics {
                elapsed_secs: elapsed,
                tool_calls_total: self.termination_tool_calls,
                tool_calls_succeeded: self.termination_tool_successes,
                consecutive_errors: self.termination_consecutive_errors,
                total_errors: self.termination_total_errors,
            };
            let (should_stop, reason) = evaluator.should_terminate(&metrics);
            if should_stop {
                tracing::info!(thread_id = %self.tid, reason = ?reason, "sub-agent terminated by condition");
                self.engine.emit_workflow_notice(
                    &self.tid,
                    "termination-triggered",
                    &format!(
                        "Sub-agent terminated: {}",
                        reason.as_deref().unwrap_or("condition met")
                    ),
                    None,
                );
                return true;
            }
        }

        if self.termination_tool_calls.is_multiple_of(5) {
            if let Some(budget) = self.task_context_budget.as_mut() {
                let current_tokens = {
                    let threads = self.engine.threads.read().await;
                    threads
                        .get(&self.tid)
                        .map(|thread| estimate_message_tokens(&thread.messages))
                        .unwrap_or(0) as u32
                };
                budget.set_consumed(current_tokens);
                match budget.check() {
                    crate::agent::subagent::context_budget::BudgetStatus::Exceeded {
                        overflow_action,
                        ..
                    } => match overflow_action {
                        crate::agent::types::ContextOverflowAction::Error => {
                            tracing::warn!(thread_id = %self.tid, "context budget exceeded - stopping");
                            self.engine.emit_workflow_notice(
                                &self.tid,
                                "budget-exceeded",
                                "Context budget exceeded, execution stopped.",
                                None,
                            );
                            return true;
                        }
                        _ => {
                            tracing::info!(thread_id = %self.tid, "context budget exceeded - relying on compaction");
                        }
                    },
                    crate::agent::subagent::context_budget::BudgetStatus::Warning {
                        consumed,
                        max,
                    } => {
                        tracing::debug!(thread_id = %self.tid, consumed, max, "context budget warning");
                    }
                    _ => {}
                }
            }
        }

        false
    }
}

#[cfg(test)]
mod tests {
    use super::inter_tool_call_delay;
    use std::time::Duration;

    #[test]
    fn inter_tool_call_delay_uses_configured_delay_between_consecutive_tools() {
        assert_eq!(inter_tool_call_delay(0, 500), None);
        assert_eq!(
            inter_tool_call_delay(1, 500),
            Some(Duration::from_millis(500))
        );
        assert_eq!(
            inter_tool_call_delay(2, 500),
            Some(Duration::from_millis(500))
        );
        assert_eq!(inter_tool_call_delay(1, 0), None);
    }
}
