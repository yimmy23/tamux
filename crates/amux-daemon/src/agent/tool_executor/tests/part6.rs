    #[tokio::test]
    async fn send_discord_message_uses_canonical_dm_reply_context_for_user_targets() {
        let root = tempdir().expect("tempdir");
        let manager = SessionManager::new_test(root.path()).await;
        let mut config = AgentConfig::default();
        config.gateway.enabled = true;
        let engine = AgentEngine::new_test(manager, config, root.path()).await;
        engine.init_gateway().await;
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        engine.set_gateway_ipc_sender(Some(tx)).await;

        {
            let mut gw_guard = engine.gateway_state.lock().await;
            let gw = gw_guard.as_mut().expect("gateway state should exist");
            gw.discord_dm_channels_by_user
                .insert("user:123456789".to_string(), "DM123".to_string());
            gw.reply_contexts.insert(
                "Discord:DM123".to_string(),
                crate::agent::gateway::ThreadContext {
                    discord_message_id: Some("987654321".to_string()),
                    ..Default::default()
                },
            );
        }

        let send_engine = engine.clone();
        let send_task = tokio::spawn(async move {
            execute_gateway_message(
                "send_discord_message",
                &serde_json::json!({
                    "user_id": "123456789",
                    "message": "discord reply"
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

        engine
            .complete_gateway_send_result(GatewaySendResult {
                correlation_id: request.correlation_id.clone(),
                platform: "discord".to_string(),
                channel_id: "DM123".to_string(),
                requested_channel_id: Some("user:123456789".to_string()),
                delivery_id: Some("delivery-2".to_string()),
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

    #[tokio::test]
    async fn send_telegram_message_emits_gateway_ipc_request() {
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
                "send_telegram_message",
                &serde_json::json!({
                    "chat_id": "777",
                    "message": "telegram reply",
                    "reply_to_message_id": 42
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
        assert_eq!(request.platform, "telegram");
        assert_eq!(request.channel_id, "777");
        assert_eq!(request.thread_id.as_deref(), Some("42"));
        assert_eq!(request.content, "telegram reply");

        engine
            .complete_gateway_send_result(GatewaySendResult {
                correlation_id: request.correlation_id.clone(),
                platform: "telegram".to_string(),
                channel_id: "777".to_string(),
                requested_channel_id: Some("777".to_string()),
                delivery_id: Some("99".to_string()),
                ok: true,
                error: None,
                completed_at_ms: 1,
            })
            .await;

        let result = send_task
            .await
            .expect("send task should join")
            .expect("send should succeed");
        assert_eq!(result, "Telegram message sent to 777");
    }

    // -----------------------------------------------------------------------
    // Source authority classification tests (UNCR-03)
    // -----------------------------------------------------------------------

    use super::{classify_freshness, classify_source_authority, format_result_with_authority};

    #[test]
    fn classify_source_authority_official_rust_docs() {
        assert_eq!(
            classify_source_authority("https://docs.rust-lang.org/book/"),
            "official"
        );
    }

    #[test]
    fn classify_source_authority_community_stackoverflow() {
        assert_eq!(
            classify_source_authority("https://stackoverflow.com/questions/123"),
            "community"
        );
    }

    #[test]
    fn classify_source_authority_unknown_random_site() {
        assert_eq!(
            classify_source_authority("https://random-site.example.com"),
            "unknown"
        );
    }

    #[test]
    fn classify_source_authority_official_mdn() {
        assert_eq!(
            classify_source_authority("https://developer.mozilla.org/en-US/docs"),
            "official"
        );
    }

    #[test]
    fn classify_source_authority_community_reddit() {
        assert_eq!(
            classify_source_authority("https://reddit.com/r/rust"),
            "community"
        );
    }

    #[test]
    fn classify_source_authority_community_medium() {
        assert_eq!(
            classify_source_authority("https://medium.com/@author/article"),
            "community"
        );
    }

    #[test]
    fn classify_source_authority_official_cppreference() {
        assert_eq!(
            classify_source_authority("https://cppreference.com/w/cpp"),
            "official"
        );
    }

    #[test]
    fn classify_source_authority_empty_string_no_panic() {
        // Should return "unknown" without panicking.
        assert_eq!(classify_source_authority(""), "unknown");
    }

    #[test]
    fn format_result_with_authority_prepends_official_tag() {
        let result = format_result_with_authority(
            "Rust Book",
            "https://docs.rust-lang.org/book/",
            "The Rust Programming Language",
        );
        assert!(result.starts_with("- [official]"));
        assert!(result.contains("**Rust Book**"));
        assert!(result.contains("https://docs.rust-lang.org/book/"));
        assert!(result.contains("The Rust Programming Language"));
        assert!(
            result.contains("freshness:"),
            "research result formatting should expose freshness alongside source authority"
        );
    }

    #[test]
    fn classify_freshness_labels_recent_stale_and_old_dates() {
        assert_eq!(classify_freshness(Some("2026-03-20")), "recent");
        assert_eq!(classify_freshness(Some("2025-12-01T14:00:00Z")), "stale");
        assert_eq!(classify_freshness(Some("2024-01-01")), "old");
        assert_eq!(classify_freshness(Some("not-a-date")), "unknown");
        assert_eq!(classify_freshness(None), "unknown");
    }

    #[tokio::test]
    async fn spawn_subagent_bootstraps_todos_for_planned_chat_threads() {
        let root = tempdir().expect("tempdir should succeed");
        let manager = SessionManager::new_test(root.path()).await;
        let engine = AgentEngine::new_test(manager.clone(), AgentConfig::default(), root.path()).await;
        let (event_tx, _) = broadcast::channel(8);
        let thread_id = "thread-planned-subagents";

        {
            let mut threads = engine.threads.write().await;
            threads.insert(
                thread_id.to_string(),
                crate::agent::types::AgentThread {
                    id: thread_id.to_string(),
                    title: "Parallel skill work".to_string(),
                    messages: vec![crate::agent::types::AgentMessage::user(
                        "Investigate the failing tests, then update the parser, and finally rerun the suite.",
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

        let tool_call = ToolCall::with_default_weles_review(
            "tool-spawn-subagent-bootstrap".to_string(),
            ToolFunction {
                name: "spawn_subagent".to_string(),
                arguments: serde_json::json!({
                    "title": "Write foundational skill files",
                    "description": "Create the foundational skill files in parallel so the parent can integrate the batches."
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

        assert!(
            !result.is_error,
            "spawn_subagent should bootstrap plan state instead of failing: {}",
            result.content
        );
        assert!(result.content.contains("Spawned subagent"));

        let todos = engine.get_todos(thread_id).await;
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0].status, TodoStatus::InProgress);
        assert!(
            todos[0].content.contains("Write foundational skill files"),
            "bootstrap todo should reflect the delegated work"
        );

        let tasks = engine.list_tasks().await;
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].source, "subagent");
        assert_eq!(tasks[0].parent_thread_id.as_deref(), Some(thread_id));
    }
