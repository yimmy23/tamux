use super::*;
use amux_shared::providers::PROVIDER_ID_OPENAI;

#[tokio::test]
async fn persisted_assistant_messages_reload_provider_final_result_metadata() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let thread_id = "thread_provider_final_result_reload";

    {
        let mut threads = engine.threads.write().await;
        threads.insert(
            thread_id.to_string(),
            AgentThread {
                id: thread_id.to_string(),
                agent_name: None,
                title: "Provider final result reload".to_string(),
                messages: vec![
                    AgentMessage::user("hello", 1),
                    AgentMessage {
                        id: "assistant-provider-final-result".to_string(),
                        role: MessageRole::Assistant,
                        content: "Hello from Responses.".to_string(),
                        tool_calls: None,
                        tool_call_id: None,
                        tool_name: None,
                        tool_arguments: None,
                        tool_status: None,
                        weles_review: None,
                        input_tokens: 7,
                        output_tokens: 3,
                        provider: Some(amux_shared::providers::PROVIDER_ID_OPENAI.to_string()),
                        model: Some("gpt-5.4-mini".to_string()),
                        api_transport: Some(ApiTransport::Responses),
                        response_id: Some("resp_provider_final_result".to_string()),
                        upstream_message: None,
                        provider_final_result: Some(
                            CompletionProviderFinalResult::OpenAiResponses(
                                CompletionOpenAiResponsesFinalResult {
                                    id: Some("resp_provider_final_result".to_string()),
                                    output_text: "Hello from Responses.".to_string(),
                                    reasoning: None,
                                    tool_calls: Vec::new(),
                                    response: Some(
                                        crate::agent::llm_client::OpenAiResponsesTerminalResponse {
                                            id: "resp_provider_final_result".to_string(),
                                            object: "response".to_string(),
                                            status: "completed".to_string(),
                                            output: Vec::new(),
                                            usage: crate::agent::llm_client::OpenAiResponsesResponseUsage {
                                                input_tokens: 7,
                                                output_tokens: 3,
                                                total_tokens: None,
                                            },
                                            error: None,
                                        },
                                    ),
                                    response_json: Some(r#"{"id":"resp_provider_final_result","object":"response","status":"completed","output":[],"usage":{"input_tokens":7,"output_tokens":3},"error":null,"metadata":{"source":"persisted-test"}}"#.to_string()),
                                    input_tokens: Some(7),
                                    output_tokens: Some(3),
                                },
                            ),
                        ),
                        author_agent_id: None,
                        author_agent_name: None,
                        reasoning: None,
                        message_kind: AgentMessageKind::Normal,
                        compaction_strategy: None,
                        compaction_payload: None,
                        offloaded_payload_id: None,
                        structural_refs: Vec::new(),
                        timestamp: 2,
                    },
                ],
                pinned: false,
                upstream_thread_id: None,
                upstream_transport: None,
                upstream_provider: None,
                upstream_model: None,
                upstream_assistant_id: None,
                total_input_tokens: 7,
                total_output_tokens: 3,
                created_at: 1,
                updated_at: 2,
            },
        );
    }

    engine.persist_thread_by_id(thread_id).await;
    drop(engine);

    let manager = SessionManager::new_test(root.path()).await;
    let reloaded_engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    reloaded_engine.hydrate().await.expect("hydrate");
    let threads = reloaded_engine.threads.read().await;
    let thread = threads.get(thread_id).expect("thread should reload");
    let assistant = thread
        .messages
        .iter()
        .find(|message| message.role == MessageRole::Assistant)
        .expect("assistant message should reload");

    match assistant
        .provider_final_result
        .as_ref()
        .expect("provider final result should reload")
    {
        CompletionProviderFinalResult::OpenAiResponses(response) => {
            assert_eq!(response.id.as_deref(), Some("resp_provider_final_result"));
            assert_eq!(response.output_text, "Hello from Responses.");
            assert_eq!(response.input_tokens, Some(7));
            assert_eq!(response.output_tokens, Some(3));
            let terminal_response = response
                .response
                .as_ref()
                .expect("canonical terminal response should reload");
            assert_eq!(terminal_response.id, "resp_provider_final_result");
            assert_eq!(terminal_response.object, "response");
            assert_eq!(terminal_response.status, "completed");
            assert_eq!(terminal_response.output, Vec::<serde_json::Value>::new());
            assert_eq!(terminal_response.usage.input_tokens, 7);
            assert_eq!(terminal_response.usage.output_tokens, 3);
            let response_json: serde_json::Value = serde_json::from_str(
                response
                    .response_json
                    .as_deref()
                    .expect("raw terminal response JSON should reload"),
            )
            .expect("response_json should decode");
            assert_eq!(response_json["metadata"]["source"], "persisted-test");
        }
        other => panic!("expected OpenAI Responses final result, got {other:?}"),
    }
}

#[tokio::test]
async fn persisted_thread_metadata_reloads_thread_participants() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let thread_id = "thread_participants_reload";

    {
        let mut threads = engine.threads.write().await;
        threads.insert(
            thread_id.to_string(),
            AgentThread {
                id: thread_id.to_string(),
                agent_name: Some(crate::agent::agent_identity::MAIN_AGENT_NAME.to_string()),
                title: "Participants reload".to_string(),
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
    }
    engine.thread_participants.write().await.insert(
        thread_id.to_string(),
        vec![
            crate::agent::ThreadParticipantState {
                agent_id: "weles".to_string(),
                agent_name: "Weles".to_string(),
                instruction: "verify claims".to_string(),
                status: crate::agent::ThreadParticipantStatus::Active,
                created_at: 10,
                updated_at: 11,
                deactivated_at: None,
                last_contribution_at: Some(12),
            },
            crate::agent::ThreadParticipantState {
                agent_id: "rarog".to_string(),
                agent_name: "Rarog".to_string(),
                instruction: "watch performance".to_string(),
                status: crate::agent::ThreadParticipantStatus::Inactive,
                created_at: 20,
                updated_at: 21,
                deactivated_at: Some(22),
                last_contribution_at: None,
            },
        ],
    );

    engine.persist_thread_by_id(thread_id).await;
    drop(engine);

    let manager = SessionManager::new_test(root.path()).await;
    let reloaded_engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    reloaded_engine.hydrate().await.expect("hydrate");

    let participants = reloaded_engine
        .thread_participants
        .read()
        .await
        .get(thread_id)
        .cloned()
        .expect("participants should reload");

    assert_eq!(participants.len(), 2);
    assert_eq!(participants[0].agent_id, "weles");
    assert_eq!(participants[0].instruction, "verify claims");
    assert_eq!(
        participants[0].status,
        crate::agent::ThreadParticipantStatus::Active
    );
    assert_eq!(participants[0].last_contribution_at, Some(12));
    assert_eq!(participants[1].agent_id, "rarog");
    assert_eq!(
        participants[1].status,
        crate::agent::ThreadParticipantStatus::Inactive
    );
    assert_eq!(participants[1].deactivated_at, Some(22));
}

#[tokio::test]
async fn visible_thread_participant_send_records_message_author_and_updates_participant_state() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind participant visible-send server");
    let addr = listener
        .local_addr()
        .expect("participant visible-send addr");

    tokio::spawn(async move {
        for _ in 0..3 {
            let Ok((mut socket, _)) = listener.accept().await else {
                break;
            };
            let _ = read_http_request_body(&mut socket)
                .await
                .expect("read participant visible-send request");
            let response_body = concat!(
                "data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_participant_visible\"}}\n\n",
                "data: {\"type\":\"response.output_text.delta\",\"delta\":\"I checked that claim and it is inaccurate.\"}\n\n",
                "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_participant_visible\",\"object\":\"response\",\"status\":\"completed\",\"output\":[],\"usage\":{\"input_tokens\":11,\"output_tokens\":8},\"error\":null}}\n\n"
            );
            let response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                response_body.len(),
                response_body
            );
            socket
                .write_all(response.as_bytes())
                .await
                .expect("write participant visible-send response");
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
    let thread_id = "thread_participant_visible_send";

    {
        let mut threads = engine.threads.write().await;
        threads.insert(
            thread_id.to_string(),
            AgentThread {
                id: thread_id.to_string(),
                agent_name: Some(crate::agent::agent_identity::MAIN_AGENT_NAME.to_string()),
                title: "Participant visible send".to_string(),
                messages: vec![AgentMessage::user("Can someone verify this?", 1)],
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
    }

    engine
        .upsert_thread_participant(thread_id, "weles", "verify claims and jump in when needed")
        .await
        .expect("participant should register");

    engine
        .send_visible_thread_participant_message(
            thread_id,
            "weles",
            None,
            "The operator asked for a factual verification. Reply only if you found something important.",
        )
        .await
        .expect("participant visible send should succeed");

    let threads = engine.threads.read().await;
    let thread = threads
        .get(thread_id)
        .expect("thread should still exist after participant send");
    let assistant = thread
        .messages
        .iter()
        .rev()
        .find(|message| message.role == MessageRole::Assistant)
        .expect("participant visible send should append an assistant message");

    assert_eq!(
        assistant.content,
        "I checked that claim and it is inaccurate."
    );
    assert_eq!(assistant.author_agent_id.as_deref(), Some("weles"));
    assert_eq!(assistant.author_agent_name.as_deref(), Some("Weles"));
    assert_eq!(
        thread
            .messages
            .iter()
            .filter(|message| message.role == MessageRole::User)
            .count(),
        1,
        "visible participant sends should not append an extra user turn"
    );
    drop(threads);

    let participants = engine.list_thread_participants(thread_id).await;
    let weles = participants
        .iter()
        .find(|participant| participant.agent_id == "weles")
        .expect("weles participant should remain registered");
    assert!(
        weles.last_contribution_at.is_some(),
        "participant send should stamp last contribution time"
    );
}

#[tokio::test]
async fn force_send_interrupts_stream() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind participant force server");
    let addr = listener.local_addr().expect("participant force addr");

    tokio::spawn(async move {
        for _ in 0..3 {
            let Ok((mut socket, _)) = listener.accept().await else {
                break;
            };
            let _ = read_http_request_body(&mut socket)
                .await
                .expect("read participant force request");
            let response_body = concat!(
                "data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_participant_force\"}}\n\n",
                "data: {\"type\":\"response.output_text.delta\",\"delta\":\"Urgent fix\"}\n\n",
                "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_participant_force\",\"object\":\"response\",\"status\":\"completed\",\"output\":[],\"usage\":{\"input_tokens\":8,\"output_tokens\":4},\"error\":null}}\n\n"
            );
            let response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                response_body.len(),
                response_body
            );
            socket
                .write_all(response.as_bytes())
                .await
                .expect("write participant force response");
        }
    });
    let server_url = format!("http://{addr}/v1");

    let mut config = AgentConfig::default();
    config.provider = PROVIDER_ID_OPENAI.to_string();
    config.base_url = server_url;
    config.model = "gpt-5.4-mini".to_string();
    config.api_key = "test-key".to_string();
    config.auth_source = AuthSource::ApiKey;
    config.api_transport = ApiTransport::Responses;
    config.auto_retry = false;
    config.max_retries = 0;
    config.max_tool_loops = 1;

    let engine = AgentEngine::new_test(manager, config, root.path()).await;
    let thread_id = "thread_participant_force_interrupt";
    engine.threads.write().await.insert(
        thread_id.to_string(),
        AgentThread {
            id: thread_id.to_string(),
            agent_name: Some(crate::agent::agent_identity::MAIN_AGENT_NAME.to_string()),
            title: "Participant force interrupt".to_string(),
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

    let (_generation, token, _retry_now) = engine.begin_stream_cancellation(thread_id).await;

    let suggestion = engine
        .queue_thread_participant_suggestion(thread_id, "weles", "Urgent fix", true)
        .await
        .expect("force-send enqueue should succeed");

    assert_eq!(suggestion.target_agent_id, "weles");
    assert!(
        token.is_cancelled(),
        "force-send should cancel the active stream"
    );

    let threads = engine.threads.read().await;
    let last = threads
        .get(thread_id)
        .and_then(|thread| thread.messages.last())
        .expect("thread should have a last message");
    assert_eq!(last.author_agent_id.as_deref(), Some("weles"));
}

#[tokio::test]
async fn force_send_auto_posts_on_enqueue() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind participant auto-post server");
    let addr = listener.local_addr().expect("participant auto-post addr");

    tokio::spawn(async move {
        for _ in 0..3 {
            let Ok((mut socket, _)) = listener.accept().await else {
                break;
            };
            let _ = read_http_request_body(&mut socket)
                .await
                .expect("read participant auto-post request");
            let response_body = concat!(
                "data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_participant_force_auto\"}}\n\n",
                "data: {\"type\":\"response.output_text.delta\",\"delta\":\"Auto-posted fix\"}\n\n",
                "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_participant_force_auto\",\"object\":\"response\",\"status\":\"completed\",\"output\":[],\"usage\":{\"input_tokens\":8,\"output_tokens\":4},\"error\":null}}\n\n"
            );
            let response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                response_body.len(),
                response_body
            );
            socket
                .write_all(response.as_bytes())
                .await
                .expect("write participant auto-post response");
        }
    });
    let server_url = format!("http://{addr}/v1");

    let mut config = AgentConfig::default();
    config.provider = PROVIDER_ID_OPENAI.to_string();
    config.base_url = server_url;
    config.model = "gpt-5.4-mini".to_string();
    config.api_key = "test-key".to_string();
    config.auth_source = AuthSource::ApiKey;
    config.api_transport = ApiTransport::Responses;
    config.auto_retry = false;
    config.max_retries = 0;
    config.max_tool_loops = 1;

    let engine = AgentEngine::new_test(manager, config, root.path()).await;
    let thread_id = "thread_participant_force_auto";
    engine.threads.write().await.insert(
        thread_id.to_string(),
        AgentThread {
            id: thread_id.to_string(),
            agent_name: Some(crate::agent::agent_identity::MAIN_AGENT_NAME.to_string()),
            title: "Participant force auto".to_string(),
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

    let _ = engine
        .queue_thread_participant_suggestion(thread_id, "weles", "Auto-posted fix", true)
        .await
        .expect("force-send enqueue should succeed");

    let threads = engine.threads.read().await;
    let last = threads
        .get(thread_id)
        .and_then(|thread| thread.messages.last())
        .expect("thread should have a last message");
    assert_eq!(last.author_agent_id.as_deref(), Some("weles"));
}

#[tokio::test]
async fn participant_suggestions_fifo_order() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let thread_id = "thread_participant_fifo";
    engine.threads.write().await.insert(
        thread_id.to_string(),
        AgentThread {
            id: thread_id.to_string(),
            agent_name: Some(crate::agent::agent_identity::MAIN_AGENT_NAME.to_string()),
            title: "Participant FIFO".to_string(),
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

    let first = engine
        .queue_thread_participant_suggestion(thread_id, "weles", "A", false)
        .await
        .expect("first suggestion");
    let second = engine
        .queue_thread_participant_suggestion(thread_id, "weles", "B", false)
        .await
        .expect("second suggestion");
    let suggestions = engine.list_thread_participant_suggestions(thread_id).await;
    assert_eq!(suggestions[0].id, first.id);
    assert_eq!(suggestions[1].id, second.id);
}

#[tokio::test]
async fn deactivating_missing_thread_participant_returns_ok() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let thread_id = "thread-missing-participant";

    engine.threads.write().await.insert(
        thread_id.to_string(),
        AgentThread {
            id: thread_id.to_string(),
            agent_name: Some(crate::agent::agent_identity::MAIN_AGENT_NAME.to_string()),
            title: "Missing participant".to_string(),
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
        .apply_thread_participant_command(thread_id, "weles", "deactivate", None)
        .await
        .expect("missing participant deactivation should be non-fatal");

    assert!(
        engine.list_thread_participants(thread_id).await.is_empty(),
        "missing participant deactivation should not register a participant"
    );
}

#[tokio::test]
async fn internal_delegate_does_not_register_participant() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind internal delegate server");
    let addr = listener.local_addr().expect("internal delegate addr");

    tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.expect("accept");
        let _ = read_http_request_body(&mut socket)
            .await
            .expect("read internal delegate request");
        let response_body = concat!(
            "data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_internal_delegate\"}}\n\n",
            "data: {\"type\":\"response.output_text.delta\",\"delta\":\"Internal delegation complete.\"}\n\n",
            "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_internal_delegate\",\"object\":\"response\",\"status\":\"completed\",\"output\":[],\"usage\":{\"input_tokens\":5,\"output_tokens\":4},\"error\":null}}\n\n"
        );
        let response = format!(
            "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
            response_body.len(),
            response_body
        );
        socket
            .write_all(response.as_bytes())
            .await
            .expect("write internal delegate response");
    });

    let mut config = AgentConfig::default();
    config.provider = PROVIDER_ID_OPENAI.to_string();
    config.base_url = format!("http://{addr}/v1");
    config.model = "gpt-5.4-mini".to_string();
    let engine = AgentEngine::new_test(manager, config, root.path()).await;
    let thread_id = "thread-internal-delegate";

    engine.threads.write().await.insert(
        thread_id.to_string(),
        AgentThread {
            id: thread_id.to_string(),
            agent_name: Some(crate::agent::agent_identity::MAIN_AGENT_NAME.to_string()),
            title: "Internal delegate".to_string(),
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

    let before = engine.list_thread_participants(thread_id).await;
    engine
        .send_internal_delegate_message(Some(thread_id), "weles", None, "check the thread")
        .await
        .expect("internal delegate should succeed");
    let after = engine.list_thread_participants(thread_id).await;

    assert_eq!(before, after);
}

#[tokio::test]
async fn upserting_builtin_alias_stores_canonical_agent_name() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let thread_id = "thread-alias-canonical";

    engine.threads.write().await.insert(
        thread_id.to_string(),
        AgentThread {
            id: thread_id.to_string(),
            agent_name: Some(crate::agent::agent_identity::MAIN_AGENT_NAME.to_string()),
            title: "Canonical alias".to_string(),
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

    let participant = engine
        .upsert_thread_participant(thread_id, "swarog", "watch for regressions")
        .await
        .expect("builtin alias should resolve");

    assert_eq!(participant.agent_id, "svarog");
    assert_eq!(participant.agent_name, "Svarog");
}
