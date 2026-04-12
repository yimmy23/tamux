use super::*;
use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;

#[tokio::test]
async fn thread_participant_update_and_deactivate_persist() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let thread_id = "thread-participant-update-persist";

    engine.threads.write().await.insert(
        thread_id.to_string(),
        AgentThread {
            id: thread_id.to_string(),
            agent_name: Some(crate::agent::agent_identity::MAIN_AGENT_NAME.to_string()),
            title: "Participant persistence".to_string(),
            messages: vec![AgentMessage::user("hello", 1)],
            pinned: false,
            upstream_thread_id: None,
            upstream_transport: None,
            upstream_provider: None,
            upstream_model: None,
            upstream_assistant_id: None,
            total_input_tokens: 0,
            total_output_tokens: 0,
            created_at: 1,
            updated_at: 1,
        },
    );

    engine
        .upsert_thread_participant(thread_id, "weles", "verify claims")
        .await
        .expect("initial upsert should succeed");
    engine
        .upsert_thread_participant(thread_id, "weles", "focus on risk")
        .await
        .expect("participant update should succeed");
    engine
        .deactivate_thread_participant(thread_id, "weles")
        .await
        .expect("participant deactivate should succeed");

    drop(engine);

    let manager = SessionManager::new_test(root.path()).await;
    let reloaded = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    reloaded.hydrate().await.expect("hydrate should succeed");

    let participants = reloaded.list_thread_participants(thread_id).await;
    let weles = participants
        .iter()
        .find(|participant| participant.agent_id == "weles")
        .expect("weles participant should reload");

    assert_eq!(weles.instruction, "focus on risk");
    assert_eq!(
        weles.status,
        crate::agent::ThreadParticipantStatus::Inactive
    );
}

#[tokio::test]
async fn participant_upsert_does_not_change_thread_owner() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let thread_id = "thread-participant-owner-stability";

    engine.threads.write().await.insert(
        thread_id.to_string(),
        AgentThread {
            id: thread_id.to_string(),
            agent_name: Some(crate::agent::agent_identity::MAIN_AGENT_NAME.to_string()),
            title: "Participant owner stability".to_string(),
            messages: vec![AgentMessage::user("hello", 1)],
            pinned: false,
            upstream_thread_id: None,
            upstream_transport: None,
            upstream_provider: None,
            upstream_model: None,
            upstream_assistant_id: None,
            total_input_tokens: 0,
            total_output_tokens: 0,
            created_at: 1,
            updated_at: 1,
        },
    );

    let owner_before = engine
        .threads
        .read()
        .await
        .get(thread_id)
        .and_then(|thread| thread.agent_name.clone())
        .expect("thread owner should exist before participant upsert");

    engine
        .upsert_thread_participant(thread_id, "weles", "verify claims")
        .await
        .expect("participant upsert should succeed");

    let owner_after = engine
        .threads
        .read()
        .await
        .get(thread_id)
        .and_then(|thread| thread.agent_name.clone())
        .expect("thread owner should exist after participant upsert");

    assert_eq!(owner_before, owner_after);
}

#[tokio::test]
async fn thread_participant_deactivate_does_not_create_message() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let thread_id = "thread-participant-deactivate-no-message";

    engine.threads.write().await.insert(
        thread_id.to_string(),
        AgentThread {
            id: thread_id.to_string(),
            agent_name: Some(crate::agent::agent_identity::MAIN_AGENT_NAME.to_string()),
            title: "Participant deactivate no message".to_string(),
            messages: vec![AgentMessage::user("hello", 1)],
            pinned: false,
            upstream_thread_id: None,
            upstream_transport: None,
            upstream_provider: None,
            upstream_model: None,
            upstream_assistant_id: None,
            total_input_tokens: 0,
            total_output_tokens: 0,
            created_at: 1,
            updated_at: 1,
        },
    );

    engine
        .upsert_thread_participant(thread_id, "weles", "verify claims")
        .await
        .expect("participant upsert should succeed");

    let before = engine
        .threads
        .read()
        .await
        .get(thread_id)
        .map(|thread| thread.messages.len())
        .expect("thread should exist before deactivate");

    engine
        .deactivate_thread_participant(thread_id, "weles")
        .await
        .expect("participant deactivate should succeed");

    let after = engine
        .threads
        .read()
        .await
        .get(thread_id)
        .map(|thread| thread.messages.len())
        .expect("thread should exist after deactivate");

    assert_eq!(before, after);
}

#[tokio::test]
async fn stop_marks_participant_inactive_and_clears_live_suggestions() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let thread_id = "thread-participant-stop-clears-suggestions";

    engine.threads.write().await.insert(
        thread_id.to_string(),
        AgentThread {
            id: thread_id.to_string(),
            agent_name: Some(crate::agent::agent_identity::MAIN_AGENT_NAME.to_string()),
            title: "Participant stop clears suggestions".to_string(),
            messages: vec![AgentMessage::user("hello", 1)],
            pinned: false,
            upstream_thread_id: None,
            upstream_transport: None,
            upstream_provider: None,
            upstream_model: None,
            upstream_assistant_id: None,
            total_input_tokens: 0,
            total_output_tokens: 0,
            created_at: 1,
            updated_at: 1,
        },
    );

    engine
        .upsert_thread_participant(thread_id, "weles", "verify claims")
        .await
        .expect("participant upsert should succeed");

    {
        let mut streams = engine.stream_cancellations.lock().await;
        streams.insert(
            thread_id.to_string(),
            StreamCancellationEntry {
                generation: 1,
                token: CancellationToken::new(),
                retry_now: Arc::new(Notify::new()),
                started_at: 1,
                last_progress_at: 1,
                last_progress_kind: StreamProgressKind::Started,
                last_progress_excerpt: String::new(),
            },
        );
    }

    engine
        .queue_thread_participant_suggestion(thread_id, "weles", "queued note", false)
        .await
        .expect("queue while stream is active");
    assert_eq!(
        engine
            .list_thread_participant_suggestions(thread_id)
            .await
            .len(),
        1
    );

    engine
        .apply_thread_participant_command(thread_id, "weles", "stop", None)
        .await
        .expect("stop should succeed");

    let participants = engine.list_thread_participants(thread_id).await;
    assert_eq!(participants.len(), 1);
    assert_eq!(participants[0].agent_id, "weles");
    assert_eq!(participants[0].status, ThreadParticipantStatus::Inactive);
    assert!(
        engine
            .list_thread_participant_suggestions(thread_id)
            .await
            .is_empty(),
        "stop should clear queued participant suggestions"
    );

    let error = engine
        .queue_thread_participant_suggestion(thread_id, "weles", "should fail", false)
        .await
        .expect_err("inactive participant should not accept new queued suggestions");
    assert!(
        error
            .to_string()
            .contains("participant is not active on thread"),
        "unexpected error: {error}"
    );
}

#[tokio::test]
async fn leave_removes_participant_and_readding_starts_fresh() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let thread_id = "thread-participant-leave-removes-instance";

    engine.threads.write().await.insert(
        thread_id.to_string(),
        AgentThread {
            id: thread_id.to_string(),
            agent_name: Some(crate::agent::agent_identity::MAIN_AGENT_NAME.to_string()),
            title: "Participant leave removes instance".to_string(),
            messages: vec![AgentMessage::user("hello", 1)],
            pinned: false,
            upstream_thread_id: None,
            upstream_transport: None,
            upstream_provider: None,
            upstream_model: None,
            upstream_assistant_id: None,
            total_input_tokens: 0,
            total_output_tokens: 0,
            created_at: 1,
            updated_at: 1,
        },
    );

    engine
        .upsert_thread_participant(thread_id, "weles", "verify claims")
        .await
        .expect("participant upsert should succeed");
    engine
        .append_visible_thread_participant_message(thread_id, "weles", "first note")
        .await
        .expect("participant note should succeed");

    {
        let mut streams = engine.stream_cancellations.lock().await;
        streams.insert(
            thread_id.to_string(),
            StreamCancellationEntry {
                generation: 1,
                token: CancellationToken::new(),
                retry_now: Arc::new(Notify::new()),
                started_at: 1,
                last_progress_at: 1,
                last_progress_kind: StreamProgressKind::Started,
                last_progress_excerpt: String::new(),
            },
        );
    }

    engine
        .queue_thread_participant_suggestion(thread_id, "weles", "queued note", false)
        .await
        .expect("queue while stream is active");

    engine
        .apply_thread_participant_command(thread_id, "weles", "leave", None)
        .await
        .expect("leave should succeed");

    assert!(
        engine.list_thread_participants(thread_id).await.is_empty(),
        "leave should remove the participant record entirely"
    );
    assert!(
        engine
            .list_thread_participant_suggestions(thread_id)
            .await
            .is_empty(),
        "leave should clear queued participant suggestions"
    );

    engine
        .upsert_thread_participant(thread_id, "weles", "fresh instructions")
        .await
        .expect("re-adding participant should succeed");
    let participants = engine.list_thread_participants(thread_id).await;
    assert_eq!(participants.len(), 1);
    assert_eq!(participants[0].agent_id, "weles");
    assert_eq!(participants[0].instruction, "fresh instructions");
    assert_eq!(participants[0].status, ThreadParticipantStatus::Active);
    assert_eq!(
        participants[0].last_contribution_at, None,
        "re-added participant should be a fresh instance"
    );
    assert_eq!(participants[0].deactivated_at, None);
}
