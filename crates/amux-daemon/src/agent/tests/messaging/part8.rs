use super::*;
use amux_shared::providers::PROVIDER_ID_OPENAI;
use tokio::time::{timeout, Duration};

#[tokio::test]
async fn participant_send_now_posts_direct_visible_message_without_llm_roundtrip() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let recorded_bodies = Arc::new(StdMutex::new(VecDeque::new()));
    let base_url = spawn_recording_openai_server(recorded_bodies.clone()).await;

    let mut config = AgentConfig::default();
    config.provider = PROVIDER_ID_OPENAI.to_string();
    config.base_url = base_url;
    config.model = "gpt-5.4-mini".to_string();
    config.api_key = "test-key".to_string();
    config.auth_source = AuthSource::ApiKey;
    config.api_transport = ApiTransport::ChatCompletions;
    config.auto_retry = false;
    config.max_retries = 0;
    config.max_tool_loops = 1;
    let engine = AgentEngine::new_test(manager, config, root.path()).await;
    let thread_id = "thread_participant_send_direct_post";

    engine.threads.write().await.insert(
        thread_id.to_string(),
        AgentThread {
            id: thread_id.to_string(),
            agent_name: Some(crate::agent::agent_identity::MAIN_AGENT_NAME.to_string()),
            title: "Participant direct post".to_string(),
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
        .expect("participant should register");
    let suggestion = engine
        .queue_thread_participant_suggestion(
            thread_id,
            "weles",
            "Participant verdict: claim is false.",
            false,
        )
        .await
        .expect("queue suggestion");

    engine
        .send_thread_participant_suggestion(thread_id, &suggestion.id, None)
        .await
        .expect("send suggestion");

    let threads = engine.threads.read().await;
    let thread = threads
        .get(thread_id)
        .expect("thread should still exist after participant send-now");
    let assistant = thread
        .messages
        .iter()
        .rev()
        .find(|message| {
            message.role == MessageRole::Assistant
                && message.author_agent_id.as_deref() == Some("weles")
        })
        .expect("send-now should append a visible participant-authored message");

    assert_eq!(assistant.content, "Participant verdict: claim is false.");
    assert_eq!(assistant.author_agent_name.as_deref(), Some("Weles"));
    assert_eq!(assistant.provider, None);
    assert_eq!(assistant.model, None);
    assert_eq!(
        thread
            .messages
            .iter()
            .filter(|message| message.role == MessageRole::User)
            .count(),
        1,
        "send-now should not add a fresh user turn"
    );
    drop(threads);

    assert!(
        recorded_bodies.lock().expect("lock request log").is_empty(),
        "send-now should post the participant suggestion directly without a new LLM request"
    );

    let dm_thread_id = crate::agent::agent_identity::internal_dm_thread_id(
        crate::agent::agent_identity::MAIN_AGENT_ID,
        "weles",
    );
    assert!(
        engine.get_thread(&dm_thread_id).await.is_none(),
        "send-now should not create a hidden Weles DM thread"
    );
}

#[tokio::test]
async fn participant_failed_suggestions_reload() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let thread_id = "thread_participant_failed_reload";

    engine.threads.write().await.insert(
        thread_id.to_string(),
        AgentThread {
            id: thread_id.to_string(),
            agent_name: Some(crate::agent::agent_identity::MAIN_AGENT_NAME.to_string()),
            title: "Participant failed reload".to_string(),
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
        .expect("participant should register");
    let suggestion = engine
        .queue_thread_participant_suggestion(thread_id, "weles", "Check claim", false)
        .await
        .expect("queue suggestion");
    engine
        .fail_thread_participant_suggestion(thread_id, &suggestion.id, "provider unavailable")
        .await
        .expect("fail suggestion");

    drop(engine);

    let manager = SessionManager::new_test(root.path()).await;
    let reloaded = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    reloaded.hydrate().await.expect("hydrate");

    let suggestions = reloaded
        .list_thread_participant_suggestions(thread_id)
        .await;
    assert_eq!(suggestions.len(), 1);
    assert_eq!(
        suggestions[0].status,
        ThreadParticipantSuggestionStatus::Failed
    );
    assert_eq!(
        suggestions[0].error.as_deref(),
        Some("provider unavailable")
    );
}

#[tokio::test]
async fn participant_dismissed_suggestions_do_not_reload() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let thread_id = "thread_participant_dismiss_reload";

    engine.threads.write().await.insert(
        thread_id.to_string(),
        AgentThread {
            id: thread_id.to_string(),
            agent_name: Some(crate::agent::agent_identity::MAIN_AGENT_NAME.to_string()),
            title: "Participant dismiss reload".to_string(),
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
        .expect("participant should register");
    let suggestion = engine
        .queue_thread_participant_suggestion(thread_id, "weles", "Check claim", false)
        .await
        .expect("queue suggestion");
    engine
        .dismiss_thread_participant_suggestion(thread_id, &suggestion.id)
        .await
        .expect("dismiss suggestion");

    drop(engine);

    let manager = SessionManager::new_test(root.path()).await;
    let reloaded = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    reloaded.hydrate().await.expect("hydrate");

    assert!(reloaded
        .list_thread_participant_suggestions(thread_id)
        .await
        .is_empty());
}

#[tokio::test]
async fn participant_sent_suggestions_do_not_reload() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("addr");

    tokio::spawn(async move {
        for _ in 0..3 {
            let Ok((mut socket, _)) = listener.accept().await else {
                break;
            };
            let _ = read_http_request_body(&mut socket)
                .await
                .expect("read request");
            let response_body = concat!(
                "data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_participant_send_reload\"}}\n\n",
                "data: {\"type\":\"response.output_text.delta\",\"delta\":\"Sent suggestion\"}\n\n",
                "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_participant_send_reload\",\"object\":\"response\",\"status\":\"completed\",\"output\":[],\"usage\":{\"input_tokens\":6,\"output_tokens\":2},\"error\":null}}\n\n"
            );
            let response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                response_body.len(),
                response_body
            );
            socket
                .write_all(response.as_bytes())
                .await
                .expect("write response");
        }
    });

    let mut config = AgentConfig::default();
    config.provider = PROVIDER_ID_OPENAI.to_string();
    config.base_url = format!("http://{addr}/v1");
    config.model = "gpt-5.4-mini".to_string();
    config.api_key = "test-key".to_string();
    config.auth_source = AuthSource::ApiKey;
    config.api_transport = ApiTransport::Responses;
    config.auto_retry = false;
    config.max_retries = 0;
    config.max_tool_loops = 1;
    let engine = AgentEngine::new_test(manager, config, root.path()).await;
    let thread_id = "thread_participant_send_reload";

    engine.threads.write().await.insert(
        thread_id.to_string(),
        AgentThread {
            id: thread_id.to_string(),
            agent_name: Some(crate::agent::agent_identity::MAIN_AGENT_NAME.to_string()),
            title: "Participant send reload".to_string(),
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
        .expect("participant should register");
    let suggestion = engine
        .queue_thread_participant_suggestion(thread_id, "weles", "Check claim", false)
        .await
        .expect("queue suggestion");
    engine
        .send_thread_participant_suggestion(thread_id, &suggestion.id, None)
        .await
        .expect("send suggestion");

    drop(engine);

    let manager = SessionManager::new_test(root.path()).await;
    let reloaded = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    reloaded.hydrate().await.expect("hydrate");

    assert!(reloaded
        .list_thread_participant_suggestions(thread_id)
        .await
        .is_empty());
}

#[tokio::test]
async fn participant_observer_failure_keeps_outer_send_successful() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind participant observer failure server");
    let addr = listener.local_addr().expect("observer failure addr");

    tokio::spawn(async move {
        let mut request_count = 0usize;
        loop {
            let Ok((mut socket, _)) = listener.accept().await else {
                return;
            };
            let _body = read_http_request_body(&mut socket)
                .await
                .expect("read request");
            request_count += 1;
            match request_count {
                1 => {
                    let response_body = concat!(
                        "data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_outer_send\"}}\n\n",
                        "data: {\"type\":\"response.output_text.delta\",\"delta\":\"outer send ok\"}\n\n",
                        "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_outer_send\",\"object\":\"response\",\"status\":\"completed\",\"output\":[],\"usage\":{\"input_tokens\":4,\"output_tokens\":2},\"error\":null}}\n\n"
                    );
                    let response = format!(
                        "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                        response_body.len(),
                        response_body
                    );
                    socket
                        .write_all(response.as_bytes())
                        .await
                        .expect("write outer send response");
                }
                2 => {
                    drop(socket);
                    return;
                }
                _ => return,
            }
        }
    });

    let mut config = AgentConfig::default();
    config.provider = PROVIDER_ID_OPENAI.to_string();
    config.base_url = format!("http://{addr}/v1");
    config.model = "gpt-5.4-mini".to_string();
    config.api_key = "test-key".to_string();
    config.auth_source = AuthSource::ApiKey;
    config.api_transport = ApiTransport::Responses;
    config.auto_retry = false;
    config.max_retries = 0;
    config.max_tool_loops = 1;
    let engine = AgentEngine::new_test(manager, config, root.path()).await;
    let thread_id = "thread_participant_outer_send_contract";

    engine.threads.write().await.insert(
        thread_id.to_string(),
        AgentThread {
            id: thread_id.to_string(),
            agent_name: Some(crate::agent::agent_identity::MAIN_AGENT_NAME.to_string()),
            title: "Outer send contract".to_string(),
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
        .expect("participant should register");

    let mut events = engine.subscribe();
    let send_result = engine
        .send_message_with_session_and_surface(Some(thread_id), None, "Please respond", None)
        .await;
    assert!(
        send_result.is_ok(),
        "observer failure should not fail the outer send"
    );

    let notice = timeout(Duration::from_secs(1), async {
        loop {
            match events.recv().await.expect("event stream should stay open") {
                AgentEvent::WorkflowNotice {
                    thread_id: event_thread_id,
                    kind,
                    message,
                    details,
                } if event_thread_id == thread_id && kind == "participant_observer_error" => {
                    return (message, details);
                }
                _ => continue,
            }
        }
    })
    .await
    .expect("expected participant observer workflow notice");

    assert_eq!(notice.0, "participant observers failed");
    assert!(
        notice.1.is_some(),
        "workflow notice should include failure details"
    );

    assert!(
        engine
            .list_thread_participant_suggestions(thread_id)
            .await
            .is_empty(),
        "force-send participant failure should not leave queued or failed suggestions behind"
    );
}
