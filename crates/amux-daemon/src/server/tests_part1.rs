use super::{
    build_gateway_bootstrap_payload, build_session_end_episode_payload,
    concierge_welcome_fingerprint, enqueue_gateway_incoming_event, handle_connection,
    is_expected_disconnect_error, persist_gateway_health_update, StartupReadiness,
};
use crate::agent::types::AgentConfig;
use crate::agent::types::{
    AgentEvent, AgentMessage, AgentThread, ConciergeAction, ConciergeActionType,
    ConciergeDetailLevel,
};
use crate::agent::AgentEngine;
use crate::agent::{StreamCancellationEntry, StreamProgressKind};
use crate::history::HistoryStore;
use crate::plugin::PluginManager;
use crate::session_manager::SessionManager;
use amux_protocol::{
    AmuxCodec, ClientMessage, DaemonMessage, GatewayConnectionStatus, GatewayHealthState,
    GatewayIncomingEvent, GatewayRegistration, GatewaySendRequest, SessionInfo,
    GATEWAY_IPC_PROTOCOL_VERSION,
};
use futures::{SinkExt, StreamExt};
use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::io::DuplexStream;
use tokio::task::JoinHandle;
use tokio::time::{timeout, Duration};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("daemon crate dir")
        .parent()
        .expect("workspace root")
        .to_path_buf()
}
use tokio_util::codec::Framed;

#[test]
fn concierge_welcome_fingerprint_matches_for_identical_events() {
    let event_a = AgentEvent::ConciergeWelcome {
        thread_id: "concierge".to_string(),
        content: "Welcome".to_string(),
        detail_level: ConciergeDetailLevel::ProactiveTriage,
        actions: vec![ConciergeAction {
            label: "Dismiss".to_string(),
            action_type: ConciergeActionType::DismissWelcome,
            thread_id: None,
        }],
    };
    let event_b = event_a.clone();
    assert_eq!(
        concierge_welcome_fingerprint(&event_a),
        concierge_welcome_fingerprint(&event_b)
    );
}

#[test]
fn concierge_welcome_fingerprint_changes_with_action_payload() {
    let event_a = AgentEvent::ConciergeWelcome {
        thread_id: "concierge".to_string(),
        content: "Welcome".to_string(),
        detail_level: ConciergeDetailLevel::ProactiveTriage,
        actions: vec![ConciergeAction {
            label: "Dismiss".to_string(),
            action_type: ConciergeActionType::DismissWelcome,
            thread_id: None,
        }],
    };
    let event_b = AgentEvent::ConciergeWelcome {
        thread_id: "concierge".to_string(),
        content: "Welcome".to_string(),
        detail_level: ConciergeDetailLevel::ProactiveTriage,
        actions: vec![ConciergeAction {
            label: "Continue".to_string(),
            action_type: ConciergeActionType::ContinueSession,
            thread_id: Some("thread-1".to_string()),
        }],
    };
    assert_ne!(
        concierge_welcome_fingerprint(&event_a),
        concierge_welcome_fingerprint(&event_b)
    );
}

#[test]
fn expected_disconnect_error_matches_unexpected_eof() {
    let error: anyhow::Error =
        std::io::Error::new(std::io::ErrorKind::UnexpectedEof, "peer closed").into();
    assert!(is_expected_disconnect_error(&error));
}

#[test]
fn expected_disconnect_error_does_not_match_invalid_data() {
    let error: anyhow::Error =
        std::io::Error::new(std::io::ErrorKind::InvalidData, "bad frame").into();
    assert!(!is_expected_disconnect_error(&error));
}

#[test]
fn gateway_incoming_events_forward_without_thread_subscription() {
    let event = AgentEvent::GatewayIncoming {
        platform: "WhatsApp".to_string(),
        sender: "alice".to_string(),
        content: "hello".to_string(),
        channel: "alice@s.whatsapp.net".to_string(),
    };
    let client_threads = HashSet::new();
    assert!(super::should_forward_agent_event(&event, &client_threads));
}

#[test]
fn oversized_thread_agent_event_downgrades_to_reload_required() {
    let event = AgentEvent::Delta {
        thread_id: "thread-big".to_string(),
        content: "x".repeat(amux_protocol::MAX_IPC_FRAME_SIZE_BYTES + 1024),
    };

    let (event_json, truncated) =
        super::cap_agent_event_for_ipc(&event).expect("event should produce fallback payload");
    assert!(truncated);

    let parsed: serde_json::Value =
        serde_json::from_str(&event_json).expect("parse fallback event json");
    assert_eq!(
        parsed.get("type").and_then(|value| value.as_str()),
        Some("thread_reload_required")
    );
    assert_eq!(
        parsed.get("thread_id").and_then(|value| value.as_str()),
        Some("thread-big")
    );
    assert!(amux_protocol::daemon_message_fits_ipc(
        &DaemonMessage::AgentEvent { event_json }
    ));
}

#[test]
fn oversized_global_agent_event_downgrades_to_workflow_notice() {
    let event = AgentEvent::GatewayIncoming {
        platform: "Slack".to_string(),
        sender: "bot".to_string(),
        content: "x".repeat(amux_protocol::MAX_IPC_FRAME_SIZE_BYTES + 1024),
        channel: "C123".to_string(),
    };

    let (event_json, truncated) =
        super::cap_agent_event_for_ipc(&event).expect("event should produce fallback payload");
    assert!(truncated);

    let parsed: serde_json::Value =
        serde_json::from_str(&event_json).expect("parse fallback event json");
    assert_eq!(
        parsed.get("type").and_then(|value| value.as_str()),
        Some("workflow_notice")
    );
    assert_eq!(
        parsed.get("kind").and_then(|value| value.as_str()),
        Some("oversized-agent-event")
    );
    assert!(amux_protocol::daemon_message_fits_ipc(
        &DaemonMessage::AgentEvent { event_json }
    ));
}

#[test]
fn operator_question_events_forward_for_subscribed_thread() {
    let event = AgentEvent::OperatorQuestion {
        question_id: "oq-1".to_string(),
        content: "Choose one".to_string(),
        options: vec!["A".to_string(), "B".to_string()],
        session_id: Some("session-1".to_string()),
        thread_id: Some("thread-1".to_string()),
    };
    let client_threads = HashSet::from(["thread-1".to_string()]);
    assert!(super::should_forward_agent_event(&event, &client_threads));
}

#[test]
fn participant_suggestion_events_forward_for_subscribed_thread() {
    let event = AgentEvent::ParticipantSuggestion {
        thread_id: "thread-1".to_string(),
        suggestion: crate::agent::ThreadParticipantSuggestion {
            id: "suggestion-1".to_string(),
            target_agent_id: "weles".to_string(),
            target_agent_name: "Weles".to_string(),
            instruction: "Check claim".to_string(),
            suggestion_kind: crate::agent::ThreadParticipantSuggestionKind::PreparedMessage,
            force_send: false,
            status: crate::agent::ThreadParticipantSuggestionStatus::Queued,
            created_at: 10,
            updated_at: 10,
            auto_send_at: None,
            source_message_timestamp: None,
            error: None,
        },
    };
    let client_threads = HashSet::from(["thread-1".to_string()]);
    assert!(super::should_forward_agent_event(&event, &client_threads));

    let (event_json, truncated) =
        super::cap_agent_event_for_ipc(&event).expect("participant suggestion should fit IPC");
    assert!(!truncated);
    let parsed: serde_json::Value =
        serde_json::from_str(&event_json).expect("parse participant suggestion event json");
    assert_eq!(
        parsed.get("type").and_then(|value| value.as_str()),
        Some("participant_suggestion")
    );
    assert_eq!(
        parsed.get("thread_id").and_then(|value| value.as_str()),
        Some("thread-1")
    );
    assert!(amux_protocol::daemon_message_fits_ipc(
        &DaemonMessage::AgentEvent { event_json }
    ));
}

#[test]
fn build_session_end_episode_payload_generates_summary_and_tags() {
    let session_id = uuid::Uuid::nil();
    let info = SessionInfo {
        id: session_id,
        title: Some("Cargo test runner".to_string()),
        cwd: Some("/workspace/cmux-next".to_string()),
        cols: 80,
        rows: 24,
        created_at: 0,
        workspace_id: Some("ws-main".to_string()),
        exit_code: None,
        is_alive: true,
        active_command: Some("cargo test -p tamux-daemon".to_string()),
    };

    let (summary, entities) = build_session_end_episode_payload(
        &session_id.to_string(),
        Some(&info),
        Some("running\n test result: ok. 3 passed"),
    );

    assert!(summary.contains("Cargo test runner ended in cmux-next"));
    assert!(summary.contains("Last output: test result: ok. 3 passed"));
    assert!(entities.contains(&format!("session:{session_id}")));
    assert!(entities.contains(&"workspace:ws-main".to_string()));
    assert!(entities.contains(&"cwd:cmux-next".to_string()));
    assert!(entities.contains(&"command:cargo".to_string()));
    assert!(entities.contains(&"title:cargo-test-runner".to_string()));
}

#[tokio::test]
async fn gateway_bootstrap_uses_persisted_cursor_and_thread_state() {
    let root = std::env::current_dir()
        .expect("cwd")
        .join("tmp")
        .join(format!("server-gateway-bootstrap-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).expect("create test root");

    let history = Arc::new(
        HistoryStore::new_test_store(&root)
            .await
            .expect("create test history"),
    );

    history
        .save_gateway_replay_cursor("slack", "C123", "1712345678.000100", "message_ts")
        .await
        .expect("persist cursor");
    history
        .upsert_gateway_thread_binding("Slack:C123", "thread-123", 1234)
        .await
        .expect("persist binding");
    history
        .upsert_gateway_route_mode("Slack:C123", amux_protocol::AGENT_ID_SWAROG, 2345)
        .await
        .expect("persist route mode");

    let mut config = AgentConfig::default();
    config.gateway.enabled = true;
    config.gateway.slack_token = "slack-token".to_string();
    config.gateway.slack_channel_filter = "C123".to_string();
    config.gateway.command_prefix = "!tamux".to_string();
    config.gateway.gateway_electron_bridges_enabled = true;

    let manager = SessionManager::new_with_history(history.clone(), config.pty_channel_capacity);
    let agent = AgentEngine::new_with_shared_history(manager, config, history.clone());

    let payload = build_gateway_bootstrap_payload(&agent, "boot-test".to_string()).await;

    assert_eq!(payload.bootstrap_correlation_id, "boot-test");
    assert!(payload
        .feature_flags
        .contains(&"gateway_enabled".to_string()));
    assert!(payload
        .feature_flags
        .contains(&"gateway_electron_bridges_enabled".to_string()));
    assert!(payload
        .providers
        .iter()
        .any(|provider| provider.platform == "slack" && provider.enabled));
    assert!(payload
        .continuity
        .cursors
        .iter()
        .any(|cursor| cursor.platform == "slack" && cursor.channel_id == "C123"));
    assert!(payload
        .continuity
        .thread_bindings
        .iter()
        .any(|binding| binding.channel_key == "Slack:C123"
            && binding.thread_id.as_deref() == Some("thread-123")));
    assert!(payload
        .continuity
        .route_modes
        .iter()
        .any(|mode| mode.channel_key == "Slack:C123"
            && matches!(mode.route_mode, amux_protocol::GatewayRouteMode::Swarog)));
    assert!(payload
        .continuity
        .cursors
        .iter()
        .any(|cursor| cursor.platform == "slack" && cursor.channel_id == "C123"));

    let _ = std::fs::remove_dir_all(root);
}

#[tokio::test]
async fn gateway_bootstrap_restores_health_snapshots() {
    let root = std::env::current_dir()
        .expect("cwd")
        .join("tmp")
        .join(format!("server-gateway-health-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).expect("create test root");

    let history = Arc::new(
        HistoryStore::new_test_store(&root)
            .await
            .expect("create test history"),
    );
    history
        .upsert_gateway_health_snapshot(
            &amux_protocol::GatewayHealthState {
                platform: "slack".to_string(),
                status: amux_protocol::GatewayConnectionStatus::Error,
                last_success_at_ms: Some(111),
                last_error_at_ms: Some(222),
                consecutive_failure_count: 3,
                last_error: Some("timeout".to_string()),
                current_backoff_secs: 30,
            },
            333,
        )
        .await
        .expect("persist health snapshot");

    let mut config = AgentConfig::default();
    config.gateway.enabled = true;
    let manager = SessionManager::new_with_history(history.clone(), config.pty_channel_capacity);
    let agent = AgentEngine::new_with_shared_history(manager, config, history.clone());

    agent.init_gateway().await;

    let state_guard = agent.gateway_state.lock().await;
    let state = state_guard.as_ref().expect("gateway state should exist");
    assert_eq!(
        state.slack_health.status,
        crate::agent::RuntimeGatewayConnectionStatus::Error
    );
    assert_eq!(state.slack_health.last_success_at, Some(111));
    assert_eq!(state.slack_health.last_error_at, Some(222));
    assert_eq!(state.slack_health.consecutive_failure_count, 3);
    assert_eq!(state.slack_health.last_error.as_deref(), Some("timeout"));
    assert_eq!(state.slack_health.current_backoff_secs, 30);

    let payload = build_gateway_bootstrap_payload(&agent, "boot-health".to_string()).await;
    assert_eq!(payload.continuity.health_snapshots.len(), 1);
    assert_eq!(payload.continuity.health_snapshots[0].platform, "slack");
    assert_eq!(
        payload.continuity.health_snapshots[0].status,
        GatewayConnectionStatus::Error
    );

    drop(state_guard);
    let _ = std::fs::remove_dir_all(root);
}

#[tokio::test]
async fn gateway_incoming_ipc_event_enqueues_agent_processing_without_poll_loop() {
    let root = std::env::current_dir()
        .expect("cwd")
        .join("tmp")
        .join(format!("server-gateway-ipc-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).expect("create test root");

    let history = Arc::new(
        HistoryStore::new_test_store(&root)
            .await
            .expect("create test history"),
    );
    let mut config = AgentConfig::default();
    config.gateway.enabled = true;
    let manager = SessionManager::new_with_history(history.clone(), config.pty_channel_capacity);
    let agent = AgentEngine::new_with_shared_history(manager, config, history.clone());
    agent.init_gateway().await;

    enqueue_gateway_incoming_event(
        &agent,
        GatewayIncomingEvent {
            platform: "Slack".to_string(),
            channel_id: "C123".to_string(),
            sender_id: "U123".to_string(),
            sender_display: Some("Alice".to_string()),
            content: "hello".to_string(),
            message_id: Some("msg-1".to_string()),
            thread_id: Some("thread-1".to_string()),
            received_at_ms: 1234,
            raw_event_json: None,
        },
    )
    .await
    .expect("enqueue gateway event");

    let queue = agent.gateway_injected_messages.lock().await;
    assert_eq!(queue.len(), 1);
    let queued = queue.front().expect("queued gateway message");
    let thread_context = queued
        .thread_context
        .as_ref()
        .expect("thread context should be preserved");
    assert_eq!(thread_context.slack_thread_ts.as_deref(), Some("thread-1"));
    assert_eq!(thread_context.discord_message_id, None);
    assert_eq!(thread_context.telegram_message_id, None);

    let _ = std::fs::remove_dir_all(root);
}

#[tokio::test]
async fn gateway_health_updates_emit_status_and_digest_events() {
    let root = std::env::current_dir()
        .expect("cwd")
        .join("tmp")
        .join(format!(
            "server-gateway-health-events-{}",
            uuid::Uuid::new_v4()
        ));
    std::fs::create_dir_all(&root).expect("create test root");

    let history = Arc::new(
        HistoryStore::new_test_store(&root)
            .await
            .expect("create test history"),
    );
    let mut config = AgentConfig::default();
    config.gateway.enabled = true;
    let manager = SessionManager::new_with_history(history.clone(), config.pty_channel_capacity);
    let agent = AgentEngine::new_with_shared_history(manager, config, history.clone());
    agent.init_gateway().await;
    let mut events = agent.subscribe();

    persist_gateway_health_update(
        &agent,
        GatewayHealthState {
            platform: "slack".to_string(),
            status: GatewayConnectionStatus::Error,
            last_success_at_ms: Some(100),
            last_error_at_ms: Some(200),
            consecutive_failure_count: 2,
            last_error: Some("timeout".to_string()),
            current_backoff_secs: 30,
        },
    )
    .await
    .expect("persist health update");

    let status_event = timeout(Duration::from_secs(1), async {
        loop {
            match events.recv().await.expect("gateway status event") {
                crate::agent::types::AgentEvent::GatewayStatus {
                    platform,
                    status,
                    last_error,
                    consecutive_failures,
                } => {
                    break (platform, status, last_error, consecutive_failures);
                }
                _ => {}
            }
        }
    })
    .await
    .expect("timed out waiting for gateway status");
    assert_eq!(status_event.0, "Slack");
    assert_eq!(status_event.1, "error");
    assert_eq!(status_event.2.as_deref(), Some("timeout"));
    assert_eq!(status_event.3, Some(2));

    let digest_event = timeout(Duration::from_secs(1), async {
        loop {
            match events.recv().await.expect("heartbeat digest event") {
                crate::agent::types::AgentEvent::HeartbeatDigest {
                    actionable, digest, ..
                } => break (actionable, digest),
                _ => {}
            }
        }
    })
    .await
    .expect("timed out waiting for heartbeat digest");
    assert!(digest_event.0);
    assert!(digest_event.1.contains("Slack disconnected"));

    let audits = agent
        .history
        .list_action_audit(None, None, 20)
        .await
        .expect("list action audits");
    assert!(audits.iter().any(|entry| {
        entry.action_type == "gateway_health_transition" && entry.summary.contains("Slack -> error")
    }));

    let _ = std::fs::remove_dir_all(root);
}
