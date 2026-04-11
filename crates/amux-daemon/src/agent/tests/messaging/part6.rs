use super::*;

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
