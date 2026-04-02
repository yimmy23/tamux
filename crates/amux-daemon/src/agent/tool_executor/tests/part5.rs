    #[tokio::test]
    async fn managed_command_wait_can_be_cancelled() {
        let (_tx, mut rx) = broadcast::channel(4);
        let token = CancellationToken::new();
        token.cancel();

        let error =
            wait_for_managed_command_outcome(&mut rx, SessionId::nil(), "exec-1", 30, Some(&token))
                .await
                .err()
                .expect("managed wait should abort when cancellation is requested");

        assert!(error.to_string().contains("cancelled"));
    }

    #[tokio::test]
    async fn managed_command_wait_fails_when_session_exits() {
        let (tx, mut rx) = broadcast::channel(4);
        tx.send(DaemonMessage::SessionExited {
            id: SessionId::nil(),
            exit_code: Some(1),
        })
        .expect("session exit should broadcast");

        let error = wait_for_managed_command_outcome(&mut rx, SessionId::nil(), "exec-1", 30, None)
            .await
            .expect_err("managed wait should fail when the session exits");

        assert!(error.to_string().contains("session exited"));
    }

    #[tokio::test]
    async fn headless_shell_command_can_be_cancelled() {
        let root = tempfile::tempdir().unwrap();
        let session_manager = SessionManager::new_test(root.path()).await;
        let token = CancellationToken::new();
        let cancel = token.clone();

        let join = tokio::spawn(async move {
            execute_headless_shell_command(
                &serde_json::json!({
                    "command": "sleep 30",
                    "timeout_seconds": 30
                }),
                &session_manager,
                None,
                "bash_command",
                Some(token),
            )
            .await
        });

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        cancel.cancel();

        let error = join
            .await
            .expect("headless shell join should complete")
            .expect_err("headless shell should abort when cancellation is requested");

        assert!(error.to_string().contains("cancelled"));
    }

    #[test]
    fn resolve_skill_path_finds_generated_skill_by_stem() {
        let root = std::env::temp_dir().join(format!("tamux-skill-test-{}", uuid::Uuid::new_v4()));
        let generated = root.join("generated");
        fs::create_dir_all(&generated).expect("skill test directory should be created");
        let skill_path = generated.join("build-release.md");
        fs::write(&skill_path, "# Build release\n").expect("skill file should be written");

        let resolved = resolve_skill_path(&root, "build-release", None)
            .expect("generated skill should resolve");
        assert_eq!(resolved, skill_path);

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn resolve_skill_path_prefers_selected_variant() {
        let root = std::env::temp_dir().join(format!("tamux-skill-test-{}", uuid::Uuid::new_v4()));
        let generated = root.join("generated");
        fs::create_dir_all(&generated).expect("skill test directory should be created");
        let canonical = generated.join("build-release.md");
        let frontend = generated.join("build-release--frontend.md");
        fs::write(&canonical, "# Build release\n").expect("canonical skill file should be written");
        fs::write(&frontend, "# Frontend build release\n")
            .expect("variant skill file should be written");

        let resolved = resolve_skill_path(
            &root,
            "build-release",
            Some(&SkillVariantRecord {
                variant_id: "variant-1".to_string(),
                skill_name: "build-release".to_string(),
                variant_name: "frontend".to_string(),
                relative_path: "generated/build-release--frontend.md".to_string(),
                parent_variant_id: Some("parent-1".to_string()),
                version: "v2.0".to_string(),
                context_tags: vec!["frontend".to_string()],
                use_count: 0,
                success_count: 0,
                failure_count: 0,
                status: "active".to_string(),
                last_used_at: None,
                created_at: 0,
                updated_at: 0,
            }),
        )
        .expect("selected variant should resolve");
        assert_eq!(resolved, frontend);

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn list_sessions_tool_requires_workspace_topology() {
        let config = AgentConfig::default();
        let temp_dir = std::env::temp_dir();

        let no_topology = get_available_tools(&config, &temp_dir, false);
        assert!(no_topology
            .iter()
            .all(|tool| tool.function.name != "list_sessions"));
        assert!(no_topology
            .iter()
            .any(|tool| tool.function.name == "list_terminals"));

        let with_topology = get_available_tools(&config, &temp_dir, true);
        assert!(with_topology
            .iter()
            .any(|tool| tool.function.name == "list_sessions"));
    }

    #[test]
    fn python_execute_tool_is_exposed_with_expected_schema() {
        let config = AgentConfig::default();
        let temp_dir = std::env::temp_dir();
        let tools = get_available_tools(&config, &temp_dir, false);
        let python_execute = tools
            .iter()
            .find(|tool| tool.function.name == "python_execute")
            .expect("python_execute tool should be available");

        let properties = python_execute
            .function
            .parameters
            .get("properties")
            .expect("python_execute schema should expose properties");

        assert!(properties.get("code").is_some(), "schema should include code");
        assert!(properties.get("cwd").is_some(), "schema should include cwd");
        assert!(
            properties.get("timeout_seconds").is_some(),
            "schema should include timeout_seconds"
        );
        assert_eq!(
            python_execute
                .function
                .parameters
                .get("required")
                .and_then(|value| value.as_array())
                .map(|items| items.iter().filter_map(|item| item.as_str()).collect::<Vec<_>>()),
            Some(vec!["code"])
        );
    }

    #[test]
    fn current_datetime_tool_is_exposed() {
        let config = AgentConfig::default();
        let temp_dir = std::env::temp_dir();
        let tools = get_available_tools(&config, &temp_dir, false);
        let current_datetime = tools
            .iter()
            .find(|tool| tool.function.name == "get_current_datetime")
            .expect("get_current_datetime tool should be available");

        let properties = current_datetime
            .function
            .parameters
            .get("properties")
            .and_then(|value| value.as_object())
            .expect("get_current_datetime schema should expose properties object");

        assert!(properties.is_empty(), "datetime tool should not require arguments");
        assert!(current_datetime.function.parameters.get("required").is_none());
    }

    #[test]
    fn scrub_sensitive_redacts_common_api_key_lines() {
        let input = "openai api_key=sk-live-secret\nAuthorization: Bearer abc123secret";
        let scrubbed = crate::scrub::scrub_sensitive(input);
        assert!(!scrubbed.contains("sk-live-secret"));
        assert!(!scrubbed.contains("abc123secret"));
        assert!(scrubbed.contains("***REDACTED***"));
    }

    #[tokio::test]
    async fn divergent_tool_get_session_serializes_completion_fields() {
        let root = tempdir().expect("tempdir");
        let manager = SessionManager::new_test(root.path()).await;
        let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
        let session_id = engine
            .start_divergent_session("pick caching strategy", None, "thread-tool-div", None)
            .await
            .expect("start divergent session");
        let labels = {
            let sessions = engine.divergent_sessions.read().await;
            sessions
                .get(&session_id)
                .expect("session exists")
                .framings
                .iter()
                .map(|f| f.label.clone())
                .collect::<Vec<_>>()
        };
        for (idx, label) in labels.iter().enumerate() {
            engine
                .record_divergent_contribution(
                    &session_id,
                    label,
                    if idx == 0 {
                        "Prefer deterministic correctness-first approach"
                    } else {
                        "Prefer lower-latency pragmatic approach"
                    },
                )
                .await
                .expect("record contribution");
        }
        engine
            .complete_divergent_session(&session_id)
            .await
            .expect("complete divergent session");

        let response = execute_get_divergent_session(
            &serde_json::json!({ "session_id": session_id }),
            &engine,
        )
        .await
        .expect("tool execution should succeed");
        let payload: serde_json::Value =
            serde_json::from_str(&response).expect("tool payload should be valid JSON");
        assert_eq!(
            payload.get("status").and_then(|v| v.as_str()),
            Some("complete")
        );
        assert!(payload
            .get("tensions_markdown")
            .and_then(|v| v.as_str())
            .is_some_and(|value| !value.is_empty()));
        assert!(payload
            .get("mediator_prompt")
            .and_then(|v| v.as_str())
            .is_some_and(|value| !value.is_empty()));
    }

    #[tokio::test]
    async fn divergent_tool_get_session_in_progress_omits_completion_output() {
        let root = tempdir().expect("tempdir");
        let manager = SessionManager::new_test(root.path()).await;
        let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
        let session_id = engine
            .start_divergent_session("evaluate rollout sequence", None, "thread-tool-div-2", None)
            .await
            .expect("start divergent session");

        let response = execute_get_divergent_session(
            &serde_json::json!({ "session_id": session_id }),
            &engine,
        )
        .await
        .expect("tool execution should succeed");
        let payload: serde_json::Value =
            serde_json::from_str(&response).expect("tool payload should be valid JSON");
        assert_eq!(
            payload.get("status").and_then(|v| v.as_str()),
            Some("running")
        );
        assert!(
            payload
                .get("tensions_markdown")
                .is_some_and(|v| v.is_null()),
            "in-progress session should not report tensions output"
        );
        assert!(
            payload.get("mediator_prompt").is_some_and(|v| v.is_null()),
            "in-progress session should not report mediator output"
        );
        let progress = payload
            .get("framing_progress")
            .expect("framing_progress should exist");
        assert_eq!(progress.get("completed").and_then(|v| v.as_u64()), Some(0));
    }

    #[tokio::test]
    async fn send_slack_message_emits_gateway_ipc_request() {
        let root = tempdir().expect("tempdir");
        let manager = SessionManager::new_test(root.path()).await;
        let mut config = AgentConfig::default();
        config.gateway.enabled = true;
        let engine = AgentEngine::new_test(manager, config, root.path()).await;
        engine.init_gateway().await;
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        engine.set_gateway_ipc_sender(Some(tx)).await;

        let send_engine = engine.clone();
        let send_task = tokio::spawn(async move {
            execute_gateway_message(
                "send_slack_message",
                &serde_json::json!({
                    "channel": "C123",
                    "message": "hello from daemon",
                    "thread_ts": "1712345678.000100"
                }),
                &send_engine,
                &reqwest::Client::new(),
            )
            .await
        });

        let request = match timeout(Duration::from_millis(250), rx.recv())
            .await
            .expect("gateway send request should be emitted")
            .expect("gateway send request should exist")
        {
            DaemonMessage::GatewaySendRequest { request } => request,
            other => panic!("expected GatewaySendRequest, got {other:?}"),
        };
        assert_eq!(request.platform, "slack");
        assert_eq!(request.channel_id, "C123");
        assert_eq!(request.thread_id.as_deref(), Some("1712345678.000100"));
        assert_eq!(request.content, "hello from daemon");

        engine
            .complete_gateway_send_result(GatewaySendResult {
                correlation_id: request.correlation_id.clone(),
                platform: "slack".to_string(),
                channel_id: "C123".to_string(),
                requested_channel_id: Some("C123".to_string()),
                delivery_id: Some("1712345678.000200".to_string()),
                ok: true,
                error: None,
                completed_at_ms: 1,
            })
            .await;

        let result = send_task
            .await
            .expect("send task should join")
            .expect("send should succeed");
        assert_eq!(result, "Slack message sent to #C123");
    }

    #[tokio::test]
    async fn send_discord_message_emits_gateway_ipc_request() {
        let root = tempdir().expect("tempdir");
        let manager = SessionManager::new_test(root.path()).await;
        let mut config = AgentConfig::default();
        config.gateway.enabled = true;
        let engine = AgentEngine::new_test(manager, config, root.path()).await;
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        engine.set_gateway_ipc_sender(Some(tx)).await;

        let send_engine = engine.clone();
        let send_task = tokio::spawn(async move {
            execute_gateway_message(
                "send_discord_message",
                &serde_json::json!({
                    "user_id": "123456789",
                    "message": "discord reply",
                    "reply_to_message_id": "987654321"
                }),
                &send_engine,
                &reqwest::Client::new(),
            )
            .await
        });

        let request = match timeout(Duration::from_millis(250), rx.recv())
            .await
            .expect("gateway send request should be emitted")
            .expect("gateway send request should exist")
        {
            DaemonMessage::GatewaySendRequest { request } => request,
            other => panic!("expected GatewaySendRequest, got {other:?}"),
        };
        assert_eq!(request.platform, "discord");
        assert_eq!(request.channel_id, "user:123456789");
        assert_eq!(request.thread_id.as_deref(), Some("987654321"));
        assert_eq!(request.content, "discord reply");

        engine
            .complete_gateway_send_result(GatewaySendResult {
                correlation_id: request.correlation_id.clone(),
                platform: "discord".to_string(),
                channel_id: "user:123456789".to_string(),
                requested_channel_id: Some("user:123456789".to_string()),
                delivery_id: Some("delivery-1".to_string()),
                ok: true,
                error: None,
                completed_at_ms: 1,
            })
            .await;

        let result = send_task
            .await
            .expect("send task should join")
            .expect("send should succeed");
        assert_eq!(result, "Discord message sent to user:123456789");
    }
