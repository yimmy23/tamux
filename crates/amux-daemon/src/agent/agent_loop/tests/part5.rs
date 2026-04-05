use super::*;

#[tokio::test]
async fn openai_done_event_exposes_provider_final_result() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind responses event server");
    let addr = listener.local_addr().expect("responses event server addr");

    tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.expect("accept");
        let _ = read_http_request(&mut socket, "responses event request").await;
        let response_body = concat!(
            "data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_event_final\"}}\n\n",
            "data: {\"type\":\"response.output_text.delta\",\"delta\":\"Event response\"}\n\n",
                "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_event_final\",\"object\":\"response\",\"status\":\"completed\",\"output\":[],\"usage\":{\"input_tokens\":5,\"output_tokens\":2},\"error\":null,\"metadata\":{\"source\":\"event-test\"}}}\n\n"
        );
        let response = format!(
            "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
            response_body.len(),
            response_body
        );
        socket
            .write_all(response.as_bytes())
            .await
            .expect("write responses event response");
    });

    let mut config = AgentConfig::default();
    config.provider = "openai".to_string();
    config.base_url = format!("http://{addr}/v1");
    config.model = "gpt-5.4-mini".to_string();
    config.api_key = "test-key".to_string();
    config.auth_source = AuthSource::ApiKey;
    config.api_transport = ApiTransport::Responses;
    config.auto_retry = false;
    config.max_retries = 0;
    config.max_tool_loops = 1;

    let engine = AgentEngine::new_test(manager, config, root.path()).await;
    let mut events = engine.subscribe();
    let thread_id = "thread-openai-provider-final-result-event";
    {
        let mut threads = engine.threads.write().await;
        threads.insert(
            thread_id.to_string(),
            crate::agent::types::AgentThread {
                id: thread_id.to_string(),
                agent_name: None,
                title: "OpenAI provider final result event".to_string(),
                messages: vec![crate::agent::types::AgentMessage::user("hello", 1)],
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
        .send_message_inner(
            Some(thread_id),
            "hello",
            None,
            None,
            None,
            None,
            None,
            None,
            true,
        )
        .await
        .expect("send message should complete");

    let mut done_provider_final_result = None;
    while let Ok(event) = events.try_recv() {
        if let AgentEvent::Done {
            provider_final_result,
            ..
        } = event
        {
            done_provider_final_result = provider_final_result;
        }
    }

    match done_provider_final_result.expect("done event should expose provider final result") {
        CompletionProviderFinalResult::OpenAiResponses(response) => {
            assert_eq!(response.id.as_deref(), Some("resp_event_final"));
            assert_eq!(response.output_text, "Event response");
            assert_eq!(response.input_tokens, Some(5));
            assert_eq!(response.output_tokens, Some(2));
            let terminal_response = response
                .response
                .as_ref()
                .expect("done event should expose canonical terminal response");
            assert_eq!(terminal_response.id, "resp_event_final");
            assert_eq!(terminal_response.object, "response");
            assert_eq!(terminal_response.status, "completed");
            assert_eq!(terminal_response.output, Vec::<serde_json::Value>::new());
            assert_eq!(terminal_response.usage.input_tokens, 5);
            assert_eq!(terminal_response.usage.output_tokens, 2);
                let response_json: serde_json::Value = serde_json::from_str(
                    response
                        .response_json
                        .as_deref()
                        .expect("raw terminal response JSON should be present on the done event"),
                )
                .expect("response_json should decode");
                assert_eq!(response_json["metadata"]["source"], "event-test");
        }
        other => panic!("expected OpenAI Responses final result, got {other:?}"),
    }
}

#[tokio::test]
async fn anthropic_done_event_exposes_provider_final_result() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind anthropic event server");
    let addr = listener.local_addr().expect("anthropic event server addr");

    tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.expect("accept");
        let _ = read_http_request(&mut socket, "anthropic event request").await;
        let response_body = concat!(
            "data: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_event_final\",\"type\":\"message\",\"role\":\"assistant\",\"model\":\"claude-sonnet-4-20250514\",\"usage\":{\"input_tokens\":5}}}\n\n",
            "data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\n",
            "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"Anthropic event\"}}\n\n",
            "data: {\"type\":\"content_block_stop\",\"index\":0}\n\n",
            "data: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"},\"usage\":{\"output_tokens\":2}}\n\n",
            "data: {\"type\":\"message_stop\"}\n\n"
        );
        let response = format!(
            "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
            response_body.len(),
            response_body
        );
        socket
            .write_all(response.as_bytes())
            .await
            .expect("write anthropic event response");
    });

    let mut config = AgentConfig::default();
    config.provider = amux_shared::providers::PROVIDER_ID_ANTHROPIC.to_string();
    config.base_url = format!("http://{addr}/anthropic");
    config.model = "claude-sonnet-4-20250514".to_string();
    config.api_key = "test-key".to_string();
    config.auth_source = AuthSource::ApiKey;
    config.api_transport = ApiTransport::ChatCompletions;
    config.auto_retry = false;
    config.max_retries = 0;
    config.max_tool_loops = 1;

    let engine = AgentEngine::new_test(manager, config, root.path()).await;
    let mut events = engine.subscribe();
    let thread_id = "thread-anthropic-provider-final-result-event";
    {
        let mut threads = engine.threads.write().await;
        threads.insert(
            thread_id.to_string(),
            crate::agent::types::AgentThread {
                id: thread_id.to_string(),
                agent_name: None,
                title: "Anthropic provider final result event".to_string(),
                messages: vec![crate::agent::types::AgentMessage::user("hello", 1)],
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
        .send_message_inner(
            Some(thread_id),
            "hello",
            None,
            None,
            None,
            None,
            None,
            None,
            true,
        )
        .await
        .expect("send message should complete");

    let mut done_provider_final_result = None;
    while let Ok(event) = events.try_recv() {
        if let AgentEvent::Done {
            provider_final_result,
            ..
        } = event
        {
            done_provider_final_result = provider_final_result;
        }
    }

    match done_provider_final_result.expect("done event should expose provider final result") {
        CompletionProviderFinalResult::AnthropicMessage(message) => {
            assert_eq!(message.id.as_deref(), Some("msg_event_final"));
            assert_eq!(message.content_blocks.len(), 1);
            assert_eq!(message.content_blocks[0].text.as_deref(), Some("Anthropic event"));
        }
        other => panic!("expected Anthropic final result, got {other:?}"),
    }
}

#[tokio::test]
async fn concierge_recovery_persists_upstream_recovery_causal_trace() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

    let structured = StructuredUpstreamFailure {
        class: "request_invalid".to_string(),
        summary:
            "provider rejected the daemon request as invalid: Invalid 'input[12].name': empty string"
                .to_string(),
        diagnostics: serde_json::json!({
            "raw_message": "Invalid 'input[12].name': empty string"
        }),
    };
    let mut attempted = std::collections::HashSet::new();

    let disposition = engine
        .maybe_recover_fixable_upstream_failure(
            "thread-recovery-trace",
            &structured,
            false,
            false,
            &mut attempted,
        )
        .await
        .expect("recovery evaluation should succeed");

    assert!(disposition.started_investigation);
    assert!(disposition.retry_attempted);

    let records = engine
        .history
        .list_recent_causal_trace_records("upstream_recovery", 4)
        .await
        .expect("list recovery traces");
    assert_eq!(records.len(), 1, "expected one persisted recovery trace");

    let factors = serde_json::from_str::<Vec<crate::agent::learning::traces::CausalFactor>>(
        &records[0].causal_factors_json,
    )
    .expect("deserialize recovery factors");
    assert!(
        factors.iter().any(|factor| {
            factor.description == "upstream signature: request-invalid-empty-tool-name"
        }),
        "expected the persisted trace to include the upstream recovery signature"
    );

    let outcome = serde_json::from_str::<crate::agent::learning::traces::CausalTraceOutcome>(
        &records[0].outcome_json,
    )
    .expect("deserialize recovery outcome");
    match outcome {
        crate::agent::learning::traces::CausalTraceOutcome::NearMiss {
            what_went_wrong,
            how_recovered,
        } => {
            assert!(what_went_wrong.contains("empty string"));
            assert!(how_recovered.contains("repair"));
        }
        other => panic!("expected a near-miss recovery trace, got {other:?}"),
    }
}
