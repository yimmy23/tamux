use super::*;

#[tokio::test]
async fn send_message_request_uses_spawned_persona_identity_in_continuity_summary() {
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
    let thread_id = "thread-spawned-runtime-continuity";
    let spawned_persona_id = crate::agent::agent_identity::RADOGOST_AGENT_ID;
    let spawned_persona_name = crate::agent::agent_identity::RADOGOST_AGENT_NAME;
    let override_prompt = format!(
        "{} {}\n{} {}\nYou are {} ({}) operating as a spawned tamux agent.",
        crate::agent::agent_identity::PERSONA_MARKER,
        spawned_persona_name,
        crate::agent::agent_identity::PERSONA_ID_MARKER,
        spawned_persona_id,
        spawned_persona_name,
        spawned_persona_id,
    );

    {
        let mut threads = engine.threads.write().await;
        threads.insert(
            thread_id.to_string(),
            crate::agent::types::AgentThread {
                id: thread_id.to_string(),
                title: "Spawned runtime continuity thread".to_string(),
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
            id: "task-spawned-runtime-continuity".to_string(),
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
            goal_run_id: Some("goal-spawned-runtime-1".to_string()),
            goal_run_title: Some("Spawned goal".to_string()),
            goal_step_id: Some("step-spawned-runtime-1".to_string()),
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
            override_system_prompt: Some(override_prompt),
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
        let mut stores = engine.episodic_store.write().await;
        let store = stores.entry(spawned_persona_id.to_string()).or_default();
        store.counter_who.current_focus = Some("Tool: read_file".to_string());
    }

    let outcome = engine
        .send_message_inner(
            Some(thread_id),
            "Investigate the failure",
            Some("task-spawned-runtime-continuity"),
            None,
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
        "expected the execution prompt to include a continuity summary for spawned personas",
    );
    assert!(
        recorded.iter().any(|body| body.contains(&format!(
            "I am carrying this forward as {spawned_persona_name}."
        ))),
        "expected continuity summary to use the spawned persona name",
    );
    assert!(
        recorded
            .iter()
            .any(|body| body.contains("comparing tradeoffs")),
        "expected continuity summary to include the spawned persona guidance",
    );
    assert!(
        recorded.iter().all(
            |body| !body.contains(&format!("I am carrying this forward as {MAIN_AGENT_NAME}."))
        ),
        "spawned persona continuity should not fall back to the main agent name",
    );
}
