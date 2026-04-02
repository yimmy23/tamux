use super::*;

fn base_ctx() -> ReplanContext {
    ReplanContext {
        current_step_index: 0,
        step_title: "Implement feature X".into(),
        stuck_reason: None,
        attempt_count: 0,
        error_rate: 0.1,
        tool_success_rate: 0.9,
        context_utilization_pct: 40,
        has_checkpoint: true,
        recent_tool_names: vec!["code_edit".into(), "code_edit".into(), "search".into()],
    }
}

#[test]
fn compress_retry_for_no_progress_attempt_0() {
    let mut ctx = base_ctx();
    ctx.stuck_reason = Some(StuckReason::NoProgress);
    ctx.attempt_count = 0;

    let decision = select_replan_strategy(&ctx);
    assert!(matches!(decision.strategy, ReplanStrategy::CompressRetry));
}

#[test]
fn spawn_expert_for_no_progress_attempt_1() {
    let mut ctx = base_ctx();
    ctx.stuck_reason = Some(StuckReason::NoProgress);
    ctx.attempt_count = 1;

    let decision = select_replan_strategy(&ctx);
    assert!(matches!(
        decision.strategy,
        ReplanStrategy::SpawnExpert { .. }
    ));
}

#[test]
fn user_guidance_for_no_progress_attempt_2_plus() {
    let mut ctx = base_ctx();
    ctx.stuck_reason = Some(StuckReason::NoProgress);
    ctx.attempt_count = 3;

    let decision = select_replan_strategy(&ctx);
    assert!(matches!(
        decision.strategy,
        ReplanStrategy::UserGuidance { .. }
    ));
}

#[test]
fn alternative_tools_for_error_loop() {
    let mut ctx = base_ctx();
    ctx.stuck_reason = Some(StuckReason::ErrorLoop);
    ctx.error_rate = 0.5;

    let decision = select_replan_strategy(&ctx);
    assert!(matches!(
        decision.strategy,
        ReplanStrategy::AlternativeTools { .. }
    ));
}

#[test]
fn goal_revision_for_tool_call_loop() {
    let mut ctx = base_ctx();
    ctx.stuck_reason = Some(StuckReason::ToolCallLoop);

    let decision = select_replan_strategy(&ctx);
    assert!(matches!(
        decision.strategy,
        ReplanStrategy::GoalRevision { .. }
    ));
}

#[test]
fn compress_retry_for_resource_exhaustion() {
    let mut ctx = base_ctx();
    ctx.stuck_reason = Some(StuckReason::ResourceExhaustion);
    ctx.context_utilization_pct = 95;

    let decision = select_replan_strategy(&ctx);
    assert!(matches!(decision.strategy, ReplanStrategy::CompressRetry));
}

#[test]
fn parallelize_for_timeout_attempt_0() {
    let mut ctx = base_ctx();
    ctx.stuck_reason = Some(StuckReason::Timeout);
    ctx.attempt_count = 0;

    let decision = select_replan_strategy(&ctx);
    assert!(matches!(
        decision.strategy,
        ReplanStrategy::Parallelize { .. }
    ));
}

#[test]
fn user_guidance_for_timeout_attempt_1_plus() {
    let mut ctx = base_ctx();
    ctx.stuck_reason = Some(StuckReason::Timeout);
    ctx.attempt_count = 2;

    let decision = select_replan_strategy(&ctx);
    assert!(matches!(
        decision.strategy,
        ReplanStrategy::UserGuidance { .. }
    ));
}

#[test]
fn goal_revision_for_high_error_rate() {
    let mut ctx = base_ctx();
    ctx.stuck_reason = Some(StuckReason::NoProgress);
    ctx.error_rate = 0.85;

    let decision = select_replan_strategy(&ctx);
    assert!(matches!(
        decision.strategy,
        ReplanStrategy::GoalRevision { .. }
    ));
}

#[test]
fn compress_retry_for_preventive_no_stuck_reason() {
    let decision = select_replan_strategy(&base_ctx());
    assert!(matches!(decision.strategy, ReplanStrategy::CompressRetry));
}

#[test]
fn fallback_differs_from_primary() {
    let mut ctx = base_ctx();
    ctx.stuck_reason = Some(StuckReason::NoProgress);
    ctx.attempt_count = 0;

    let decision = select_replan_strategy(&ctx);
    let fallback = decision.fallback.expect("expected a fallback strategy");
    assert!(!matches!(fallback, ReplanStrategy::CompressRetry));
}

#[test]
fn build_replan_prompt_includes_step_title() {
    let decision = ReplanDecision {
        strategy: ReplanStrategy::CompressRetry,
        reasoning: "test reasoning".into(),
        confidence: 0.75,
        fallback: None,
    };

    let prompt = build_replan_prompt(&decision, "Write unit tests");
    assert!(prompt.contains("Write unit tests"));
}

#[test]
fn resource_exhaustion_ignores_high_error_rate() {
    let mut ctx = base_ctx();
    ctx.stuck_reason = Some(StuckReason::ResourceExhaustion);
    ctx.error_rate = 0.9;
    ctx.context_utilization_pct = 98;

    let decision = select_replan_strategy(&ctx);
    assert!(matches!(decision.strategy, ReplanStrategy::CompressRetry));
}

#[test]
fn alternative_tools_disables_most_used() {
    let mut ctx = base_ctx();
    ctx.stuck_reason = Some(StuckReason::ErrorLoop);
    ctx.recent_tool_names = vec![
        "bash".into(),
        "code_edit".into(),
        "bash".into(),
        "bash".into(),
    ];

    let decision = select_replan_strategy(&ctx);
    if let ReplanStrategy::AlternativeTools { disable_tools } = &decision.strategy {
        assert_eq!(disable_tools, &["bash"]);
    } else {
        panic!("expected AlternativeTools, got {:?}", decision.strategy);
    }
}

#[test]
fn spawn_expert_infers_expertise() {
    let mut ctx = base_ctx();
    ctx.step_title = "Fix SQL migration for users table".into();
    ctx.stuck_reason = Some(StuckReason::NoProgress);
    ctx.attempt_count = 1;

    let decision = select_replan_strategy(&ctx);
    if let ReplanStrategy::SpawnExpert { expertise } = &decision.strategy {
        assert_eq!(expertise, "databases");
    } else {
        panic!("expected SpawnExpert, got {:?}", decision.strategy);
    }
}

#[test]
fn build_replan_prompt_includes_fallback() {
    let decision = ReplanDecision {
        strategy: ReplanStrategy::CompressRetry,
        reasoning: "context is large".into(),
        confidence: 0.8,
        fallback: Some(ReplanStrategy::UserGuidance {
            question: "Help?".into(),
        }),
    };

    let prompt = build_replan_prompt(&decision, "Deploy service");
    assert!(prompt.contains("Fallback strategy"));
}
