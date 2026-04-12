#[tokio::test]
async fn unauthorized_participant_updates_are_rejected() {
    let mut conn = spawn_test_connection().await;
    let thread_id = "thread-participant-auth";

    conn.agent.threads.write().await.insert(
        thread_id.to_string(),
        crate::agent::types::AgentThread {
            id: thread_id.to_string(),
            agent_name: Some(crate::agent::agent_identity::MAIN_AGENT_NAME.to_string()),
            title: "Participant auth".to_string(),
            messages: vec![crate::agent::types::AgentMessage::user("hello", 1)],
            pinned: false,
            upstream_thread_id: None,
            upstream_transport: None,
            upstream_provider: None,
            upstream_model: None,
            upstream_assistant_id: None,
            created_at: 1,
            updated_at: 2,
            total_input_tokens: 0,
            total_output_tokens: 0,
        },
    );
    conn.agent
        .set_thread_client_surface(thread_id, amux_protocol::ClientSurface::Tui)
        .await;

    conn.framed
        .send(ClientMessage::AgentThreadParticipantCommand {
            thread_id: thread_id.to_string(),
            target_agent_id: "weles".to_string(),
            action: "upsert".to_string(),
            instruction: Some("verify claims".to_string()),
            session_id: None,
            client_surface: Some(amux_protocol::ClientSurface::Electron),
        })
        .await
        .expect("send participant command");

    match conn.recv().await {
        DaemonMessage::Error { message } => {
            assert!(message.contains("unauthorized"));
        }
        other => panic!("expected participant authorization error, got {other:?}"),
    }

    conn.shutdown().await;
}

#[tokio::test]
async fn unauthorized_participant_suggestion_updates_are_rejected() {
    let mut conn = spawn_test_connection().await;
    let thread_id = "thread-suggestion-auth";

    conn.agent.threads.write().await.insert(
        thread_id.to_string(),
        crate::agent::types::AgentThread {
            id: thread_id.to_string(),
            agent_name: Some(crate::agent::agent_identity::MAIN_AGENT_NAME.to_string()),
            title: "Suggestion auth".to_string(),
            messages: vec![crate::agent::types::AgentMessage::user("hello", 1)],
            pinned: false,
            upstream_thread_id: None,
            upstream_transport: None,
            upstream_provider: None,
            upstream_model: None,
            upstream_assistant_id: None,
            created_at: 1,
            updated_at: 2,
            total_input_tokens: 0,
            total_output_tokens: 0,
        },
    );
    conn.agent
        .set_thread_client_surface(thread_id, amux_protocol::ClientSurface::Tui)
        .await;

    conn.framed
        .send(ClientMessage::AgentSendParticipantSuggestion {
            thread_id: thread_id.to_string(),
            suggestion_id: "sugg-1".to_string(),
            session_id: None,
            client_surface: Some(amux_protocol::ClientSurface::Electron),
        })
        .await
        .expect("send suggestion command");

    match conn.recv().await {
        DaemonMessage::Error { message } => {
            assert!(message.contains("unauthorized"));
        }
        other => panic!("expected suggestion authorization error, got {other:?}"),
    }

    conn.framed
        .send(ClientMessage::AgentDismissParticipantSuggestion {
            thread_id: thread_id.to_string(),
            suggestion_id: "missing".to_string(),
            session_id: None,
            client_surface: Some(amux_protocol::ClientSurface::Electron),
        })
        .await
        .expect("send dismiss suggestion command");

    match conn.recv().await {
        DaemonMessage::Error { message } => {
            assert!(message.contains("unauthorized"));
        }
        other => panic!("expected dismiss authorization error, got {other:?}"),
    }

    conn.shutdown().await;
}

#[tokio::test]
async fn get_thread_includes_thread_participants_for_reload() {
    let mut conn = spawn_test_connection().await;
    let thread_id = "thread-detail-participants";

    conn.agent.threads.write().await.insert(
        thread_id.to_string(),
        crate::agent::types::AgentThread {
            id: thread_id.to_string(),
            agent_name: Some(crate::agent::agent_identity::MAIN_AGENT_NAME.to_string()),
            title: "Thread detail".to_string(),
            messages: vec![crate::agent::types::AgentMessage::user("hello", 1)],
            pinned: false,
            upstream_thread_id: None,
            upstream_transport: None,
            upstream_provider: None,
            upstream_model: None,
            upstream_assistant_id: None,
            created_at: 1,
            updated_at: 2,
            total_input_tokens: 0,
            total_output_tokens: 0,
        },
    );
    conn.agent
        .upsert_thread_participant(thread_id, "weles", "verify claims")
        .await
        .expect("upsert participant");

    conn.framed
        .send(ClientMessage::AgentGetThread {
            thread_id: thread_id.to_string(),
            message_limit: None,
            message_offset: None,
        })
        .await
        .expect("request thread detail");

    let thread_json = match conn.recv().await {
        DaemonMessage::AgentThreadDetail { thread_json } => thread_json,
        other => panic!("expected thread detail, got {other:?}"),
    };
    let value: serde_json::Value =
        serde_json::from_str(&thread_json).expect("decode thread detail payload");
    let participants = value
        .get("thread_participants")
        .and_then(|entry| entry.as_array())
        .expect("participants should be included");
    assert_eq!(participants.len(), 1);
    assert_eq!(participants[0]["agent_id"], "weles");
    assert_eq!(participants[0]["instruction"], "verify claims");

    conn.shutdown().await;
}

#[tokio::test]
async fn get_thread_includes_failed_participant_suggestions() {
    let mut conn = spawn_test_connection().await;
    let thread_id = "thread-detail-suggestions";

    conn.agent.threads.write().await.insert(
        thread_id.to_string(),
        crate::agent::types::AgentThread {
            id: thread_id.to_string(),
            agent_name: Some(crate::agent::agent_identity::MAIN_AGENT_NAME.to_string()),
            title: "Thread suggestions".to_string(),
            messages: vec![crate::agent::types::AgentMessage::user("hello", 1)],
            pinned: false,
            upstream_thread_id: None,
            upstream_transport: None,
            upstream_provider: None,
            upstream_model: None,
            upstream_assistant_id: None,
            created_at: 1,
            updated_at: 2,
            total_input_tokens: 0,
            total_output_tokens: 0,
        },
    );
    conn.agent
        .thread_participant_suggestions
        .write()
        .await
        .insert(
            thread_id.to_string(),
            vec![crate::agent::ThreadParticipantSuggestion {
                id: "suggestion-1".to_string(),
                target_agent_id: "weles".to_string(),
                target_agent_name: "Weles".to_string(),
                instruction: "verify claims".to_string(),
                force_send: false,
                status: crate::agent::ThreadParticipantSuggestionStatus::Failed,
                created_at: 10,
                updated_at: 11,
                error: Some("provider unavailable".to_string()),
            }],
        );

    conn.framed
        .send(ClientMessage::AgentGetThread {
            thread_id: thread_id.to_string(),
            message_limit: None,
            message_offset: None,
        })
        .await
        .expect("request thread detail");

    let thread_json = match conn.recv().await {
        DaemonMessage::AgentThreadDetail { thread_json } => thread_json,
        other => panic!("expected thread detail, got {other:?}"),
    };
    let value: serde_json::Value =
        serde_json::from_str(&thread_json).expect("decode thread detail payload");
    let suggestions = value
        .get("queued_participant_suggestions")
        .and_then(|entry| entry.as_array())
        .expect("suggestions should be included");
    assert_eq!(suggestions.len(), 1);
    assert_eq!(suggestions[0]["status"], "failed");
    assert_eq!(suggestions[0]["error"], "provider unavailable");

    conn.shutdown().await;
}
