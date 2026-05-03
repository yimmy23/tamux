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
                "target": "ls"
            })
            .to_string(),
        })
        .await
        .expect("request async synthesize tool");

    let operation_id = match conn.recv_with_timeout(Duration::from_millis(250)).await {
        DaemonMessage::OperationAccepted {
            operation_id, kind, ..
        } => {
            assert_eq!(kind, zorai_protocol::tool_names::SYNTHESIZE_TOOL);
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
                other => {
                    panic!("expected Pong while synthesize tool runs in background, got {other:?}")
                }
            }
        }
    })
    .await
    .unwrap_or(false);

    assert!(
        pong_received,
        "ping should not be blocked behind synthesize tool"
    );

    conn.framed
        .send(ClientMessage::AgentGetOperationStatus { operation_id })
        .await
        .expect("query synthesize tool operation status");

    match conn.recv_with_timeout(Duration::from_secs(1)).await {
        DaemonMessage::OperationStatus { snapshot } => {
            assert_eq!(snapshot.kind, zorai_protocol::tool_names::SYNTHESIZE_TOOL);
            assert!(matches!(
                snapshot.state,
                zorai_protocol::OperationLifecycleState::Accepted
                    | zorai_protocol::OperationLifecycleState::Started
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
async fn synthesize_tool_async_request_returns_correlated_terminal_result() {
    let mut config = AgentConfig::default();
    config.tool_synthesis.enabled = true;
    let mut conn = spawn_test_connection_with_config(config).await;
    declare_async_command_capability(&mut conn).await;

    conn.framed
        .send(ClientMessage::AgentSynthesizeTool {
            request_json: serde_json::json!({
                "kind": "cli",
                "target": "ls"
            })
            .to_string(),
        })
        .await
        .expect("request async synthesize tool");

    let operation_id = match conn.recv_with_timeout(Duration::from_secs(1)).await {
        DaemonMessage::OperationAccepted {
            operation_id, kind, ..
        } => {
            assert_eq!(kind, zorai_protocol::tool_names::SYNTHESIZE_TOOL);
            operation_id
        }
        other => panic!("expected synthesize-tool operation acceptance, got {other:?}"),
    };

    conn.framed
        .send(ClientMessage::AgentGetOperationStatus {
            operation_id: operation_id.clone(),
        })
        .await
        .expect("query synthesize tool operation status");

    match conn.recv_with_timeout(Duration::from_secs(1)).await {
        DaemonMessage::OperationStatus { snapshot } => {
            assert_eq!(snapshot.kind, zorai_protocol::tool_names::SYNTHESIZE_TOOL);
            assert!(matches!(
                snapshot.state,
                zorai_protocol::OperationLifecycleState::Accepted
                    | zorai_protocol::OperationLifecycleState::Started
                    | zorai_protocol::OperationLifecycleState::Completed
                    | zorai_protocol::OperationLifecycleState::Failed
            ));
        }
        other => panic!("expected synthesize tool status snapshot, got {other:?}"),
    }

    match conn.recv_with_timeout(Duration::from_secs(3)).await {
        DaemonMessage::AgentGeneratedToolResult {
            operation_id: result_operation_id,
            tool_name,
            result_json,
        } => {
            assert_eq!(result_operation_id.as_deref(), Some(operation_id.as_str()));
            assert!(tool_name.is_none());
            let payload: serde_json::Value =
                serde_json::from_str(&result_json).expect("valid generated tool payload");
            assert_eq!(payload["status"], "new");
            assert_eq!(payload["kind"], "cli");
        }
        other => panic!("expected synthesize tool result, got {other:?}"),
    }

    conn.shutdown().await;
}

#[tokio::test]
async fn synthesize_tool_request_without_declared_capability_still_returns_operation_acceptance() {
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
                "target": "ls"
            })
            .to_string(),
        })
        .await
        .expect("request synthesize tool without declared capability");

    let operation_id = match conn.recv_with_timeout(Duration::from_millis(250)).await {
        DaemonMessage::OperationAccepted {
            operation_id, kind, ..
        } => {
            assert_eq!(kind, zorai_protocol::tool_names::SYNTHESIZE_TOOL);
            operation_id
        }
        other => panic!("expected synthesize-tool operation acceptance, got {other:?}"),
    };

    conn.framed
        .send(ClientMessage::Ping)
        .await
        .expect("send ping while legacy synthesize tool is active");

    let pong_received = timeout(Duration::from_millis(250), async {
        loop {
            match conn.recv_with_timeout(Duration::from_millis(250)).await {
                DaemonMessage::Pong => return true,
                DaemonMessage::AgentError { .. } => continue,
                other => {
                    panic!("expected Pong while synthesize tool runs in background, got {other:?}")
                }
            }
        }
    })
    .await
    .unwrap_or(false);

    assert!(
        pong_received,
        "ping should not be blocked behind synthesize tool without declared capability"
    );

    match conn.recv_with_timeout(Duration::from_secs(3)).await {
        DaemonMessage::AgentError { message } => {
            assert!(message.contains("failed to synthesize generated tool"));
            assert!(message.contains("timed out"));
        }
        other => panic!("expected legacy synthesize tool error result, got {other:?}"),
    }

    conn.framed
        .send(ClientMessage::AgentGetOperationStatus { operation_id })
        .await
        .expect("query synthesize tool operation status");

    match conn.recv_with_timeout(Duration::from_secs(1)).await {
        DaemonMessage::OperationStatus { snapshot } => {
            assert_eq!(snapshot.kind, zorai_protocol::tool_names::SYNTHESIZE_TOOL);
        }
        other => panic!("expected synthesize tool status snapshot, got {other:?}"),
    }

    conn.shutdown().await;
}

#[tokio::test]
async fn synthesize_tool_request_without_declared_capability_returns_correlated_terminal_result() {
    let mut config = AgentConfig::default();
    config.tool_synthesis.enabled = true;
    let mut conn = spawn_test_connection_with_config(config).await;

    conn.framed
        .send(ClientMessage::AgentSynthesizeTool {
            request_json: serde_json::json!({
                "kind": "cli",
                "target": "ls"
            })
            .to_string(),
        })
        .await
        .expect("request synthesize tool without declared capability");

    let operation_id = match conn.recv_with_timeout(Duration::from_secs(1)).await {
        DaemonMessage::OperationAccepted {
            operation_id, kind, ..
        } => {
            assert_eq!(kind, zorai_protocol::tool_names::SYNTHESIZE_TOOL);
            operation_id
        }
        other => panic!("expected synthesize-tool operation acceptance, got {other:?}"),
    };

    match conn.recv_with_timeout(Duration::from_secs(3)).await {
        DaemonMessage::AgentGeneratedToolResult {
            operation_id: result_operation_id,
            tool_name,
            result_json,
        } => {
            assert_eq!(result_operation_id.as_deref(), Some(operation_id.as_str()));
            assert!(tool_name.is_none());
            let payload: serde_json::Value =
                serde_json::from_str(&result_json).expect("valid generated tool payload");
            assert_eq!(payload["status"], "new");
            assert_eq!(payload["kind"], "cli");
        }
        other => panic!("expected synthesize tool result, got {other:?}"),
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
        DaemonMessage::OperationAccepted {
            operation_id, kind, ..
        } => {
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

    assert!(
        pong_received,
        "ping should not be blocked behind divergent session startup"
    );

    conn.framed
        .send(ClientMessage::AgentGetOperationStatus { operation_id })
        .await
        .expect("query divergent session operation status");

    match conn.recv_with_timeout(Duration::from_secs(1)).await {
        DaemonMessage::OperationStatus { snapshot } => {
            assert_eq!(snapshot.kind, "start_divergent_session");
            assert!(matches!(
                snapshot.state,
                zorai_protocol::OperationLifecycleState::Accepted
                    | zorai_protocol::OperationLifecycleState::Started
            ));
        }
        other => panic!("expected divergent session status snapshot, got {other:?}"),
    }

    match conn.recv_with_timeout(Duration::from_secs(3)).await {
        DaemonMessage::AgentDivergentSessionStarted {
            operation_id: _,
            session_json,
        } => {
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
async fn divergent_session_async_request_returns_correlated_terminal_result() {
    let mut conn = spawn_test_connection().await;
    declare_async_command_capability(&mut conn).await;

    conn.framed
        .send(ClientMessage::AgentStartDivergentSession {
            problem_statement: "compare rollout options".to_string(),
            thread_id: "thread-div-correlated".to_string(),
            goal_run_id: None,
            custom_framings_json: None,
        })
        .await
        .expect("request async divergent session");

    let operation_id = match conn.recv_with_timeout(Duration::from_secs(1)).await {
        DaemonMessage::OperationAccepted {
            operation_id, kind, ..
        } => {
            assert_eq!(kind, "start_divergent_session");
            operation_id
        }
        other => panic!("expected divergent-session operation acceptance, got {other:?}"),
    };

    conn.framed
        .send(ClientMessage::AgentGetOperationStatus {
            operation_id: operation_id.clone(),
        })
        .await
        .expect("query divergent session operation status");

    match conn.recv_with_timeout(Duration::from_secs(1)).await {
        DaemonMessage::OperationStatus { snapshot } => {
            assert_eq!(snapshot.kind, "start_divergent_session");
            assert!(matches!(
                snapshot.state,
                zorai_protocol::OperationLifecycleState::Accepted
                    | zorai_protocol::OperationLifecycleState::Started
                    | zorai_protocol::OperationLifecycleState::Completed
                    | zorai_protocol::OperationLifecycleState::Failed
            ));
        }
        other => panic!("expected divergent session status snapshot, got {other:?}"),
    }

    match conn.recv_with_timeout(Duration::from_secs(3)).await {
        DaemonMessage::AgentDivergentSessionStarted {
            operation_id: result_operation_id,
            session_json,
        } => {
            assert_eq!(result_operation_id.as_deref(), Some(operation_id.as_str()));
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
                        assert_eq!(kind, "start_divergent_session");
                        operation_id = Some(accepted_operation_id);
                        continue;
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

    assert!(
        pong_received,
        "ping should not be blocked behind legacy divergent session startup"
    );
    assert!(
            operation_id.is_some(),
            "divergent session should now return operation acceptance even without capability declaration"
        );

    match conn.recv_with_timeout(Duration::from_secs(3)).await {
        DaemonMessage::AgentDivergentSessionStarted {
            operation_id: result_operation_id,
            session_json,
        } => {
            assert_eq!(result_operation_id.as_deref(), operation_id.as_deref());
            let payload: serde_json::Value =
                serde_json::from_str(&session_json).expect("valid divergent session payload");
            assert_eq!(payload["status"], "started");
            assert!(payload["session_id"].as_str().is_some());
        }
        other => panic!("expected legacy divergent session result, got {other:?}"),
    }

    conn.shutdown().await;
}

#[tokio::test]
async fn divergent_session_request_without_declared_capability_still_returns_operation_acceptance()
{
    let mut conn = spawn_test_connection().await;

    conn.framed
        .send(ClientMessage::AgentStartDivergentSession {
            problem_statement: "compare rollback options".to_string(),
            thread_id: "thread-div-legacy-correlation".to_string(),
            goal_run_id: None,
            custom_framings_json: None,
        })
        .await
        .expect("request legacy divergent session");

    let operation_id = match conn.recv_with_timeout(Duration::from_secs(3)).await {
        DaemonMessage::OperationAccepted {
            operation_id, kind, ..
        } => {
            assert_eq!(kind, "start_divergent_session");
            operation_id
        }
        other => panic!("expected divergent-session acceptance, got {other:?}"),
    };

    match conn.recv_with_timeout(Duration::from_secs(3)).await {
        DaemonMessage::AgentDivergentSessionStarted {
            operation_id: result_operation_id,
            session_json,
        } => {
            assert_eq!(result_operation_id.as_deref(), Some(operation_id.as_str()));
            let payload: serde_json::Value =
                serde_json::from_str(&session_json).expect("valid divergent session payload");
            assert_eq!(payload["status"], "started");
            assert!(payload["session_id"].as_str().is_some());
        }
        other => panic!("expected legacy divergent session result, got {other:?}"),
    }

    conn.shutdown().await;
}
