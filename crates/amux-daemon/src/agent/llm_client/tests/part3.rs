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
        };

        let body = build_openai_responses_body(
            "github-copilot",
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
        };

        let body = build_openai_responses_body(
            "github-copilot",
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
        provider_auth_store::save_provider_auth_state("github-copilot", "github_copilot", &auth)
            .unwrap();

        let models = validate_provider_connection(
            "github-copilot",
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

    #[tokio::test]
    async fn copilot_responses_parser_collects_reasoning_summary_parts() {
        let client = reqwest::Client::new();
        let body = concat!(
            "data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_123\"}}\n",
            "data: {\"type\":\"response.output_item.added\",\"output_index\":0,\"item\":{\"type\":\"reasoning\",\"id\":\"rs_1\",\"encrypted_content\":\"enc\"}}\n",
            "data: {\"type\":\"response.reasoning_summary_part.added\",\"item_id\":\"rs_1\",\"summary_index\":0}\n",
            "data: {\"type\":\"response.reasoning_summary_text.delta\",\"item_id\":\"rs_1\",\"summary_index\":0,\"delta\":\"thinking\"}\n",
            "data: {\"type\":\"response.completed\",\"response\":{\"usage\":{\"input_tokens\":1,\"output_tokens\":2}}}\n"
        );
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

        let (tx, mut rx) = mpsc::channel(8);
        parse_openai_responses_sse(response, "github-copilot", &tx)
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
        let client = reqwest::Client::new();
        let body = concat!(
            "data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_456\"}}\n",
            "data: {\"type\":\"response.output_item.added\",\"output_index\":0,\"item\":{\"type\":\"reasoning\",\"id\":\"rs_2\",\"encrypted_content\":\"enc\"}}\n",
            "data: {\"type\":\"response.output_item.done\",\"output_index\":0,\"item\":{\"type\":\"reasoning\",\"id\":\"rs_2_rotated\",\"summary\":[{\"type\":\"summary_text\",\"text\":\"final reasoning\"}]}}\n",
            "data: {\"type\":\"response.completed\",\"response\":{\"usage\":{\"input_tokens\":3,\"output_tokens\":4}}}\n"
        );
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

        let (tx, mut rx) = mpsc::channel(8);
        parse_openai_responses_sse(response, "github-copilot", &tx)
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
