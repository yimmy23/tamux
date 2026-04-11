use super::*;
use amux_shared::providers::PROVIDER_ID_OPENAI;
use tempfile::TempDir;
use tokio::time::{timeout, Duration};

async fn make_runner_test_engine(config: AgentConfig) -> (Arc<AgentEngine>, TempDir) {
    let temp_dir = TempDir::new().expect("temp dir");
    let session_manager = SessionManager::new_test(temp_dir.path()).await;
    let history = HistoryStore::new_test_store(temp_dir.path())
        .await
        .expect("history store");
    let data_dir = temp_dir.path().join("agent");
    std::fs::create_dir_all(&data_dir).expect("create agent data dir");
    let engine = AgentEngine::new_with_storage_and_http_client(
        session_manager,
        config,
        history,
        data_dir,
        reqwest::Client::new(),
    );
    (engine, temp_dir)
}

#[tokio::test]
async fn participant_runner_enqueues_suggestion() {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("addr");

    tokio::spawn(async move {
        for _ in 0..3 {
            let Ok((mut socket, _)) = listener.accept().await else {
                break;
            };
            let _ = read_http_request_body(&mut socket).await.expect("read request");
            let response_body = concat!(
                "data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_participant_runner\"}}\n\n",
                "data: {\"type\":\"response.output_text.delta\",\"delta\":\"FORCE: no\\nMESSAGE: Verify claim X before sending.\"}\n\n",
                "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_participant_runner\",\"object\":\"response\",\"status\":\"completed\",\"output\":[],\"usage\":{\"input_tokens\":6,\"output_tokens\":7},\"error\":null}}\n\n"
            );
            let response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                response_body.len(),
                response_body
            );
            socket.write_all(response.as_bytes()).await.expect("write response");
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
    let (engine, _temp_dir) = make_runner_test_engine(config).await;
    let thread_id = "thread_participant_runner";

    engine.threads.write().await.insert(
        thread_id.to_string(),
        AgentThread {
            id: thread_id.to_string(),
            agent_name: Some(crate::agent::agent_identity::MAIN_AGENT_NAME.to_string()),
            title: "Participant runner".to_string(),
            messages: vec![AgentMessage::user("Check claim X", 1)],
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

    timeout(
        Duration::from_secs(2),
        engine.run_participant_observers(thread_id),
    )
    .await
    .expect("participant observers should complete without stalling")
    .expect("participant observers should run");

    let suggestions = engine.list_thread_participant_suggestions(thread_id).await;
    assert_eq!(suggestions.len(), 1);
    assert_eq!(suggestions[0].instruction, "Verify claim X before sending.");
}

#[tokio::test]
async fn participant_runner_force_send_posts_visible_message() {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("addr");

    tokio::spawn(async move {
        let mut request_count = 0usize;
        loop {
            let Ok((mut socket, _)) = listener.accept().await else {
                break;
            };
            let body = read_http_request_body(&mut socket).await.expect("read request");
            request_count += 1;
            let response_body = match request_count {
                1 => concat!(
                    "data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_participant_runner_force_observer\"}}\n\n",
                    "data: {\"type\":\"response.output_text.delta\",\"delta\":\"FORCE: yes\\nMESSAGE: Visible participant post.\"}\n\n",
                    "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_participant_runner_force_observer\",\"object\":\"response\",\"status\":\"completed\",\"output\":[],\"usage\":{\"input_tokens\":6,\"output_tokens\":7},\"error\":null}}\n\n"
                ),
                2 => concat!(
                    "data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_participant_runner_force_visible\"}}\n\n",
                    "data: {\"type\":\"response.output_text.delta\",\"delta\":\"Visible participant post.\"}\n\n",
                    "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_participant_runner_force_visible\",\"object\":\"response\",\"status\":\"completed\",\"output\":[],\"usage\":{\"input_tokens\":4,\"output_tokens\":2},\"error\":null}}\n\n"
                ),
                _ => concat!(
                    "data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_participant_runner_force_tail\"}}\n\n",
                    "data: {\"type\":\"response.output_text.delta\",\"delta\":\"ok\"}\n\n",
                    "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_participant_runner_force_tail\",\"object\":\"response\",\"status\":\"completed\",\"output\":[],\"usage\":{\"input_tokens\":1,\"output_tokens\":1},\"error\":null}}\n\n"
                ),
            };
            let response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                response_body.len(),
                response_body
            );
            socket.write_all(response.as_bytes()).await.expect("write response");
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
    let (engine, _temp_dir) = make_runner_test_engine(config).await;
    let thread_id = "thread_participant_runner_force";

    engine.threads.write().await.insert(
        thread_id.to_string(),
        AgentThread {
            id: thread_id.to_string(),
            agent_name: Some(crate::agent::agent_identity::MAIN_AGENT_NAME.to_string()),
            title: "Participant runner force".to_string(),
            messages: vec![AgentMessage::user("Check claim X", 1)],
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

    timeout(Duration::from_secs(2), engine.run_participant_observers(thread_id))
    .await
    .expect("participant observers should complete without stalling")
    .expect("participant observers should run");

    let threads = engine.threads.read().await;
    let thread = threads
        .get(thread_id)
        .expect("thread should still exist after participant send");
    let assistant = thread
        .messages
        .iter()
        .rev()
        .find(|message| {
            message.role == MessageRole::Assistant
                && message.author_agent_id.as_deref() == Some("weles")
        })
        .expect("force-send participant post should stay on the visible thread");

    assert_eq!(assistant.content, "Visible participant post.");
    assert_eq!(assistant.author_agent_name.as_deref(), Some("Weles"));
    assert_eq!(
        thread
            .messages
            .iter()
            .filter(|message| message.role == MessageRole::User)
            .count(),
        1,
        "force-send participant posts should not add an extra user turn"
    );
    drop(threads);

    let dm_thread_id = crate::agent::agent_identity::internal_dm_thread_id(
        crate::agent::agent_identity::MAIN_AGENT_ID,
        "weles",
    );
    assert!(
        engine.get_thread(&dm_thread_id).await.is_none(),
        "force-send participant posts should not create a hidden DM thread"
    );

    let suggestions = engine.list_thread_participant_suggestions(thread_id).await;
    assert!(
        suggestions.is_empty(),
        "force-send participant posts should not leave queued or failed suggestions behind"
    );
}

#[tokio::test]
async fn participant_runner_uses_single_visible_snapshot_per_cycle() {
    let recorded_bodies = Arc::new(StdMutex::new(Vec::new()));
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("addr");

    tokio::spawn({
        let recorded_bodies = recorded_bodies.clone();
        async move {
            let mut participant_prompt_count = 0usize;

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
                    .push(body.clone());
                let response_body = if body.contains("Role: participant observer") {
                    participant_prompt_count += 1;
                    match participant_prompt_count {
                        1 => concat!(
                            "data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_participant_runner_snapshot_1\"}}\n\n",
                            "data: {\"type\":\"response.output_text.delta\",\"delta\":\"FORCE: yes\\nMESSAGE: First participant visible post.\"}\n\n",
                            "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_participant_runner_snapshot_1\",\"object\":\"response\",\"status\":\"completed\",\"output\":[],\"usage\":{\"input_tokens\":6,\"output_tokens\":7},\"error\":null}}\n\n"
                        ),
                        2 => concat!(
                            "data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_participant_runner_snapshot_2\"}}\n\n",
                            "data: {\"type\":\"response.output_text.delta\",\"delta\":\"FORCE: no\\nMESSAGE: Second participant follow-up.\"}\n\n",
                            "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_participant_runner_snapshot_2\",\"object\":\"response\",\"status\":\"completed\",\"output\":[],\"usage\":{\"input_tokens\":5,\"output_tokens\":3},\"error\":null}}\n\n"
                        ),
                        _ => concat!(
                            "data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_participant_runner_snapshot_tail\"}}\n\n",
                            "data: {\"type\":\"response.output_text.delta\",\"delta\":\"ok\"}\n\n",
                            "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_participant_runner_snapshot_tail\",\"object\":\"response\",\"status\":\"completed\",\"output\":[],\"usage\":{\"input_tokens\":1,\"output_tokens\":1},\"error\":null}}\n\n"
                        ),
                    }
                } else if body.contains("First participant visible post.") {
                    concat!(
                        "data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_participant_runner_snapshot_visible\"}}\n\n",
                        "data: {\"type\":\"response.output_text.delta\",\"delta\":\"First participant visible post.\"}\n\n",
                        "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_participant_runner_snapshot_visible\",\"object\":\"response\",\"status\":\"completed\",\"output\":[],\"usage\":{\"input_tokens\":4,\"output_tokens\":2},\"error\":null}}\n\n"
                    )
                } else {
                    concat!(
                        "data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_participant_runner_snapshot_other\"}}\n\n",
                        "data: {\"type\":\"response.output_text.delta\",\"delta\":\"ok\"}\n\n",
                        "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_participant_runner_snapshot_other\",\"object\":\"response\",\"status\":\"completed\",\"output\":[],\"usage\":{\"input_tokens\":1,\"output_tokens\":1},\"error\":null}}\n\n"
                    )
                };

                let response = format!(
                    "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                    response_body.len(),
                    response_body
                );
                socket.write_all(response.as_bytes()).await.expect("write response");
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
    let (engine, _temp_dir) = make_runner_test_engine(config).await;
    let thread_id = "thread_participant_runner_snapshot";

    engine.threads.write().await.insert(
        thread_id.to_string(),
        AgentThread {
            id: thread_id.to_string(),
            agent_name: Some(crate::agent::agent_identity::MAIN_AGENT_NAME.to_string()),
            title: "Participant runner snapshot".to_string(),
            messages: vec![AgentMessage::user("Check claim X", 1)],
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
        .expect("first participant should register");
    engine
        .upsert_thread_participant(thread_id, "swarog", "verify the second detail")
        .await
        .expect("second participant should register");

    engine
        .run_participant_observers(thread_id)
        .await
        .expect("participant observers should run");

    let visible_snapshot_bodies: Vec<String> = recorded_bodies
        .lock()
        .expect("lock request log")
        .iter()
        .filter(|body| {
            body.contains("Role: participant observer")
                && body.contains("verify the second detail")
        })
        .cloned()
        .collect();

    assert!(
        !visible_snapshot_bodies.is_empty(),
        "expected a prompt body for the second participant"
    );
    assert!(
        visible_snapshot_bodies
            .iter()
            .all(|body| !body.contains("First participant visible post.")),
        "the second participant should use the original visible-thread snapshot for the same observer cycle"
    );
}

#[tokio::test]
async fn participant_runner_skips_no_suggestion() {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("addr");

    tokio::spawn(async move {
        for _ in 0..3 {
            let Ok((mut socket, _)) = listener.accept().await else {
                break;
            };
            let _ = read_http_request_body(&mut socket).await.expect("read request");
            let response_body = concat!(
                "data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_participant_runner_none\"}}\n\n",
                "data: {\"type\":\"response.output_text.delta\",\"delta\":\"NO_SUGGESTION\"}\n\n",
                "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_participant_runner_none\",\"object\":\"response\",\"status\":\"completed\",\"output\":[],\"usage\":{\"input_tokens\":4,\"output_tokens\":2},\"error\":null}}\n\n"
            );
            let response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                response_body.len(),
                response_body
            );
            socket.write_all(response.as_bytes()).await.expect("write response");
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
    let (engine, _temp_dir) = make_runner_test_engine(config).await;
    let thread_id = "thread_participant_runner_none";

    engine.threads.write().await.insert(
        thread_id.to_string(),
        AgentThread {
            id: thread_id.to_string(),
            agent_name: Some(crate::agent::agent_identity::MAIN_AGENT_NAME.to_string()),
            title: "Participant runner none".to_string(),
            messages: vec![AgentMessage::user("noop", 1)],
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
        .run_participant_observers(thread_id)
        .await
        .expect("participant observers should run");

    assert!(engine
        .list_thread_participant_suggestions(thread_id)
        .await
        .is_empty());
}

#[tokio::test]
async fn participant_suggestions_not_visible_to_participants() {
    let (engine, _temp_dir) = make_runner_test_engine(AgentConfig::default()).await;
    let thread_id = "thread_participant_prompt_hidden_suggestion";

    engine.threads.write().await.insert(
        thread_id.to_string(),
        AgentThread {
            id: thread_id.to_string(),
            agent_name: Some(crate::agent::agent_identity::MAIN_AGENT_NAME.to_string()),
            title: "Prompt visibility".to_string(),
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
        .queue_thread_participant_suggestion(thread_id, "weles", "Hidden", false)
        .await
        .expect("queue suggestion");

    let prompt = engine
        .build_participant_prompt(thread_id, "weles")
        .await
        .expect("prompt should build");
    assert!(!prompt.contains("Hidden"));
}

#[tokio::test]
async fn participant_prompt_excludes_internal_delegation() {
    let (engine, _temp_dir) = make_runner_test_engine(AgentConfig::default()).await;
    let thread_id = "thread_participant_prompt_internal_delegate";

    engine.threads.write().await.insert(
        thread_id.to_string(),
        AgentThread {
            id: thread_id.to_string(),
            agent_name: Some(crate::agent::agent_identity::MAIN_AGENT_NAME.to_string()),
            title: "Prompt visibility".to_string(),
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
        .append_internal_delegate_message(thread_id, "secret")
        .await
        .expect("append internal delegate message");

    let prompt = engine
        .build_participant_prompt(thread_id, "weles")
        .await
        .expect("prompt should build");
    assert!(!prompt.contains("secret"));
}
