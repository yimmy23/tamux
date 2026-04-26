use super::*;

#[tokio::test]
async fn post_tool_policy_checkpoint_pivots_for_non_error_stuckness_with_runtime_side_effect() {
    let recorded_bodies = Arc::new(StdMutex::new(VecDeque::new()));
    let root = tempdir().unwrap();
    let readable_path = root.path().join("policy-loop-note.txt");
    std::fs::write(&readable_path, "ok\n").expect("seed readable file");
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.provider = "openai".to_string();
    config.base_url = spawn_policy_pivot_tool_call_server(
        recorded_bodies.clone(),
        readable_path.display().to_string(),
    )
    .await;
    config.model = "gpt-4o-mini".to_string();
    config.api_key = "test-key".to_string();
    config.api_transport = ApiTransport::ChatCompletions;
    config.auto_retry = false;
    config.max_retries = 0;
    config.max_tool_loops = 2;

    let engine = AgentEngine::new_test(manager, config, root.path()).await;
    let thread_id = "thread-policy-non-error-loop-proof";

    {
        let mut threads = engine.threads.write().await;
        threads.insert(
            thread_id.to_string(),
            crate::agent::types::AgentThread {
                id: thread_id.to_string(),
                agent_name: None,
                title: "Policy thread".to_string(),
                messages: vec![crate::agent::types::AgentMessage::user(
                    "Investigate the failure",
                    1,
                )],
                pinned: false,
                upstream_thread_id: None,
                upstream_transport: None,
                upstream_provider: None,
                upstream_model: None,
                upstream_assistant_id: None,
                total_input_tokens: 0,
                total_output_tokens: 0,
                created_at: 1,
                updated_at: 1,
            },
        );
    }

    {
        let mut tasks = engine.tasks.lock().await;
        tasks.push_back(crate::agent::types::AgentTask {
            id: "task-policy-non-error-loop-proof".to_string(),
            title: "Investigate failure".to_string(),
            description: "Inspect the failing path".to_string(),
            status: TaskStatus::InProgress,
            priority: crate::agent::types::TaskPriority::Normal,
            progress: 0,
            created_at: 1,
            started_at: Some(1),
            completed_at: None,
            error: None,
            result: None,
            thread_id: Some(thread_id.to_string()),
            source: "goal_run".to_string(),
            notify_on_complete: false,
            notify_channels: Vec::new(),
            dependencies: Vec::new(),
            command: None,
            session_id: None,
            goal_run_id: Some("goal-1".to_string()),
            goal_run_title: Some("Test goal".to_string()),
            goal_step_id: Some("step-1".to_string()),
            goal_step_title: Some("Investigate failure".to_string()),
            parent_task_id: None,
            parent_thread_id: None,
            runtime: "daemon".to_string(),
            retry_count: 1,
            max_retries: 2,
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
            override_provider: None,
            override_model: None,
            override_system_prompt: None,
            context_budget_tokens: None,
            context_overflow_action: None,
            termination_conditions: None,
            success_criteria: None,
            max_duration_secs: None,
            supervisor_config: None,
            sub_agent_def_id: None,
        });
    }
    {
        let mut goal_runs = engine.goal_runs.lock().await;
        goal_runs.push_back(crate::agent::types::GoalRun {
            id: "goal-1".to_string(),
            title: "Test goal".to_string(),
            goal: "Recover from repeated failure".to_string(),
            client_request_id: None,
            status: crate::agent::types::GoalRunStatus::Running,
            priority: crate::agent::types::TaskPriority::Normal,
            created_at: 1,
            updated_at: 1,
            started_at: Some(1),
            completed_at: None,
            thread_id: Some(thread_id.to_string()),
            root_thread_id: Some(thread_id.to_string()),
            active_thread_id: Some(thread_id.to_string()),
            execution_thread_ids: vec![thread_id.to_string()],
            session_id: None,
            current_step_index: 0,
            current_step_title: Some("Investigate failure".to_string()),
            current_step_kind: Some(crate::agent::types::GoalRunStepKind::Research),
            launch_assignment_snapshot: Vec::new(),
            runtime_assignment_list: Vec::new(),
            planner_owner_profile: None,
            current_step_owner_profile: None,
            replan_count: 0,
            max_replans: 2,
            plan_summary: None,
            reflection_summary: None,
            memory_updates: Vec::new(),
            generated_skill_path: None,
            last_error: None,
            failure_cause: None,
            stopped_reason: None,
            child_task_ids: vec!["task-policy-non-error-loop-proof".to_string()],
            child_task_count: 1,
            approval_count: 0,
            awaiting_approval_id: None,
            policy_fingerprint: None,
            approval_expires_at: None,
            containment_scope: None,
            compensation_status: None,
            compensation_summary: None,
            active_task_id: Some("task-policy-non-error-loop-proof".to_string()),
            duration_ms: None,
            steps: vec![crate::agent::types::GoalRunStep {
                id: "step-1".to_string(),
                position: 0,
                title: "Investigate failure".to_string(),
                instructions: "Inspect the failing path".to_string(),
                kind: crate::agent::types::GoalRunStepKind::Research,
                success_criteria: "Know why it failed".to_string(),
                session_id: None,
                status: crate::agent::types::GoalRunStepStatus::InProgress,
                task_id: Some("task-policy-non-error-loop-proof".to_string()),
                summary: None,
                error: None,
                started_at: Some(1),
                completed_at: None,
            }],
            events: Vec::new(),
            dossier: None,
            total_prompt_tokens: 0,
            total_completion_tokens: 0,
            estimated_cost_usd: None,
            model_usage: Vec::new(),
            autonomy_level: Default::default(),
            authorship_tag: None,
        });
    }

    let tool_args = serde_json::json!({
        "path": readable_path.display().to_string(),
        "offset": 0,
        "limit": 1,
    })
    .to_string();
    let args_hash =
        crate::agent::episodic::counter_who::compute_approach_hash("read_file", &tool_args);
    {
        let mut stores = engine.episodic_store.write().await;
        let store = stores
            .entry(crate::agent::agent_identity::MAIN_AGENT_ID.to_string())
            .or_default();
        store.counter_who.current_focus = Some("Tool: read_file".to_string());
        store.counter_who.correction_patterns = vec![crate::agent::episodic::CorrectionPattern {
            pattern: "Inspect workspace state before retrying".to_string(),
            correction_count: 2,
            last_correction_at: 3,
        }];
        store.counter_who.tried_approaches = VecDeque::from(vec![
            crate::agent::episodic::TriedApproach {
                approach_hash: args_hash.clone(),
                description: format!("read_file({tool_args})"),
                outcome: crate::agent::episodic::EpisodeOutcome::Failure,
                timestamp: 1,
            },
            crate::agent::episodic::TriedApproach {
                approach_hash: args_hash.clone(),
                description: format!("read_file({tool_args})"),
                outcome: crate::agent::episodic::EpisodeOutcome::Failure,
                timestamp: 2,
            },
        ])
        .into_iter()
        .collect();
    }
    engine
        .add_negative_constraint(crate::agent::episodic::NegativeConstraint {
            id: "nk-policy-test-1".to_string(),
            episode_id: None,
            constraint_type: crate::agent::episodic::ConstraintType::RuledOut,
            subject: "Investigate failure".to_string(),
            solution_class: Some("recovery".to_string()),
            description: "The old recovery path already failed twice.".to_string(),
            confidence: 0.95,
            state: crate::agent::episodic::ConstraintState::Dead,
            evidence_count: 2,
            direct_observation: true,
            derived_from_constraint_ids: Vec::new(),
            related_subject_tokens: vec!["investigate".to_string(), "failure".to_string()],
            valid_until: Some(crate::agent::now_millis() + 60_000),
            created_at: crate::agent::now_millis(),
        })
        .await
        .expect("seed negative knowledge for policy prompt");
    {
        let mut awareness = engine.awareness.write().await;
        for ts in 1..=4 {
            awareness.record_outcome(
                thread_id,
                "thread",
                "read_file",
                &args_hash,
                false,
                false,
                ts,
            );
        }
    }

    let task_snapshot = {
        let tasks = engine.tasks.lock().await;
        tasks
            .iter()
            .find(|task| task.id == "task-policy-non-error-loop-proof")
            .cloned()
            .expect("task")
    };
    let recent_tool_outcomes = vec![summarize_tool_result_for_policy(
        "read_file",
        &ToolResult {
            tool_call_id: "call_policy_read_1".to_string(),
            name: "read_file".to_string(),
            content: "ok\n".to_string(),
            is_error: false,
            weles_review: None,
            pending_approval: None,
        },
    )];

    let action = apply_post_tool_policy_checkpoint(
        &engine,
        thread_id,
        "task-policy-non-error-loop-proof",
        &task_snapshot,
        &args_hash,
        &recent_tool_outcomes,
        10,
    )
    .await
    .expect("policy checkpoint should succeed")
    .expect("policy checkpoint should apply an action");

    assert_eq!(
        action,
        crate::agent::orchestrator_policy::PolicyLoopAction::RestartLoop
    );

    let scope = crate::agent::orchestrator_policy::PolicyDecisionScope {
        thread_id: thread_id.to_string(),
        goal_run_id: Some("goal-1".to_string()),
    };
    let recent_decision = engine
        .latest_policy_decision(&scope, 10)
        .await
        .expect("expected checkpoint to persist a policy decision");
    assert_eq!(
        recent_decision.decision.action,
        crate::agent::orchestrator_policy::PolicyAction::Pivot
    );
    assert_eq!(
        recent_decision.decision.strategy_hint.as_deref(),
        Some("Inspect the workspace state before more reads.")
    );

    let recorded = recorded_bodies.lock().expect("lock recorded bodies");
    assert!(
        recorded.iter().any(|body| body
            .contains("tamux orchestrator should continue, pivot, escalate, or halt_retries")),
        "expected the policy checkpoint to issue the orchestrator policy evaluation prompt",
    );
    assert!(
        recorded
            .iter()
            .any(|body| body.contains("## Continuity summary")),
        "expected the policy prompt to include continuity summary context",
    );
    assert!(
        recorded
            .iter()
            .any(|body| body.contains("Continuity summary")
                && body.contains("I am carrying this forward as")
                && body.contains(MAIN_AGENT_NAME)
                && body.contains("Test goal")
                && body.contains("Investigate failure")),
        "expected the policy continuity summary to include persona identity plus explicit goal and task titles",
    );
    assert!(
        recorded
            .iter()
            .any(|body| body.contains("## Ruled-out approaches")),
        "expected the policy prompt to include ruled-out approaches context",
    );
    assert!(
        recorded
            .iter()
            .any(|body| body.contains("The old recovery path already failed twice.")),
        "expected the policy prompt to surface matching negative knowledge",
    );
    drop(recorded);

    let threads = engine.threads.read().await;
    let thread = threads.get(thread_id).expect("thread");
    assert!(
        thread.messages.iter().any(|message| {
            message.role == MessageRole::System
                && message
                    .content
                    .contains("Inspect the workspace state before more reads.")
        }),
        "expected pivot application to inject a strategy refresh system message",
    );
}
