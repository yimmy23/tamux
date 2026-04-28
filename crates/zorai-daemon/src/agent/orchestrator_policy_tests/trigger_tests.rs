use std::collections::HashMap;

use crate::agent::metacognitive::self_assessment::Assessment;

use super::super::*;
use super::common::*;

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
        Some("goal-1"),
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
        Some("goal-1"),
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
        },
    );
}

#[test]
fn hash_map_aliases_remain_usable_in_trigger_tests() {
    let contexts: HashMap<String, PolicyTriggerContext> = HashMap::new();
    assert!(contexts.is_empty());
}
