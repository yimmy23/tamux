use super::*;
use amux_shared::providers::{
    PROVIDER_ID_CUSTOM, PROVIDER_ID_MINIMAX_CODING_PLAN, PROVIDER_ID_OPENAI,
};

#[tokio::test]
async fn send_message_request_includes_runtime_continuity_and_negative_knowledge() {
    let recorded_bodies = Arc::new(StdMutex::new(VecDeque::new()));
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.provider = PROVIDER_ID_OPENAI.to_string();
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
                agent_name: None,
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
            policy_fingerprint: None,
            approval_expires_at: None,
            containment_scope: None,
            compensation_status: None,
            compensation_summary: None,
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
        recorded
            .iter()
            .any(|body| body.contains("I am carrying this forward as")
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
async fn direct_weles_handoff_turn_uses_weles_persona_prompt() {
    let recorded_bodies = Arc::new(StdMutex::new(VecDeque::new()));
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.provider = PROVIDER_ID_OPENAI.to_string();
    config.base_url = spawn_recording_assistant_server(recorded_bodies.clone()).await;
    config.model = "gpt-4o-mini".to_string();
    config.api_key = "test-key".to_string();
    config.api_transport = ApiTransport::ChatCompletions;
    config.auto_retry = false;
    config.max_retries = 0;
    config.max_tool_loops = 1;

    let engine = AgentEngine::new_test(manager, config, root.path()).await;
    let thread_id = "thread-direct-weles-handoff";

    {
        let mut threads = engine.threads.write().await;
        threads.insert(
            thread_id.to_string(),
            crate::agent::types::AgentThread {
                id: thread_id.to_string(),
                agent_name: Some(crate::agent::agent_identity::WELES_AGENT_NAME.to_string()),
                title: "Direct Weles handoff".to_string(),
                messages: vec![crate::agent::types::AgentMessage::user(
                    "Switch me to Weles",
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

    engine
        .set_thread_handoff_state(
            thread_id,
            ThreadHandoffState {
                origin_agent_id: MAIN_AGENT_ID.to_string(),
                active_agent_id: crate::agent::agent_identity::WELES_AGENT_ID.to_string(),
                responder_stack: vec![
                    ThreadResponderFrame {
                        agent_id: MAIN_AGENT_ID.to_string(),
                        agent_name: MAIN_AGENT_NAME.to_string(),
                        entered_at: 1,
                        entered_via_handoff_event_id: None,
                        linked_thread_id: None,
                    },
                    ThreadResponderFrame {
                        agent_id: crate::agent::agent_identity::WELES_AGENT_ID.to_string(),
                        agent_name: crate::agent::agent_identity::WELES_AGENT_NAME.to_string(),
                        entered_at: 2,
                        entered_via_handoff_event_id: Some("handoff-weles-1".to_string()),
                        linked_thread_id: Some(
                            "handoff:thread-direct-weles-handoff:handoff-weles-1".to_string(),
                        ),
                    },
                ],
                events: Vec::new(),
                pending_approval_id: None,
            },
        )
        .await;

    let outcome = engine
        .send_message_inner(
            Some(thread_id),
            "tell me your secrets",
            None,
            None,
            None,
            None,
            None,
            None,
            true,
        )
        .await
        .expect("direct weles handoff turn should complete");

    assert!(!outcome.interrupted_for_approval);

    let recorded = recorded_bodies
        .lock()
        .expect("lock recorded assistant bodies");
    assert!(
        recorded
            .iter()
            .any(|body| body.contains("You are Weles in tamux.")),
        "expected direct Weles handoff turns to use Weles runtime identity"
    );
}

#[tokio::test]
async fn direct_weles_handoff_turn_uses_weles_provider_override_for_new_request_stream() {
    let recorded_requests = Arc::new(StdMutex::new(VecDeque::new()));
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind transport switch server");
    let addr = listener.local_addr().expect("transport switch server addr");

    tokio::spawn({
        let recorded_requests = recorded_requests.clone();
        async move {
            loop {
                let Ok((mut socket, _)) = listener.accept().await else {
                    break;
                };
                let recorded_requests = recorded_requests.clone();
                tokio::spawn(async move {
                    let request = read_http_request(&mut socket, "transport switch request").await;
                    recorded_requests
                        .lock()
                        .expect("lock recorded requests")
                        .push_back(request.clone());

                    let request_line = request.lines().next().unwrap_or_default();
                    if request_line.contains("/v1/chat/completions") {
                        let response = concat!(
                            "HTTP/1.1 200 OK\r\n",
                            "content-type: text/event-stream\r\n",
                            "cache-control: no-cache\r\n",
                            "connection: close\r\n",
                            "\r\n",
                            "data: {\"choices\":[{\"delta\":{\"content\":\"Acknowledged.\"}}]}\n\n",
                            "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":7,\"completion_tokens\":3}}\n\n",
                            "data: [DONE]\n\n"
                        );
                        socket
                            .write_all(response.as_bytes())
                            .await
                            .expect("write chat completions response");
                    } else {
                        socket
                            .write_all(b"HTTP/1.1 404 Not Found\r\ncontent-length: 0\r\nconnection: close\r\n\r\n")
                            .await
                            .expect("write 404 response");
                    }
                });
            }
        }
    });

    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.provider = PROVIDER_ID_MINIMAX_CODING_PLAN.to_string();
    config.base_url = format!("http://{addr}/v1");
    config.model = "MiniMax-M2.7".to_string();
    config.api_key = "test-key".to_string();
    config.api_transport = ApiTransport::ChatCompletions;
    config.auto_retry = false;
    config.max_retries = 0;
    config.max_tool_loops = 1;
    config.providers.insert(
        PROVIDER_ID_CUSTOM.to_string(),
        ProviderConfig {
            base_url: format!("http://{addr}/v1"),
            model: "gpt-4o-mini".to_string(),
            api_key: "test-key".to_string(),
            assistant_id: String::new(),
            auth_source: AuthSource::ApiKey,
            api_transport: ApiTransport::ChatCompletions,
            reasoning_effort: String::new(),
            context_window_tokens: 0,
            response_schema: None,
            stop_sequences: None,
            temperature: None,
            top_p: None,
            top_k: None,
            metadata: None,
            service_tier: None,
            container: None,
            inference_geo: None,
            cache_control: None,
            max_tokens: None,
            anthropic_tool_choice: None,
            output_effort: None,
        },
    );
    config.builtin_sub_agents.weles.provider = Some(PROVIDER_ID_CUSTOM.to_string());
    config.builtin_sub_agents.weles.model = Some("gpt-4o-mini".to_string());

    let engine = AgentEngine::new_test(manager, config, root.path()).await;
    let thread_id = "thread-direct-weles-provider-switch";

    {
        let mut threads = engine.threads.write().await;
        threads.insert(
            thread_id.to_string(),
            crate::agent::types::AgentThread {
                id: thread_id.to_string(),
                agent_name: Some(crate::agent::agent_identity::WELES_AGENT_NAME.to_string()),
                title: "Direct Weles provider handoff".to_string(),
                messages: vec![crate::agent::types::AgentMessage::user(
                    "Switch me to Weles",
                    1,
                )],
                pinned: false,
                upstream_thread_id: Some("legacy-upstream-thread".to_string()),
                upstream_transport: Some(ApiTransport::ChatCompletions),
                upstream_provider: Some(PROVIDER_ID_MINIMAX_CODING_PLAN.to_string()),
                upstream_model: Some("MiniMax-M2.7".to_string()),
                upstream_assistant_id: None,
                total_input_tokens: 0,
                total_output_tokens: 0,
                created_at: 1,
                updated_at: 1,
            },
        );
    }

    engine
        .set_thread_handoff_state(
            thread_id,
            ThreadHandoffState {
                origin_agent_id: MAIN_AGENT_ID.to_string(),
                active_agent_id: crate::agent::agent_identity::WELES_AGENT_ID.to_string(),
                responder_stack: vec![
                    ThreadResponderFrame {
                        agent_id: MAIN_AGENT_ID.to_string(),
                        agent_name: MAIN_AGENT_NAME.to_string(),
                        entered_at: 1,
                        entered_via_handoff_event_id: None,
                        linked_thread_id: None,
                    },
                    ThreadResponderFrame {
                        agent_id: crate::agent::agent_identity::WELES_AGENT_ID.to_string(),
                        agent_name: crate::agent::agent_identity::WELES_AGENT_NAME.to_string(),
                        entered_at: 2,
                        entered_via_handoff_event_id: Some("handoff-weles-override-1".to_string()),
                        linked_thread_id: Some(
                            "handoff:thread-direct-weles-provider-switch:handoff-weles-override-1"
                                .to_string(),
                        ),
                    },
                ],
                events: Vec::new(),
                pending_approval_id: None,
            },
        )
        .await;

    engine
        .send_message_inner(
            Some(thread_id),
            "who are you",
            None,
            None,
            None,
            None,
            None,
            None,
            true,
        )
        .await
        .expect("direct weles handoff turn should use Weles provider override");

    let recorded = recorded_requests
        .lock()
        .expect("lock recorded transport-switch requests");
    let request = recorded
        .iter()
        .find(|request| request.contains("POST /v1/chat/completions"))
        .expect("expected Weles handoff to open a fresh chat completions request");
    let body = request_body(request);
    assert!(body.contains("You are Weles in tamux."));
}

#[tokio::test]
async fn new_targeted_weles_thread_uses_weles_runtime_provider_and_model() {
    let recorded_requests = Arc::new(StdMutex::new(VecDeque::new()));
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.provider = PROVIDER_ID_OPENAI.to_string();
    config.base_url = "http://127.0.0.1:1/v1".to_string();
    config.model = "svarog-model".to_string();
    config.api_key = "test-key".to_string();
    config.api_transport = ApiTransport::ChatCompletions;
    config.reasoning_effort = "high".to_string();
    config.auto_retry = false;
    config.max_retries = 0;
    config.max_tool_loops = 1;
    config.providers.insert(
        "custom-weles".to_string(),
        ProviderConfig {
            base_url: spawn_recording_request_server(recorded_requests.clone()).await,
            model: "weles-model".to_string(),
            api_key: "test-key".to_string(),
            assistant_id: String::new(),
            auth_source: AuthSource::ApiKey,
            api_transport: ApiTransport::ChatCompletions,
            reasoning_effort: "medium".to_string(),
            context_window_tokens: 0,
            response_schema: None,
            stop_sequences: None,
            temperature: None,
            top_p: None,
            top_k: None,
            metadata: None,
            service_tier: None,
            container: None,
            inference_geo: None,
            cache_control: None,
            max_tokens: None,
            anthropic_tool_choice: None,
            output_effort: None,
        },
    );
    config.builtin_sub_agents.weles.provider = Some("custom-weles".to_string());
    config.builtin_sub_agents.weles.model = Some("weles-model".to_string());
    config.builtin_sub_agents.weles.reasoning_effort = Some("medium".to_string());

    let engine = AgentEngine::new_test(manager, config, root.path()).await;

    let thread_id = engine
        .send_message_with_session_surface_and_target(
            None,
            None,
            "Review this change",
            None,
            Some("weles"),
        )
        .await
        .expect("new targeted Weles thread should complete");

    let recorded = recorded_requests
        .lock()
        .expect("lock targeted weles requests");
    let request = recorded
        .iter()
        .find(|request| request.contains("POST /v1/chat/completions"))
        .expect("expected Weles-targeted thread to use the Weles provider request");
    let body = request_body(request);
    assert!(body.contains("You are Weles in tamux."));
    assert!(body.contains("weles-model"));
    assert!(!body.contains("svarog-model"));

    let threads = engine.threads.read().await;
    let thread = threads.get(&thread_id).expect("thread should exist");
    assert_eq!(thread.agent_name.as_deref(), Some(WELES_AGENT_NAME));
}

#[tokio::test]
async fn new_targeted_rarog_thread_uses_concierge_runtime_provider_and_model() {
    let recorded_requests = Arc::new(StdMutex::new(VecDeque::new()));
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind new targeted rarog server");
    let addr = listener
        .local_addr()
        .expect("new targeted rarog server addr");

    tokio::spawn({
        let recorded_requests = recorded_requests.clone();
        async move {
            loop {
                let Ok((mut socket, _)) = listener.accept().await else {
                    break;
                };
                let recorded_requests = recorded_requests.clone();
                tokio::spawn(async move {
                    let request =
                        read_http_request(&mut socket, "new targeted rarog request").await;
                    recorded_requests
                        .lock()
                        .expect("lock targeted rarog requests")
                        .push_back(request.clone());

                    let request_line = request.lines().next().unwrap_or_default();
                    if request_line.contains("/v1/chat/completions") {
                        let response = concat!(
                            "HTTP/1.1 200 OK\r\n",
                            "content-type: text/event-stream\r\n",
                            "cache-control: no-cache\r\n",
                            "connection: close\r\n",
                            "\r\n",
                            "data: {\"choices\":[{\"delta\":{\"content\":\"Acknowledged.\"}}]}\n\n",
                            "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":7,\"completion_tokens\":3}}\n\n",
                            "data: [DONE]\n\n"
                        );
                        socket
                            .write_all(response.as_bytes())
                            .await
                            .expect("write targeted rarog response");
                    } else {
                        socket
                            .write_all(b"HTTP/1.1 404 Not Found\r\ncontent-length: 0\r\nconnection: close\r\n\r\n")
                            .await
                            .expect("write targeted rarog 404 response");
                    }
                });
            }
        }
    });

    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.provider = PROVIDER_ID_MINIMAX_CODING_PLAN.to_string();
    config.base_url = format!("http://{addr}/v1");
    config.model = "svarog-model".to_string();
    config.api_key = "test-key".to_string();
    config.api_transport = ApiTransport::ChatCompletions;
    config.reasoning_effort = "high".to_string();
    config.auto_retry = false;
    config.max_retries = 0;
    config.max_tool_loops = 1;
    config.providers.insert(
        PROVIDER_ID_CUSTOM.to_string(),
        ProviderConfig {
            base_url: format!("http://{addr}/v1"),
            model: "rarog-model".to_string(),
            api_key: "test-key".to_string(),
            assistant_id: String::new(),
            auth_source: AuthSource::ApiKey,
            api_transport: ApiTransport::ChatCompletions,
            reasoning_effort: "medium".to_string(),
            context_window_tokens: 0,
            response_schema: None,
            stop_sequences: None,
            temperature: None,
            top_p: None,
            top_k: None,
            metadata: None,
            service_tier: None,
            container: None,
            inference_geo: None,
            cache_control: None,
            max_tokens: None,
            anthropic_tool_choice: None,
            output_effort: None,
        },
    );
    config.concierge.provider = Some(PROVIDER_ID_CUSTOM.to_string());
    config.concierge.model = Some("rarog-model".to_string());
    config.concierge.reasoning_effort = Some("medium".to_string());

    let engine = AgentEngine::new_test(manager, config, root.path()).await;

    let thread_id = engine
        .send_message_with_session_surface_and_target(
            None,
            None,
            "Triage this issue",
            None,
            Some("rarog"),
        )
        .await
        .expect("new targeted Rarog thread should complete");

    let recorded = recorded_requests
        .lock()
        .expect("lock targeted rarog requests");
    let request = recorded
        .iter()
        .find(|request| request.contains("POST /v1/chat/completions"))
        .expect("expected Rarog-targeted thread to use the concierge provider request");
    let body = request_body(request);
    let body_json: serde_json::Value =
        serde_json::from_str(&body).expect("recorded request body should be valid json");
    assert!(body.contains("You are the tamux concierge"));
    assert_eq!(
        body_json.get("model").and_then(|value| value.as_str()),
        Some("rarog-model")
    );

    let threads = engine.threads.read().await;
    let thread = threads.get(&thread_id).expect("thread should exist");
    assert_eq!(thread.agent_name.as_deref(), Some(CONCIERGE_AGENT_NAME));
}

#[tokio::test]
async fn new_targeted_rarog_thread_prefers_concierge_model_override_over_stored_provider_model() {
    let recorded_requests = Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind targeted rarog override server");
    let addr = listener
        .local_addr()
        .expect("targeted rarog override server addr");

    tokio::spawn({
        let recorded_requests = recorded_requests.clone();
        async move {
            loop {
                let Ok((mut socket, _)) = listener.accept().await else {
                    break;
                };
                let mut buf = vec![0u8; 131072];
                let n = socket
                    .read(&mut buf)
                    .await
                    .expect("read targeted rarog override request");
                if n == 0 {
                    continue;
                }
                let request = String::from_utf8_lossy(&buf[..n]).to_string();
                recorded_requests
                    .lock()
                    .expect("lock targeted rarog override requests")
                    .push(request.clone());

                if request.starts_with("POST /v1/chat/completions") {
                    let response = concat!(
                        "HTTP/1.1 200 OK\r\n",
                        "content-type: text/event-stream\r\n",
                        "cache-control: no-cache\r\n",
                        "connection: close\r\n",
                        "\r\n",
                        "data: {\"choices\":[{\"delta\":{\"content\":\"Acknowledged.\"}}]}\n\n",
                        "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":7,\"completion_tokens\":3}}\n\n",
                        "data: [DONE]\n\n"
                    );
                    socket
                        .write_all(response.as_bytes())
                        .await
                        .expect("write targeted rarog override response");
                } else {
                    socket
                        .write_all(b"HTTP/1.1 404 Not Found\r\ncontent-length: 0\r\nconnection: close\r\n\r\n")
                        .await
                        .expect("write targeted rarog override 404 response");
                }
            }
        }
    });

    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.provider = PROVIDER_ID_MINIMAX_CODING_PLAN.to_string();
    config.base_url = format!("http://{addr}/v1");
    config.model = "svarog-model".to_string();
    config.api_key = "test-key".to_string();
    config.api_transport = ApiTransport::ChatCompletions;
    config.reasoning_effort = "high".to_string();
    config.auto_retry = false;
    config.max_retries = 0;
    config.max_tool_loops = 1;
    config.providers.insert(
        PROVIDER_ID_CUSTOM.to_string(),
        ProviderConfig {
            base_url: format!("http://{addr}/v1"),
            model: "MiniMax-M2.5".to_string(),
            api_key: "test-key".to_string(),
            assistant_id: String::new(),
            auth_source: AuthSource::ApiKey,
            api_transport: ApiTransport::ChatCompletions,
            reasoning_effort: "medium".to_string(),
            context_window_tokens: 0,
            response_schema: None,
            stop_sequences: None,
            temperature: None,
            top_p: None,
            top_k: None,
            metadata: None,
            service_tier: None,
            container: None,
            inference_geo: None,
            cache_control: None,
            max_tokens: None,
            anthropic_tool_choice: None,
            output_effort: None,
        },
    );
    config.concierge.provider = Some(PROVIDER_ID_CUSTOM.to_string());
    config.concierge.model = Some("qwen3.5-plus".to_string());
    config.concierge.reasoning_effort = Some("medium".to_string());

    let engine = AgentEngine::new_test(manager, config, root.path()).await;

    engine
        .send_message_with_session_surface_and_target(
            None,
            None,
            "Triage this issue",
            None,
            Some("rarog"),
        )
        .await
        .expect("new targeted Rarog thread should honor concierge model override");

    let recorded = recorded_requests
        .lock()
        .expect("lock targeted rarog override requests");
    let request = recorded
        .iter()
        .find(|request| request.contains("POST /v1/chat/completions"))
        .expect("expected targeted Rarog thread to hit concierge provider request");
    assert!(
        request.contains("\"model\":\"qwen3.5-plus\""),
        "targeted Rarog request should carry concierge.model override"
    );
}

#[tokio::test]
async fn successful_handoff_tool_call_restarts_same_turn_under_requested_agent() {
    let request_counter = Arc::new(AtomicUsize::new(0));
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind handoff stop server");
    let addr = listener.local_addr().expect("handoff stop server addr");

    tokio::spawn({
        let request_counter = request_counter.clone();
        async move {
            loop {
                let Ok((mut socket, _)) = listener.accept().await else {
                    break;
                };
                let request_counter = request_counter.clone();
                tokio::spawn(async move {
                    let attempt = request_counter.fetch_add(1, Ordering::SeqCst);
                    let _request = read_http_request(&mut socket, "handoff stop request").await;
                    let response = if attempt == 0 {
                        concat!(
                            "HTTP/1.1 200 OK\r\n",
                            "content-type: text/event-stream\r\n",
                            "cache-control: no-cache\r\n",
                            "connection: close\r\n",
                            "\r\n",
                            "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call_handoff_1\",\"function\":{\"name\":\"handoff_thread_agent\",\"arguments\":\"{\\\"action\\\":\\\"push_handoff\\\",\\\"target_agent_id\\\":\\\"rarog\\\",\\\"reason\\\":\\\"Operator explicitly requested Rarog\\\",\\\"summary\\\":\\\"Switch control to Rarog\\\",\\\"requested_by\\\":\\\"user\\\"}\"}}]}}],\"usage\":{\"prompt_tokens\":7,\"completion_tokens\":3}}\n\n",
                            "data: [DONE]\n\n"
                        )
                    } else {
                        concat!(
                            "HTTP/1.1 200 OK\r\n",
                            "content-type: text/event-stream\r\n",
                            "cache-control: no-cache\r\n",
                            "connection: close\r\n",
                            "\r\n",
                            "data: {\"choices\":[{\"delta\":{\"content\":\"I'm Rarog.\"}}]}\n\n",
                            "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":7,\"completion_tokens\":3}}\n\n",
                            "data: [DONE]\n\n"
                        )
                    };
                    socket
                        .write_all(response.as_bytes())
                        .await
                        .expect("write handoff stop response");
                });
            }
        }
    });

    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.provider = PROVIDER_ID_OPENAI.to_string();
    config.base_url = format!("http://{addr}/v1");
    config.model = "gpt-4o-mini".to_string();
    config.api_key = "test-key".to_string();
    config.api_transport = ApiTransport::ChatCompletions;
    config.auto_retry = false;
    config.max_retries = 0;
    config.max_tool_loops = 2;

    let engine = AgentEngine::new_test(manager, config, root.path()).await;
    let thread_id = "thread-handoff-stops-turn";

    {
        let mut threads = engine.threads.write().await;
        threads.insert(
            thread_id.to_string(),
            crate::agent::types::AgentThread {
                id: thread_id.to_string(),
                agent_name: Some(crate::agent::agent_identity::MAIN_AGENT_NAME.to_string()),
                title: "Handoff stop thread".to_string(),
                messages: vec![crate::agent::types::AgentMessage::user("gimme Rarog", 1)],
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

    engine
        .send_message_inner(
            Some(thread_id),
            "gimme Rarog",
            None,
            None,
            None,
            None,
            None,
            None,
            true,
        )
        .await
        .expect("handoff should restart the same operator turn under Rarog");

    assert_eq!(request_counter.load(Ordering::SeqCst), 2);

    let threads = engine.threads.read().await;
    let thread = threads.get(thread_id).expect("thread should exist");
    assert!(thread
        .messages
        .iter()
        .any(|message| message.content == "I'm Rarog."));
}

#[tokio::test]
async fn successful_handoff_restarts_same_turn_under_requested_agent_with_summary() {
    let request_counter = Arc::new(AtomicUsize::new(0));
    let recorded_requests = Arc::new(StdMutex::new(VecDeque::new()));
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind handoff restart server");
    let addr = listener.local_addr().expect("handoff restart server addr");

    tokio::spawn({
        let request_counter = request_counter.clone();
        let recorded_requests = recorded_requests.clone();
        async move {
            loop {
                let Ok((mut socket, _)) = listener.accept().await else {
                    break;
                };
                let request_counter = request_counter.clone();
                let recorded_requests = recorded_requests.clone();
                tokio::spawn(async move {
                    let attempt = request_counter.fetch_add(1, Ordering::SeqCst);
                    let request = read_http_request(&mut socket, "handoff restart request").await;
                    recorded_requests
                        .lock()
                        .expect("lock recorded requests")
                        .push_back(request.clone());

                    let response = if attempt == 0 {
                        concat!(
                            "HTTP/1.1 200 OK\r\n",
                            "content-type: text/event-stream\r\n",
                            "cache-control: no-cache\r\n",
                            "connection: close\r\n",
                            "\r\n",
                            "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call_handoff_1\",\"function\":{\"name\":\"handoff_thread_agent\",\"arguments\":\"{\\\"action\\\":\\\"push_handoff\\\",\\\"target_agent_id\\\":\\\"weles\\\",\\\"reason\\\":\\\"Operator explicitly requested Weles\\\",\\\"summary\\\":\\\"Switch control to Weles and continue helping from governance scope\\\",\\\"requested_by\\\":\\\"user\\\"}\"}}]}}],\"usage\":{\"prompt_tokens\":7,\"completion_tokens\":3}}\n\n",
                            "data: [DONE]\n\n"
                        )
                    } else {
                        concat!(
                            "HTTP/1.1 200 OK\r\n",
                            "content-type: text/event-stream\r\n",
                            "cache-control: no-cache\r\n",
                            "connection: close\r\n",
                            "\r\n",
                            "data: {\"choices\":[{\"delta\":{\"content\":\"I'm Weles.\"}}]}\n\n",
                            "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":7,\"completion_tokens\":3}}\n\n",
                            "data: [DONE]\n\n"
                        )
                    };

                    socket
                        .write_all(response.as_bytes())
                        .await
                        .expect("write handoff restart response");
                });
            }
        }
    });

    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.provider = "custom-main".to_string();
    config.base_url = format!("http://{addr}/v1");
    config.model = "main-model".to_string();
    config.api_key = "test-key".to_string();
    config.api_transport = ApiTransport::ChatCompletions;
    config.auto_retry = false;
    config.max_retries = 0;
    config.max_tool_loops = 2;
    config.providers.insert(
        "custom-main".to_string(),
        ProviderConfig {
            base_url: format!("http://{addr}/v1"),
            model: "main-model".to_string(),
            api_key: "test-key".to_string(),
            assistant_id: String::new(),
            auth_source: AuthSource::ApiKey,
            api_transport: ApiTransport::ChatCompletions,
            reasoning_effort: String::new(),
            context_window_tokens: 0,
            response_schema: None,
            stop_sequences: None,
            temperature: None,
            top_p: None,
            top_k: None,
            metadata: None,
            service_tier: None,
            container: None,
            inference_geo: None,
            cache_control: None,
            max_tokens: None,
            anthropic_tool_choice: None,
            output_effort: None,
        },
    );
    config.providers.insert(
        "custom-weles".to_string(),
        ProviderConfig {
            base_url: format!("http://{addr}/v1"),
            model: "weles-model".to_string(),
            api_key: "test-key".to_string(),
            assistant_id: String::new(),
            auth_source: AuthSource::ApiKey,
            api_transport: ApiTransport::ChatCompletions,
            reasoning_effort: String::new(),
            context_window_tokens: 0,
            response_schema: None,
            stop_sequences: None,
            temperature: None,
            top_p: None,
            top_k: None,
            metadata: None,
            service_tier: None,
            container: None,
            inference_geo: None,
            cache_control: None,
            max_tokens: None,
            anthropic_tool_choice: None,
            output_effort: None,
        },
    );
    config.builtin_sub_agents.weles.provider = Some("custom-weles".to_string());
    config.builtin_sub_agents.weles.model = Some("weles-model".to_string());

    let engine = AgentEngine::new_test(manager, config, root.path()).await;
    let thread_id = "thread-handoff-restart-same-turn";

    {
        let mut threads = engine.threads.write().await;
        threads.insert(
            thread_id.to_string(),
            crate::agent::types::AgentThread {
                id: thread_id.to_string(),
                agent_name: Some(crate::agent::agent_identity::MAIN_AGENT_NAME.to_string()),
                title: "Handoff restart thread".to_string(),
                messages: vec![crate::agent::types::AgentMessage::user("give me Weles", 1)],
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

    engine
        .send_message_inner(
            Some(thread_id),
            "give me Weles",
            None,
            None,
            None,
            None,
            None,
            None,
            true,
        )
        .await
        .expect("handoff should restart the same operator turn under Weles");

    assert_eq!(request_counter.load(Ordering::SeqCst), 2);

    let recorded = recorded_requests
        .lock()
        .expect("lock handoff restart requests");
    let second_request = recorded
        .get(1)
        .expect("second request should exist after handoff restart");
    let second_body = request_body(second_request);
    assert!(second_body.contains("You are Weles in tamux."));
    assert!(second_body.contains("weles-model"));
    assert!(second_body.contains("User requested you while talking to Svarog"));
    assert!(
        second_body.contains("Switch control to Weles and continue helping from governance scope")
    );

    let threads = engine.threads.read().await;
    let thread = threads.get(thread_id).expect("thread should exist");
    assert!(thread
        .messages
        .iter()
        .any(|message| message.content == "I'm Weles."));
}

#[tokio::test]
async fn direct_rarog_handoff_turn_uses_real_concierge_runtime_config() {
    let recorded_requests = Arc::new(StdMutex::new(VecDeque::new()));
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind concierge runtime server");
    let addr = listener
        .local_addr()
        .expect("concierge runtime server addr");

    tokio::spawn({
        let recorded_requests = recorded_requests.clone();
        async move {
            loop {
                let Ok((mut socket, _)) = listener.accept().await else {
                    break;
                };
                let recorded_requests = recorded_requests.clone();
                tokio::spawn(async move {
                    let request = read_http_request(&mut socket, "concierge runtime request").await;
                    recorded_requests
                        .lock()
                        .expect("lock concierge requests")
                        .push_back(request.clone());

                    let request_line = request.lines().next().unwrap_or_default();
                    if request_line.contains("/v1/chat/completions") {
                        let response = concat!(
                            "HTTP/1.1 200 OK\r\n",
                            "content-type: text/event-stream\r\n",
                            "cache-control: no-cache\r\n",
                            "connection: close\r\n",
                            "\r\n",
                            "data: {\"choices\":[{\"delta\":{\"content\":\"Acknowledged.\"}}]}\n\n",
                            "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":7,\"completion_tokens\":3}}\n\n",
                            "data: [DONE]\n\n"
                        );
                        socket
                            .write_all(response.as_bytes())
                            .await
                            .expect("write concierge response");
                    } else {
                        socket
                            .write_all(b"HTTP/1.1 404 Not Found\r\ncontent-length: 0\r\nconnection: close\r\n\r\n")
                            .await
                            .expect("write 404 response");
                    }
                });
            }
        }
    });

    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.provider = PROVIDER_ID_MINIMAX_CODING_PLAN.to_string();
    config.base_url = format!("http://{addr}/v1");
    config.model = "MiniMax-M2.7".to_string();
    config.api_key = "test-key".to_string();
    config.api_transport = ApiTransport::ChatCompletions;
    config.auto_retry = false;
    config.max_retries = 0;
    config.max_tool_loops = 1;
    config.providers.insert(
        "custom".to_string(),
        ProviderConfig {
            base_url: format!("http://{addr}/v1"),
            model: "gpt-4o-mini".to_string(),
            api_key: "test-key".to_string(),
            assistant_id: String::new(),
            auth_source: AuthSource::ApiKey,
            api_transport: ApiTransport::ChatCompletions,
            reasoning_effort: String::new(),
            context_window_tokens: 0,
            response_schema: None,
            stop_sequences: None,
            temperature: None,
            top_p: None,
            top_k: None,
            metadata: None,
            service_tier: None,
            container: None,
            inference_geo: None,
            cache_control: None,
            max_tokens: None,
            anthropic_tool_choice: None,
            output_effort: None,
        },
    );
    config.concierge.provider = Some(PROVIDER_ID_CUSTOM.to_string());
    config.concierge.model = Some("gpt-4o-mini".to_string());

    let engine = AgentEngine::new_test(manager, config, root.path()).await;
    let thread_id = "thread-direct-rarog-provider-switch";

    {
        let mut threads = engine.threads.write().await;
        threads.insert(
            thread_id.to_string(),
            crate::agent::types::AgentThread {
                id: thread_id.to_string(),
                agent_name: Some(crate::agent::agent_identity::CONCIERGE_AGENT_NAME.to_string()),
                title: "Direct Rarog provider handoff".to_string(),
                messages: vec![crate::agent::types::AgentMessage::user("gimme Rarog", 1)],
                pinned: false,
                upstream_thread_id: Some("legacy-upstream-thread".to_string()),
                upstream_transport: Some(ApiTransport::ChatCompletions),
                upstream_provider: Some(PROVIDER_ID_MINIMAX_CODING_PLAN.to_string()),
                upstream_model: Some("MiniMax-M2.7".to_string()),
                upstream_assistant_id: None,
                total_input_tokens: 0,
                total_output_tokens: 0,
                created_at: 1,
                updated_at: 1,
            },
        );
    }

    engine
        .set_thread_handoff_state(
            thread_id,
            ThreadHandoffState {
                origin_agent_id: MAIN_AGENT_ID.to_string(),
                active_agent_id: crate::agent::agent_identity::CONCIERGE_AGENT_ID.to_string(),
                responder_stack: vec![
                    ThreadResponderFrame {
                        agent_id: MAIN_AGENT_ID.to_string(),
                        agent_name: MAIN_AGENT_NAME.to_string(),
                        entered_at: 1,
                        entered_via_handoff_event_id: None,
                        linked_thread_id: None,
                    },
                    ThreadResponderFrame {
                        agent_id: crate::agent::agent_identity::CONCIERGE_AGENT_ID.to_string(),
                        agent_name: crate::agent::agent_identity::CONCIERGE_AGENT_NAME.to_string(),
                        entered_at: 2,
                        entered_via_handoff_event_id: Some("handoff-rarog-override-1".to_string()),
                        linked_thread_id: Some(
                            "handoff:thread-direct-rarog-provider-switch:handoff-rarog-override-1"
                                .to_string(),
                        ),
                    },
                ],
                events: Vec::new(),
                pending_approval_id: None,
            },
        )
        .await;

    engine
        .send_message_inner(
            Some(thread_id),
            "who are you",
            None,
            None,
            None,
            None,
            None,
            None,
            true,
        )
        .await
        .expect("direct rarog handoff turn should use concierge runtime");

    let recorded = recorded_requests
        .lock()
        .expect("lock concierge runtime requests");
    let request = recorded
        .iter()
        .find(|request| request.contains("POST /v1/chat/completions"))
        .expect("expected concierge handoff to open a fresh chat completions request");
    let body = request_body(request);
    assert!(body.contains("You are the tamux concierge"));
    assert!(body.contains("Rarog"));
}

#[tokio::test]
async fn transport_incompatibility_does_not_mutate_persisted_config_and_emits_notice() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let request_counter = Arc::new(AtomicUsize::new(0));
    let mut config = AgentConfig::default();
    config.provider = PROVIDER_ID_CUSTOM.to_string();
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
            stop_sequences: None,
            temperature: None,
            top_p: None,
            top_k: None,
            metadata: None,
            service_tier: None,
            container: None,
            inference_geo: None,
            cache_control: None,
            max_tokens: None,
            anthropic_tool_choice: None,
            output_effort: None,
        },
    );

    let engine = AgentEngine::new_test(manager, config, root.path()).await;
    engine.persist_config().await;
    let mut events = engine.subscribe();

    let _error = match engine
        .send_message_inner(None, "hello", None, None, None, None, None, None, true)
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
async fn auto_retry_wait_escalates_to_fresh_runner_after_repeated_waits() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let request_counter = Arc::new(AtomicUsize::new(0));
    let success_request_started = Arc::new(tokio::sync::Notify::new());
    let release_success_request = Arc::new(tokio::sync::Notify::new());
    let mut config = AgentConfig::default();
    config.provider = PROVIDER_ID_CUSTOM.to_string();
    config.base_url = spawn_transient_failures_then_blocking_success_server(
        request_counter.clone(),
        2,
        success_request_started.clone(),
        release_success_request.clone(),
    )
    .await;
    config.model = "gpt-4.1".to_string();
    config.api_key = "test-key".to_string();
    config.auth_source = AuthSource::ApiKey;
    config.api_transport = ApiTransport::ChatCompletions;
    config.auto_retry = true;
    config.max_retries = 0;
    config.retry_delay_ms = 5;
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
            stop_sequences: None,
            temperature: None,
            top_p: None,
            top_k: None,
            metadata: None,
            service_tier: None,
            container: None,
            inference_geo: None,
            cache_control: None,
            max_tokens: None,
            anthropic_tool_choice: None,
            output_effort: None,
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
                agent_name: None,
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
                .send_message_inner(
                    Some(thread_id),
                    "hello",
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    true,
                )
                .await
        }
    });

    let initial_generation = tokio::time::timeout(std::time::Duration::from_secs(1), async {
        loop {
            if request_counter.load(Ordering::SeqCst) >= 1 {
                let streams = engine.stream_cancellations.lock().await;
                if let Some(entry) = streams.get(thread_id) {
                    break entry.generation;
                }
            }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("initial stream generation should be registered");

    tokio::time::timeout(
        std::time::Duration::from_secs(1),
        success_request_started.notified(),
    )
    .await
    .expect("fresh recovery request should start after repeated waits");

    let refreshed_generation = {
        let streams = engine.stream_cancellations.lock().await;
        streams
            .get(thread_id)
            .map(|entry| entry.generation)
            .expect("fresh recovery stream should replace the active stream entry")
    };
    assert!(
        refreshed_generation > initial_generation,
        "repeated auto-retry waits should replace the broken stream with a fresh runner"
    );

    release_success_request.notify_waiters();

    let result = tokio::time::timeout(std::time::Duration::from_secs(1), task)
        .await
        .expect("fresh recovery loop should finish");
    let joined = result.expect("join send task");
    let outcome = joined.expect("fresh recovery loop should succeed");
    assert_eq!(outcome.thread_id, thread_id);
    assert_eq!(request_counter.load(Ordering::SeqCst), 3);
}

#[tokio::test]
async fn structured_upstream_diagnostics_are_not_persisted_or_streamed_to_user() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let request_counter = Arc::new(AtomicUsize::new(0));
    let mut config = AgentConfig::default();
    config.provider = PROVIDER_ID_CUSTOM.to_string();
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
        .send_message_inner(None, "hello", None, None, None, None, None, None, true)
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
async fn retry_stream_now_replaces_waiting_stream_with_fresh_send_generation() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let request_counter = Arc::new(AtomicUsize::new(0));
    let release_second_request = Arc::new(tokio::sync::Notify::new());
    let mut config = AgentConfig::default();
    config.provider = PROVIDER_ID_CUSTOM.to_string();
    config.base_url = spawn_transient_failure_then_blocking_server(
        request_counter.clone(),
        release_second_request.clone(),
    )
    .await;
    config.model = "gpt-4.1".to_string();
    config.api_key = "test-key".to_string();
    config.auth_source = AuthSource::ApiKey;
    config.api_transport = ApiTransport::ChatCompletions;
    config.auto_retry = true;
    config.max_retries = 0;
    config.retry_delay_ms = 10_000;
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
            stop_sequences: None,
            temperature: None,
            top_p: None,
            top_k: None,
            metadata: None,
            service_tier: None,
            container: None,
            inference_geo: None,
            cache_control: None,
            max_tokens: None,
            anthropic_tool_choice: None,
            output_effort: None,
        },
    );

    let engine = AgentEngine::new_test(manager, config, root.path()).await;
    let thread_id = "thread-retry-now-refreshes-stream";
    {
        let mut threads = engine.threads.write().await;
        threads.insert(
            thread_id.to_string(),
            crate::agent::types::AgentThread {
                id: thread_id.to_string(),
                agent_name: None,
                title: "Retry refresh".to_string(),
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

    let mut events = engine.subscribe();
    let mut send_task = tokio::spawn({
        let engine = engine.clone();
        async move {
            engine
                .send_message_inner(
                    Some(thread_id),
                    "hello",
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    true,
                )
                .await
        }
    });

    let waiting_generation = tokio::time::timeout(std::time::Duration::from_secs(1), async {
        loop {
            match events.recv().await {
                Ok(AgentEvent::RetryStatus {
                    thread_id: event_thread_id,
                    phase,
                    ..
                }) if event_thread_id == thread_id && phase == "waiting" => {
                    let streams = engine.stream_cancellations.lock().await;
                    break streams
                        .get(thread_id)
                        .map(|entry| entry.generation)
                        .expect("waiting retry stream should be registered");
                }
                Ok(_) => {}
                Err(_) => {}
            }
        }
    })
    .await
    .expect("retry waiting status should appear");

    assert!(
        engine.retry_stream_now(thread_id).await,
        "retry-now should start a fresh resend"
    );

    tokio::time::timeout(std::time::Duration::from_secs(1), async {
        while request_counter.load(Ordering::SeqCst) < 2 {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("retry-now should perform a second request");

    let refreshed_generation = {
        let streams = engine.stream_cancellations.lock().await;
        streams
            .get(thread_id)
            .map(|entry| entry.generation)
            .expect("fresh resend should replace the active stream entry")
    };
    assert!(
        refreshed_generation > waiting_generation,
        "retry-now should replace the waiting stream with a fresh generation"
    );

    let original = tokio::time::timeout(std::time::Duration::from_secs(1), &mut send_task)
        .await
        .expect("original send task should stop once retry-now spawns a fresh send")
        .expect("original send join should succeed");
    if let Err(error) = original {
        panic!("original send task should finish cleanly after retry-now: {error}");
    }

    assert!(
        engine.stop_stream(thread_id).await,
        "fresh resend stream should still be cancellable"
    );
    release_second_request.notify_waiters();
}

#[tokio::test]
async fn anthropic_transport_retry_restarts_with_fresh_runner_state() {
    let recorded_bodies = Arc::new(StdMutex::new(VecDeque::new()));
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.provider = PROVIDER_ID_MINIMAX_CODING_PLAN.to_string();
    config.base_url = spawn_anthropic_rebuild_sensitive_retry_server(recorded_bodies.clone()).await;
    config.model = "MiniMax-M2.7".to_string();
    config.api_key = "test-key".to_string();
    config.auth_source = AuthSource::ApiKey;
    config.api_transport = ApiTransport::ChatCompletions;
    config.auto_retry = true;
    config.max_retries = 1;
    config.retry_delay_ms = 200;

    let engine = AgentEngine::new_test(manager, config, root.path()).await;
    let thread_id = "thread-anthropic-fresh-retry";
    {
        let mut threads = engine.threads.write().await;
        threads.insert(
            thread_id.to_string(),
            crate::agent::types::AgentThread {
                id: thread_id.to_string(),
                agent_name: None,
                title: "Anthropic fresh retry".to_string(),
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

    let send_task = tokio::spawn({
        let engine = engine.clone();
        async move {
            engine
                .send_message_inner(
                    Some(thread_id),
                    "hello",
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    true,
                )
                .await
        }
    });

    tokio::time::timeout(std::time::Duration::from_secs(1), async {
        loop {
            if recorded_bodies
                .lock()
                .expect("lock recorded anthropic bodies")
                .len()
                >= 1
            {
                let mut threads = engine.threads.write().await;
                let thread = threads.get_mut(thread_id).expect("thread should exist");
                let last_user = thread
                    .messages
                    .iter_mut()
                    .rev()
                    .find(|message| message.role == MessageRole::User)
                    .expect("user message should exist");
                last_user.content = "hello again".to_string();
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("first anthropic request should be recorded");

    let outcome = tokio::time::timeout(std::time::Duration::from_secs(2), send_task)
        .await
        .expect("send task should complete")
        .expect("send task join should succeed")
        .expect("fresh runner retry should succeed");

    assert_eq!(outcome.thread_id, thread_id);

    let recorded = recorded_bodies
        .lock()
        .expect("lock recorded anthropic bodies");
    assert!(
        recorded.len() >= 2,
        "expected at least two anthropic requests, saw {recorded:?}"
    );
    assert!(
        recorded[0].contains("\"hello\""),
        "expected the initial anthropic request to use the original thread content: {}",
        recorded[0]
    );
    assert!(
        recorded[1].contains("hello again"),
        "expected the retried anthropic request to rebuild from fresh thread state: {}",
        recorded[1]
    );
}

#[tokio::test]
async fn anthropic_outer_auto_retry_restarts_with_fresh_runner_state() {
    let recorded_bodies = Arc::new(StdMutex::new(VecDeque::new()));
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.provider = PROVIDER_ID_CUSTOM.to_string();
    config.base_url = spawn_anthropic_rebuild_sensitive_retry_server(recorded_bodies.clone()).await;
    config.model = "claude-test".to_string();
    config.api_key = "test-key".to_string();
    config.auth_source = AuthSource::ApiKey;
    config.api_transport = ApiTransport::ChatCompletions;
    config.auto_retry = true;
    config.max_retries = 0;
    config.retry_delay_ms = 200;

    let engine = AgentEngine::new_test(manager, config, root.path()).await;
    let thread_id = "thread-anthropic-outer-fresh-retry";
    {
        let mut threads = engine.threads.write().await;
        threads.insert(
            thread_id.to_string(),
            crate::agent::types::AgentThread {
                id: thread_id.to_string(),
                agent_name: None,
                title: "Anthropic outer fresh retry".to_string(),
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

    let send_task = tokio::spawn({
        let engine = engine.clone();
        async move {
            engine
                .send_message_inner(
                    Some(thread_id),
                    "hello",
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    true,
                )
                .await
        }
    });

    tokio::time::timeout(std::time::Duration::from_secs(1), async {
        loop {
            if recorded_bodies
                .lock()
                .expect("lock recorded anthropic bodies")
                .len()
                >= 1
            {
                let mut threads = engine.threads.write().await;
                let thread = threads.get_mut(thread_id).expect("thread should exist");
                let last_user = thread
                    .messages
                    .iter_mut()
                    .rev()
                    .find(|message| message.role == MessageRole::User)
                    .expect("user message should exist");
                last_user.content = "hello again".to_string();
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("first anthropic request should be recorded");

    let outcome = tokio::time::timeout(std::time::Duration::from_secs(2), send_task)
        .await
        .expect("send task should complete")
        .expect("send task join should succeed")
        .expect("outer fresh runner retry should succeed");

    assert_eq!(outcome.thread_id, thread_id);

    let recorded = recorded_bodies
        .lock()
        .expect("lock recorded anthropic bodies");
    assert!(
        recorded.len() >= 2,
        "expected at least two anthropic requests, saw {recorded:?}"
    );
    assert!(
        recorded[0].contains("\"hello\""),
        "expected the initial anthropic request to use the original thread content: {}",
        recorded[0]
    );
    assert!(
        recorded[1].contains("hello again"),
        "expected the outer auto-retry anthropic request to rebuild from fresh thread state: {}",
        recorded[1]
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

#[tokio::test]
async fn strong_match_requires_read_skill_before_non_discovery_tool() {
    let root = tempdir().unwrap();
    let skill_dir = root.path().join("skills").join("systematic-debugging");
    fs::create_dir_all(&skill_dir).expect("create skills directory");
    fs::write(
        skill_dir.join("SKILL.md"),
        "# Systematic debugging\nUse this workflow to debug panic in rust service failures. Choose it when the task is to debug panic in rust service incidents.\n",
    )
    .expect("write skill");

    let readable_path = root.path().join("allowed.txt");
    fs::write(&readable_path, "allowed through\n").expect("write readable file");

    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.provider = PROVIDER_ID_OPENAI.to_string();
    config.base_url = spawn_scripted_tool_call_server(vec![(
        "read_file".to_string(),
        serde_json::json!({ "path": readable_path }).to_string(),
    )])
    .await;
    config.model = "gpt-4o-mini".to_string();
    config.api_key = "test-key".to_string();
    config.api_transport = ApiTransport::ChatCompletions;
    config.auto_retry = false;
    config.max_retries = 0;
    config.max_tool_loops = 1;
    config.skill_recommendation.strong_match_threshold = 0.60;
    config.skill_recommendation.weak_match_threshold = 0.30;

    let engine = AgentEngine::new_test(manager, config, root.path()).await;
    let discovery = engine
        .discover_skill_recommendations_public("debug panic in rust service", None, 3, None)
        .await
        .expect("skill discovery should succeed");
    assert_eq!(discovery.confidence_tier, "strong");
    assert!(
        discovery
            .recommended_action
            .contains("read_skill systematic-debugging"),
        "expected discovery to require reading the matched skill first: {}",
        discovery.recommended_action
    );

    let thread_id = "thread-strong-skill-gate";
    let mut events = engine.subscribe();

    let outcome = engine
        .send_message_inner(
            Some(thread_id),
            "debug panic in rust service",
            None,
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

    let mut saw_gate_notice = false;
    let mut saw_gate_result = false;
    while let Ok(event) = events.try_recv() {
        match event {
            AgentEvent::WorkflowNotice {
                thread_id: event_thread_id,
                kind,
                message,
                ..
            } if event_thread_id == thread_id && kind == "skill-gate" => {
                if message.contains("read_skill systematic-debugging") {
                    saw_gate_notice = true;
                }
            }
            AgentEvent::ToolResult {
                thread_id: event_thread_id,
                name,
                content,
                is_error,
                ..
            } if event_thread_id == thread_id && name == "read_file" => {
                saw_gate_result = true;
                assert!(is_error, "blocked tool should surface as an error result");
                assert!(
                    content.contains("read_skill systematic-debugging"),
                    "expected blocked tool result to point at the required skill read: {content}"
                );
            }
            _ => {}
        }
    }
    assert!(saw_gate_notice, "expected a skill gate workflow notice");
    assert!(saw_gate_result, "expected a blocked read_file tool result");

    let persisted = engine
        .history
        .get_thread(thread_id)
        .await
        .expect("read persisted thread")
        .expect("thread should persist");
    let metadata = persisted.metadata_json.expect("thread metadata");
    assert!(metadata.contains("\"recommended_skill\":\"systematic-debugging\""));
    assert!(metadata.contains("\"compliant\":false"));
}

#[tokio::test]
async fn weak_match_allows_progress_without_skip_rationale() {
    let root = tempdir().unwrap();
    let skill_dir = root.path().join("skills").join("debugging-playbook");
    fs::create_dir_all(&skill_dir).expect("create skills directory");
    fs::write(
        skill_dir.join("SKILL.md"),
        "# Debugging playbook\nUse this skill for generic debug investigations.\n",
    )
    .expect("write skill");

    let readable_path = root.path().join("allowed.txt");
    fs::write(&readable_path, "allowed through\n").expect("write readable file");

    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.provider = PROVIDER_ID_OPENAI.to_string();
    config.base_url = spawn_scripted_tool_call_server(vec![(
        "read_file".to_string(),
        serde_json::json!({ "path": readable_path }).to_string(),
    )])
    .await;
    config.model = "gpt-4o-mini".to_string();
    config.api_key = "test-key".to_string();
    config.api_transport = ApiTransport::ChatCompletions;
    config.auto_retry = false;
    config.max_retries = 0;
    config.max_tool_loops = 1;
    config.skill_recommendation.strong_match_threshold = 0.80;
    config.skill_recommendation.weak_match_threshold = 0.30;

    let engine = AgentEngine::new_test(manager, config, root.path()).await;
    let thread_id = "thread-weak-skill-gate";

    engine
        .send_message_inner(
            Some(thread_id),
            "debug panic",
            None,
            None,
            None,
            None,
            None,
            None,
            true,
        )
        .await
        .expect("initial send should complete");

    let first_read_attempt = {
        let threads = engine.threads.read().await;
        threads
            .get(thread_id)
            .expect("thread should exist")
            .messages
            .iter()
            .rev()
            .find(|message| {
                message.role == MessageRole::Tool
                    && message.tool_name.as_deref() == Some("read_file")
            })
            .cloned()
            .expect("initial read_file result should be recorded")
    };
    assert_eq!(first_read_attempt.tool_status.as_deref(), Some("done"));
    assert!(first_read_attempt.content.contains("allowed through"));

    let threads = engine.threads.read().await;
    let thread = threads.get(thread_id).expect("thread should exist");
    let skip_message = thread.messages.iter().find(|message| {
        message.role == MessageRole::Tool
            && message.tool_name.as_deref() == Some("justify_skill_skip")
    });
    assert!(
        skip_message.is_none(),
        "weak matches should not require justify_skill_skip before progress"
    );

    let persisted = engine
        .history
        .get_thread(thread_id)
        .await
        .expect("read persisted thread")
        .expect("thread should persist");
    let metadata = persisted.metadata_json.expect("thread metadata");
    assert!(!metadata.contains("\"skip_rationale\""));
    assert!(metadata.contains("\"recommended_action\":\"read_skill debugging-playbook\""));
    assert!(metadata.contains("\"compliant\":false"));
}

#[tokio::test]
async fn local_strong_match_still_runs_when_background_community_scout_enabled() {
    let root = tempdir().unwrap();
    let skill_dir = root.path().join("skills").join("systematic-debugging");
    fs::create_dir_all(&skill_dir).expect("create skills directory");
    fs::write(
        skill_dir.join("SKILL.md"),
        "# Systematic debugging\nUse this workflow to debug panic failures in rust services.\n",
    )
    .expect("write skill");

    let registry_dir = root.path().join("registry");
    fs::create_dir_all(&registry_dir).expect("create registry directory");
    fs::write(
        registry_dir.join("index.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "version": 1,
            "updated_at": 42,
            "skills": [{
                "name": "community-debugging-expert",
                "description": "Advanced panic debugging workflow from the registry.",
                "version": "1.0.0",
                "publisher_id": "publisher-1",
                "publisher_verified": true,
                "success_rate": 0.91,
                "use_count": 18,
                "content_hash": "abc123",
                "tamux_version": "0.3.1",
                "maturity_at_publish": "proven",
                "tags": ["debug", "rust", "panic"],
                "published_at": 42
            }]
        }))
        .expect("serialize registry index"),
    )
    .expect("write registry index");

    let readable_path = root.path().join("allowed.txt");
    fs::write(&readable_path, "allowed through\n").expect("write readable file");

    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.provider = PROVIDER_ID_OPENAI.to_string();
    config.base_url = spawn_scripted_tool_call_server(vec![(
        "read_file".to_string(),
        serde_json::json!({ "path": readable_path }).to_string(),
    )])
    .await;
    config.model = "gpt-4o-mini".to_string();
    config.api_key = "test-key".to_string();
    config.api_transport = ApiTransport::ChatCompletions;
    config.auto_retry = false;
    config.max_retries = 0;
    config.max_tool_loops = 1;
    config.skill_recommendation.strong_match_threshold = 0.60;
    config.skill_recommendation.weak_match_threshold = 0.30;
    config.skill_recommendation.background_community_search = true;
    config.extra.insert(
        "registry_url".to_string(),
        serde_json::Value::String("http://127.0.0.1:9".to_string()),
    );

    let engine = AgentEngine::new_test(manager, config, root.path()).await;
    let thread_id = "thread-community-scout-enabled";
    let mut events = engine.subscribe();

    engine
        .send_message_inner(
            Some(thread_id),
            "debug panic in rust service",
            None,
            None,
            None,
            None,
            None,
            None,
            true,
        )
        .await
        .expect("send message should complete");

    let persisted = engine
        .history
        .get_thread(thread_id)
        .await
        .expect("read persisted thread")
        .expect("thread should persist");
    let metadata = persisted.metadata_json.expect("thread metadata");
    assert!(metadata.contains("\"recommended_skill\":\"systematic-debugging\""));

    let scout_notice = tokio::time::timeout(std::time::Duration::from_millis(500), async {
        loop {
            match events.recv().await {
                Ok(AgentEvent::WorkflowNotice {
                    thread_id: event_thread_id,
                    kind,
                    details,
                    ..
                }) if event_thread_id == thread_id && kind == "skill-community-scout" => {
                    return details.expect("scout notice details");
                }
                Ok(_) => continue,
                Err(error) => panic!("workflow event stream closed unexpectedly: {error}"),
            }
        }
    })
    .await
    .expect("expected community scout notice");

    assert!(
        scout_notice.contains("community-debugging-expert"),
        "expected scout notice to include candidate payload, got: {scout_notice}"
    );
    assert!(scout_notice.contains("\"community_preapprove_timeout_secs\":30"));
}

#[tokio::test]
async fn disabled_background_community_scout_does_not_search_registry() {
    let root = tempdir().unwrap();
    let skill_dir = root.path().join("skills").join("systematic-debugging");
    fs::create_dir_all(&skill_dir).expect("create skills directory");
    fs::write(
        skill_dir.join("SKILL.md"),
        "# Systematic debugging\nUse this workflow to debug panic failures in rust services.\n",
    )
    .expect("write skill");

    let registry_dir = root.path().join("registry");
    fs::create_dir_all(&registry_dir).expect("create registry directory");
    fs::write(
        registry_dir.join("index.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "version": 1,
            "updated_at": 42,
            "skills": [{
                "name": "community-debugging-expert",
                "description": "Advanced panic debugging workflow from the registry.",
                "version": "1.0.0",
                "publisher_id": "publisher-1",
                "publisher_verified": true,
                "success_rate": 0.91,
                "use_count": 18,
                "content_hash": "abc123",
                "tamux_version": "0.3.1",
                "maturity_at_publish": "proven",
                "tags": ["debug", "rust", "panic"],
                "published_at": 42
            }]
        }))
        .expect("serialize registry index"),
    )
    .expect("write registry index");

    let readable_path = root.path().join("allowed.txt");
    fs::write(&readable_path, "allowed through\n").expect("write readable file");

    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.provider = PROVIDER_ID_OPENAI.to_string();
    config.base_url = spawn_scripted_tool_call_server(vec![(
        "read_file".to_string(),
        serde_json::json!({ "path": readable_path }).to_string(),
    )])
    .await;
    config.model = "gpt-4o-mini".to_string();
    config.api_key = "test-key".to_string();
    config.api_transport = ApiTransport::ChatCompletions;
    config.auto_retry = false;
    config.max_retries = 0;
    config.max_tool_loops = 1;
    config.skill_recommendation.strong_match_threshold = 0.60;
    config.skill_recommendation.weak_match_threshold = 0.30;
    config.skill_recommendation.background_community_search = false;
    config.extra.insert(
        "registry_url".to_string(),
        serde_json::Value::String("http://127.0.0.1:9".to_string()),
    );

    let engine = AgentEngine::new_test(manager, config, root.path()).await;
    let thread_id = "thread-community-scout-disabled";
    let mut events = engine.subscribe();

    engine
        .send_message_inner(
            Some(thread_id),
            "debug panic in rust service",
            None,
            None,
            None,
            None,
            None,
            None,
            true,
        )
        .await
        .expect("send message should complete");

    let saw_scout_notice = tokio::time::timeout(std::time::Duration::from_millis(250), async {
        loop {
            match events.recv().await {
                Ok(AgentEvent::WorkflowNotice {
                    thread_id: event_thread_id,
                    kind,
                    ..
                }) if event_thread_id == thread_id && kind == "skill-community-scout" => {
                    return true;
                }
                Ok(_) => continue,
                Err(_) => return false,
            }
        }
    })
    .await
    .unwrap_or(false);

    assert!(
        !saw_scout_notice,
        "community scout should stay disabled for this turn"
    );
}
