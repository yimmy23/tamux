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
                    "target": format!("ls hello-{idx}")
                })
                .to_string(),
            })
            .await
            .expect("request synthesize tool during saturation setup");

        match conn.recv_with_timeout(Duration::from_secs(2)).await {
            DaemonMessage::OperationAccepted { kind, .. } => {
                assert_eq!(kind, "synthesize_tool");
            }
            other => {
                panic!("expected synthesize-tool acceptance during saturation setup, got {other:?}")
            }
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
        other => panic!(
            "expected provider acceptance while agent-work queue is saturated, got {other:?}"
        ),
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
                    "target": "ls first"
                })
                .to_string(),
            })
            .await
            .expect("request first synthesize tool");

        match conn.recv_with_timeout(Duration::from_secs(2)).await {
            DaemonMessage::OperationAccepted {
                operation_id, kind, ..
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
            other => panic!("expected synthesize-tool acceptance during load setup, got {other:?}"),
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
        other => panic!("expected OperationStatus under agent-work load, got {other:?}"),
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
    let _env_lock = crate::test_support::env_test_lock();
    let _env_guard = crate::test_support::EnvGuard::new(&["TAMUX_REGISTRY_TOKEN"]);
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
        DaemonMessage::OperationAccepted {
            operation_id, kind, ..
        } => {
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
                other => {
                    panic!("expected Pong while skill publish runs in background, got {other:?}")
                }
            }
        }
    })
    .await
    .unwrap_or(false);

    assert!(
        pong_received,
        "ping should not be blocked behind skill publish"
    );

    conn.framed
        .send(ClientMessage::AgentGetOperationStatus {
            operation_id: operation_id.clone(),
        })
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
        DaemonMessage::SkillPublishResult {
            operation_id: result_operation_id,
            success,
            message,
        } => {
            assert_eq!(result_operation_id.as_deref(), Some(operation_id.as_str()));
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
    let _env_lock = crate::test_support::env_test_lock();
    let _env_guard = crate::test_support::EnvGuard::new(&["TAMUX_REGISTRY_TOKEN"]);
    let (registry_url, registry_task) =
        spawn_test_registry_publish_server(Duration::from_secs(1)).await;
    let mut config = AgentConfig::default();
    config.extra.insert(
        "registry_url".to_string(),
        serde_json::Value::String(registry_url),
    );
    let mut conn = spawn_test_connection_with_config(config).await;
    let variant_id = register_publishable_skill_variant(&conn, "publish-legacy", "proven").await;
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

    let mut operation_id = None;
    let pong_received = timeout(Duration::from_millis(250), async {
        loop {
            match conn.recv_with_timeout(Duration::from_millis(250)).await {
                DaemonMessage::Pong => return true,
                DaemonMessage::OperationAccepted {
                    operation_id: accepted_operation_id,
                    kind,
                    ..
                } => {
                    assert_eq!(kind, "skill_publish");
                    operation_id = Some(accepted_operation_id);
                    continue;
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

    assert!(
        pong_received,
        "ping should not be blocked behind legacy skill publish"
    );
    assert!(
        operation_id.is_some(),
        "skill publish should now return operation acceptance even without capability declaration"
    );

    match conn.recv_with_timeout(Duration::from_secs(3)).await {
        DaemonMessage::SkillPublishResult {
            operation_id: result_operation_id,
            success,
            message,
        } => {
            assert_eq!(result_operation_id.as_deref(), operation_id.as_deref());
            assert!(success, "expected successful publish result: {message}");
            assert!(message.contains("Published skill"));
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
        DaemonMessage::OperationAccepted {
            operation_id, kind, ..
        } => {
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
                other => {
                    panic!("expected Pong while skill import runs in background, got {other:?}")
                }
            }
        }
    })
    .await
    .unwrap_or(false);

    assert!(
        pong_received,
        "ping should not be blocked behind skill import"
    );

    conn.framed
        .send(ClientMessage::AgentGetOperationStatus {
            operation_id: operation_id.clone(),
        })
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
            operation_id: result_operation_id,
            success,
            message,
            variant_id,
            scan_verdict,
            ..
        } => {
            assert_eq!(result_operation_id.as_deref(), Some(operation_id.as_str()));
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

    let mut operation_id = None;
    let pong_received = timeout(Duration::from_millis(250), async {
        loop {
            match conn.recv_with_timeout(Duration::from_millis(250)).await {
                DaemonMessage::Pong => return true,
                DaemonMessage::OperationAccepted {
                    operation_id: accepted_operation_id,
                    kind,
                    ..
                } => {
                    assert_eq!(kind, "skill_import");
                    operation_id = Some(accepted_operation_id);
                    continue;
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

    assert!(
        pong_received,
        "ping should not be blocked behind legacy skill import"
    );
    assert!(
        operation_id.is_some(),
        "skill import should now return operation acceptance even without capability declaration"
    );

    match conn.recv_with_timeout(Duration::from_secs(3)).await {
        DaemonMessage::SkillImportResult {
            operation_id: result_operation_id,
            success,
            message,
            variant_id,
            scan_verdict,
            ..
        } => {
            assert_eq!(result_operation_id.as_deref(), operation_id.as_deref());
            assert!(
                success,
                "expected successful legacy import result: {message}"
            );
            assert!(variant_id.is_some(), "expected imported variant id");
            assert_eq!(scan_verdict.as_deref(), Some("warn"));
        }
        other => panic!("expected legacy skill import result, got {other:?}"),
    }

    timeout(Duration::from_secs(2), registry_task)
        .await
        .expect("registry task timed out")
        .expect("registry task join failed");
    conn.shutdown().await;
}
