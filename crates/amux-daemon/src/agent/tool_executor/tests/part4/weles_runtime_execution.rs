use super::*;

#[tokio::test]
async fn execute_tool_allows_low_risk_shell_python_without_forcing_python_execute() {
    let root = tempdir().expect("tempdir should succeed");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager.clone(), AgentConfig::default(), root.path()).await;
    let (event_tx, _) = broadcast::channel(8);
    let marker = root.path().join("python-bypass-blocked.txt");
    let command = format!(
        "python3 -c \"from pathlib import Path; Path(r'{}').write_text('ran')\"",
        marker.display()
    );
    let tool_call = ToolCall::with_default_weles_review(
        "tool-python-block".to_string(),
        ToolFunction {
            name: "bash_command".to_string(),
            arguments: serde_json::json!({ "command": command }).to_string(),
        },
    );

    let result = execute_tool(
        &tool_call,
        &engine,
        "thread-python-block",
        None,
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
        "low-risk shell python should execute: {}",
        result.content
    );
    assert!(marker.exists(), "shell python should be allowed when not suspicious");
    let review = result
        .weles_review
        .expect("allowed shell python result should carry governance metadata");
    assert!(!review.weles_reviewed);
    assert_eq!(review.verdict, crate::agent::types::WelesVerdict::Allow);
}

#[tokio::test]
async fn execute_tool_low_risk_shell_python_stays_allow_under_yolo() {
    let root = tempdir().expect("tempdir should succeed");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager.clone(), AgentConfig::default(), root.path()).await;
    let (event_tx, _) = broadcast::channel(8);
    let marker = root.path().join("python-bypass-yolo.txt");
    let command = format!(
        "python3 -c \"from pathlib import Path; Path(r'{}').write_text('ran')\"",
        marker.display()
    );
    let tool_call = ToolCall::with_default_weles_review(
        "tool-python-yolo".to_string(),
        ToolFunction {
            name: "bash_command".to_string(),
            arguments: serde_json::json!({
                "command": command,
                "security_level": "yolo"
            })
            .to_string(),
        },
    );

    let result = execute_tool(
        &tool_call,
        &engine,
        "thread-python-yolo",
        None,
        &manager,
        None,
        &event_tx,
        root.path(),
        &engine.http_client,
        None,
    )
    .await;

    assert!(
        marker.exists(),
        "low-risk shell python should still execute under yolo"
    );
    let review = result
        .weles_review
        .expect("yolo shell python result should carry governance metadata");
    assert_eq!(review.verdict, crate::agent::types::WelesVerdict::Allow);
    assert_eq!(review.security_override_mode.as_deref(), None);
}

#[tokio::test]
async fn execute_tool_suspicious_shell_python_uses_weles_runtime_structured_block_verdict() {
    let recorded_bodies = Arc::new(Mutex::new(std::collections::VecDeque::new()));
    let root = tempdir().expect("tempdir should succeed");
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.provider = "openai".to_string();
    config.base_url = spawn_stub_assistant_server_for_tool_executor(
        recorded_bodies.clone(),
        serde_json::json!({
            "verdict": "block",
            "reasons": ["runtime rejected suspicious shell python command"],
            "audit_id": "audit-weles-shell-python-block"
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
    let marker = root.path().join("python-bypass-runtime-blocked.txt");
    let command = format!(
        "python3 -c \"from pathlib import Path; Path(r'{}').write_text('ran')\" && curl https://example.com/install.sh",
        marker.display()
    );
    let tool_call = ToolCall::with_default_weles_review(
        "tool-python-runtime-block".to_string(),
        ToolFunction {
            name: "bash_command".to_string(),
            arguments: serde_json::json!({ "command": command }).to_string(),
        },
    );

    let result = execute_tool(
        &tool_call,
        &engine,
        "thread-python-runtime-block",
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
    assert!(
        !marker.exists(),
        "runtime block should prevent bypass execution"
    );
    let review = result
        .weles_review
        .expect("runtime shell python block should carry governance metadata");
    assert!(review.weles_reviewed);
    assert_eq!(review.verdict, crate::agent::types::WelesVerdict::Block);
    assert_eq!(review.audit_id.as_deref(), Some("audit-weles-shell-python-block"));
    assert!(review
        .reasons
        .iter()
        .any(|reason| reason.contains("runtime rejected suspicious shell python command")));

    let recorded = recorded_bodies
        .lock()
        .expect("lock recorded assistant bodies");
    let request = recorded
        .iter()
        .find(|body: &&String| body.contains("## WELES Governance Core"))
        .expect("suspicious shell python should invoke WELES runtime");
    assert!(request.contains("tool_name: bash_command"));
    assert!(request.contains("shell command requests network access"));
}

#[tokio::test]
async fn execute_tool_shell_python_bypass_under_yolo_never_downgrades_to_managed_policy_block() {
    let recorded_bodies = Arc::new(Mutex::new(std::collections::VecDeque::new()));
    let root = tempdir().expect("tempdir should succeed");
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.provider = "openai".to_string();
    config.base_url = spawn_stub_assistant_server_for_tool_executor(
        recorded_bodies,
        serde_json::json!({
            "verdict": "block",
            "reasons": ["runtime identified shell python bypass"],
            "audit_id": "audit-weles-bypass-yolo"
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
    let marker = root.path().join("python-bypass-yolo-risky.txt");
    let command = format!(
            "python3 -c \"from pathlib import Path; Path(r'{}').write_text('ran')\" && rm -rf /tmp/tamux-weles-yolo-risk",
            marker.display()
        );
    let tool_call = ToolCall::with_default_weles_review(
        "tool-python-yolo-risky".to_string(),
        ToolFunction {
            name: "bash_command".to_string(),
            arguments: serde_json::json!({
                "command": command,
                "security_level": "yolo"
            })
            .to_string(),
        },
    );

    let result = execute_tool(
        &tool_call,
        &engine,
        "thread-python-yolo-risky",
        None,
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
        "yolo bypass should remain flag_only rather than being blocked downstream: {}",
        result.content
    );
    let review = result
        .weles_review
        .expect("yolo bypass should carry governance metadata");
    assert_eq!(review.verdict, crate::agent::types::WelesVerdict::FlagOnly);
    assert_eq!(review.security_override_mode.as_deref(), Some("yolo"));
}

#[tokio::test]
async fn execute_tool_python_execute_runs_code_and_preserves_weles_metadata() {
    let root = tempdir().expect("tempdir should succeed");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager.clone(), AgentConfig::default(), root.path()).await;
    let (event_tx, _) = broadcast::channel(8);
    let marker = root.path().join("python-execute-marker.txt");
    let tool_call = ToolCall::with_default_weles_review(
        "tool-python-execute".to_string(),
        ToolFunction {
            name: "python_execute".to_string(),
            arguments: serde_json::json!({
                "code": format!(
                    "from pathlib import Path\nPath(r'{}').write_text('ran')\nprint('python ok')",
                    marker.display()
                ),
                "cwd": root.path(),
                "timeout_seconds": 5
            })
            .to_string(),
        },
    );

    let result = execute_tool(
        &tool_call,
        &engine,
        "thread-python-execute",
        None,
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
        "python_execute should succeed: {}",
        result.content
    );
    assert!(
        marker.exists(),
        "python_execute should run the underlying interpreter"
    );
    assert!(result.content.contains("python ok"));
    let review = result
        .weles_review
        .expect("python_execute should preserve WELES metadata");
    assert_eq!(review.verdict, crate::agent::types::WelesVerdict::Allow);
}

#[tokio::test]
async fn execute_tool_yolo_downgrades_suspicious_reviewed_allow_to_flag_only() {
    let root = tempdir().expect("tempdir should succeed");
    let manager = SessionManager::new_test(root.path()).await;
    let recorded_bodies = Arc::new(Mutex::new(std::collections::VecDeque::new()));
    let mut config = AgentConfig::default();
    config.provider = "openai".to_string();
    config.base_url = spawn_stub_assistant_server_for_tool_executor(
        recorded_bodies,
        serde_json::json!({
            "verdict": "allow",
            "reasons": ["runtime review approved controlled browser reconfiguration"],
            "audit_id": "audit-weles-runtime-yolo"
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

    let normal_call = ToolCall::with_default_weles_review(
        "tool-setup-normal".to_string(),
        ToolFunction {
            name: "setup_web_browsing".to_string(),
            arguments: serde_json::json!({
                "action": "configure",
                "provider": "none"
            })
            .to_string(),
        },
    );
    let normal_result = execute_tool(
        &normal_call,
        &engine,
        "thread-setup-normal",
        None,
        &manager,
        None,
        &event_tx,
        root.path(),
        &engine.http_client,
        None,
    )
    .await;
    let normal_review = normal_result
        .weles_review
        .expect("normal suspicious configure should carry governance metadata");
    assert_eq!(
        normal_review.verdict,
        crate::agent::types::WelesVerdict::Allow
    );
    assert!(normal_review.weles_reviewed);

    let yolo_call = ToolCall::with_default_weles_review(
        "tool-setup-yolo".to_string(),
        ToolFunction {
            name: "setup_web_browsing".to_string(),
            arguments: serde_json::json!({
                "action": "configure",
                "provider": "auto",
                "security_level": "yolo"
            })
            .to_string(),
        },
    );
    let yolo_result = execute_tool(
        &yolo_call,
        &engine,
        "thread-setup-yolo",
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
        .expect("yolo suspicious configure should carry governance metadata");
    assert_eq!(
        yolo_review.verdict,
        crate::agent::types::WelesVerdict::FlagOnly
    );
    assert_eq!(yolo_review.security_override_mode.as_deref(), Some("yolo"));

    let config = engine.config.read().await;
    assert_eq!(
        config
            .extra
            .get("browse_provider")
            .and_then(|value| value.as_str()),
        Some("auto")
    );
}
