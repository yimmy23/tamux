use super::*;

#[tokio::test]
async fn execute_tool_guarded_call_uses_weles_runtime_structured_block_verdict() {
    let recorded_bodies = Arc::new(Mutex::new(std::collections::VecDeque::new()));
    let root = tempdir().expect("tempdir should succeed");
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.provider = "openai".to_string();
    config.base_url = spawn_stub_assistant_server_for_tool_executor(
        recorded_bodies.clone(),
        serde_json::json!({
            "verdict": "block",
            "reasons": ["runtime policy denied browser reconfiguration"],
            "audit_id": "audit-weles-runtime-block"
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
    let (event_tx, _) = broadcast::channel(8);
    let tool_call = ToolCall::with_default_weles_review(
        "tool-runtime-block".to_string(),
        ToolFunction {
            name: "setup_web_browsing".to_string(),
            arguments: serde_json::json!({
                "action": "configure",
                "provider": "none"
            })
            .to_string(),
        },
    );

    let result = execute_tool(
        &tool_call,
        &engine,
        "thread-runtime-block",
        None,
        &manager,
        None,
        &event_tx,
        root.path(),
        &engine.http_client,
        None,
    )
    .await;

    assert!(result.is_error);
    assert!(result
        .content
        .contains("runtime policy denied browser reconfiguration"));
    let review = result
        .weles_review
        .expect("runtime block result should carry governance metadata");
    assert!(review.weles_reviewed);
    assert_eq!(review.verdict, crate::agent::types::WelesVerdict::Block);
    assert_eq!(
        review.audit_id.as_deref(),
        Some("audit-weles-runtime-block")
    );
    assert!(review
        .reasons
        .iter()
        .any(|reason| reason.contains("runtime policy denied browser reconfiguration")));

    let recorded = recorded_bodies
        .lock()
        .expect("lock recorded assistant bodies");
    let request = recorded
        .iter()
        .find(|body: &&String| body.contains("## WELES Governance Core"))
        .expect("guarded execution should invoke WELES runtime");
    assert!(request.contains("tool_name: setup_web_browsing"));
    assert!(request.contains("security_level: moderate"));

    let config = engine.config.read().await;
    assert_eq!(
        config
            .extra
            .get("browse_provider")
            .and_then(|value| value.as_str()),
        None
    );
}

#[tokio::test]
async fn execute_tool_guarded_call_uses_weles_runtime_structured_allow_metadata() {
    let recorded_bodies = Arc::new(Mutex::new(std::collections::VecDeque::new()));
    let root = tempdir().expect("tempdir should succeed");
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.provider = "openai".to_string();
    config.base_url = spawn_stub_assistant_server_for_tool_executor(
        recorded_bodies.clone(),
        serde_json::json!({
            "verdict": "allow",
            "reasons": ["runtime review approved controlled browser reconfiguration"],
            "audit_id": "audit-weles-runtime-allow"
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
    let (event_tx, _) = broadcast::channel(8);
    let tool_call = ToolCall::with_default_weles_review(
        "tool-runtime-allow".to_string(),
        ToolFunction {
            name: "setup_web_browsing".to_string(),
            arguments: serde_json::json!({
                "action": "configure",
                "provider": "none"
            })
            .to_string(),
        },
    );

    let result = execute_tool(
        &tool_call,
        &engine,
        "thread-runtime-allow",
        None,
        &manager,
        None,
        &event_tx,
        root.path(),
        &engine.http_client,
        None,
    )
    .await;

    assert!(!result.is_error);
    let review = result
        .weles_review
        .expect("runtime allow result should carry governance metadata");
    assert!(review.weles_reviewed);
    assert_eq!(review.verdict, crate::agent::types::WelesVerdict::Allow);
    assert_eq!(
        review.audit_id.as_deref(),
        Some("audit-weles-runtime-allow")
    );
    assert!(review.reasons.iter().any(
        |reason| reason.contains("runtime review approved controlled browser reconfiguration")
    ));

    let recorded = recorded_bodies
        .lock()
        .expect("lock recorded assistant bodies");
    let request = recorded
        .iter()
        .find(|body: &&String| body.contains("## WELES Governance Core"))
        .expect("guarded execution should invoke WELES runtime");
    assert!(request.contains("tool_name: setup_web_browsing"));
}

#[tokio::test]
async fn execute_tool_low_risk_read_file_stays_direct_allow() {
    let root = tempdir().expect("tempdir should succeed");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager.clone(), AgentConfig::default(), root.path()).await;
    let (event_tx, _) = broadcast::channel(8);
    let file_path = root.path().join("notes.txt");
    tokio::fs::write(&file_path, "hello from read path\n")
        .await
        .expect("write test file should succeed");
    let tool_call = ToolCall::with_default_weles_review(
        "tool-read-file".to_string(),
        ToolFunction {
            name: "read_file".to_string(),
            arguments: serde_json::json!({ "path": file_path }).to_string(),
        },
    );

    let result = execute_tool(
        &tool_call,
        &engine,
        "thread-read-file",
        None,
        &manager,
        None,
        &event_tx,
        root.path(),
        &engine.http_client,
        None,
    )
    .await;

    assert!(!result.is_error);
    let review = result
        .weles_review
        .expect("direct allow should carry explicit governance metadata");
    assert!(!review.weles_reviewed);
    assert_eq!(review.verdict, crate::agent::types::WelesVerdict::Allow);
    assert!(review
        .reasons
        .iter()
        .any(|reason| reason.contains("allow_direct") || reason.contains("low-risk")));
}

#[tokio::test]
async fn execute_tool_unavailable_guarded_review_blocks_closed_normally_and_degrades_under_yolo() {
    let root = tempdir().expect("tempdir should succeed");
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.extra.insert(
        "weles_review_available".to_string(),
        serde_json::Value::Bool(false),
    );
    let engine = AgentEngine::new_test(manager.clone(), config, root.path()).await;
    let (event_tx, _) = broadcast::channel(8);

    let blocked_call = ToolCall::with_default_weles_review(
        "tool-setup-unavailable-block".to_string(),
        ToolFunction {
            name: "setup_web_browsing".to_string(),
            arguments: serde_json::json!({
                "action": "configure",
                "provider": "none"
            })
            .to_string(),
        },
    );
    let blocked_result = execute_tool(
        &blocked_call,
        &engine,
        "thread-setup-unavailable-block",
        None,
        &manager,
        None,
        &event_tx,
        root.path(),
        &engine.http_client,
        None,
    )
    .await;
    assert!(blocked_result.is_error);
    let blocked_review = blocked_result
        .weles_review
        .expect("blocked unavailable review should carry metadata");
    assert!(!blocked_review.weles_reviewed);
    assert_eq!(
        blocked_review.verdict,
        crate::agent::types::WelesVerdict::Block
    );
    assert!(blocked_review
        .reasons
        .iter()
        .any(|reason| reason.contains("unavailable")));

    {
        let config = engine.config.read().await;
        assert_eq!(
            config
                .extra
                .get("browse_provider")
                .and_then(|value| value.as_str()),
            None
        );
    }

    let yolo_call = ToolCall::with_default_weles_review(
        "tool-setup-unavailable-yolo".to_string(),
        ToolFunction {
            name: "setup_web_browsing".to_string(),
            arguments: serde_json::json!({
                "action": "configure",
                "provider": "none",
                "security_level": "yolo"
            })
            .to_string(),
        },
    );
    let yolo_result = execute_tool(
        &yolo_call,
        &engine,
        "thread-setup-unavailable-yolo",
        None,
        &manager,
        None,
        &event_tx,
        root.path(),
        &engine.http_client,
        None,
    )
    .await;
    let yolo_review = yolo_result
        .weles_review
        .expect("flag_only unavailable review should carry metadata");
    assert_eq!(
        yolo_review.verdict,
        crate::agent::types::WelesVerdict::FlagOnly
    );
    assert!(!yolo_review.weles_reviewed);
    assert_eq!(yolo_review.security_override_mode.as_deref(), Some("yolo"));
    assert!(yolo_review
        .reasons
        .iter()
        .any(|reason| reason.contains("unavailable")));

    let config = engine.config.read().await;
    assert_eq!(
        config
            .extra
            .get("browse_provider")
            .and_then(|value| value.as_str()),
        Some("none")
    );
}

#[tokio::test]
async fn execute_tool_weles_internal_task_allows_low_risk_shell_python_without_recursive_governance_review(
) {
    let root = tempdir().expect("tempdir should succeed");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager.clone(), AgentConfig::default(), root.path()).await;
    let (event_tx, _) = broadcast::channel(8);
    let thread_id = "thread-weles-internal-bypass";

    {
        let mut threads = engine.threads.write().await;
        threads.insert(
            thread_id.to_string(),
            crate::agent::types::AgentThread {
                id: thread_id.to_string(),
                agent_name: None,
                title: "WELES internal runtime thread".to_string(),
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

    let weles_task = super::spawn_weles_internal_subagent(
        &engine,
        thread_id,
        None,
        "governance",
        "bash_command",
        &serde_json::json!({"command": "python3 -c \"print('hi')\""}),
        SecurityLevel::Highest,
        &[],
    )
    .await
    .expect("daemon-owned WELES governance spawn should succeed");

    let before_tasks = engine.list_tasks().await.len();
    let marker = root.path().join("weles-internal-shell-python.txt");
    let tool_call = ToolCall::with_default_weles_review(
        "tool-weles-recursion-guard".to_string(),
        ToolFunction {
            name: "bash_command".to_string(),
            arguments: serde_json::json!({
                "command": format!(
                    "python3 -c \"from pathlib import Path; Path(r'{}').write_text('hi')\"",
                    marker.display()
                ),
                "timeout_seconds": 5
            })
            .to_string(),
        },
    );

    let result = execute_tool(
        &tool_call,
        &engine,
        thread_id,
        Some(&weles_task.id),
        &manager,
        None,
        &event_tx,
        root.path(),
        &engine.http_client,
        None,
    )
    .await;

    assert!(
        !result.is_error,
        "low-risk shell python should stay allowed inside WELES internal tasks: {}",
        result.content
    );
    assert!(
        marker.exists(),
        "low-risk shell python should still execute"
    );
    let review = result
        .weles_review
        .expect("internal WELES runtime should keep governance metadata");
    assert!(!review.weles_reviewed);
    assert_eq!(review.verdict, crate::agent::types::WelesVerdict::Allow);
    assert_eq!(engine.list_tasks().await.len(), before_tasks);
}
