    #[test]
    fn messages_to_api_format_normalizes_empty_tool_ids() {
        let messages = vec![
            AgentMessage {
                id: String::new(),
                role: MessageRole::Assistant,
                content: String::new(),
                tool_calls: Some(vec![ToolCall {
                    id: String::new(),
                    function: ToolFunction {
                        name: "list_sessions".to_string(),
                        arguments: "{}".to_string(),
                    },
                    weles_review: None,
                }]),
                tool_call_id: None,
                tool_name: None,
                tool_arguments: None,
                tool_status: None,
                weles_review: None,
                input_tokens: 0,
                output_tokens: 0,
                provider: None,
                model: None,
                api_transport: None,
                response_id: None,
                reasoning: None,
                timestamp: 42,
            },
            AgentMessage {
                id: String::new(),
                role: MessageRole::Tool,
                content: "ok".to_string(),
                tool_calls: None,
                tool_call_id: Some(String::new()),
                tool_name: Some("list_sessions".to_string()),
                tool_arguments: Some("{}".to_string()),
                tool_status: Some("done".to_string()),
                weles_review: None,
                input_tokens: 0,
                output_tokens: 0,
                provider: None,
                model: None,
                api_transport: None,
                response_id: None,
                reasoning: None,
                timestamp: 43,
            },
        ];

        let api_messages = messages_to_api_format(&messages);
        assert_eq!(api_messages.len(), 2);
        let assistant_tool_id = api_messages[0]
            .tool_calls
            .as_ref()
            .and_then(|calls| calls.first())
            .map(|call| call.id.clone())
            .expect("assistant tool call should have normalized id");
        assert!(!assistant_tool_id.is_empty());
        assert_eq!(
            api_messages[1].tool_call_id.as_deref(),
            Some(assistant_tool_id.as_str())
        );
    }

    #[test]
    fn chat_completion_messages_null_assistant_content_for_tool_calls() {
        let messages = vec![ApiMessage {
            role: "assistant".to_string(),
            content: ApiContent::Text("I'll inspect that now".to_string()),
            tool_call_id: None,
            name: None,
            tool_calls: Some(vec![ApiToolCall {
                id: "call_1".to_string(),
                call_type: "function".to_string(),
                function: ApiToolCallFunction {
                    name: "list_sessions".to_string(),
                    arguments: "{}".to_string(),
                },
            }]),
        }];

        let serialized =
            build_chat_completion_messages("system prompt", &messages).expect("serialize");
        assert_eq!(serialized.len(), 2);
        assert_eq!(serialized[1]["role"], "assistant");
        assert!(serialized[1]["content"].is_null());
        assert_eq!(serialized[1]["tool_calls"][0]["id"], "call_1");
    }

    #[test]
    fn github_models_chat_completion_url_uses_inference_prefix() {
        assert_eq!(
            build_chat_completion_url("https://models.github.ai"),
            "https://models.github.ai/inference/chat/completions"
        );
        assert_eq!(
            build_chat_completion_url("https://models.github.ai/inference"),
            "https://models.github.ai/inference/chat/completions"
        );
    }

    #[test]
    fn github_copilot_default_url_uses_copilot_api_origin() {
        let provider = get_provider_definition("github-copilot").expect("copilot provider");
        assert_eq!(provider.default_base_url, "https://api.githubcopilot.com");
        assert_eq!(
            build_chat_completion_url(provider.default_base_url),
            "https://api.githubcopilot.com/chat/completions"
        );
    }

    #[test]
    fn github_copilot_stored_auth_adds_bearer_header_to_requests() {
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

        let client = reqwest::Client::new();
        let config = ProviderConfig {
            base_url: "https://models.github.ai".to_string(),
            model: "openai/gpt-4.1".to_string(),
            api_key: String::new(),
            assistant_id: String::new(),
            auth_source: AuthSource::GithubCopilot,
            api_transport: ApiTransport::ChatCompletions,
            reasoning_effort: String::new(),
            context_window_tokens: 0,
            response_schema: None,
        };
        let request = apply_openai_auth_headers(
            client.get("https://models.github.ai/inference/chat/completions"),
            "github-copilot",
            &config,
        )
        .build()
        .expect("request should build");

        assert_eq!(
            request
                .headers()
                .get("authorization")
                .and_then(|value| value.to_str().ok()),
            Some("Bearer ghu_browser_token")
        );
    }

    #[test]
    fn github_copilot_requests_include_copilot_headers() {
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

        let client = reqwest::Client::new();
        let config = ProviderConfig {
            base_url: "https://api.githubcopilot.com".to_string(),
            model: "gpt-4.1".to_string(),
            api_key: String::new(),
            assistant_id: String::new(),
            auth_source: AuthSource::GithubCopilot,
            api_transport: ApiTransport::ChatCompletions,
            reasoning_effort: String::new(),
            context_window_tokens: 0,
            response_schema: None,
        };
        let request = apply_openai_auth_headers(
            client.get("https://api.githubcopilot.com/chat/completions"),
            "github-copilot",
            &config,
        )
        .build()
        .expect("request should build");

        assert_eq!(
            request
                .headers()
                .get("openai-intent")
                .and_then(|value| value.to_str().ok()),
            Some("conversation-edits")
        );
        assert_eq!(
            request
                .headers()
                .get("x-initiator")
                .and_then(|value| value.to_str().ok()),
            Some("user")
        );
        assert!(request.headers().get("user-agent").is_some());
    }

    #[test]
    fn github_copilot_responses_request_includes_reasoning_summary() {
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
            &[ApiMessage {
                role: "user".to_string(),
                content: ApiContent::Text("hello".to_string()),
                tool_call_id: None,
                name: None,
                tool_calls: None,
            }],
            &[],
            None,
            false,
        );

        assert_eq!(body["reasoning"]["effort"], "high");
        assert_eq!(body["reasoning"]["summary"], "auto");
    }

    #[tokio::test]
    async fn request_invalid_responses_400_malformed_body_is_classified_request_invalid() {
        let request_paths = Arc::new(Mutex::new(VecDeque::new()));
        let chat_requests = Arc::new(AtomicUsize::new(0));
        let base_url = spawn_responses_error_server(
            "400 Bad Request",
            r#"{"error":{"message":"Invalid 'input[12].name': empty string"}}"#.to_string(),
            None,
            request_paths,
            chat_requests,
        )
        .await;

        let stream = send_completion_request(
            &reqwest::Client::new(),
            "github-copilot",
            &responses_test_config(base_url, AuthSource::GithubCopilot),
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
        assert!(diagnostics
            .diagnostics
            .to_string()
            .contains("input[12].name"));
    }

    #[tokio::test]
    async fn temporary_upstream_responses_503_is_classified_temporary_upstream() {
        let request_paths = Arc::new(Mutex::new(VecDeque::new()));
        let chat_requests = Arc::new(AtomicUsize::new(0));
        let base_url = spawn_responses_error_server(
            "503 Service Unavailable",
            r#"{"error":{"message":"Service unavailable, try again later"}}"#.to_string(),
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
        assert_eq!(diagnostics.class, "temporary_upstream");
        assert!(diagnostics.diagnostics.to_string().contains("503"));
    }

    #[tokio::test]
    async fn rate_limit_responses_429_uses_retry_after_delay_from_body() {
        let request_paths = Arc::new(Mutex::new(VecDeque::new()));
        let chat_requests = Arc::new(AtomicUsize::new(0));
        let base_url = spawn_responses_error_server(
            "429 Too Many Requests",
            r#"{"message":"You are being rate limited.","retry_after":0.641,"global":false}"#
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
                max_retries: 1,
                retry_delay_ms: 5_000,
            },
        );

        let chunks = collect_chunks(stream).await;
        let retry = chunks
            .iter()
            .find_map(|chunk| match chunk {
                CompletionChunk::Retry { delay_ms, .. } => Some(*delay_ms),
                _ => None,
            })
            .expect("stream should emit retry chunk");
        assert_eq!(retry, 641);

        let message = match chunks.last().expect("terminal chunk") {
            CompletionChunk::Error { message } => message,
            other => panic!("expected error chunk, got {other:?}"),
        };
        let diagnostics = parse_structured_error(message);
        assert_eq!(diagnostics.class, "rate_limit");
        assert_eq!(diagnostics.diagnostics["retry_after_ms"], 641);
    }
