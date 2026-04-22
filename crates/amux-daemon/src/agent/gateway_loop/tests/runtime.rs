use super::*;
use amux_protocol::AGENT_ID_SWAROG;

#[test]
fn hydrate_initializes_gateway_after_config_load_and_before_thread_restore() {
    let persistence_source =
        fs::read_to_string(repo_root().join("crates/amux-daemon/src/agent/persistence.rs"))
            .expect("read persistence.rs");

    let config_idx = persistence_source
        .find("self.persist_sanitized_config(cfg, collisions).await;")
        .expect("hydrate should persist config before gateway init");
    let init_gateway_idx = persistence_source
        .find("self.init_gateway().await;")
        .expect("hydrate should initialize gateway during startup restore");
    let list_threads_idx = persistence_source
        .find("match self.history.list_threads().await {")
        .expect("hydrate should restore threads");

    assert!(
        config_idx < init_gateway_idx,
        "hydrate should initialize gateway after loading config items"
    );
    assert!(
        init_gateway_idx < list_threads_idx,
        "hydrate should initialize gateway before thread restore work"
    );
}

#[test]
fn background_runtime_splits_maintenance_work_into_separate_loops() {
    let source = gateway_loop_production_source();

    for required in [
        "engine.run_task_dispatch_loop(rx).await",
        "engine.run_gateway_event_drain_loop(rx).await",
        "engine.run_heartbeat_loop(rx).await",
        "engine.run_anticipatory_loop(rx).await",
        "engine.run_watcher_refresh_loop(rx).await",
        "engine.run_gateway_supervision_loop(rx).await",
        "engine.run_stalled_turn_supervision_loop(rx).await",
        "engine.run_quiet_goal_supervision_loop(rx).await",
        "engine.run_subagent_supervision_loop(rx).await",
    ] {
        assert!(
            source.contains(required),
            "background runtime should spawn dedicated worker loop: {required}"
        );
    }

    assert!(
        !source.contains("let mut supervisor_tick = tokio::time::interval"),
        "background runtime should not multiplex maintenance work through a shared supervisor tick"
    );
}

#[tokio::test]
async fn gateway_init_loads_replay_cursors() {
    let root = make_test_root("gateway-init-loads-replay-cursors");
    let manager = SessionManager::new_test(&root).await;

    let mut config = AgentConfig::default();
    config.gateway.enabled = true;
    config.gateway.telegram_token = "telegram-token".to_string();
    config.gateway.slack_token = "slack-token".to_string();
    config.gateway.slack_channel_filter = "C123".to_string();
    config.gateway.discord_token = "discord-token".to_string();
    config.gateway.discord_channel_filter = "D456".to_string();
    config.gateway.whatsapp_token = "whatsapp-token".to_string();

    let engine = AgentEngine::new_test(manager, config, &root).await;

    engine
        .history
        .save_gateway_replay_cursor("telegram", "other", "99", "update_id")
        .await
        .expect("save other telegram cursor");
    engine
        .history
        .save_gateway_replay_cursor("telegram", "global", "42", "update_id")
        .await
        .expect("save telegram cursor");
    engine
        .history
        .save_gateway_replay_cursor("slack", "C123", "1712345678.000100", "message_ts")
        .await
        .expect("save slack cursor");
    engine
        .history
        .save_gateway_replay_cursor("discord", "D456", "998877665544", "message_id")
        .await
        .expect("save discord cursor");
    engine
        .history
        .save_gateway_replay_cursor("whatsapp", "15551234567", "wamid-1", "message_id")
        .await
        .expect("save whatsapp cursor");

    engine.init_gateway().await;

    let state_guard = engine.gateway_state.lock().await;
    let state = state_guard.as_ref().expect("gateway state should exist");
    assert_eq!(state.telegram_replay_cursor, Some(42));
    assert_eq!(
        state
            .slack_replay_cursors
            .get("C123")
            .map(std::string::String::as_str),
        Some("1712345678.000100")
    );
    assert_eq!(
        state
            .discord_replay_cursors
            .get("D456")
            .map(std::string::String::as_str),
        Some("998877665544")
    );
    assert_eq!(
        state
            .whatsapp_replay_cursors
            .get("15551234567")
            .map(std::string::String::as_str),
        Some("wamid-1")
    );
    assert!(state.replay_cycle_active.is_empty());

    drop(state_guard);
    fs::remove_dir_all(&root).expect("cleanup test root");
}

#[tokio::test]
async fn gateway_state_updates_survive_gateway_restart() {
    let root = make_test_root("gateway-state-updates-survive-restart");
    let manager = SessionManager::new_test(&root).await;

    let mut config = AgentConfig::default();
    config.gateway.enabled = true;
    let engine = AgentEngine::new_test(manager, config, &root).await;

    engine
        .history
        .save_gateway_replay_cursor("slack", "C123", "1712345678.000100", "message_ts")
        .await
        .expect("save slack cursor");
    engine
        .history
        .upsert_gateway_thread_binding("Slack:C123", "thread-123", 1111)
        .await
        .expect("save thread binding");
    engine
        .history
        .upsert_gateway_route_mode("Slack:C123", AGENT_ID_SWAROG, 2222)
        .await
        .expect("save route mode");

    engine.init_gateway().await;
    *engine.gateway_state.lock().await = None;
    engine.init_gateway().await;

    let state_guard = engine.gateway_state.lock().await;
    let state = state_guard.as_ref().expect("gateway state should exist");
    assert_eq!(
        state
            .slack_replay_cursors
            .get("C123")
            .map(std::string::String::as_str),
        Some("1712345678.000100")
    );
    drop(state_guard);

    let bindings = engine
        .history
        .list_gateway_thread_bindings()
        .await
        .expect("list thread bindings");
    assert!(bindings
        .iter()
        .any(|(channel_key, thread_id)| channel_key == "Slack:C123" && thread_id == "thread-123"));

    let modes = engine
        .history
        .list_gateway_route_modes()
        .await
        .expect("list route modes");
    assert!(modes
        .iter()
        .any(|(channel_key, route_mode)| channel_key == "Slack:C123"
            && route_mode == AGENT_ID_SWAROG));

    fs::remove_dir_all(&root).expect("cleanup test root");
}

#[tokio::test]
async fn gateway_health_snapshots_survive_gateway_restart() {
    let root = make_test_root("gateway-health-snapshots-survive-restart");
    let manager = SessionManager::new_test(&root).await;

    let mut config = AgentConfig::default();
    config.gateway.enabled = true;
    let engine = AgentEngine::new_test(manager, config, &root).await;

    let snapshot = amux_protocol::GatewayHealthState {
        platform: "slack".to_string(),
        status: amux_protocol::GatewayConnectionStatus::Error,
        last_success_at_ms: Some(111),
        last_error_at_ms: Some(222),
        consecutive_failure_count: 3,
        last_error: Some("timeout".to_string()),
        current_backoff_secs: 30,
    };
    engine
        .history
        .upsert_gateway_health_snapshot(&snapshot, 333)
        .await
        .expect("save health snapshot");

    engine.init_gateway().await;
    *engine.gateway_state.lock().await = None;
    engine.init_gateway().await;

    let state_guard = engine.gateway_state.lock().await;
    let state = state_guard.as_ref().expect("gateway state should exist");
    assert_eq!(state.slack_health.status, GatewayConnectionStatus::Error);
    assert_eq!(state.slack_health.last_success_at, Some(111));
    assert_eq!(state.slack_health.last_error_at, Some(222));
    assert_eq!(state.slack_health.consecutive_failure_count, 3);
    assert_eq!(state.slack_health.last_error.as_deref(), Some("timeout"));
    assert_eq!(state.slack_health.current_backoff_secs, 30);

    fs::remove_dir_all(&root).expect("cleanup test root");
}

#[tokio::test]
async fn gateway_auto_send_thread_response_emits_gateway_request_for_latest_assistant_message() {
    let root = make_test_root("gateway-auto-send-thread-response");
    let manager = SessionManager::new_test(&root).await;
    let mut config = AgentConfig::default();
    config.gateway.enabled = true;
    let engine = AgentEngine::new_test(manager, config, &root).await;
    engine.init_gateway().await;

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    engine.set_gateway_ipc_sender(Some(tx)).await;

    let incoming = gateway::IncomingMessage {
        platform: "Discord".to_string(),
        sender: "alice".to_string(),
        content: "Can you check the release notes?".to_string(),
        channel: "user:123456789".to_string(),
        message_id: Some("discord-incoming-1".to_string()),
        thread_context: None,
    };

    let thread_id = engine
        .persist_gateway_fast_path_exchange("Discord:user:123456789", &incoming, "Initial reply")
        .await
        .expect("persist fast-path exchange");

    {
        let mut threads = engine.threads.write().await;
        let thread = threads
            .get_mut(&thread_id)
            .expect("gateway thread should exist");
        thread.messages.push(AgentMessage {
            id: "assistant-latest".to_string(),
            role: MessageRole::Assistant,
            content: "Intermittent update from the agent".to_string(),
            content_blocks: Vec::new(),
            tool_calls: None,
            tool_call_id: None,
            tool_name: None,
            tool_arguments: None,
            tool_status: None,
            weles_review: None,
            input_tokens: 0,
            output_tokens: 0,
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
            timestamp: now_millis(),
        });
        thread.updated_at = now_millis();
    }
    engine.persist_thread_by_id(&thread_id).await;

    let helper_engine = engine.clone();
    let helper_thread_id = thread_id.clone();
    let send_task = tokio::spawn(async move {
        helper_engine
            .maybe_auto_send_gateway_thread_response(&helper_thread_id)
            .await;
    });

    let request = match tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv())
        .await
        .expect("gateway send request should be emitted")
        .expect("gateway send request should exist")
    {
        amux_protocol::DaemonMessage::GatewaySendRequest { request } => request,
        other => panic!("expected GatewaySendRequest, got {other:?}"),
    };
    assert_eq!(request.platform, "discord");
    assert_eq!(request.channel_id, "user:123456789");
    assert_eq!(request.content, "Intermittent update from the agent");

    engine
        .complete_gateway_send_result(amux_protocol::GatewaySendResult {
            correlation_id: request.correlation_id.clone(),
            platform: "discord".to_string(),
            channel_id: "user:123456789".to_string(),
            requested_channel_id: Some("user:123456789".to_string()),
            delivery_id: Some("delivery-1".to_string()),
            ok: true,
            error: None,
            completed_at_ms: now_millis(),
        })
        .await;

    send_task.await.expect("auto-send task should join");
    fs::remove_dir_all(&root).expect("cleanup test root");
}

#[cfg(unix)]
#[tokio::test]
async fn daemon_respawns_gateway_process_when_enabled() {
    let root = make_test_root("daemon-respawns-gateway-process");
    let manager = SessionManager::new_test(&root).await;
    let mut config = AgentConfig::default();
    config.gateway.enabled = true;
    let engine = AgentEngine::new_test(manager, config, &root).await;
    let gateway_path = make_gateway_test_binary(&root, "tamux-gateway-test");

    engine
        .maybe_spawn_gateway_with_path(&gateway_path)
        .await
        .expect("spawn gateway");

    let proc_guard = engine.gateway_process.lock().await;
    assert!(proc_guard.is_some(), "gateway process should be running");
    drop(proc_guard);

    let attempts = engine.gateway_restart_attempts().await;
    assert_eq!(attempts, 0);

    engine.stop_gateway().await;
    let _ = fs::remove_dir_all(&root);
}

#[cfg(unix)]
#[tokio::test]
async fn daemon_gateway_restart_backoff_applies_after_ipc_loss() {
    let root = make_test_root("daemon-gateway-restart-backoff");
    let manager = SessionManager::new_test(&root).await;
    let mut config = AgentConfig::default();
    config.gateway.enabled = true;
    let engine = AgentEngine::new_test(manager, config, &root).await;
    let gateway_path = make_gateway_test_binary(&root, "tamux-gateway-test");

    engine
        .maybe_spawn_gateway_with_path(&gateway_path)
        .await
        .expect("spawn gateway");
    engine.record_gateway_ipc_loss("ipc lost").await;

    assert!(engine.gateway_process.lock().await.is_none());
    let deadline = engine.gateway_restart_not_before_ms().await;
    assert!(deadline.is_some(), "restart deadline should be scheduled");

    engine.stop_gateway().await;
    let _ = fs::remove_dir_all(&root);
}

#[cfg(unix)]
#[tokio::test]
async fn daemon_gateway_reload_requests_clean_restart() {
    let root = make_test_root("daemon-gateway-reload");
    let manager = SessionManager::new_test(&root).await;
    let mut config = AgentConfig::default();
    config.gateway.enabled = true;
    let engine = AgentEngine::new_test(manager, config, &root).await;
    let gateway_path = make_gateway_test_binary(&root, "tamux-gateway-test");
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    engine.set_gateway_ipc_sender(Some(tx)).await;

    engine
        .reinit_gateway_with_path(&gateway_path)
        .await
        .expect("reinit gateway");

    let msg = rx.recv().await.expect("expected reload command");
    assert!(matches!(
        msg,
        amux_protocol::DaemonMessage::GatewayReloadCommand { .. }
    ));
    assert!(engine.gateway_process.lock().await.is_some());

    engine.stop_gateway().await;
    let _ = fs::remove_dir_all(&root);
}

#[cfg(unix)]
#[tokio::test]
async fn gateway_config_reload_uses_spawn_restart_path_not_init_gateway() {
    let root = make_test_root("gateway-config-reload");
    let manager = SessionManager::new_test(&root).await;
    let mut config = AgentConfig::default();
    config.gateway.enabled = true;
    config.gateway.slack_token = "slack-token".to_string();
    let engine = AgentEngine::new_test(manager, config, &root).await;
    let gateway_path = make_gateway_test_binary(&root, "tamux-gateway-test");
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    engine.set_gateway_ipc_sender(Some(tx)).await;

    engine
        .reinit_gateway_with_path(&gateway_path)
        .await
        .expect("reinit gateway");

    let msg = rx.recv().await.expect("expected reload command");
    assert!(matches!(
        msg,
        amux_protocol::DaemonMessage::GatewayReloadCommand { .. }
    ));
    assert!(engine.gateway_state.lock().await.is_some());
    assert!(engine.gateway_process.lock().await.is_some());

    engine.stop_gateway().await;
    let _ = fs::remove_dir_all(&root);
}

#[tokio::test]
async fn gateway_fast_path_reply_creates_thread_binding_and_history() {
    let root = make_test_root("gateway-fast-path-persists-history");
    let manager = SessionManager::new_test(&root).await;
    let mut config = AgentConfig::default();
    config.gateway.enabled = true;
    let engine = AgentEngine::new_test(manager, config, &root).await;
    let msg = super::gateway::IncomingMessage {
        platform: "Discord".into(),
        sender: "alice".into(),
        content: "switch to svarog".into(),
        channel: "D123".into(),
        message_id: Some("discord:msg-1".into()),
        thread_context: Some(super::gateway::ThreadContext {
            discord_message_id: Some("discord:msg-1".into()),
            ..Default::default()
        }),
    };
    let thread_id = engine
        .persist_gateway_fast_path_exchange(
            "Discord:D123",
            &msg,
            &gateway_route_confirmation(gateway::GatewayRouteMode::Swarog),
        )
        .await
        .expect("persist fast-path exchange");

    let messages = engine
        .history
        .list_recent_messages(&thread_id, 10)
        .await
        .expect("list thread messages");
    assert!(messages
        .iter()
        .any(|msg| msg.role == "user" && msg.content == "switch to svarog"));
    assert!(messages.iter().any(|msg| {
        msg.role == "assistant"
            && msg.content == gateway_route_confirmation(gateway::GatewayRouteMode::Swarog)
    }));
    fs::remove_dir_all(&root).expect("cleanup test root");
}
