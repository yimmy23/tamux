#![allow(dead_code)]

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::agent::metacognitive::self_assessment::Assessment;

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
    pub continuity_summary: Option<String>,
    pub counter_who_context: Option<String>,
    pub negative_constraints_context: Option<String>,
    pub self_assessment_summary: Option<String>,
    pub thread_context: Option<String>,
    pub recent_decision_summary: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum TriggerOutcome {
    NoIntervention,
    EvaluatePolicy(PolicyTriggerContext),
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
pub(super) struct PolicyDecisionSemanticIdentity {
    pub action: PolicyAction,
    pub retry_guard: Option<String>,
    pub strategy_hint: Option<String>,
}

impl PolicyDecision {
    pub(super) fn normalized_strategy_hint(&self) -> Option<String> {
        self.strategy_hint
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| value.to_ascii_lowercase())
    }

    pub(super) fn semantic_identity(&self) -> PolicyDecisionSemanticIdentity {
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
