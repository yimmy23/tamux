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
                assert_eq!(kind, zorai_protocol::tool_names::PLUGIN_API_CALL);
            }
            other => {
                panic!("expected plugin api acceptance during saturation setup, got {other:?}")
            }
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
    assert!(matches!(
        conn.recv_with_timeout(Duration::from_secs(1)).await,
        DaemonMessage::Pong
    ));

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
                assert_eq!(kind, zorai_protocol::tool_names::PLUGIN_API_CALL);
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
    assert!(
        metrics["plugin_io"]["rejection_count"]
            .as_u64()
            .unwrap_or(0)
            >= 1
    );
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
                assert_eq!(kind, zorai_protocol::tool_names::PLUGIN_API_CALL);
            }
            other => {
                panic!("expected plugin api acceptance during saturation setup, got {other:?}")
            }
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
