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
        DaemonMessage::OperationAccepted {
            operation_id, kind, ..
        } => {
            assert_eq!(kind, "explain_action");
            operation_id
        }
        other => panic!("expected explain-action acceptance, got {other:?}"),
    };

    conn.framed
        .send(ClientMessage::AgentGetOperationStatus {
            operation_id: operation_id.clone(),
        })
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

    match conn.recv().await {
        DaemonMessage::AgentExplanation {
            operation_id: result_operation_id,
            explanation_json,
        } => {
            assert_eq!(result_operation_id.as_deref(), Some(operation_id.as_str()));
            let payload: serde_json::Value =
                serde_json::from_str(&explanation_json).expect("valid explanation payload");
            assert_eq!(payload["action_id"], "missing-action");
        }
        other => panic!("expected correlated explain-action payload, got {other:?}"),
    }

    conn.shutdown().await;
}

#[tokio::test]
async fn explain_action_request_without_declared_capability_still_returns_operation_acceptance() {
    let mut conn = spawn_test_connection().await;

    conn.framed
        .send(ClientMessage::AgentExplainAction {
            action_id: "missing-action".to_string(),
            step_index: None,
        })
        .await
        .expect("request explain action");

    let operation_id = match conn.recv().await {
        DaemonMessage::OperationAccepted {
            operation_id, kind, ..
        } => {
            assert_eq!(kind, "explain_action");
            operation_id
        }
        other => panic!("expected explain-action acceptance, got {other:?}"),
    };

    match conn.recv().await {
        DaemonMessage::AgentExplanation {
            operation_id: result_operation_id,
            explanation_json,
        } => {
            assert_eq!(result_operation_id.as_deref(), Some(operation_id.as_str()));
            let payload: serde_json::Value =
                serde_json::from_str(&explanation_json).expect("valid explanation payload");
            assert_eq!(payload["action_id"], "missing-action");
            assert_eq!(payload["source"], "fallback");
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

    let first = conn.recv_with_timeout(Duration::from_secs(2)).await;
    let (operation_id, auth_url) = match first {
        DaemonMessage::OperationAccepted {
            operation_id, kind, ..
        } => {
            assert_eq!(kind, "plugin_oauth_start");
            match conn.recv_with_timeout(Duration::from_secs(2)).await {
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
            match conn.recv_with_timeout(Duration::from_secs(2)).await {
                DaemonMessage::Pong => return true,
                DaemonMessage::PluginOAuthComplete { .. } => continue,
                other => {
                    panic!("expected Pong while plugin oauth runs in background, got {other:?}")
                }
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
    assert!(
        pong_received,
        "ping should not be blocked behind plugin oauth flow"
    );

    if let Some(ref operation_id) = operation_id {
        conn.framed
            .send(ClientMessage::AgentGetOperationStatus {
                operation_id: operation_id.clone(),
            })
            .await
            .expect("query plugin oauth status");

        match conn.recv_with_timeout(Duration::from_secs(2)).await {
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
            match conn.recv_with_timeout(Duration::from_secs(2)).await {
                DaemonMessage::PluginOAuthComplete {
                    operation_id: result_operation_id,
                    name,
                    ..
                } => {
                    assert_eq!(result_operation_id.as_deref(), operation_id.as_deref());
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
async fn plugin_oauth_request_without_declared_capability_still_returns_operation_acceptance() {
    let mut conn = spawn_test_connection().await;
    register_test_oauth_plugin(&conn, "oauth-test-legacy").await;

    conn.framed
        .send(ClientMessage::PluginOAuthStart {
            name: "oauth-test-legacy".to_string(),
        })
        .await
        .expect("request plugin oauth start without declared capability");

    let (operation_id, auth_url) = match conn.recv_with_timeout(Duration::from_secs(2)).await {
        DaemonMessage::OperationAccepted {
            operation_id, kind, ..
        } => {
            assert_eq!(kind, "plugin_oauth_start");
            match conn.recv_with_timeout(Duration::from_secs(2)).await {
                DaemonMessage::PluginOAuthUrl { name, url } => {
                    assert_eq!(name, "oauth-test-legacy");
                    (operation_id, url)
                }
                other => panic!("expected plugin oauth url after acceptance, got {other:?}"),
            }
        }
        other => panic!("expected plugin oauth acceptance, got {other:?}"),
    };

    conn.framed
        .send(ClientMessage::Ping)
        .await
        .expect("send ping while legacy oauth flow is active");

    let pong_received = timeout(Duration::from_millis(250), async {
        loop {
            match conn.recv_with_timeout(Duration::from_secs(2)).await {
                DaemonMessage::Pong => return true,
                DaemonMessage::PluginOAuthComplete { .. } => continue,
                other => {
                    panic!("expected Pong while plugin oauth runs in background, got {other:?}")
                }
            }
        }
    })
    .await
    .unwrap_or(false);

    complete_test_oauth_callback(&auth_url).await;

    assert!(
        pong_received,
        "ping should not be blocked behind plugin oauth flow without declared capability"
    );

    let _ = timeout(Duration::from_secs(1), async {
        loop {
            match conn.recv_with_timeout(Duration::from_secs(2)).await {
                DaemonMessage::PluginOAuthComplete {
                    operation_id: result_operation_id,
                    name,
                    ..
                } => {
                    assert_eq!(result_operation_id.as_deref(), Some(operation_id.as_str()));
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
            operation_id, kind, ..
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
                other => {
                    panic!("expected Pong while plugin api call runs in background, got {other:?}")
                }
            }
        }
    })
    .await
    .unwrap_or(false);

    assert!(
        pong_received,
        "ping should not be blocked behind plugin api call"
    );

    conn.framed
        .send(ClientMessage::AgentGetOperationStatus {
            operation_id: operation_id.clone(),
        })
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
                    operation_id: result_operation_id,
                    plugin_name,
                    endpoint_name,
                    success,
                    error_type,
                    ..
                } => {
                    assert_eq!(result_operation_id.as_deref(), Some(operation_id.as_str()));
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
async fn plugin_api_call_request_without_declared_capability_still_returns_operation_acceptance() {
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
        .expect("request plugin api call without declared capability");

    let operation_id = match conn.recv_with_timeout(Duration::from_secs(2)).await {
        DaemonMessage::OperationAccepted {
            operation_id, kind, ..
        } => {
            assert_eq!(kind, "plugin_api_call");
            operation_id
        }
        other => panic!("expected plugin api operation acceptance, got {other:?}"),
    };

    conn.framed
        .send(ClientMessage::Ping)
        .await
        .expect("send ping while legacy plugin api call is active");

    let pong_received = timeout(Duration::from_millis(250), async {
        loop {
            match conn.recv_with_timeout(Duration::from_millis(250)).await {
                DaemonMessage::Pong => return true,
                DaemonMessage::PluginApiCallResult { .. } => continue,
                other => {
                    panic!("expected Pong while plugin api call runs in background, got {other:?}")
                }
            }
        }
    })
    .await
    .unwrap_or(false);

    assert!(
        pong_received,
        "ping should not be blocked behind plugin api call without declared capability"
    );

    let result = timeout(Duration::from_secs(6), async {
        loop {
            match conn.recv_with_timeout(Duration::from_secs(6)).await {
                DaemonMessage::PluginApiCallResult {
                    operation_id: result_operation_id,
                    plugin_name,
                    endpoint_name,
                    success,
                    error_type,
                    ..
                } => {
                    assert_eq!(result_operation_id.as_deref(), Some(operation_id.as_str()));
                    assert_eq!(plugin_name, "api-test-legacy");
                    assert_eq!(endpoint_name, "slow");
                    assert!(!success);
                    assert_eq!(error_type.as_deref(), Some("timeout"));
                    return;
                }
                DaemonMessage::Pong => continue,
                other => panic!("expected plugin api result during cleanup, got {other:?}"),
            }
        }
    })
    .await;

    assert!(
        result.is_ok(),
        "plugin api result should eventually arrive without declared capability"
    );
    conn.shutdown().await;
}
