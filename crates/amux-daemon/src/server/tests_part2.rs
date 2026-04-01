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
        plugin_manager: Arc<PluginManager>,
    }

    impl TestConnection {
        async fn recv(&mut self) -> DaemonMessage {
            self.recv_with_timeout(Duration::from_millis(500)).await
        }

        async fn recv_with_timeout(&mut self, duration: Duration) -> DaemonMessage {
            timeout(duration, self.framed.next())
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
                plugin_manager: _,
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
            plugin_manager.clone(),
        ));

        TestConnection {
            framed: Framed::new(client_stream, AmuxCodec),
            task: server_task,
            root,
            agent,
            plugin_manager,
        }
    }

    async fn spawn_test_connection() -> TestConnection {
        spawn_test_connection_with_config(AgentConfig::default()).await
    }

    async fn declare_async_command_capability(conn: &mut TestConnection) {
        conn.framed
            .send(ClientMessage::AgentDeclareAsyncCommandCapability {
                capability: amux_protocol::AsyncCommandCapability {
                    version: 1,
                    supports_operation_acceptance: true,
                },
            })
            .await
            .expect("declare async command capability");

        match conn.recv().await {
            DaemonMessage::AgentAsyncCommandCapabilityAck { capability } => {
                assert_eq!(capability.version, 1);
                assert!(capability.supports_operation_acceptance);
            }
            other => panic!("expected async command capability ack, got {other:?}"),
        }
    }

    async fn register_test_oauth_plugin(conn: &TestConnection, name: &str) {
        let plugin_dir = conn.root.join("plugins").join(name);
        std::fs::create_dir_all(&plugin_dir).expect("create oauth plugin dir");
        let manifest = serde_json::json!({
            "name": name,
            "version": "1.0.0",
            "schema_version": 1,
            "settings": {
                "client_id": {
                    "type": "string",
                    "label": "Client ID",
                    "required": true
                }
            },
            "auth": {
                "type": "oauth2",
                "authorization_url": "https://example.com/oauth/authorize",
                "token_url": "http://127.0.0.1:9/token",
                "pkce": true
            }
        });
        std::fs::write(
            plugin_dir.join("plugin.json"),
            serde_json::to_vec_pretty(&manifest).expect("serialize oauth plugin manifest"),
        )
        .expect("write oauth plugin manifest");

        conn.plugin_manager
            .register_plugin(name, "test")
            .await
            .expect("register oauth plugin");
        conn.plugin_manager
            .update_setting(name, "client_id", "test-client", false)
            .await
            .expect("configure oauth plugin client id");
    }

    async fn register_test_api_plugin(conn: &TestConnection, name: &str) {
        let plugin_dir = conn.root.join("plugins").join(name);
        std::fs::create_dir_all(&plugin_dir).expect("create api plugin dir");
        let manifest = serde_json::json!({
            "name": name,
            "version": "1.0.0",
            "schema_version": 1,
            "api": {
                "base_url": "https://example.com",
                "endpoints": {
                    "slow": {
                        "method": "GET",
                        "path": "/slow"
                    }
                }
            }
        });
        std::fs::write(
            plugin_dir.join("plugin.json"),
            serde_json::to_vec_pretty(&manifest).expect("serialize api plugin manifest"),
        )
        .expect("write api plugin manifest");

        conn.plugin_manager
            .register_plugin(name, "test")
            .await
            .expect("register api plugin");
    }

    async fn register_publishable_skill_variant(
        conn: &TestConnection,
        skill_name: &str,
        status: &str,
    ) -> String {
        let skill_dir = conn
            .agent
            .history
            .data_dir()
            .join("skills")
            .join("community")
            .join(skill_name);
        std::fs::create_dir_all(&skill_dir).expect("create publishable skill dir");
        let skill_path = skill_dir.join("SKILL.md");
        std::fs::write(
            &skill_path,
            format!("---\nname: {skill_name}\ndescription: test skill\n---\nBody"),
        )
        .expect("write publishable skill");

        let record = conn
            .agent
            .history
            .register_skill_document(&skill_path)
            .await
            .expect("register publishable skill");
        conn.agent
            .history
            .update_skill_variant_status(&record.variant_id, status)
            .await
            .expect("set skill publish status");
        record.variant_id
    }

    async fn spawn_test_registry_publish_server(delay: Duration) -> (String, JoinHandle<()>) {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind test registry listener");
        let addr = listener.local_addr().expect("test registry local addr");
        let task = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.expect("accept registry publish");
            let mut request = Vec::new();
            let mut buf = [0u8; 4096];
            let header_end = loop {
                let n = stream.read(&mut buf).await.expect("read registry request");
                assert!(n > 0, "registry request closed before headers completed");
                request.extend_from_slice(&buf[..n]);
                if let Some(end) = request.windows(4).position(|w| w == b"\r\n\r\n") {
                    break end + 4;
                }
            };
            let header_text = String::from_utf8_lossy(&request[..header_end]);
            let content_length = header_text
                .lines()
                .find_map(|line| {
                    let (name, value) = line.split_once(':')?;
                    if name.eq_ignore_ascii_case("content-length") {
                        value.trim().parse::<usize>().ok()
                    } else {
                        None
                    }
                })
                .unwrap_or(0);
            while request.len().saturating_sub(header_end) < content_length {
                let n = stream.read(&mut buf).await.expect("read registry body");
                assert!(n > 0, "registry request closed before body completed");
                request.extend_from_slice(&buf[..n]);
            }
            tokio::time::sleep(delay).await;
            stream
                .write_all(
                    b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok",
                )
                .await
                .expect("write registry response");
        });

        (format!("http://{}", addr), task)
    }

    fn build_test_skill_archive(skill_name: &str) -> Vec<u8> {
        use std::io::Cursor;

        let content = format!(
            "---\nname: {skill_name}\ndescription: imported test skill\nallowed_tools:\n  - read_file\n---\nImported body"
        );
        let encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        let mut builder = tar::Builder::new(encoder);
        let bytes = content.into_bytes();
        let mut header = tar::Header::new_gnu();
        header.set_mode(0o644);
        header.set_size(bytes.len() as u64);
        header.set_cksum();
        builder
            .append_data(&mut header, "SKILL.md", Cursor::new(bytes))
            .expect("append skill file to archive");
        let encoder = builder.into_inner().expect("finish tar builder");
        encoder.finish().expect("finish skill archive")
    }

    async fn spawn_test_registry_fetch_server(
        skill_name: &str,
        delay: Duration,
    ) -> (String, JoinHandle<()>) {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let archive = build_test_skill_archive(skill_name);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind test skill registry listener");
        let addr = listener.local_addr().expect("test skill registry local addr");
        let expected_path = format!("/skills/{skill_name}.tar.gz");
        let task = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.expect("accept registry fetch");
            let mut request = Vec::new();
            let mut buf = [0u8; 4096];
            loop {
                let n = stream.read(&mut buf).await.expect("read registry fetch request");
                assert!(n > 0, "registry fetch request closed before headers completed");
                request.extend_from_slice(&buf[..n]);
                if request.windows(4).any(|w| w == b"\r\n\r\n") {
                    break;
                }
            }
            let header_text = String::from_utf8_lossy(&request);
            let request_line = header_text.lines().next().expect("registry fetch request line");
            assert!(
                request_line.contains(&expected_path),
                "expected registry fetch path {expected_path}, got {request_line}"
            );

            tokio::time::sleep(delay).await;
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: application/gzip\r\nConnection: close\r\n\r\n",
                archive.len()
            );
            stream
                .write_all(response.as_bytes())
                .await
                .expect("write registry fetch headers");
            stream
                .write_all(&archive)
                .await
                .expect("write registry fetch archive");
        });

        (format!("http://{}", addr), task)
    }

    async fn complete_test_oauth_callback(auth_url: &str) {
        let parsed = url::Url::parse(auth_url).expect("parse auth url");
        let redirect_uri = parsed
            .query_pairs()
            .find(|(key, _)| key == "redirect_uri")
            .map(|(_, value)| value.into_owned())
            .expect("redirect_uri query parameter");
        let state = parsed
            .query_pairs()
            .find(|(key, _)| key == "state")
            .map(|(_, value)| value.into_owned())
            .expect("state query parameter");
        let redirect = url::Url::parse(&redirect_uri).expect("parse redirect uri");
        let host = redirect.host_str().expect("redirect host");
        let port = redirect.port_or_known_default().expect("redirect port");
        let path = redirect.path();

        let mut stream = tokio::net::TcpStream::connect((host, port))
            .await
            .expect("connect oauth callback listener");
        let request = format!(
            "GET {path}?code=test-auth-code&state={state} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n"
        );
        use tokio::io::AsyncWriteExt;
        stream
            .write_all(request.as_bytes())
            .await
            .expect("write oauth callback request");
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
    async fn concierge_welcome_request_does_not_block_ping() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind fake llm listener");
        let addr = listener.local_addr().expect("listener addr");
        let accept_task = tokio::spawn(async move {
            let (_stream, _) = listener.accept().await.expect("accept concierge request");
            tokio::time::sleep(Duration::from_secs(5)).await;
        });

        let mut config = AgentConfig::default();
        config.provider = "openai".to_string();
        config.base_url = format!("http://{addr}");
        config.api_key = "test-key".to_string();
        config.model = "gpt-5.4".to_string();
        config.concierge.detail_level = crate::agent::types::ConciergeDetailLevel::ContextSummary;

        let mut conn = spawn_test_connection_with_config(config).await;

        conn.framed
            .send(ClientMessage::AgentSubscribe)
            .await
            .expect("subscribe to agent events");
        conn.framed
            .send(ClientMessage::AgentRequestConciergeWelcome)
            .await
            .expect("request concierge welcome");
        conn.framed
            .send(ClientMessage::Ping)
            .await
            .expect("send ping while concierge work is active");

        let pong_received = timeout(Duration::from_millis(250), async {
            loop {
                match conn.recv().await {
                    DaemonMessage::Pong => return true,
                    DaemonMessage::AgentEvent { .. } => continue,
                    other => {
                        panic!(
                            "expected Pong while concierge work runs in background, got {other:?}"
                        )
                    }
                }
            }
        })
        .await
        .unwrap_or(false);

        assert!(
            pong_received,
            "ping should not be blocked behind concierge welcome generation"
        );

        accept_task.abort();
        conn.shutdown().await;
    }

    #[tokio::test]
    async fn operation_status_query_returns_current_snapshot() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind fake llm listener");
        let addr = listener.local_addr().expect("listener addr");
        let accept_task = tokio::spawn(async move {
            let (_stream, _) = listener.accept().await.expect("accept concierge request");
            tokio::time::sleep(Duration::from_secs(5)).await;
        });

        let mut config = AgentConfig::default();
        config.provider = "openai".to_string();
        config.base_url = format!("http://{addr}");
        config.api_key = "test-key".to_string();
        config.model = "gpt-5.4".to_string();
        config.concierge.detail_level = crate::agent::types::ConciergeDetailLevel::ContextSummary;

        let mut conn = spawn_test_connection_with_config(config).await;
        declare_async_command_capability(&mut conn).await;

        conn.framed
            .send(ClientMessage::AgentRequestConciergeWelcome)
            .await
            .expect("request concierge welcome");

        let operation_id = match conn.recv().await {
            DaemonMessage::OperationAccepted { operation_id, kind, .. } => {
                assert_eq!(kind, "concierge_welcome");
                operation_id
            }
            other => panic!("expected operation acceptance, got {other:?}"),
        };

        conn.framed
            .send(ClientMessage::AgentGetOperationStatus {
                operation_id: operation_id.clone(),
            })
            .await
            .expect("query operation status");

        match conn.recv().await {
            DaemonMessage::OperationStatus { snapshot } => {
                assert_eq!(snapshot.operation_id, operation_id);
                assert_eq!(snapshot.kind, "concierge_welcome");
                assert!(matches!(
                    snapshot.state,
                    amux_protocol::OperationLifecycleState::Accepted
                        | amux_protocol::OperationLifecycleState::Started
                ));
            }
            other => panic!("expected operation status snapshot, got {other:?}"),
        }

        accept_task.abort();
        conn.shutdown().await;
    }

    #[tokio::test]
    async fn operation_status_query_survives_reconnect() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind fake llm listener");
        let addr = listener.local_addr().expect("listener addr");
        let accept_task = tokio::spawn(async move {
            let (_stream, _) = listener.accept().await.expect("accept concierge request");
            tokio::time::sleep(Duration::from_secs(5)).await;
        });

        let mut config = AgentConfig::default();
        config.provider = "openai".to_string();
        config.base_url = format!("http://{addr}");
        config.api_key = "test-key".to_string();
        config.model = "gpt-5.4".to_string();
        config.concierge.detail_level = crate::agent::types::ConciergeDetailLevel::ContextSummary;

        let mut conn = spawn_test_connection_with_config(config.clone()).await;
        declare_async_command_capability(&mut conn).await;
        conn.framed
            .send(ClientMessage::AgentRequestConciergeWelcome)
            .await
            .expect("request concierge welcome");

        let operation_id = match conn.recv().await {
            DaemonMessage::OperationAccepted { operation_id, .. } => operation_id,
            other => panic!("expected operation acceptance, got {other:?}"),
        };

        let mut reconnect = spawn_test_connection_with_config(config).await;
        declare_async_command_capability(&mut reconnect).await;
        reconnect
            .framed
            .send(ClientMessage::AgentGetOperationStatus {
                operation_id: operation_id.clone(),
            })
            .await
            .expect("query operation status after reconnect");

        match reconnect.recv().await {
            DaemonMessage::OperationStatus { snapshot } => {
                assert_eq!(snapshot.operation_id, operation_id);
                assert_eq!(snapshot.kind, "concierge_welcome");
            }
            other => panic!("expected operation status snapshot after reconnect, got {other:?}"),
        }

        accept_task.abort();
        reconnect.shutdown().await;
        conn.shutdown().await;
    }

    #[tokio::test]
    async fn concierge_welcome_does_not_delay_provider_validation_acceptance() {
        let concierge_listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind fake concierge listener");
        let concierge_addr = concierge_listener.local_addr().expect("concierge listener addr");
        let concierge_accept_task = tokio::spawn(async move {
            let (_stream, _) = concierge_listener
                .accept()
                .await
                .expect("accept concierge request");
            tokio::time::sleep(Duration::from_secs(5)).await;
        });

        let provider_listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind fake provider listener");
        let provider_addr = provider_listener.local_addr().expect("provider listener addr");
        let provider_accept_task = tokio::spawn(async move {
            let (_stream, _) = provider_listener
                .accept()
                .await
                .expect("accept provider request");
            tokio::time::sleep(Duration::from_secs(5)).await;
        });

        let mut config = AgentConfig::default();
        config.provider = "openai".to_string();
        config.base_url = format!("http://{concierge_addr}");
        config.api_key = "test-key".to_string();
        config.model = "gpt-5.4".to_string();
        config.concierge.detail_level = crate::agent::types::ConciergeDetailLevel::ContextSummary;

        let mut conn = spawn_test_connection_with_config(config).await;
        declare_async_command_capability(&mut conn).await;

        conn.framed
            .send(ClientMessage::AgentSubscribe)
            .await
            .expect("subscribe to agent events");
        conn.framed
            .send(ClientMessage::AgentRequestConciergeWelcome)
            .await
            .expect("request concierge welcome");

        let concierge_accepted = timeout(Duration::from_secs(2), async {
            loop {
                match conn.recv_with_timeout(Duration::from_secs(2)).await {
                    DaemonMessage::OperationAccepted { kind, .. } if kind == "concierge_welcome" => {
                        return true;
                    }
                    DaemonMessage::AgentEvent { .. } => continue,
                    other => panic!("expected concierge acceptance, got {other:?}"),
                }
            }
        })
        .await
        .unwrap_or(false);

        assert!(concierge_accepted, "concierge welcome should be accepted");

        conn.framed
            .send(ClientMessage::AgentValidateProvider {
                provider_id: "openai".to_string(),
                base_url: format!("http://{provider_addr}"),
                api_key: "test-key".to_string(),
                auth_source: "api_key".to_string(),
            })
            .await
            .expect("request provider validation while concierge is active");

        let provider_accepted = timeout(Duration::from_millis(500), async {
            loop {
                match conn.recv_with_timeout(Duration::from_millis(500)).await {
                    DaemonMessage::OperationAccepted { kind, .. } if kind == "provider_validation" => {
                        return true;
                    }
                    DaemonMessage::AgentEvent { .. } => continue,
                    other => panic!("expected provider validation acceptance while concierge is active, got {other:?}"),
                }
            }
        })
        .await
        .unwrap_or(false);

        assert!(provider_accepted, "provider validation should be accepted while concierge is active");

        conn.framed
            .send(ClientMessage::Ping)
            .await
            .expect("send ping while concierge and provider work are active");

        let pong_received = timeout(Duration::from_millis(500), async {
            loop {
                match conn.recv_with_timeout(Duration::from_millis(500)).await {
                    DaemonMessage::Pong => return true,
                    DaemonMessage::AgentEvent { .. } => continue,
                    other => panic!("expected Pong while concierge and provider work are active, got {other:?}"),
                }
            }
        })
        .await
        .unwrap_or(false);

        assert!(pong_received, "ping should not be blocked while concierge and provider work are active");

        concierge_accept_task.abort();
        provider_accept_task.abort();
        conn.shutdown().await;
    }

    #[tokio::test]
    async fn concierge_welcome_does_not_delay_fetch_models_acceptance() {
        let concierge_listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind fake concierge listener");
        let concierge_addr = concierge_listener.local_addr().expect("concierge listener addr");
        let concierge_accept_task = tokio::spawn(async move {
            let (_stream, _) = concierge_listener
                .accept()
                .await
                .expect("accept concierge request");
            tokio::time::sleep(Duration::from_secs(5)).await;
        });

        let models_listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind fake models listener");
        let models_addr = models_listener.local_addr().expect("models listener addr");
        let models_accept_task = tokio::spawn(async move {
            let (_stream, _) = models_listener
                .accept()
                .await
                .expect("accept models request");
            tokio::time::sleep(Duration::from_secs(5)).await;
        });

        let mut config = AgentConfig::default();
        config.provider = "openai".to_string();
        config.base_url = format!("http://{concierge_addr}");
        config.api_key = "test-key".to_string();
        config.model = "gpt-5.4".to_string();
        config.concierge.detail_level = crate::agent::types::ConciergeDetailLevel::ContextSummary;

        let mut conn = spawn_test_connection_with_config(config).await;
        declare_async_command_capability(&mut conn).await;

        conn.framed
            .send(ClientMessage::AgentSubscribe)
            .await
            .expect("subscribe to agent events");
        conn.framed
            .send(ClientMessage::AgentRequestConciergeWelcome)
            .await
            .expect("request concierge welcome");

        let concierge_accepted = timeout(Duration::from_secs(2), async {
            loop {
                match conn.recv_with_timeout(Duration::from_secs(2)).await {
                    DaemonMessage::OperationAccepted { kind, .. } if kind == "concierge_welcome" => {
                        return true;
                    }
                    DaemonMessage::AgentEvent { .. } => continue,
                    other => panic!("expected concierge acceptance, got {other:?}"),
                }
            }
        })
        .await
        .unwrap_or(false);

        assert!(concierge_accepted, "concierge welcome should be accepted");

        conn.framed
            .send(ClientMessage::AgentFetchModels {
                provider_id: "openai".to_string(),
                base_url: format!("http://{models_addr}"),
                api_key: "test-key".to_string(),
            })
            .await
            .expect("request fetch-models while concierge is active");

        let models_accepted = timeout(Duration::from_millis(500), async {
            loop {
                match conn.recv_with_timeout(Duration::from_millis(500)).await {
                    DaemonMessage::OperationAccepted { kind, .. } if kind == "fetch_models" => {
                        return true;
                    }
                    DaemonMessage::AgentEvent { .. } => continue,
                    other => panic!("expected fetch-models acceptance while concierge is active, got {other:?}"),
                }
            }
        })
        .await
        .unwrap_or(false);

        assert!(models_accepted, "fetch-models should be accepted while concierge is active");

        conn.framed
            .send(ClientMessage::Ping)
            .await
            .expect("send ping while concierge and fetch-models are active");

        let pong_received = timeout(Duration::from_millis(500), async {
            loop {
                match conn.recv_with_timeout(Duration::from_millis(500)).await {
                    DaemonMessage::Pong => return true,
                    DaemonMessage::AgentEvent { .. } => continue,
                    other => panic!("expected Pong while concierge and fetch-models are active, got {other:?}"),
                }
            }
        })
        .await
        .unwrap_or(false);

        assert!(pong_received, "ping should not be blocked while concierge and fetch-models are active");

        concierge_accept_task.abort();
        models_accept_task.abort();
        conn.shutdown().await;
    }

    #[tokio::test]
    async fn provider_validation_async_request_does_not_block_ping() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind fake provider listener");
        let addr = listener.local_addr().expect("listener addr");
        let accept_task = tokio::spawn(async move {
            let (_stream, _) = listener.accept().await.expect("accept provider request");
            tokio::time::sleep(Duration::from_secs(5)).await;
        });

        let mut conn = spawn_test_connection().await;
        declare_async_command_capability(&mut conn).await;

        conn.framed
            .send(ClientMessage::AgentValidateProvider {
                provider_id: "openai".to_string(),
                base_url: format!("http://{addr}"),
                api_key: "test-key".to_string(),
                auth_source: "api_key".to_string(),
            })
            .await
            .expect("request provider validation");

        let operation_id = match conn.recv().await {
            DaemonMessage::OperationAccepted { operation_id, kind, .. } => {
                assert_eq!(kind, "provider_validation");
                operation_id
            }
            other => panic!("expected provider validation acceptance, got {other:?}"),
        };

        conn.framed
            .send(ClientMessage::Ping)
            .await
            .expect("send ping while provider validation is active");

        let pong_received = timeout(Duration::from_millis(250), async {
            loop {
                match conn.recv().await {
                    DaemonMessage::Pong => return true,
                    other => panic!(
                        "expected Pong while provider validation runs in background, got {other:?}"
                    ),
                }
            }
        })
        .await
        .unwrap_or(false);

        assert!(
            pong_received,
            "ping should not be blocked behind provider validation"
        );

        conn.framed
            .send(ClientMessage::AgentGetOperationStatus { operation_id })
            .await
            .expect("query provider validation status");

        match conn.recv().await {
            DaemonMessage::OperationStatus { snapshot } => {
                assert_eq!(snapshot.kind, "provider_validation");
                assert!(matches!(
                    snapshot.state,
                    amux_protocol::OperationLifecycleState::Accepted
                        | amux_protocol::OperationLifecycleState::Started
                ));
            }
            other => panic!("expected provider validation status snapshot, got {other:?}"),
        }

        accept_task.abort();
        conn.shutdown().await;
    }

    #[tokio::test]
    async fn provider_validation_legacy_request_does_not_block_ping() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind fake provider listener");
        let addr = listener.local_addr().expect("listener addr");
        let accept_task = tokio::spawn(async move {
            let (_stream, _) = listener.accept().await.expect("accept provider request");
            tokio::time::sleep(Duration::from_secs(5)).await;
        });

        let mut conn = spawn_test_connection().await;

        conn.framed
            .send(ClientMessage::AgentValidateProvider {
                provider_id: "openai".to_string(),
                base_url: format!("http://{addr}"),
                api_key: "test-key".to_string(),
                auth_source: "api_key".to_string(),
            })
            .await
            .expect("request provider validation");
        conn.framed
            .send(ClientMessage::Ping)
            .await
            .expect("send ping while provider validation is active");

        let pong_received = timeout(Duration::from_millis(250), async {
            loop {
                match conn.recv().await {
                    DaemonMessage::Pong => return true,
                    DaemonMessage::OperationAccepted { .. } => {
                        panic!("legacy client should not receive operation acceptance")
                    }
                    other => panic!(
                        "expected Pong while legacy provider validation runs in background, got {other:?}"
                    ),
                }
            }
        })
        .await
        .unwrap_or(false);

        assert!(
            pong_received,
            "ping should not be blocked for legacy provider validation"
        );

        accept_task.abort();
        conn.shutdown().await;
    }

    #[tokio::test]
    async fn fetch_models_async_request_does_not_block_ping() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind fake models listener");
        let addr = listener.local_addr().expect("listener addr");
        let accept_task = tokio::spawn(async move {
            let (_stream, _) = listener.accept().await.expect("accept models request");
            tokio::time::sleep(Duration::from_secs(5)).await;
        });

        let mut conn = spawn_test_connection().await;
        declare_async_command_capability(&mut conn).await;

        conn.framed
            .send(ClientMessage::AgentFetchModels {
                provider_id: "openai".to_string(),
                base_url: format!("http://{addr}"),
                api_key: "test-key".to_string(),
            })
            .await
            .expect("request models fetch");

        let operation_id = match conn.recv().await {
            DaemonMessage::OperationAccepted { operation_id, kind, .. } => {
                assert_eq!(kind, "fetch_models");
                operation_id
            }
            other => panic!("expected fetch-models acceptance, got {other:?}"),
        };

        conn.framed
            .send(ClientMessage::Ping)
            .await
            .expect("send ping while models fetch is active");

        let pong_received = timeout(Duration::from_millis(250), async {
            loop {
                match conn.recv().await {
                    DaemonMessage::Pong => return true,
                    other => {
                        panic!(
                            "expected Pong while fetch-models runs in background, got {other:?}"
                        )
                    }
                }
            }
        })
        .await
        .unwrap_or(false);

        assert!(pong_received, "ping should not be blocked behind fetch-models");

        conn.framed
            .send(ClientMessage::AgentGetOperationStatus { operation_id })
            .await
            .expect("query fetch-models status");

        match conn.recv().await {
            DaemonMessage::OperationStatus { snapshot } => {
                assert_eq!(snapshot.kind, "fetch_models");
                assert!(matches!(
                    snapshot.state,
                    amux_protocol::OperationLifecycleState::Accepted
                        | amux_protocol::OperationLifecycleState::Started
                ));
            }
            other => panic!("expected fetch-models status snapshot, got {other:?}"),
        }

        accept_task.abort();
        conn.shutdown().await;
    }

    #[tokio::test]
    async fn fetch_models_legacy_request_does_not_block_ping() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind fake models listener");
        let addr = listener.local_addr().expect("listener addr");
        let accept_task = tokio::spawn(async move {
            let (_stream, _) = listener.accept().await.expect("accept models request");
            tokio::time::sleep(Duration::from_secs(5)).await;
        });

        let mut conn = spawn_test_connection().await;

        conn.framed
            .send(ClientMessage::AgentFetchModels {
                provider_id: "openai".to_string(),
                base_url: format!("http://{addr}"),
                api_key: "test-key".to_string(),
            })
            .await
            .expect("request models fetch");
        conn.framed
            .send(ClientMessage::Ping)
            .await
            .expect("send ping while models fetch is active");

        let pong_received = timeout(Duration::from_millis(250), async {
            loop {
                match conn.recv().await {
                    DaemonMessage::Pong => return true,
                    DaemonMessage::OperationAccepted { .. } => {
                        panic!("legacy client should not receive operation acceptance")
                    }
                    other => {
                        panic!(
                            "expected Pong while legacy fetch-models runs in background, got {other:?}"
                        )
                    }
                }
            }
        })
        .await
        .unwrap_or(false);

        assert!(
            pong_received,
            "ping should not be blocked for legacy fetch-models"
        );

        accept_task.abort();
        conn.shutdown().await;
    }

    #[tokio::test]
    async fn explain_action_async_request_returns_operation_acceptance() {
        let mut conn = spawn_test_connection().await;
        declare_async_command_capability(&mut conn).await;

        conn.framed
            .send(ClientMessage::AgentExplainAction {
                action_id: "missing-action".to_string(),
                step_index: None,
            })
            .await
            .expect("request explain action");

        let operation_id = match conn.recv().await {
            DaemonMessage::OperationAccepted { operation_id, kind, .. } => {
                assert_eq!(kind, "explain_action");
                operation_id
            }
            other => panic!("expected explain-action acceptance, got {other:?}"),
        };

        conn.framed
            .send(ClientMessage::AgentGetOperationStatus { operation_id })
            .await
            .expect("query explain-action status");

        match conn.recv().await {
            DaemonMessage::OperationStatus { snapshot } => {
                assert_eq!(snapshot.kind, "explain_action");
                assert!(matches!(
                    snapshot.state,
                    amux_protocol::OperationLifecycleState::Accepted
                        | amux_protocol::OperationLifecycleState::Started
                        | amux_protocol::OperationLifecycleState::Completed
                ));
            }
            other => panic!("expected explain-action status snapshot, got {other:?}"),
        }

        conn.shutdown().await;
    }

    #[tokio::test]
    async fn explain_action_legacy_request_returns_explanation_payload() {
        let mut conn = spawn_test_connection().await;

        conn.framed
            .send(ClientMessage::AgentExplainAction {
                action_id: "missing-action".to_string(),
                step_index: None,
            })
            .await
            .expect("request explain action");

        match conn.recv().await {
            DaemonMessage::AgentExplanation { explanation_json } => {
                let payload: serde_json::Value =
                    serde_json::from_str(&explanation_json).expect("valid explanation payload");
                assert_eq!(payload["action_id"], "missing-action");
                assert_eq!(payload["source"], "fallback");
            }
            DaemonMessage::OperationAccepted { .. } => {
                panic!("legacy client should not receive explain-action acceptance")
            }
            other => panic!("expected explain-action payload, got {other:?}"),
        }

        conn.shutdown().await;
    }

    #[tokio::test]
    async fn plugin_oauth_async_request_does_not_block_ping() {
        let mut conn = spawn_test_connection().await;
        register_test_oauth_plugin(&conn, "oauth-test").await;
        declare_async_command_capability(&mut conn).await;

        conn.framed
            .send(ClientMessage::PluginOAuthStart {
                name: "oauth-test".to_string(),
            })
            .await
            .expect("request plugin oauth start");

        let first = conn.recv().await;
        let (operation_id, auth_url) = match first {
            DaemonMessage::OperationAccepted {
                operation_id,
                kind,
                ..
            } => {
                assert_eq!(kind, "plugin_oauth_start");
                match conn.recv().await {
                    DaemonMessage::PluginOAuthUrl { name, url } => {
                        assert_eq!(name, "oauth-test");
                        (Some(operation_id), url)
                    }
                    other => panic!("expected plugin oauth url after acceptance, got {other:?}"),
                }
            }
            DaemonMessage::PluginOAuthUrl { name, url } => {
                assert_eq!(name, "oauth-test");
                (None, url)
            }
            other => panic!("expected operation acceptance or oauth url, got {other:?}"),
        };

        conn.framed
            .send(ClientMessage::Ping)
            .await
            .expect("send ping while oauth flow is active");

        let pong_received = timeout(Duration::from_millis(250), async {
            loop {
                match conn.recv().await {
                    DaemonMessage::Pong => return true,
                    DaemonMessage::PluginOAuthComplete { .. } => continue,
                    other => panic!(
                        "expected Pong while plugin oauth runs in background, got {other:?}"
                    ),
                }
            }
        })
        .await
        .unwrap_or(false);

        complete_test_oauth_callback(&auth_url).await;

        assert!(
            operation_id.is_some(),
            "async-capable client should receive operation acceptance before oauth url"
        );
        assert!(pong_received, "ping should not be blocked behind plugin oauth flow");

        if let Some(operation_id) = operation_id {
            conn.framed
                .send(ClientMessage::AgentGetOperationStatus { operation_id })
                .await
                .expect("query plugin oauth status");

            match conn.recv().await {
                DaemonMessage::OperationStatus { snapshot } => {
                    assert_eq!(snapshot.kind, "plugin_oauth_start");
                    assert!(matches!(
                        snapshot.state,
                        amux_protocol::OperationLifecycleState::Accepted
                            | amux_protocol::OperationLifecycleState::Started
                            | amux_protocol::OperationLifecycleState::Completed
                            | amux_protocol::OperationLifecycleState::Failed
                    ));
                }
                other => panic!("expected plugin oauth status snapshot, got {other:?}"),
            }
        }

        let _ = timeout(Duration::from_secs(1), async {
            loop {
                match conn.recv().await {
                    DaemonMessage::PluginOAuthComplete { name, .. } => {
                        assert_eq!(name, "oauth-test");
                        return;
                    }
                    DaemonMessage::OperationStatus { .. } | DaemonMessage::Pong => continue,
                    other => panic!("expected oauth completion during cleanup, got {other:?}"),
                }
            }
        })
        .await;

        conn.shutdown().await;
    }

    #[tokio::test]
    async fn plugin_oauth_legacy_request_does_not_block_ping() {
        let mut conn = spawn_test_connection().await;
        register_test_oauth_plugin(&conn, "oauth-test-legacy").await;

        conn.framed
            .send(ClientMessage::PluginOAuthStart {
                name: "oauth-test-legacy".to_string(),
            })
            .await
            .expect("request legacy plugin oauth start");

        let auth_url = match conn.recv().await {
            DaemonMessage::PluginOAuthUrl { name, url } => {
                assert_eq!(name, "oauth-test-legacy");
                url
            }
            DaemonMessage::OperationAccepted { .. } => {
                panic!("legacy client should not receive operation acceptance")
            }
            other => panic!("expected plugin oauth url for legacy client, got {other:?}"),
        };

        conn.framed
            .send(ClientMessage::Ping)
            .await
            .expect("send ping while legacy oauth flow is active");

        let pong_received = timeout(Duration::from_millis(250), async {
            loop {
                match conn.recv().await {
                    DaemonMessage::Pong => return true,
                    DaemonMessage::PluginOAuthComplete { .. } => continue,
                    DaemonMessage::OperationAccepted { .. } => {
                        panic!("legacy client should not receive operation acceptance")
                    }
                    other => panic!(
                        "expected Pong while legacy plugin oauth runs in background, got {other:?}"
                    ),
                }
            }
        })
        .await
        .unwrap_or(false);

        complete_test_oauth_callback(&auth_url).await;

        assert!(
            pong_received,
            "ping should not be blocked behind legacy plugin oauth flow"
        );

        let _ = timeout(Duration::from_secs(1), async {
            loop {
                match conn.recv().await {
                    DaemonMessage::PluginOAuthComplete { name, .. } => {
                        assert_eq!(name, "oauth-test-legacy");
                        return;
                    }
                    DaemonMessage::Pong => continue,
                    other => panic!("expected oauth completion during cleanup, got {other:?}"),
                }
            }
        })
        .await;

        conn.shutdown().await;
    }

    #[tokio::test]
    async fn plugin_api_call_async_request_does_not_block_ping() {
        let mut conn = spawn_test_connection().await;
        register_test_api_plugin(&conn, "api-test").await;
        conn.plugin_manager
            .set_test_api_call_delay(Duration::from_secs(5))
            .await;
        declare_async_command_capability(&mut conn).await;

        conn.framed
            .send(ClientMessage::PluginApiCall {
                plugin_name: "api-test".to_string(),
                endpoint_name: "slow".to_string(),
                params: "{}".to_string(),
            })
            .await
            .expect("request async plugin api call");

        let operation_id = match conn.recv_with_timeout(Duration::from_secs(2)).await {
            DaemonMessage::OperationAccepted {
                operation_id,
                kind,
                ..
            } => {
                assert_eq!(kind, "plugin_api_call");
                operation_id
            }
            other => panic!("expected plugin api operation acceptance, got {other:?}"),
        };

        conn.framed
            .send(ClientMessage::Ping)
            .await
            .expect("send ping while plugin api call is active");

        let pong_received = timeout(Duration::from_millis(250), async {
            loop {
                match conn.recv_with_timeout(Duration::from_millis(250)).await {
                    DaemonMessage::Pong => return true,
                    DaemonMessage::PluginApiCallResult { .. } => continue,
                    other => panic!(
                        "expected Pong while plugin api call runs in background, got {other:?}"
                    ),
                }
            }
        })
        .await
        .unwrap_or(false);

        assert!(pong_received, "ping should not be blocked behind plugin api call");

        conn.framed
            .send(ClientMessage::AgentGetOperationStatus { operation_id })
            .await
            .expect("query plugin api operation status");

        match conn.recv_with_timeout(Duration::from_secs(2)).await {
            DaemonMessage::OperationStatus { snapshot } => {
                assert_eq!(snapshot.kind, "plugin_api_call");
                assert!(matches!(
                    snapshot.state,
                    amux_protocol::OperationLifecycleState::Accepted
                        | amux_protocol::OperationLifecycleState::Started
                ));
            }
            other => panic!("expected plugin api status snapshot, got {other:?}"),
        }

        let result = timeout(Duration::from_secs(6), async {
            loop {
                match conn.recv_with_timeout(Duration::from_secs(6)).await {
                    DaemonMessage::PluginApiCallResult {
                        plugin_name,
                        endpoint_name,
                        success,
                        error_type,
                        ..
                    } => {
                        assert_eq!(plugin_name, "api-test");
                        assert_eq!(endpoint_name, "slow");
                        assert!(!success);
                        assert_eq!(error_type.as_deref(), Some("timeout"));
                        return;
                    }
                    DaemonMessage::Pong | DaemonMessage::OperationStatus { .. } => continue,
                    other => panic!("expected plugin api result during cleanup, got {other:?}"),
                }
            }
        })
        .await;

        assert!(result.is_ok(), "plugin api result should eventually arrive");
        conn.shutdown().await;
    }

    #[tokio::test]
    async fn plugin_api_call_legacy_request_does_not_block_ping() {
        let mut conn = spawn_test_connection().await;
        register_test_api_plugin(&conn, "api-test-legacy").await;
        conn.plugin_manager
            .set_test_api_call_delay(Duration::from_secs(5))
            .await;

        conn.framed
            .send(ClientMessage::PluginApiCall {
                plugin_name: "api-test-legacy".to_string(),
                endpoint_name: "slow".to_string(),
                params: "{}".to_string(),
            })
            .await
            .expect("request legacy plugin api call");

        conn.framed
            .send(ClientMessage::Ping)
            .await
            .expect("send ping while legacy plugin api call is active");

        let pong_received = timeout(Duration::from_millis(250), async {
            loop {
                match conn.recv_with_timeout(Duration::from_millis(250)).await {
                    DaemonMessage::Pong => return true,
                    DaemonMessage::OperationAccepted { .. } => {
                        panic!("legacy client should not receive operation acceptance")
                    }
                    DaemonMessage::PluginApiCallResult { .. } => continue,
                    other => panic!(
                        "expected Pong while legacy plugin api call runs in background, got {other:?}"
                    ),
                }
            }
        })
        .await
        .unwrap_or(false);

        assert!(
            pong_received,
            "ping should not be blocked behind legacy plugin api call"
        );

        let result = timeout(Duration::from_secs(6), async {
            loop {
                match conn.recv_with_timeout(Duration::from_secs(6)).await {
                    DaemonMessage::PluginApiCallResult {
                        plugin_name,
                        endpoint_name,
                        success,
                        error_type,
                        ..
                    } => {
                        assert_eq!(plugin_name, "api-test-legacy");
                        assert_eq!(endpoint_name, "slow");
                        assert!(!success);
                        assert_eq!(error_type.as_deref(), Some("timeout"));
                        return;
                    }
                    DaemonMessage::Pong => continue,
                    DaemonMessage::OperationAccepted { .. } => {
                        panic!("legacy client should not receive operation acceptance")
                    }
                    other => panic!("expected plugin api result during cleanup, got {other:?}"),
                }
            }
        })
        .await;

        assert!(result.is_ok(), "legacy plugin api result should eventually arrive");
        conn.shutdown().await;
    }

    #[tokio::test]
    async fn plugin_queue_saturation_rejects_extra_plugin_api_call_but_accepts_provider_work() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind fake provider listener");
        let addr = listener.local_addr().expect("provider listener addr");
        let accept_task = tokio::spawn(async move {
            let (_stream, _) = listener.accept().await.expect("accept provider request");
            tokio::time::sleep(Duration::from_secs(5)).await;
        });

        let mut conn = spawn_test_connection().await;
        register_test_api_plugin(&conn, "api-test-saturated").await;
        conn.plugin_manager
            .set_test_api_call_delay(Duration::from_secs(5))
            .await;
        declare_async_command_capability(&mut conn).await;

        for idx in 0..crate::server::BackgroundPendingCounts::capacity(
            crate::server::BackgroundSubsystem::PluginIo,
        ) {
            conn.framed
                .send(ClientMessage::PluginApiCall {
                    plugin_name: "api-test-saturated".to_string(),
                    endpoint_name: "slow".to_string(),
                    params: format!("{{\"idx\":{idx}}}"),
                })
                .await
                .expect("request saturated plugin api call");

            match conn.recv_with_timeout(Duration::from_secs(2)).await {
                DaemonMessage::OperationAccepted { kind, .. } => {
                    assert_eq!(kind, "plugin_api_call");
                }
                other => panic!("expected plugin api acceptance during saturation setup, got {other:?}"),
            }
        }

        conn.framed
            .send(ClientMessage::PluginApiCall {
                plugin_name: "api-test-saturated".to_string(),
                endpoint_name: "slow".to_string(),
                params: "{\"idx\":999}".to_string(),
            })
            .await
            .expect("request overflow plugin api call");

        match conn.recv_with_timeout(Duration::from_secs(2)).await {
            DaemonMessage::Error { message } => {
                assert!(message.contains("plugin_io") || message.contains("queue is full"));
            }
            other => panic!("expected plugin queue saturation error, got {other:?}"),
        }

        conn.framed
            .send(ClientMessage::AgentValidateProvider {
                provider_id: "openai".to_string(),
                base_url: format!("http://{addr}"),
                api_key: "test-key".to_string(),
                auth_source: "api_key".to_string(),
            })
            .await
            .expect("request provider validation while plugin queue is saturated");

        match conn.recv_with_timeout(Duration::from_secs(2)).await {
            DaemonMessage::OperationAccepted { kind, .. } => {
                assert_eq!(kind, "provider_validation");
            }
            other => panic!("expected provider validation acceptance while plugin queue is saturated, got {other:?}"),
        }

        conn.framed
            .send(ClientMessage::Ping)
            .await
            .expect("send ping while plugin queue is saturated");
        assert!(matches!(conn.recv_with_timeout(Duration::from_secs(1)).await, DaemonMessage::Pong));

        accept_task.abort();
        conn.shutdown().await;
    }

    #[tokio::test]
    async fn subsystem_metrics_query_reports_plugin_rejections_after_queue_saturation() {
        let mut conn = spawn_test_connection().await;
        register_test_api_plugin(&conn, "api-test-metrics").await;
        conn.plugin_manager
            .set_test_api_call_delay(Duration::from_secs(5))
            .await;
        declare_async_command_capability(&mut conn).await;

        for idx in 0..crate::server::BackgroundPendingCounts::capacity(
            crate::server::BackgroundSubsystem::PluginIo,
        ) {
            conn.framed
                .send(ClientMessage::PluginApiCall {
                    plugin_name: "api-test-metrics".to_string(),
                    endpoint_name: "slow".to_string(),
                    params: format!("{{\"idx\":{idx}}}"),
                })
                .await
                .expect("request saturated plugin api call for metrics query");

            match conn.recv_with_timeout(Duration::from_secs(2)).await {
                DaemonMessage::OperationAccepted { kind, .. } => {
                    assert_eq!(kind, "plugin_api_call");
                }
                other => panic!(
                    "expected plugin api acceptance during metrics saturation setup, got {other:?}"
                ),
            }
        }

        conn.framed
            .send(ClientMessage::PluginApiCall {
                plugin_name: "api-test-metrics".to_string(),
                endpoint_name: "slow".to_string(),
                params: "{\"idx\":999}".to_string(),
            })
            .await
            .expect("request overflow plugin api call for metrics query");

        match conn.recv_with_timeout(Duration::from_secs(2)).await {
            DaemonMessage::Error { message } => {
                assert!(message.contains("plugin_io") || message.contains("queue is full"));
            }
            other => panic!("expected plugin queue saturation error, got {other:?}"),
        }

        conn.framed
            .send(ClientMessage::AgentGetSubsystemMetrics)
            .await
            .expect("query subsystem metrics after rejection");

        let metrics_json = match conn.recv_with_timeout(Duration::from_secs(2)).await {
            DaemonMessage::AgentSubsystemMetrics { metrics_json } => metrics_json,
            other => panic!("expected AgentSubsystemMetrics response, got {other:?}"),
        };

        let metrics: serde_json::Value =
            serde_json::from_str(&metrics_json).expect("deserialize subsystem metrics response");
        assert!(metrics["plugin_io"]["rejection_count"].as_u64().unwrap_or(0) >= 1);
        assert!(metrics["plugin_io"]["max_depth"].as_u64().unwrap_or(0) >= 1);

        conn.shutdown().await;
    }

    #[tokio::test]
    async fn plugin_queue_saturation_rejects_plugin_oauth_start_before_acceptance() {
        let mut conn = spawn_test_connection().await;
        register_test_api_plugin(&conn, "api-test-saturated-oauth").await;
        register_test_oauth_plugin(&conn, "oauth-test-saturated").await;
        conn.plugin_manager
            .set_test_api_call_delay(Duration::from_secs(5))
            .await;
        declare_async_command_capability(&mut conn).await;

        for idx in 0..crate::server::BackgroundPendingCounts::capacity(
            crate::server::BackgroundSubsystem::PluginIo,
        ) {
            conn.framed
                .send(ClientMessage::PluginApiCall {
                    plugin_name: "api-test-saturated-oauth".to_string(),
                    endpoint_name: "slow".to_string(),
                    params: format!("{{\"idx\":{idx}}}"),
                })
                .await
                .expect("request saturated plugin api call");

            match conn.recv_with_timeout(Duration::from_secs(2)).await {
                DaemonMessage::OperationAccepted { kind, .. } => {
                    assert_eq!(kind, "plugin_api_call");
                }
                other => panic!("expected plugin api acceptance during saturation setup, got {other:?}"),
            }
        }

        conn.framed
            .send(ClientMessage::PluginOAuthStart {
                name: "oauth-test-saturated".to_string(),
            })
            .await
            .expect("request oauth start while plugin queue is saturated");

        match conn.recv_with_timeout(Duration::from_secs(2)).await {
            DaemonMessage::Error { message } => {
                assert!(message.contains("plugin_io") || message.contains("queue is full"));
            }
            other => panic!("expected plugin queue saturation error for oauth start, got {other:?}"),
        }

        conn.shutdown().await;
    }

    #[tokio::test]
    async fn provider_queue_saturation_rejects_extra_validation_but_accepts_agent_work() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind fake provider listener");
        let addr = listener.local_addr().expect("provider listener addr");
        let accept_task = tokio::spawn(async move {
            loop {
                let (stream, _) = listener.accept().await.expect("accept provider request");
                tokio::spawn(async move {
                    let _stream = stream;
                    tokio::time::sleep(Duration::from_secs(5)).await;
                });
            }
        });

        let mut conn = spawn_test_connection().await;
        declare_async_command_capability(&mut conn).await;

        for _ in 0..crate::server::BackgroundPendingCounts::capacity(
            crate::server::BackgroundSubsystem::ProviderIo,
        ) {
            conn.framed
                .send(ClientMessage::AgentValidateProvider {
                    provider_id: "openai".to_string(),
                    base_url: format!("http://{addr}"),
                    api_key: "test-key".to_string(),
                    auth_source: "api_key".to_string(),
                })
                .await
                .expect("request provider validation during saturation setup");

            match conn.recv_with_timeout(Duration::from_secs(2)).await {
                DaemonMessage::OperationAccepted { kind, .. } => {
                    assert_eq!(kind, "provider_validation");
                }
                other => panic!("expected provider validation acceptance during saturation setup, got {other:?}"),
            }
        }

        conn.framed
            .send(ClientMessage::AgentValidateProvider {
                provider_id: "openai".to_string(),
                base_url: format!("http://{addr}"),
                api_key: "test-key".to_string(),
                auth_source: "api_key".to_string(),
            })
            .await
            .expect("request overflow provider validation");

        match conn.recv_with_timeout(Duration::from_secs(2)).await {
            DaemonMessage::Error { message } => {
                assert!(message.contains("provider_io") || message.contains("queue is full"));
            }
            other => panic!("expected provider queue saturation error, got {other:?}"),
        }

        conn.framed
            .send(ClientMessage::AgentExplainAction {
                action_id: "missing-action".to_string(),
                step_index: None,
            })
            .await
            .expect("request explain action while provider queue is saturated");

        match conn.recv_with_timeout(Duration::from_secs(2)).await {
            DaemonMessage::OperationAccepted { kind, .. } => {
                assert_eq!(kind, "explain_action");
            }
            other => panic!("expected agent-work acceptance while provider queue is saturated, got {other:?}"),
        }

        accept_task.abort();
        conn.shutdown().await;
    }

    #[tokio::test]
    async fn config_set_item_commit_does_not_block_ping_while_reconcile_runs() {
        let mut conn = spawn_test_connection().await;
        conn.agent
            .set_test_config_reconcile_delay(Some(Duration::from_secs(1)))
            .await;

        conn.framed
            .send(ClientMessage::AgentSetConfigItem {
                key_path: "/managed_execution/security_level".to_string(),
                value_json: r#""yolo""#.to_string(),
            })
            .await
            .expect("request config item update");
        conn.framed
            .send(ClientMessage::Ping)
            .await
            .expect("send ping while config reconcile is active");

        let pong_received = timeout(Duration::from_millis(250), async {
            loop {
                match conn.recv_with_timeout(Duration::from_millis(250)).await {
                    DaemonMessage::Pong => return true,
                    other => panic!("expected Pong while config reconcile runs in background, got {other:?}"),
                }
            }
        })
        .await
        .unwrap_or(false);

        assert!(pong_received, "ping should not be blocked behind config reconcile");

        let updated = conn.agent.get_config().await;
        assert_eq!(
            updated.managed_execution.security_level,
            amux_protocol::SecurityLevel::Yolo
        );

        conn.shutdown().await;
    }

    #[tokio::test]
    async fn provider_model_commit_does_not_block_ping_while_reconcile_runs() {
        let mut conn = spawn_test_connection().await;
        let mut config = conn.agent.get_config().await;
        config.api_key = "sk-test".to_string();
        conn.agent.set_config(config).await;
        conn.agent
            .set_test_config_reconcile_delay(Some(Duration::from_secs(1)))
            .await;

        conn.framed
            .send(ClientMessage::AgentSetProviderModel {
                provider_id: "openai".to_string(),
                model: "gpt-5.4-mini".to_string(),
            })
            .await
            .expect("request provider/model update");
        conn.framed
            .send(ClientMessage::Ping)
            .await
            .expect("send ping while provider-model reconcile is active");

        let pong_received = timeout(Duration::from_millis(250), async {
            loop {
                match conn.recv_with_timeout(Duration::from_millis(250)).await {
                    DaemonMessage::Pong => return true,
                    other => panic!(
                        "expected Pong while provider-model reconcile runs in background, got {other:?}"
                    ),
                }
            }
        })
        .await
        .unwrap_or(false);

        assert!(
            pong_received,
            "ping should not be blocked behind provider-model reconcile"
        );

        let updated = conn.agent.get_config().await;
        assert_eq!(updated.provider, "openai");
        assert_eq!(updated.model, "gpt-5.4-mini");

        conn.shutdown().await;
    }

    #[tokio::test]
    async fn async_config_set_item_returns_operation_acceptance_while_reconcile_runs() {
        let mut conn = spawn_test_connection().await;
        conn.agent
            .set_test_config_reconcile_delay(Some(Duration::from_secs(1)))
            .await;
        declare_async_command_capability(&mut conn).await;

        conn.framed
            .send(ClientMessage::AgentSetConfigItem {
                key_path: "/managed_execution/security_level".to_string(),
                value_json: r#""yolo""#.to_string(),
            })
            .await
            .expect("request async config item update");

        let operation_id = match conn.recv_with_timeout(Duration::from_secs(2)).await {
            DaemonMessage::OperationAccepted {
                operation_id,
                kind,
                ..
            } => {
                assert_eq!(kind, "config_set_item");
                operation_id
            }
            other => panic!("expected config-set operation acceptance, got {other:?}"),
        };

        conn.framed
            .send(ClientMessage::Ping)
            .await
            .expect("send ping while async config reconcile is active");

        match conn.recv_with_timeout(Duration::from_millis(250)).await {
            DaemonMessage::Pong => {}
            other => panic!("expected Pong while async config reconcile runs, got {other:?}"),
        }

        conn.framed
            .send(ClientMessage::AgentGetOperationStatus { operation_id })
            .await
            .expect("query config-set operation status");

        match conn.recv_with_timeout(Duration::from_secs(2)).await {
            DaemonMessage::OperationStatus { snapshot } => {
                assert_eq!(snapshot.kind, "config_set_item");
                assert!(matches!(
                    snapshot.state,
                    amux_protocol::OperationLifecycleState::Accepted
                        | amux_protocol::OperationLifecycleState::Started
                        | amux_protocol::OperationLifecycleState::Completed
                ));
            }
            other => panic!("expected config-set operation status, got {other:?}"),
        }

        conn.shutdown().await;
    }

    #[tokio::test]
    async fn async_provider_model_update_returns_operation_acceptance_while_reconcile_runs() {
        let mut conn = spawn_test_connection().await;
        let mut config = conn.agent.get_config().await;
        config.api_key = "sk-test".to_string();
        conn.agent.set_config(config).await;
        conn.agent
            .set_test_config_reconcile_delay(Some(Duration::from_secs(1)))
            .await;
        declare_async_command_capability(&mut conn).await;

        conn.framed
            .send(ClientMessage::AgentSetProviderModel {
                provider_id: "openai".to_string(),
                model: "gpt-5.4-mini".to_string(),
            })
            .await
            .expect("request async provider/model update");

        let operation_id = match conn.recv_with_timeout(Duration::from_secs(2)).await {
            DaemonMessage::OperationAccepted {
                operation_id,
                kind,
                ..
            } => {
                assert_eq!(kind, "set_provider_model");
                operation_id
            }
            other => panic!("expected provider-model operation acceptance, got {other:?}"),
        };

        conn.framed
            .send(ClientMessage::Ping)
            .await
            .expect("send ping while async provider-model reconcile is active");

        match conn.recv_with_timeout(Duration::from_millis(250)).await {
            DaemonMessage::Pong => {}
            other => panic!(
                "expected Pong while async provider-model reconcile runs, got {other:?}"
            ),
        }

        conn.framed
            .send(ClientMessage::AgentGetOperationStatus { operation_id })
            .await
            .expect("query provider-model operation status");

        match conn.recv_with_timeout(Duration::from_secs(2)).await {
            DaemonMessage::OperationStatus { snapshot } => {
                assert_eq!(snapshot.kind, "set_provider_model");
                assert!(matches!(
                    snapshot.state,
                    amux_protocol::OperationLifecycleState::Accepted
                        | amux_protocol::OperationLifecycleState::Started
                        | amux_protocol::OperationLifecycleState::Completed
                ));
            }
            other => panic!("expected provider-model operation status, got {other:?}"),
        }

        conn.shutdown().await;
    }

    #[tokio::test]
    async fn config_queries_return_desired_and_effective_state_while_reconcile_runs() {
        let mut conn = spawn_test_connection().await;
        conn.agent
            .set_test_config_reconcile_delay(Some(Duration::from_secs(1)))
            .await;

        conn.framed
            .send(ClientMessage::AgentSetConfigItem {
                key_path: "/managed_execution/security_level".to_string(),
                value_json: r#""yolo""#.to_string(),
            })
            .await
            .expect("request config item update");

        conn.framed
            .send(ClientMessage::AgentGetConfig)
            .await
            .expect("query desired config while reconcile is active");

        let config_json = match conn.recv_with_timeout(Duration::from_millis(250)).await {
            DaemonMessage::AgentConfigResponse { config_json } => config_json,
            other => panic!(
                "expected AgentConfigResponse while reconcile is active, got {other:?}"
            ),
        };
        let desired: AgentConfig =
            serde_json::from_str(&config_json).expect("deserialize desired config");
        assert_eq!(
            desired.managed_execution.security_level,
            amux_protocol::SecurityLevel::Yolo
        );

        conn.framed
            .send(ClientMessage::AgentGetEffectiveConfigState)
            .await
            .expect("query effective config state while reconcile is active");

        let state_json = match conn.recv_with_timeout(Duration::from_millis(250)).await {
            DaemonMessage::AgentEffectiveConfigState { state_json } => state_json,
            other => panic!(
                "expected AgentEffectiveConfigState while reconcile is active, got {other:?}"
            ),
        };
        let effective: crate::agent::ConfigEffectiveRuntimeState =
            serde_json::from_str(&state_json).expect("deserialize effective config state");
        assert_eq!(
            effective.reconcile.state,
            crate::agent::ConfigReconcileState::Reconciling
        );
        assert!(
            effective.reconcile.effective_revision < effective.reconcile.desired_revision,
            "effective state should lag desired while reconcile is active"
        );

        conn.framed
            .send(ClientMessage::Ping)
            .await
            .expect("send ping while config queries are active");

        match conn.recv_with_timeout(Duration::from_millis(250)).await {
            DaemonMessage::Pong => {}
            other => panic!(
                "expected Pong while config queries run during reconcile, got {other:?}"
            ),
        }

        conn.shutdown().await;
    }

    #[tokio::test]
    async fn async_config_set_item_reports_failed_operation_when_reconcile_fails() {
        let mut conn = spawn_test_connection().await;
        conn.agent
            .set_test_config_reconcile_failure(Some("forced reconcile failure".to_string()))
            .await;
        declare_async_command_capability(&mut conn).await;

        conn.framed
            .send(ClientMessage::AgentSetConfigItem {
                key_path: "/managed_execution/security_level".to_string(),
                value_json: r#""yolo""#.to_string(),
            })
            .await
            .expect("request async config item update with forced reconcile failure");

        let operation_id = match conn.recv_with_timeout(Duration::from_secs(2)).await {
            DaemonMessage::OperationAccepted {
                operation_id,
                kind,
                ..
            } => {
                assert_eq!(kind, "config_set_item");
                operation_id
            }
            other => panic!("expected config-set operation acceptance, got {other:?}"),
        };

        tokio::time::sleep(Duration::from_millis(50)).await;

        conn.framed
            .send(ClientMessage::AgentGetOperationStatus { operation_id })
            .await
            .expect("query failed config-set operation status");

        match conn.recv_with_timeout(Duration::from_secs(2)).await {
            DaemonMessage::OperationStatus { snapshot } => {
                assert_eq!(snapshot.kind, "config_set_item");
                assert_eq!(
                    snapshot.state,
                    amux_protocol::OperationLifecycleState::Failed
                );
            }
            other => panic!("expected failed config-set operation status, got {other:?}"),
        }

        conn.shutdown().await;
    }

    #[tokio::test]
    async fn agent_work_queue_saturation_rejects_extra_explain_action_but_accepts_provider_work() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind fake provider listener");
        let addr = listener.local_addr().expect("provider listener addr");
        let accept_task = tokio::spawn(async move {
            let (_stream, _) = listener.accept().await.expect("accept provider request");
            tokio::time::sleep(Duration::from_secs(5)).await;
        });

        let mut config = AgentConfig::default();
        config.tool_synthesis.enabled = true;
        let mut conn = spawn_test_connection_with_config(config).await;
        conn.agent
            .set_test_synthesize_tool_delay(Some(Duration::from_secs(5)))
            .await;
        declare_async_command_capability(&mut conn).await;

        for idx in 0..crate::server::BackgroundPendingCounts::capacity(
            crate::server::BackgroundSubsystem::AgentWork,
        ) {
            conn.agent
                .set_test_synthesize_tool_delay(Some(Duration::from_secs(5)))
                .await;
            conn.framed
                .send(ClientMessage::AgentSynthesizeTool {
                    request_json: serde_json::json!({
                        "kind": "cli",
                        "target": format!("echo hello-{idx}")
                    })
                    .to_string(),
                })
                .await
                .expect("request synthesize tool during saturation setup");

            match conn.recv_with_timeout(Duration::from_secs(2)).await {
                DaemonMessage::OperationAccepted { kind, .. } => {
                    assert_eq!(kind, "synthesize_tool");
                }
                other => panic!("expected synthesize-tool acceptance during saturation setup, got {other:?}"),
            }
        }

        conn.framed
            .send(ClientMessage::AgentExplainAction {
                action_id: "missing-action".to_string(),
                step_index: None,
            })
            .await
            .expect("request explain action while agent-work queue is saturated");

        match conn.recv_with_timeout(Duration::from_secs(2)).await {
            DaemonMessage::Error { message } => {
                assert!(message.contains("agent_work") || message.contains("queue is full"));
            }
            other => panic!("expected agent-work queue saturation error, got {other:?}"),
        }

        conn.framed
            .send(ClientMessage::AgentValidateProvider {
                provider_id: "openai".to_string(),
                base_url: format!("http://{addr}"),
                api_key: "test-key".to_string(),
                auth_source: "api_key".to_string(),
            })
            .await
            .expect("request provider validation while agent-work queue is saturated");

        match conn.recv_with_timeout(Duration::from_secs(2)).await {
            DaemonMessage::OperationAccepted { kind, .. } => {
                assert_eq!(kind, "provider_validation");
            }
            other => panic!("expected provider acceptance while agent-work queue is saturated, got {other:?}"),
        }

        accept_task.abort();
        conn.shutdown().await;
    }

    #[tokio::test]
    async fn agent_work_load_does_not_block_config_or_operation_status_queries() {
        let mut config = AgentConfig::default();
        config.tool_synthesis.enabled = true;
        let mut conn = spawn_test_connection_with_config(config).await;
        declare_async_command_capability(&mut conn).await;

        conn.agent
            .set_test_synthesize_tool_delay(Some(Duration::from_secs(5)))
            .await;

        let capacity = crate::server::BackgroundPendingCounts::capacity(
            crate::server::BackgroundSubsystem::AgentWork,
        );

        let first_operation_id = {
            conn.framed
                .send(ClientMessage::AgentSynthesizeTool {
                    request_json: serde_json::json!({
                        "kind": "cli",
                        "target": "echo first"
                    })
                    .to_string(),
                })
                .await
                .expect("request first synthesize tool");

            match conn.recv_with_timeout(Duration::from_secs(2)).await {
                DaemonMessage::OperationAccepted {
                    operation_id,
                    kind,
                    ..
                } => {
                    assert_eq!(kind, "synthesize_tool");
                    operation_id
                }
                other => panic!("expected first synthesize-tool acceptance, got {other:?}"),
            }
        };

        for idx in 1..capacity {
            conn.agent
                .set_test_synthesize_tool_delay(Some(Duration::from_secs(5)))
                .await;
            conn.framed
                .send(ClientMessage::AgentSynthesizeTool {
                    request_json: serde_json::json!({
                        "kind": "cli",
                        "target": format!("echo {idx}")
                    })
                    .to_string(),
                })
                .await
                .expect("request synthesize tool during load setup");

            match conn.recv_with_timeout(Duration::from_secs(2)).await {
                DaemonMessage::OperationAccepted { kind, .. } => {
                    assert_eq!(kind, "synthesize_tool");
                }
                other => panic!(
                    "expected synthesize-tool acceptance during load setup, got {other:?}"
                ),
            }
        }

        conn.framed
            .send(ClientMessage::AgentGetConfig)
            .await
            .expect("query config while agent-work is busy");

        match conn.recv_with_timeout(Duration::from_millis(250)).await {
            DaemonMessage::AgentConfigResponse { config_json } => {
                let desired: AgentConfig =
                    serde_json::from_str(&config_json).expect("deserialize config response");
                assert!(desired.tool_synthesis.enabled);
            }
            other => panic!("expected AgentConfigResponse under agent-work load, got {other:?}"),
        }

        conn.framed
            .send(ClientMessage::AgentGetOperationStatus {
                operation_id: first_operation_id,
            })
            .await
            .expect("query operation status while agent-work is busy");

        match conn.recv_with_timeout(Duration::from_millis(250)).await {
            DaemonMessage::OperationStatus { snapshot } => {
                assert_eq!(snapshot.kind, "synthesize_tool");
                assert!(matches!(
                    snapshot.state,
                    amux_protocol::OperationLifecycleState::Accepted
                        | amux_protocol::OperationLifecycleState::Started
                ));
            }
            other => panic!(
                "expected OperationStatus under agent-work load, got {other:?}"
            ),
        }

        conn.framed
            .send(ClientMessage::Ping)
            .await
            .expect("send ping while agent-work and queries are active");

        match conn.recv_with_timeout(Duration::from_millis(250)).await {
            DaemonMessage::Pong => {}
            other => panic!("expected Pong under agent-work load, got {other:?}"),
        }

        conn.shutdown().await;
    }

    #[tokio::test]
    async fn skill_publish_async_request_does_not_block_ping() {
        let (registry_url, registry_task) =
            spawn_test_registry_publish_server(Duration::from_secs(1)).await;
        let mut config = AgentConfig::default();
        config.extra.insert(
            "registry_url".to_string(),
            serde_json::Value::String(registry_url),
        );
        let mut conn = spawn_test_connection_with_config(config).await;
        let variant_id = register_publishable_skill_variant(&conn, "publish-async", "proven").await;
        std::env::set_var("TAMUX_REGISTRY_TOKEN", "test-token");
        declare_async_command_capability(&mut conn).await;

        conn.framed
            .send(ClientMessage::SkillPublish {
                identifier: variant_id,
            })
            .await
            .expect("request async skill publish");

        let operation_id = match conn.recv_with_timeout(Duration::from_millis(250)).await {
            DaemonMessage::OperationAccepted { operation_id, kind, .. } => {
                assert_eq!(kind, "skill_publish");
                operation_id
            }
            other => panic!("expected skill publish operation acceptance, got {other:?}"),
        };

        conn.framed
            .send(ClientMessage::Ping)
            .await
            .expect("send ping while skill publish is active");

        let pong_received = timeout(Duration::from_millis(250), async {
            loop {
                match conn.recv_with_timeout(Duration::from_millis(250)).await {
                    DaemonMessage::Pong => return true,
                    DaemonMessage::SkillPublishResult { .. } => continue,
                    other => panic!(
                        "expected Pong while skill publish runs in background, got {other:?}"
                    ),
                }
            }
        })
        .await
        .unwrap_or(false);

        assert!(pong_received, "ping should not be blocked behind skill publish");

        conn.framed
            .send(ClientMessage::AgentGetOperationStatus { operation_id })
            .await
            .expect("query skill publish operation status");

        match conn.recv_with_timeout(Duration::from_secs(1)).await {
            DaemonMessage::OperationStatus { snapshot } => {
                assert_eq!(snapshot.kind, "skill_publish");
                assert!(matches!(
                    snapshot.state,
                    amux_protocol::OperationLifecycleState::Accepted
                        | amux_protocol::OperationLifecycleState::Started
                ));
            }
            other => panic!("expected skill publish status snapshot, got {other:?}"),
        }

        match conn.recv_with_timeout(Duration::from_secs(3)).await {
            DaemonMessage::SkillPublishResult { success, message } => {
                assert!(success, "expected successful publish result: {message}");
                assert!(message.contains("Published skill"));
            }
            other => panic!("expected skill publish result, got {other:?}"),
        }

        timeout(Duration::from_secs(2), registry_task)
            .await
            .expect("registry task timed out")
            .expect("registry task join failed");
        conn.shutdown().await;
    }

    #[tokio::test]
    async fn skill_publish_legacy_request_does_not_block_ping() {
        let (registry_url, registry_task) =
            spawn_test_registry_publish_server(Duration::from_secs(1)).await;
        let mut config = AgentConfig::default();
        config.extra.insert(
            "registry_url".to_string(),
            serde_json::Value::String(registry_url),
        );
        let mut conn = spawn_test_connection_with_config(config).await;
        let variant_id =
            register_publishable_skill_variant(&conn, "publish-legacy", "proven").await;
        std::env::set_var("TAMUX_REGISTRY_TOKEN", "test-token");

        conn.framed
            .send(ClientMessage::SkillPublish {
                identifier: variant_id,
            })
            .await
            .expect("request legacy skill publish");

        conn.framed
            .send(ClientMessage::Ping)
            .await
            .expect("send ping while legacy skill publish is active");

        let pong_received = timeout(Duration::from_millis(250), async {
            loop {
                match conn.recv_with_timeout(Duration::from_millis(250)).await {
                    DaemonMessage::Pong => return true,
                    DaemonMessage::OperationAccepted { .. } => {
                        panic!("legacy client should not receive operation acceptance")
                    }
                    DaemonMessage::SkillPublishResult { .. } => continue,
                    other => panic!(
                        "expected Pong while legacy skill publish runs in background, got {other:?}"
                    ),
                }
            }
        })
        .await
        .unwrap_or(false);

        assert!(pong_received, "ping should not be blocked behind legacy skill publish");

        match conn.recv_with_timeout(Duration::from_secs(3)).await {
            DaemonMessage::SkillPublishResult { success, message } => {
                assert!(success, "expected successful publish result: {message}");
                assert!(message.contains("Published skill"));
            }
            DaemonMessage::OperationAccepted { .. } => {
                panic!("legacy client should not receive operation acceptance")
            }
            other => panic!("expected legacy skill publish result, got {other:?}"),
        }

        timeout(Duration::from_secs(2), registry_task)
            .await
            .expect("registry task timed out")
            .expect("registry task join failed");
        conn.shutdown().await;
    }

    #[tokio::test]
    async fn skill_import_async_request_does_not_block_ping() {
        let (registry_url, registry_task) =
            spawn_test_registry_fetch_server("import-async", Duration::from_secs(1)).await;
        let mut config = AgentConfig::default();
        config.extra.insert(
            "registry_url".to_string(),
            serde_json::Value::String(registry_url),
        );
        let mut conn = spawn_test_connection_with_config(config).await;
        declare_async_command_capability(&mut conn).await;

        conn.framed
            .send(ClientMessage::SkillImport {
                source: "import-async".to_string(),
                force: true,
                publisher_verified: true,
            })
            .await
            .expect("request async skill import");

        let operation_id = match conn.recv_with_timeout(Duration::from_millis(250)).await {
            DaemonMessage::OperationAccepted { operation_id, kind, .. } => {
                assert_eq!(kind, "skill_import");
                operation_id
            }
            other => panic!("expected skill import operation acceptance, got {other:?}"),
        };

        conn.framed
            .send(ClientMessage::Ping)
            .await
            .expect("send ping while skill import is active");

        let pong_received = timeout(Duration::from_millis(250), async {
            loop {
                match conn.recv_with_timeout(Duration::from_millis(250)).await {
                    DaemonMessage::Pong => return true,
                    DaemonMessage::SkillImportResult { .. } => continue,
                    other => panic!(
                        "expected Pong while skill import runs in background, got {other:?}"
                    ),
                }
            }
        })
        .await
        .unwrap_or(false);

        assert!(pong_received, "ping should not be blocked behind skill import");

        conn.framed
            .send(ClientMessage::AgentGetOperationStatus { operation_id })
            .await
            .expect("query skill import operation status");

        match conn.recv_with_timeout(Duration::from_secs(1)).await {
            DaemonMessage::OperationStatus { snapshot } => {
                assert_eq!(snapshot.kind, "skill_import");
                assert!(matches!(
                    snapshot.state,
                    amux_protocol::OperationLifecycleState::Accepted
                        | amux_protocol::OperationLifecycleState::Started
                ));
            }
            other => panic!("expected skill import status snapshot, got {other:?}"),
        }

        match conn.recv_with_timeout(Duration::from_secs(3)).await {
            DaemonMessage::SkillImportResult {
                success,
                message,
                variant_id,
                scan_verdict,
                ..
            } => {
                assert!(success, "expected successful import result: {message}");
                assert!(variant_id.is_some(), "expected imported variant id");
                assert_eq!(scan_verdict.as_deref(), Some("warn"));
            }
            other => panic!("expected skill import result, got {other:?}"),
        }

        timeout(Duration::from_secs(2), registry_task)
            .await
            .expect("registry task timed out")
            .expect("registry task join failed");
        conn.shutdown().await;
    }

    #[tokio::test]
    async fn skill_import_legacy_request_does_not_block_ping() {
        let (registry_url, registry_task) =
            spawn_test_registry_fetch_server("import-legacy", Duration::from_secs(1)).await;
        let mut config = AgentConfig::default();
        config.extra.insert(
            "registry_url".to_string(),
            serde_json::Value::String(registry_url),
        );
        let mut conn = spawn_test_connection_with_config(config).await;

        conn.framed
            .send(ClientMessage::SkillImport {
                source: "import-legacy".to_string(),
                force: true,
                publisher_verified: true,
            })
            .await
            .expect("request legacy skill import");

        conn.framed
            .send(ClientMessage::Ping)
            .await
            .expect("send ping while legacy skill import is active");

        let pong_received = timeout(Duration::from_millis(250), async {
            loop {
                match conn.recv_with_timeout(Duration::from_millis(250)).await {
                    DaemonMessage::Pong => return true,
                    DaemonMessage::OperationAccepted { .. } => {
                        panic!("legacy client should not receive operation acceptance")
                    }
                    DaemonMessage::SkillImportResult { .. } => continue,
                    other => panic!(
                        "expected Pong while legacy skill import runs in background, got {other:?}"
                    ),
                }
            }
        })
        .await
        .unwrap_or(false);

        assert!(pong_received, "ping should not be blocked behind legacy skill import");

        match conn.recv_with_timeout(Duration::from_secs(3)).await {
            DaemonMessage::SkillImportResult {
                success,
                message,
                variant_id,
                scan_verdict,
                ..
            } => {
                assert!(success, "expected successful legacy import result: {message}");
                assert!(variant_id.is_some(), "expected imported variant id");
                assert_eq!(scan_verdict.as_deref(), Some("warn"));
            }
            DaemonMessage::OperationAccepted { .. } => {
                panic!("legacy client should not receive operation acceptance")
            }
            other => panic!("expected legacy skill import result, got {other:?}"),
        }

        timeout(Duration::from_secs(2), registry_task)
            .await
            .expect("registry task timed out")
            .expect("registry task join failed");
        conn.shutdown().await;
    }

    #[tokio::test]
    async fn synthesize_tool_async_request_does_not_block_ping() {
        let mut config = AgentConfig::default();
        config.tool_synthesis.enabled = true;
        let mut conn = spawn_test_connection_with_config(config).await;
        conn.agent
            .set_test_synthesize_tool_delay(Some(Duration::from_secs(1)))
            .await;
        declare_async_command_capability(&mut conn).await;

        conn.framed
            .send(ClientMessage::AgentSynthesizeTool {
                request_json: serde_json::json!({
                    "kind": "cli",
                    "target": "echo hello"
                })
                .to_string(),
            })
            .await
            .expect("request async synthesize tool");

        let operation_id = match conn.recv_with_timeout(Duration::from_millis(250)).await {
            DaemonMessage::OperationAccepted { operation_id, kind, .. } => {
                assert_eq!(kind, "synthesize_tool");
                operation_id
            }
            other => panic!("expected synthesize-tool operation acceptance, got {other:?}"),
        };

        conn.framed
            .send(ClientMessage::Ping)
            .await
            .expect("send ping while synthesize tool is active");

        let pong_received = timeout(Duration::from_millis(250), async {
            loop {
                match conn.recv_with_timeout(Duration::from_millis(250)).await {
                    DaemonMessage::Pong => return true,
                    DaemonMessage::AgentError { .. } => continue,
                    other => panic!(
                        "expected Pong while synthesize tool runs in background, got {other:?}"
                    ),
                }
            }
        })
        .await
        .unwrap_or(false);

        assert!(pong_received, "ping should not be blocked behind synthesize tool");

        conn.framed
            .send(ClientMessage::AgentGetOperationStatus { operation_id })
            .await
            .expect("query synthesize tool operation status");

        match conn.recv_with_timeout(Duration::from_secs(1)).await {
            DaemonMessage::OperationStatus { snapshot } => {
                assert_eq!(snapshot.kind, "synthesize_tool");
                assert!(matches!(
                    snapshot.state,
                    amux_protocol::OperationLifecycleState::Accepted
                        | amux_protocol::OperationLifecycleState::Started
                ));
            }
            other => panic!("expected synthesize tool status snapshot, got {other:?}"),
        }

        match conn.recv_with_timeout(Duration::from_secs(3)).await {
            DaemonMessage::AgentError { message } => {
                assert!(message.contains("failed to synthesize generated tool"));
                assert!(message.contains("timed out"));
            }
            other => panic!("expected synthesize tool error result, got {other:?}"),
        }

        conn.shutdown().await;
    }

    #[tokio::test]
    async fn synthesize_tool_legacy_request_does_not_block_ping() {
        let mut config = AgentConfig::default();
        config.tool_synthesis.enabled = true;
        let mut conn = spawn_test_connection_with_config(config).await;
        conn.agent
            .set_test_synthesize_tool_delay(Some(Duration::from_secs(1)))
            .await;

        conn.framed
            .send(ClientMessage::AgentSynthesizeTool {
                request_json: serde_json::json!({
                    "kind": "cli",
                    "target": "echo hello"
                })
                .to_string(),
            })
            .await
            .expect("request legacy synthesize tool");

        conn.framed
            .send(ClientMessage::Ping)
            .await
            .expect("send ping while legacy synthesize tool is active");

        let pong_received = timeout(Duration::from_millis(250), async {
            loop {
                match conn.recv_with_timeout(Duration::from_millis(250)).await {
                    DaemonMessage::Pong => return true,
                    DaemonMessage::OperationAccepted { .. } => {
                        panic!("legacy client should not receive operation acceptance")
                    }
                    DaemonMessage::AgentError { .. } => continue,
                    other => panic!(
                        "expected Pong while legacy synthesize tool runs in background, got {other:?}"
                    ),
                }
            }
        })
        .await
        .unwrap_or(false);

        assert!(pong_received, "ping should not be blocked behind legacy synthesize tool");

        match conn.recv_with_timeout(Duration::from_secs(3)).await {
            DaemonMessage::AgentError { message } => {
                assert!(message.contains("failed to synthesize generated tool"));
                assert!(message.contains("timed out"));
            }
            DaemonMessage::OperationAccepted { .. } => {
                panic!("legacy client should not receive operation acceptance")
            }
            other => panic!("expected legacy synthesize tool error result, got {other:?}"),
        }

        conn.shutdown().await;
    }

    #[tokio::test]
    async fn divergent_session_async_request_does_not_block_ping() {
        let mut conn = spawn_test_connection().await;
        conn.agent
            .set_test_divergent_session_delay(Some(Duration::from_secs(1)))
            .await;
        declare_async_command_capability(&mut conn).await;

        conn.framed
            .send(ClientMessage::AgentStartDivergentSession {
                problem_statement: "compare rollout options".to_string(),
                thread_id: "thread-div-async".to_string(),
                goal_run_id: None,
                custom_framings_json: None,
            })
            .await
            .expect("request async divergent session");

        let operation_id = match conn.recv_with_timeout(Duration::from_millis(250)).await {
            DaemonMessage::OperationAccepted { operation_id, kind, .. } => {
                assert_eq!(kind, "start_divergent_session");
                operation_id
            }
            other => panic!("expected divergent-session operation acceptance, got {other:?}"),
        };

        conn.framed
            .send(ClientMessage::Ping)
            .await
            .expect("send ping while divergent session is active");

        let pong_received = timeout(Duration::from_millis(250), async {
            loop {
                match conn.recv_with_timeout(Duration::from_millis(250)).await {
                    DaemonMessage::Pong => return true,
                    DaemonMessage::AgentDivergentSessionStarted { .. } => continue,
                    other => panic!(
                        "expected Pong while divergent session runs in background, got {other:?}"
                    ),
                }
            }
        })
        .await
        .unwrap_or(false);

        assert!(pong_received, "ping should not be blocked behind divergent session startup");

        conn.framed
            .send(ClientMessage::AgentGetOperationStatus { operation_id })
            .await
            .expect("query divergent session operation status");

        match conn.recv_with_timeout(Duration::from_secs(1)).await {
            DaemonMessage::OperationStatus { snapshot } => {
                assert_eq!(snapshot.kind, "start_divergent_session");
                assert!(matches!(
                    snapshot.state,
                    amux_protocol::OperationLifecycleState::Accepted
                        | amux_protocol::OperationLifecycleState::Started
                ));
            }
            other => panic!("expected divergent session status snapshot, got {other:?}"),
        }

        match conn.recv_with_timeout(Duration::from_secs(3)).await {
            DaemonMessage::AgentDivergentSessionStarted { session_json } => {
                let payload: serde_json::Value =
                    serde_json::from_str(&session_json).expect("valid divergent session payload");
                assert_eq!(payload["status"], "started");
                assert!(payload["session_id"].as_str().is_some());
            }
            other => panic!("expected divergent session result, got {other:?}"),
        }

        conn.shutdown().await;
    }

    #[tokio::test]
    async fn divergent_session_legacy_request_does_not_block_ping() {
        let mut conn = spawn_test_connection().await;
        conn.agent
            .set_test_divergent_session_delay(Some(Duration::from_secs(1)))
            .await;

        conn.framed
            .send(ClientMessage::AgentStartDivergentSession {
                problem_statement: "compare rollback options".to_string(),
                thread_id: "thread-div-legacy".to_string(),
                goal_run_id: None,
                custom_framings_json: None,
            })
            .await
            .expect("request legacy divergent session");

        conn.framed
            .send(ClientMessage::Ping)
            .await
            .expect("send ping while legacy divergent session is active");

        let pong_received = timeout(Duration::from_millis(250), async {
            loop {
                match conn.recv_with_timeout(Duration::from_millis(250)).await {
                    DaemonMessage::Pong => return true,
                    DaemonMessage::OperationAccepted { .. } => {
                        panic!("legacy client should not receive operation acceptance")
                    }
                    DaemonMessage::AgentDivergentSessionStarted { .. } => continue,
                    other => panic!(
                        "expected Pong while legacy divergent session runs in background, got {other:?}"
                    ),
                }
            }
        })
        .await
        .unwrap_or(false);

        assert!(pong_received, "ping should not be blocked behind legacy divergent session startup");

        match conn.recv_with_timeout(Duration::from_secs(3)).await {
            DaemonMessage::AgentDivergentSessionStarted { session_json } => {
                let payload: serde_json::Value =
                    serde_json::from_str(&session_json).expect("valid divergent session payload");
                assert_eq!(payload["status"], "started");
                assert!(payload["session_id"].as_str().is_some());
            }
            DaemonMessage::OperationAccepted { .. } => {
                panic!("legacy client should not receive operation acceptance")
            }
            other => panic!("expected legacy divergent session result, got {other:?}"),
        }

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
