    #[test]
    fn daemon_boxes_large_gateway_hot_path_futures() {
        let root = repo_root();
        let source = [
            root.join("crates/amux-daemon/src/server.rs"),
            root.join("crates/amux-daemon/src/server/post_tests.rs"),
            root.join("crates/amux-daemon/src/server/dispatch_part3.rs"),
        ]
        .into_iter()
        .map(|path| fs::read_to_string(path).expect("read split server source"))
        .collect::<Vec<_>>()
        .join("\n");

        for required in [
            "Box::pin(loop_agent.run_loop(shutdown_rx)).await;",
            "Box::pin(handle_connection(stream, manager, agent, plugin_manager)).await",
            "if let Err(e) = Box::pin(agent.send_message_with_session(",
            "match Box::pin(agent.send_direct_message(",
        ] {
            assert!(
                source.contains(required),
                "server hot path should box oversized future: {required}"
            );
        }
    }

    struct TestConnection {
        framed: Framed<DuplexStream, AmuxCodec>,
        task: JoinHandle<anyhow::Result<()>>,
        root: PathBuf,
        agent: Arc<AgentEngine>,
    }

    impl TestConnection {
        async fn recv(&mut self) -> DaemonMessage {
            timeout(Duration::from_millis(500), self.framed.next())
                .await
                .expect("timed out waiting for daemon message")
                .expect("connection closed")
                .expect("codec failure")
        }

        async fn shutdown(self) {
            let TestConnection {
                framed,
                task,
                root,
                agent: _,
            } = self;
            drop(framed);
            let join = timeout(Duration::from_secs(2), task)
                .await
                .expect("server task did not shut down in time")
                .expect("server task join failed");
            join.expect("server task returned error");
            let _ = std::fs::remove_dir_all(root);
        }
    }

    async fn spawn_test_connection_with_config(agent_config: AgentConfig) -> TestConnection {
        let root = std::env::current_dir()
            .expect("cwd")
            .join("tmp")
            .join(format!(
                "server-whatsapp-link-test-{}",
                uuid::Uuid::new_v4()
            ));
        std::fs::create_dir_all(&root).expect("create test root");

        let history = Arc::new(
            HistoryStore::new_test_store(&root)
                .await
                .expect("create test history"),
        );
        let manager =
            SessionManager::new_with_history(history.clone(), agent_config.pty_channel_capacity);
        let agent =
            AgentEngine::new_with_shared_history(manager.clone(), agent_config, history.clone());
        let plugin_manager = Arc::new(PluginManager::new(history, root.join("plugins")));

        let (client_stream, server_stream) = tokio::io::duplex(128 * 1024);
        let server_task = tokio::spawn(handle_connection(
            server_stream,
            manager,
            agent.clone(),
            plugin_manager,
        ));

        TestConnection {
            framed: Framed::new(client_stream, AmuxCodec),
            task: server_task,
            root,
            agent,
        }
    }

    async fn spawn_test_connection() -> TestConnection {
        spawn_test_connection_with_config(AgentConfig::default()).await
    }

    async fn register_gateway(conn: &mut TestConnection) -> String {
        conn.framed
            .send(ClientMessage::GatewayRegister {
                registration: GatewayRegistration {
                    gateway_id: "gateway-main".to_string(),
                    instance_id: "instance-01".to_string(),
                    protocol_version: GATEWAY_IPC_PROTOCOL_VERSION,
                    supported_platforms: vec!["slack".to_string(), "discord".to_string()],
                    process_id: Some(4242),
                },
            })
            .await
            .expect("send gateway register");
        match conn.recv().await {
            DaemonMessage::GatewayBootstrap { payload } => payload.bootstrap_correlation_id,
            other => panic!("expected GatewayBootstrap, got {other:?}"),
        }
    }

    async fn acknowledge_gateway_bootstrap(conn: &mut TestConnection, correlation_id: String) {
        conn.framed
            .send(ClientMessage::GatewayAck {
                ack: amux_protocol::GatewayAck {
                    correlation_id,
                    accepted: true,
                    detail: Some("bootstrap applied".to_string()),
                },
            })
            .await
            .expect("send gateway ack");
    }

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
                update: amux_protocol::GatewayCursorState {
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
                update: amux_protocol::GatewayHealthState {
                    platform: "slack".to_string(),
                    status: amux_protocol::GatewayConnectionStatus::Connected,
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
                ack: amux_protocol::GatewayAck {
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
                update: amux_protocol::GatewayCursorState {
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
                update: amux_protocol::GatewayThreadBindingState {
                    channel_key: "Slack:C123".to_string(),
                    thread_id: Some("thread-123".to_string()),
                    updated_at_ms: 2222,
                },
            })
            .await
            .expect("send binding update");
        conn.framed
            .send(ClientMessage::GatewayRouteModeUpdate {
                update: amux_protocol::GatewayRouteModeState {
                    channel_key: "Slack:C123".to_string(),
                    route_mode: amux_protocol::GatewayRouteMode::Swarog,
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
        assert!(bindings.iter().any(
            |(channel_key, thread_id)| channel_key == "Slack:C123" && thread_id == "thread-123"
        ));

        let modes = conn
            .agent
            .history
            .list_gateway_route_modes()
            .await
            .expect("list route modes");
        assert!(
            modes
                .iter()
                .any(|(channel_key, route_mode)| channel_key == "Slack:C123"
                    && route_mode == "swarog")
        );

        conn.shutdown().await;
    }

    #[tokio::test]
    async fn gateway_send_results_complete_waiters_and_update_last_response_state() {
        let mut config = AgentConfig::default();
        config.gateway.enabled = true;
        let mut conn = spawn_test_connection_with_config(config).await;
        let correlation_id = register_gateway(&mut conn).await;
        acknowledge_gateway_bootstrap(&mut conn, correlation_id).await;
        conn.agent.init_gateway().await;

        let agent = conn.agent.clone();
        let send_task = tokio::spawn(async move {
            agent
                .request_gateway_send(GatewaySendRequest {
                    correlation_id: "send-1".to_string(),
                    platform: "slack".to_string(),
                    channel_id: "C123".to_string(),
                    thread_id: Some("1712345678.000100".to_string()),
                    content: "hello".to_string(),
                })
                .await
        });

        let request = match conn.recv().await {
            DaemonMessage::GatewaySendRequest { request } => request,
            other => panic!("expected GatewaySendRequest, got {other:?}"),
        };
        assert_eq!(request.correlation_id, "send-1");
        assert_eq!(request.channel_id, "C123");

        conn.framed
            .send(ClientMessage::GatewaySendResult {
                result: amux_protocol::GatewaySendResult {
                    correlation_id: request.correlation_id.clone(),
                    platform: "slack".to_string(),
                    channel_id: "C123".to_string(),
                    requested_channel_id: Some("C123".to_string()),
                    delivery_id: Some("1712345678.000200".to_string()),
                    ok: true,
                    error: None,
                    completed_at_ms: 1234,
                },
            })
            .await
            .expect("send gateway result");

        let result = send_task
            .await
            .expect("join send task")
            .expect("gateway send should complete");
        assert!(result.ok);

        let gw_guard = conn.agent.gateway_state.lock().await;
        let gw = gw_guard.as_ref().expect("gateway state should exist");
        assert!(gw.last_response_at.contains_key("Slack:C123"));
        drop(gw_guard);

        conn.shutdown().await;
    }
