use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use super::metacognitive::self_assessment::Assessment;
use super::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PolicyTriggerInput {
    pub thread_id: String,
    pub goal_run_id: Option<String>,
    pub repeated_approach: bool,
    pub awareness_stuck: bool,
    pub should_pivot: bool,
    pub should_escalate: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PolicySelfAssessmentSummary {
    pub should_pivot: bool,
    pub should_escalate: bool,
}

impl PolicySelfAssessmentSummary {
    pub(crate) fn is_actionable(&self) -> bool {
        self.should_pivot || self.should_escalate
    }
}

impl From<&Assessment> for PolicySelfAssessmentSummary {
    fn from(value: &Assessment) -> Self {
        Self {
            should_pivot: value.should_pivot,
            should_escalate: value.should_escalate,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PolicyTriggerContext {
    pub thread_id: String,
    pub goal_run_id: Option<String>,
    pub repeated_approach: bool,
    pub awareness_stuck: bool,
    pub self_assessment: PolicySelfAssessmentSummary,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PolicyToolOutcomeSummary {
    pub tool_name: String,
    pub outcome: String,
    pub summary: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PolicyEvaluationContext {
    pub trigger: PolicyTriggerContext,
    pub current_retry_guard: Option<String>,
    pub recent_tool_outcomes: Vec<PolicyToolOutcomeSummary>,
    pub awareness_summary: Option<String>,
    pub counter_who_context: Option<String>,
    pub self_assessment_summary: Option<String>,
    pub thread_context: Option<String>,
    pub recent_decision_summary: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum TriggerOutcome {
    NoIntervention,
    EvaluatePolicy(PolicyTriggerContext),
}

pub(crate) fn evaluate_triggers(input: &PolicyTriggerInput) -> TriggerOutcome {
    let self_assessment = PolicySelfAssessmentSummary {
        should_pivot: input.should_pivot,
        should_escalate: input.should_escalate,
    };

    if !input.repeated_approach && !input.awareness_stuck && !self_assessment.is_actionable() {
        return TriggerOutcome::NoIntervention;
    }

    TriggerOutcome::EvaluatePolicy(PolicyTriggerContext {
        thread_id: input.thread_id.clone(),
        goal_run_id: input.goal_run_id.clone(),
        repeated_approach: input.repeated_approach,
        awareness_stuck: input.awareness_stuck,
        self_assessment,
    })
}

pub(crate) fn aggregate_trigger_contexts(
    inputs: &[PolicyTriggerInput],
) -> HashMap<String, PolicyTriggerContext> {
    let mut contexts = HashMap::new();

    for context in inputs
        .iter()
        .filter_map(|input| match evaluate_triggers(input) {
            TriggerOutcome::NoIntervention => None,
            TriggerOutcome::EvaluatePolicy(context) => Some(context),
        })
    {
        contexts
            .entry(context.thread_id.clone())
            .and_modify(|existing: &mut PolicyTriggerContext| {
                existing.goal_run_id = match (&existing.goal_run_id, &context.goal_run_id) {
                    (Some(existing_id), _) => Some(existing_id.clone()),
                    (None, Some(incoming_id)) => Some(incoming_id.clone()),
                    (None, None) => None,
                };
                existing.repeated_approach |= context.repeated_approach;
                existing.awareness_stuck |= context.awareness_stuck;
                existing.self_assessment.should_pivot |= context.self_assessment.should_pivot;
                existing.self_assessment.should_escalate |= context.self_assessment.should_escalate;
            })
            .or_insert(context);
    }

    contexts
}

pub(crate) type RecentPolicyDecisionsByScope = HashMap<PolicyDecisionScope, RecentPolicyDecision>;
pub(crate) type RetryGuardsByScope = HashMap<PolicyDecisionScope, String>;
pub(crate) type ShortLivedRecentPolicyDecisions =
    HashMap<PolicyDecisionScope, RecentPolicyDecision>;
pub(crate) type ShortLivedRetryGuards = HashMap<PolicyDecisionScope, RecentRetryGuard>;

pub(crate) const SHORT_LIVED_POLICY_WINDOW_SECS: u64 = 60;
const POLICY_PROMPT_MAX_TOOL_OUTCOMES: usize = 4;
const POLICY_PROMPT_MAX_FIELD_CHARS: usize = 220;
const POLICY_PROMPT_MAX_TOOL_SUMMARY_CHARS: usize = 160;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct PolicyDecisionScope {
    pub thread_id: String,
    pub goal_run_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum PolicyAction {
    Continue,
    Pivot,
    Escalate,
    HaltRetries,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PolicyLoopAction {
    Continue,
    RestartLoop,
    InterruptForApproval,
    AbortRetry,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PolicyDecisionSource {
    FreshEvaluation,
    ReusedRecent,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SelectedPolicyDecision {
    pub source: PolicyDecisionSource,
    pub decision: PolicyDecision,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PolicyDecision {
    pub action: PolicyAction,
    pub reason: String,
    pub strategy_hint: Option<String>,
    pub retry_guard: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RecentPolicyDecision {
    pub decision: PolicyDecision,
    pub decided_at_epoch_secs: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RecentRetryGuard {
    pub approach_hash: String,
    pub recorded_at_epoch_secs: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PolicyDecisionValidationError {
    MissingReason { action: PolicyAction },
    RetryGuardNotAllowed { action: PolicyAction },
    RetryGuardRequired { action: PolicyAction },
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PolicyDecisionSemanticIdentity {
    action: PolicyAction,
    retry_guard: Option<String>,
    strategy_hint: Option<String>,
}

impl PolicyDecision {
    fn normalized_strategy_hint(&self) -> Option<String> {
        self.strategy_hint
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| value.to_ascii_lowercase())
    }

    fn semantic_identity(&self) -> PolicyDecisionSemanticIdentity {
        let retry_guard = self.retry_guard.clone();

        PolicyDecisionSemanticIdentity {
            action: self.action.clone(),
            strategy_hint: if retry_guard.is_none() && self.action == PolicyAction::Pivot {
                self.normalized_strategy_hint()
            } else {
                None
            },
            retry_guard,
        }
    }
}

pub(crate) fn validate_policy_decision(
    decision: &PolicyDecision,
) -> Result<PolicyDecision, PolicyDecisionValidationError> {
    let reason = decision.reason.trim().to_string();
    let normalize = |value: &Option<String>| {
        value
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    };
    let strategy_hint = normalize(&decision.strategy_hint);
    let retry_guard = normalize(&decision.retry_guard);

    if decision.action != PolicyAction::Continue && reason.is_empty() {
        return Err(PolicyDecisionValidationError::MissingReason {
            action: decision.action.clone(),
        });
    }

    match decision.action {
        PolicyAction::Continue if retry_guard.is_some() => {
            return Err(PolicyDecisionValidationError::RetryGuardNotAllowed {
                action: PolicyAction::Continue,
            });
        }
        PolicyAction::HaltRetries if retry_guard.is_none() => {
            return Err(PolicyDecisionValidationError::RetryGuardRequired {
                action: PolicyAction::HaltRetries,
            });
        }
        _ => {}
    }

    Ok(PolicyDecision {
        action: decision.action.clone(),
        reason,
        strategy_hint,
        retry_guard,
    })
}

fn is_within_active_window(
    recorded_at_epoch_secs: u64,
    now_epoch_secs: u64,
    active_window_secs: u64,
) -> bool {
    now_epoch_secs.saturating_sub(recorded_at_epoch_secs) <= active_window_secs
}

pub(crate) fn record_policy_decision(
    recent_decisions: &mut ShortLivedRecentPolicyDecisions,
    scope: &PolicyDecisionScope,
    decision: PolicyDecision,
    now_epoch_secs: u64,
) {
    recent_decisions.retain(|_, recent| {
        is_within_active_window(
            recent.decided_at_epoch_secs,
            now_epoch_secs,
            SHORT_LIVED_POLICY_WINDOW_SECS,
        )
    });
    recent_decisions.insert(
        scope.clone(),
        RecentPolicyDecision {
            decision,
            decided_at_epoch_secs: now_epoch_secs,
        },
    );
}

pub(crate) fn latest_policy_decision(
    recent_decisions: &mut ShortLivedRecentPolicyDecisions,
    scope: &PolicyDecisionScope,
    now_epoch_secs: u64,
    active_window_secs: u64,
) -> Option<RecentPolicyDecision> {
    recent_decisions.retain(|_, recent| {
        is_within_active_window(
            recent.decided_at_epoch_secs,
            now_epoch_secs,
            active_window_secs,
        )
    });
    recent_decisions.get(scope).and_then(|recent| {
        is_within_active_window(
            recent.decided_at_epoch_secs,
            now_epoch_secs,
            active_window_secs,
        )
        .then(|| recent.clone())
    })
}

pub(crate) fn record_retry_guard(
    retry_guards: &mut ShortLivedRetryGuards,
    scope: &PolicyDecisionScope,
    approach_hash: &str,
    now_epoch_secs: u64,
) {
    retry_guards.retain(|_, recent| {
        is_within_active_window(
            recent.recorded_at_epoch_secs,
            now_epoch_secs,
            SHORT_LIVED_POLICY_WINDOW_SECS,
        )
    });
    retry_guards.insert(
        scope.clone(),
        RecentRetryGuard {
            approach_hash: approach_hash.to_string(),
            recorded_at_epoch_secs: now_epoch_secs,
        },
    );
}

pub(crate) fn is_retry_guard_active(
    retry_guards: &mut ShortLivedRetryGuards,
    scope: &PolicyDecisionScope,
    approach_hash: &str,
    now_epoch_secs: u64,
    active_window_secs: u64,
) -> bool {
    retry_guards.retain(|_, recent| {
        is_within_active_window(
            recent.recorded_at_epoch_secs,
            now_epoch_secs,
            active_window_secs,
        )
    });
    retry_guards.get(scope).is_some_and(|recent| {
        recent.approach_hash == approach_hash
            && is_within_active_window(
                recent.recorded_at_epoch_secs,
                now_epoch_secs,
                active_window_secs,
            )
    })
}

pub(crate) fn should_reuse_recent_decision(
    recent_decisions: &RecentPolicyDecisionsByScope,
    scope: &PolicyDecisionScope,
    candidate: &PolicyDecision,
    now_epoch_secs: u64,
    active_window_secs: u64,
) -> bool {
    recent_decisions.get(scope).is_some_and(|recent| {
        recent.decision.semantic_identity() == candidate.semantic_identity()
            && is_within_active_window(
                recent.decided_at_epoch_secs,
                now_epoch_secs,
                active_window_secs,
            )
    })
}

pub(crate) fn has_active_retry_guard(
    retry_guards: &RetryGuardsByScope,
    scope: &PolicyDecisionScope,
    retry_guard: &str,
) -> bool {
    retry_guards
        .get(scope)
        .is_some_and(|active_retry_guard| active_retry_guard == retry_guard)
}

fn normalized_optional_text(value: &Option<String>) -> Option<String> {
    value
        .as_ref()
        .map(|value| normalize_policy_prompt_text(value, POLICY_PROMPT_MAX_FIELD_CHARS))
}

fn normalize_policy_prompt_text(value: &str, max_chars: usize) -> String {
    let collapsed = value.split_whitespace().collect::<Vec<_>>().join(" ");

    if collapsed.is_empty() {
        return String::new();
    }

    let mut normalized = String::new();
    for (index, ch) in collapsed.chars().enumerate() {
        if index >= max_chars {
            normalized.push_str("...");
            break;
        }
        normalized.push(ch);
    }

    normalized
}

fn format_policy_prompt_section(title: &str, value: Option<&str>) -> String {
    let content = value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("none");
    format!("## {title}\n{content}\n")
}

pub(crate) fn build_policy_eval_prompt(context: &PolicyEvaluationContext) -> String {
    let mut prompt = String::from(
        "You are evaluating whether the tamux orchestrator should continue, pivot, escalate, or halt_retries.\n\
         Return strict JSON only with this shape:\n\
         {\"action\":\"continue|pivot|escalate|halt_retries\",\"reason\":\"...\",\"strategy_hint\":null,\"retry_guard\":null}\n\
         Requirements:\n\
         - Use `continue` when evidence is weak or mixed.\n\
         - Keep `reason` short and concrete.\n\
         - Keep `strategy_hint` short and only use it for pivot.\n\
         - Only set `retry_guard` when blocking the same failing approach is justified.\n\
         - Do not invent missing context.\n\n",
    );

    let trigger = &context.trigger;
    let goal_run_id = trigger.goal_run_id.as_deref().unwrap_or("none");
    prompt.push_str(&format!(
        "## Trigger context\nthread_id: {}\ngoal_run_id: {}\nrepeated_approach: {}\nawareness_stuck: {}\nself_assessment.should_pivot: {}\nself_assessment.should_escalate: {}\n\n",
        trigger.thread_id,
        goal_run_id,
        trigger.repeated_approach,
        trigger.awareness_stuck,
        trigger.self_assessment.should_pivot,
        trigger.self_assessment.should_escalate,
    ));

    let tool_outcomes = if context.recent_tool_outcomes.is_empty() {
        "- none".to_string()
    } else {
        let rendered = context
            .recent_tool_outcomes
            .iter()
            .take(POLICY_PROMPT_MAX_TOOL_OUTCOMES)
            .map(|outcome| {
                format!(
                    "- {} => {}: {}",
                    normalize_policy_prompt_text(&outcome.tool_name, 40),
                    normalize_policy_prompt_text(&outcome.outcome, 20),
                    normalize_policy_prompt_text(
                        &outcome.summary,
                        POLICY_PROMPT_MAX_TOOL_SUMMARY_CHARS
                    )
                )
            })
            .collect::<Vec<_>>();
        let omitted_count = context
            .recent_tool_outcomes
            .len()
            .saturating_sub(POLICY_PROMPT_MAX_TOOL_OUTCOMES);
        let mut lines = rendered;
        if omitted_count > 0 {
            lines.push(format!(
                "- ... {omitted_count} additional tool outcomes omitted"
            ));
        }
        lines.join("\n")
    };
    prompt.push_str(&format!("## Recent tool outcomes\n{tool_outcomes}\n\n"));

    prompt.push_str(&format_policy_prompt_section(
        "Awareness summary",
        normalized_optional_text(&context.awareness_summary).as_deref(),
    ));
    prompt.push('\n');
    prompt.push_str(&format_policy_prompt_section(
        "Counter-who context",
        normalized_optional_text(&context.counter_who_context).as_deref(),
    ));
    prompt.push('\n');
    prompt.push_str(&format_policy_prompt_section(
        "Self-assessment summary",
        normalized_optional_text(&context.self_assessment_summary).as_deref(),
    ));
    prompt.push('\n');
    prompt.push_str(&format_policy_prompt_section(
        "Thread context",
        normalized_optional_text(&context.thread_context).as_deref(),
    ));
    prompt.push('\n');
    prompt.push_str(&format_policy_prompt_section(
        "Recent policy decision summary",
        normalized_optional_text(&context.recent_decision_summary).as_deref(),
    ));

    prompt
}

fn continue_policy_decision(reason: &str) -> PolicyDecision {
    PolicyDecision {
        action: PolicyAction::Continue,
        reason: reason.to_string(),
        strategy_hint: None,
        retry_guard: None,
    }
}

pub(crate) fn normalize_policy_eval_decision(decision: Option<PolicyDecision>) -> PolicyDecision {
    match decision {
        Some(decision) => match validate_policy_decision(&decision) {
            Ok(validated) => validated,
            Err(_) => continue_policy_decision(
                "Policy evaluation returned an invalid decision; continuing current execution.",
            ),
        },
        None => {
            continue_policy_decision("Policy evaluation unavailable; continuing current execution.")
        }
    }
}

fn trigger_requires_intervention(trigger: &PolicyTriggerContext) -> bool {
    trigger.repeated_approach
        || trigger.awareness_stuck
        || trigger.self_assessment.should_pivot
        || trigger.self_assessment.should_escalate
}

pub(crate) fn select_orchestrator_policy_decision(
    recent: Option<&RecentPolicyDecision>,
    trigger: &PolicyTriggerContext,
    evaluated: PolicyDecision,
) -> SelectedPolicyDecision {
    if trigger_requires_intervention(trigger) {
        if let Some(recent) = recent {
            if recent.decision.action != PolicyAction::Continue
                && recent.decision.semantic_identity() == evaluated.semantic_identity()
            {
                return SelectedPolicyDecision {
                    source: PolicyDecisionSource::ReusedRecent,
                    decision: recent.decision.clone(),
                };
            }
        }
    }

    SelectedPolicyDecision {
        source: PolicyDecisionSource::FreshEvaluation,
        decision: evaluated,
    }
}

fn summarize_recent_policy_decision(recent: &RecentPolicyDecision) -> String {
    let action = match recent.decision.action {
        PolicyAction::Continue => "continue",
        PolicyAction::Pivot => "pivot",
        PolicyAction::Escalate => "escalate",
        PolicyAction::HaltRetries => "halt_retries",
    };
    let reason = recent.decision.reason.trim();
    if reason.is_empty() {
        format!("Recent policy decision: {action}.")
    } else {
        format!("Recent policy decision: {action} because {reason}")
    }
}

fn decision_matches_runtime_context(
    decision: &PolicyDecision,
    context: &PolicyEvaluationContext,
) -> bool {
    decision
        .retry_guard
        .as_deref()
        .zip(context.current_retry_guard.as_deref())
        .is_some_and(|(decision_guard, current_guard)| decision_guard == current_guard)
}

fn infer_stuck_reason(trigger: &PolicyTriggerContext) -> Option<crate::agent::types::StuckReason> {
    if trigger.repeated_approach {
        Some(crate::agent::types::StuckReason::ErrorLoop)
    } else if trigger.awareness_stuck {
        Some(crate::agent::types::StuckReason::NoProgress)
    } else {
        None
    }
}

fn build_strategy_refresh_prompt(
    trigger: &PolicyTriggerContext,
    step_title: &str,
    task_retry_count: u32,
    tool_success_rate: f64,
    strategy_hint: Option<&str>,
) -> String {
    let decision = super::metacognitive::replanning::select_replan_strategy(
        &super::metacognitive::replanning::ReplanContext {
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
        },
    );
    let mut prompt = super::metacognitive::replanning::build_replan_prompt(&decision, step_title);
    if let Some(strategy_hint) = strategy_hint
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        prompt.push_str("\n\nPolicy hint: ");
        prompt.push_str(strategy_hint);
    }
    prompt
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
                    input_tokens: 0,
                    output_tokens: 0,
                    provider: None,
                    model: None,
                    api_transport: None,
                    response_id: None,
                    reasoning: None,
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
            tasks.iter()
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
        let task_id = match task_id {
            Some(task_id) => task_id,
            None => return Ok(PolicyLoopAction::Continue),
        };
        let attempts = {
            let tasks = self.tasks.lock().await;
            tasks.iter()
                .find(|task| task.id == task_id)
                .map(|task| task.retry_count)
                .unwrap_or(0)
        };
        let from_level = super::metacognitive::escalation::EscalationLevel::SelfCorrection;
        let to_level = super::metacognitive::escalation::EscalationLevel::User;
        let audit = super::metacognitive::escalation::escalation_audit_data(
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
            tasks.iter()
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
                decision.reason
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

    pub(super) async fn apply_orchestrator_policy_decision(
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
                let detail = decision
                    .retry_guard
                    .as_ref()
                    .map(|retry_guard| format!("approach_hash={retry_guard}; reason={}", decision.reason));
                self.mark_policy_halted_retry_failure(thread_id, task_id, detail)
                    .await;
                Ok(PolicyLoopAction::AbortRetry)
            }
        }
    }

    pub(super) async fn enforce_orchestrator_retry_guard(
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

    pub(super) async fn evaluate_orchestrator_policy_turn(
        &self,
        scope: &PolicyDecisionScope,
        context: PolicyEvaluationContext,
        now_epoch_secs: u64,
    ) -> Result<SelectedPolicyDecision> {
        let recent = self.latest_policy_decision(scope, now_epoch_secs).await;
        if let Some(recent) = recent.as_ref() {
            if recent.decision.action != PolicyAction::Continue
                && decision_matches_runtime_context(&recent.decision, &context)
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
        let evaluated = normalize_policy_eval_decision(
            self.request_orchestrator_policy_decision(&prompt).await?,
        );
        let selection = if decision_matches_runtime_context(&evaluated, &context) {
            select_orchestrator_policy_decision(recent.as_ref(), &context.trigger, evaluated.clone())
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

#[cfg(test)]
#[path = "orchestrator_policy_tests.rs"]
mod tests;
