use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use zorai_protocol::{
    ClientMessage, DaemonMessage, GatewayBootstrapPayload, GatewayConnectionStatus,
    GatewayContinuityState, GatewayCursorState, GatewayHealthState, GatewayIncomingEvent,
    GatewayProviderBootstrap, GatewayRouteMode, GatewayRouteModeState, GatewaySendRequest,
    GatewayThreadBindingState,
};

use crate::router::GatewayMessage;

use super::GatewayRuntimeCore;

fn sample_bootstrap() -> GatewayBootstrapPayload {
    GatewayBootstrapPayload {
        bootstrap_correlation_id: "bootstrap-1".to_string(),
        feature_flags: vec!["gateway_enabled".to_string()],
        providers: vec![GatewayProviderBootstrap {
            platform: "slack".to_string(),
            enabled: true,
            credentials_json: r#"{"token":"xoxb-test"}"#.to_string(),
            config_json: "{}".to_string(),
        }],
        continuity: GatewayContinuityState {
            cursors: vec![GatewayCursorState {
                platform: "slack".to_string(),
                channel_id: "C123".to_string(),
                cursor_value: "1712345678.000100".to_string(),
                cursor_type: "message_ts".to_string(),
                updated_at_ms: 1234,
            }],
            thread_bindings: vec![GatewayThreadBindingState {
                channel_key: "slack:C123".to_string(),
                thread_id: Some("thread-77".to_string()),
                updated_at_ms: 2222,
            }],
            route_modes: vec![GatewayRouteModeState {
                channel_key: "slack:C123".to_string(),
                route_mode: GatewayRouteMode::Swarog,
                updated_at_ms: 3333,
            }],
            health_snapshots: vec![GatewayHealthState {
                platform: "slack".to_string(),
                status: GatewayConnectionStatus::Connected,
                last_success_at_ms: Some(4444),
                last_error_at_ms: None,
                consecutive_failure_count: 0,
                last_error: None,
                current_backoff_secs: 0,
            }],
        },
    }
}

fn sample_incoming_message() -> GatewayMessage {
    GatewayMessage {
        platform: "slack".to_string(),
        channel_id: "C123".to_string(),
        user_id: "U123".to_string(),
        sender_display: Some("Alice".to_string()),
        text: "hello from provider".to_string(),
        message_id: Some("msg-123".to_string()),
        thread_id: Some("thread-1".to_string()),
        timestamp: 1712345678,
        raw_event_json: Some("{\"text\":\"hello from provider\"}".to_string()),
    }
}

fn sample_send_request() -> GatewaySendRequest {
    GatewaySendRequest {
        correlation_id: "send-1".to_string(),
        platform: "slack".to_string(),
        channel_id: "C123".to_string(),
        thread_id: Some("thread-1".to_string()),
        content: "hello back".to_string(),
    }
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("gateway crate dir")
        .parent()
        .expect("workspace root")
        .to_path_buf()
}

#[tokio::test]
async fn gateway_runtime_bootstraps_from_daemon_message() {
    let (daemon_tx, mut daemon_rx) = tokio::sync::mpsc::unbounded_channel::<ClientMessage>();
    let (provider_tx, _provider_rx) = tokio::sync::mpsc::unbounded_channel::<GatewaySendRequest>();
    let mut runtime = GatewayRuntimeCore::new(daemon_tx, provider_tx);

    runtime
        .bootstrap_from_daemon_message(DaemonMessage::GatewayBootstrap {
            payload: sample_bootstrap(),
        })
        .expect("bootstrap should apply");

    let state = runtime.state();
    assert_eq!(state.bootstrap_correlation_id(), "bootstrap-1");
    assert!(state
        .feature_flags()
        .iter()
        .any(|flag| flag == "gateway_enabled"));
    assert_eq!(
        state.thread_binding("slack:C123"),
        Some("thread-77".to_string())
    );
    assert_eq!(
        state.route_mode("slack:C123"),
        Some(GatewayRouteMode::Swarog)
    );
    assert_eq!(
        state.health_snapshot("slack").map(|value| value.status),
        Some(GatewayConnectionStatus::Connected)
    );
    assert!(daemon_rx.try_recv().is_err());
}

#[tokio::test]
async fn gateway_runtime_routes_incoming_event_to_daemon_channel() {
    let (daemon_tx, mut daemon_rx) = tokio::sync::mpsc::unbounded_channel::<ClientMessage>();
    let (provider_tx, _provider_rx) = tokio::sync::mpsc::unbounded_channel::<GatewaySendRequest>();
    let mut runtime = GatewayRuntimeCore::new(daemon_tx, provider_tx);

    runtime
        .route_incoming_provider_event(sample_incoming_message())
        .expect("incoming event should route");

    match daemon_rx.recv().await {
        Some(ClientMessage::GatewayIncomingEvent { event }) => {
            assert_eq!(
                event,
                GatewayIncomingEvent {
                    platform: "slack".to_string(),
                    channel_id: "C123".to_string(),
                    sender_id: "U123".to_string(),
                    sender_display: Some("Alice".to_string()),
                    content: "hello from provider".to_string(),
                    message_id: Some("msg-123".to_string()),
                    thread_id: Some("thread-1".to_string()),
                    received_at_ms: 1712345678000,
                    raw_event_json: Some("{\"text\":\"hello from provider\"}".to_string()),
                }
            );
        }
        other => panic!("expected GatewayIncomingEvent, got {other:?}"),
    }
}

#[tokio::test]
async fn gateway_runtime_applies_outbound_send_request_to_provider_queue() {
    let (daemon_tx, _daemon_rx) = tokio::sync::mpsc::unbounded_channel::<ClientMessage>();
    let (provider_tx, mut provider_rx) =
        tokio::sync::mpsc::unbounded_channel::<GatewaySendRequest>();
    let mut runtime = GatewayRuntimeCore::new(daemon_tx, provider_tx);

    let request = sample_send_request();
    runtime
        .apply_daemon_message(DaemonMessage::GatewaySendRequest {
            request: request.clone(),
        })
        .expect("send request should route to provider queue");

    let queued = provider_rx.recv().await.expect("request queued");
    assert_eq!(queued, request);
}

#[tokio::test]
async fn gateway_runtime_preserves_requested_channel_on_queue_failure() {
    let (daemon_tx, mut daemon_rx) = tokio::sync::mpsc::unbounded_channel::<ClientMessage>();
    let (provider_tx, _provider_rx) = tokio::sync::mpsc::unbounded_channel::<GatewaySendRequest>();
    let mut runtime = GatewayRuntimeCore::new(daemon_tx, provider_tx);
    let mut provider_senders = std::collections::HashMap::new();

    super::dispatch_send_request(&mut runtime, &mut provider_senders, sample_send_request())
        .expect("send request should emit failure result");

    match daemon_rx.recv().await {
        Some(ClientMessage::GatewaySendResult { result }) => {
            assert!(!result.ok);
            assert_eq!(result.channel_id, "C123");
            assert_eq!(result.requested_channel_id.as_deref(), Some("C123"));
            assert_eq!(result.error.as_deref(), Some("provider queue unavailable"));
        }
        other => panic!("expected GatewaySendResult, got {other:?}"),
    }
}

#[tokio::test]
async fn gateway_runtime_delivers_outbound_response_to_origin_provider() {
    let (daemon_tx, _daemon_rx) = tokio::sync::mpsc::unbounded_channel::<ClientMessage>();
    let (provider_tx, _provider_rx) = tokio::sync::mpsc::unbounded_channel::<GatewaySendRequest>();
    let mut runtime = GatewayRuntimeCore::new(daemon_tx, provider_tx);

    let (slack_tx, mut slack_rx) = tokio::sync::mpsc::unbounded_channel::<GatewaySendRequest>();
    let (discord_tx, mut discord_rx) = tokio::sync::mpsc::unbounded_channel::<GatewaySendRequest>();
    let mut provider_senders = HashMap::new();
    provider_senders.insert("slack".to_string(), slack_tx);
    provider_senders.insert("discord".to_string(), discord_tx);

    let request = GatewaySendRequest {
        correlation_id: "send-discord-1".to_string(),
        platform: "discord".to_string(),
        channel_id: "user:123456789".to_string(),
        thread_id: Some("987654321".to_string()),
        content: "discord reply".to_string(),
    };

    super::dispatch_send_request(&mut runtime, &mut provider_senders, request.clone())
        .expect("send request should dispatch");

    let queued = discord_rx
        .recv()
        .await
        .expect("discord provider request queued");
    assert_eq!(queued, request);
    assert!(
        slack_rx.try_recv().is_err(),
        "slack provider should stay idle"
    );
}

#[test]
fn gateway_process_full_round_trip_uses_single_transport_owner() {
    let daemon_root = repo_root().join("crates/zorai-daemon/src/agent");
    let daemon_gateway_source =
        fs::read_to_string(daemon_root.join("gateway.rs")).expect("read daemon gateway source");
    let daemon_loop_source = fs::read_to_string(daemon_root.join("gateway_loop.rs"))
        .or_else(|_| fs::read_to_string(daemon_root.join("gateway_loop/mod.rs")))
        .expect("read daemon gateway loop source");
    let daemon_loop_production_source = daemon_loop_source
        .split("\n#[cfg(test)]")
        .next()
        .unwrap_or(daemon_loop_source.as_str());
    let slack_source = fs::read_to_string(repo_root().join("crates/zorai-gateway/src/slack.rs"))
        .expect("read slack provider source");
    let discord_source =
        fs::read_to_string(repo_root().join("crates/zorai-gateway/src/discord.rs"))
            .expect("read discord provider source");
    let telegram_source =
        fs::read_to_string(repo_root().join("crates/zorai-gateway/src/telegram.rs"))
            .expect("read telegram provider source");

    for forbidden in [
        "https://slack.com/api",
        "https://discord.com/api/v10",
        "https://api.telegram.org",
        "conversations.history",
        "getUpdates?offset=",
        "users/@me/channels",
    ] {
        assert!(
            !daemon_gateway_source.contains(forbidden)
                && !daemon_loop_production_source.contains(forbidden),
            "daemon still owns transport marker: {forbidden}"
        );
    }

    for (source, required) in [
        (&slack_source, "chat.postMessage"),
        (&discord_source, "/users/@me/channels"),
        (&telegram_source, "sendMessage"),
    ] {
        assert!(
            source.contains(required),
            "gateway transport owner lost required provider implementation marker: {required}"
        );
    }
}

#[tokio::test]
async fn gateway_runtime_emits_live_cursor_thread_binding_and_route_mode_updates() {
    let (daemon_tx, mut daemon_rx) = tokio::sync::mpsc::unbounded_channel::<ClientMessage>();
    let (provider_tx, _provider_rx) = tokio::sync::mpsc::unbounded_channel::<GatewaySendRequest>();
    let mut runtime = GatewayRuntimeCore::new(daemon_tx, provider_tx);

    runtime
        .emit_live_cursor_update(GatewayCursorState {
            platform: "slack".to_string(),
            channel_id: "C123".to_string(),
            cursor_value: "1712345678.000200".to_string(),
            cursor_type: "message_ts".to_string(),
            updated_at_ms: 4444,
        })
        .expect("cursor update should emit");
    runtime
        .emit_live_thread_binding_update(GatewayThreadBindingState {
            channel_key: "slack:C123".to_string(),
            thread_id: Some("thread-88".to_string()),
            updated_at_ms: 5555,
        })
        .expect("thread binding update should emit");
    runtime
        .emit_live_route_mode_update(GatewayRouteModeState {
            channel_key: "slack:C123".to_string(),
            route_mode: GatewayRouteMode::Rarog,
            updated_at_ms: 6666,
        })
        .expect("route mode update should emit");
    runtime
        .emit_health_update(GatewayHealthState {
            platform: "slack".to_string(),
            status: GatewayConnectionStatus::Connected,
            last_success_at_ms: Some(1111),
            last_error_at_ms: None,
            consecutive_failure_count: 0,
            last_error: None,
            current_backoff_secs: 0,
        })
        .expect("health update should emit");

    assert_eq!(
        runtime
            .state()
            .health_snapshot("SLACK")
            .map(|value| value.status),
        Some(GatewayConnectionStatus::Connected)
    );

    assert!(matches!(
        daemon_rx.recv().await,
        Some(ClientMessage::GatewayCursorUpdate { .. })
    ));
    assert!(matches!(
        daemon_rx.recv().await,
        Some(ClientMessage::GatewayThreadBindingUpdate { .. })
    ));
    assert!(matches!(
        daemon_rx.recv().await,
        Some(ClientMessage::GatewayRouteModeUpdate { .. })
    ));
    assert!(matches!(
        daemon_rx.recv().await,
        Some(ClientMessage::GatewayHealthUpdate { .. })
    ));
}
