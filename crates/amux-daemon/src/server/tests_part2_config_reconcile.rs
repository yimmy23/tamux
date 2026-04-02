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
            other => panic!(
                "expected provider validation acceptance during saturation setup, got {other:?}"
            ),
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
        other => panic!(
            "expected agent-work acceptance while provider queue is saturated, got {other:?}"
        ),
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
                DaemonMessage::OperationAccepted { kind, .. } => {
                    assert_eq!(kind, "config_set_item");
                }
                other => {
                    panic!("expected Pong while config reconcile runs in background, got {other:?}")
                }
            }
        }
    })
    .await
    .unwrap_or(false);

    assert!(
        pong_received,
        "ping should not be blocked behind config reconcile"
    );

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
                    DaemonMessage::OperationAccepted { kind, .. } => {
                        assert_eq!(kind, "set_provider_model");
                    }
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
            operation_id, kind, ..
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
            operation_id, kind, ..
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
        other => panic!("expected Pong while async provider-model reconcile runs, got {other:?}"),
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

    match conn.recv_with_timeout(Duration::from_millis(250)).await {
        DaemonMessage::OperationAccepted { kind, .. } => {
            assert_eq!(kind, "config_set_item");
        }
        other => panic!("expected config-set operation acceptance, got {other:?}"),
    }

    conn.framed
        .send(ClientMessage::AgentGetConfig)
        .await
        .expect("query desired config while reconcile is active");

    let config_json = match conn.recv_with_timeout(Duration::from_millis(250)).await {
        DaemonMessage::AgentConfigResponse { config_json } => config_json,
        other => panic!("expected AgentConfigResponse while reconcile is active, got {other:?}"),
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
        other => {
            panic!("expected AgentEffectiveConfigState while reconcile is active, got {other:?}")
        }
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
        other => panic!("expected Pong while config queries run during reconcile, got {other:?}"),
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
            operation_id, kind, ..
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
async fn config_set_item_request_without_declared_capability_still_returns_operation_acceptance() {
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
        .expect("request config item update without declared capability");

    let operation_id = match conn.recv_with_timeout(Duration::from_secs(2)).await {
        DaemonMessage::OperationAccepted {
            operation_id, kind, ..
        } => {
            assert_eq!(kind, "config_set_item");
            operation_id
        }
        other => panic!("expected config-set operation acceptance, got {other:?}"),
    };

    conn.framed
        .send(ClientMessage::Ping)
        .await
        .expect("send ping while legacy config reconcile is active");

    match conn.recv_with_timeout(Duration::from_millis(250)).await {
        DaemonMessage::Pong => {}
        other => panic!("expected Pong while legacy config reconcile runs, got {other:?}"),
    }

    conn.framed
        .send(ClientMessage::AgentGetOperationStatus { operation_id })
        .await
        .expect("query config-set operation status");

    match conn.recv_with_timeout(Duration::from_secs(2)).await {
        DaemonMessage::OperationStatus { snapshot } => {
            assert_eq!(snapshot.kind, "config_set_item");
        }
        other => panic!("expected config-set operation status, got {other:?}"),
    }

    conn.shutdown().await;
}

#[tokio::test]
async fn provider_model_request_without_declared_capability_still_returns_operation_acceptance() {
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
        .expect("request provider/model update without declared capability");

    let operation_id = match conn.recv_with_timeout(Duration::from_secs(2)).await {
        DaemonMessage::OperationAccepted {
            operation_id, kind, ..
        } => {
            assert_eq!(kind, "set_provider_model");
            operation_id
        }
        other => panic!("expected provider-model operation acceptance, got {other:?}"),
    };

    conn.framed
        .send(ClientMessage::Ping)
        .await
        .expect("send ping while legacy provider-model reconcile is active");

    match conn.recv_with_timeout(Duration::from_millis(250)).await {
        DaemonMessage::Pong => {}
        other => panic!("expected Pong while legacy provider-model reconcile runs, got {other:?}"),
    }

    conn.framed
        .send(ClientMessage::AgentGetOperationStatus { operation_id })
        .await
        .expect("query provider-model operation status");

    match conn.recv_with_timeout(Duration::from_secs(2)).await {
        DaemonMessage::OperationStatus { snapshot } => {
            assert_eq!(snapshot.kind, "set_provider_model");
        }
        other => panic!("expected provider-model operation status, got {other:?}"),
    }

    conn.shutdown().await;
}
