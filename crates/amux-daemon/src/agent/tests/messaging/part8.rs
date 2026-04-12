use super::*;
use amux_shared::providers::PROVIDER_ID_OPENAI;
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::time::{timeout, Duration};

#[tokio::test]
async fn participant_send_now_posts_direct_visible_message_then_continues_thread() {
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

    let thread_messages = {
        let threads = engine.threads.read().await;
        threads
            .get(thread_id)
            .expect("thread should still exist after participant send-now")
            .messages
            .clone()
    };
    let assistant = thread_messages
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
        thread_messages
            .iter()
            .filter(|message| message.role == MessageRole::User)
            .count(),
        1,
        "send-now should not add a fresh user turn"
    );
    let follow_up = thread_messages
        .iter()
        .rev()
        .find(|message| {
            message.role == MessageRole::Assistant
                && message.author_agent_id.as_deref() != Some("weles")
        })
        .expect("send-now should trigger a separate active-agent follow-up");
    assert_eq!(follow_up.content, "Gateway reply ok");

    assert_eq!(
        recorded_bodies.lock().expect("lock request log").len(),
        1,
        "send-now should wake the active thread agent once after the direct participant post"
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
async fn participant_follow_up_and_later_turn_persist_across_reload() {
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
    let engine = AgentEngine::new_test(manager, config.clone(), root.path()).await;
    let thread_id = "thread_participant_reload_persistence";

    engine.threads.write().await.insert(
        thread_id.to_string(),
        AgentThread {
            id: thread_id.to_string(),
            agent_name: Some(crate::agent::agent_identity::MAIN_AGENT_NAME.to_string()),
            title: "Participant reload persistence".to_string(),
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

    engine
        .resend_existing_user_message(thread_id, "hello")
        .await
        .expect("later resend should succeed");

    let live_messages = {
        let threads = engine.threads.read().await;
        threads
            .get(thread_id)
            .expect("thread should still exist")
            .messages
            .clone()
    };
    assert!(
        live_messages.len() >= 4,
        "expected user, participant post, participant follow-up, and a later main-agent turn"
    );

    drop(engine);

    let manager = SessionManager::new_test(root.path()).await;
    let reloaded = AgentEngine::new_test(manager, config, root.path()).await;
    reloaded.hydrate().await.expect("hydrate");

    let reloaded_messages = reloaded
        .get_thread(thread_id)
        .await
        .expect("reloaded thread should exist")
        .messages;

    assert_eq!(reloaded_messages.len(), live_messages.len());
    assert_eq!(
        reloaded_messages
            .iter()
            .map(|message| message.content.as_str())
            .collect::<Vec<_>>(),
        live_messages
            .iter()
            .map(|message| message.content.as_str())
            .collect::<Vec<_>>()
    );
    assert_eq!(
        reloaded_messages
            .last()
            .and_then(|message| message.author_agent_id.as_deref()),
        live_messages
            .last()
            .and_then(|message| message.author_agent_id.as_deref())
    );
}

#[tokio::test]
async fn queued_participant_suggestion_auto_sends_after_active_stream_finishes() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind queued participant server");
    let addr = listener.local_addr().expect("queued participant addr");
    let request_counter = Arc::new(AtomicUsize::new(0));
    let first_request_started = Arc::new(tokio::sync::Notify::new());
    let release_first_response = Arc::new(tokio::sync::Notify::new());
    let request_counter_task = request_counter.clone();
    let first_request_started_task = first_request_started.clone();
    let release_first_response_task = release_first_response.clone();

    tokio::spawn(async move {
        loop {
            let Ok((mut socket, _)) = listener.accept().await else {
                break;
            };
            let body = read_http_request_body(&mut socket)
                .await
                .expect("read queued participant request");
            let request_index = request_counter_task.fetch_add(1, Ordering::SeqCst);
            let response_body = match request_index {
                0 => {
                    first_request_started_task.notify_waiters();
                    release_first_response_task.notified().await;
                    concat!(
                        "data: {\"choices\":[{\"delta\":{\"content\":\"Main reply before participant note.\"}}]}\n\n",
                        "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":9,\"completion_tokens\":5}}\n\n",
                        "data: [DONE]\n\n"
                    )
                }
                _ if body.contains("Role: participant observer") => concat!(
                    "data: {\"choices\":[{\"delta\":{\"content\":\"NO_SUGGESTION\"}}]}\n\n",
                    "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":5,\"completion_tokens\":2}}\n\n",
                    "data: [DONE]\n\n"
                ),
                1 => concat!(
                    "data: {\"choices\":[{\"delta\":{\"content\":\"Swarozyc followed the queued participant note.\"}}]}\n\n",
                    "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":8,\"completion_tokens\":6}}\n\n",
                    "data: [DONE]\n\n"
                ),
                _ => concat!(
                    "data: {\"choices\":[{\"delta\":{\"content\":\"unexpected extra request\"}}]}\n\n",
                    "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":1,\"completion_tokens\":1}}\n\n",
                    "data: [DONE]\n\n"
                ),
            };
            let response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncache-control: no-cache\r\nconnection: close\r\n\r\n{}",
                response_body
            );
            socket
                .write_all(response.as_bytes())
                .await
                .expect("write queued participant response");
        }
    });

    let mut config = AgentConfig::default();
    config.provider = PROVIDER_ID_OPENAI.to_string();
    config.base_url = format!("http://{addr}/v1");
    config.model = "gpt-5.4-mini".to_string();
    config.api_key = "test-key".to_string();
    config.auth_source = AuthSource::ApiKey;
    config.api_transport = ApiTransport::ChatCompletions;
    config.auto_retry = false;
    config.max_retries = 0;
    config.max_tool_loops = 1;
    let engine = Arc::new(AgentEngine::new_test(manager, config, root.path()).await);
    let thread_id = "thread_participant_queue_auto_send";

    engine.threads.write().await.insert(
        thread_id.to_string(),
        AgentThread {
            id: thread_id.to_string(),
            agent_name: Some(crate::agent::agent_identity::MAIN_AGENT_NAME.to_string()),
            title: "Participant queue auto send".to_string(),
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

    let send_task = tokio::spawn({
        let engine = engine.clone();
        async move {
            engine
                .resend_existing_user_message(thread_id, "hello")
                .await
        }
    });

    timeout(Duration::from_secs(1), first_request_started.notified())
        .await
        .expect("initial resend should start");
    timeout(Duration::from_secs(1), async {
        loop {
            let streams = engine.stream_cancellations.lock().await;
            if streams.contains_key(thread_id) {
                break;
            }
            drop(streams);
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("active stream should be registered");

    let queued = engine
        .queue_thread_participant_suggestion(thread_id, "weles", "Queued participant note.", false)
        .await
        .expect("queue participant suggestion while stream is active");
    assert_eq!(queued.status, ThreadParticipantSuggestionStatus::Queued);

    release_first_response.notify_waiters();

    timeout(Duration::from_secs(2), send_task)
        .await
        .expect("resend plus queued follow-up should finish")
        .expect("join resend task")
        .expect("resend plus queued follow-up should succeed");

    let thread_messages = {
        let threads = engine.threads.read().await;
        threads
            .get(thread_id)
            .expect("thread should still exist")
            .messages
            .clone()
    };
    let participant_idx = thread_messages
        .iter()
        .position(|message| {
            message.role == MessageRole::Assistant
                && message.author_agent_id.as_deref() == Some("weles")
                && message.content == "Queued participant note."
        })
        .expect("queued participant note should auto-post once the stream finishes");
    let follow_up = thread_messages[participant_idx + 1..]
        .iter()
        .find(|message| {
            message.role == MessageRole::Assistant
                && message.author_agent_id.as_deref() != Some("weles")
        })
        .expect("queued participant note should wake the main agent once");
    assert_eq!(
        follow_up.content,
        "Swarozyc followed the queued participant note."
    );
    assert_eq!(
        request_counter.load(Ordering::SeqCst),
        3,
        "expected one main resend, one observer review, and one follow-up after auto-sending the queued participant note"
    );
    assert!(
        engine
            .list_thread_participant_suggestions(thread_id)
            .await
            .is_empty(),
        "auto-sent queued participant suggestion should be dismissed from the queue"
    );
}

#[tokio::test]
async fn queued_participant_suggestion_auto_sends_when_thread_is_already_idle() {
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
    let thread_id = "thread_participant_queue_auto_send_idle";

    engine.threads.write().await.insert(
        thread_id.to_string(),
        AgentThread {
            id: thread_id.to_string(),
            agent_name: Some(crate::agent::agent_identity::MAIN_AGENT_NAME.to_string()),
            title: "Participant queue auto send idle".to_string(),
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

    engine
        .queue_thread_participant_suggestion(thread_id, "weles", "Late idle participant note.", false)
        .await
        .expect("queue participant suggestion after thread is already idle");

    timeout(Duration::from_secs(2), async {
        loop {
            let thread_messages = {
                let threads = engine.threads.read().await;
                threads
                    .get(thread_id)
                    .expect("thread should still exist")
                    .messages
                    .clone()
            };
            let has_participant_note = thread_messages.iter().any(|message| {
                message.role == MessageRole::Assistant
                    && message.author_agent_id.as_deref() == Some("weles")
                    && message.content == "Late idle participant note."
            });
            let has_main_follow_up = thread_messages.iter().any(|message| {
                message.role == MessageRole::Assistant
                    && message.author_agent_id.as_deref() != Some("weles")
                    && message.content == "Gateway reply ok"
            });
            if has_participant_note && has_main_follow_up {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("queued participant note should auto-send even when it arrives after the thread is idle");

    assert_eq!(
        recorded_bodies.lock().expect("lock request log").len(),
        1,
        "idle auto-send should wake the main thread agent once"
    );
    assert!(
        engine
            .list_thread_participant_suggestions(thread_id)
            .await
            .is_empty(),
        "idle auto-sent suggestion should not remain queued"
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

#[tokio::test]
async fn resend_existing_user_message_runs_participant_observers_after_stream_completion() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let recorded_bodies = Arc::new(StdMutex::new(VecDeque::new()));
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind resend observer server");
    let addr = listener.local_addr().expect("resend observer addr");

    tokio::spawn({
        let recorded_bodies = recorded_bodies.clone();
        async move {
            loop {
                let Ok((mut socket, _)) = listener.accept().await else {
                    break;
                };
                let body = read_http_request_body(&mut socket)
                    .await
                    .expect("read request");
                recorded_bodies
                    .lock()
                    .expect("lock request log")
                    .push_back(body.clone());
                let response_body = if body.contains("Role: participant observer") {
                    concat!(
                        "data: {\"choices\":[{\"delta\":{\"content\":\"NO_SUGGESTION\"}}]}\n\n",
                        "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":5,\"completion_tokens\":2}}\n\n",
                        "data: [DONE]\n\n"
                    )
                } else {
                    concat!(
                        "data: {\"choices\":[{\"delta\":{\"content\":\"Main reply before observer.\"}}]}\n\n",
                        "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":7,\"completion_tokens\":3}}\n\n",
                        "data: [DONE]\n\n"
                    )
                };
                let response = format!(
                    "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncache-control: no-cache\r\nconnection: close\r\n\r\n{}",
                    response_body
                );
                socket
                    .write_all(response.as_bytes())
                    .await
                    .expect("write response");
            }
        }
    });

    let mut config = AgentConfig::default();
    config.provider = PROVIDER_ID_OPENAI.to_string();
    config.base_url = format!("http://{addr}/v1");
    config.model = "gpt-5.4-mini".to_string();
    config.api_key = "test-key".to_string();
    config.auth_source = AuthSource::ApiKey;
    config.api_transport = ApiTransport::ChatCompletions;
    config.auto_retry = false;
    config.max_retries = 0;
    config.max_tool_loops = 1;
    let engine = AgentEngine::new_test(manager, config, root.path()).await;
    let thread_id = "thread_resend_runs_participant_observers";

    engine.threads.write().await.insert(
        thread_id.to_string(),
        AgentThread {
            id: thread_id.to_string(),
            agent_name: Some(crate::agent::agent_identity::MAIN_AGENT_NAME.to_string()),
            title: "Resend observer hook".to_string(),
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

    recorded_bodies
        .lock()
        .expect("lock request log")
        .clear();

    engine
        .resend_existing_user_message(thread_id, "hello")
        .await
        .expect("resend should succeed");

    let request_bodies = recorded_bodies
        .lock()
        .expect("lock request log")
        .iter()
        .cloned()
        .collect::<Vec<_>>();
    assert!(
        request_bodies
            .iter()
            .any(|body| body.contains("Role: participant observer")),
        "resend completion should trigger a participant observer prompt"
    );
}
