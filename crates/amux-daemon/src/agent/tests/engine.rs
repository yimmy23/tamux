#[cfg(test)]
use super::*;
use amux_shared::providers::{
    PROVIDER_ID_CHATGPT_SUBSCRIPTION, PROVIDER_ID_CUSTOM, PROVIDER_ID_GROQ, PROVIDER_ID_OPENAI,
};
use tempfile::TempDir;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
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
async fn hydrate_keeps_persisted_thread_messages_lazy_until_thread_detail_is_requested() {
    let (engine, temp_dir) = make_test_engine(AgentConfig::default()).await;
    let thread_id = "thread-hydrate-lazy-history";
    let message_count = 12u64;

    engine.threads.write().await.insert(
        thread_id.to_string(),
        AgentThread {
            id: thread_id.to_string(),
            agent_name: Some(crate::agent::agent_identity::MAIN_AGENT_NAME.to_string()),
            title: "Lazy Hydrated Thread".to_string(),
            messages: (0..message_count)
                .map(|index| AgentMessage::user(format!("message-{index}"), 2_000 + index))
                .collect(),
            pinned: false,
            upstream_thread_id: None,
            upstream_transport: None,
            upstream_provider: None,
            upstream_model: None,
            upstream_assistant_id: None,
            created_at: 2_000,
            updated_at: 2_000 + message_count,
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

    let in_memory = rehydrated.threads.read().await;
    let thread = in_memory
        .get(thread_id)
        .expect("thread summary should exist after hydrate");
    assert!(
        thread.messages.is_empty(),
        "startup hydrate should avoid loading full message bodies into memory"
    );
    drop(in_memory);

    let loaded = rehydrated
        .get_thread(thread_id)
        .await
        .expect("thread detail should lazy-load persisted history");
    assert_eq!(loaded.messages.len(), message_count as usize);
    assert_eq!(
        loaded
            .messages
            .last()
            .map(|message| message.content.as_str()),
        Some("message-11")
    );
}

#[tokio::test]
async fn hydrate_restores_user_defined_subagent_thread_identity() {
    let mut config = AgentConfig::default();
    config.sub_agents.push(SubAgentDefinition {
        id: "dola".to_string(),
        name: "Dola".to_string(),
        provider: PROVIDER_ID_OPENAI.to_string(),
        model: "gpt-5.4-mini".to_string(),
        role: Some("review specialist".to_string()),
        system_prompt: Some("Review code carefully.".to_string()),
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
        reasoning_effort: Some("medium".to_string()),
        created_at: 1,
    });
    let (engine, temp_dir) = make_test_engine(config.clone()).await;
    let thread_id = "thread-hydrate-dola";

    let (created_thread_id, _created) = engine
        .get_or_create_thread_with_target(Some(thread_id), "Review this", Some("dola"))
        .await;
    assert_eq!(created_thread_id, thread_id);
    engine.persist_thread_by_id(thread_id).await;

    let rehydrated = AgentEngine::new_test(
        SessionManager::new_test(temp_dir.path()).await,
        config,
        temp_dir.path(),
    )
    .await;
    rehydrated.hydrate().await.expect("hydrate should succeed");

    let thread = rehydrated
        .get_thread(thread_id)
        .await
        .expect("thread should be restored after hydrate");
    assert_eq!(thread.agent_name.as_deref(), Some("Dola"));

    let listed = rehydrated.list_threads().await;
    assert!(
        listed
            .iter()
            .any(|thread| thread.id == thread_id && thread.agent_name.as_deref() == Some("Dola")),
        "thread list should preserve user-defined subagent identity after hydrate"
    );
}

#[tokio::test]
async fn persist_thread_by_id_preserves_lazy_hydrated_thread_history() {
    let (engine, temp_dir) = make_test_engine(AgentConfig::default()).await;
    let thread_id = "thread-lazy-persist-safe";

    engine.threads.write().await.insert(
        thread_id.to_string(),
        AgentThread {
            id: thread_id.to_string(),
            agent_name: Some(crate::agent::agent_identity::MAIN_AGENT_NAME.to_string()),
            title: "Persisted Lazy Thread".to_string(),
            messages: (0..8)
                .map(|index| AgentMessage::user(format!("message-{index}"), 3_000 + index))
                .collect(),
            pinned: false,
            upstream_thread_id: None,
            upstream_transport: None,
            upstream_provider: None,
            upstream_model: None,
            upstream_assistant_id: None,
            created_at: 3_000,
            updated_at: 3_008,
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
    rehydrated.persist_thread_by_id(thread_id).await;

    let reloaded = AgentEngine::new_test(
        SessionManager::new_test(temp_dir.path()).await,
        AgentConfig::default(),
        temp_dir.path(),
    )
    .await;
    reloaded
        .hydrate()
        .await
        .expect("second hydrate should succeed");

    let thread = reloaded
        .get_thread(thread_id)
        .await
        .expect("thread should still have its persisted history");
    assert_eq!(thread.messages.len(), 8);
    assert_eq!(
        thread
            .messages
            .last()
            .map(|message| message.content.as_str()),
        Some("message-7")
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
        content_blocks: Vec::new(),
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
        tool_output_preview_path: None,
        structural_refs: Vec::new(),
        pinned_for_compaction: false,
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
        planner_owner_profile: None,
        current_step_owner_profile: None,
        replan_count: 0,
        max_replans: 0,
        plan_summary: None,
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
        dossier: None,
        total_prompt_tokens: 0,
        total_completion_tokens: 0,
        estimated_cost_usd: None,
        model_usage: Vec::new(),
        autonomy_level: crate::agent::AutonomyLevel::Supervised,
        authorship_tag: None,
        launch_assignment_snapshot: Vec::new(),
        runtime_assignment_list: Vec::new(),
        root_thread_id: None,
        active_thread_id: None,
        execution_thread_ids: Vec::new(),
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
    let _saved_env = crate::test_support::EnvGuard::new(&[
        "TAMUX_PROVIDER_AUTH_DB_PATH",
        "TAMUX_CODEX_CLI_AUTH_PATH",
    ]);
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
    assert!(
        suggestion.is_none(),
        "OpenAI subscription auth must be present before suggesting it as an alternative"
    );
}

#[tokio::test]
async fn provider_alternative_uses_candidate_default_model_for_empty_named_model() {
    let _env_guard = crate::agent::provider_auth_store::provider_auth_test_env_lock();
    let _saved_env = crate::test_support::EnvGuard::new(&[
        "TAMUX_PROVIDER_AUTH_DB_PATH",
        "TAMUX_CODEX_CLI_AUTH_PATH",
    ]);
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

async fn read_http_request_body(socket: &mut tokio::net::TcpStream) -> std::io::Result<String> {
    let mut buffer = Vec::with_capacity(65536);
    let mut temp = [0u8; 4096];
    let headers_end = loop {
        let read = socket.read(&mut temp).await?;
        if read == 0 {
            return Ok(String::new());
        }
        buffer.extend_from_slice(&temp[..read]);
        if let Some(index) = buffer.windows(4).position(|window| window == b"\r\n\r\n") {
            break index + 4;
        }
    };

    let headers = String::from_utf8_lossy(&buffer[..headers_end]);
    let content_length = headers
        .lines()
        .find_map(|line| {
            let mut parts = line.splitn(2, ':');
            let name = parts.next()?.trim();
            let value = parts.next()?.trim();
            name.eq_ignore_ascii_case("content-length")
                .then(|| value.parse::<usize>().ok())
                .flatten()
        })
        .unwrap_or(0);

    while buffer.len().saturating_sub(headers_end) < content_length {
        let read = socket.read(&mut temp).await?;
        if read == 0 {
            break;
        }
        buffer.extend_from_slice(&temp[..read]);
    }

    let available = buffer.len().saturating_sub(headers_end).min(content_length);
    Ok(String::from_utf8_lossy(&buffer[headers_end..headers_end + available]).to_string())
}

#[tokio::test]
async fn force_compact_reuses_task_provider_override_for_builtin_persona_threads() {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind compaction replay server");
    let addr = listener
        .local_addr()
        .expect("compaction replay server addr");

    tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.expect("accept replay request");
        let _ = read_http_request_body(&mut socket)
            .await
            .expect("read replay request");
        let response_body = concat!(
            "data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_compact_task_builtin\"}}\n\n",
            "data: {\"type\":\"response.output_text.delta\",\"delta\":\"Compaction replay ok\"}\n\n",
            "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_compact_task_builtin\",\"object\":\"response\",\"status\":\"completed\",\"output\":[],\"usage\":{\"input_tokens\":6,\"output_tokens\":3},\"error\":null}}\n\n"
        );
        let response = format!(
            "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
            response_body.len(),
            response_body
        );
        socket
            .write_all(response.as_bytes())
            .await
            .expect("write replay response");
    });

    let mut config = AgentConfig::default();
    config.provider = PROVIDER_ID_OPENAI.to_string();
    config.base_url = format!("http://{addr}/v1");
    config.model = "gpt-5.4-mini".to_string();
    config.api_key = "test-key".to_string();
    config.auth_source = AuthSource::ApiKey;
    config.api_transport = ApiTransport::Responses;
    config.auto_retry = false;
    config.max_retries = 0;
    config.max_tool_loops = 1;
    let (engine, _temp_dir) = make_test_engine(config).await;
    let thread_id = "thread-force-compact-task-builtin";

    let (created_thread_id, _created) = engine
        .get_or_create_thread_with_target(Some(thread_id), "Continue this task", Some("dazhbog"))
        .await;
    assert_eq!(created_thread_id, thread_id);

    {
        let mut threads = engine.threads.write().await;
        let thread = threads
            .get_mut(thread_id)
            .expect("task-owned builtin thread should exist");
        thread
            .messages
            .push(AgentMessage::user("Continue this task", 1));
        thread.updated_at = 1;
    }

    engine
        .tasks
        .lock()
        .await
        .push_back(crate::agent::types::AgentTask {
            id: "task-force-compact-dazhbog".to_string(),
            title: "Task-owned builtin thread".to_string(),
            description:
                "Regression coverage for manual compaction on builtin persona task threads."
                    .to_string(),
            status: crate::agent::types::TaskStatus::Queued,
            priority: crate::agent::types::TaskPriority::Normal,
            progress: 0,
            created_at: 1,
            started_at: None,
            completed_at: None,
            error: None,
            result: None,
            thread_id: Some(thread_id.to_string()),
            source: "subagent".to_string(),
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
            override_provider: Some(PROVIDER_ID_OPENAI.to_string()),
            override_model: Some("gpt-5.4-mini".to_string()),
            override_system_prompt: Some(
                "Agent persona: Dazhbog\nAgent persona id: dazhbog\nTask-owned builtin persona."
                    .to_string(),
            ),
            sub_agent_def_id: Some("dazhbog".to_string()),
        });

    let compacted = engine
        .force_compact_and_continue(thread_id)
        .await
        .expect("manual compaction should reuse task override provider");
    assert!(compacted);

    let thread = engine
        .get_thread(thread_id)
        .await
        .expect("thread should remain available after manual compaction");
    assert!(
        thread
            .messages
            .iter()
            .any(|message| message.role == MessageRole::Assistant
                && message.content.contains("Compaction replay ok")),
        "manual compaction should continue the task-owned builtin thread via the task override"
    );
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

#[tokio::test]
async fn due_task_routine_materializes_enqueued_task_and_advances_next_run_at() {
    let (engine, _temp_dir) = make_test_engine(AgentConfig::default()).await;
    let due_at = now_millis().saturating_sub(1_000);

    engine
        .history
        .upsert_routine_definition(&crate::history::RoutineDefinitionRow {
            id: "routine-daily-brief-materialize".to_string(),
            title: "Daily brief".to_string(),
            description: "Send a daily project brief".to_string(),
            enabled: true,
            paused_at: None,
            schedule_expression: "* * * * *".to_string(),
            target_kind: "task".to_string(),
            target_payload_json: serde_json::json!({
                "title": "Prepare daily brief",
                "description": "Prepare the daily project brief",
                "priority": "high"
            })
            .to_string(),
            next_run_at: Some(due_at),
            last_run_at: None,
            created_at: due_at.saturating_sub(5_000),
            updated_at: due_at,
        })
        .await
        .expect("store routine definition");

    let materialized_after = now_millis();
    let created = engine
        .materialize_due_routines()
        .await
        .expect("due routine should materialize into a task");
    let materialized_before = now_millis();

    assert_eq!(created.len(), 1, "exactly one due routine should materialize");
    assert_eq!(created[0].title, "Prepare daily brief");
    assert_eq!(created[0].description, "Prepare the daily project brief");
    assert_eq!(created[0].priority, TaskPriority::High);
    assert_eq!(created[0].source, "routine:routine-daily-brief-materialize");
    assert_eq!(created[0].status, TaskStatus::Queued);

    let tasks = engine.list_tasks().await;
    let task = tasks
        .iter()
        .find(|task| task.source == "routine:routine-daily-brief-materialize")
        .expect("materialized task should be queued");
    assert_eq!(task.title, "Prepare daily brief");
    assert_eq!(task.priority, TaskPriority::High);

    let updated = engine
        .history
        .get_routine_definition("routine-daily-brief-materialize")
        .await
        .expect("load updated routine")
        .expect("updated routine should still exist");
    let last_run_at = updated.last_run_at.expect("last_run_at should be recorded");
    let next_run_at = updated.next_run_at.expect("next_run_at should advance");
    assert!(last_run_at >= materialized_after);
    assert!(last_run_at <= materialized_before);
    assert!(next_run_at > last_run_at);
}

#[tokio::test]
async fn paused_due_routine_does_not_materialize_until_resumed() {
    let (engine, _temp_dir) = make_test_engine(AgentConfig::default()).await;
    let due_at = now_millis().saturating_sub(1_000);

    engine
        .history
        .upsert_routine_definition(&crate::history::RoutineDefinitionRow {
            id: "routine-paused-materialize".to_string(),
            title: "Paused brief".to_string(),
            description: "Paused routine should not execute".to_string(),
            enabled: true,
            paused_at: None,
            schedule_expression: "* * * * *".to_string(),
            target_kind: "task".to_string(),
            target_payload_json: serde_json::json!({
                "title": "Prepare paused brief",
                "description": "This should wait until resume",
                "priority": "normal"
            })
            .to_string(),
            next_run_at: Some(due_at),
            last_run_at: None,
            created_at: due_at.saturating_sub(5_000),
            updated_at: due_at,
        })
        .await
        .expect("store paused routine definition");

    let paused = engine
        .pause_routine_json("routine-paused-materialize")
        .await
        .expect("pause should succeed");
    assert!(paused["routine"]["paused_at"].as_u64().is_some());

    let paused_created = engine
        .materialize_due_routines()
        .await
        .expect("paused routine check should succeed");
    assert!(paused_created.is_empty(), "paused due routine should not materialize");
    assert!(
        engine.list_tasks().await.is_empty(),
        "no task should be enqueued while paused"
    );

    let resumed = engine
        .resume_routine_json("routine-paused-materialize")
        .await
        .expect("resume should succeed");
    assert!(resumed["routine"]["paused_at"].is_null());

    let resumed_created = engine
        .materialize_due_routines()
        .await
        .expect("resumed routine should materialize");
    assert_eq!(resumed_created.len(), 1);
    assert_eq!(resumed_created[0].source, "routine:routine-paused-materialize");
}
