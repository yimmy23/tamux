use super::*;

#[tokio::test]
async fn execute_tool_routes_weles_runtime_review_over_internal_dm_thread() {
    let recorded_bodies = Arc::new(Mutex::new(std::collections::VecDeque::new()));
    let root = tempdir().expect("tempdir should succeed");
    let manager = SessionManager::new_test(root.path()).await;
    let (event_tx, _) = broadcast::channel(8);
    let mut config = AgentConfig::default();
    config.provider = "openai".to_string();
    config.base_url = spawn_stub_assistant_server_for_tool_executor(
        recorded_bodies,
        serde_json::json!({
            "verdict": "block",
            "reasons": ["reviewed over internal dm"],
            "audit_id": "audit-internal-dm"
        })
        .to_string(),
    )
    .await;
    config.model = "gpt-4o-mini".to_string();
    config.api_key = "test-key".to_string();
    config.api_transport = crate::agent::types::ApiTransport::ChatCompletions;
    config.auto_retry = false;
    config.max_retries = 0;
    config.max_tool_loops = 1;

    let engine = AgentEngine::new_test(manager.clone(), config, root.path()).await;
    let thread_id = "thread-weles-runtime-review";
    {
        let mut threads = engine.threads.write().await;
        threads.insert(
            thread_id.to_string(),
            crate::agent::types::AgentThread {
                id: thread_id.to_string(),
                agent_name: None,
                title: "Operator thread".to_string(),
                messages: vec![crate::agent::types::AgentMessage::user("Inspect tool", 1)],
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

    let tool_call = ToolCall::with_default_weles_review(
        "tool-weles-internal-dm".to_string(),
        ToolFunction {
            name: "bash_command".to_string(),
            arguments: serde_json::json!({
                "command": "curl -s https://example.com",
                "timeout_seconds": 5
            })
            .to_string(),
        },
    );

    let result = execute_tool(
        &tool_call,
        &engine,
        thread_id,
        None,
        &manager,
        None,
        &event_tx,
        root.path(),
        &engine.http_client,
        None,
    )
    .await;

    assert!(result.is_error, "reviewed command should stay blocked");
    let review = result
        .weles_review
        .expect("guarded result should include WELES review metadata");
    assert!(review.weles_reviewed);
    assert_eq!(review.verdict, crate::agent::types::WelesVerdict::Block);
    assert!(review
        .reasons
        .iter()
        .any(|reason| reason.contains("reviewed over internal dm")));

    let dm_thread_id = crate::agent::agent_identity::internal_dm_thread_id(
        crate::agent::agent_identity::MAIN_AGENT_ID,
        crate::agent::agent_identity::WELES_AGENT_ID,
    );
    let threads = engine.threads.read().await;
    let dm_thread = threads
        .get(&dm_thread_id)
        .expect("WELES review should create an internal DM thread");
    assert_eq!(
        dm_thread.title,
        crate::agent::agent_identity::internal_dm_thread_title(
            crate::agent::agent_identity::MAIN_AGENT_ID,
            crate::agent::agent_identity::WELES_AGENT_ID,
        )
    );
    assert!(
        dm_thread.messages.iter().any(|message| {
            message.role == crate::agent::types::MessageRole::Assistant
                && message.content.contains("audit-internal-dm")
        }),
        "WELES assistant response should be stored on the internal DM thread"
    );
    assert_eq!(
        threads
            .get(thread_id)
            .expect("operator thread should remain present")
            .messages
            .len(),
        1,
        "operator thread should not receive the internal WELES exchange"
    );
}

#[tokio::test]
async fn execute_tool_weles_scope_blocks_shell_python_without_recursive_spawn_from_message_loop() {
    let recorded_bodies = Arc::new(Mutex::new(std::collections::VecDeque::new()));
    let root = tempdir().expect("tempdir should succeed");
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.provider = "openai".to_string();
    config.base_url = spawn_stub_assistant_server_for_tool_executor(
        recorded_bodies,
        serde_json::json!({
            "verdict": "allow",
            "reasons": ["should never be used for recursive review"],
            "audit_id": "audit-should-not-recurse"
        })
        .to_string(),
    )
    .await;
    config.model = "gpt-4o-mini".to_string();
    config.api_key = "test-key".to_string();
    config.api_transport = crate::agent::types::ApiTransport::ChatCompletions;
    config.auto_retry = false;
    config.max_retries = 0;
    config.max_tool_loops = 1;

    let engine = AgentEngine::new_test(manager.clone(), config, root.path()).await;
    let thread_id = "thread-weles-loop";
    {
        let mut threads = engine.threads.write().await;
        threads.insert(
            thread_id.to_string(),
            crate::agent::types::AgentThread {
                id: thread_id.to_string(),
                agent_name: None,
                title: "WELES loop thread".to_string(),
                messages: vec![crate::agent::types::AgentMessage::user("Inspect tool", 1)],
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

    let task = super::spawn_weles_internal_subagent(
            &engine,
            thread_id,
            None,
            "governance",
            "bash_command",
            &serde_json::json!({"command": "git clone https://github.com/mkurman/claude-scientific-skills.git ~/skills/claude-scientific-skills 2>&1"}),
            SecurityLevel::Highest,
            &["shell command requests network access".to_string()],
        )
        .await
        .expect("daemon-owned WELES governance spawn should succeed");
    let before_tasks = engine.list_tasks().await.len();

    let outcome = engine
        .send_message_inner(
            Some(thread_id),
            "Run the governance review",
            Some(&task.id),
            None,
            None,
            None,
            None,
            None,
            true,
        )
        .await
        .expect("WELES runtime send should complete without recursion crash");

    assert_eq!(outcome.thread_id, thread_id);
    assert_eq!(engine.list_tasks().await.len(), before_tasks);
}

#[tokio::test]
async fn execute_dispatched_weles_task_uses_internal_dm_thread() {
    let recorded_bodies = Arc::new(Mutex::new(std::collections::VecDeque::new()));
    let root = tempdir().expect("tempdir should succeed");
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.provider = "openai".to_string();
    config.base_url =
        spawn_stub_assistant_server_for_tool_executor(recorded_bodies, "Acknowledged.".to_string())
            .await;
    config.model = "gpt-4o-mini".to_string();
    config.api_key = "test-key".to_string();
    config.api_transport = crate::agent::types::ApiTransport::ChatCompletions;
    config.auto_retry = false;
    config.max_retries = 0;
    config.max_tool_loops = 1;

    let engine = AgentEngine::new_test(manager.clone(), config, root.path()).await;
    let thread_id = "thread-weles-dispatch";
    {
        let mut threads = engine.threads.write().await;
        threads.insert(
            thread_id.to_string(),
            crate::agent::types::AgentThread {
                id: thread_id.to_string(),
                agent_name: None,
                title: "Operator thread".to_string(),
                messages: vec![crate::agent::types::AgentMessage::user("Inspect tool", 1)],
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

    let task = super::spawn_weles_internal_subagent(
        &engine,
        thread_id,
        None,
        "governance",
        "bash_command",
        &serde_json::json!({"command": "curl -s https://example.com"}),
        SecurityLevel::Moderate,
        &["network access".to_string()],
    )
    .await
    .expect("daemon-owned WELES governance spawn should succeed");

    engine
        .clone()
        .dispatch_ready_tasks()
        .await
        .expect("scheduler should dispatch the queued WELES task");

    timeout(Duration::from_secs(5), async {
        loop {
            let tasks = engine.list_tasks().await;
            let stored = tasks
                .iter()
                .find(|entry| entry.id == task.id)
                .expect("task should remain persisted");
            if stored.status == crate::agent::types::TaskStatus::Completed {
                break;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    })
    .await
    .expect("WELES dispatched task should complete");

    let dm_thread_id = crate::agent::agent_identity::internal_dm_thread_id(
        crate::agent::agent_identity::MAIN_AGENT_ID,
        crate::agent::agent_identity::WELES_AGENT_ID,
    );
    let threads = engine.threads.read().await;
    let dm_thread = threads
        .get(&dm_thread_id)
        .expect("dispatcher should route WELES task onto internal DM");
    assert!(
        dm_thread.messages.iter().any(|message| {
            message.role == crate::agent::types::MessageRole::Assistant
                && message.content.contains("Acknowledged.")
        }),
        "internal DM thread should contain the WELES reply"
    );
    assert_eq!(
        threads
            .get(thread_id)
            .expect("operator thread should remain present")
            .messages
            .len(),
        1,
        "operator thread should not receive the dispatched WELES exchange"
    );
    drop(threads);

    let tasks = engine.list_tasks().await;
    let stored = tasks
        .iter()
        .find(|entry| entry.id == task.id)
        .expect("task should remain persisted");
    assert_eq!(stored.thread_id.as_deref(), Some(dm_thread_id.as_str()));
    assert_eq!(stored.status, crate::agent::types::TaskStatus::Completed);
}
