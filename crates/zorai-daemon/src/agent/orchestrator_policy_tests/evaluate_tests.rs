use std::collections::VecDeque;
use std::sync::{Arc, Mutex as StdMutex};

use super::super::*;
use super::common::*;

#[tokio::test]
async fn apply_recent_policy_decision_is_persisted_and_reused_on_next_relevant_turn() {
    let engine = test_engine().await;
    let thread_id = "thread-policy-reuse";
    seed_runtime(&engine, thread_id).await;
    let scope = scope(thread_id, Some("goal-1"));
    let trigger = PolicyTriggerContext {
        thread_id: thread_id.to_string(),
        goal_run_id: Some("goal-1".to_string()),
        repeated_approach: true,
        awareness_stuck: true,
        self_assessment: PolicySelfAssessmentSummary {
            should_pivot: true,
            should_escalate: false,
        },
    };
    let pivot_decision = PolicyDecision {
        action: PolicyAction::Pivot,
        reason: "Switch away from the repeating failure.".to_string(),
        strategy_hint: Some("Inspect the workspace before running commands again.".to_string()),
        retry_guard: Some("approach-hash-1".to_string()),
    };

    engine
        .record_policy_decision(&scope, pivot_decision.clone(), 1_000)
        .await;
    let recent = engine
        .latest_policy_decision(&scope, 1_010)
        .await
        .expect("recent policy decision");

    let selection = select_orchestrator_policy_decision(
        Some(&recent),
        &trigger,
        PolicyDecision {
            action: PolicyAction::Pivot,
            reason: "Fresh wording but same bounded pivot.".to_string(),
            strategy_hint: Some("Inspect the workspace before running commands again.".to_string()),
            retry_guard: Some("approach-hash-1".to_string()),
        },
    );

    assert_eq!(selection.source, PolicyDecisionSource::ReusedRecent);
    assert_eq!(selection.decision, pivot_decision);
}

#[tokio::test]
async fn evaluate_policy_turn_reuses_persisted_recent_decision_for_matching_runtime_candidate() {
    let recorded_bodies = Arc::new(StdMutex::new(VecDeque::new()));
    let engine = policy_runtime_engine(
        r#"{"action":"pivot","reason":"Current path is still stuck.","strategy_hint":"Inspect the workspace before running commands again."}"#,
        recorded_bodies.clone(),
    )
    .await;
    let scope = scope("thread-runtime-reuse", Some("goal-1"));
    let persisted = PolicyDecision {
        action: PolicyAction::Pivot,
        reason: "Switch away from the repeating failure.".to_string(),
        strategy_hint: Some("Inspect the workspace before running commands again.".to_string()),
        retry_guard: Some("approach-hash-1".to_string()),
    };

    engine
        .record_policy_decision(&scope, persisted.clone(), 1_000)
        .await;

    let selection = engine
        .evaluate_orchestrator_policy_turn(&scope, policy_eval_context(), 1_010)
        .await
        .expect("policy evaluation should succeed");

    assert_eq!(selection.source, PolicyDecisionSource::ReusedRecent);
    assert_eq!(selection.decision, persisted);
    assert!(recorded_bodies.lock().expect("lock request log").is_empty());
}

#[tokio::test]
async fn evaluate_policy_turn_does_not_reuse_recent_decision_for_different_runtime_retry_guard() {
    let recorded_bodies = Arc::new(StdMutex::new(VecDeque::new()));
    let engine = policy_runtime_engine(
        r#"{"action":"halt_retries","reason":"Stop retrying the new failing approach.","strategy_hint":null}"#,
        recorded_bodies.clone(),
    )
    .await;
    let scope = scope("thread-runtime-no-reuse", Some("goal-1"));

    engine
        .record_policy_decision(
            &scope,
            PolicyDecision {
                action: PolicyAction::HaltRetries,
                reason: "Stop retrying the first failing approach.".to_string(),
                strategy_hint: None,
                retry_guard: Some("approach-hash-1".to_string()),
            },
            1_000,
        )
        .await;

    let mut context = policy_eval_context();
    context.current_retry_guard = Some("approach-hash-2".to_string());

    let selection = engine
        .evaluate_orchestrator_policy_turn(&scope, context, 1_010)
        .await
        .expect("policy evaluation should succeed");

    assert_eq!(selection.source, PolicyDecisionSource::FreshEvaluation);
    assert_eq!(selection.decision.action, PolicyAction::HaltRetries);
    assert_eq!(
        selection.decision.retry_guard.as_deref(),
        Some("approach-hash-2")
    );
    let recorded = recorded_bodies.lock().expect("lock request log");
    assert!(
        recorded.iter().any(|body| {
            body.contains("structured_output")
                || body.contains("\"response_format\"")
                || body.contains("\"text\":{\"format\"")
        }),
        "expected a fresh structured policy evaluation request for the new retry guard",
    );
}

#[tokio::test]
async fn evaluate_policy_turn_records_runtime_owned_guard_for_fresh_halt_retries() {
    let recorded_bodies = Arc::new(StdMutex::new(VecDeque::new()));
    let engine = policy_runtime_engine(
        r#"{"action":"halt_retries","reason":"Stop retrying the same failing approach.","strategy_hint":null}"#,
        recorded_bodies,
    )
    .await;
    let scope = scope("thread-runtime-owned-guard", Some("goal-1"));

    let selection = engine
        .evaluate_orchestrator_policy_turn(&scope, policy_eval_context(), 1_010)
        .await
        .expect("policy evaluation should succeed");

    assert_eq!(selection.source, PolicyDecisionSource::FreshEvaluation);
    assert_eq!(selection.decision.action, PolicyAction::HaltRetries);
    assert_eq!(
        selection.decision.retry_guard.as_deref(),
        Some("approach-hash-1")
    );

    let recent = engine
        .latest_policy_decision(&scope, 1_020)
        .await
        .expect("recent policy decision");
    assert_eq!(
        recent.decision.retry_guard.as_deref(),
        Some("approach-hash-1")
    );
}

#[tokio::test]
async fn evaluate_policy_turn_fresh_halt_retries_without_live_guard_degrades_to_continue() {
    let recorded_bodies = Arc::new(StdMutex::new(VecDeque::new()));
    let engine = policy_runtime_engine(
        r#"{"action":"halt_retries","reason":"Stop retrying the same failing approach.","strategy_hint":null}"#,
        recorded_bodies,
    )
    .await;
    let scope = scope("thread-runtime-no-live-guard", Some("goal-1"));
    let mut context = policy_eval_context();
    context.current_retry_guard = None;

    let selection = engine
        .evaluate_orchestrator_policy_turn(&scope, context, 1_010)
        .await
        .expect("policy evaluation should succeed");

    assert_eq!(selection.source, PolicyDecisionSource::FreshEvaluation);
    assert_eq!(selection.decision.action, PolicyAction::Continue);
    assert_eq!(selection.decision.retry_guard, None);
}

#[tokio::test]
async fn evaluate_policy_turn_reuses_recent_non_guarded_decision_for_matching_runtime_candidate() {
    let recorded_bodies = Arc::new(StdMutex::new(VecDeque::new()));
    let engine = policy_runtime_engine(
        r#"{"action":"escalate","reason":"Operator guidance is still needed.","strategy_hint":null}"#,
        recorded_bodies.clone(),
    )
    .await;
    let scope = scope("thread-runtime-reuse-non-guarded", Some("goal-1"));
    let persisted = PolicyDecision {
        action: PolicyAction::Escalate,
        reason: "Repeated failures need operator guidance now.".to_string(),
        strategy_hint: None,
        retry_guard: None,
    };

    engine
        .record_policy_decision(&scope, persisted.clone(), 1_000)
        .await;

    let selection = engine
        .evaluate_orchestrator_policy_turn(&scope, policy_eval_context(), 1_010)
        .await
        .expect("policy evaluation should succeed");

    assert_eq!(selection.source, PolicyDecisionSource::ReusedRecent);
    assert_eq!(selection.decision, persisted);
    let recorded = recorded_bodies.lock().expect("lock request log");
    assert!(
        recorded.iter().any(|body| {
            body.contains("structured_output")
                || body.contains("\"response_format\"")
                || body.contains("\"text\":{\"format\"")
        }),
        "expected runtime evaluation to inspect a fresh structured candidate before reusing the recent non-guarded decision",
    );
}

#[tokio::test]
async fn evaluate_policy_turn_does_not_reuse_recent_non_guarded_decision_for_materially_different_candidate(
) {
    let recorded_bodies = Arc::new(StdMutex::new(VecDeque::new()));
    let engine = policy_runtime_engine(
        r#"{"action":"pivot","reason":"A different bounded strategy is more appropriate.","strategy_hint":"Inspect the workspace before running commands again."}"#,
        recorded_bodies.clone(),
    )
    .await;
    let scope = scope("thread-runtime-no-reuse-non-guarded", Some("goal-1"));

    engine
        .record_policy_decision(
            &scope,
            PolicyDecision {
                action: PolicyAction::Escalate,
                reason: "Repeated failures need operator guidance now.".to_string(),
                strategy_hint: None,
                retry_guard: None,
            },
            1_000,
        )
        .await;

    let selection = engine
        .evaluate_orchestrator_policy_turn(&scope, policy_eval_context(), 1_010)
        .await
        .expect("policy evaluation should succeed");

    assert_eq!(selection.source, PolicyDecisionSource::FreshEvaluation);
    assert_eq!(selection.decision.action, PolicyAction::Pivot);
    assert_eq!(
        selection.decision.strategy_hint.as_deref(),
        Some("Inspect the workspace before running commands again."),
    );
    let recorded = recorded_bodies.lock().expect("lock request log");
    assert!(
        recorded.iter().any(|body| {
            body.contains("structured_output")
                || body.contains("\"response_format\"")
                || body.contains("\"text\":{\"format\"")
        }),
        "expected a fresh structured policy evaluation request for the materially different non-guarded candidate",
    );
}
