use amux_protocol::{
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
