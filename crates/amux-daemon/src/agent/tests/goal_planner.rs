use super::*;
use crate::session_manager::SessionManager;
use tempfile::tempdir;

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
        replan_count: 0,
        max_replans: 2,
        plan_summary: Some("plan".to_string()),
        reflection_summary: None,
        memory_updates: Vec::new(),
        generated_skill_path: None,
        last_error: None,
        failure_cause: None,
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
        total_prompt_tokens: 0,
        total_completion_tokens: 0,
        estimated_cost_usd: None,
        autonomy_level: super::autonomy::AutonomyLevel::Supervised,
        authorship_tag: None,
    }
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
        )
        .await;

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
        replan_count: 0,
        max_replans: 2,
        plan_summary: Some("plan".to_string()),
        reflection_summary: None,
        memory_updates: Vec::new(),
        generated_skill_path: None,
        last_error: None,
        failure_cause: None,
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
        total_prompt_tokens: 0,
        total_completion_tokens: 0,
        estimated_cost_usd: None,
        autonomy_level: super::autonomy::AutonomyLevel::Aware,
        authorship_tag: None,
    }
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
