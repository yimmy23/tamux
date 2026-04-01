use super::*;

fn sample_provider_config() -> ProviderConfig {
    ProviderConfig {
        base_url: "https://example.invalid".to_string(),
        model: "test-model".to_string(),
        api_key: String::new(),
        assistant_id: String::new(),
        auth_source: AuthSource::ApiKey,
        api_transport: ApiTransport::ChatCompletions,
        reasoning_effort: "low".to_string(),
        context_window_tokens: 128_000,
        response_schema: None,
    }
}

fn sample_message(content: &str) -> AgentMessage {
    AgentMessage::user(content, 1)
}

fn sample_thread(messages: Vec<AgentMessage>) -> AgentThread {
    AgentThread {
        id: "thread-1".to_string(),
        title: "Test".to_string(),
        messages,
        pinned: false,
        upstream_thread_id: None,
        upstream_transport: None,
        upstream_provider: None,
        upstream_model: None,
        upstream_assistant_id: None,
        created_at: 1,
        updated_at: 1,
        total_input_tokens: 0,
        total_output_tokens: 0,
    }
}

#[test]
fn compaction_candidate_is_none_when_request_is_within_budget() {
    let config = AgentConfig::default();
    let provider = sample_provider_config();
    let messages = vec![sample_message("short message")];

    assert_eq!(compaction_candidate(&messages, &config, &provider), None);
}

#[test]
fn compaction_candidate_exposes_the_older_slice_boundary() {
    let mut config = AgentConfig::default();
    config.max_context_messages = 3;
    config.keep_recent_on_compact = 2;
    let provider = sample_provider_config();
    let messages = vec![
        sample_message("one"),
        sample_message("two"),
        sample_message("three"),
        sample_message("four"),
    ];

    let candidate =
        compaction_candidate(&messages, &config, &provider).expect("candidate should exist");

    assert_eq!(candidate.split_at, 2);
    assert!(candidate.target_tokens >= MIN_CONTEXT_TARGET_TOKENS);
}

#[test]
fn github_copilot_tool_follow_up_disables_previous_response_continuity() {
    let mut config = AgentConfig::default();
    config.provider = "github-copilot".to_string();

    let provider = ProviderConfig {
        base_url: "https://api.githubcopilot.com".to_string(),
        model: "gpt-5.4".to_string(),
        api_key: String::new(),
        assistant_id: String::new(),
        auth_source: AuthSource::GithubCopilot,
        api_transport: ApiTransport::Responses,
        reasoning_effort: "high".to_string(),
        context_window_tokens: 128_000,
        response_schema: None,
    };

    let thread = sample_thread(vec![
        AgentMessage::user("first question", 1),
        AgentMessage {
            id: "assistant-1".to_string(),
            role: MessageRole::Assistant,
            content: "I'll inspect that".to_string(),
            tool_calls: Some(vec![ToolCall {
                id: "call_1".to_string(),
                function: ToolFunction {
                    name: "list_files".to_string(),
                    arguments: "{}".to_string(),
                },
                weles_review: None,
            }]),
            tool_call_id: None,
            tool_name: None,
            tool_arguments: None,
            tool_status: None,
            weles_review: None,
            input_tokens: 11,
            output_tokens: 7,
            provider: Some("github-copilot".to_string()),
            model: Some("gpt-5.4".to_string()),
            api_transport: Some(ApiTransport::Responses),
            response_id: Some("resp_123".to_string()),
            reasoning: Some("reasoned".to_string()),
            timestamp: 2,
        },
        AgentMessage {
            id: "tool-1".to_string(),
            role: MessageRole::Tool,
            content: "file list".to_string(),
            tool_calls: None,
            tool_call_id: Some("call_1".to_string()),
            tool_name: Some("list_files".to_string()),
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
            timestamp: 3,
        },
        AgentMessage::user("continue", 4),
    ]);

    let prepared = prepare_llm_request(&thread, &config, &provider);

    assert_eq!(prepared.transport, ApiTransport::Responses);
    assert_eq!(prepared.previous_response_id, None);
    assert_eq!(prepared.messages.len(), 4);
    assert_eq!(prepared.messages[0].role, "user");
    assert_eq!(prepared.messages[1].role, "assistant");
    assert_eq!(prepared.messages[2].role, "tool");
    assert_eq!(prepared.messages[3].role, "user");
}

#[test]
fn github_copilot_responses_request_uses_previous_response_id_for_plain_follow_up_turns() {
    let mut config = AgentConfig::default();
    config.provider = "github-copilot".to_string();

    let provider = ProviderConfig {
        base_url: "https://api.githubcopilot.com".to_string(),
        model: "gpt-5.4".to_string(),
        api_key: String::new(),
        assistant_id: String::new(),
        auth_source: AuthSource::GithubCopilot,
        api_transport: ApiTransport::Responses,
        reasoning_effort: "high".to_string(),
        context_window_tokens: 128_000,
        response_schema: None,
    };

    let thread = sample_thread(vec![
        AgentMessage::user("first question", 1),
        AgentMessage {
            id: "assistant-1".to_string(),
            role: MessageRole::Assistant,
            content: "Initial answer".to_string(),
            tool_calls: None,
            tool_call_id: None,
            tool_name: None,
            tool_arguments: None,
            tool_status: None,
            weles_review: None,
            input_tokens: 11,
            output_tokens: 7,
            provider: Some("github-copilot".to_string()),
            model: Some("gpt-5.4".to_string()),
            api_transport: Some(ApiTransport::Responses),
            response_id: Some("resp_123".to_string()),
            reasoning: Some("reasoned".to_string()),
            timestamp: 2,
        },
        AgentMessage::user("continue", 3),
    ]);

    let prepared = prepare_llm_request(&thread, &config, &provider);

    assert_eq!(prepared.transport, ApiTransport::Responses);
    assert_eq!(prepared.previous_response_id.as_deref(), Some("resp_123"));
    assert_eq!(prepared.messages.len(), 1);
    assert_eq!(prepared.messages[0].role, "user");
}
