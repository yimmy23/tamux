use super::*;
use crate::session_manager::SessionManager;
use tempfile::tempdir;

fn goal_run_json(goal_run: &GoalRun) -> serde_json::Value {
    serde_json::to_value(goal_run).expect("serialize goal run")
}

fn runtime_owner_profile(
    agent_label: &str,
    provider: &str,
    model: &str,
    reasoning_effort: Option<&str>,
) -> crate::agent::types::GoalRuntimeOwnerProfile {
    crate::agent::types::GoalRuntimeOwnerProfile {
        agent_label: agent_label.to_string(),
        provider: provider.to_string(),
        model: model.to_string(),
        reasoning_effort: reasoning_effort.map(str::to_string),
    }
}

fn sample_goal_assignment(role_id: &str, provider: &str, model: &str) -> GoalAgentAssignment {
    GoalAgentAssignment {
        role_id: role_id.to_string(),
        enabled: true,
        provider: provider.to_string(),
        model: model.to_string(),
        reasoning_effort: Some("medium".to_string()),
        inherit_from_main: false,
    }
}

fn sample_goal_run(goal_run_id: &str) -> GoalRun {
    GoalRun {
        id: goal_run_id.to_string(),
        title: "supervised goal".to_string(),
        goal: "validate supervised gating".to_string(),
        client_request_id: None,
        status: GoalRunStatus::Running,
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
        planner_owner_profile: None,
        current_step_owner_profile: None,
        replan_count: 0,
        max_replans: 2,
        plan_summary: Some("plan".to_string()),
        reflection_summary: None,
        memory_updates: Vec::new(),
        generated_skill_path: None,
        last_error: None,
        failure_cause: None,
        stopped_reason: None,
        child_task_ids: Vec::new(),
        child_task_count: 0,
        approval_count: 0,
        awaiting_approval_id: None,
        policy_fingerprint: None,
        approval_expires_at: None,
        containment_scope: None,
        compensation_status: None,
        compensation_summary: None,
        active_task_id: None,
        duration_ms: None,
        steps: vec![GoalRunStep {
            id: "step-1".to_string(),
            position: 0,
            title: "step-1".to_string(),
            instructions: "do supervised work".to_string(),
            kind: GoalRunStepKind::Command,
            success_criteria: "done".to_string(),
            session_id: None,
            status: GoalRunStepStatus::Pending,
            task_id: None,
            summary: None,
            error: None,
            started_at: None,
            completed_at: None,
        }],
        events: Vec::new(),
        dossier: None,
        total_prompt_tokens: 0,
        total_completion_tokens: 0,
        estimated_cost_usd: None,
        autonomy_level: super::autonomy::AutonomyLevel::Supervised,
        authorship_tag: None,
        launch_assignment_snapshot: Vec::new(),
        runtime_assignment_list: Vec::new(),
        root_thread_id: None,
        active_thread_id: None,
        execution_thread_ids: Vec::new(),
    }
}

#[tokio::test]
async fn start_goal_run_seeds_launch_assignment_snapshot_and_thread_routing_defaults() {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let thread_id = "thread-goal-launch-routing".to_string();

    let goal_run = engine
        .start_goal_run(
            "validate mission control launch metadata".to_string(),
            Some("mission control launch".to_string()),
            Some(thread_id.clone()),
            None,
            None,
            None,
            None,
            None,
        )
        .await;

    let config = engine.config.read().await;
    let expected_provider =
        resolve_active_provider_config(&config).expect("active provider should resolve");
    let expected_assignment = GoalAgentAssignment {
        role_id: crate::agent::agent_identity::MAIN_AGENT_ID.to_string(),
        enabled: true,
        provider: config.provider.clone(),
        model: expected_provider.model.clone(),
        reasoning_effort: Some(expected_provider.reasoning_effort.clone()),
        inherit_from_main: false,
    };

    assert_eq!(goal_run.root_thread_id.as_deref(), Some(thread_id.as_str()));
    assert_eq!(
        goal_run.active_thread_id.as_deref(),
        Some(thread_id.as_str())
    );
    assert_eq!(goal_run.execution_thread_ids, vec![thread_id.clone()]);
    assert_eq!(
        goal_run.launch_assignment_snapshot,
        vec![expected_assignment.clone()]
    );
    assert_eq!(goal_run.runtime_assignment_list, vec![expected_assignment]);

    let serialized = goal_run_json(&goal_run);
    assert_eq!(serialized["root_thread_id"], serde_json::json!(thread_id));
    assert_eq!(serialized["active_thread_id"], serde_json::json!(thread_id));
    assert_eq!(
        serialized["execution_thread_ids"],
        serde_json::json!([thread_id])
    );
    assert_eq!(
        serialized["launch_assignment_snapshot"][0]["provider"],
        serde_json::json!(config.provider.clone())
    );
}

#[tokio::test]
async fn start_goal_run_uses_provided_launch_assignment_snapshot_when_present() {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let launch_assignments = vec![
        GoalAgentAssignment {
            role_id: crate::agent::agent_identity::MAIN_AGENT_ID.to_string(),
            enabled: true,
            provider: "openai".to_string(),
            model: "gpt-5.4".to_string(),
            reasoning_effort: Some("medium".to_string()),
            inherit_from_main: false,
        },
        GoalAgentAssignment {
            role_id: "planning".to_string(),
            enabled: true,
            provider: "openai".to_string(),
            model: "gpt-5.4-mini".to_string(),
            reasoning_effort: Some("high".to_string()),
            inherit_from_main: false,
        },
    ];

    let goal_run = engine
        .start_goal_run_with_surface(
            "validate launch roster".to_string(),
            Some("launch roster".to_string()),
            None,
            None,
            None,
            None,
            None,
            None,
            Some(launch_assignments.clone()),
        )
        .await;

    assert_eq!(goal_run.launch_assignment_snapshot, launch_assignments);
    assert_eq!(
        goal_run.runtime_assignment_list,
        goal_run.launch_assignment_snapshot
    );
}

#[tokio::test]
async fn sync_goal_run_with_task_advances_active_thread_and_execution_thread_list() {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let goal_run_id = "goal-thread-routing";
    let mut goal_run = sample_goal_run(goal_run_id);
    goal_run.root_thread_id = Some("thread-root".to_string());
    goal_run.active_thread_id = Some("thread-root".to_string());
    goal_run.execution_thread_ids = vec!["thread-root".to_string()];
    goal_run.thread_id = Some("thread-root".to_string());
    engine.goal_runs.lock().await.push_back(goal_run);

    let task = AgentTask {
        id: "task-thread-routing".to_string(),
        title: "step task".to_string(),
        description: "step task".to_string(),
        status: TaskStatus::InProgress,
        priority: TaskPriority::Normal,
        progress: 10,
        created_at: now_millis(),
        started_at: Some(now_millis()),
        completed_at: None,
        error: None,
        result: None,
        thread_id: Some("thread-specialist".to_string()),
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
        max_retries: 0,
        next_retry_at: None,
        scheduled_at: None,
        blocked_reason: None,
        awaiting_approval_id: None,
        policy_fingerprint: None,
        approval_expires_at: None,
        containment_scope: None,
        compensation_status: None,
        compensation_summary: None,
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
        sub_agent_def_id: Some("android-verifier".to_string()),
    };

    engine.sync_goal_run_with_task(goal_run_id, &task).await;

    let updated = engine
        .get_goal_run(goal_run_id)
        .await
        .expect("goal run should still exist");
    assert_eq!(updated.thread_id.as_deref(), Some("thread-specialist"));
    assert_eq!(
        updated.active_thread_id.as_deref(),
        Some("thread-specialist")
    );
    assert_eq!(
        updated.execution_thread_ids,
        vec!["thread-root".to_string(), "thread-specialist".to_string()]
    );
}

#[tokio::test]
async fn fail_goal_run_advances_active_thread_and_execution_thread_list() {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let goal_run_id = "goal-thread-routing-failure";
    let mut goal_run = sample_goal_run(goal_run_id);
    goal_run.root_thread_id = Some("thread-root".to_string());
    goal_run.active_thread_id = Some("thread-root".to_string());
    goal_run.execution_thread_ids = vec!["thread-root".to_string()];
    goal_run.thread_id = Some("thread-root".to_string());
    engine.goal_runs.lock().await.push_back(goal_run);

    engine
        .fail_goal_run(
            goal_run_id,
            "specialist execution failed",
            "execution",
            Some("thread-specialist".to_string()),
        )
        .await;

    let updated = engine
        .get_goal_run(goal_run_id)
        .await
        .expect("goal run should still exist");
    assert_eq!(updated.thread_id.as_deref(), Some("thread-specialist"));
    assert_eq!(
        updated.active_thread_id.as_deref(),
        Some("thread-specialist")
    );
    assert_eq!(
        updated.execution_thread_ids,
        vec!["thread-root".to_string(), "thread-specialist".to_string()]
    );
}

#[tokio::test]
async fn enqueue_goal_run_step_marks_supervised_task_as_awaiting_approval_before_dispatch() {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let goal_run_id = "goal-supervised";

    engine
        .goal_runs
        .lock()
        .await
        .push_back(sample_goal_run(goal_run_id));

    engine
        .enqueue_goal_run_step(goal_run_id)
        .await
        .expect("enqueue should succeed");

    let goal = engine
        .get_goal_run(goal_run_id)
        .await
        .expect("goal should exist");
    let tasks = engine.tasks.lock().await;
    let step_task_id = goal.steps[0]
        .task_id
        .clone()
        .expect("step should be linked to a task");
    let step_task = tasks
        .iter()
        .find(|task| task.id == step_task_id)
        .cloned()
        .expect("step task should exist");

    assert_eq!(goal.status, GoalRunStatus::AwaitingApproval);
    assert!(
        goal.awaiting_approval_id.is_some(),
        "supervised gate should assign an approval id on goal run"
    );
    assert_eq!(step_task.status, TaskStatus::AwaitingApproval);
    assert_eq!(
        step_task.awaiting_approval_id, goal.awaiting_approval_id,
        "task and goal should share the same gate identifier"
    );
}

#[tokio::test]
async fn fail_goal_run_settles_unresolved_goal_replan_trace() {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let goal_run_id = "goal-replan-failure";

    engine
        .goal_runs
        .lock()
        .await
        .push_back(sample_goal_run(goal_run_id));

    let selected_json = serde_json::json!({
        "option_type": "goal_replan",
        "reasoning": "Retry with a narrower command sequence",
        "rejection_reason": null,
        "estimated_success_prob": 0.58,
        "arguments_hash": "ctx_hash"
    })
    .to_string();
    let unresolved =
        serde_json::to_string(&crate::agent::learning::traces::CausalTraceOutcome::Unresolved)
            .expect("serialize unresolved outcome");
    engine
        .history
        .insert_causal_trace(
            "causal_goal_replan_failure_hook",
            None,
            Some(goal_run_id),
            None,
            "replan_selection",
            &selected_json,
            "[]",
            "ctx_hash",
            "[]",
            &unresolved,
            Some("gpt-4o-mini"),
            now_millis(),
        )
        .await
        .expect("insert goal replan causal trace");

    engine
        .fail_goal_run(
            goal_run_id,
            "managed command failed permanently",
            "execution",
            None,
        )
        .await;

    let failed_goal = engine
        .get_goal_run(goal_run_id)
        .await
        .expect("failed goal should still exist");
    let dossier = failed_goal
        .dossier
        .expect("failed goal should persist dossier report");
    assert_eq!(
        dossier
            .report
            .as_ref()
            .expect("failed goal should capture goal report")
            .state,
        GoalProjectionState::Failed
    );
    assert_eq!(
        dossier
            .latest_resume_decision
            .as_ref()
            .expect("failed goal should record terminal decision")
            .action,
        GoalResumeAction::Stop
    );
    assert_eq!(
        dossier
            .latest_resume_decision
            .as_ref()
            .expect("failed goal should record reason code")
            .reason_code,
        "goal_failed"
    );

    let records = engine
        .history
        .list_recent_causal_trace_records("goal_replan", 1)
        .await
        .expect("list goal replan traces");
    let outcome = serde_json::from_str::<crate::agent::learning::traces::CausalTraceOutcome>(
        &records[0].outcome_json,
    )
    .expect("deserialize settled outcome");
    match outcome {
        crate::agent::learning::traces::CausalTraceOutcome::Failure { reason } => {
            assert!(reason.contains("managed command failed permanently"));
        }
        other => panic!("expected failure outcome, got {other:?}"),
    }
}

#[tokio::test]
async fn fail_goal_run_appends_failure_factor_to_goal_replan_trace() {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let goal_run_id = "goal-replan-failure-factor";

    engine
        .goal_runs
        .lock()
        .await
        .push_back(sample_goal_run(goal_run_id));

    let selected_json = serde_json::json!({
        "option_type": "goal_replan",
        "reasoning": "Retry with a narrower command sequence",
        "rejection_reason": null,
        "estimated_success_prob": 0.58,
        "arguments_hash": "ctx_hash"
    })
    .to_string();
    let unresolved =
        serde_json::to_string(&crate::agent::learning::traces::CausalTraceOutcome::Unresolved)
            .expect("serialize unresolved outcome");
    let factors_json = serde_json::to_string(&vec![crate::agent::learning::traces::CausalFactor {
        factor_type: crate::agent::learning::traces::FactorType::PatternMatch,
        description: "replan used a smaller command sequence".to_string(),
        weight: 0.7,
    }])
    .expect("serialize factors");
    engine
        .history
        .insert_causal_trace(
            "causal_goal_replan_failure_factor_hook",
            None,
            Some(goal_run_id),
            None,
            "replan_selection",
            &selected_json,
            "[]",
            "ctx_hash",
            &factors_json,
            &unresolved,
            Some("gpt-4o-mini"),
            now_millis(),
        )
        .await
        .expect("insert goal replan causal trace");

    engine
        .fail_goal_run(
            goal_run_id,
            "managed command failed permanently",
            "execution",
            None,
        )
        .await;

    let records = engine
        .history
        .list_recent_causal_trace_records("goal_replan", 1)
        .await
        .expect("list goal replan traces");
    let factors = serde_json::from_str::<Vec<crate::agent::learning::traces::CausalFactor>>(
        &records[0].causal_factors_json,
    )
    .expect("deserialize causal factors");
    assert!(
        factors.iter().any(|factor| matches!(
            factor.factor_type,
            crate::agent::learning::traces::FactorType::PastFailure
        ) && factor
            .description
            .contains("managed command failed permanently")),
        "expected settled goal replan trace to append a final failure factor"
    );
}

#[tokio::test]
async fn plan_goal_run_preserves_planner_owner_profile_when_plan_request_fails() {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.provider = "openai".to_string();
    config.model = "gpt-4o-mini".to_string();
    config.api_key = "test-key".to_string();
    config.api_transport = ApiTransport::ChatCompletions;
    config.auto_retry = false;
    config.max_retries = 0;
    config.max_tool_loops = 1;
    config.base_url = "http://127.0.0.1:9/v1".to_string();
    let expected_provider = config.provider.clone();
    let expected_model = config.model.clone();
    let expected_reasoning = Some(config.reasoning_effort.clone());
    let engine = AgentEngine::new_test(manager, config.clone(), root.path()).await;

    let goal_run_id = "goal-plan-failure-owner";
    let mut goal_run = sample_goal_run_with_kind(
        goal_run_id,
        GoalRunStepKind::Command,
        "Run the planned work",
    );
    goal_run.status = GoalRunStatus::Queued;
    goal_run.steps.clear();
    goal_run.current_step_index = 0;
    goal_run.current_step_title = None;
    goal_run.current_step_kind = None;
    engine.goal_runs.lock().await.push_back(goal_run);

    let result = engine.plan_goal_run(goal_run_id).await;
    assert!(
        result.is_err(),
        "planning should fail against a dead endpoint"
    );

    let persisted = engine
        .history
        .list_goal_runs()
        .await
        .expect("goal runs should be readable from history")
        .into_iter()
        .find(|goal_run| goal_run.id == goal_run_id)
        .expect("goal run should still exist after failed planning");
    let planner_owner = persisted
        .planner_owner_profile
        .as_ref()
        .expect("failed planning should retain planner attribution");
    assert_eq!(
        planner_owner,
        &runtime_owner_profile(
            crate::agent::agent_identity::MAIN_AGENT_NAME,
            &expected_provider,
            &expected_model,
            expected_reasoning.as_deref()
        )
    );
    assert_eq!(persisted.status, GoalRunStatus::Planning);
}

#[tokio::test]
async fn handle_goal_run_step_failure_preserves_planner_owner_profile_when_replan_request_fails() {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.provider = "openai".to_string();
    config.model = "gpt-4o-mini".to_string();
    config.api_key = "test-key".to_string();
    config.api_transport = ApiTransport::ChatCompletions;
    config.auto_retry = false;
    config.max_retries = 0;
    config.max_tool_loops = 1;
    config.base_url = "http://127.0.0.1:9/v1".to_string();
    let expected_provider = config.provider.clone();
    let expected_model = config.model.clone();
    let expected_reasoning = Some(config.reasoning_effort.clone());
    let engine = AgentEngine::new_test(manager, config.clone(), root.path()).await;

    let goal_run_id = "goal-replan-failure-owner";
    let mut goal_run = sample_goal_run_with_kind(
        goal_run_id,
        GoalRunStepKind::Command,
        "Run the failing command and recover",
    );
    goal_run.thread_id = Some("thread-replan-owner".to_string());
    goal_run.current_step_index = 0;
    goal_run.current_step_title = Some("step-1".to_string());
    goal_run.current_step_kind = Some(GoalRunStepKind::Command);
    engine.goal_runs.lock().await.push_back(goal_run.clone());

    let failed_task = AgentTask {
        id: "task-replan-owner".to_string(),
        title: "failed step".to_string(),
        description: "failed step".to_string(),
        status: TaskStatus::Failed,
        priority: TaskPriority::Normal,
        progress: 0,
        created_at: now_millis(),
        started_at: Some(now_millis().saturating_sub(5_000)),
        completed_at: Some(now_millis()),
        error: Some("managed command failed permanently".to_string()),
        result: None,
        thread_id: Some("thread-replan-owner".to_string()),
        source: "goal_run".to_string(),
        notify_on_complete: false,
        notify_channels: Vec::new(),
        dependencies: Vec::new(),
        command: None,
        session_id: None,
        goal_run_id: Some(goal_run_id.to_string()),
        goal_run_title: Some(goal_run.title.clone()),
        goal_step_id: Some("step-1".to_string()),
        goal_step_title: Some("step-1".to_string()),
        parent_task_id: None,
        parent_thread_id: None,
        runtime: "daemon".to_string(),
        retry_count: 0,
        max_retries: 0,
        next_retry_at: None,
        scheduled_at: None,
        blocked_reason: None,
        awaiting_approval_id: None,
        policy_fingerprint: None,
        approval_expires_at: None,
        containment_scope: None,
        compensation_status: None,
        compensation_summary: None,
        lane_id: None,
        last_error: Some("managed command failed permanently".to_string()),
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
    };

    let result = engine
        .handle_goal_run_step_failure(goal_run_id, &failed_task)
        .await;
    assert!(
        result.is_err(),
        "replan should fail against a dead endpoint"
    );

    let persisted = engine
        .history
        .list_goal_runs()
        .await
        .expect("goal runs should be readable from history")
        .into_iter()
        .find(|goal_run| goal_run.id == goal_run_id)
        .expect("goal run should still exist after failed replan");
    let planner_owner = persisted
        .planner_owner_profile
        .as_ref()
        .expect("failed replan should retain planner attribution");
    assert_eq!(
        planner_owner,
        &runtime_owner_profile(
            crate::agent::agent_identity::MAIN_AGENT_NAME,
            &expected_provider,
            &expected_model,
            expected_reasoning.as_deref()
        )
    );
    assert_eq!(persisted.status, GoalRunStatus::Running);
}

#[tokio::test]
async fn sync_goal_run_with_task_emits_owner_only_changes_and_persists_them() {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let goal_run_id = "goal-sync-owner-change";
    let task_id = "task-sync-owner-change";
    let mut goal_run = sample_goal_run_with_kind(
        goal_run_id,
        GoalRunStepKind::Command,
        "Run the current step",
    );
    goal_run.status = GoalRunStatus::Running;
    goal_run.started_at = Some(now_millis());
    goal_run.active_task_id = Some(task_id.to_string());
    goal_run.current_step_title = Some("step-1".to_string());
    goal_run.current_step_kind = Some(GoalRunStepKind::Command);
    goal_run.current_step_owner_profile = None;
    if let Some(step) = goal_run.steps.get_mut(0) {
        step.status = GoalRunStepStatus::InProgress;
        step.started_at = Some(now_millis());
        step.task_id = Some(task_id.to_string());
    }
    engine.goal_runs.lock().await.push_back(goal_run);

    let mut events = engine.subscribe();
    let task = AgentTask {
        id: task_id.to_string(),
        title: "run step".to_string(),
        description: "run step".to_string(),
        status: TaskStatus::InProgress,
        priority: TaskPriority::Normal,
        progress: 25,
        created_at: now_millis(),
        started_at: Some(now_millis()),
        completed_at: None,
        error: None,
        result: None,
        thread_id: Some("thread-sync-owner-change".to_string()),
        source: "goal_run".to_string(),
        notify_on_complete: false,
        notify_channels: Vec::new(),
        dependencies: Vec::new(),
        command: None,
        session_id: None,
        goal_run_id: Some(goal_run_id.to_string()),
        goal_run_title: Some("goal with custom step".to_string()),
        goal_step_id: Some("step-1".to_string()),
        goal_step_title: Some("step-1".to_string()),
        parent_task_id: None,
        parent_thread_id: None,
        runtime: "daemon".to_string(),
        retry_count: 0,
        max_retries: 0,
        next_retry_at: None,
        scheduled_at: None,
        blocked_reason: None,
        awaiting_approval_id: None,
        policy_fingerprint: None,
        approval_expires_at: None,
        containment_scope: None,
        compensation_status: None,
        compensation_summary: None,
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
        override_provider: Some("anthropic".to_string()),
        override_model: Some("claude-3.5-sonnet".to_string()),
        override_system_prompt: None,
        sub_agent_def_id: None,
    };

    engine.sync_goal_run_with_task(goal_run_id, &task).await;

    let emitted = tokio::time::timeout(std::time::Duration::from_secs(1), events.recv())
        .await
        .expect("goal run update should be emitted")
        .expect("goal run update should arrive");
    match emitted {
        AgentEvent::GoalRunUpdate {
            goal_run: Some(goal_run),
            ..
        } => {
            let owner = goal_run
                .current_step_owner_profile
                .as_ref()
                .expect("goal run update should include owner metadata");
            assert_eq!(
                owner,
                &runtime_owner_profile(
                    crate::agent::agent_identity::MAIN_AGENT_NAME,
                    "anthropic",
                    "claude-3.5-sonnet",
                    Some("high")
                )
            );
        }
        other => panic!("expected goal run update, got {other:?}"),
    }

    let persisted = engine
        .history
        .list_goal_runs()
        .await
        .expect("goal runs should be readable from history")
        .into_iter()
        .find(|goal_run| goal_run.id == goal_run_id)
        .expect("goal run should exist after sync");
    assert_eq!(
        persisted
            .current_step_owner_profile
            .as_ref()
            .expect("owner metadata should persist"),
        &runtime_owner_profile(
            crate::agent::agent_identity::MAIN_AGENT_NAME,
            "anthropic",
            "claude-3.5-sonnet",
            Some("high")
        )
    );
}

#[tokio::test]
async fn sync_goal_run_with_task_preserves_captured_subagent_owner_profile_after_registry_change() {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.sub_agents.push(SubAgentDefinition {
        id: "android-verifier".to_string(),
        name: "Android Verifier".to_string(),
        provider: "openai".to_string(),
        model: "gpt-4o-mini".to_string(),
        role: Some("verification specialist".to_string()),
        system_prompt: Some("Verify Android build artifacts.".to_string()),
        tool_whitelist: None,
        tool_blacklist: None,
        context_budget_tokens: None,
        max_duration_secs: None,
        supervisor_config: None,
        enabled: true,
        builtin: false,
        immutable_identity: false,
        disable_allowed: true,
        delete_allowed: true,
        protected_reason: None,
        reasoning_effort: None,
        created_at: now_millis(),
    });
    let engine = AgentEngine::new_test(manager, config, root.path()).await;
    let goal_run_id = "goal-sync-owner-stability";
    let task_id = "task-sync-owner-stability";

    let mut goal_run = sample_goal_run_with_kind(
        goal_run_id,
        GoalRunStepKind::Command,
        "Run the current step",
    );
    goal_run.status = GoalRunStatus::Running;
    goal_run.started_at = Some(now_millis());
    goal_run.active_task_id = Some(task_id.to_string());
    goal_run.current_step_title = Some("step-1".to_string());
    goal_run.current_step_kind = Some(GoalRunStepKind::Command);
    goal_run.current_step_owner_profile = Some(runtime_owner_profile(
        "Android Verifier",
        "openai",
        "gpt-4o-mini",
        None,
    ));
    if let Some(step) = goal_run.steps.get_mut(0) {
        step.status = GoalRunStepStatus::Pending;
        step.task_id = Some(task_id.to_string());
    }
    engine.goal_runs.lock().await.push_back(goal_run);

    {
        let mut live_config = engine.config.write().await;
        if let Some(def) = live_config
            .sub_agents
            .iter_mut()
            .find(|definition| definition.id == "android-verifier")
        {
            def.provider = "anthropic".to_string();
            def.model = "claude-3.5-sonnet".to_string();
            def.reasoning_effort = Some("low".to_string());
        }
    }

    let task = AgentTask {
        id: task_id.to_string(),
        title: "run step".to_string(),
        description: "run step".to_string(),
        status: TaskStatus::InProgress,
        priority: TaskPriority::Normal,
        progress: 25,
        created_at: now_millis(),
        started_at: Some(now_millis()),
        completed_at: None,
        error: None,
        result: None,
        thread_id: Some("thread-sync-owner-stability".to_string()),
        source: "goal_run".to_string(),
        notify_on_complete: false,
        notify_channels: Vec::new(),
        dependencies: Vec::new(),
        command: None,
        session_id: None,
        goal_run_id: Some(goal_run_id.to_string()),
        goal_run_title: Some("goal with custom step".to_string()),
        goal_step_id: Some("step-1".to_string()),
        goal_step_title: Some("step-1".to_string()),
        parent_task_id: None,
        parent_thread_id: None,
        runtime: "daemon".to_string(),
        retry_count: 0,
        max_retries: 0,
        next_retry_at: None,
        scheduled_at: None,
        blocked_reason: None,
        awaiting_approval_id: None,
        policy_fingerprint: None,
        approval_expires_at: None,
        containment_scope: None,
        compensation_status: None,
        compensation_summary: None,
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
        sub_agent_def_id: Some("android-verifier".to_string()),
    };

    engine.sync_goal_run_with_task(goal_run_id, &task).await;

    let updated = engine
        .get_goal_run(goal_run_id)
        .await
        .expect("goal run should still exist");
    assert_eq!(
        updated
            .current_step_owner_profile
            .as_ref()
            .expect("captured owner metadata should remain stable"),
        &runtime_owner_profile("Android Verifier", "openai", "gpt-4o-mini", None)
    );
}

#[tokio::test]
async fn requeue_goal_run_step_clears_current_step_owner_profile() {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let goal_run_id = "goal-requeue-owner";
    let mut goal_run = sample_goal_run_with_kind(
        goal_run_id,
        GoalRunStepKind::Command,
        "Run the current step",
    );
    goal_run.active_task_id = Some("task-requeue-owner".to_string());
    goal_run.current_step_owner_profile = Some(runtime_owner_profile(
        crate::agent::agent_identity::MAIN_AGENT_NAME,
        "openai",
        "gpt-4o-mini",
        Some("high"),
    ));
    if let Some(step) = goal_run.steps.get_mut(0) {
        step.task_id = Some("task-requeue-owner".to_string());
        step.status = GoalRunStepStatus::InProgress;
    }
    engine.goal_runs.lock().await.push_back(goal_run);

    engine
        .requeue_goal_run_step(goal_run_id, "task vanished")
        .await;

    let updated = engine
        .get_goal_run(goal_run_id)
        .await
        .expect("goal run should still exist");
    assert!(updated.current_step_owner_profile.is_none());
    assert!(updated.active_task_id.is_none());
    assert!(updated.steps[0].task_id.is_none());
    assert_eq!(updated.steps[0].status, GoalRunStepStatus::Pending);
}

fn sample_goal_run_with_kind(
    goal_run_id: &str,
    kind: GoalRunStepKind,
    instructions: &str,
) -> GoalRun {
    GoalRun {
        id: goal_run_id.to_string(),
        title: "goal with custom step".to_string(),
        goal: "validate custom step routing".to_string(),
        client_request_id: None,
        status: GoalRunStatus::Running,
        priority: TaskPriority::Normal,
        created_at: now_millis(),
        updated_at: now_millis(),
        started_at: Some(now_millis()),
        completed_at: None,
        thread_id: Some("thread-goal-custom".to_string()),
        session_id: None,
        current_step_index: 0,
        current_step_title: Some("step-1".to_string()),
        current_step_kind: Some(kind.clone()),
        planner_owner_profile: None,
        current_step_owner_profile: None,
        replan_count: 0,
        max_replans: 2,
        plan_summary: Some("plan".to_string()),
        reflection_summary: None,
        memory_updates: Vec::new(),
        generated_skill_path: None,
        last_error: None,
        failure_cause: None,
        stopped_reason: None,
        child_task_ids: Vec::new(),
        child_task_count: 0,
        approval_count: 0,
        awaiting_approval_id: None,
        policy_fingerprint: None,
        approval_expires_at: None,
        containment_scope: None,
        compensation_status: None,
        compensation_summary: None,
        active_task_id: None,
        duration_ms: None,
        steps: vec![GoalRunStep {
            id: "step-1".to_string(),
            position: 0,
            title: "step-1".to_string(),
            instructions: instructions.to_string(),
            kind,
            success_criteria: "done".to_string(),
            session_id: None,
            status: GoalRunStepStatus::Pending,
            task_id: None,
            summary: None,
            error: None,
            started_at: None,
            completed_at: None,
        }],
        events: Vec::new(),
        dossier: None,
        total_prompt_tokens: 0,
        total_completion_tokens: 0,
        estimated_cost_usd: None,
        autonomy_level: super::autonomy::AutonomyLevel::Aware,
        authorship_tag: None,
        launch_assignment_snapshot: Vec::new(),
        runtime_assignment_list: Vec::new(),
        root_thread_id: None,
        active_thread_id: None,
        execution_thread_ids: Vec::new(),
    }
}

async fn write_step_completion_marker(engine: &AgentEngine, goal_run_id: &str, step_index: usize) {
    let path = crate::agent::goal_dossier::goal_step_completion_marker_path(
        &engine.data_dir,
        goal_run_id,
        step_index,
    );
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .expect("create marker parent dir");
    }
    tokio::fs::write(&path, format!("step {} complete\n", step_index + 1))
        .await
        .expect("write step completion marker");
}

#[test]
fn goal_step_completion_marker_path_uses_human_step_number() {
    let marker =
        crate::agent::goal_dossier::goal_step_completion_marker_relative_path("goal-marker", 0);
    assert_eq!(
        marker.to_string_lossy(),
        ".tamux/goals/goal-marker/inventory/execution/step-1-complete.md"
    );
}

#[tokio::test]
async fn enqueue_goal_run_step_applies_goal_local_overrides_for_builtin_binding() {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let goal_run_id = "goal-local-command";

    let mut goal_run = sample_goal_run_with_kind(
        goal_run_id,
        GoalRunStepKind::Command,
        "Break work into a release plan",
    );
    goal_run.launch_assignment_snapshot =
        vec![sample_goal_assignment("planning", "openai", "gpt-5.4-mini")];
    goal_run.runtime_assignment_list = goal_run.launch_assignment_snapshot.clone();
    goal_run.dossier = Some(GoalRunDossier {
        units: vec![GoalDeliveryUnit {
            id: "step-1".to_string(),
            title: "step-1".to_string(),
            status: GoalProjectionState::Pending,
            execution_binding: GoalRoleBinding::Builtin("planning".to_string()),
            verification_binding: GoalRoleBinding::Builtin(
                crate::agent::agent_identity::MAIN_AGENT_ID.to_string(),
            ),
            summary: None,
            proof_checks: Vec::new(),
            evidence: Vec::new(),
            report: None,
        }],
        ..Default::default()
    });
    engine.goal_runs.lock().await.push_back(goal_run);

    engine
        .enqueue_goal_run_step(goal_run_id)
        .await
        .expect("enqueue should succeed");

    let updated = engine
        .get_goal_run(goal_run_id)
        .await
        .expect("goal should still exist");
    let task_id = updated.steps[0]
        .task_id
        .clone()
        .expect("step should link a task");
    let task = engine
        .tasks
        .lock()
        .await
        .iter()
        .find(|task| task.id == task_id)
        .cloned()
        .expect("goal task should exist");

    assert_eq!(task.override_provider.as_deref(), Some("openai"));
    assert_eq!(task.override_model.as_deref(), Some("gpt-5.4-mini"));
    assert!(task.sub_agent_def_id.is_none());
}

#[tokio::test]
async fn enqueue_goal_run_specialist_step_uses_goal_local_assignment_overrides() {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let goal_run_id = "goal-local-specialist";

    let mut goal_run = sample_goal_run_with_kind(
        goal_run_id,
        GoalRunStepKind::Specialist("planning".to_string()),
        "Break work into a release plan",
    );
    goal_run.launch_assignment_snapshot =
        vec![sample_goal_assignment("planning", "openai", "gpt-5.4-mini")];
    goal_run.runtime_assignment_list = goal_run.launch_assignment_snapshot.clone();
    engine.goal_runs.lock().await.push_back(goal_run);

    engine
        .enqueue_goal_run_step(goal_run_id)
        .await
        .expect("enqueue should succeed");

    let updated = engine
        .get_goal_run(goal_run_id)
        .await
        .expect("goal should still exist");
    let task_id = updated.steps[0]
        .task_id
        .clone()
        .expect("step should link a task");
    let task = engine
        .tasks
        .lock()
        .await
        .iter()
        .find(|task| task.id == task_id)
        .cloned()
        .expect("goal task should exist");

    assert_eq!(task.source, "handoff");
    assert_eq!(task.override_model.as_deref(), Some("gpt-5.4-mini"));
    assert!(task.sub_agent_def_id.is_none());
    assert!(
        engine
            .resolve_handoff_log_id_by_task_id(&task.id)
            .await
            .expect("handoff lookup should succeed")
            .is_some(),
        "goal-local specialist should still persist handoff linkage"
    );
}

#[tokio::test]
async fn current_step_owner_profile_reports_goal_local_agent_details() {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let goal_run_id = "goal-local-owner-profile";

    let mut goal_run = sample_goal_run_with_kind(
        goal_run_id,
        GoalRunStepKind::Command,
        "Break work into a release plan",
    );
    goal_run.launch_assignment_snapshot =
        vec![sample_goal_assignment("planning", "openai", "gpt-5.4-mini")];
    goal_run.runtime_assignment_list = goal_run.launch_assignment_snapshot.clone();
    goal_run.dossier = Some(GoalRunDossier {
        units: vec![GoalDeliveryUnit {
            id: "step-1".to_string(),
            title: "step-1".to_string(),
            status: GoalProjectionState::Pending,
            execution_binding: GoalRoleBinding::Builtin("planning".to_string()),
            verification_binding: GoalRoleBinding::Builtin(
                crate::agent::agent_identity::MAIN_AGENT_ID.to_string(),
            ),
            summary: None,
            proof_checks: Vec::new(),
            evidence: Vec::new(),
            report: None,
        }],
        ..Default::default()
    });
    engine.goal_runs.lock().await.push_back(goal_run);

    engine
        .enqueue_goal_run_step(goal_run_id)
        .await
        .expect("enqueue should succeed");

    let updated = engine
        .get_goal_run(goal_run_id)
        .await
        .expect("goal should still exist");
    let owner = updated
        .current_step_owner_profile
        .expect("goal should expose local owner profile");
    assert_eq!(owner.agent_label, "Planning");
    assert_eq!(owner.provider, "openai");
    assert_eq!(owner.model, "gpt-5.4-mini");
    assert_eq!(owner.reasoning_effort.as_deref(), Some("medium"));
}

#[tokio::test]
async fn implementation_completion_queues_verifier_before_advancing_goal_step() {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let goal_run_id = "goal-step-verification-queue";

    let mut goal_run = sample_goal_run_with_kind(
        goal_run_id,
        GoalRunStepKind::Command,
        "Build the Android artifact",
    );
    goal_run.steps[0].status = GoalRunStepStatus::InProgress;
    goal_run.steps[0].task_id = Some("task-impl".to_string());
    goal_run.dossier = Some(GoalRunDossier {
        units: vec![GoalDeliveryUnit {
            id: "step-1".to_string(),
            title: "step-1".to_string(),
            status: GoalProjectionState::InProgress,
            execution_binding: GoalRoleBinding::Builtin("swarog".to_string()),
            verification_binding: GoalRoleBinding::Subagent("android-verifier".to_string()),
            summary: None,
            proof_checks: vec![GoalProofCheck {
                id: "proof-build-debug".to_string(),
                title: "Debug build succeeds".to_string(),
                state: GoalProjectionState::Pending,
                summary: None,
                evidence_ids: Vec::new(),
                resolved_at: None,
            }],
            evidence: Vec::new(),
            report: None,
        }],
        ..Default::default()
    });
    engine.goal_runs.lock().await.push_back(goal_run);

    let completed_task = AgentTask {
        id: "task-impl".to_string(),
        title: "implement step".to_string(),
        description: "implement step".to_string(),
        status: TaskStatus::Completed,
        priority: TaskPriority::Normal,
        progress: 100,
        created_at: now_millis(),
        started_at: Some(now_millis().saturating_sub(1_000)),
        completed_at: Some(now_millis()),
        error: None,
        result: Some("build completed successfully".to_string()),
        thread_id: None,
        source: "goal_run".to_string(),
        notify_on_complete: false,
        notify_channels: Vec::new(),
        dependencies: Vec::new(),
        command: None,
        session_id: None,
        goal_run_id: Some(goal_run_id.to_string()),
        goal_run_title: Some("goal with custom step".to_string()),
        goal_step_id: Some("step-1".to_string()),
        goal_step_title: Some("step-1".to_string()),
        parent_task_id: None,
        parent_thread_id: None,
        runtime: "daemon".to_string(),
        retry_count: 0,
        max_retries: 0,
        next_retry_at: None,
        scheduled_at: None,
        blocked_reason: None,
        awaiting_approval_id: None,
        policy_fingerprint: None,
        approval_expires_at: None,
        containment_scope: None,
        compensation_status: None,
        compensation_summary: None,
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
    };

    engine
        .handle_goal_run_step_completion(goal_run_id, &completed_task)
        .await
        .expect("implementation completion should queue verification");

    let updated = engine
        .get_goal_run(goal_run_id)
        .await
        .expect("goal should still exist");
    assert_eq!(
        updated.current_step_index, 0,
        "step should not advance before verification"
    );
    assert_eq!(updated.steps[0].status, GoalRunStepStatus::InProgress);

    let verifier_task_id = updated.steps[0]
        .task_id
        .clone()
        .expect("verification task should take over current step");
    assert_ne!(verifier_task_id, "task-impl");

    let verifier_task = engine
        .tasks
        .lock()
        .await
        .iter()
        .find(|task| task.id == verifier_task_id)
        .cloned()
        .expect("verification task should be enqueued");
    assert_eq!(verifier_task.source, "goal_verification");
    assert_eq!(
        verifier_task.sub_agent_def_id.as_deref(),
        Some("android-verifier")
    );
    assert_eq!(verifier_task.goal_step_id.as_deref(), Some("step-1"));

    let dossier = updated
        .dossier
        .expect("verification should keep dossier state");
    assert_eq!(dossier.units[0].status, GoalProjectionState::InProgress);
    assert_eq!(
        dossier.units[0].proof_checks[0].state,
        GoalProjectionState::InProgress
    );
    assert_eq!(
        dossier.units[0]
            .report
            .as_ref()
            .expect("verification queue should emit a unit report")
            .state,
        GoalProjectionState::InProgress
    );
}

#[tokio::test]
async fn verification_binding_uses_goal_local_assignment_overrides() {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let goal_run_id = "goal-local-verification";

    let mut goal_run = sample_goal_run_with_kind(
        goal_run_id,
        GoalRunStepKind::Command,
        "Build the Android artifact",
    );
    goal_run.steps[0].status = GoalRunStepStatus::InProgress;
    goal_run.steps[0].task_id = Some("task-impl".to_string());
    goal_run.launch_assignment_snapshot =
        vec![sample_goal_assignment("verifier", "openai", "gpt-5.4-mini")];
    goal_run.runtime_assignment_list = goal_run.launch_assignment_snapshot.clone();
    goal_run.dossier = Some(GoalRunDossier {
        units: vec![GoalDeliveryUnit {
            id: "step-1".to_string(),
            title: "step-1".to_string(),
            status: GoalProjectionState::InProgress,
            execution_binding: GoalRoleBinding::Builtin(
                crate::agent::agent_identity::MAIN_AGENT_ID.to_string(),
            ),
            verification_binding: GoalRoleBinding::Builtin("verifier".to_string()),
            summary: None,
            proof_checks: vec![GoalProofCheck {
                id: "proof-build-debug".to_string(),
                title: "Debug build succeeds".to_string(),
                state: GoalProjectionState::Pending,
                summary: None,
                evidence_ids: Vec::new(),
                resolved_at: None,
            }],
            evidence: Vec::new(),
            report: None,
        }],
        ..Default::default()
    });
    engine.goal_runs.lock().await.push_back(goal_run);

    let completed_task = AgentTask {
        id: "task-impl".to_string(),
        title: "implement step".to_string(),
        description: "implement step".to_string(),
        status: TaskStatus::Completed,
        priority: TaskPriority::Normal,
        progress: 100,
        created_at: now_millis(),
        started_at: Some(now_millis().saturating_sub(1_000)),
        completed_at: Some(now_millis()),
        error: None,
        result: Some("build completed successfully".to_string()),
        thread_id: None,
        source: "goal_run".to_string(),
        notify_on_complete: false,
        notify_channels: Vec::new(),
        dependencies: Vec::new(),
        command: None,
        session_id: None,
        goal_run_id: Some(goal_run_id.to_string()),
        goal_run_title: Some("goal with custom step".to_string()),
        goal_step_id: Some("step-1".to_string()),
        goal_step_title: Some("step-1".to_string()),
        parent_task_id: None,
        parent_thread_id: None,
        runtime: "daemon".to_string(),
        retry_count: 0,
        max_retries: 0,
        next_retry_at: None,
        scheduled_at: None,
        blocked_reason: None,
        awaiting_approval_id: None,
        policy_fingerprint: None,
        approval_expires_at: None,
        containment_scope: None,
        compensation_status: None,
        compensation_summary: None,
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
    };

    engine
        .handle_goal_run_step_completion(goal_run_id, &completed_task)
        .await
        .expect("implementation completion should queue verification");

    let updated = engine
        .get_goal_run(goal_run_id)
        .await
        .expect("goal should still exist");
    let verification_task_id = updated.steps[0]
        .task_id
        .clone()
        .expect("verification task should take over current step");
    let verification_task = engine
        .tasks
        .lock()
        .await
        .iter()
        .find(|task| task.id == verification_task_id)
        .cloned()
        .expect("verification task should exist");

    assert_eq!(
        verification_task.override_provider.as_deref(),
        Some("openai")
    );
    assert_eq!(
        verification_task.override_model.as_deref(),
        Some("gpt-5.4-mini")
    );
    assert!(verification_task.sub_agent_def_id.is_none());
}

#[tokio::test]
async fn verifier_completion_advances_goal_step_and_resolves_proof_checks() {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let goal_run_id = "goal-step-verification-complete";

    let mut goal_run = sample_goal_run_with_kind(
        goal_run_id,
        GoalRunStepKind::Command,
        "Build the Android artifact",
    );
    goal_run.steps[0].status = GoalRunStepStatus::InProgress;
    goal_run.steps[0].task_id = Some("task-impl".to_string());
    goal_run.steps.push(GoalRunStep {
        id: "step-2".to_string(),
        position: 1,
        title: "step-2".to_string(),
        instructions: "ship result".to_string(),
        kind: GoalRunStepKind::Command,
        success_criteria: "result shipped".to_string(),
        session_id: None,
        status: GoalRunStepStatus::Pending,
        task_id: None,
        summary: None,
        error: None,
        started_at: None,
        completed_at: None,
    });
    goal_run.dossier = Some(GoalRunDossier {
        units: vec![GoalDeliveryUnit {
            id: "step-1".to_string(),
            title: "step-1".to_string(),
            status: GoalProjectionState::InProgress,
            execution_binding: GoalRoleBinding::Builtin("swarog".to_string()),
            verification_binding: GoalRoleBinding::Builtin("main".to_string()),
            summary: None,
            proof_checks: vec![GoalProofCheck {
                id: "proof-build-debug".to_string(),
                title: "Debug build succeeds".to_string(),
                state: GoalProjectionState::Pending,
                summary: None,
                evidence_ids: Vec::new(),
                resolved_at: None,
            }],
            evidence: Vec::new(),
            report: None,
        }],
        ..Default::default()
    });
    engine.goal_runs.lock().await.push_back(goal_run);

    let implementation_task = AgentTask {
        id: "task-impl".to_string(),
        title: "implement step".to_string(),
        description: "implement step".to_string(),
        status: TaskStatus::Completed,
        priority: TaskPriority::Normal,
        progress: 100,
        created_at: now_millis(),
        started_at: Some(now_millis().saturating_sub(1_000)),
        completed_at: Some(now_millis()),
        error: None,
        result: Some("build completed successfully".to_string()),
        thread_id: None,
        source: "goal_run".to_string(),
        notify_on_complete: false,
        notify_channels: Vec::new(),
        dependencies: Vec::new(),
        command: None,
        session_id: None,
        goal_run_id: Some(goal_run_id.to_string()),
        goal_run_title: Some("goal with custom step".to_string()),
        goal_step_id: Some("step-1".to_string()),
        goal_step_title: Some("step-1".to_string()),
        parent_task_id: None,
        parent_thread_id: None,
        runtime: "daemon".to_string(),
        retry_count: 0,
        max_retries: 0,
        next_retry_at: None,
        scheduled_at: None,
        blocked_reason: None,
        awaiting_approval_id: None,
        policy_fingerprint: None,
        approval_expires_at: None,
        containment_scope: None,
        compensation_status: None,
        compensation_summary: None,
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
    };

    engine
        .handle_goal_run_step_completion(goal_run_id, &implementation_task)
        .await
        .expect("implementation completion should queue verification");

    let verifier_task = engine
        .tasks
        .lock()
        .await
        .iter()
        .find(|task| task.source == "goal_verification")
        .cloned()
        .expect("verification task should exist");
    assert!(
        verifier_task.sub_agent_def_id.is_none(),
        "main-agent verification should stay on the builtin daemon path"
    );

    let mut completed_verifier = verifier_task.clone();
    completed_verifier.status = TaskStatus::Completed;
    completed_verifier.result = Some("all proof checks satisfied".to_string());
    write_step_completion_marker(&engine, goal_run_id, 0).await;

    engine
        .handle_goal_run_step_completion(goal_run_id, &completed_verifier)
        .await
        .expect("verification completion should advance the step");

    let updated = engine
        .get_goal_run(goal_run_id)
        .await
        .expect("goal should still exist");
    assert_eq!(updated.current_step_index, 1);
    assert_eq!(updated.steps[0].status, GoalRunStepStatus::Completed);
    assert_eq!(updated.current_step_title.as_deref(), Some("step-2"));

    let dossier = updated.dossier.expect("verification should update dossier");
    assert_eq!(dossier.units[0].status, GoalProjectionState::Completed);
    assert_eq!(
        dossier.units[0].proof_checks[0].state,
        GoalProjectionState::Completed
    );
    assert!(
        !dossier.units[0].evidence.is_empty(),
        "verification completion should capture evidence"
    );
    assert_eq!(
        dossier
            .latest_resume_decision
            .as_ref()
            .expect("verification completion should advance the goal")
            .action,
        GoalResumeAction::Advance
    );
}

#[tokio::test]
async fn handle_goal_run_step_completion_records_dossier_report_and_advance_decision() {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let goal_run_id = "goal-step-completion-dossier";

    let mut goal_run = sample_goal_run_with_kind(
        goal_run_id,
        GoalRunStepKind::Command,
        "Run the build and continue",
    );
    goal_run.steps[0].status = GoalRunStepStatus::InProgress;
    goal_run.steps[0].task_id = Some("task-step-complete".to_string());
    goal_run.steps.push(GoalRunStep {
        id: "step-2".to_string(),
        position: 1,
        title: "step-2".to_string(),
        instructions: "verify artifacts".to_string(),
        kind: GoalRunStepKind::Research,
        success_criteria: "artifacts verified".to_string(),
        session_id: None,
        status: GoalRunStepStatus::Pending,
        task_id: None,
        summary: None,
        error: None,
        started_at: None,
        completed_at: None,
    });
    engine.goal_runs.lock().await.push_back(goal_run);

    let completed_task = AgentTask {
        id: "task-step-complete".to_string(),
        title: "complete step".to_string(),
        description: "complete step".to_string(),
        status: TaskStatus::Completed,
        priority: TaskPriority::Normal,
        progress: 100,
        created_at: now_millis(),
        started_at: Some(now_millis().saturating_sub(1_000)),
        completed_at: Some(now_millis()),
        error: None,
        result: Some("ok".to_string()),
        thread_id: None,
        source: "goal_run".to_string(),
        notify_on_complete: false,
        notify_channels: Vec::new(),
        dependencies: Vec::new(),
        command: None,
        session_id: None,
        goal_run_id: Some(goal_run_id.to_string()),
        goal_run_title: Some("goal with custom step".to_string()),
        goal_step_id: Some("step-1".to_string()),
        goal_step_title: Some("step-1".to_string()),
        parent_task_id: None,
        parent_thread_id: None,
        runtime: "daemon".to_string(),
        retry_count: 0,
        max_retries: 0,
        next_retry_at: None,
        scheduled_at: None,
        blocked_reason: None,
        awaiting_approval_id: None,
        policy_fingerprint: None,
        approval_expires_at: None,
        containment_scope: None,
        compensation_status: None,
        compensation_summary: None,
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
    };

    write_step_completion_marker(&engine, goal_run_id, 0).await;

    engine
        .handle_goal_run_step_completion(goal_run_id, &completed_task)
        .await
        .expect("step completion should succeed");

    let updated = engine
        .get_goal_run(goal_run_id)
        .await
        .expect("goal run should still exist");
    let dossier = updated
        .dossier
        .expect("completion should create dossier state");
    assert_eq!(
        dossier
            .latest_resume_decision
            .as_ref()
            .expect("completion should record latest resume decision")
            .action,
        GoalResumeAction::Advance
    );
    assert_eq!(
        dossier
            .latest_resume_decision
            .as_ref()
            .expect("completion should record reason code")
            .reason_code,
        "step_completed"
    );
    assert_eq!(
        dossier.units[0]
            .report
            .as_ref()
            .expect("completed unit should have a report")
            .state,
        GoalProjectionState::Completed
    );
    assert_eq!(dossier.units[0].status, GoalProjectionState::Completed);
    assert_eq!(
        dossier.units[0].summary.as_deref(),
        Some("step completed"),
        "live dossier unit state should match the emitted report before persistence refresh"
    );
}

#[tokio::test]
async fn handle_goal_run_step_completion_blocks_when_completion_marker_is_missing() {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let goal_run_id = "goal-step-marker-missing";

    let mut goal_run = sample_goal_run_with_kind(
        goal_run_id,
        GoalRunStepKind::Command,
        "Run the build and continue",
    );
    goal_run.steps[0].status = GoalRunStepStatus::InProgress;
    goal_run.steps[0].task_id = Some("task-step-marker-missing".to_string());
    goal_run.steps.push(GoalRunStep {
        id: "step-2".to_string(),
        position: 1,
        title: "step-2".to_string(),
        instructions: "verify artifacts".to_string(),
        kind: GoalRunStepKind::Research,
        success_criteria: "artifacts verified".to_string(),
        session_id: None,
        status: GoalRunStepStatus::Pending,
        task_id: None,
        summary: None,
        error: None,
        started_at: None,
        completed_at: None,
    });
    engine.goal_runs.lock().await.push_back(goal_run);

    let completed_task = AgentTask {
        id: "task-step-marker-missing".to_string(),
        title: "complete step".to_string(),
        description: "complete step".to_string(),
        status: TaskStatus::Completed,
        priority: TaskPriority::Normal,
        progress: 100,
        created_at: now_millis(),
        started_at: Some(now_millis().saturating_sub(1_000)),
        completed_at: Some(now_millis()),
        error: None,
        result: Some("ok".to_string()),
        thread_id: Some("thread-goal-custom".to_string()),
        source: "goal_run".to_string(),
        notify_on_complete: false,
        notify_channels: Vec::new(),
        dependencies: Vec::new(),
        command: None,
        session_id: None,
        goal_run_id: Some(goal_run_id.to_string()),
        goal_run_title: Some("goal with custom step".to_string()),
        goal_step_id: Some("step-1".to_string()),
        goal_step_title: Some("step-1".to_string()),
        parent_task_id: None,
        parent_thread_id: None,
        runtime: "daemon".to_string(),
        retry_count: 0,
        max_retries: 0,
        next_retry_at: None,
        scheduled_at: None,
        blocked_reason: None,
        awaiting_approval_id: None,
        policy_fingerprint: None,
        approval_expires_at: None,
        containment_scope: None,
        compensation_status: None,
        compensation_summary: None,
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
    };
    engine.tasks.lock().await.push_back(completed_task.clone());

    engine
        .replace_thread_todos(
            "thread-goal-custom",
            vec![TodoItem {
                id: "todo-1".to_string(),
                content: "done".to_string(),
                status: TodoStatus::Completed,
                position: 0,
                step_index: Some(0),
                created_at: 0,
                updated_at: 0,
            }],
            Some(completed_task.id.as_str()),
        )
        .await;

    engine
        .handle_goal_run_step_completion(goal_run_id, &completed_task)
        .await
        .expect("completion hook should not hard-fail when marker is missing");

    let marker_path = crate::agent::goal_dossier::goal_step_completion_marker_path(
        &engine.data_dir,
        goal_run_id,
        0,
    );
    let updated = engine
        .get_goal_run(goal_run_id)
        .await
        .expect("goal run should still exist");
    assert_eq!(updated.current_step_index, 0);
    assert_eq!(updated.steps[0].status, GoalRunStepStatus::InProgress);

    let stored_task = engine
        .tasks
        .lock()
        .await
        .iter()
        .find(|task| task.id == completed_task.id)
        .cloned()
        .expect("task should still exist");
    assert_eq!(stored_task.status, TaskStatus::Queued);
    assert!(
        stored_task
            .description
            .contains(&marker_path.display().to_string()),
        "queued retry should instruct the agent to create the marker file"
    );
}

#[tokio::test]
async fn exhausted_completion_marker_retries_require_human_approval() {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let goal_run_id = "goal-step-marker-approval";

    let mut goal_run = sample_goal_run_with_kind(
        goal_run_id,
        GoalRunStepKind::Command,
        "Run the build and continue",
    );
    goal_run.steps[0].status = GoalRunStepStatus::InProgress;
    goal_run.steps[0].task_id = Some("task-step-marker-approval".to_string());
    engine.goal_runs.lock().await.push_back(goal_run);

    let template_task = AgentTask {
        id: "task-step-marker-approval".to_string(),
        title: "complete step".to_string(),
        description: "complete step".to_string(),
        status: TaskStatus::Completed,
        priority: TaskPriority::Normal,
        progress: 100,
        created_at: now_millis(),
        started_at: Some(now_millis().saturating_sub(1_000)),
        completed_at: Some(now_millis()),
        error: None,
        result: Some("ok".to_string()),
        thread_id: Some("thread-goal-custom".to_string()),
        source: "goal_run".to_string(),
        notify_on_complete: false,
        notify_channels: Vec::new(),
        dependencies: Vec::new(),
        command: None,
        session_id: None,
        goal_run_id: Some(goal_run_id.to_string()),
        goal_run_title: Some("goal with custom step".to_string()),
        goal_step_id: Some("step-1".to_string()),
        goal_step_title: Some("step-1".to_string()),
        parent_task_id: None,
        parent_thread_id: None,
        runtime: "daemon".to_string(),
        retry_count: 0,
        max_retries: 0,
        next_retry_at: None,
        scheduled_at: None,
        blocked_reason: None,
        awaiting_approval_id: None,
        policy_fingerprint: None,
        approval_expires_at: None,
        containment_scope: None,
        compensation_status: None,
        compensation_summary: None,
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
    };
    engine.tasks.lock().await.push_back(template_task.clone());

    engine
        .replace_thread_todos(
            "thread-goal-custom",
            vec![TodoItem {
                id: "todo-1".to_string(),
                content: "done".to_string(),
                status: TodoStatus::Completed,
                position: 0,
                step_index: Some(0),
                created_at: 0,
                updated_at: 0,
            }],
            Some(template_task.id.as_str()),
        )
        .await;

    for _ in 0..4 {
        let mut completed_task = engine
            .tasks
            .lock()
            .await
            .iter()
            .find(|task| task.id == template_task.id)
            .cloned()
            .expect("task should still exist");
        completed_task.status = TaskStatus::Completed;
        completed_task.completed_at = Some(now_millis());

        engine
            .handle_goal_run_step_completion(goal_run_id, &completed_task)
            .await
            .expect("completion hook should not hard-fail during reminder escalation");
    }

    let updated = engine
        .get_goal_run(goal_run_id)
        .await
        .expect("goal run should still exist");
    assert_eq!(updated.status, GoalRunStatus::AwaitingApproval);
    assert!(
        updated.awaiting_approval_id.is_some(),
        "goal should surface a human-approval requirement after exhausting retries"
    );

    let stored_task = engine
        .tasks
        .lock()
        .await
        .iter()
        .find(|task| task.id == template_task.id)
        .cloned()
        .expect("task should still exist");
    assert_eq!(stored_task.status, TaskStatus::AwaitingApproval);
    assert!(
        stored_task
            .blocked_reason
            .as_deref()
            .is_some_and(|reason| reason.contains(
                &crate::agent::goal_dossier::goal_step_completion_marker_path(
                    &engine.data_dir,
                    goal_run_id,
                    0,
                )
                .display()
                .to_string()
            )),
        "approval details should explain which marker file is missing"
    );
}

#[tokio::test]
async fn handle_goal_run_step_completion_schedules_subagent_verification_before_advance() {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.sub_agents.push(SubAgentDefinition {
        id: "android-verifier".to_string(),
        name: "Android Verifier".to_string(),
        provider: "openai".to_string(),
        model: "gpt-4o-mini".to_string(),
        role: Some("verification specialist".to_string()),
        system_prompt: Some("Verify Android build artifacts.".to_string()),
        tool_whitelist: None,
        tool_blacklist: None,
        context_budget_tokens: None,
        max_duration_secs: None,
        supervisor_config: None,
        enabled: true,
        builtin: false,
        immutable_identity: false,
        disable_allowed: true,
        delete_allowed: true,
        protected_reason: None,
        reasoning_effort: None,
        created_at: now_millis(),
    });
    let engine = AgentEngine::new_test(manager, config, root.path()).await;
    let goal_run_id = "goal-verification-subagent";

    let mut goal_run = sample_goal_run_with_kind(
        goal_run_id,
        GoalRunStepKind::Command,
        "Create the Android shell",
    );
    goal_run.steps[0].status = GoalRunStepStatus::InProgress;
    goal_run.steps[0].task_id = Some("task-step-exec".to_string());
    goal_run.steps[0].summary = Some("build finished".to_string());
    goal_run.steps.push(GoalRunStep {
        id: "step-2".to_string(),
        position: 1,
        title: "step-2".to_string(),
        instructions: "continue".to_string(),
        kind: GoalRunStepKind::Research,
        success_criteria: "step 2 done".to_string(),
        session_id: None,
        status: GoalRunStepStatus::Pending,
        task_id: None,
        summary: None,
        error: None,
        started_at: None,
        completed_at: None,
    });
    goal_run.active_task_id = Some("task-step-exec".to_string());
    goal_run.child_task_ids.push("task-step-exec".to_string());
    goal_run.child_task_count = 1;
    goal_run.dossier = Some(GoalRunDossier {
        units: vec![GoalDeliveryUnit {
            id: "step-1".to_string(),
            title: "Create the Android shell".to_string(),
            status: GoalProjectionState::InProgress,
            execution_binding: GoalRoleBinding::Builtin(
                crate::agent::agent_identity::MAIN_AGENT_ID.to_string(),
            ),
            verification_binding: GoalRoleBinding::Subagent("android-verifier".to_string()),
            summary: Some("Build the shell".to_string()),
            proof_checks: vec![GoalProofCheck {
                id: "proof-build-debug".to_string(),
                title: "assembleDebug passes".to_string(),
                state: GoalProjectionState::Pending,
                summary: Some("run the Android debug build".to_string()),
                evidence_ids: Vec::new(),
                resolved_at: None,
            }],
            evidence: Vec::new(),
            report: None,
        }],
        projection_state: GoalProjectionState::InProgress,
        summary: Some("Build the shell".to_string()),
        ..Default::default()
    });
    engine.goal_runs.lock().await.push_back(goal_run);

    let completed_task = AgentTask {
        id: "task-step-exec".to_string(),
        title: "complete step".to_string(),
        description: "complete step".to_string(),
        status: TaskStatus::Completed,
        priority: TaskPriority::Normal,
        progress: 100,
        created_at: now_millis(),
        started_at: Some(now_millis().saturating_sub(1_000)),
        completed_at: Some(now_millis()),
        error: None,
        result: Some("ok".to_string()),
        thread_id: Some("thread-goal-custom".to_string()),
        source: "goal_run".to_string(),
        notify_on_complete: false,
        notify_channels: Vec::new(),
        dependencies: Vec::new(),
        command: None,
        session_id: None,
        goal_run_id: Some(goal_run_id.to_string()),
        goal_run_title: Some("goal with custom step".to_string()),
        goal_step_id: Some("step-1".to_string()),
        goal_step_title: Some("step-1".to_string()),
        parent_task_id: None,
        parent_thread_id: None,
        runtime: "daemon".to_string(),
        retry_count: 0,
        max_retries: 0,
        next_retry_at: None,
        scheduled_at: None,
        blocked_reason: None,
        awaiting_approval_id: None,
        policy_fingerprint: None,
        approval_expires_at: None,
        containment_scope: None,
        compensation_status: None,
        compensation_summary: None,
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
    };

    engine
        .handle_goal_run_step_completion(goal_run_id, &completed_task)
        .await
        .expect("step completion should schedule verification");

    let updated = engine
        .get_goal_run(goal_run_id)
        .await
        .expect("goal run should still exist");
    let updated_json = goal_run_json(&updated);
    let current_step_owner = updated_json
        .get("current_step_owner_profile")
        .and_then(serde_json::Value::as_object)
        .expect("goal run in progress should expose current step owner metadata");
    assert_eq!(
        current_step_owner
            .get("agent_label")
            .and_then(serde_json::Value::as_str),
        Some("Android Verifier")
    );
    assert_eq!(
        current_step_owner
            .get("provider")
            .and_then(serde_json::Value::as_str),
        Some("openai")
    );
    assert_eq!(
        current_step_owner
            .get("model")
            .and_then(serde_json::Value::as_str),
        Some("gpt-4o-mini")
    );
    assert!(
        current_step_owner.get("reasoning_effort").is_none()
            || current_step_owner
                .get("reasoning_effort")
                .is_some_and(|value| value.is_null()),
        "subagent metadata should omit unset reasoning effort"
    );
    assert_eq!(updated.current_step_index, 0);
    assert_eq!(updated.status, GoalRunStatus::Running);
    assert!(updated
        .dossier
        .as_ref()
        .expect("dossier should exist")
        .units[0]
        .proof_checks[0]
        .state
        .eq(&GoalProjectionState::InProgress));

    let verification_task_id = updated.steps[0]
        .task_id
        .clone()
        .expect("verification task should replace the execution task");
    let verification_task = engine
        .tasks
        .lock()
        .await
        .iter()
        .find(|task| task.id == verification_task_id)
        .cloned()
        .expect("verification task should exist");

    assert_eq!(verification_task.source, "goal_verification");
    assert_eq!(
        verification_task.sub_agent_def_id.as_deref(),
        Some("android-verifier")
    );
    assert_eq!(
        verification_task.override_provider.as_deref(),
        Some("openai")
    );
    assert_eq!(
        verification_task.override_model.as_deref(),
        Some("gpt-4o-mini")
    );
    assert_eq!(
        updated
            .dossier
            .as_ref()
            .expect("dossier should exist")
            .units[0]
            .report
            .as_ref()
            .expect("verification should write a dossier report")
            .state,
        GoalProjectionState::InProgress
    );

    let mut verification_complete = verification_task.clone();
    verification_complete.status = TaskStatus::Completed;
    verification_complete.completed_at = Some(now_millis());
    verification_complete.result = Some("verification passed".to_string());
    write_step_completion_marker(&engine, goal_run_id, 0).await;

    engine
        .handle_goal_run_step_completion(goal_run_id, &verification_complete)
        .await
        .expect("verification completion should advance the goal");

    let final_goal = engine
        .get_goal_run(goal_run_id)
        .await
        .expect("goal run should still exist");
    assert_eq!(final_goal.current_step_index, 1);
    assert_eq!(final_goal.steps[0].status, GoalRunStepStatus::Completed);
    assert_eq!(
        final_goal
            .dossier
            .as_ref()
            .expect("dossier should exist")
            .units[0]
            .status,
        GoalProjectionState::Completed
    );
    assert_eq!(
        final_goal
            .dossier
            .as_ref()
            .expect("dossier should exist")
            .units[0]
            .proof_checks[0]
            .state,
        GoalProjectionState::Completed
    );
    assert_eq!(
        final_goal
            .dossier
            .as_ref()
            .expect("dossier should exist")
            .latest_resume_decision
            .as_ref()
            .expect("verification should record a resume decision")
            .action,
        GoalResumeAction::Advance
    );
}

#[tokio::test]
async fn handle_goal_run_step_completion_resolves_builtin_verification_binding_to_existing_infra() {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.builtin_sub_agents.weles.provider = Some("openai".to_string());
    config.builtin_sub_agents.weles.model = Some("gpt-4o-mini".to_string());
    let engine = AgentEngine::new_test(manager, config, root.path()).await;
    let goal_run_id = "goal-verification-builtin";

    let mut goal_run = sample_goal_run_with_kind(
        goal_run_id,
        GoalRunStepKind::Command,
        "Create the Android shell",
    );
    goal_run.steps[0].status = GoalRunStepStatus::InProgress;
    goal_run.steps[0].task_id = Some("task-step-exec".to_string());
    goal_run.active_task_id = Some("task-step-exec".to_string());
    goal_run.child_task_ids.push("task-step-exec".to_string());
    goal_run.child_task_count = 1;
    goal_run.dossier = Some(GoalRunDossier {
        units: vec![GoalDeliveryUnit {
            id: "step-1".to_string(),
            title: "Create the Android shell".to_string(),
            status: GoalProjectionState::InProgress,
            execution_binding: GoalRoleBinding::Builtin(
                crate::agent::agent_identity::MAIN_AGENT_ID.to_string(),
            ),
            verification_binding: GoalRoleBinding::Builtin(
                crate::agent::agent_identity::WELES_BUILTIN_SUBAGENT_ID.to_string(),
            ),
            summary: Some("Build the shell".to_string()),
            proof_checks: vec![GoalProofCheck {
                id: "proof-build-debug".to_string(),
                title: "assembleDebug passes".to_string(),
                state: GoalProjectionState::Pending,
                summary: Some("run the Android debug build".to_string()),
                evidence_ids: Vec::new(),
                resolved_at: None,
            }],
            evidence: Vec::new(),
            report: None,
        }],
        projection_state: GoalProjectionState::InProgress,
        summary: Some("Build the shell".to_string()),
        ..Default::default()
    });
    engine.goal_runs.lock().await.push_back(goal_run);

    let completed_task = AgentTask {
        id: "task-step-exec".to_string(),
        title: "complete step".to_string(),
        description: "complete step".to_string(),
        status: TaskStatus::Completed,
        priority: TaskPriority::Normal,
        progress: 100,
        created_at: now_millis(),
        started_at: Some(now_millis().saturating_sub(1_000)),
        completed_at: Some(now_millis()),
        error: None,
        result: Some("ok".to_string()),
        thread_id: Some("thread-goal-custom".to_string()),
        source: "goal_run".to_string(),
        notify_on_complete: false,
        notify_channels: Vec::new(),
        dependencies: Vec::new(),
        command: None,
        session_id: None,
        goal_run_id: Some(goal_run_id.to_string()),
        goal_run_title: Some("goal with custom step".to_string()),
        goal_step_id: Some("step-1".to_string()),
        goal_step_title: Some("step-1".to_string()),
        parent_task_id: None,
        parent_thread_id: None,
        runtime: "daemon".to_string(),
        retry_count: 0,
        max_retries: 0,
        next_retry_at: None,
        scheduled_at: None,
        blocked_reason: None,
        awaiting_approval_id: None,
        policy_fingerprint: None,
        approval_expires_at: None,
        containment_scope: None,
        compensation_status: None,
        compensation_summary: None,
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
    };

    engine
        .handle_goal_run_step_completion(goal_run_id, &completed_task)
        .await
        .expect("step completion should schedule verification");

    let verification_task_id = engine
        .get_goal_run(goal_run_id)
        .await
        .expect("goal run should exist")
        .steps[0]
        .task_id
        .clone()
        .expect("verification task should replace the execution task");
    let verification_task = engine
        .tasks
        .lock()
        .await
        .iter()
        .find(|task| task.id == verification_task_id)
        .cloned()
        .expect("verification task should exist");

    assert_eq!(verification_task.source, "goal_verification");
    assert_eq!(
        verification_task.sub_agent_def_id.as_deref(),
        Some(crate::agent::agent_identity::WELES_BUILTIN_SUBAGENT_ID)
    );
    assert_eq!(
        verification_task.override_provider.as_deref(),
        Some("openai")
    );
    assert_eq!(
        verification_task.override_model.as_deref(),
        Some("gpt-4o-mini")
    );
}

#[tokio::test]
async fn enqueue_goal_run_step_starts_debate_session_for_debate_kind() {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.debate.enabled = true;
    let engine = AgentEngine::new_test(manager, config, root.path()).await;
    let goal_run_id = "goal-debate";

    engine
        .goal_runs
        .lock()
        .await
        .push_back(sample_goal_run_with_kind(
            goal_run_id,
            GoalRunStepKind::Debate,
            "Debate the rollout tradeoffs for the migration",
        ));

    engine
        .enqueue_goal_run_step(goal_run_id)
        .await
        .expect("enqueue should succeed");

    let goal = engine
        .get_goal_run(goal_run_id)
        .await
        .expect("goal should exist");
    let task_id = goal.steps[0]
        .task_id
        .clone()
        .expect("debate step should create a tracking task");
    let tasks = engine.tasks.lock().await;
    let task = tasks
        .iter()
        .find(|task| task.id == task_id)
        .cloned()
        .expect("tracking task should exist");
    drop(tasks);

    assert_eq!(task.source, "debate");
    assert!(task.title.starts_with("Debate:"));
    assert!(task.description.contains("Debate session"));

    let session_id = task
        .description
        .split_whitespace()
        .nth(2)
        .expect("session id token should exist")
        .to_string();
    let debate_payload = engine
        .get_debate_session_payload(&session_id)
        .await
        .expect("debate session should be retrievable");
    assert_eq!(
        debate_payload.get("topic").and_then(|v| v.as_str()),
        Some("Debate the rollout tradeoffs for the migration")
    );
    assert_eq!(
        debate_payload.get("status").and_then(|v| v.as_str()),
        Some("in_progress")
    );
}

#[tokio::test]
async fn handle_goal_run_step_failure_surfaces_strained_replan_summary_guidance() {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let recorded_bodies =
        std::sync::Arc::new(std::sync::Mutex::new(std::collections::VecDeque::new()));
    let mut config = AgentConfig::default();
    config.provider = "openai".to_string();
    config.base_url = crate::agent::tests::spawn_goal_recording_server(
        recorded_bodies,
        serde_json::json!({
            "title": "Recovery plan",
            "summary": "Retry with the normal recovery flow.",
            "steps": [
                {
                    "title": "Narrow the failing command",
                    "instructions": "Reduce scope and retry the command.",
                    "kind": "command",
                    "success_criteria": "command succeeds",
                    "session_id": null,
                    "llm_confidence": "likely",
                    "llm_confidence_rationale": "bounded retry"
                }
            ],
            "rejected_alternatives": ["Alternative A: repeat the same broad command"]
        })
        .to_string(),
    )
    .await;
    config.model = "gpt-4o-mini".to_string();
    config.api_key = "test-key".to_string();
    config.api_transport = ApiTransport::ChatCompletions;
    config.auto_retry = false;
    config.max_retries = 0;
    config.max_tool_loops = 1;
    let engine = AgentEngine::new_test(manager, config, root.path()).await;

    {
        let mut model = engine.operator_model.write().await;
        model.cognitive_style.message_count = 1;
        model.operator_satisfaction.score = 0.18;
        model.operator_satisfaction.label = "strained".to_string();
    }

    let goal_run_id = "goal-strained-replan-summary";
    let mut goal_run = sample_goal_run_with_kind(
        goal_run_id,
        GoalRunStepKind::Command,
        "Run the failing command and recover if needed",
    );
    goal_run.thread_id = Some("thread-strained-replan".to_string());
    goal_run.current_step_index = 0;
    goal_run.current_step_title = Some("step-1".to_string());
    goal_run.current_step_kind = Some(GoalRunStepKind::Command);
    engine.goal_runs.lock().await.push_back(goal_run.clone());

    let failed_task = AgentTask {
        id: "task-strained-replan".to_string(),
        title: "failed step".to_string(),
        description: "failed step".to_string(),
        status: TaskStatus::Failed,
        priority: TaskPriority::Normal,
        progress: 0,
        created_at: now_millis(),
        started_at: Some(now_millis().saturating_sub(5_000)),
        completed_at: Some(now_millis()),
        error: Some("managed command failed permanently".to_string()),
        result: None,
        thread_id: Some("thread-strained-replan".to_string()),
        source: "goal_run".to_string(),
        notify_on_complete: false,
        notify_channels: Vec::new(),
        dependencies: Vec::new(),
        command: None,
        session_id: None,
        goal_run_id: Some(goal_run_id.to_string()),
        goal_run_title: Some(goal_run.title.clone()),
        goal_step_id: Some("step-1".to_string()),
        goal_step_title: Some("step-1".to_string()),
        parent_task_id: None,
        parent_thread_id: None,
        runtime: "daemon".to_string(),
        retry_count: 0,
        max_retries: 0,
        next_retry_at: None,
        scheduled_at: None,
        blocked_reason: None,
        awaiting_approval_id: None,
        policy_fingerprint: None,
        approval_expires_at: None,
        containment_scope: None,
        compensation_status: None,
        compensation_summary: None,
        lane_id: None,
        last_error: Some("managed command failed permanently".to_string()),
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
    };

    engine
        .handle_goal_run_step_failure(goal_run_id, &failed_task)
        .await
        .expect("replan should succeed");

    let updated = engine
        .get_goal_run(goal_run_id)
        .await
        .expect("goal run should exist after replan");
    let summary = updated
        .reflection_summary
        .as_deref()
        .expect("replan summary should be surfaced");
    assert!(summary.contains("Meta-cognitive intervention:"));
    assert!(summary.contains("Conservative execution mode:"));
    assert!(summary.contains("prefer proven tools"));
    assert!(summary.contains("keep iteration bounds short"));
    let dossier = updated
        .dossier
        .expect("replan should preserve dossier state");
    assert_eq!(
        dossier
            .latest_resume_decision
            .as_ref()
            .expect("replan should record resume decision")
            .action,
        GoalResumeAction::Replan
    );
    assert_eq!(
        dossier.units[0]
            .report
            .as_ref()
            .expect("failed unit should capture a report")
            .state,
        GoalProjectionState::Failed
    );
    assert_eq!(dossier.units[0].status, GoalProjectionState::Failed);
    assert_eq!(
        dossier.units[0].summary.as_deref(),
        Some("managed command failed permanently")
    );
}

#[tokio::test]
async fn low_confidence_plan_gate_creates_task_backed_approval_that_can_be_resolved() {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let recorded_bodies =
        std::sync::Arc::new(std::sync::Mutex::new(std::collections::VecDeque::new()));
    let mut config = AgentConfig::default();
    config.provider = "openai".to_string();
    config.base_url = crate::agent::tests::spawn_goal_recording_server(
        recorded_bodies,
        serde_json::json!({
            "title": "Needs review",
            "summary": "Plan includes a risky unknown step.",
            "steps": [
                {
                    "title": "[LOW] Inspect the unknown deployment state",
                    "instructions": "Inspect the current deployment and confirm unknowns.",
                    "kind": "research",
                    "success_criteria": "deployment state understood",
                    "session_id": null,
                    "llm_confidence": "unlikely",
                    "llm_confidence_rationale": "missing state"
                }
            ],
            "rejected_alternatives": []
        })
        .to_string(),
    )
    .await;
    config.model = "gpt-4o-mini".to_string();
    config.api_key = "test-key".to_string();
    config.api_transport = ApiTransport::ChatCompletions;
    config.auto_retry = false;
    config.max_retries = 0;
    config.max_tool_loops = 1;
    config.uncertainty.enabled = true;
    let engine = AgentEngine::new_test(manager, config, root.path()).await;

    let goal_run_id = "goal-low-confidence-plan";
    let mut goal_run = sample_goal_run_with_kind(
        goal_run_id,
        GoalRunStepKind::Research,
        "Inspect the deployment before taking action",
    );
    goal_run.status = GoalRunStatus::Queued;
    goal_run.steps.clear();
    goal_run.current_step_index = 0;
    goal_run.current_step_title = None;
    goal_run.current_step_kind = None;
    goal_run.thread_id = Some("thread-low-confidence-plan".to_string());
    engine.goal_runs.lock().await.push_back(goal_run);

    engine
        .plan_goal_run(goal_run_id)
        .await
        .expect("planning should succeed");

    let awaiting_goal = engine
        .get_goal_run(goal_run_id)
        .await
        .expect("goal run should exist");
    assert_eq!(awaiting_goal.status, GoalRunStatus::AwaitingApproval);
    let approval_id = awaiting_goal
        .awaiting_approval_id
        .clone()
        .expect("low-confidence plan should produce an approval id");

    let approval_task = engine
        .tasks
        .lock()
        .await
        .iter()
        .find(|task| task.awaiting_approval_id.as_deref() == Some(approval_id.as_str()))
        .cloned()
        .expect("low-confidence plan should create a task-backed approval");
    assert_eq!(approval_task.source, "goal_plan_approval");
    assert_eq!(approval_task.status, TaskStatus::AwaitingApproval);

    assert!(
        engine
            .handle_task_approval_resolution(
                &approval_id,
                amux_protocol::ApprovalDecision::ApproveOnce
            )
            .await,
        "approval resolution should succeed for low-confidence plan reviews"
    );

    let resumed_goal = engine
        .get_goal_run(goal_run_id)
        .await
        .expect("goal run should still exist");
    assert_eq!(resumed_goal.status, GoalRunStatus::Running);
    assert!(resumed_goal.awaiting_approval_id.is_none());

    let resolved_task = engine
        .tasks
        .lock()
        .await
        .iter()
        .find(|task| task.id == approval_task.id)
        .cloned()
        .expect("approval task should still exist");
    assert_eq!(resolved_task.status, TaskStatus::Completed);
    assert!(resolved_task.awaiting_approval_id.is_none());
}

#[tokio::test]
async fn plan_goal_run_populates_dossier_units_and_proof_checks() {
    let root = tempdir().expect("temp dir");
    let manager = SessionManager::new_test(root.path()).await;
    let recorded_bodies =
        std::sync::Arc::new(std::sync::Mutex::new(std::collections::VecDeque::new()));
    let mut config = AgentConfig::default();
    config.provider = "openai".to_string();
    config.base_url = crate::agent::tests::spawn_goal_recording_server(
        recorded_bodies,
        serde_json::json!({
            "title": "Titan first slice",
            "summary": "Build the first validated slice.",
            "steps": [
                {
                    "title": "Create the Android shell",
                    "instructions": "Create the app shell and wire the first screen.",
                    "kind": "command",
                    "success_criteria": "shell builds successfully",
                    "execution_binding": "builtin:swarog",
                    "verification_binding": "subagent:android-verifier",
                    "proof_checks": [
                        {
                            "id": "proof-build-debug",
                            "title": "assembleDebug passes",
                            "summary": "Run the Android debug build successfully."
                        }
                    ],
                    "session_id": null,
                    "llm_confidence": "likely",
                    "llm_confidence_rationale": "bounded first slice"
                }
            ],
            "rejected_alternatives": []
        })
        .to_string(),
    )
    .await;
    config.model = "gpt-4o-mini".to_string();
    config.api_key = "test-key".to_string();
    config.api_transport = ApiTransport::ChatCompletions;
    config.auto_retry = false;
    config.max_retries = 0;
    config.max_tool_loops = 1;
    let engine = AgentEngine::new_test(manager, config, root.path()).await;

    let goal_run_id = "goal-dossier-plan";
    let mut goal_run = sample_goal_run_with_kind(
        goal_run_id,
        GoalRunStepKind::Command,
        "Create the Android shell",
    );
    goal_run.status = GoalRunStatus::Queued;
    goal_run.steps.clear();
    goal_run.current_step_index = 0;
    goal_run.current_step_title = None;
    goal_run.current_step_kind = None;
    engine.goal_runs.lock().await.push_back(goal_run);

    engine
        .plan_goal_run(goal_run_id)
        .await
        .expect("planning should succeed");

    let updated_goal = engine
        .get_goal_run(goal_run_id)
        .await
        .expect("goal run should exist");
    let updated_goal_json = goal_run_json(&updated_goal);
    let planner_owner = updated_goal_json
        .get("planner_owner_profile")
        .and_then(serde_json::Value::as_object)
        .expect("planned goal should include planner owner metadata");
    assert_eq!(
        planner_owner
            .get("agent_label")
            .and_then(serde_json::Value::as_str),
        Some(crate::agent::agent_identity::MAIN_AGENT_NAME)
    );
    assert_eq!(
        planner_owner
            .get("provider")
            .and_then(serde_json::Value::as_str),
        Some("openai")
    );
    assert_eq!(
        planner_owner
            .get("model")
            .and_then(serde_json::Value::as_str),
        Some("gpt-4o-mini")
    );
    assert_eq!(
        planner_owner
            .get("reasoning_effort")
            .and_then(serde_json::Value::as_str),
        Some("high")
    );
    assert!(
        updated_goal_json
            .get("current_step_owner_profile")
            .is_none(),
        "newly planned goal should not yet have a current step owner"
    );
    let dossier = updated_goal.dossier.expect("dossier should be populated");
    assert_eq!(dossier.units.len(), 1);
    assert_eq!(dossier.units[0].title, "[LOW] Create the Android shell");
    assert_eq!(
        dossier.units[0].execution_binding,
        GoalRoleBinding::Builtin("swarog".to_string())
    );
    assert_eq!(
        dossier.units[0].verification_binding,
        GoalRoleBinding::Subagent("android-verifier".to_string())
    );
    assert_eq!(dossier.units[0].proof_checks.len(), 1);
    assert_eq!(dossier.units[0].proof_checks[0].id, "proof-build-debug");
}
