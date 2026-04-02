use std::collections::HashMap;

use super::super::*;
use super::common::*;

#[test]
fn decision_validate_continue_accepts_structured_output() {
    let decision: PolicyDecision = serde_json::from_str(
        r#"{
            "action": "continue",
            "reason": "",
            "strategy_hint": null,
            "retry_guard": null
        }"#,
    )
    .unwrap();

    assert_eq!(
        validate_policy_decision(&decision),
        Ok(PolicyDecision {
            action: PolicyAction::Continue,
            reason: String::new(),
            strategy_hint: None,
            retry_guard: None,
        }),
    );
}

#[test]
fn decision_missing_retry_guard_defaults_to_none_during_deserialization() {
    let decision: PolicyDecision = serde_json::from_str(
        r#"{
            "action": "halt_retries",
            "reason": "Stop retrying the same failing approach.",
            "strategy_hint": null
        }"#,
    )
    .unwrap();

    assert_eq!(
        decision,
        PolicyDecision {
            action: PolicyAction::HaltRetries,
            reason: "Stop retrying the same failing approach.".to_string(),
            strategy_hint: None,
            retry_guard: None,
        },
    );
}

#[test]
fn decision_validate_pivot_accepts_retry_guard() {
    let decision: PolicyDecision = serde_json::from_str(
        r#"{
            "action": "pivot",
            "reason": "Repeated failures indicate the current strategy is stuck.",
            "strategy_hint": "Switch to a narrower inspection-first plan.",
            "retry_guard": "approach-hash-1"
        }"#,
    )
    .unwrap();

    assert_eq!(
        validate_policy_decision(&decision),
        Ok(PolicyDecision {
            action: PolicyAction::Pivot,
            reason: "Repeated failures indicate the current strategy is stuck.".to_string(),
            strategy_hint: Some("Switch to a narrower inspection-first plan.".to_string()),
            retry_guard: Some("approach-hash-1".to_string()),
        }),
    );
}

#[test]
fn decision_invalid_action_string_is_rejected() {
    let result = serde_json::from_str::<PolicyDecision>(
        r#"{
            "action": "retry_forever",
            "reason": "keep going",
            "strategy_hint": null,
            "retry_guard": null
        }"#,
    );

    assert!(result.is_err());
}

#[test]
fn decision_empty_reason_is_rejected_for_non_continue_actions() {
    let mut decision = decision(PolicyAction::Escalate);
    decision.reason = "   ".to_string();

    assert_eq!(
        validate_policy_decision(&decision),
        Err(PolicyDecisionValidationError::MissingReason {
            action: PolicyAction::Escalate,
        }),
    );
}

#[test]
fn decision_continue_with_retry_guard_is_rejected() {
    let mut decision = decision(PolicyAction::Continue);
    decision.retry_guard = Some("approach-hash-1".to_string());

    assert_eq!(
        validate_policy_decision(&decision),
        Err(PolicyDecisionValidationError::RetryGuardNotAllowed {
            action: PolicyAction::Continue,
        }),
    );
}

#[test]
fn decision_halt_retries_without_retry_guard_is_rejected() {
    let mut decision = decision(PolicyAction::HaltRetries);
    decision.reason = "Stop retrying the same failing approach.".to_string();

    assert_eq!(
        validate_policy_decision(&decision),
        Err(PolicyDecisionValidationError::RetryGuardRequired {
            action: PolicyAction::HaltRetries,
        }),
    );
}

#[test]
fn decision_unknown_fields_are_rejected_during_deserialization() {
    let result = serde_json::from_str::<PolicyDecision>(
        r#"{
            "action": "continue",
            "reason": "",
            "strategy_hint": null,
            "retry_guard": null,
            "extra_field": true
        }"#,
    );

    assert!(result.is_err());
}

#[test]
fn decision_antithrash_reuses_semantically_identical_decision_despite_wording_drift() {
    let scope = scope("thread-1", Some("goal-1"));
    let mut recorded = decision(PolicyAction::Pivot);
    recorded.reason = "We already know this approach is looping.".to_string();
    recorded.strategy_hint = Some("Use a different tool sequence.".to_string());
    recorded.retry_guard = Some("approach-hash-1".to_string());
    let mut candidate = decision(PolicyAction::Pivot);
    candidate.reason = "The current approach is still stuck.".to_string();
    candidate.strategy_hint = Some("Try a narrower recovery path.".to_string());
    candidate.retry_guard = Some("approach-hash-1".to_string());
    let recent_decisions = HashMap::from([(
        scope.clone(),
        RecentPolicyDecision {
            decision: recorded,
            decided_at_epoch_secs: 1_000,
        },
    )]);

    assert!(should_reuse_recent_decision(
        &recent_decisions,
        &scope,
        &candidate,
        1_030,
        60,
    ));
}

#[test]
fn decision_antithrash_and_retry_guards_do_not_leak_across_goal_runs() {
    let goal_one_scope = scope("thread-1", Some("goal-1"));
    let goal_two_scope = scope("thread-1", Some("goal-2"));
    let mut candidate = decision(PolicyAction::HaltRetries);
    candidate.reason = "Stop retrying the same failing approach.".to_string();
    candidate.retry_guard = Some("approach-hash-1".to_string());
    let recent_decisions = HashMap::from([(
        goal_one_scope.clone(),
        RecentPolicyDecision {
            decision: candidate.clone(),
            decided_at_epoch_secs: 1_000,
        },
    )]);
    let retry_guards = RetryGuardsByScope::from([(goal_one_scope, "approach-hash-1".to_string())]);

    assert!(!should_reuse_recent_decision(
        &recent_decisions,
        &goal_two_scope,
        &candidate,
        1_030,
        60,
    ));
    assert!(!has_active_retry_guard(
        &retry_guards,
        &goal_two_scope,
        "approach-hash-1",
    ));
}

#[test]
fn decision_pivot_without_retry_guard_and_different_strategy_hint_does_not_reuse() {
    let scope = scope("thread-1", Some("goal-1"));
    let mut recorded = decision(PolicyAction::Pivot);
    recorded.reason = "Try a filesystem-first investigation.".to_string();
    recorded.strategy_hint = Some("inspect logs first".to_string());
    let mut candidate = decision(PolicyAction::Pivot);
    candidate.reason = "Switch to a config-first recovery path.".to_string();
    candidate.strategy_hint = Some("review config before logs".to_string());
    let recent_decisions = HashMap::from([(
        scope.clone(),
        RecentPolicyDecision {
            decision: recorded,
            decided_at_epoch_secs: 1_000,
        },
    )]);

    assert!(!should_reuse_recent_decision(
        &recent_decisions,
        &scope,
        &candidate,
        1_030,
        60,
    ));
}

#[test]
fn decision_pivot_without_retry_guard_and_normalized_strategy_hint_reuses() {
    let scope = scope("thread-1", Some("goal-1"));
    let mut recorded = decision(PolicyAction::Pivot);
    recorded.reason = "Try a filesystem-first investigation.".to_string();
    recorded.strategy_hint = Some(" Inspect Logs First ".to_string());
    let mut candidate = decision(PolicyAction::Pivot);
    candidate.reason = "The current plan is still stuck.".to_string();
    candidate.strategy_hint = Some("inspect logs first".to_string());
    let recent_decisions = HashMap::from([(
        scope.clone(),
        RecentPolicyDecision {
            decision: recorded,
            decided_at_epoch_secs: 1_000,
        },
    )]);

    assert!(should_reuse_recent_decision(
        &recent_decisions,
        &scope,
        &candidate,
        1_030,
        60,
    ));
}

#[test]
fn policy_memory_stores_and_retrieves_latest_decision_by_thread_id() {
    let mut recent_decisions = ShortLivedRecentPolicyDecisions::new();
    let scope = scope("thread-1", Some("goal-1"));
    let recorded = reasoned_decision(PolicyAction::Pivot, "Switch to a narrower recovery path.");

    record_policy_decision(&mut recent_decisions, &scope, recorded.clone(), 1_000);

    assert_eq!(
        latest_policy_decision(&mut recent_decisions, &scope, 1_030, 60),
        Some(RecentPolicyDecision {
            decision: recorded,
            decided_at_epoch_secs: 1_000,
        }),
    );
}

#[test]
fn retry_guard_blocks_same_approach_hash_in_same_thread_id() {
    let mut retry_guards = ShortLivedRetryGuards::new();
    let scope = scope("thread-1", Some("goal-1"));

    record_retry_guard(&mut retry_guards, &scope, "approach-hash-1", 1_000);

    assert!(is_retry_guard_active(
        &mut retry_guards,
        &scope,
        "approach-hash-1",
        1_030,
        60,
    ));
}

#[test]
fn retry_guard_does_not_block_different_approach_hash() {
    let mut retry_guards = ShortLivedRetryGuards::new();
    let scope = scope("thread-1", Some("goal-1"));

    record_retry_guard(&mut retry_guards, &scope, "approach-hash-1", 1_000);

    assert!(!is_retry_guard_active(
        &mut retry_guards,
        &scope,
        "approach-hash-2",
        1_030,
        60,
    ));
}

#[test]
fn policy_memory_expired_entries_stop_applying() {
    let mut recent_decisions = ShortLivedRecentPolicyDecisions::new();
    let scope = scope("thread-1", Some("goal-1"));
    let recorded = reasoned_decision(PolicyAction::HaltRetries, "Stop retrying the same failure.");

    record_policy_decision(&mut recent_decisions, &scope, recorded, 1_000);

    assert_eq!(
        latest_policy_decision(&mut recent_decisions, &scope, 1_061, 60),
        None
    );
    assert!(recent_decisions.is_empty());
}

#[test]
fn retry_guard_expired_entries_stop_applying() {
    let mut retry_guards = ShortLivedRetryGuards::new();
    let scope = scope("thread-1", Some("goal-1"));

    record_retry_guard(&mut retry_guards, &scope, "approach-hash-1", 1_000);

    assert!(!is_retry_guard_active(
        &mut retry_guards,
        &scope,
        "approach-hash-1",
        1_061,
        60,
    ));
    assert!(retry_guards.is_empty());
}

#[test]
fn short_lived_policy_memory_does_not_leak_across_goal_runs_in_same_thread() {
    let mut recent_decisions = ShortLivedRecentPolicyDecisions::new();
    let goal_one_scope = scope("thread-1", Some("goal-1"));
    let goal_two_scope = scope("thread-1", Some("goal-2"));

    record_policy_decision(
        &mut recent_decisions,
        &goal_one_scope,
        reasoned_decision(PolicyAction::Pivot, "Switch approach."),
        1_000,
    );

    assert_eq!(
        latest_policy_decision(&mut recent_decisions, &goal_two_scope, 1_030, 60),
        None
    );
}

#[test]
fn short_lived_retry_guard_does_not_leak_across_goal_runs_in_same_thread() {
    let mut retry_guards = ShortLivedRetryGuards::new();
    let goal_one_scope = scope("thread-1", Some("goal-1"));
    let goal_two_scope = scope("thread-1", Some("goal-2"));

    record_retry_guard(&mut retry_guards, &goal_one_scope, "approach-hash-1", 1_000);

    assert!(!is_retry_guard_active(
        &mut retry_guards,
        &goal_two_scope,
        "approach-hash-1",
        1_030,
        60,
    ));
}

#[test]
fn apply_recent_policy_decision_is_not_reused_for_materially_different_retry_guard() {
    let trigger = PolicyTriggerContext {
        thread_id: "thread-policy-reuse-different".to_string(),
        goal_run_id: Some("goal-1".to_string()),
        repeated_approach: true,
        awareness_stuck: true,
        self_assessment: PolicySelfAssessmentSummary {
            should_pivot: false,
            should_escalate: false,
        },
    };
    let recent = RecentPolicyDecision {
        decision: PolicyDecision {
            action: PolicyAction::HaltRetries,
            reason: "Stop retrying the first failing approach.".to_string(),
            strategy_hint: None,
            retry_guard: Some("approach-hash-1".to_string()),
        },
        decided_at_epoch_secs: 1_000,
    };
    let evaluated = PolicyDecision {
        action: PolicyAction::HaltRetries,
        reason: "Stop retrying the new failing approach.".to_string(),
        strategy_hint: None,
        retry_guard: Some("approach-hash-2".to_string()),
    };

    let selection = select_orchestrator_policy_decision(Some(&recent), &trigger, evaluated.clone());

    assert_eq!(selection.source, PolicyDecisionSource::FreshEvaluation);
    assert_eq!(selection.decision, evaluated);
}
