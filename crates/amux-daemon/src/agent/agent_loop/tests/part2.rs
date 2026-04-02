use super::*;

#[tokio::test]
async fn send_message_request_includes_runtime_continuity_and_negative_knowledge() {
    let recorded_bodies = Arc::new(StdMutex::new(VecDeque::new()));
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.provider = "openai".to_string();
    config.base_url = spawn_recording_assistant_server(recorded_bodies.clone()).await;
    config.model = "gpt-4o-mini".to_string();
    config.api_key = "test-key".to_string();
    config.api_transport = ApiTransport::ChatCompletions;
    config.auto_retry = false;
    config.max_retries = 0;
    config.max_tool_loops = 1;

    let engine = AgentEngine::new_test(manager, config, root.path()).await;
    let thread_id = "thread-runtime-continuity-request";

    {
        let mut threads = engine.threads.write().await;
        threads.insert(
            thread_id.to_string(),
            crate::agent::types::AgentThread {
                id: thread_id.to_string(),
                title: "Runtime continuity thread".to_string(),
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
            id: "task-runtime-continuity-request".to_string(),
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
            goal_run_id: Some("goal-runtime-1".to_string()),
            goal_run_title: Some("Test goal".to_string()),
            goal_step_id: Some("step-runtime-1".to_string()),
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
            id: "goal-runtime-1".to_string(),
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
            session_id: None,
            current_step_index: 0,
            current_step_title: Some("Investigate failure".to_string()),
            current_step_kind: Some(crate::agent::types::GoalRunStepKind::Research),
            replan_count: 0,
            max_replans: 2,
            plan_summary: None,
            reflection_summary: None,
            memory_updates: Vec::new(),
            generated_skill_path: None,
            last_error: None,
            failure_cause: None,
            child_task_ids: vec!["task-runtime-continuity-request".to_string()],
            child_task_count: 1,
            approval_count: 0,
            awaiting_approval_id: None,
            active_task_id: Some("task-runtime-continuity-request".to_string()),
            duration_ms: None,
            steps: vec![crate::agent::types::GoalRunStep {
                id: "step-runtime-1".to_string(),
                position: 0,
                title: "Investigate failure".to_string(),
                instructions: "Inspect the failing path".to_string(),
                kind: crate::agent::types::GoalRunStepKind::Research,
                success_criteria: "Know why it failed".to_string(),
                session_id: None,
                status: crate::agent::types::GoalRunStepStatus::InProgress,
                task_id: Some("task-runtime-continuity-request".to_string()),
                summary: None,
                error: None,
                started_at: Some(1),
                completed_at: None,
            }],
            events: Vec::new(),
            total_prompt_tokens: 0,
            total_completion_tokens: 0,
            estimated_cost_usd: None,
            autonomy_level: Default::default(),
            authorship_tag: None,
        });
    }

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
    }
    engine
        .add_negative_constraint(crate::agent::episodic::NegativeConstraint {
            id: "nk-runtime-test-1".to_string(),
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
        .expect("seed negative knowledge for runtime prompt");

    let outcome = engine
        .send_message_inner(
            Some(thread_id),
            "Investigate the failure",
            Some("task-runtime-continuity-request"),
            None,
            None,
            None,
            None,
            true,
        )
        .await
        .expect("send message should complete");

    assert!(!outcome.interrupted_for_approval);

    let recorded = recorded_bodies
        .lock()
        .expect("lock recorded assistant bodies");
    assert!(
        recorded
            .iter()
            .any(|body| body.contains("## Working Continuity")),
        "expected the execution prompt to include the continuity summary section",
    );
    assert!(
        recorded.iter().any(|body| body.contains("I am carrying this forward as")
            && body.contains(MAIN_AGENT_NAME)
            && body.contains("Test goal")
            && body.contains("Investigate failure")
            && body.contains("I am continuing the same workstream")),
        "expected the execution prompt to include active persona identity plus explicit goal, step, and task titles",
    );
    assert!(
        recorded
            .iter()
            .any(|body| body.contains("## Ruled-Out Approaches (Negative Knowledge)")),
        "expected the execution prompt to include ruled-out approaches",
    );
    assert!(
        recorded
            .iter()
            .any(|body| body.contains("The old recovery path already failed twice.")),
        "expected the execution prompt to include matching negative knowledge",
    );
}

#[tokio::test]
async fn transport_incompatibility_does_not_mutate_persisted_config_and_emits_notice() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let request_counter = Arc::new(AtomicUsize::new(0));
    let mut config = AgentConfig::default();
    config.provider = "custom".to_string();
    config.base_url = spawn_transport_incompatibility_server(request_counter.clone()).await;
    config.model = "gpt-4.1".to_string();
    config.api_key = "test-key".to_string();
    config.auth_source = AuthSource::ApiKey;
    config.api_transport = ApiTransport::Responses;
    config.auto_retry = false;
    config.max_retries = 0;
    config.providers.insert(
        "custom".to_string(),
        ProviderConfig {
            base_url: config.base_url.clone(),
            model: config.model.clone(),
            api_key: config.api_key.clone(),
            assistant_id: String::new(),
            auth_source: AuthSource::ApiKey,
            api_transport: ApiTransport::Responses,
            reasoning_effort: String::new(),
            context_window_tokens: 0,
            response_schema: None,
        },
    );

    let engine = AgentEngine::new_test(manager, config, root.path()).await;
    engine.persist_config().await;
    let mut events = engine.subscribe();

    let _error = match engine
        .send_message_inner(None, "hello", None, None, None, None, None, true)
        .await
    {
        Ok(_) => panic!("transport incompatibility should fail the turn"),
        Err(error) => error,
    };

    let stored_config = engine.config.read().await.clone();
    assert_eq!(stored_config.api_transport, ApiTransport::Responses);
    assert_eq!(
        stored_config
            .providers
            .get("custom")
            .expect("provider entry")
            .api_transport,
        ApiTransport::Responses
    );

    let persisted_items = engine
        .history
        .list_agent_config_items()
        .await
        .expect("list persisted config items");
    let persisted = crate::agent::config::load_config_from_items(persisted_items)
        .expect("decode persisted config");
    assert_eq!(persisted.api_transport, ApiTransport::Responses);
    assert_eq!(
        persisted
            .providers
            .get("custom")
            .expect("persisted provider entry")
            .api_transport,
        ApiTransport::Responses
    );

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let mut saw_notice = false;
    let mut saw_done = false;
    let mut seen_notice_kinds = Vec::new();
    while let Ok(event) = events.try_recv() {
        match event {
            AgentEvent::WorkflowNotice {
                kind,
                message,
                details,
                ..
            } => {
                seen_notice_kinds.push(kind.clone());
                if kind == "transport-incompatible" || kind == "upstream-error" {
                    saw_notice = true;
                    assert!(
                        message.contains("incompatible")
                            || details
                                .as_deref()
                                .is_some_and(|d| d.contains("transport_incompatible"))
                    );
                    let details = details.expect("notice should include diagnostics");
                    assert!(details.contains("transport_incompatible"));
                    assert!(details.contains("Responses API not supported"));
                }
            }
            AgentEvent::Done { .. } => saw_done = true,
            _ => {}
        }
    }

    assert!(
        saw_notice,
        "expected operator-visible transport incompatibility notice, saw {:?}",
        seen_notice_kinds
    );
    assert!(
        saw_done,
        "expected turn completion event for surfaced error"
    );
    assert_eq!(request_counter.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn auto_retry_wait_repeats_scheduled_retry_cycles_until_cancelled() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let request_counter = Arc::new(AtomicUsize::new(0));
    let mut config = AgentConfig::default();
    config.provider = "custom".to_string();
    config.base_url = spawn_transient_transport_failure_server(request_counter.clone()).await;
    config.model = "gpt-4.1".to_string();
    config.api_key = "test-key".to_string();
    config.auth_source = AuthSource::ApiKey;
    config.api_transport = ApiTransport::ChatCompletions;
    config.auto_retry = true;
    config.max_retries = 1;
    config.retry_delay_ms = 100;
    config.providers.insert(
        "custom".to_string(),
        ProviderConfig {
            base_url: config.base_url.clone(),
            model: config.model.clone(),
            api_key: config.api_key.clone(),
            assistant_id: String::new(),
            auth_source: AuthSource::ApiKey,
            api_transport: ApiTransport::ChatCompletions,
            reasoning_effort: String::new(),
            context_window_tokens: 0,
            response_schema: None,
        },
    );

    let engine = AgentEngine::new_test(manager, config, root.path()).await;
    let thread_id = "thread-auto-retry-loop";
    {
        let mut threads = engine.threads.write().await;
        threads.insert(
            thread_id.to_string(),
            crate::agent::types::AgentThread {
                id: thread_id.to_string(),
                title: "Auto retry loop".to_string(),
                messages: vec![crate::agent::types::AgentMessage::user("hello", 1)],
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

    let mut task = tokio::spawn({
        let engine = engine.clone();
        async move {
            engine
                .send_message_inner(Some(thread_id), "hello", None, None, None, None, None, true)
                .await
        }
    });

    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(1700), async {
            loop {
                if request_counter.load(Ordering::SeqCst) > 4 {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        })
        .await
        .is_ok(),
        "expected multiple scheduled retry cycles to perform real reconnect attempts"
    );
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(50), &mut task)
            .await
            .is_err(),
        "send loop should keep retrying until explicitly cancelled"
    );
    assert!(
        engine.stop_stream(thread_id).await,
        "stream should be cancellable"
    );

    let result = tokio::time::timeout(std::time::Duration::from_secs(1), task)
        .await
        .expect("cancelled retry loop should finish");
    let joined = result.expect("join send task");
    if let Err(error) = joined {
        panic!("cancelled retry loop should end cleanly: {error}");
    }
}

#[tokio::test]
async fn structured_upstream_diagnostics_are_not_persisted_or_streamed_to_user() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let request_counter = Arc::new(AtomicUsize::new(0));
    let mut config = AgentConfig::default();
    config.provider = "custom".to_string();
    config.base_url = spawn_transport_incompatibility_server(request_counter.clone()).await;
    config.model = "gpt-4.1".to_string();
    config.api_key = "test-key".to_string();
    config.auth_source = AuthSource::ApiKey;
    config.api_transport = ApiTransport::Responses;
    config.auto_retry = false;
    config.max_retries = 0;

    let engine = AgentEngine::new_test(manager, config, root.path()).await;
    let mut events = engine.subscribe();

    let error = match engine
        .send_message_inner(None, "hello", None, None, None, None, None, true)
        .await
    {
        Ok(_) => panic!("structured upstream failure should fail the turn"),
        Err(error) => error,
    };
    assert!(
        error.to_string().contains(UPSTREAM_DIAGNOSTICS_MARKER),
        "precondition: returned error still carries structured diagnostics envelope"
    );

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let mut saw_error_delta = false;
    while let Ok(event) = events.try_recv() {
        if let AgentEvent::Delta { content, .. } = event {
            if content.starts_with("Error: ") {
                saw_error_delta = true;
                assert!(
                    !content.contains(UPSTREAM_DIAGNOSTICS_MARKER),
                    "error delta should not expose structured diagnostics"
                );
            }
        }
    }
    assert!(saw_error_delta, "expected streamed error delta");

    let threads = engine.threads.read().await;
    let thread = threads.values().next().expect("thread should be created");
    let assistant_error = thread
        .messages
        .iter()
        .find(|message| {
            message.role == MessageRole::Assistant && message.content.starts_with("Error: ")
        })
        .expect("assistant error should be persisted");
    assert!(
        !assistant_error
            .content
            .contains(UPSTREAM_DIAGNOSTICS_MARKER),
        "persisted assistant error should not include structured diagnostics"
    );
}

#[tokio::test]
async fn concierge_recovery_fixable_request_invalid_starts_one_background_investigation() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

    let structured = StructuredUpstreamFailure {
        class: "request_invalid".to_string(),
        summary: "provider rejected the daemon request as invalid: Invalid 'input[12].name': empty string".to_string(),
        diagnostics: serde_json::json!({
            "raw_message": "Invalid 'input[12].name': empty string"
        }),
    };
    let mut attempted = std::collections::HashSet::new();

    let first = engine
        .maybe_recover_fixable_upstream_failure(
            "thread-recovery",
            &structured,
            false,
            false,
            &mut attempted,
        )
        .await
        .expect("recovery evaluation should succeed");
    let second = engine
        .maybe_recover_fixable_upstream_failure(
            "thread-recovery",
            &structured,
            false,
            false,
            &mut attempted,
        )
        .await
        .expect("repeat recovery evaluation should succeed");

    assert!(first.started_investigation);
    assert!(first.retry_attempted);
    assert_eq!(
        first.signature.as_deref(),
        Some("request-invalid-empty-tool-name")
    );
    assert!(!second.started_investigation);
    assert!(!second.retry_attempted);

    let tasks = engine.tasks.lock().await;
    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0].source, "concierge_recovery");
    assert_eq!(
        tasks[0].parent_thread_id.as_deref(),
        Some("thread-recovery")
    );
}

#[tokio::test]
async fn concierge_recovery_transport_signature_is_blocked_after_committed_output() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

    let structured = StructuredUpstreamFailure {
        class: "transport_incompatible".to_string(),
        summary: "The selected provider/transport combination is incompatible: request body mismatch from stale thread state".to_string(),
        diagnostics: serde_json::json!({
            "details": "request body mismatch from stale thread state"
        }),
    };
    let mut attempted = std::collections::HashSet::new();

    let disposition = engine
        .maybe_recover_fixable_upstream_failure(
            "thread-visible-output",
            &structured,
            true,
            false,
            &mut attempted,
        )
        .await
        .expect("recovery evaluation should succeed");

    assert!(disposition.started_investigation);
    assert!(!disposition.retry_attempted);

    let tasks = engine.tasks.lock().await;
    assert_eq!(tasks.len(), 1);
}
