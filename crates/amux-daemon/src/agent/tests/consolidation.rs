use super::*;
use tempfile::tempdir;
use tokio::time::Duration;

#[test]
fn idle_returns_true_when_all_conditions_met() {
    assert!(is_idle_for_consolidation(
        0,
        0,
        0,
        Some(1000),
        1000 + DEFAULT_IDLE_THRESHOLD_MS,
        DEFAULT_IDLE_THRESHOLD_MS,
    ));
}

#[test]
fn idle_returns_false_with_active_task() {
    assert!(!is_idle_for_consolidation(
        1,
        0,
        0,
        Some(0),
        DEFAULT_IDLE_THRESHOLD_MS + 1,
        DEFAULT_IDLE_THRESHOLD_MS,
    ));
}

#[test]
fn idle_returns_false_with_active_goal_run() {
    assert!(!is_idle_for_consolidation(
        0,
        1,
        0,
        Some(0),
        DEFAULT_IDLE_THRESHOLD_MS + 1,
        DEFAULT_IDLE_THRESHOLD_MS,
    ));
}

#[test]
fn idle_returns_false_with_active_stream() {
    assert!(!is_idle_for_consolidation(
        0,
        0,
        1,
        Some(0),
        DEFAULT_IDLE_THRESHOLD_MS + 1,
        DEFAULT_IDLE_THRESHOLD_MS,
    ));
}

#[test]
fn idle_returns_false_with_recent_presence() {
    assert!(!is_idle_for_consolidation(
        0,
        0,
        0,
        Some(10_000),
        10_001,
        DEFAULT_IDLE_THRESHOLD_MS,
    ));
}

#[test]
fn idle_returns_true_when_no_presence_recorded() {
    assert!(is_idle_for_consolidation(
        0,
        0,
        0,
        None,
        1000,
        DEFAULT_IDLE_THRESHOLD_MS,
    ));
}

#[tokio::test]
async fn maybe_run_consolidation_if_idle_blocks_when_goal_run_is_awaiting_approval() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.consolidation.enabled = true;
    config.consolidation.idle_threshold_secs = 0;
    let engine = AgentEngine::new_test(manager, config, root.path()).await;

    let goal = GoalRun {
        id: "goal-awaiting-approval".to_string(),
        title: "goal awaiting approval".to_string(),
        goal: "wait for operator approval".to_string(),
        client_request_id: None,
        status: GoalRunStatus::AwaitingApproval,
        priority: TaskPriority::Normal,
        created_at: 0,
        updated_at: 0,
        started_at: None,
        completed_at: None,
        thread_id: None,
        session_id: None,
        current_step_index: 0,
        current_step_title: None,
        current_step_kind: None,
        replan_count: 0,
        max_replans: 3,
        plan_summary: None,
        reflection_summary: None,
        memory_updates: Vec::new(),
        generated_skill_path: None,
        last_error: None,
        failure_cause: None,
        child_task_ids: Vec::new(),
        child_task_count: 0,
        approval_count: 0,
        awaiting_approval_id: Some("approval-1".to_string()),
        policy_fingerprint: None,
        approval_expires_at: None,
        containment_scope: None,
        compensation_status: None,
        compensation_summary: None,
        active_task_id: None,
        duration_ms: None,
        steps: Vec::new(),
        events: Vec::new(),
        total_prompt_tokens: 0,
        total_completion_tokens: 0,
        estimated_cost_usd: None,
        autonomy_level: Default::default(),
        authorship_tag: None,
    };
    engine.goal_runs.lock().await.push_back(goal);

    let result = engine
        .maybe_run_consolidation_if_idle(Duration::from_millis(5))
        .await;
    assert!(
        result.is_none(),
        "dream/consolidation should stay paused while a goal run is awaiting approval"
    );
}

#[test]
fn decay_returns_half_at_half_life() {
    let now = 1_000_000_000u64;
    let half_life_ms = (DEFAULT_HALF_LIFE_HOURS * 3_600_000.0) as u64;
    let last_confirmed = now - half_life_ms;
    let confidence = compute_decay_confidence(last_confirmed, now, DEFAULT_HALF_LIFE_HOURS);
    assert!(
        (confidence - 0.5).abs() < 0.01,
        "expected ~0.5, got {confidence}"
    );
}

#[test]
fn decay_returns_near_one_for_just_confirmed() {
    let now = 1_000_000_000u64;
    let confidence = compute_decay_confidence(now, now, DEFAULT_HALF_LIFE_HOURS);
    assert!(
        (confidence - 1.0).abs() < 0.001,
        "expected ~1.0, got {confidence}"
    );
}

#[test]
fn decay_returns_zero_for_zero_timestamp() {
    let confidence = compute_decay_confidence(0, 1_000_000, DEFAULT_HALF_LIFE_HOURS);
    assert_eq!(confidence, 0.0);
}

#[test]
fn decay_returns_zero_for_nonpositive_half_life() {
    let confidence = compute_decay_confidence(500_000, 1_000_000, 0.0);
    assert_eq!(confidence, 0.0);
    let confidence = compute_decay_confidence(500_000, 1_000_000, -5.0);
    assert_eq!(confidence, 0.0);
}

#[test]
fn decay_clamps_to_valid_range() {
    let c1 = compute_decay_confidence(1, 2, DEFAULT_HALF_LIFE_HOURS);
    assert!((0.0..=1.0).contains(&c1));

    let c2 = compute_decay_confidence(1, u64::MAX / 2, DEFAULT_HALF_LIFE_HOURS);
    assert!((0.0..=1.0).contains(&c2));
}

#[test]
fn decay_handles_very_large_age_without_panic() {
    let confidence = compute_decay_confidence(0, 5_000_000_000, DEFAULT_HALF_LIFE_HOURS);
    assert_eq!(confidence, 0.0);

    let confidence = compute_decay_confidence(1, 5_000_000_000, DEFAULT_HALF_LIFE_HOURS);
    assert!((0.0..=1.0).contains(&confidence));
    assert!(confidence < 0.001);
}
