use amux_shared::providers::{
    PROVIDER_ID_GITHUB_COPILOT, PROVIDER_ID_OPENAI, PROVIDER_ID_OPENROUTER,
};

    #[test]
    fn messages_to_api_format_normalizes_empty_tool_ids() {
        let messages = vec![
            AgentMessage {
                id: String::new(),
                role: MessageRole::Assistant,
                content: String::new(),
                content_blocks: Vec::new(),
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
                cost: None,
                provider: None,
                model: None,
                api_transport: None,
                response_id: None,
                upstream_message: None,
                provider_final_result: None,
                author_agent_id: None,
                author_agent_name: None,
                reasoning: None,
                message_kind: crate::agent::types::AgentMessageKind::Normal,
                compaction_strategy: None,
                compaction_payload: None,
                offloaded_payload_id: None,
                tool_output_preview_path: None,
                structural_refs: Vec::new(),
                pinned_for_compaction: false,
                timestamp: 42,
            },
            AgentMessage {
                id: String::new(),
                role: MessageRole::Tool,
                content: "ok".to_string(),
                content_blocks: Vec::new(),
                tool_calls: None,
                tool_call_id: Some(String::new()),
                tool_name: Some("list_sessions".to_string()),
                tool_arguments: Some("{}".to_string()),
                tool_status: Some("done".to_string()),
                weles_review: None,
                input_tokens: 0,
                output_tokens: 0,
                cost: None,
                provider: None,
                model: None,
                api_transport: None,
                response_id: None,
                upstream_message: None,
                provider_final_result: None,
                author_agent_id: None,
                author_agent_name: None,
                reasoning: None,
                message_kind: crate::agent::types::AgentMessageKind::Normal,
                compaction_strategy: None,
                compaction_payload: None,
                offloaded_payload_id: None,
                tool_output_preview_path: None,
                structural_refs: Vec::new(),
                pinned_for_compaction: false,
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
        let provider =
            get_provider_definition(PROVIDER_ID_GITHUB_COPILOT).expect("copilot provider");
        assert_eq!(provider.default_base_url, "https://api.githubcopilot.com");
        assert_eq!(
            build_chat_completion_url(provider.default_base_url),
            "https://api.githubcopilot.com/chat/completions"
        );
    }

    #[test]
    fn deepseek_default_url_uses_documented_chat_completion_endpoint() {
        let provider =
            get_provider_definition(amux_shared::providers::PROVIDER_ID_DEEPSEEK)
                .expect("deepseek provider");
        assert_eq!(provider.default_base_url, "https://api.deepseek.com");
        assert_eq!(
            build_chat_completion_url(provider.default_base_url),
            "https://api.deepseek.com/chat/completions"
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
        provider_auth_store::save_provider_auth_state(
            PROVIDER_ID_GITHUB_COPILOT,
            "github_copilot",
            &auth,
        )
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
        let request = apply_openai_auth_headers(
            client.get("https://models.github.ai/inference/chat/completions"),
            PROVIDER_ID_GITHUB_COPILOT,
            &config,
            CopilotInitiator::User,
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
        provider_auth_store::save_provider_auth_state(
            PROVIDER_ID_GITHUB_COPILOT,
            "github_copilot",
            &auth,
        )
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
        let request = apply_openai_auth_headers(
            client.get("https://api.githubcopilot.com/chat/completions"),
            PROVIDER_ID_GITHUB_COPILOT,
            &config,
            CopilotInitiator::User,
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
    fn github_copilot_internal_requests_use_agent_initiator() {
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
            PROVIDER_ID_GITHUB_COPILOT,
            "github_copilot",
            &auth,
        )
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
        let request = apply_openai_auth_headers(
            client.get("https://api.githubcopilot.com/chat/completions"),
            PROVIDER_ID_GITHUB_COPILOT,
            &config,
            CopilotInitiator::Agent,
        )
        .build()
        .expect("request should build");

        assert_eq!(
            request
                .headers()
                .get("x-initiator")
                .and_then(|value| value.to_str().ok()),
            Some("agent")
        );
    }

    #[test]
    fn openrouter_requests_include_app_attribution_headers() {
        let client = reqwest::Client::new();
        let config = ProviderConfig {
            base_url: "https://openrouter.ai/api/v1".to_string(),
            model: "arcee-ai/trinity-large-thinking".to_string(),
            api_key: "openrouter-key".to_string(),
            assistant_id: String::new(),
            auth_source: AuthSource::ApiKey,
            api_transport: ApiTransport::ChatCompletions,
            reasoning_effort: String::new(),
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

        let request = apply_openai_auth_headers(
            client.get("https://openrouter.ai/api/v1/chat/completions"),
            PROVIDER_ID_OPENROUTER,
            &config,
            CopilotInitiator::Agent,
        )
        .build()
        .expect("request should build");

        assert_eq!(
            request
                .headers()
                .get("http-referer")
                .and_then(|value| value.to_str().ok()),
            Some("https://tamux.app")
        );
        assert_eq!(
            request
                .headers()
                .get("x-openrouter-title")
                .and_then(|value| value.to_str().ok()),
            Some("tamux")
        );
        assert_eq!(
            request
                .headers()
                .get("x-openrouter-categories")
                .and_then(|value| value.to_str().ok()),
            Some("cli-agent")
        );
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
            PROVIDER_ID_GITHUB_COPILOT,
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

    #[test]
    fn responses_request_sets_tool_choice_when_tools_are_present() {
        let config = ProviderConfig {
            base_url: "https://api.openai.com".to_string(),
            model: "gpt-5.4".to_string(),
            api_key: String::new(),
            assistant_id: String::new(),
            auth_source: AuthSource::ApiKey,
            api_transport: ApiTransport::Responses,
            reasoning_effort: "off".to_string(),
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
            PROVIDER_ID_OPENAI,
            &config,
            "system prompt",
            &[ApiMessage {
                role: "user".to_string(),
                content: ApiContent::Text("hello".to_string()),
                tool_call_id: None,
                name: None,
                tool_calls: None,
            }],
            &[ToolDefinition {
                tool_type: "function".to_string(),
                function: ToolFunctionDef {
                    name: "update_memory".to_string(),
                    description: "Store durable memory".to_string(),
                    parameters: serde_json::json!({
                        "type": "object",
                        "properties": {
                            "content": { "type": "string" }
                        },
                        "required": ["content"]
                    }),
                },
            }],
            None,
            false,
        );

        assert_eq!(body["tool_choice"], "auto");
    }

    #[test]
    fn build_openai_responses_request_omits_previous_response_id_for_chatgpt_subscription() {
        let config = ProviderConfig {
            base_url: "https://api.openai.com/v1".to_string(),
            model: "gpt-5.4".to_string(),
            api_key: String::new(),
            assistant_id: String::new(),
            auth_source: AuthSource::ChatgptSubscription,
            api_transport: ApiTransport::Responses,
            reasoning_effort: "high".to_string(),
            context_window_tokens: 0,
            response_schema: Some(serde_json::json!({
                "type": "object",
                "properties": {
                    "answer": { "type": "string" }
                },
                "required": ["answer"]
            })),
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

        let request = build_openai_responses_request(
            PROVIDER_ID_OPENAI,
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
            ],
            &[ToolDefinition {
                tool_type: "function".to_string(),
                function: ToolFunctionDef {
                    name: "update_memory".to_string(),
                    description: "Store durable memory".to_string(),
                    parameters: serde_json::json!({
                        "type": "object",
                        "properties": {
                            "content": { "type": "string" }
                        },
                        "required": ["content"]
                    }),
                },
            }],
            Some("resp_123"),
            true,
        );

        let body = serde_json::to_value(&request).expect("serialize request");

        assert_eq!(body["model"], "gpt-5.4");
        assert_eq!(body["instructions"], "system prompt");
        assert!(body.get("previous_response_id").is_none());
        assert_eq!(body["stream"], true);
        assert_eq!(body["input"][0]["role"], "user");
        assert_eq!(body["input"].as_array().map(Vec::len), Some(4));
        assert_eq!(body["tools"][0]["type"], "function");
        assert_eq!(body["tools"][0]["name"], "update_memory");
        assert_eq!(body["tool_choice"], "auto");
        assert_eq!(body["text"]["format"]["type"], "json_schema");
        assert_eq!(body["text"]["verbosity"], "high");
        assert_eq!(body["reasoning"]["effort"], "high");
        assert!(body["reasoning"].get("summary").is_none());
        assert_eq!(body["store"], false);
        assert_eq!(
            body["include"],
            serde_json::json!(["reasoning.encrypted_content"])
        );
    }

    #[test]
    fn anthropic_request_maps_response_schema_to_output_config_json_schema() {
        let client = reqwest::Client::new();
        let config = ProviderConfig {
            base_url: "https://api.anthropic.com".to_string(),
            model: "claude-sonnet-4-6".to_string(),
            api_key: String::new(),
            assistant_id: String::new(),
            auth_source: AuthSource::ApiKey,
            api_transport: ApiTransport::ChatCompletions,
            reasoning_effort: "off".to_string(),
            context_window_tokens: 0,
            response_schema: Some(serde_json::json!({
                "type": "object",
                "properties": {
                    "answer": { "type": "string" }
                },
                "required": ["answer"]
            })),
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

        let request = build_anthropic_request(
            &client,
            "anthropic",
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
            false,
        )
        .expect("request should build");

        let body: serde_json::Value = serde_json::from_slice(
            request.body().and_then(|body| body.as_bytes()).expect("body bytes"),
        )
        .expect("json body");

        assert_eq!(body["output_config"]["format"]["type"], "json_schema");
        assert_eq!(body["output_config"]["format"]["schema"], config.response_schema.unwrap());
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
            PROVIDER_ID_GITHUB_COPILOT,
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

    #[tokio::test]
    async fn rate_limit_responses_429_uses_openrouter_raw_message_without_truncation() {
        let request_paths = Arc::new(Mutex::new(VecDeque::new()));
        let chat_requests = Arc::new(AtomicUsize::new(0));
        let raw_detail = format!(
            "{}qwen/qwen3.6-plus:free is temporarily rate-limited upstream. Please retry shortly, or add your own key to accumulate your rate limits.",
            "prefix ".repeat(40)
        );
        let body = serde_json::json!({
            "error": {
                "message": "Provider returned error",
                "code": 429,
                "metadata": {
                    "raw": raw_detail
                }
            }
        })
        .to_string();
        let base_url = spawn_responses_error_server(
            "429 Too Many Requests",
            body,
            None,
            request_paths,
            chat_requests,
        )
        .await;

        let stream = send_completion_request(
            &reqwest::Client::new(),
            PROVIDER_ID_OPENROUTER,
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
        let retry_message = chunks
            .iter()
            .find_map(|chunk| match chunk {
                CompletionChunk::Retry { message, .. } => Some(message.as_str()),
                _ => None,
            })
            .expect("stream should emit retry chunk");
        assert!(
            retry_message.contains("add your own key to accumulate your rate limits"),
            "retry chunk should preserve the detailed upstream raw message: {retry_message}"
        );

        let terminal_message = match chunks.last().expect("terminal chunk") {
            CompletionChunk::Error { message } => message,
            other => panic!("expected error chunk, got {other:?}"),
        };
        let diagnostics = parse_structured_error(terminal_message);
        assert_eq!(diagnostics.class, "rate_limit");
        assert!(
            diagnostics.summary.contains("add your own key to accumulate your rate limits"),
            "terminal summary should preserve the detailed upstream raw message: {}",
            diagnostics.summary
        );
        assert!(
            diagnostics.diagnostics["raw_message"]
                .as_str()
                .is_some_and(|value| value.contains("add your own key to accumulate your rate limits")),
            "structured diagnostics should preserve the detailed upstream raw message"
        );
    }
