    #[tokio::test]
    async fn request_invalid_responses_422_invalid_payload_is_classified_request_invalid() {
        let request_paths = Arc::new(Mutex::new(VecDeque::new()));
        let chat_requests = Arc::new(AtomicUsize::new(0));
        let base_url = spawn_responses_error_server(
            "422 Unprocessable Entity",
            r#"{"error":{"message":"Invalid request body: input[3].content is required"}}"#
                .to_string(),
            None,
            request_paths,
            chat_requests,
        )
        .await;

        let stream = send_completion_request(
            &reqwest::Client::new(),
                amux_shared::providers::PROVIDER_ID_OPENAI,
            &responses_test_config(base_url, AuthSource::ApiKey),
            "system",
            &[ApiMessage {
                role: "user".to_string(),
                content: ApiContent::Text("hello".to_string()),
                tool_call_id: None,
                name: None,
                tool_calls: None,
            }],
            &[],
            ApiTransport::Responses,
            None,
            None,
            RetryStrategy::Bounded {
                max_retries: 0,
                retry_delay_ms: 0,
            },
        );

        let chunks = collect_chunks(stream).await;
        let message = match chunks.last().expect("terminal chunk") {
            CompletionChunk::Error { message } => message,
            other => panic!("expected error chunk, got {other:?}"),
        };
        let diagnostics = parse_structured_error(message);
        assert_eq!(diagnostics.class, "request_invalid");
        assert!(diagnostics.summary.contains("invalid"));
        assert!(diagnostics
            .diagnostics
            .to_string()
            .contains("input[3].content"));
    }

    #[tokio::test]
    async fn transport_incompatibility_responses_does_not_fallback_to_chat_completions() {
        let request_paths = Arc::new(Mutex::new(VecDeque::new()));
        let chat_requests = Arc::new(AtomicUsize::new(0));
        let base_url = spawn_responses_error_server(
            "405 Method Not Allowed",
            r#"{"error":{"message":"Responses API not supported here"}}"#.to_string(),
            Some(
                concat!(
                    "data: {\"choices\":[{\"delta\":{\"content\":\"legacy fallback\"}}]}\n\n",
                    "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":1,\"completion_tokens\":1}}\n\n",
                    "data: [DONE]\n\n"
                )
                .to_string(),
            ),
            request_paths.clone(),
            chat_requests.clone(),
        )
        .await;

        let stream = send_completion_request(
            &reqwest::Client::new(),
            "openai",
            &responses_test_config(base_url, AuthSource::ApiKey),
            "system",
            &[ApiMessage {
                role: "user".to_string(),
                content: ApiContent::Text("hello".to_string()),
                tool_call_id: None,
                name: None,
                tool_calls: None,
            }],
            &[],
            ApiTransport::Responses,
            None,
            None,
            RetryStrategy::Bounded {
                max_retries: 0,
                retry_delay_ms: 0,
            },
        );

        let chunks = collect_chunks(stream).await;
        assert!(matches!(chunks.last(), Some(CompletionChunk::Error { .. })));
        assert_eq!(chat_requests.load(Ordering::SeqCst), 0);
        let paths = request_paths.lock().expect("lock request paths");
        assert!(paths.iter().any(|path| path.ends_with("/responses")));
        assert!(!paths.iter().any(|path| path.ends_with("/chat/completions")));
    }

    #[test]
    fn github_copilot_continued_responses_request_omits_orphaned_tool_outputs() {
        let config = ProviderConfig {
            base_url: "https://api.githubcopilot.com".to_string(),
            model: "gpt-5.4".to_string(),
            api_key: String::new(),
            assistant_id: String::new(),
            auth_source: AuthSource::GithubCopilot,
            api_transport: ApiTransport::Responses,
            reasoning_effort: "high".to_string(),
            context_window_tokens: 0,
            response_schema: None,
            stop_sequences: None,
            temperature: None,
            top_p: None,
            top_k: None,
            metadata: None,
            service_tier: None,
            container: None,
            inference_geo: None,
            cache_control: None,
            max_tokens: None,
            anthropic_tool_choice: None,
            output_effort: None,
        };

        let body = build_openai_responses_body(
            amux_shared::providers::PROVIDER_ID_GITHUB_COPILOT,
            &config,
            "system prompt",
            &[
                ApiMessage {
                    role: "tool".to_string(),
                    content: ApiContent::Text("file list".to_string()),
                    tool_call_id: Some("call_orphaned".to_string()),
                    name: Some("list_files".to_string()),
                    tool_calls: None,
                },
                ApiMessage {
                    role: "user".to_string(),
                    content: ApiContent::Text("continue".to_string()),
                    tool_call_id: None,
                    name: None,
                    tool_calls: None,
                },
            ],
            &[],
            Some("resp_123"),
            false,
        );

        let input = body["input"].as_array().expect("input array");
        assert_eq!(body["previous_response_id"], "resp_123");
        assert_eq!(input.len(), 1);
        assert_eq!(input[0]["role"], "user");
    }

    #[test]
    fn github_copilot_full_responses_request_omits_orphaned_function_calls() {
        let config = ProviderConfig {
            base_url: "https://api.githubcopilot.com".to_string(),
            model: "gpt-5.4".to_string(),
            api_key: String::new(),
            assistant_id: String::new(),
            auth_source: AuthSource::GithubCopilot,
            api_transport: ApiTransport::Responses,
            reasoning_effort: "high".to_string(),
            context_window_tokens: 0,
            response_schema: None,
            stop_sequences: None,
            temperature: None,
            top_p: None,
            top_k: None,
            metadata: None,
            service_tier: None,
            container: None,
            inference_geo: None,
            cache_control: None,
            max_tokens: None,
            anthropic_tool_choice: None,
            output_effort: None,
        };

        let body = build_openai_responses_body(
            amux_shared::providers::PROVIDER_ID_GITHUB_COPILOT,
            &config,
            "system prompt",
            &[
                ApiMessage {
                    role: "user".to_string(),
                    content: ApiContent::Text("first question".to_string()),
                    tool_call_id: None,
                    name: None,
                    tool_calls: None,
                },
                ApiMessage {
                    role: "assistant".to_string(),
                    content: ApiContent::Text("I'll inspect that".to_string()),
                    tool_call_id: None,
                    name: None,
                    tool_calls: Some(vec![ApiToolCall {
                        id: "call_orphaned".to_string(),
                        call_type: "function".to_string(),
                        function: ApiToolCallFunction {
                            name: "read_file".to_string(),
                            arguments: "{\"path\":\"MEMORY.md\"}".to_string(),
                        },
                    }]),
                },
                ApiMessage {
                    role: "user".to_string(),
                    content: ApiContent::Text("continue".to_string()),
                    tool_call_id: None,
                    name: None,
                    tool_calls: None,
                },
            ],
            &[],
            None,
            false,
        );

        let input = body["input"].as_array().expect("input array");
        assert_eq!(input.len(), 3);
        assert_eq!(input[0]["role"], "user");
        assert_eq!(input[1]["role"], "assistant");
        assert_eq!(input[2]["role"], "user");
        assert!(input
            .iter()
            .all(|item| item.get("type").and_then(|value| value.as_str()) != Some("function_call")));
    }

    #[test]
    fn github_copilot_full_responses_request_preserves_function_call_history() {
        let config = ProviderConfig {
            base_url: "https://api.githubcopilot.com".to_string(),
            model: "gpt-5.4".to_string(),
            api_key: String::new(),
            assistant_id: String::new(),
            auth_source: AuthSource::GithubCopilot,
            api_transport: ApiTransport::Responses,
            reasoning_effort: "high".to_string(),
            context_window_tokens: 0,
            response_schema: None,
            stop_sequences: None,
            temperature: None,
            top_p: None,
            top_k: None,
            metadata: None,
            service_tier: None,
            container: None,
            inference_geo: None,
            cache_control: None,
            max_tokens: None,
            anthropic_tool_choice: None,
            output_effort: None,
        };

        let body = build_openai_responses_body(
            amux_shared::providers::PROVIDER_ID_GITHUB_COPILOT,
            &config,
            "system prompt",
            &[
                ApiMessage {
                    role: "user".to_string(),
                    content: ApiContent::Text("first question".to_string()),
                    tool_call_id: None,
                    name: None,
                    tool_calls: None,
                },
                ApiMessage {
                    role: "assistant".to_string(),
                    content: ApiContent::Text("I'll inspect that".to_string()),
                    tool_call_id: None,
                    name: None,
                    tool_calls: Some(vec![ApiToolCall {
                        id: "call_1".to_string(),
                        call_type: "function".to_string(),
                        function: ApiToolCallFunction {
                            name: "read_file".to_string(),
                            arguments: "{\"path\":\"MEMORY.md\"}".to_string(),
                        },
                    }]),
                },
                ApiMessage {
                    role: "tool".to_string(),
                    content: ApiContent::Text("file contents".to_string()),
                    tool_call_id: Some("call_1".to_string()),
                    name: Some("read_file".to_string()),
                    tool_calls: None,
                },
                ApiMessage {
                    role: "user".to_string(),
                    content: ApiContent::Text("continue".to_string()),
                    tool_call_id: None,
                    name: None,
                    tool_calls: None,
                },
            ],
            &[],
            None,
            false,
        );

        let input = body["input"].as_array().expect("input array");
        assert_eq!(input.len(), 5);
        assert_eq!(input[0]["role"], "user");
        assert_eq!(input[1]["role"], "assistant");
        assert_eq!(input[2]["type"], "function_call");
        assert_eq!(input[2]["call_id"], "call_1");
        assert_eq!(input[2]["name"], "read_file");
        assert_eq!(input[3]["type"], "function_call_output");
        assert_eq!(input[3]["call_id"], "call_1");
        assert_eq!(input[4]["role"], "user");
    }

    #[tokio::test]
    async fn copilot_validation_returns_static_catalog_models() {
        let _lock = crate::agent::provider_auth_store::provider_auth_test_env_lock();
        let _guard = EnvGuard::new(&[
            "TAMUX_PROVIDER_AUTH_DB_PATH",
            "TAMUX_GITHUB_COPILOT_DISABLE_GH_CLI",
            "TAMUX_GITHUB_COPILOT_DISABLE_ENV_AUTH",
            "COPILOT_GITHUB_TOKEN",
            "GH_TOKEN",
            "GITHUB_TOKEN",
        ]);
        let root = tempdir().unwrap();
        let db_path = root.path().join("provider-auth.db");
        std::env::set_var("TAMUX_PROVIDER_AUTH_DB_PATH", &db_path);
        std::env::set_var("TAMUX_GITHUB_COPILOT_DISABLE_GH_CLI", "1");
        std::env::set_var("TAMUX_GITHUB_COPILOT_DISABLE_ENV_AUTH", "1");
        let auth = serde_json::json!({
            "auth_mode": "github_copilot",
            "access_token": "ghu_browser_token",
            "source": "test",
            "updated_at": 1,
            "created_at": 1
        });
        provider_auth_store::save_provider_auth_state(
            amux_shared::providers::PROVIDER_ID_GITHUB_COPILOT,
            "github_copilot",
            &auth,
        )
            .unwrap();

        let models = validate_provider_connection(
            amux_shared::providers::PROVIDER_ID_GITHUB_COPILOT,
            "https://api.githubcopilot.com",
            "",
            AuthSource::GithubCopilot,
        )
        .await
        .expect("validation should succeed")
        .expect("copilot validation should return models");

        assert!(models.len() > 10);
        assert!(models.iter().any(|model| model.id == "gpt-4.1"));
        assert!(models.iter().any(|model| model.id == "gpt-5.4"));
        assert!(models.iter().any(|model| model.id == "claude-sonnet-4.6"));
        assert!(models.iter().any(|model| model.id == "raptor-mini"));
        assert!(models.iter().any(|model| model.id == "goldeneye"));
    }

    #[tokio::test]
    async fn unsupported_provider_model_fetch_returns_empty_catalog() {
        let models = fetch_models("featherless", "http://127.0.0.1:9", "")
            .await
            .expect("unsupported providers should not surface a fetch error");

        assert!(models.is_empty());
    }

    async fn responses_sse_test_response(body: &str) -> (reqwest::Response, tokio::task::JoinHandle<()>) {
        let client = reqwest::Client::new();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind test server");
        let addr = listener.local_addr().expect("local addr");
        let body = body.to_string();
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.expect("accept");
            let response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            let _ = tokio::io::AsyncWriteExt::write_all(&mut socket, response.as_bytes()).await;
        });
        let response = client
            .get(format!("http://{addr}"))
            .send()
            .await
            .expect("send test request");
        (response, server)
    }

    #[tokio::test]
    async fn copilot_responses_parser_collects_reasoning_summary_parts() {
        let body = concat!(
            "data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_123\"}}\n",
            "data: {\"type\":\"response.output_item.added\",\"output_index\":0,\"item\":{\"type\":\"reasoning\",\"id\":\"rs_1\",\"encrypted_content\":\"enc\"}}\n",
            "data: {\"type\":\"response.reasoning_summary_part.added\",\"item_id\":\"rs_1\",\"summary_index\":0}\n",
            "data: {\"type\":\"response.reasoning_summary_text.delta\",\"item_id\":\"rs_1\",\"summary_index\":0,\"delta\":\"thinking\"}\n",
            "data: {\"type\":\"response.completed\",\"response\":{\"usage\":{\"input_tokens\":1,\"output_tokens\":2}}}\n"
        );
        let (response, server) = responses_sse_test_response(body).await;

        let (tx, mut rx) = mpsc::channel(8);
        parse_openai_responses_sse(response, amux_shared::providers::PROVIDER_ID_GITHUB_COPILOT, &tx)
            .await
            .expect("parse should succeed");
        drop(tx);

        let mut reasoning_deltas = Vec::new();
        let mut final_reasoning = None;
        while let Some(chunk) = rx.recv().await {
            match chunk.expect("chunk") {
                CompletionChunk::Delta {
                    reasoning: Some(reasoning),
                    ..
                } => reasoning_deltas.push(reasoning),
                CompletionChunk::Done {
                    reasoning: Some(reasoning),
                    ..
                } => final_reasoning = Some(reasoning),
                _ => {}
            }
        }

        assert_eq!(reasoning_deltas, vec!["thinking".to_string()]);
        assert_eq!(final_reasoning.as_deref(), Some("thinking"));
        server.await.expect("server task");
    }

    #[tokio::test]
    async fn copilot_responses_parser_collects_reasoning_summary_from_output_item_done() {
        let body = concat!(
            "data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_456\"}}\n",
            "data: {\"type\":\"response.output_item.added\",\"output_index\":0,\"item\":{\"type\":\"reasoning\",\"id\":\"rs_2\",\"encrypted_content\":\"enc\"}}\n",
            "data: {\"type\":\"response.output_item.done\",\"output_index\":0,\"item\":{\"type\":\"reasoning\",\"id\":\"rs_2_rotated\",\"summary\":[{\"type\":\"summary_text\",\"text\":\"final reasoning\"}]}}\n",
            "data: {\"type\":\"response.completed\",\"response\":{\"usage\":{\"input_tokens\":3,\"output_tokens\":4}}}\n"
        );
        let (response, server) = responses_sse_test_response(body).await;

        let (tx, mut rx) = mpsc::channel(8);
        parse_openai_responses_sse(response, amux_shared::providers::PROVIDER_ID_GITHUB_COPILOT, &tx)
            .await
            .expect("parse should succeed");
        drop(tx);

        let mut reasoning_deltas = Vec::new();
        let mut final_reasoning = None;
        while let Some(chunk) = rx.recv().await {
            match chunk.expect("chunk") {
                CompletionChunk::Delta {
                    reasoning: Some(reasoning),
                    ..
                } => reasoning_deltas.push(reasoning),
                CompletionChunk::Done {
                    reasoning: Some(reasoning),
                    ..
                } => final_reasoning = Some(reasoning),
                _ => {}
            }
        }

        assert!(reasoning_deltas.is_empty());
        assert_eq!(final_reasoning.as_deref(), Some("final reasoning"));
        server.await.expect("server task");
    }

    #[tokio::test]
    async fn copilot_responses_parser_tolerates_unknown_events_before_completion() {
        let body = concat!(
            "data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_unknown\"}}\n",
            "data: {\"type\":\"response.custom_ignored\",\"value\":\"ignored\"}\n",
            "data: {\"type\":\"response.output_text.delta\",\"delta\":\"hello\"}\n",
            "data: {\"type\":\"response.completed\",\"response\":{\"usage\":{\"input_tokens\":5,\"output_tokens\":7}}}\n"
        );
        let (response, server) = responses_sse_test_response(body).await;

        let (tx, mut rx) = mpsc::channel(8);
        parse_openai_responses_sse(response, amux_shared::providers::PROVIDER_ID_GITHUB_COPILOT, &tx)
            .await
            .expect("parse should succeed");
        drop(tx);

        let mut deltas = Vec::new();
        let mut done_content = None;
        while let Some(chunk) = rx.recv().await {
            match chunk.expect("chunk") {
                CompletionChunk::Delta { content, .. } if !content.is_empty() => deltas.push(content),
                CompletionChunk::Done { content, .. } => done_content = Some(content),
                _ => {}
            }
        }

        assert_eq!(deltas, vec!["hello".to_string()]);
        assert_eq!(done_content.as_deref(), Some("hello"));
        server.await.expect("server task");
    }

    #[tokio::test]
    async fn copilot_responses_parser_rejects_chat_completions_payloads() {
        let body = concat!(
            "data: {\"id\":\"chatcmpl_123\",\"choices\":[{\"delta\":{\"content\":\"legacy\"}}]}\n",
            "data: [DONE]\n"
        );
        let (response, server) = responses_sse_test_response(body).await;

        let (tx, _rx) = mpsc::channel(8);
        let err = parse_openai_responses_sse(
            response,
            amux_shared::providers::PROVIDER_ID_GITHUB_COPILOT,
            &tx,
        )
        .await
        .expect_err("choices payload should be rejected");

        let failure = upstream_failure_error(&err).expect("structured upstream failure");
        assert_eq!(failure.class, UpstreamFailureClass::TransportIncompatible);
        server.await.expect("server task");
    }

    #[tokio::test]
    async fn copilot_responses_parser_surfaces_response_failed_as_structured_error() {
        let body = concat!(
            "data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_failed\"}}\n",
            "data: {\"type\":\"response.failed\",\"response\":{\"id\":\"resp_failed\",\"status\":\"failed\",\"usage\":{\"input_tokens\":2,\"output_tokens\":0,\"total_tokens\":2},\"error\":{\"code\":\"server_error\",\"message\":\"Over capacity\"}}}\n"
        );
        let (response, server) = responses_sse_test_response(body).await;

        let (tx, mut rx) = mpsc::channel(8);
        parse_openai_responses_sse(response, amux_shared::providers::PROVIDER_ID_GITHUB_COPILOT, &tx)
            .await
            .expect("parse should complete with an error chunk");
        drop(tx);

        let mut saw_error = None;
        while let Some(chunk) = rx.recv().await {
            if let CompletionChunk::Error { message } = chunk.expect("chunk") {
                saw_error = Some(message);
                break;
            }
        }

        let message = saw_error.expect("expected structured error chunk");
        let diagnostics = parse_structured_error(&message);
        assert_eq!(diagnostics.class, "temporary_upstream");
        assert_eq!(diagnostics.diagnostics["event_type"], "response.failed");
        assert_eq!(diagnostics.diagnostics["response_id"], "resp_failed");
        assert_eq!(diagnostics.diagnostics["upstream_error"]["code"], "server_error");
        assert_eq!(diagnostics.diagnostics["upstream_error"]["message"], "Over capacity");
        server.await.expect("server task");
    }

    #[tokio::test]
    async fn copilot_responses_parser_classifies_missing_tool_call_output_as_request_invalid() {
        let body = concat!(
            "data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_failed\"}}\n",
            "data: {\"type\":\"response.failed\",\"response\":{\"id\":\"resp_failed\",\"status\":\"failed\",\"usage\":{\"input_tokens\":2,\"output_tokens\":0,\"total_tokens\":2},\"error\":{\"code\":\"bad_request\",\"message\":\"No tool call found for function call output with call_id call_oTvo8eFwoaFskvWlhGcUp5rU.\"}}}\n"
        );
        let (response, server) = responses_sse_test_response(body).await;

        let (tx, mut rx) = mpsc::channel(8);
        parse_openai_responses_sse(response, amux_shared::providers::PROVIDER_ID_GITHUB_COPILOT, &tx)
            .await
            .expect("parse should complete with an error chunk");
        drop(tx);

        let mut saw_error = None;
        while let Some(chunk) = rx.recv().await {
            if let CompletionChunk::Error { message } = chunk.expect("chunk") {
                saw_error = Some(message);
                break;
            }
        }

        let message = saw_error.expect("expected structured error chunk");
        let diagnostics = parse_structured_error(&message);
        assert_eq!(diagnostics.class, "request_invalid");
        assert_eq!(diagnostics.diagnostics["event_type"], "response.failed");
        assert_eq!(diagnostics.diagnostics["response_id"], "resp_failed");
        assert_eq!(diagnostics.diagnostics["upstream_error"]["code"], "bad_request");
        assert_eq!(
            diagnostics.diagnostics["upstream_error"]["message"],
            "No tool call found for function call output with call_id call_oTvo8eFwoaFskvWlhGcUp5rU."
        );
        server.await.expect("server task");
    }

    #[tokio::test]
    async fn copilot_responses_parser_surfaces_top_level_error_as_structured_error() {
        let body = concat!(
            "data: {\"type\":\"error\",\"error\":{\"code\":\"rate_limit_exceeded\",\"message\":\"Slow down\",\"param\":\"model\"}}\n"
        );
        let (response, server) = responses_sse_test_response(body).await;

        let (tx, mut rx) = mpsc::channel(8);
        parse_openai_responses_sse(response, amux_shared::providers::PROVIDER_ID_GITHUB_COPILOT, &tx)
            .await
            .expect("parse should complete with an error chunk");
        drop(tx);

        let mut saw_error = None;
        while let Some(chunk) = rx.recv().await {
            if let CompletionChunk::Error { message } = chunk.expect("chunk") {
                saw_error = Some(message);
                break;
            }
        }

        let message = saw_error.expect("expected structured error chunk");
        let diagnostics = parse_structured_error(&message);
        assert_eq!(diagnostics.class, "rate_limit");
        assert_eq!(diagnostics.diagnostics["event_type"], "error");
        assert_eq!(diagnostics.diagnostics["upstream_error"]["code"], "rate_limit_exceeded");
        assert_eq!(diagnostics.diagnostics["upstream_error"]["message"], "Slow down");
        assert_eq!(diagnostics.diagnostics["upstream_error"]["param"], "model");
        server.await.expect("server task");
    }

    #[tokio::test]
    async fn copilot_responses_parser_errors_on_truncated_stream_without_terminal_event() {
        let body = concat!(
            "data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_truncated\"}}\n",
            "data: {\"type\":\"response.output_text.delta\",\"delta\":\"hello\"}\n"
        );
        let (response, server) = responses_sse_test_response(body).await;

        let (tx, _rx) = mpsc::channel(8);
        let err = parse_openai_responses_sse(
            response,
            amux_shared::providers::PROVIDER_ID_GITHUB_COPILOT,
            &tx,
        )
        .await
        .expect_err("unterminated responses stream should fail");

        assert!(
            err.to_string().contains("terminal") || err.to_string().contains("truncated"),
            "unexpected error: {err:#}"
        );
        server.await.expect("server task");
    }

    #[tokio::test]
    async fn copilot_responses_parser_errors_on_malformed_event_payload() {
        let body = concat!(
            "data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_bad_json\"}}\n",
            "data: {\"type\":\"response.output_text.delta\",\"delta\":\"unterminated\"\n"
        );
        let (response, server) = responses_sse_test_response(body).await;

        let (tx, _rx) = mpsc::channel(8);
        let err = parse_openai_responses_sse(
            response,
            amux_shared::providers::PROVIDER_ID_GITHUB_COPILOT,
            &tx,
        )
        .await
        .expect_err("malformed responses event should fail");

        assert!(
            err.to_string().contains("malformed") || err.to_string().contains("json"),
            "unexpected error: {err:#}"
        );
        server.await.expect("server task");
    }

    #[tokio::test]
    async fn chat_completions_parser_provider_final_result_uses_normalized_tool_calls() {
        let body = concat!(
            "data: {\"id\":\"chatcmpl_tool_final\",\"model\":\"gpt-5.4\",\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":1,\"function\":{\"name\":\"second\",\"arguments\":\"\"}}]}}]}\n",
            "data: {\"id\":\"chatcmpl_tool_final\",\"model\":\"gpt-5.4\",\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call_0\",\"function\":{\"name\":\"first\",\"arguments\":\"{\\\"a\\\":1}\"}},{\"index\":1,\"function\":{\"arguments\":\"{\\\"b\\\":2}\"}}]}}]}\n",
            "data: {\"id\":\"chatcmpl_tool_final\",\"model\":\"gpt-5.4\",\"choices\":[{\"delta\":{},\"finish_reason\":\"tool_calls\"}],\"usage\":{\"prompt_tokens\":5,\"completion_tokens\":3}}\n",
            "data: [DONE]\n"
        );
        let (response, server) = responses_sse_test_response(body).await;

        let (tx, mut rx) = mpsc::channel(8);
        parse_openai_sse(response, &tx)
            .await
            .expect("parse should succeed");
        drop(tx);

        let mut emitted_tool_calls = None;
        let mut final_result_tool_calls = None;
        while let Some(chunk) = rx.recv().await {
            if let CompletionChunk::ToolCalls {
                tool_calls,
                provider_final_result,
                ..
            } = chunk.expect("chunk")
            {
                final_result_tool_calls = Some(match provider_final_result
                    .expect("tool chunk should include provider final result")
                {
                    crate::agent::types::CompletionProviderFinalResult::OpenAiChatCompletions(
                        response,
                    ) => response.tool_calls,
                    other => panic!(
                        "expected OpenAI Chat Completions final result, got {other:?}"
                    ),
                });
                emitted_tool_calls = Some(tool_calls);
                break;
            }
        }

        let emitted_tool_calls = emitted_tool_calls.expect("expected tool-calls chunk");
        let final_result_tool_calls =
            final_result_tool_calls.expect("expected provider final result tool calls");

        assert_eq!(emitted_tool_calls.len(), 2);
        assert_eq!(final_result_tool_calls.len(), emitted_tool_calls.len());
        assert_eq!(emitted_tool_calls[0].id, "call_0");
        assert_eq!(emitted_tool_calls[0].function.name, "first");
        assert_eq!(emitted_tool_calls[1].id, "synthetic_tool_call_openai_1_second");
        assert_eq!(emitted_tool_calls[1].function.name, "second");
        assert_eq!(final_result_tool_calls[0].id, emitted_tool_calls[0].id);
        assert_eq!(
            final_result_tool_calls[0].function.name,
            emitted_tool_calls[0].function.name
        );
        assert_eq!(
            final_result_tool_calls[0].function.arguments,
            emitted_tool_calls[0].function.arguments
        );
        assert_eq!(final_result_tool_calls[1].id, emitted_tool_calls[1].id);
        assert_eq!(
            final_result_tool_calls[1].function.name,
            emitted_tool_calls[1].function.name
        );
        assert_eq!(
            final_result_tool_calls[1].function.arguments,
            emitted_tool_calls[1].function.arguments
        );

        server.await.expect("server task");
    }
