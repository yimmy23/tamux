#[tokio::test]
async fn daemon_collaboration_sessions_reply_emits_client_event() {
    let (event_tx, mut event_rx) = mpsc::channel(8);

    let should_continue = handle_daemon_message_for_test(
        DaemonMessage::AgentCollaborationSessions {
            sessions_json: serde_json::json!([
                {
                    "id": "session-1",
                    "parent_task_id": "task-1",
                    "disagreements": [
                        {
                            "topic": "deployment strategy",
                            "positions": ["roll forward", "roll back"]
                        }
                    ]
                }
            ])
            .to_string(),
        },
        &event_tx,
    )
    .await;

    assert!(should_continue);
    match event_rx.recv().await.expect("expected collaboration sessions event") {
        ClientEvent::CollaborationSessions { sessions_json } => {
            let parsed: Value = serde_json::from_str(&sessions_json).expect("valid collaboration sessions json");
            assert_eq!(parsed[0]["id"], "session-1");
        }
        other => panic!("expected collaboration sessions event, got {:?}", other),
    }
}

#[tokio::test]
async fn daemon_generated_tools_reply_emits_client_event() {
    let (event_tx, mut event_rx) = mpsc::channel(8);

    let should_continue = handle_daemon_message_for_test(
        DaemonMessage::AgentGeneratedTools {
            tools_json: serde_json::json!([
                {
                    "id": "tool-1",
                    "name": "tool-1",
                    "status": "active"
                }
            ])
            .to_string(),
        },
        &event_tx,
    )
    .await;

    assert!(should_continue);
    match event_rx.recv().await.expect("expected generated tools event") {
        ClientEvent::GeneratedTools { tools_json } => {
            let parsed: Value = serde_json::from_str(&tools_json).expect("valid generated tools json");
            assert_eq!(parsed[0]["id"], "tool-1");
            assert_eq!(parsed[0]["status"], "active");
        }
        other => panic!("expected generated tools event, got {:?}", other),
    }
}

#[tokio::test]
async fn daemon_collaboration_vote_result_reply_emits_client_event() {
    let (event_tx, mut event_rx) = mpsc::channel(8);

    let should_continue = handle_daemon_message_for_test(
        DaemonMessage::AgentCollaborationVoteResult {
            report_json: serde_json::json!({
                "session_id": "session-1",
                "resolution": "resolved"
            })
            .to_string(),
        },
        &event_tx,
    )
    .await;

    assert!(should_continue);
    match event_rx.recv().await.expect("expected collaboration vote result event") {
        ClientEvent::CollaborationVoteResult { report_json } => {
            let parsed: Value = serde_json::from_str(&report_json).expect("valid collaboration vote result json");
            assert_eq!(parsed["session_id"], "session-1");
            assert_eq!(parsed["resolution"], "resolved");
        }
        other => panic!("expected collaboration vote result event, got {:?}", other),
    }
}