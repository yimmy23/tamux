use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use super::metacognitive::self_assessment::Assessment;

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
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
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
        context
            .recent_tool_outcomes
            .iter()
            .map(|outcome| {
                format!(
                    "- {} => {}: {}",
                    outcome.tool_name,
                    outcome.outcome,
                    outcome.summary.trim()
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
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

#[cfg(test)]
#[path = "orchestrator_policy_tests.rs"]
mod tests;
