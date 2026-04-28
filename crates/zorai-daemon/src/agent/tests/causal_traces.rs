use super::*;
use crate::session_manager::SessionManager;
use tempfile::tempdir;

#[test]
fn estimated_success_probability_defaults_when_no_history() {
    assert!((estimated_success_probability(0, 0, false) - 0.65).abs() < f64::EPSILON);
    assert!((estimated_success_probability(0, 0, true) - 0.35).abs() < f64::EPSILON);
}

#[test]
fn plan_success_estimate_decreases_with_complexity() {
    assert!(estimate_plan_success(2, 0) > estimate_plan_success(6, 3));
}

#[test]
fn command_family_normalizes_prefix() {
    assert_eq!(command_family("git push origin main"), "git_push");
    assert_eq!(command_family("rm -rf build"), "rm__rf");
}

#[test]
fn summarize_outcome_preserves_recovery_for_near_miss() {
    let summary = summarize_outcome(
        crate::agent::learning::traces::CausalTraceOutcome::NearMiss {
            what_went_wrong: "command timed out".to_string(),
            how_recovered: "replanned into smaller steps".to_string(),
        },
    )
    .expect("near miss should summarize");

    assert!(summary.is_near_miss);
    assert_eq!(summary.reason, "command timed out");
    assert_eq!(
        summary.recovery.as_deref(),
        Some("replanned into smaller steps")
    );
}

#[test]
fn family_outcome_summary_tracks_failures_and_near_misses() {
    let mut summary = FamilyOutcomeSummary::default();
    summary.record(OutcomeSummary {
        reason: "permissions denied".to_string(),
        recovery: None,
        is_near_miss: false,
    });
    summary.record(OutcomeSummary {
        reason: "command timed out".to_string(),
        recovery: Some("replanned into smaller steps".to_string()),
        is_near_miss: true,
    });

    assert_eq!(summary.failure_count, 1);
    assert_eq!(summary.near_miss_count, 1);
    assert_eq!(summary.reasons.len(), 2);
    assert_eq!(summary.recoveries, vec!["replanned into smaller steps"]);
}

#[tokio::test]
async fn causal_guidance_summary_includes_upstream_recovery_patterns() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

    let selected_json = serde_json::json!({
        "option_type": "upstream_recovery",
        "reasoning": "Recovered from a daemon-generated invalid upstream request.",
        "rejection_reason": null,
        "estimated_success_prob": 0.72,
        "arguments_hash": "ctx_hash"
    })
    .to_string();
    let factors_json = serde_json::to_string(&vec![
        crate::agent::learning::traces::CausalFactor {
            factor_type: crate::agent::learning::traces::FactorType::PatternMatch,
            description: "upstream signature: request-invalid-empty-tool-name".to_string(),
            weight: 0.9,
        },
        crate::agent::learning::traces::CausalFactor {
            factor_type: crate::agent::learning::traces::FactorType::ResourceConstraint,
            description: "automatic retry repaired thread state before continuing".to_string(),
            weight: 0.6,
        },
    ])
    .expect("serialize factors");
    let outcome_json = serde_json::to_string(
        &crate::agent::learning::traces::CausalTraceOutcome::NearMiss {
            what_went_wrong: "provider rejected invalid tool metadata".to_string(),
            how_recovered: "repair the thread state and retry once".to_string(),
        },
    )
    .expect("serialize outcome");

    engine
        .history
        .insert_causal_trace(
            "causal_test_upstream_recovery",
            Some("thread-upstream-guidance"),
            None,
            None,
            "recovery",
            crate::agent::learning::traces::DecisionType::Recovery.family_label(),
            &selected_json,
            "[]",
            "ctx_hash",
            &factors_json,
            &outcome_json,
            Some("gpt-4o-mini"),
            now_millis(),
        )
        .await
        .expect("insert causal trace");

    let summary = engine
        .build_causal_guidance_summary()
        .await
        .expect("expected causal guidance summary");
    assert!(
        summary.contains("upstream recovery / request_invalid_empty_tool_name"),
        "expected upstream recovery guidance in summary: {summary}"
    );
    assert!(
        summary.contains("repair the thread state and retry once"),
        "expected the recovery pattern to be surfaced in summary: {summary}"
    );
}

#[tokio::test]
async fn settle_goal_plan_causal_traces_marks_unresolved_plan_success() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

    let selected_json = serde_json::json!({
        "option_type": "goal_plan",
        "reasoning": "Use a three-step plan",
        "rejection_reason": null,
        "estimated_success_prob": 0.72,
        "arguments_hash": "ctx_hash"
    })
    .to_string();
    let unresolved =
        serde_json::to_string(&crate::agent::learning::traces::CausalTraceOutcome::Unresolved)
            .expect("serialize unresolved outcome");

    engine
        .history
        .insert_causal_trace(
            "causal_test_goal_plan_success",
            Some("thread-goal-plan"),
            Some("goal-plan-1"),
            None,
            "plan_selection",
            crate::agent::learning::traces::DecisionType::PlanSelection.family_label(),
            &selected_json,
            "[]",
            "ctx_hash",
            "[]",
            &unresolved,
            Some("gpt-4o-mini"),
            now_millis(),
        )
        .await
        .expect("insert causal trace");

    let updated = engine
        .settle_goal_plan_causal_traces("goal-plan-1", "success", None)
        .await;
    assert_eq!(updated, 1);

    let records = engine
        .history
        .list_recent_causal_trace_records("goal_plan", 1)
        .await
        .expect("list settled goal plan traces");
    let outcome = serde_json::from_str::<crate::agent::learning::traces::CausalTraceOutcome>(
        &records[0].outcome_json,
    )
    .expect("deserialize settled outcome");
    assert!(matches!(
        outcome,
        crate::agent::learning::traces::CausalTraceOutcome::Success
    ));
}

#[tokio::test]
async fn settle_goal_plan_causal_traces_marks_unresolved_replan_failure() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

    let selected_json = serde_json::json!({
        "option_type": "goal_replan",
        "reasoning": "Retry with smaller recovery steps",
        "rejection_reason": null,
        "estimated_success_prob": 0.54,
        "arguments_hash": "ctx_hash"
    })
    .to_string();
    let unresolved =
        serde_json::to_string(&crate::agent::learning::traces::CausalTraceOutcome::Unresolved)
            .expect("serialize unresolved outcome");

    engine
        .history
        .insert_causal_trace(
            "causal_test_goal_replan_failure",
            Some("thread-goal-replan"),
            Some("goal-replan-1"),
            Some("task-replan-1"),
            "replan_selection",
            crate::agent::learning::traces::DecisionType::ReplanSelection.family_label(),
            &selected_json,
            "[]",
            "ctx_hash",
            "[]",
            &unresolved,
            Some("gpt-4o-mini"),
            now_millis(),
        )
        .await
        .expect("insert causal trace");

    let updated = engine
        .settle_goal_plan_causal_traces(
            "goal-replan-1",
            "failure",
            Some("the revised plan still failed at execution time"),
        )
        .await;
    assert_eq!(updated, 1);

    let records = engine
        .history
        .list_recent_causal_trace_records("goal_replan", 1)
        .await
        .expect("list settled goal replan traces");
    let outcome = serde_json::from_str::<crate::agent::learning::traces::CausalTraceOutcome>(
        &records[0].outcome_json,
    )
    .expect("deserialize settled outcome");
    match outcome {
        crate::agent::learning::traces::CausalTraceOutcome::Failure { reason } => {
            assert!(reason.contains("revised plan"));
        }
        other => panic!("expected failure outcome, got {other:?}"),
    }
}
