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
                    agent_name: None,
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

    #[tokio::test]
    async fn handoff_thread_agent_push_updates_active_responder_and_writes_system_event() {
        let root = tempdir().expect("tempdir should succeed");
        let manager = SessionManager::new_test(root.path()).await;
        let engine =
            AgentEngine::new_test(manager.clone(), AgentConfig::default(), root.path()).await;
        let (event_tx, _) = broadcast::channel(8);
        let thread_id = "thread-handoff-push";

        {
            let mut threads = engine.threads.write().await;
            threads.insert(
                thread_id.to_string(),
                crate::agent::types::AgentThread {
                    id: thread_id.to_string(),
                    agent_name: Some(crate::agent::agent_identity::MAIN_AGENT_NAME.to_string()),
                    title: "Handoff candidate".to_string(),
                    messages: vec![crate::agent::types::AgentMessage::user(
                        "Please ask Weles to review the risky migration.",
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
                crate::agent::ThreadHandoffState {
                    origin_agent_id: crate::agent::agent_identity::MAIN_AGENT_ID.to_string(),
                    active_agent_id: crate::agent::agent_identity::MAIN_AGENT_ID.to_string(),
                    responder_stack: vec![crate::agent::ThreadResponderFrame {
                        agent_id: crate::agent::agent_identity::MAIN_AGENT_ID.to_string(),
                        agent_name: crate::agent::agent_identity::MAIN_AGENT_NAME.to_string(),
                        entered_at: 1,
                        entered_via_handoff_event_id: None,
                        linked_thread_id: None,
                    }],
                    events: Vec::new(),
                    pending_approval_id: None,
                },
            )
            .await;

        let tool_call = ToolCall::with_default_weles_review(
            "tool-handoff-thread-push".to_string(),
            ToolFunction {
                name: "handoff_thread_agent".to_string(),
                arguments: serde_json::json!({
                    "action": "push_handoff",
                    "target_agent_id": "weles",
                    "reason": "Risky migration needs governance review",
                    "summary": "Review the migration plan, identify risk, and continue answering from Weles.",
                    "requested_by": "user"
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

        assert!(!result.is_error, "handoff push should succeed: {}", result.content);
        assert!(result.pending_approval.is_none());
        let state = engine
            .thread_handoff_state(thread_id)
            .await
            .expect("handoff state should exist");
        assert_eq!(state.active_agent_id, "weles");
        assert_eq!(state.responder_stack.len(), 2);

        let thread = engine
            .threads
            .read()
            .await
            .get(thread_id)
            .cloned()
            .expect("thread should exist");
        assert_eq!(thread.agent_name.as_deref(), Some("Weles"));
        let system_event = thread
            .messages
            .iter()
            .find(|message| message.role == crate::agent::types::MessageRole::System)
            .expect("handoff should append a system event");
        assert!(
            system_event.content.contains("[[handoff_event]]"),
            "system event should use the structured handoff marker"
        );
        assert!(system_event.content.contains("\"to_agent_name\":\"Weles\""));
    }

    #[tokio::test]
    async fn handoff_thread_agent_push_accepts_svarog_alias_for_main_agent() {
        let root = tempdir().expect("tempdir should succeed");
        let manager = SessionManager::new_test(root.path()).await;
        let engine =
            AgentEngine::new_test(manager.clone(), AgentConfig::default(), root.path()).await;
        let (event_tx, _) = broadcast::channel(8);
        let thread_id = "thread-handoff-push-svarog-alias";

        {
            let mut threads = engine.threads.write().await;
            threads.insert(
                thread_id.to_string(),
                crate::agent::types::AgentThread {
                    id: thread_id.to_string(),
                    agent_name: Some(crate::agent::agent_identity::WELES_AGENT_NAME.to_string()),
                    title: "Handoff alias candidate".to_string(),
                    messages: vec![crate::agent::types::AgentMessage::user(
                        "Please hand this back to Svarog.",
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
                crate::agent::ThreadHandoffState {
                    origin_agent_id: crate::agent::agent_identity::MAIN_AGENT_ID.to_string(),
                    active_agent_id: crate::agent::agent_identity::WELES_AGENT_ID.to_string(),
                    responder_stack: vec![
                        crate::agent::ThreadResponderFrame {
                            agent_id: crate::agent::agent_identity::MAIN_AGENT_ID.to_string(),
                            agent_name: crate::agent::agent_identity::MAIN_AGENT_NAME.to_string(),
                            entered_at: 1,
                            entered_via_handoff_event_id: None,
                            linked_thread_id: None,
                        },
                        crate::agent::ThreadResponderFrame {
                            agent_id: crate::agent::agent_identity::WELES_AGENT_ID.to_string(),
                            agent_name: crate::agent::agent_identity::WELES_AGENT_NAME.to_string(),
                            entered_at: 2,
                            entered_via_handoff_event_id: Some("handoff-existing".to_string()),
                            linked_thread_id: Some("handoff:existing".to_string()),
                        },
                    ],
                    events: Vec::new(),
                    pending_approval_id: None,
                },
            )
            .await;

        let tool_call = ToolCall::with_default_weles_review(
            "tool-handoff-thread-push-svarog-alias".to_string(),
            ToolFunction {
                name: "handoff_thread_agent".to_string(),
                arguments: serde_json::json!({
                    "action": "push_handoff",
                    "target_agent_id": "svarog",
                    "reason": "Operator requested to switch to Svarog",
                    "summary": "Operator wants to continue with Svarog, the main agent.",
                    "requested_by": "user"
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

        assert!(!result.is_error, "svarog alias handoff should succeed: {}", result.content);
        let state = engine
            .thread_handoff_state(thread_id)
            .await
            .expect("handoff state should exist");
        assert_eq!(state.active_agent_id, crate::agent::agent_identity::MAIN_AGENT_ID);
    }

    #[tokio::test]
    async fn handoff_thread_agent_agent_push_requires_approval_outside_yolo() {
        let root = tempdir().expect("tempdir should succeed");
        let manager = SessionManager::new_test(root.path()).await;
        let mut config = AgentConfig::default();
        config.managed_execution.security_level = SecurityLevel::Moderate;
        let engine = AgentEngine::new_test(manager.clone(), config, root.path()).await;
        let (event_tx, _) = broadcast::channel(8);
        let thread_id = "thread-handoff-approval";

        {
            let mut threads = engine.threads.write().await;
            threads.insert(
                thread_id.to_string(),
                crate::agent::types::AgentThread {
                    id: thread_id.to_string(),
                    agent_name: Some(crate::agent::agent_identity::MAIN_AGENT_NAME.to_string()),
                    title: "Approval handoff".to_string(),
                    messages: vec![crate::agent::types::AgentMessage::user(
                        "Maybe ask Weles to take over this risky thread.",
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
                crate::agent::ThreadHandoffState {
                    origin_agent_id: crate::agent::agent_identity::MAIN_AGENT_ID.to_string(),
                    active_agent_id: crate::agent::agent_identity::MAIN_AGENT_ID.to_string(),
                    responder_stack: vec![crate::agent::ThreadResponderFrame {
                        agent_id: crate::agent::agent_identity::MAIN_AGENT_ID.to_string(),
                        agent_name: crate::agent::agent_identity::MAIN_AGENT_NAME.to_string(),
                        entered_at: 1,
                        entered_via_handoff_event_id: None,
                        linked_thread_id: None,
                    }],
                    events: Vec::new(),
                    pending_approval_id: None,
                },
            )
            .await;

        let tool_call = ToolCall::with_default_weles_review(
            "tool-handoff-thread-approval".to_string(),
            ToolFunction {
                name: "handoff_thread_agent".to_string(),
                arguments: serde_json::json!({
                    "action": "push_handoff",
                    "target_agent_id": "weles",
                    "reason": "Governance review required",
                    "summary": "Let Weles take over and continue this safety-sensitive thread.",
                    "requested_by": "agent"
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

        assert!(!result.is_error, "approval handoff should not hard-fail");
        let pending = result
            .pending_approval
            .as_ref()
            .expect("agent-initiated handoff should request approval");
        assert!(pending.command.contains("handoff_thread_agent"));

        let state = engine
            .thread_handoff_state(thread_id)
            .await
            .expect("handoff state should still exist");
        assert_eq!(
            state.active_agent_id,
            crate::agent::agent_identity::MAIN_AGENT_ID,
            "active responder should not switch before approval"
        );
        assert_eq!(state.pending_approval_id.as_deref(), Some(pending.approval_id.as_str()));

        let tasks = engine.list_tasks().await;
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].status, crate::agent::types::TaskStatus::AwaitingApproval);
        assert_eq!(tasks[0].source, "thread_handoff");
        assert_eq!(tasks[0].awaiting_approval_id.as_deref(), Some(pending.approval_id.as_str()));
    }

    #[tokio::test]
    async fn handoff_thread_agent_return_pops_stack_and_restores_previous_responder() {
        let root = tempdir().expect("tempdir should succeed");
        let manager = SessionManager::new_test(root.path()).await;
        let engine =
            AgentEngine::new_test(manager.clone(), AgentConfig::default(), root.path()).await;
        let (event_tx, _) = broadcast::channel(8);
        let thread_id = "thread-handoff-return";

        {
            let mut threads = engine.threads.write().await;
            threads.insert(
                thread_id.to_string(),
                crate::agent::types::AgentThread {
                    id: thread_id.to_string(),
                    agent_name: Some(crate::agent::agent_identity::WELES_AGENT_NAME.to_string()),
                    title: "Return handoff".to_string(),
                    messages: vec![crate::agent::types::AgentMessage::user(
                        "Let Weles finish and then hand the thread back.",
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
                crate::agent::ThreadHandoffState {
                    origin_agent_id: crate::agent::agent_identity::MAIN_AGENT_ID.to_string(),
                    active_agent_id: crate::agent::agent_identity::WELES_AGENT_ID.to_string(),
                    responder_stack: vec![
                        crate::agent::ThreadResponderFrame {
                            agent_id: crate::agent::agent_identity::MAIN_AGENT_ID.to_string(),
                            agent_name: crate::agent::agent_identity::MAIN_AGENT_NAME.to_string(),
                            entered_at: 1,
                            entered_via_handoff_event_id: None,
                            linked_thread_id: None,
                        },
                        crate::agent::ThreadResponderFrame {
                            agent_id: crate::agent::agent_identity::WELES_AGENT_ID.to_string(),
                            agent_name: crate::agent::agent_identity::WELES_AGENT_NAME.to_string(),
                            entered_at: 2,
                            entered_via_handoff_event_id: Some("handoff-existing".to_string()),
                            linked_thread_id: Some("handoff:existing".to_string()),
                        },
                    ],
                    events: Vec::new(),
                    pending_approval_id: None,
                },
            )
            .await;

        let tool_call = ToolCall::with_default_weles_review(
            "tool-handoff-thread-return".to_string(),
            ToolFunction {
                name: "handoff_thread_agent".to_string(),
                arguments: serde_json::json!({
                    "action": "return_handoff",
                    "reason": "Weles completed the governance pass",
                    "summary": "Returning control to Swarog with the review result and next steps.",
                    "requested_by": "agent"
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

        assert!(!result.is_error, "handoff return should succeed: {}", result.content);
        assert!(result.pending_approval.is_none());
        let state = engine
            .thread_handoff_state(thread_id)
            .await
            .expect("handoff state should exist");
        assert_eq!(
            state.active_agent_id,
            crate::agent::agent_identity::MAIN_AGENT_ID
        );
        assert_eq!(state.responder_stack.len(), 1);

        let thread = engine
            .threads
            .read()
            .await
            .get(thread_id)
            .cloned()
            .expect("thread should exist");
        assert_eq!(
            thread.agent_name.as_deref(),
            Some(crate::agent::agent_identity::MAIN_AGENT_NAME)
        );
        let system_event = thread
            .messages
            .iter()
            .find(|message| message.role == crate::agent::types::MessageRole::System)
            .expect("return handoff should append a system event");
        assert!(system_event.content.contains("\"kind\":\"return\""));
        assert!(
            system_event
                .content
                .contains(&format!(
                    "\"to_agent_name\":\"{}\"",
                    crate::agent::agent_identity::MAIN_AGENT_NAME
                )),
            "system event should announce the restored responder"
        );
    }

    #[tokio::test]
    async fn approved_thread_handoff_activation_updates_stack_and_thread_identity() {
        let root = tempdir().expect("tempdir should succeed");
        let manager = SessionManager::new_test(root.path()).await;
        let mut config = AgentConfig::default();
        config.managed_execution.security_level = SecurityLevel::Moderate;
        let engine = AgentEngine::new_test(manager.clone(), config, root.path()).await;
        let (event_tx, _) = broadcast::channel(8);
        let thread_id = "thread-handoff-approved";

        {
            let mut threads = engine.threads.write().await;
            threads.insert(
                thread_id.to_string(),
                crate::agent::types::AgentThread {
                    id: thread_id.to_string(),
                    agent_name: Some(crate::agent::agent_identity::MAIN_AGENT_NAME.to_string()),
                    title: "Approve handoff".to_string(),
                    messages: vec![crate::agent::types::AgentMessage::user(
                        "If needed, let Weles take over after approval.",
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
                crate::agent::ThreadHandoffState {
                    origin_agent_id: crate::agent::agent_identity::MAIN_AGENT_ID.to_string(),
                    active_agent_id: crate::agent::agent_identity::MAIN_AGENT_ID.to_string(),
                    responder_stack: vec![crate::agent::ThreadResponderFrame {
                        agent_id: crate::agent::agent_identity::MAIN_AGENT_ID.to_string(),
                        agent_name: crate::agent::agent_identity::MAIN_AGENT_NAME.to_string(),
                        entered_at: 1,
                        entered_via_handoff_event_id: None,
                        linked_thread_id: None,
                    }],
                    events: Vec::new(),
                    pending_approval_id: None,
                },
            )
            .await;

        let tool_call = ToolCall::with_default_weles_review(
            "tool-handoff-thread-approved".to_string(),
            ToolFunction {
                name: "handoff_thread_agent".to_string(),
                arguments: serde_json::json!({
                    "action": "push_handoff",
                    "target_agent_id": "weles",
                    "reason": "Governance review required",
                    "summary": "Let Weles take over and continue this safety-sensitive thread.",
                    "requested_by": "agent"
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

        let pending = result
            .pending_approval
            .as_ref()
            .expect("agent-initiated handoff should request approval");
        assert!(
            engine
                .handle_task_approval_resolution(
                    &pending.approval_id,
                    amux_protocol::ApprovalDecision::ApproveOnce
                )
                .await,
            "approval resolution should find the synthetic handoff task"
        );

        let state = engine
            .thread_handoff_state(thread_id)
            .await
            .expect("handoff state should exist");
        assert_eq!(state.active_agent_id, crate::agent::agent_identity::WELES_AGENT_ID);
        assert!(state.pending_approval_id.is_none());
        assert_eq!(state.responder_stack.len(), 2);

        let thread = engine
            .threads
            .read()
            .await
            .get(thread_id)
            .cloned()
            .expect("thread should exist");
        assert_eq!(thread.agent_name.as_deref(), Some("Weles"));
        assert!(
            thread
                .messages
                .iter()
                .any(|message| {
                    message.role == crate::agent::types::MessageRole::System
                        && message.content.contains("\"approval_id\":\"")
                }),
            "approval activation should append a structured handoff system event"
        );
    }
