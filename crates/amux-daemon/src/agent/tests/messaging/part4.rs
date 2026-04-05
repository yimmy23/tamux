use super::*;
use amux_shared::providers::{PROVIDER_ID_ANTHROPIC, PROVIDER_ID_OPENAI};

#[tokio::test]
async fn send_direct_message_returns_upstream_message() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind anthropic direct-message server");
    let addr = listener.local_addr().expect("anthropic direct-message addr");

    tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.expect("accept");
        let _ = read_http_request_body(&mut socket)
            .await
            .expect("read anthropic request");
        let response_body = concat!(
            "data: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_direct_upstream\",\"type\":\"message\",\"role\":\"assistant\",\"model\":\"claude-sonnet-4-20250514\",\"usage\":{\"input_tokens\":2}}}\n\n",
            "data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\n",
            "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"Direct reply\"}}\n\n",
            "data: {\"type\":\"content_block_stop\",\"index\":0}\n\n",
            "data: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"},\"usage\":{\"output_tokens\":4}}\n\n",
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
            .expect("write anthropic direct-message response");
    });

    let mut config = AgentConfig::default();
    config.provider = PROVIDER_ID_ANTHROPIC.to_string();
    config.base_url = format!("http://{addr}/anthropic");
    config.model = "claude-sonnet-4-20250514".to_string();
    config.api_key = "test-key".to_string();
    config.auth_source = AuthSource::ApiKey;
    config.api_transport = ApiTransport::ChatCompletions;
    config.auto_retry = false;
    config.max_retries = 0;
    config.max_tool_loops = 1;

    let engine = AgentEngine::new_test(manager, config, root.path()).await;

    let result = engine
        .send_direct_message("main", None, None, "hello")
        .await
        .expect("direct message should complete");

    assert!(!result.thread_id.trim().is_empty());
    assert_eq!(result.response, "Direct reply");
    let upstream = result
        .upstream_message
        .as_ref()
        .expect("direct message result should expose upstream message");
    assert_eq!(upstream.id.as_deref(), Some("msg_direct_upstream"));
    assert_eq!(upstream.content_blocks.len(), 1);
    assert_eq!(upstream.content_blocks[0].text.as_deref(), Some("Direct reply"));

    match result
        .provider_final_result
        .as_ref()
        .expect("direct message result should expose provider-native final result")
    {
        CompletionProviderFinalResult::AnthropicMessage(message) => {
            assert_eq!(message.id.as_deref(), Some("msg_direct_upstream"));
            assert_eq!(message.content_blocks.len(), 1);
            assert_eq!(message.content_blocks[0].text.as_deref(), Some("Direct reply"));
        }
        other => panic!("expected Anthropic final result, got {other:?}"),
    }
}

#[tokio::test]
async fn send_direct_message_returns_openai_responses_final_result() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind responses direct-message server");
    let addr = listener.local_addr().expect("responses direct-message addr");

    tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.expect("accept");
        let _ = read_http_request_body(&mut socket)
            .await
            .expect("read responses request");
        let response_body = concat!(
            "data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_direct_final\"}}\n\n",
            "data: {\"type\":\"response.output_text.delta\",\"delta\":\"Direct responses reply\"}\n\n",
            "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_direct_final\",\"object\":\"response\",\"status\":\"completed\",\"output\":[],\"usage\":{\"input_tokens\":7,\"output_tokens\":3},\"error\":null,\"metadata\":{\"source\":\"direct-test\"}}}\n\n"
        );
        let response = format!(
            "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
            response_body.len(),
            response_body
        );
        socket
            .write_all(response.as_bytes())
            .await
            .expect("write responses direct-message response");
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

    let result = engine
        .send_direct_message("main", None, None, "hello")
        .await
        .expect("direct message should complete");

    assert_eq!(result.response, "Direct responses reply");
    let final_result = result
        .provider_final_result
        .as_ref()
        .expect("direct message result should expose provider-native final result");
    match final_result {
        CompletionProviderFinalResult::OpenAiResponses(response) => {
            assert_eq!(response.id.as_deref(), Some("resp_direct_final"));
            assert_eq!(response.output_text, "Direct responses reply");
            assert_eq!(response.input_tokens, Some(7));
            assert_eq!(response.output_tokens, Some(3));
            let terminal_response = response
                .response
                .as_ref()
                .expect("canonical terminal response should be preserved");
            assert_eq!(terminal_response.id, "resp_direct_final");
            assert_eq!(terminal_response.object, "response");
            assert_eq!(terminal_response.status, "completed");
            assert_eq!(terminal_response.output, Vec::<serde_json::Value>::new());
            assert_eq!(terminal_response.usage.input_tokens, 7);
            assert_eq!(terminal_response.usage.output_tokens, 3);
            assert_eq!(terminal_response.usage.total_tokens, None);
            let response_json: serde_json::Value = serde_json::from_str(
                response
                    .response_json
                    .as_deref()
                    .expect("raw terminal response JSON should be preserved"),
            )
            .expect("response_json should decode");
            assert_eq!(response_json["metadata"]["source"], "direct-test");
        }
        other => panic!("expected OpenAI Responses final result, got {other:?}"),
    }
}

#[tokio::test]
async fn send_direct_message_returns_openai_chat_completions_final_result() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind chat completions direct-message server");
    let addr = listener.local_addr().expect("chat completions direct-message addr");

    tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.expect("accept");
        let _ = read_http_request_body(&mut socket)
            .await
            .expect("read chat completions request");
        let response_body = concat!(
            "data: {\"id\":\"chatcmpl_direct_final\",\"model\":\"gpt-5.4-mini\",\"choices\":[{\"delta\":{\"content\":\"Direct chat reply\"}}]}\n\n",
            "data: {\"id\":\"chatcmpl_direct_final\",\"model\":\"gpt-5.4-mini\",\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":9,\"completion_tokens\":4}}\n\n",
            "data: [DONE]\n\n"
        );
        let response = format!(
            "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
            response_body.len(),
            response_body
        );
        socket
            .write_all(response.as_bytes())
            .await
            .expect("write chat completions direct-message response");
    });

    let mut config = AgentConfig::default();
    config.provider = "openai".to_string();
    config.base_url = format!("http://{addr}/v1");
    config.model = "gpt-5.4-mini".to_string();
    config.api_key = "test-key".to_string();
    config.auth_source = AuthSource::ApiKey;
    config.api_transport = ApiTransport::ChatCompletions;
    config.auto_retry = false;
    config.max_retries = 0;
    config.max_tool_loops = 1;

    let engine = AgentEngine::new_test(manager, config, root.path()).await;

    let result = engine
        .send_direct_message("main", None, None, "hello")
        .await
        .expect("direct message should complete");

    assert_eq!(result.response, "Direct chat reply");
    let final_result = result
        .provider_final_result
        .as_ref()
        .expect("direct message result should expose provider-native final result");
    match final_result {
        CompletionProviderFinalResult::OpenAiChatCompletions(response) => {
            assert_eq!(response.id.as_deref(), Some("chatcmpl_direct_final"));
            assert_eq!(response.model.as_deref(), Some("gpt-5.4-mini"));
            assert_eq!(response.output_text, "Direct chat reply");
            assert_eq!(response.finish_reason.as_deref(), Some("stop"));
            assert_eq!(response.input_tokens, Some(9));
            assert_eq!(response.output_tokens, Some(4));
        }
        other => panic!("expected OpenAI Chat Completions final result, got {other:?}"),
    }
}