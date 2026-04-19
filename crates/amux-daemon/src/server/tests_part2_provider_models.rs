#[tokio::test]
async fn concierge_welcome_does_not_delay_fetch_models_acceptance() {
    let concierge_listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind fake concierge listener");
    let concierge_addr = concierge_listener
        .local_addr()
        .expect("concierge listener addr");
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
            output_modalities: None,
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
                other => panic!(
                    "expected fetch-models acceptance while concierge is active, got {other:?}"
                ),
            }
        }
    })
    .await
    .unwrap_or(false);

    assert!(
        models_accepted,
        "fetch-models should be accepted while concierge is active"
    );

    conn.framed
        .send(ClientMessage::Ping)
        .await
        .expect("send ping while concierge and fetch-models are active");

    let pong_received = timeout(Duration::from_millis(500), async {
        loop {
            match conn.recv_with_timeout(Duration::from_millis(500)).await {
                DaemonMessage::Pong => return true,
                DaemonMessage::AgentEvent { .. } => continue,
                other => panic!(
                    "expected Pong while concierge and fetch-models are active, got {other:?}"
                ),
            }
        }
    })
    .await
    .unwrap_or(false);

    assert!(
        pong_received,
        "ping should not be blocked while concierge and fetch-models are active"
    );

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
        DaemonMessage::OperationAccepted {
            operation_id, kind, ..
        } => {
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
async fn provider_validation_async_request_returns_correlated_terminal_result() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind fake provider listener");
    let addr = listener.local_addr().expect("listener addr");

    let provider_response = b"HTTP/1.1 200 OK\r\ncontent-length: 0\r\nconnection: close\r\n\r\n";
    let accept_task = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.expect("accept provider request");
        let mut buf = [0_u8; 1024];
        let _ = tokio::time::timeout(
            Duration::from_secs(1),
            tokio::io::AsyncReadExt::read(&mut stream, &mut buf),
        )
        .await;
        tokio::io::AsyncWriteExt::write_all(&mut stream, provider_response)
            .await
            .expect("write provider response");
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

    let operation_id = match conn.recv_with_timeout(Duration::from_secs(2)).await {
        DaemonMessage::OperationAccepted {
            operation_id, kind, ..
        } => {
            assert_eq!(kind, "provider_validation");
            operation_id
        }
        other => panic!("expected provider validation acceptance, got {other:?}"),
    };

    let result = timeout(Duration::from_secs(2), async {
        loop {
            match conn.recv().await {
                DaemonMessage::AgentProviderValidation {
                    operation_id: result_operation_id,
                    provider_id,
                    valid,
                    ..
                } => {
                    assert_eq!(result_operation_id.as_deref(), Some(operation_id.as_str()));
                    assert_eq!(provider_id, "openai");
                    assert!(valid);
                    return;
                }
                DaemonMessage::OperationStatus { .. } => continue,
                other => panic!("expected provider validation terminal result, got {other:?}"),
            }
        }
    })
    .await;

    assert!(
        result.is_ok(),
        "provider validation should complete with correlation"
    );

    accept_task.await.expect("provider server task");
    conn.shutdown().await;
}

#[tokio::test]
async fn provider_validation_request_without_declared_capability_still_returns_operation_acceptance(
) {
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

    let operation_id = match conn.recv().await {
        DaemonMessage::OperationAccepted {
            operation_id, kind, ..
        } => {
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
                DaemonMessage::AgentProviderValidation {
                    operation_id: result_operation_id,
                    ..
                } => {
                    assert_eq!(result_operation_id.as_deref(), Some(operation_id.as_str()));
                    continue;
                }
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
        "ping should not be blocked for provider validation without declared capability"
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
            output_modalities: None,
        })
        .await
        .expect("request models fetch");

    let operation_id = match conn.recv().await {
        DaemonMessage::OperationAccepted {
            operation_id, kind, ..
        } => {
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
                    panic!("expected Pong while fetch-models runs in background, got {other:?}")
                }
            }
        }
    })
    .await
    .unwrap_or(false);

    assert!(
        pong_received,
        "ping should not be blocked behind fetch-models"
    );

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
async fn fetch_models_async_request_returns_correlated_terminal_result() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind fake models listener");
    let addr = listener.local_addr().expect("listener addr");

    let models_body = r#"{"data":[{"id":"gpt-5.4","object":"model"}]}"#;
    let accept_task = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.expect("accept models request");
        let mut buf = [0_u8; 1024];
        let _ = tokio::time::timeout(
            Duration::from_secs(1),
            tokio::io::AsyncReadExt::read(&mut stream, &mut buf),
        )
        .await;
        let models_response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                models_body.len(),
                models_body
            );
        tokio::io::AsyncWriteExt::write_all(&mut stream, models_response.as_bytes())
            .await
            .expect("write models response");
    });

    let mut conn = spawn_test_connection().await;
    declare_async_command_capability(&mut conn).await;

    conn.framed
        .send(ClientMessage::AgentFetchModels {
            provider_id: "openai".to_string(),
            base_url: format!("http://{addr}"),
            api_key: "test-key".to_string(),
            output_modalities: None,
        })
        .await
        .expect("request models fetch");

    let operation_id = match conn.recv_with_timeout(Duration::from_secs(2)).await {
        DaemonMessage::OperationAccepted {
            operation_id, kind, ..
        } => {
            assert_eq!(kind, "fetch_models");
            operation_id
        }
        other => panic!("expected fetch-models acceptance, got {other:?}"),
    };

    let result = timeout(Duration::from_secs(2), async {
        loop {
            match conn.recv().await {
                DaemonMessage::AgentModelsResponse {
                    operation_id: result_operation_id,
                    models_json,
                } => {
                    assert_eq!(result_operation_id.as_deref(), Some(operation_id.as_str()));
                    assert!(models_json.contains("gpt-5.4"));
                    return;
                }
                DaemonMessage::OperationStatus { .. } => continue,
                other => panic!("expected models terminal result, got {other:?}"),
            }
        }
    })
    .await;

    assert!(
        result.is_ok(),
        "fetch-models should complete with correlation"
    );

    accept_task.await.expect("models server task");
    conn.shutdown().await;
}

#[tokio::test]
async fn fetch_models_request_without_declared_capability_still_returns_operation_acceptance() {
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
            output_modalities: None,
        })
        .await
        .expect("request models fetch");

    let operation_id = match conn.recv().await {
        DaemonMessage::OperationAccepted {
            operation_id, kind, ..
        } => {
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
                DaemonMessage::AgentModelsResponse {
                    operation_id: result_operation_id,
                    ..
                } => {
                    assert_eq!(result_operation_id.as_deref(), Some(operation_id.as_str()));
                    continue;
                }
                other => {
                    panic!("expected Pong while fetch-models runs in background, got {other:?}")
                }
            }
        }
    })
    .await
    .unwrap_or(false);

    assert!(
        pong_received,
        "ping should not be blocked for fetch-models without declared capability"
    );

    accept_task.abort();
    conn.shutdown().await;
}

#[tokio::test]
async fn unsupported_provider_fetch_models_returns_empty_response_instead_of_error() {
    let mut conn = spawn_test_connection().await;

    conn.framed
        .send(ClientMessage::AgentFetchModels {
            provider_id: "featherless".to_string(),
            base_url: "http://127.0.0.1:9".to_string(),
            api_key: String::new(),
            output_modalities: None,
        })
        .await
        .expect("request unsupported provider models fetch");

    let operation_id = match conn.recv().await {
        DaemonMessage::OperationAccepted {
            operation_id, kind, ..
        } => {
            assert_eq!(kind, "fetch_models");
            operation_id
        }
        other => panic!("expected fetch-models acceptance, got {other:?}"),
    };

    match conn.recv().await {
        DaemonMessage::AgentModelsResponse {
            operation_id: result_operation_id,
            models_json,
        } => {
            assert_eq!(result_operation_id.as_deref(), Some(operation_id.as_str()));
            let models: Vec<serde_json::Value> =
                serde_json::from_str(&models_json).expect("parse models response");
            assert!(models.is_empty(), "unsupported providers should return no models");
        }
        DaemonMessage::AgentError { message } => {
            panic!("unsupported providers should not surface an agent error: {message}");
        }
        other => panic!("expected empty models response, got {other:?}"),
    }

    conn.shutdown().await;
}
