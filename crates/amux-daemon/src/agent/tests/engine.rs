#[cfg(test)]
use super::*;
use amux_shared::providers::{
    PROVIDER_ID_CHATGPT_SUBSCRIPTION, PROVIDER_ID_CUSTOM, PROVIDER_ID_GROQ, PROVIDER_ID_OPENAI,
};
use tempfile::TempDir;
use tokio::io::AsyncReadExt;
use tokio::net::TcpListener;

async fn make_test_engine(config: AgentConfig) -> (Arc<AgentEngine>, TempDir) {
    let temp_dir = TempDir::new().expect("temp dir");
    let session_manager = SessionManager::new_test(temp_dir.path()).await;
    let history = HistoryStore::new_test_store(temp_dir.path())
        .await
        .expect("history store");
    let data_dir = temp_dir.path().join("agent");
    std::fs::create_dir_all(&data_dir).expect("create agent data dir");
    let engine = AgentEngine::new_with_storage_and_http_client(
        session_manager,
        config,
        history,
        data_dir,
        build_agent_http_client(Duration::from_millis(75)),
    );
    (engine, temp_dir)
}

fn provider_config(
    base_url: &str,
    model: &str,
    api_key: &str,
    auth_source: AuthSource,
) -> ProviderConfig {
    ProviderConfig {
        base_url: base_url.to_string(),
        model: model.to_string(),
        api_key: api_key.to_string(),
        assistant_id: String::new(),
        auth_source,
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
    }
}

fn write_openai_subscription_auth() {
    let auth = serde_json::json!({
        "provider": "openai-codex",
        "auth_mode": "chatgpt_subscription",
        "access_token": "header.eyJodHRwczovL2FwaS5vcGVuYWkuY29tL2F1dGgiOnsiY2hhdGdwdF9hY2NvdW50X2lkIjoiacctMSJ9LCJleHAiOjQxMDI0NDQ4MDB9.signature",
        "refresh_token": "refresh-token",
        "account_id": "acct-1",
        "expires_at": 4_102_444_800_000i64,
        "source": "test",
        "updated_at": 4_102_444_800_000i64,
        "created_at": 4_102_444_800_000i64
    });
    super::provider_auth_store::save_provider_auth_state(
        PROVIDER_ID_OPENAI,
        PROVIDER_ID_CHATGPT_SUBSCRIPTION,
        &auth,
    )
    .expect("write auth fixture");
}

#[tokio::test]
async fn provider_alternative_excludes_placeholder_provider_row() {
    let mut config = AgentConfig::default();
    config.provider = PROVIDER_ID_OPENAI.to_string();
    config.providers.insert(
        PROVIDER_ID_CUSTOM.to_string(),
        provider_config("", "", "", AuthSource::ApiKey),
    );
    let (engine, _temp_dir) = make_test_engine(config).await;

    let suggestion = engine
        .suggest_alternative_provider(PROVIDER_ID_OPENAI)
        .await;

    assert!(
        suggestion.is_none(),
        "placeholder provider rows must not be suggested"
    );
}

#[tokio::test]
async fn hydrate_restores_full_persisted_thread_history() {
    let (engine, temp_dir) = make_test_engine(AgentConfig::default()).await;
    let thread_id = "thread-hydrate-full-history";
    let message_count = 550u64;

    engine.threads.write().await.insert(
        thread_id.to_string(),
        AgentThread {
            id: thread_id.to_string(),
            agent_name: Some(crate::agent::agent_identity::MAIN_AGENT_NAME.to_string()),
            title: "Hydrated Thread".to_string(),
            messages: (0..message_count)
                .map(|index| AgentMessage::user(format!("message-{index}"), 1_000 + index))
                .collect(),
            pinned: false,
            upstream_thread_id: None,
            upstream_transport: None,
            upstream_provider: None,
            upstream_model: None,
            upstream_assistant_id: None,
            created_at: 1_000,
            updated_at: 1_000 + message_count,
            total_input_tokens: 0,
            total_output_tokens: 0,
        },
    );
    engine.persist_thread_by_id(thread_id).await;

    let rehydrated = AgentEngine::new_test(
        SessionManager::new_test(temp_dir.path()).await,
        AgentConfig::default(),
        temp_dir.path(),
    )
    .await;
    rehydrated.hydrate().await.expect("hydrate should succeed");

    let thread = rehydrated
        .get_thread(thread_id)
        .await
        .expect("thread should be restored after hydrate");
    assert_eq!(thread.messages.len(), message_count as usize);
    assert_eq!(
        thread
            .messages
            .last()
            .map(|message| message.content.as_str()),
        Some("message-549")
    );
}

#[tokio::test]
async fn hydrate_restores_thread_token_totals_from_persisted_history() {
    let (engine, temp_dir) = make_test_engine(AgentConfig::default()).await;
    let thread_id = "thread-hydrate-token-totals";
    let assistant = AgentMessage {
        id: crate::agent::types::generate_message_id(),
        role: MessageRole::Assistant,
        content: "done".to_string(),
        tool_calls: None,
        tool_call_id: None,
        tool_name: None,
        tool_arguments: None,
        tool_status: None,
        weles_review: None,
        input_tokens: 11,
        output_tokens: 7,
        cost: None,
        provider: None,
        model: None,
        api_transport: None,
        response_id: None,
        upstream_message: None,
        provider_final_result: None,
        author_agent_id: None,
        author_agent_name: None,
        reasoning: None,
        message_kind: AgentMessageKind::Normal,
        compaction_strategy: None,
        compaction_payload: None,
        offloaded_payload_id: None,
        structural_refs: Vec::new(),
        timestamp: 1_001,
    };
    let mut summary = AgentMessage::user("summary", 1_002);
    summary.input_tokens = 3;
    summary.output_tokens = 2;
    summary.message_kind = AgentMessageKind::CompactionArtifact;

    engine.threads.write().await.insert(
        thread_id.to_string(),
        AgentThread {
            id: thread_id.to_string(),
            agent_name: Some(crate::agent::agent_identity::MAIN_AGENT_NAME.to_string()),
            title: "Hydrated Tokens".to_string(),
            messages: vec![assistant, summary],
            pinned: false,
            upstream_thread_id: None,
            upstream_transport: None,
            upstream_provider: None,
            upstream_model: None,
            upstream_assistant_id: None,
            created_at: 1_000,
            updated_at: 1_002,
            total_input_tokens: 14,
            total_output_tokens: 9,
        },
    );
    engine.persist_thread_by_id(thread_id).await;

    let rehydrated = AgentEngine::new_test(
        SessionManager::new_test(temp_dir.path()).await,
        AgentConfig::default(),
        temp_dir.path(),
    )
    .await;
    rehydrated.hydrate().await.expect("hydrate should succeed");

    let thread = rehydrated
        .get_thread(thread_id)
        .await
        .expect("thread should be restored after hydrate");
    assert_eq!(thread.total_input_tokens, 14);
    assert_eq!(thread.total_output_tokens, 9);
}

#[tokio::test]
async fn hydrate_restores_full_persisted_task_log_history() {
    let (engine, temp_dir) = make_test_engine(AgentConfig::default()).await;
    let task_id = "task-hydrate-full-history";
    let log_count = 250u64;

    engine.tasks.lock().await.push_back(AgentTask {
        id: task_id.to_string(),
        title: "Hydrated Task".to_string(),
        description: "Preserve all task logs across restart".to_string(),
        status: TaskStatus::Completed,
        priority: TaskPriority::Normal,
        progress: 100,
        created_at: 2_000,
        started_at: Some(2_000),
        completed_at: Some(2_500),
        error: None,
        result: Some("done".to_string()),
        thread_id: None,
        source: "user".to_string(),
        notify_on_complete: false,
        notify_channels: Vec::new(),
        dependencies: Vec::new(),
        command: None,
        session_id: None,
        goal_run_id: None,
        goal_run_title: None,
        goal_step_id: None,
        goal_step_title: None,
        parent_task_id: None,
        parent_thread_id: None,
        runtime: "daemon".to_string(),
        retry_count: 0,
        max_retries: 1,
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
        logs: (0..log_count)
            .map(|index| AgentTaskLogEntry {
                id: format!("task-log-{index}"),
                timestamp: 2_000 + index,
                level: TaskLogLevel::Info,
                phase: "test".to_string(),
                message: format!("task-log-{index}"),
                details: None,
                attempt: 0,
            })
            .collect(),
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
    engine.persist_tasks().await;

    let rehydrated = AgentEngine::new_test(
        SessionManager::new_test(temp_dir.path()).await,
        AgentConfig::default(),
        temp_dir.path(),
    )
    .await;
    rehydrated.hydrate().await.expect("hydrate should succeed");

    let task = rehydrated
        .list_tasks()
        .await
        .into_iter()
        .find(|task| task.id == task_id)
        .expect("task should be restored after hydrate");
    assert_eq!(task.logs.len(), log_count as usize);
    assert_eq!(
        task.logs.last().map(|entry| entry.message.as_str()),
        Some("task-log-249")
    );
}

#[tokio::test]
async fn hydrate_restores_full_persisted_goal_run_event_history() {
    let (engine, temp_dir) = make_test_engine(AgentConfig::default()).await;
    let goal_run_id = "goal-hydrate-full-history";
    let event_count = 250u64;

    engine.goal_runs.lock().await.push_back(GoalRun {
        id: goal_run_id.to_string(),
        title: "Hydrated Goal Run".to_string(),
        goal: "Preserve all goal-run events across restart".to_string(),
        client_request_id: None,
        status: GoalRunStatus::Completed,
        priority: TaskPriority::Normal,
        created_at: 3_000,
        updated_at: 3_500,
        started_at: Some(3_000),
        completed_at: Some(3_500),
        thread_id: None,
        session_id: None,
        current_step_index: 0,
        current_step_title: None,
        current_step_kind: None,
        replan_count: 0,
        max_replans: 0,
        plan_summary: None,
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
        duration_ms: Some(500),
        steps: Vec::new(),
        events: (0..event_count)
            .map(|index| GoalRunEvent {
                id: format!("goal-event-{index}"),
                timestamp: 3_000 + index,
                phase: "test".to_string(),
                message: format!("goal-event-{index}"),
                details: None,
                step_index: None,
                todo_snapshot: Vec::new(),
            })
            .collect(),
        total_prompt_tokens: 0,
        total_completion_tokens: 0,
        estimated_cost_usd: None,
        autonomy_level: crate::agent::AutonomyLevel::Supervised,
        authorship_tag: None,
    });
    engine.persist_goal_runs().await;

    let rehydrated = AgentEngine::new_test(
        SessionManager::new_test(temp_dir.path()).await,
        AgentConfig::default(),
        temp_dir.path(),
    )
    .await;
    rehydrated.hydrate().await.expect("hydrate should succeed");

    let goal_run = rehydrated
        .get_goal_run(goal_run_id)
        .await
        .expect("goal run should be restored after hydrate");
    assert_eq!(goal_run.events.len(), event_count as usize);
    assert_eq!(
        goal_run.events.last().map(|event| event.message.as_str()),
        Some("goal-event-249")
    );
}

#[tokio::test]
async fn provider_alternative_excludes_failed_provider_itself() {
    let mut config = AgentConfig::default();
    config.provider = PROVIDER_ID_OPENAI.to_string();
    config.providers.insert(
        PROVIDER_ID_OPENAI.to_string(),
        provider_config(
            "https://api.openai.com/v1",
            "gpt-4o",
            "valid-key",
            AuthSource::ApiKey,
        ),
    );
    let (engine, _temp_dir) = make_test_engine(config).await;

    let suggestion = engine
        .suggest_alternative_provider(PROVIDER_ID_OPENAI)
        .await;

    assert!(
        suggestion.is_none(),
        "the failed provider itself must not be suggested as an alternative"
    );
}

#[tokio::test]
async fn provider_alternative_excludes_open_breaker_provider() {
    let mut config = AgentConfig::default();
    config.provider = PROVIDER_ID_OPENAI.to_string();
    config.providers.insert(
        PROVIDER_ID_CUSTOM.to_string(),
        provider_config(
            "https://example.invalid/v1",
            "model-a",
            "valid-key",
            AuthSource::ApiKey,
        ),
    );
    let (engine, _temp_dir) = make_test_engine(config).await;
    {
        let breaker = engine.circuit_breakers.get(PROVIDER_ID_CUSTOM).await;
        let mut breaker = breaker.lock().await;
        let now = now_millis();
        for offset in 0..5 {
            breaker.record_failure(now + offset);
        }
    }

    let suggestion = engine
        .suggest_alternative_provider(PROVIDER_ID_OPENAI)
        .await;

    assert!(
        suggestion.is_none(),
        "providers with open circuit breakers must not be suggested"
    );
}

#[tokio::test]
async fn provider_alternative_includes_configured_healthy_provider() {
    let mut config = AgentConfig::default();
    config.provider = PROVIDER_ID_OPENAI.to_string();
    config.providers.insert(
        PROVIDER_ID_CUSTOM.to_string(),
        provider_config(
            "https://example.invalid/v1",
            "model-a",
            "valid-key",
            AuthSource::ApiKey,
        ),
    );
    let (engine, _temp_dir) = make_test_engine(config).await;

    let suggestion = engine
        .suggest_alternative_provider(PROVIDER_ID_OPENAI)
        .await;

    let suggestion = suggestion.expect("healthy provider should be suggested");
    assert!(
        suggestion.contains(PROVIDER_ID_CUSTOM),
        "expected healthy configured provider to be suggested, got: {suggestion}"
    );
}

#[tokio::test]
async fn provider_alternative_excludes_openai_subscription_without_auth() {
    let _env_guard = crate::agent::provider_auth_store::provider_auth_test_env_lock();
    let temp_dir = TempDir::new().expect("temp dir");
    let db_path = temp_dir.path().join("provider-auth.db");
    std::env::set_var("TAMUX_PROVIDER_AUTH_DB_PATH", &db_path);
    std::env::set_var(
        "TAMUX_CODEX_CLI_AUTH_PATH",
        temp_dir.path().join("missing-codex-auth.json"),
    );

    let mut config = AgentConfig::default();
    config.provider = PROVIDER_ID_GROQ.to_string();
    config.providers.insert(
        PROVIDER_ID_OPENAI.to_string(),
        provider_config(
            "https://api.openai.com/v1",
            "gpt-5.4",
            "",
            AuthSource::ChatgptSubscription,
        ),
    );
    let (engine, _temp_dir) = make_test_engine(config).await;

    let suggestion = engine.suggest_alternative_provider(PROVIDER_ID_GROQ).await;

    std::env::remove_var("TAMUX_PROVIDER_AUTH_DB_PATH");
    std::env::remove_var("TAMUX_CODEX_CLI_AUTH_PATH");
    assert!(
        suggestion.is_none(),
        "OpenAI subscription auth must be present before suggesting it as an alternative"
    );
}

#[tokio::test]
async fn provider_alternative_uses_candidate_default_model_for_empty_named_model() {
    let _env_guard = crate::agent::provider_auth_store::provider_auth_test_env_lock();
    let temp_dir = TempDir::new().expect("temp dir");
    let db_path = temp_dir.path().join("provider-auth.db");
    std::env::set_var("TAMUX_PROVIDER_AUTH_DB_PATH", &db_path);
    write_openai_subscription_auth();
    std::env::set_var(
        "TAMUX_CODEX_CLI_AUTH_PATH",
        temp_dir.path().join("missing-codex-auth.json"),
    );

    let mut config = AgentConfig::default();
    config.provider = PROVIDER_ID_OPENAI.to_string();
    config.model = "gpt-5.4".to_string();
    config.providers.insert(
        PROVIDER_ID_GROQ.to_string(),
        provider_config("", "", "groq-key", AuthSource::ApiKey),
    );
    let (engine, _temp_dir) = make_test_engine(config).await;

    let resolved = {
        let config = engine.config.read().await;
        resolve_candidate_provider_config(&config, PROVIDER_ID_GROQ)
            .expect("candidate provider should resolve with its default model")
    };
    let suggestion = engine
        .suggest_alternative_provider(PROVIDER_ID_OPENAI)
        .await;

    std::env::remove_var("TAMUX_PROVIDER_AUTH_DB_PATH");
    std::env::remove_var("TAMUX_CODEX_CLI_AUTH_PATH");
    assert_eq!(resolved.model, "llama-3.3-70b-versatile");
    assert!(
        suggestion
            .as_deref()
            .unwrap_or_default()
            .contains(PROVIDER_ID_GROQ),
        "expected groq to remain eligible using its own default model"
    );
}

async fn spawn_hung_http_server() -> String {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind hung http server");
    let addr = listener.local_addr().expect("hung server local addr");
    tokio::spawn(async move {
        loop {
            let Ok((mut socket, _)) = listener.accept().await else {
                break;
            };
            tokio::spawn(async move {
                let mut buffer = [0u8; 1024];
                let _ = socket.read(&mut buffer).await;
                tokio::time::sleep(Duration::from_secs(15)).await;
            });
        }
    });
    format!("http://{addr}/v1")
}

#[tokio::test]
async fn send_message_times_out_hung_provider_request() {
    let server_url = spawn_hung_http_server().await;
    let temp_dir = TempDir::new().expect("temp dir");
    let session_manager = SessionManager::new_test(temp_dir.path()).await;
    let history = HistoryStore::new_test_store(temp_dir.path())
        .await
        .expect("history store");
    let data_dir = temp_dir.path().join("agent");
    std::fs::create_dir_all(&data_dir).expect("create agent data dir");

    let mut config = AgentConfig::default();
    config.provider = PROVIDER_ID_OPENAI.to_string();
    config.base_url = server_url;
    config.model = "gpt-4o-mini".to_string();
    config.api_transport = ApiTransport::ChatCompletions;
    config.max_retries = 0;
    config.auto_retry = false;

    let engine = AgentEngine::new_with_storage_and_http_client(
        session_manager,
        config,
        history,
        data_dir,
        build_agent_http_client(Duration::from_millis(75)),
    );

    let result = tokio::time::timeout(
        Duration::from_secs(4),
        engine.send_message_inner(
            None,
            "What model are you?",
            None,
            None,
            None,
            None,
            None,
            None,
            true,
        ),
    )
    .await
    .expect("hung provider request should time out at the HTTP layer, not the test harness");

    let error = match result {
        Ok(_) => panic!("hung provider should surface as an error"),
        Err(error) => error,
    };
    let error_text = error.to_string().to_lowercase();
    assert!(
        error_text.contains("timed out"),
        "expected timeout error, got: {error}"
    );
}

#[tokio::test]
async fn ask_operator_question_waits_for_selected_answer_and_broadcasts_events() {
    let (engine, _temp_dir) = make_test_engine(AgentConfig::default()).await;
    let mut events = engine.event_tx.subscribe();

    let engine_for_task = engine.clone();
    let pending = tokio::spawn(async move {
        engine_for_task
            .ask_operator_question(
                "Pick one:\nA. Alpha\nB. Beta",
                vec!["A".to_string(), "B".to_string()],
                Some("session-1".to_string()),
                Some("thread-1".to_string()),
            )
            .await
    });

    let question_id = match events.recv().await.expect("question event") {
        AgentEvent::OperatorQuestion {
            question_id,
            content,
            options,
            session_id,
            thread_id,
        } => {
            assert_eq!(content, "Pick one:\nA. Alpha\nB. Beta");
            assert_eq!(options, vec!["A".to_string(), "B".to_string()]);
            assert_eq!(session_id.as_deref(), Some("session-1"));
            assert_eq!(thread_id.as_deref(), Some("thread-1"));
            question_id
        }
        other => panic!("expected operator question event, got {other:?}"),
    };

    engine
        .answer_operator_question(&question_id, "B")
        .await
        .expect("answer should resolve pending question");

    match events.recv().await.expect("resolved event") {
        AgentEvent::OperatorQuestionResolved {
            question_id: resolved_id,
            answer,
            thread_id,
        } => {
            assert_eq!(resolved_id, question_id);
            assert_eq!(answer, "B");
            assert_eq!(thread_id.as_deref(), Some("thread-1"));
        }
        other => panic!("expected operator question resolved event, got {other:?}"),
    }

    let (returned_question_id, selected_answer) = pending
        .await
        .expect("question task should finish")
        .expect("question should resolve successfully");
    assert_eq!(returned_question_id, question_id);
    assert_eq!(selected_answer, "B");
}

#[tokio::test]
async fn ask_operator_question_rejects_non_compact_button_tokens() {
    let (engine, _temp_dir) = make_test_engine(AgentConfig::default()).await;

    let error = engine
        .ask_operator_question(
            "Pick one:\nA. Alpha\nB. Beta",
            vec!["Alpha".to_string(), "B".to_string()],
            None,
            None,
        )
        .await
        .expect_err("non-compact option labels must be rejected");

    assert!(
        error.to_string().contains("compact ordered tokens"),
        "expected compact-token validation error, got: {error}"
    );
}

#[tokio::test]
async fn task_and_file_tool_paths_populate_cross_thread_memory_graph() {
    let (engine, temp_dir) = make_test_engine(AgentConfig::default()).await;

    std::process::Command::new("git")
        .args(["init"])
        .current_dir(temp_dir.path())
        .output()
        .expect("git init should succeed");

    std::fs::create_dir_all(temp_dir.path().join("src")).expect("create src dir");
    std::fs::write(
        temp_dir.path().join("Cargo.toml"),
        "[package]\nname = \"demo\"\nversion = \"0.1.0\"\n",
    )
    .expect("write cargo manifest");
    std::fs::write(temp_dir.path().join("src/lib.rs"), "pub fn demo() {}\n").expect("write lib.rs");

    let task = engine
        .enqueue_task(
            "Inspect src/lib.rs".to_string(),
            "Check src/lib.rs and Cargo.toml".to_string(),
            "normal",
            None,
            None,
            Vec::new(),
            None,
            "user",
            None,
            None,
            None,
            None,
        )
        .await;

    let refs = engine
        .enrich_thread_structural_memory_from_tool_result(
            "thread-memory-graph",
            "read_file",
            &serde_json::json!({
                "path": temp_dir.path().join("src/lib.rs").to_string_lossy().to_string(),
            })
            .to_string(),
            Some("pub fn demo() {}\n"),
        )
        .await;

    assert!(
        refs.iter().any(|node_id| node_id == "node:file:src/lib.rs"),
        "structural memory should still return the observed file ref"
    );

    let task_node = engine
        .history
        .get_memory_node(&format!("node:task:{}", task.id))
        .await
        .expect("load task node")
        .expect("task node should be persisted");
    assert_eq!(task_node.node_type, "task");

    let file_node = engine
        .history
        .get_memory_node("node:file:src/lib.rs")
        .await
        .expect("load file node")
        .expect("file node should be persisted");
    assert_eq!(file_node.node_type, "file");

    let package_node = engine
        .history
        .get_memory_node("node:package:cargo:demo")
        .await
        .expect("load package node")
        .expect("package node should be persisted");
    assert_eq!(package_node.node_type, "package");

    let task_edges = engine
        .history
        .list_memory_edges_for_node(&format!("node:task:{}", task.id))
        .await
        .expect("load task edges");
    assert!(task_edges.iter().any(|edge| {
        edge.relation_type == "task_touches_file" && edge.target_node_id == "node:file:src/lib.rs"
    }));

    let file_edges = engine
        .history
        .list_memory_edges_for_node("node:file:src/lib.rs")
        .await
        .expect("load file edges");
    assert!(file_edges.iter().any(|edge| {
        edge.relation_type == "file_in_package" && edge.target_node_id == "node:package:cargo:demo"
    }));
}
