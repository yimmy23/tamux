use anyhow::Result;

use crate::agent::metacognitive::{escalation, replanning};
use crate::agent::{
    generate_message_id, make_task_log_entry, now_millis, AgentEngine, AgentEvent, AgentMessage,
    MessageRole, TaskLogLevel, TaskStatus,
};

use super::*;

fn build_strategy_refresh_prompt(
    trigger: &PolicyTriggerContext,
    step_title: &str,
    task_retry_count: u32,
    tool_success_rate: f64,
    strategy_hint: Option<&str>,
) -> String {
    let decision = replanning::select_replan_strategy(&replanning::ReplanContext {
        current_step_index: 0,
        step_title: step_title.to_string(),
        stuck_reason: infer_stuck_reason(trigger),
        attempt_count: task_retry_count,
        error_rate: if tool_success_rate >= 1.0 {
            0.0
        } else {
            1.0 - tool_success_rate
        },
        tool_success_rate,
        context_utilization_pct: 0,
        has_checkpoint: false,
        recent_tool_names: Vec::new(),
    });
    let mut prompt = replanning::build_replan_prompt(&decision, step_title);
    if let Some(strategy_hint) = strategy_hint
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        prompt.push_str("\n\nPolicy hint: ");
        prompt.push_str(strategy_hint);
    }
    prompt
}

fn retry_guard_matches_runtime_context(
    decision: &PolicyDecision,
    context: &PolicyEvaluationContext,
) -> bool {
    decision
        .retry_guard
        .as_deref()
        .zip(context.current_retry_guard.as_deref())
        .is_some_and(|(decision_guard, current_guard)| decision_guard == current_guard)
}

impl AgentEngine {
    async fn mark_policy_halted_retry_failure(
        &self,
        thread_id: &str,
        task_id: Option<&str>,
        detail: Option<String>,
    ) {
        if let Some(task_id) = task_id {
            let updated = {
                let mut tasks = self.tasks.lock().await;
                let Some(task) = tasks.iter_mut().find(|task| task.id == task_id) else {
                    return;
                };
                task.retry_count = task.max_retries;
                task.status = TaskStatus::Failed;
                task.completed_at = Some(now_millis());
                task.blocked_reason = Some("policy halted repeated retry".to_string());
                task.last_error = Some("policy halted repeated retry".to_string());
                task.logs.push(make_task_log_entry(
                    task.retry_count,
                    TaskLogLevel::Warn,
                    "policy",
                    "policy halted repeated retry",
                    detail,
                ));
                task.clone()
            };
            self.persist_tasks().await;
            self.emit_task_update(&updated, Some("Policy halted repeated retry".into()));
            if let Some(goal_run_id) = updated.goal_run_id.as_deref() {
                self.sync_goal_run_with_task(goal_run_id, &updated).await;
            }
        }
        self.append_system_message(
            thread_id,
            "Policy halted a repeated retry for the same failing approach.".to_string(),
        )
        .await;
    }

    async fn append_system_message(&self, thread_id: &str, content: String) {
        {
            let mut threads = self.threads.write().await;
            if let Some(thread) = threads.get_mut(thread_id) {
                thread.messages.push(AgentMessage {
                    id: generate_message_id(),
                    role: MessageRole::System,
                    content,
                    tool_calls: None,
                    tool_call_id: None,
                    tool_name: None,
                    tool_arguments: None,
                    tool_status: None,
                    weles_review: None,
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
                    message_kind: crate::agent::types::AgentMessageKind::Normal,
                    compaction_strategy: None,
                    compaction_payload: None,
                    offloaded_payload_id: None,
                    structural_refs: Vec::new(),
                    pinned_for_compaction: false,
                    timestamp: now_millis(),
                });
                thread.updated_at = now_millis();
            }
        }
        self.persist_thread_by_id(thread_id).await;
    }

    async fn apply_policy_pivot(
        &self,
        thread_id: &str,
        task_id: Option<&str>,
        goal_run_id: Option<&str>,
        trigger: &PolicyTriggerContext,
        decision: &PolicyDecision,
    ) -> Result<PolicyLoopAction> {
        let step_title = if let Some(goal_run_id) = goal_run_id {
            self.get_goal_run(goal_run_id)
                .await
                .and_then(|goal_run| goal_run.current_step_title)
                .unwrap_or_else(|| "current step".to_string())
        } else {
            "current step".to_string()
        };
        let task_retry_count = if let Some(task_id) = task_id {
            let tasks = self.tasks.lock().await;
            tasks
                .iter()
                .find(|task| task.id == task_id)
                .map(|task| task.retry_count)
                .unwrap_or(0)
        } else {
            0
        };
        let tool_success_rate = {
            let monitor = self.awareness.read().await;
            monitor
                .get_window(thread_id)
                .map(|window| window.short_term_success_rate)
                .unwrap_or(0.8)
        };
        let prompt = build_strategy_refresh_prompt(
            trigger,
            &step_title,
            task_retry_count,
            tool_success_rate,
            decision.strategy_hint.as_deref(),
        );
        self.append_system_message(thread_id, prompt).await;
        self.emit_workflow_notice(
            thread_id,
            "strategy-refresh",
            "Policy pivot injected a strategy refresh before retrying.",
            decision.strategy_hint.clone(),
        );
        Ok(PolicyLoopAction::RestartLoop)
    }

    async fn apply_policy_escalation(
        &self,
        thread_id: &str,
        task_id: Option<&str>,
        decision: &PolicyDecision,
        now_epoch_secs: u64,
    ) -> Result<PolicyLoopAction> {
        if self
            .mark_task_approval_rule_used("orchestrator_policy_escalation")
            .await
        {
            self.emit_workflow_notice(
                thread_id,
                "policy-escalation-always-approved",
                "Applied a saved always-approve rule for policy escalation and continued automatically.",
                Some(decision.reason.clone()),
            );
            return Ok(PolicyLoopAction::Continue);
        }

        if self.has_policy_escalation_session_grant(thread_id).await {
            self.emit_workflow_notice(
                thread_id,
                "policy-escalation-session-grant",
                "Reused session approval for policy escalation and continued automatically.",
                Some(decision.reason.clone()),
            );
            return Ok(PolicyLoopAction::Continue);
        }

        let task_id = match task_id {
            Some(task_id) => task_id,
            None => return Ok(PolicyLoopAction::Continue),
        };
        let attempts = {
            let tasks = self.tasks.lock().await;
            tasks
                .iter()
                .find(|task| task.id == task_id)
                .map(|task| task.retry_count)
                .unwrap_or(0)
        };
        let from_level = escalation::EscalationLevel::SelfCorrection;
        let to_level = escalation::EscalationLevel::User;
        let audit = escalation::escalation_audit_data(
            &from_level,
            &to_level,
            &decision.reason,
            attempts,
            Some(thread_id),
            &[],
            now_epoch_secs.saturating_mul(1000),
        );
        let pending_approval = crate::agent::types::ToolPendingApproval {
            approval_id: format!("policy-escalation-{thread_id}-{now_epoch_secs}"),
            execution_id: format!("policy-escalation-exec-{thread_id}-{now_epoch_secs}"),
            command: "orchestrator_policy_escalation".to_string(),
            rationale: decision.reason.clone(),
            risk_level: "medium".to_string(),
            blast_radius: "thread".to_string(),
            reasons: vec![decision.reason.clone()],
            session_id: None,
        };
        self.mark_task_awaiting_approval(task_id, thread_id, &pending_approval)
            .await;
        let goal_run_id = {
            let tasks = self.tasks.lock().await;
            tasks
                .iter()
                .find(|task| task.id == task_id)
                .and_then(|task| task.goal_run_id.clone())
        };
        if let Some(goal_run_id) = goal_run_id {
            let task = {
                let tasks = self.tasks.lock().await;
                tasks.iter().find(|task| task.id == task_id).cloned()
            };
            if let Some(task) = task.as_ref() {
                self.sync_goal_run_with_task(&goal_run_id, task).await;
            }
        }
        self.append_system_message(
            thread_id,
            format!(
                "Policy escalation requested operator guidance: {}",
                decision.reason,
            ),
        )
        .await;
        let _ = self.event_tx.send(AgentEvent::AuditAction {
            id: audit.audit_id.clone(),
            timestamp: audit.timestamp,
            action_type: "policy_escalation".to_string(),
            summary: audit.summary.clone(),
            explanation: Some(audit.reason.clone()),
            confidence: None,
            confidence_band: None,
            causal_trace_id: None,
            thread_id: Some(thread_id.to_string()),
        });
        let _ = self.event_tx.send(AgentEvent::EscalationUpdate {
            thread_id: thread_id.to_string(),
            from_level: audit.from_label,
            to_level: audit.to_label,
            reason: audit.reason,
            attempts: audit.attempts,
            audit_id: Some(audit.audit_id),
        });
        Ok(PolicyLoopAction::InterruptForApproval)
    }

    pub(crate) async fn apply_orchestrator_policy_decision(
        &self,
        thread_id: &str,
        task_id: Option<&str>,
        goal_run_id: Option<&str>,
        trigger: &PolicyTriggerContext,
        decision: &PolicyDecision,
        now_epoch_secs: u64,
    ) -> Result<PolicyLoopAction> {
        match decision.action {
            PolicyAction::Continue => Ok(PolicyLoopAction::Continue),
            PolicyAction::Pivot => {
                self.apply_policy_pivot(thread_id, task_id, goal_run_id, trigger, decision)
                    .await
            }
            PolicyAction::Escalate => {
                self.apply_policy_escalation(thread_id, task_id, decision, now_epoch_secs)
                    .await
            }
            PolicyAction::HaltRetries => {
                let detail = decision.retry_guard.as_ref().map(|retry_guard| {
                    format!("approach_hash={retry_guard}; reason={}", decision.reason)
                });
                self.mark_policy_halted_retry_failure(thread_id, task_id, detail)
                    .await;
                Ok(PolicyLoopAction::AbortRetry)
            }
        }
    }

    pub(crate) async fn enforce_orchestrator_retry_guard(
        &self,
        thread_id: &str,
        task_id: Option<&str>,
        scope: &PolicyDecisionScope,
        approach_hash: &str,
        now_epoch_secs: u64,
    ) -> Result<PolicyLoopAction> {
        if !self
            .is_retry_guard_active(scope, approach_hash, now_epoch_secs)
            .await
        {
            return Ok(PolicyLoopAction::Continue);
        }
        self.mark_policy_halted_retry_failure(
            thread_id,
            task_id,
            Some(format!("approach_hash={approach_hash}")),
        )
        .await;
        Ok(PolicyLoopAction::AbortRetry)
    }

    pub(crate) async fn evaluate_orchestrator_policy_turn(
        &self,
        scope: &PolicyDecisionScope,
        context: PolicyEvaluationContext,
        now_epoch_secs: u64,
    ) -> Result<SelectedPolicyDecision> {
        let recent = self.latest_policy_decision(scope, now_epoch_secs).await;
        if let Some(recent) = recent.as_ref() {
            if recent.decision.action != PolicyAction::Continue
                && recent.decision.retry_guard.is_some()
                && retry_guard_matches_runtime_context(&recent.decision, &context)
            {
                return Ok(SelectedPolicyDecision {
                    source: PolicyDecisionSource::ReusedRecent,
                    decision: recent.decision.clone(),
                });
            }
        }
        let mut context = context;
        context.recent_decision_summary = recent.as_ref().map(summarize_recent_policy_decision);
        let prompt = build_policy_eval_prompt(&context);
        let evaluated = normalize_policy_eval_decision(Some(runtime_owns_policy_retry_guard(
            self.request_orchestrator_policy_decision(&prompt)
                .await?
                .unwrap_or_else(|| {
                    continue_policy_decision(
                        "Policy evaluation unavailable; continuing current execution.",
                    )
                }),
            context.current_retry_guard.as_deref(),
        )));
        let selection = if evaluated.retry_guard.is_none()
            || retry_guard_matches_runtime_context(&evaluated, &context)
        {
            select_orchestrator_policy_decision(
                recent.as_ref(),
                &context.trigger,
                evaluated.clone(),
            )
        } else {
            SelectedPolicyDecision {
                source: PolicyDecisionSource::FreshEvaluation,
                decision: evaluated.clone(),
            }
        };
        if selection.source == PolicyDecisionSource::ReusedRecent {
            return Ok(selection);
        }
        self.record_policy_decision(scope, evaluated.clone(), now_epoch_secs)
            .await;
        Ok(SelectedPolicyDecision {
            source: PolicyDecisionSource::FreshEvaluation,
            decision: evaluated,
        })
    }
}
