use super::*;

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

pub(crate) fn summarize_recent_policy_decision(recent: &RecentPolicyDecision) -> String {
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
