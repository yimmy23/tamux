use super::*;

#[tokio::test]
async fn send_message_with_ephemeral_user_override_keeps_thread_history_clean() {
    let recorded_bodies = Arc::new(StdMutex::new(VecDeque::new()));
    let server_url = spawn_recording_openai_server(recorded_bodies.clone()).await;
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.provider = "openai".to_string();
    config.base_url = server_url;
    config.model = "gpt-4o-mini".to_string();
    config.api_transport = ApiTransport::ChatCompletions;
    config.max_retries = 0;
    config.auto_retry = false;
    let engine = AgentEngine::new_test(manager, config, root.path()).await;

    let thread_id = engine
        .send_message_with_ephemeral_user_override(
            None,
            "What model are you bro?",
            "[discord message from mariuszkurman]: What model are you bro?\nYour final assistant response will be delivered back to the user automatically.",
            std::time::Duration::from_secs(120),
        )
        .await
        .expect("send message with ephemeral override");

    let messages = engine
        .history
        .list_messages(&thread_id, Some(10))
        .await
        .expect("load persisted messages");
    let stored_user = messages
        .iter()
        .find(|message| message.role == "user")
        .expect("stored user message");
    assert_eq!(stored_user.content, "What model are you bro?");

    let request_body = recorded_bodies
        .lock()
        .expect("lock request log")
        .pop_front()
        .expect("captured llm request");
    assert!(
        request_body.contains(
            "Your final assistant response will be delivered back to the user automatically."
        ),
        "LLM request should include the ephemeral gateway wrapper"
    );
    assert!(
        !request_body.contains("\"content\":\"What model are you bro?\""),
        "LLM request should replace the raw stored user text with the ephemeral override"
    );
}
