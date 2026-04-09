#[tokio::test]
async fn daemon_operator_model_replies_emit_client_events() {
    let (event_tx, mut event_rx) = mpsc::channel(8);

    let should_continue = handle_daemon_message_for_test(
        DaemonMessage::AgentOperatorModel {
            model_json: serde_json::json!({
                "version": "1.0",
                "session_count": 4,
                "risk_fingerprint": {
                    "auto_approve_categories": ["git"]
                }
            })
            .to_string(),
        },
        &event_tx,
    )
    .await;

    assert!(should_continue);
    match event_rx.recv().await.expect("expected operator model event") {
        ClientEvent::OperatorModelSummary { model_json } => {
            let parsed: Value = serde_json::from_str(&model_json).expect("valid operator model");
            assert_eq!(parsed["version"], "1.0");
            assert_eq!(parsed["session_count"], 4);
        }
        other => panic!("expected operator model summary event, got {:?}", other),
    }

    let should_continue = handle_daemon_message_for_test(
        DaemonMessage::AgentOperatorModelReset { ok: true },
        &event_tx,
    )
    .await;

    assert!(should_continue);
    match event_rx.recv().await.expect("expected operator model reset event") {
        ClientEvent::OperatorModelReset { ok } => {
            assert!(ok);
        }
        other => panic!("expected operator model reset event, got {:?}", other),
    }
}

#[test]
fn bootstrap_rearms_after_successful_connection_cycle() {
    assert!(
        !DaemonClient::next_bootstrap_attempted(true, true),
        "a successful connection cycle should re-arm daemon bootstrap for the next disconnect"
    );
    assert!(
        !DaemonClient::next_bootstrap_attempted(false, false),
        "before any bootstrap attempt or successful connection, the latch should stay clear"
    );
    assert!(
        DaemonClient::next_bootstrap_attempted(true, false),
        "once bootstrap already ran in the current disconnected cycle, it should not rerun immediately"
    );
}