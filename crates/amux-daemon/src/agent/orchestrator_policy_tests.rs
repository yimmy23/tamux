use std::collections::HashMap;

use super::*;

fn trigger_input(thread_id: &str) -> PolicyTriggerInput {
    PolicyTriggerInput {
        thread_id: thread_id.to_string(),
        goal_run_id: None,
        repeated_approach: false,
        awareness_stuck: false,
        should_pivot: false,
        should_escalate: false,
    }
}

fn evaluate_policy_context(input: &PolicyTriggerInput) -> PolicyTriggerContext {
    match evaluate_triggers(input) {
        TriggerOutcome::EvaluatePolicy(context) => context,
        TriggerOutcome::NoIntervention => panic!("expected policy evaluation"),
    }
}

fn decision(action: PolicyAction) -> PolicyDecision {
    PolicyDecision {
        action,
        reason: String::new(),
        strategy_hint: None,
        retry_guard: None,
    }
}

fn reasoned_decision(action: PolicyAction, reason: &str) -> PolicyDecision {
    let mut decision = decision(action);
    decision.reason = reason.to_string();
    decision
}

fn scope(thread_id: &str, goal_run_id: Option<&str>) -> PolicyDecisionScope {
    PolicyDecisionScope {
        thread_id: thread_id.to_string(),
        goal_run_id: goal_run_id.map(str::to_string),
    }
}

fn policy_eval_context() -> PolicyEvaluationContext {
    PolicyEvaluationContext {
        trigger: PolicyTriggerContext {
            thread_id: "thread-9".to_string(),
            goal_run_id: Some("goal-9".to_string()),
            repeated_approach: true,
            awareness_stuck: true,
            self_assessment: PolicySelfAssessmentSummary {
                should_pivot: true,
                should_escalate: false,
            },
        },
        recent_tool_outcomes: vec![
            PolicyToolOutcomeSummary {
                tool_name: "read_file".to_string(),
                outcome: "success".to_string(),
                summary: "Read the config but found no obvious mismatch.".to_string(),
            },
            PolicyToolOutcomeSummary {
                tool_name: "bash".to_string(),
                outcome: "failure".to_string(),
                summary: "Retrying the same test command still exits with code 1.".to_string(),
            },
        ],
        awareness_summary: Some(
            "Short-term tool success rate dropped and repeated failures cluster on the same path."
                .to_string(),
        ),
        counter_who_context: Some(
            "Counter-who detected the same failing bash approach three times.".to_string(),
        ),
        self_assessment_summary: Some(
            "Negative momentum suggests the current strategy is no longer productive.".to_string(),
        ),
        thread_context: Some(
            "Operator asked for a narrow fix without broad refactoring.".to_string(),
        ),
        recent_decision_summary: Some(
            "Recent policy decision: pivot because the previous retry loop was stuck.".to_string(),
        ),
    }
}

#[test]
fn trigger_no_intervention_when_all_inputs_are_nominal() {
    let mut input = trigger_input("thread-1");
    input.goal_run_id = Some("goal-1".to_string());

    assert_eq!(evaluate_triggers(&input), TriggerOutcome::NoIntervention);
}

#[test]
fn trigger_intervention_required_for_repeated_approach_signal() {
    let mut input = trigger_input("thread-1");
    input.goal_run_id = Some("goal-1".to_string());
    input.repeated_approach = true;

    let context = evaluate_policy_context(&input);

    assert_eq!(context.thread_id, "thread-1");
    assert_eq!(context.goal_run_id.as_deref(), Some("goal-1"));
    assert!(context.repeated_approach);
    assert!(!context.awareness_stuck);
    assert!(!context.self_assessment.should_pivot);
    assert!(!context.self_assessment.should_escalate);
}

#[test]
fn trigger_intervention_required_for_awareness_stuckness() {
    let mut input = trigger_input("thread-2");
    input.awareness_stuck = true;

    let context = evaluate_policy_context(&input);

    assert_eq!(context.thread_id, "thread-2");
    assert!(context.awareness_stuck);
    assert!(!context.repeated_approach);
    assert!(!context.self_assessment.should_pivot);
    assert!(!context.self_assessment.should_escalate);
}

#[test]
fn trigger_intervention_required_for_self_assessment_pivot_or_escalate() {
    let mut pivot_input = trigger_input("thread-3");
    pivot_input.goal_run_id = Some("goal-3".to_string());
    pivot_input.should_pivot = true;

    let mut escalate_input = trigger_input("thread-4");
    escalate_input.goal_run_id = Some("goal-4".to_string());
    escalate_input.should_escalate = true;

    let pivot_context = evaluate_policy_context(&pivot_input);
    let escalate_context = evaluate_policy_context(&escalate_input);

    assert!(pivot_context.self_assessment.should_pivot);
    assert!(!pivot_context.self_assessment.should_escalate);
    assert_eq!(escalate_context.goal_run_id.as_deref(), Some("goal-4"));
    assert!(!escalate_context.self_assessment.should_pivot);
    assert!(escalate_context.self_assessment.should_escalate);
}

#[test]
fn trigger_aggregation_is_keyed_by_thread_id() {
    let inputs = vec![
        {
            let mut input = trigger_input("thread-1");
            input.goal_run_id = Some("goal-1".to_string());
            input.repeated_approach = true;
            input
        },
        {
            let mut input = trigger_input("thread-2");
            input.goal_run_id = Some("goal-2".to_string());
            input
        },
        {
            let mut input = trigger_input("thread-3");
            input.should_escalate = true;
            input
        },
    ];

    let contexts = aggregate_trigger_contexts(&inputs);

    assert_eq!(contexts.len(), 2);
    assert_eq!(
        contexts
            .get("thread-1")
            .and_then(|context| context.goal_run_id.as_deref()),
        Some("goal-1")
    );
    assert!(contexts["thread-1"].repeated_approach);
    assert!(contexts["thread-3"].self_assessment.should_escalate);
    assert!(!contexts.contains_key("thread-2"));
}

#[test]
fn trigger_aggregation_merges_active_signals_for_same_thread() {
    let inputs = vec![
        {
            let mut input = trigger_input("thread-1");
            input.goal_run_id = Some("goal-1".to_string());
            input.repeated_approach = true;
            input
        },
        {
            let mut input = trigger_input("thread-1");
            input.goal_run_id = Some("goal-1".to_string());
            input.awareness_stuck = true;
            input.should_pivot = true;
            input
        },
    ];

    let contexts = aggregate_trigger_contexts(&inputs);
    let context = &contexts["thread-1"];

    assert_eq!(context.goal_run_id.as_deref(), Some("goal-1"));
    assert!(context.repeated_approach);
    assert!(context.awareness_stuck);
    assert!(context.self_assessment.should_pivot);
    assert!(!context.self_assessment.should_escalate);
}

#[test]
fn trigger_aggregation_prefers_first_non_none_goal_run_id_for_same_thread() {
    let inputs = vec![
        {
            let mut input = trigger_input("thread-1");
            input.repeated_approach = true;
            input
        },
        {
            let mut input = trigger_input("thread-1");
            input.goal_run_id = Some("goal-1".to_string());
            input.awareness_stuck = true;
            input
        },
        {
            let mut input = trigger_input("thread-1");
            input.goal_run_id = Some("goal-2".to_string());
            input.should_escalate = true;
            input
        },
    ];

    let contexts = aggregate_trigger_contexts(&inputs);

    assert_eq!(
        contexts
            .get("thread-1")
            .and_then(|context| context.goal_run_id.as_deref()),
        Some("goal-1")
    );
}

#[test]
fn trigger_assessment_adapter_captures_pivot_and_escalate_flags() {
    let assessment = Assessment {
        making_progress: false,
        approach_optimal: false,
        should_escalate: true,
        should_pivot: true,
        should_terminate: false,
        confidence: 0.2,
        reasoning: "signals indicate intervention".to_string(),
        recommendations: vec!["pivot".to_string(), "escalate".to_string()],
    };

    assert_eq!(
        PolicySelfAssessmentSummary::from(&assessment),
        PolicySelfAssessmentSummary {
            should_pivot: true,
            should_escalate: true,
        }
    );
}

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
        })
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
        })
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
        })
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
        })
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
        })
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
        })
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
fn policy_eval_prompt_builder_includes_recent_context_sections() {
    let prompt = build_policy_eval_prompt(&policy_eval_context());

    assert!(prompt.contains("Recent tool outcomes"));
    assert!(prompt.contains("read_file => success: Read the config but found no obvious mismatch."));
    assert!(
        prompt.contains("bash => failure: Retrying the same test command still exits with code 1.")
    );
    assert!(prompt.contains("Awareness summary"));
    assert!(prompt.contains("Counter-who context"));
    assert!(prompt.contains("Self-assessment summary"));
    assert!(prompt.contains("Thread context"));
    assert!(prompt.contains("Recent policy decision summary"));
    assert!(prompt.contains("thread-9"));
    assert!(prompt.contains("goal-9"));
}

#[test]
fn policy_eval_invalid_structured_output_falls_back_safely() {
    let invalid = PolicyDecision {
        action: PolicyAction::HaltRetries,
        reason: "Stop repeating the same failing path.".to_string(),
        strategy_hint: Some("Try a different recovery path.".to_string()),
        retry_guard: None,
    };

    assert_eq!(
        normalize_policy_eval_decision(Some(invalid)),
        PolicyDecision {
            action: PolicyAction::Continue,
            reason: "Policy evaluation returned an invalid decision; continuing current execution."
                .to_string(),
            strategy_hint: None,
            retry_guard: None,
        }
    );
}

#[test]
fn policy_eval_missing_result_degrades_to_continue() {
    assert_eq!(
        normalize_policy_eval_decision(None),
        PolicyDecision {
            action: PolicyAction::Continue,
            reason: "Policy evaluation unavailable; continuing current execution.".to_string(),
            strategy_hint: None,
            retry_guard: None,
        }
    );
}
