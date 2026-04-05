use super::*;
use crate::agent::episodic::{EpisodeType, LinkType};
use crate::session_manager::SessionManager;
use tempfile::tempdir;

fn sample_supervised_goal_run(goal_run_id: &str, task_id: &str, approval_id: &str) -> GoalRun {
    GoalRun {
        id: goal_run_id.to_string(),
        title: "supervised goal".to_string(),
        goal: "verify explicit acknowledgment".to_string(),
        client_request_id: None,
        status: GoalRunStatus::AwaitingApproval,
        priority: TaskPriority::Normal,
        created_at: now_millis(),
        updated_at: now_millis(),
        started_at: Some(now_millis()),
        completed_at: None,
        thread_id: None,
        session_id: None,
        current_step_index: 0,
        current_step_title: Some("step-1".to_string()),
        current_step_kind: Some(GoalRunStepKind::Command),
        replan_count: 0,
        max_replans: 2,
        plan_summary: Some("plan".to_string()),
        reflection_summary: None,
        memory_updates: Vec::new(),
        generated_skill_path: None,
        last_error: None,
        failure_cause: None,
        child_task_ids: vec![task_id.to_string()],
        child_task_count: 1,
        approval_count: 0,
        awaiting_approval_id: Some(approval_id.to_string()),
        active_task_id: Some(task_id.to_string()),
        duration_ms: None,
        steps: vec![GoalRunStep {
            id: "step-1".to_string(),
            position: 0,
            title: "step-1".to_string(),
            instructions: "do supervised work".to_string(),
            kind: GoalRunStepKind::Command,
            success_criteria: "done".to_string(),
            session_id: None,
            status: GoalRunStepStatus::InProgress,
            task_id: Some(task_id.to_string()),
            summary: None,
            error: None,
            started_at: Some(now_millis()),
            completed_at: None,
        }],
        events: Vec::new(),
        total_prompt_tokens: 0,
        total_completion_tokens: 0,
        estimated_cost_usd: None,
        autonomy_level: super::autonomy::AutonomyLevel::Supervised,
        authorship_tag: None,
    }
}

async fn sample_awaiting_task(
    engine: &AgentEngine,
    goal_run_id: &str,
    task_id: &str,
    approval_id: &str,
) {
    engine.tasks.lock().await.push_back(AgentTask {
        id: task_id.to_string(),
        title: "step task".to_string(),
        description: "goal step work".to_string(),
        status: TaskStatus::AwaitingApproval,
        priority: TaskPriority::Normal,
        progress: 30,
        created_at: now_millis(),
        started_at: Some(now_millis()),
        completed_at: None,
        error: None,
        result: None,
        thread_id: None,
        source: "goal_run".to_string(),
        notify_on_complete: false,
        notify_channels: Vec::new(),
        dependencies: Vec::new(),
        command: None,
        session_id: None,
        goal_run_id: Some(goal_run_id.to_string()),
        goal_run_title: Some("supervised goal".to_string()),
        goal_step_id: Some("step-1".to_string()),
        goal_step_title: Some("step-1".to_string()),
        parent_task_id: None,
        parent_thread_id: None,
        runtime: "daemon".to_string(),
        retry_count: 0,
        max_retries: 3,
        next_retry_at: None,
        scheduled_at: None,
        blocked_reason: Some("awaiting supervised acknowledgment".to_string()),
        awaiting_approval_id: Some(approval_id.to_string()),
        lane_id: None,
        last_error: None,
        logs: Vec::new(),
        tool_whitelist: None,
        tool_blacklist: None,
        context_budget_tokens: None,
        context_overflow_action: None,
        termination_conditions: None,
        success_criteria: None,
        max_duration_secs: None,
        supervisor_config: None,
        override_provider: None,
        override_model: None,
        override_system_prompt: None,
        sub_agent_def_id: None,
    });
}

#[tokio::test]
async fn resume_does_not_clear_supervised_awaiting_approval_gate() {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let goal_run_id = "goal-supervised";
    let task_id = "task-supervised";
    let approval_id = "autonomy-ack-1";

    engine
        .goal_runs
        .lock()
        .await
        .push_back(sample_supervised_goal_run(
            goal_run_id,
            task_id,
            approval_id,
        ));
    sample_awaiting_task(&engine, goal_run_id, task_id, approval_id).await;

    let changed = engine.control_goal_run(goal_run_id, "resume", None).await;
    assert!(
        !changed,
        "resume should not mutate awaiting-approval supervised runs"
    );

    let goal = engine
        .get_goal_run(goal_run_id)
        .await
        .expect("goal should exist");
    assert_eq!(goal.status, GoalRunStatus::AwaitingApproval);
    assert_eq!(goal.awaiting_approval_id.as_deref(), Some(approval_id));

    let task = engine
        .tasks
        .lock()
        .await
        .iter()
        .find(|task| task.id == task_id)
        .cloned()
        .expect("task should exist");
    assert_eq!(task.status, TaskStatus::AwaitingApproval);
    assert_eq!(task.awaiting_approval_id.as_deref(), Some(approval_id));
}

#[tokio::test]
async fn explicit_acknowledgment_unblocks_goal_and_current_step_task() {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let goal_run_id = "goal-supervised";
    let task_id = "task-supervised";
    let approval_id = "autonomy-ack-2";

    engine
        .goal_runs
        .lock()
        .await
        .push_back(sample_supervised_goal_run(
            goal_run_id,
            task_id,
            approval_id,
        ));
    sample_awaiting_task(&engine, goal_run_id, task_id, approval_id).await;

    let changed = engine
        .control_goal_run(goal_run_id, "acknowledge", None)
        .await;
    assert!(changed, "acknowledge should clear supervised gate");

    let goal = engine
        .get_goal_run(goal_run_id)
        .await
        .expect("goal should exist");
    assert_eq!(goal.status, GoalRunStatus::Running);
    assert!(goal.awaiting_approval_id.is_none());

    let task = engine
        .tasks
        .lock()
        .await
        .iter()
        .find(|task| task.id == task_id)
        .cloned()
        .expect("task should exist");
    assert_eq!(task.status, TaskStatus::Queued);
    assert!(task.awaiting_approval_id.is_none());
}

#[tokio::test]
async fn cancelling_goal_run_settles_unresolved_goal_plan_trace() {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let goal_run_id = "goal-supervised-cancel";
    let task_id = "task-supervised-cancel";
    let approval_id = "autonomy-ack-cancel";

    engine
        .goal_runs
        .lock()
        .await
        .push_back(sample_supervised_goal_run(
            goal_run_id,
            task_id,
            approval_id,
        ));
    sample_awaiting_task(&engine, goal_run_id, task_id, approval_id).await;

    let selected_json = serde_json::json!({
        "option_type": "goal_plan",
        "reasoning": "Use a supervised single-step plan",
        "rejection_reason": null,
        "estimated_success_prob": 0.61,
        "arguments_hash": "ctx_hash"
    })
    .to_string();
    let unresolved =
        serde_json::to_string(&crate::agent::learning::traces::CausalTraceOutcome::Unresolved)
            .expect("serialize unresolved outcome");
    engine
        .history
        .insert_causal_trace(
            "causal_goal_plan_cancel_hook",
            None,
            Some(goal_run_id),
            None,
            "plan_selection",
            &selected_json,
            "[]",
            "ctx_hash",
            "[]",
            &unresolved,
            Some("gpt-4o-mini"),
            now_millis(),
        )
        .await
        .expect("insert goal plan causal trace");

    let changed = engine.control_goal_run(goal_run_id, "cancel", None).await;
    assert!(changed, "cancel should update goal state");

    let records = engine
        .history
        .list_recent_causal_trace_records("goal_plan", 1)
        .await
        .expect("list goal plan traces");
    let outcome = serde_json::from_str::<crate::agent::learning::traces::CausalTraceOutcome>(
        &records[0].outcome_json,
    )
    .expect("deserialize settled outcome");
    match outcome {
        crate::agent::learning::traces::CausalTraceOutcome::Failure { reason } => {
            assert!(reason.contains("cancelled"));
        }
        other => panic!("expected cancelled failure outcome, got {other:?}"),
    }
}

#[tokio::test]
async fn start_goal_run_records_goal_start_episode_with_archived_fields() {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

    let goal = engine
        .start_goal_run(
            "repair archived parity gaps".to_string(),
            Some("Repair parity".to_string()),
            Some("thread-epis-1".to_string()),
            Some("session-epis-1".to_string()),
            None,
            None,
            None,
        )
        .await;

    let episodes = engine
        .list_episodes_for_goal_run(&goal.id)
        .await
        .expect("episodes should load");
    assert_eq!(
        episodes.len(),
        1,
        "goal start should immediately record one episode"
    );

    let episode_json =
        serde_json::to_value(&episodes[0]).expect("episode should serialize to json");
    assert_eq!(episode_json["episode_type"], "goal_start");
    assert_eq!(episode_json["goal_text"], "repair archived parity gaps");
    assert_eq!(episode_json["goal_type"], "goal_run");
    assert_eq!(
        episode_json["summary"],
        "Repair parity: repair archived parity gaps"
    );
    assert!(
        episode_json.get("confidence_before").is_some(),
        "goal-start episodes should carry explicit confidence_before field"
    );
    assert!(
        episode_json.get("confidence_after").is_some(),
        "goal-start episodes should carry explicit confidence_after field"
    );
}

#[tokio::test]
async fn repeated_goal_start_creates_retry_link_to_previous_related_episode() {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

    let first = engine
        .start_goal_run(
            "repair archived parity gaps".to_string(),
            Some("Repair parity".to_string()),
            Some("thread-epis-2".to_string()),
            Some("session-epis-2".to_string()),
            None,
            None,
            None,
        )
        .await;
    engine
        .record_goal_episode(&first, crate::agent::episodic::EpisodeOutcome::Failure)
        .await
        .expect("first goal failure episode should record");

    let second = engine
        .start_goal_run(
            "repair archived parity gaps".to_string(),
            Some("Repair parity again".to_string()),
            Some("thread-epis-3".to_string()),
            Some("session-epis-3".to_string()),
            None,
            Some("req-2".to_string()),
            None,
        )
        .await;

    let episodes = engine
        .list_episodes_for_goal_run(&second.id)
        .await
        .expect("episodes should load");
    let start_episode = episodes
        .iter()
        .find(|episode| episode.episode_type == EpisodeType::GoalStart)
        .expect("second goal should have a goal_start episode");

    let links = engine
        .get_episode_links(&start_episode.id)
        .await
        .expect("links should load");
    assert!(
        links.iter().any(|link| link.link_type == LinkType::RetryOf),
        "repeated goal should link to the prior related episode as retry_of"
    );
}

#[tokio::test]
async fn repeated_goal_start_does_not_link_across_persona_scopes() {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

    run_with_agent_scope(DOMOWOJ_AGENT_ID.to_string(), async {
        let first = engine
            .start_goal_run(
                "repair archived parity gaps".to_string(),
                Some("Persona A parity".to_string()),
                Some("thread-epis-scope-1".to_string()),
                Some("session-epis-scope-1".to_string()),
                None,
                None,
                None,
            )
            .await;
        engine
            .record_goal_episode(&first, crate::agent::episodic::EpisodeOutcome::Failure)
            .await
            .expect("persona A goal failure episode should record");
    })
    .await;

    run_with_agent_scope(ROD_AGENT_ID.to_string(), async {
        let second = engine
            .start_goal_run(
                "repair archived parity gaps".to_string(),
                Some("Persona B parity".to_string()),
                Some("thread-epis-scope-2".to_string()),
                Some("session-epis-scope-2".to_string()),
                None,
                Some("req-persona-b".to_string()),
                None,
            )
            .await;

        let episodes = engine
            .list_episodes_for_goal_run(&second.id)
            .await
            .expect("persona B episodes should load");
        let start_episode = episodes
            .iter()
            .find(|episode| episode.episode_type == EpisodeType::GoalStart)
            .expect("persona B goal should have a goal_start episode");

        let links = engine
            .get_episode_links(&start_episode.id)
            .await
            .expect("persona B links should load");
        assert!(
            links.is_empty(),
            "persona-scoped goal-start episodes must not auto-link to another persona's history"
        );
    })
    .await;
}

#[tokio::test]
async fn suppressed_session_id_skips_goal_start_episode_recording() {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.episodic.suppressed_session_ids = vec!["session-suppressed".to_string()];
    let engine = AgentEngine::new_test(manager, config, root.path()).await;

    let goal = engine
        .start_goal_run(
            "do not persist this goal".to_string(),
            Some("Suppressed goal".to_string()),
            Some("thread-suppressed".to_string()),
            Some("session-suppressed".to_string()),
            None,
            None,
            None,
        )
        .await;

    let episodes = engine
        .list_episodes_for_goal_run(&goal.id)
        .await
        .expect("episodes should load");
    assert!(
        episodes.is_empty(),
        "suppressed session ids should prevent per-session episodic recording"
    );
}
