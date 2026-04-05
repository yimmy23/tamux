use super::*;

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
                        reasoning: None,
                        message_kind: AgentMessageKind::Normal,
                        compaction_strategy: None,
                        compaction_payload: None,
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