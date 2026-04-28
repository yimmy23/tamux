#[tokio::test]
async fn gateway_register_rejects_incompatible_protocol_version() {
    let mut conn = spawn_test_connection().await;

    conn.framed
        .send(ClientMessage::GatewayRegister {
            registration: GatewayRegistration {
                gateway_id: "gateway-main".to_string(),
                instance_id: "instance-01".to_string(),
                protocol_version: GATEWAY_IPC_PROTOCOL_VERSION + 1,
                supported_platforms: vec!["slack".to_string()],
                process_id: None,
            },
        })
        .await
        .expect("send gateway register");

    match conn.recv().await {
        DaemonMessage::Error { message } => {
            assert!(message.contains("unsupported gateway protocol version"))
        }
        other => panic!("expected Error, got {other:?}"),
    }

    let closed = timeout(Duration::from_millis(250), conn.framed.next()).await;
    assert!(
        matches!(closed, Ok(None)),
        "connection should close after version mismatch"
    );
    conn.shutdown().await;
}

#[tokio::test]
async fn gateway_updates_require_registration_and_bootstrap_ack() {
    let mut conn = spawn_test_connection().await;

    conn.framed
        .send(ClientMessage::GatewayCursorUpdate {
            update: zorai_protocol::GatewayCursorState {
                platform: "slack".to_string(),
                channel_id: "C123".to_string(),
                cursor_value: "1712345678.000100".to_string(),
                cursor_type: "message_ts".to_string(),
                updated_at_ms: 123,
            },
        })
        .await
        .expect("send cursor update before register");
    match conn.recv().await {
        DaemonMessage::Error { message } => {
            assert!(message.contains("gateway cursor updates require"))
        }
        other => panic!("expected Error, got {other:?}"),
    }

    let correlation_id = register_gateway(&mut conn).await;
    conn.framed
        .send(ClientMessage::GatewayHealthUpdate {
            update: zorai_protocol::GatewayHealthState {
                platform: "slack".to_string(),
                status: zorai_protocol::GatewayConnectionStatus::Connected,
                last_success_at_ms: Some(123),
                last_error_at_ms: None,
                consecutive_failure_count: 0,
                last_error: None,
                current_backoff_secs: 0,
            },
        })
        .await
        .expect("send health update before ack");
    match conn.recv().await {
        DaemonMessage::Error { message } => {
            assert!(message.contains("gateway health updates require"))
        }
        other => panic!("expected Error, got {other:?}"),
    }

    conn.framed
        .send(ClientMessage::GatewayAck {
            ack: zorai_protocol::GatewayAck {
                correlation_id: "wrong-token".to_string(),
                accepted: true,
                detail: None,
            },
        })
        .await
        .expect("send wrong ack");
    match conn.recv().await {
        DaemonMessage::Error { message } => {
            assert!(message.contains("invalid gateway bootstrap ack"))
        }
        other => panic!("expected Error, got {other:?}"),
    }

    acknowledge_gateway_bootstrap(&mut conn, correlation_id).await;
    conn.framed
        .send(ClientMessage::Ping)
        .await
        .expect("send ping after ack");
    assert!(matches!(conn.recv().await, DaemonMessage::Pong));

    conn.shutdown().await;
}

#[tokio::test]
async fn gateway_updates_persist_cursor_thread_binding_and_route_mode_after_ack() {
    let mut conn = spawn_test_connection().await;
    let correlation_id = register_gateway(&mut conn).await;
    acknowledge_gateway_bootstrap(&mut conn, correlation_id).await;

    conn.framed
        .send(ClientMessage::GatewayCursorUpdate {
            update: zorai_protocol::GatewayCursorState {
                platform: "slack".to_string(),
                channel_id: "C123".to_string(),
                cursor_value: "1712345678.000100".to_string(),
                cursor_type: "message_ts".to_string(),
                updated_at_ms: 1111,
            },
        })
        .await
        .expect("send cursor update");
    conn.framed
        .send(ClientMessage::GatewayThreadBindingUpdate {
            update: zorai_protocol::GatewayThreadBindingState {
                channel_key: "Slack:C123".to_string(),
                thread_id: Some("thread-123".to_string()),
                updated_at_ms: 2222,
            },
        })
        .await
        .expect("send binding update");
    conn.framed
        .send(ClientMessage::GatewayRouteModeUpdate {
            update: zorai_protocol::GatewayRouteModeState {
                channel_key: "Slack:C123".to_string(),
                route_mode: zorai_protocol::GatewayRouteMode::Swarog,
                updated_at_ms: 3333,
            },
        })
        .await
        .expect("send route mode update");

    conn.framed
        .send(ClientMessage::Ping)
        .await
        .expect("send ping barrier");
    assert!(matches!(conn.recv().await, DaemonMessage::Pong));

    let cursor = conn
        .agent
        .history
        .load_gateway_replay_cursor("slack", "C123")
        .await
        .expect("load cursor")
        .expect("cursor should exist");
    assert_eq!(cursor.cursor_value, "1712345678.000100");
    assert_eq!(cursor.cursor_type, "message_ts");

    let bindings = conn
        .agent
        .history
        .list_gateway_thread_bindings()
        .await
        .expect("list bindings");
    assert!(bindings
        .iter()
        .any(|(channel_key, thread_id)| channel_key == "Slack:C123" && thread_id == "thread-123"));

    let modes = conn
        .agent
        .history
        .list_gateway_route_modes()
        .await
        .expect("list route modes");
    assert!(modes
        .iter()
        .any(|(channel_key, route_mode)| channel_key == "Slack:C123" && route_mode == "swarog"));

    conn.shutdown().await;
}
