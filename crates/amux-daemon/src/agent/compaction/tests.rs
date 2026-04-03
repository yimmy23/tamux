use super::*;
use crate::agent::AgentEngine;
use crate::session_manager::SessionManager;
use tempfile::tempdir;

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
        agent_name: None,
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
fn heuristic_compaction_summary_uses_checkpoint_schema() {
    let summary = build_compaction_summary(
        &[
            AgentMessage::user(
                "Session: amux-landing website updates. Working directory: /tmp/demo. Completed: Added Agents nav link.",
                1,
            ),
            AgentMessage {
                id: "assistant-1".to_string(),
                role: MessageRole::Assistant,
                content: "Verified HTML and CSS. Status: checking responsive layout next."
                    .to_string(),
                tool_calls: None,
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
                message_kind: AgentMessageKind::Normal,
                compaction_strategy: None,
                compaction_payload: None,
                timestamp: 2,
            },
            AgentMessage {
                id: "tool-1".to_string(),
                role: MessageRole::Tool,
                content: "styles.css read complete".to_string(),
                tool_calls: None,
                tool_call_id: Some("call_1".to_string()),
                tool_name: Some("read_file".to_string()),
                tool_arguments: Some("{\"path\":\"styles.css\"}".to_string()),
                tool_status: Some("done".to_string()),
                weles_review: None,
                input_tokens: 0,
                output_tokens: 0,
                provider: None,
                model: None,
                api_transport: None,
                response_id: None,
                reasoning: None,
                message_kind: AgentMessageKind::Normal,
                compaction_strategy: None,
                compaction_payload: None,
                timestamp: 3,
            },
        ],
        2048,
    );

    assert!(summary.starts_with("# 🤖 Agent Context: State Checkpoint"));
    assert!(summary.contains("## 🎯 Primary Objective"));
    assert!(summary.contains("## 🗺️ Execution Map"));
    assert!(summary.contains("## 📁 Working Environment State"));
    assert!(summary.contains("## 🧠 Acquired Knowledge & Constraints"));
    assert!(summary.contains("## 🚫 Dead Ends & Resolved Errors"));
    assert!(summary.contains("## 🛠️ Recent Action Summary (Last 3-5 Turns)"));
    assert!(summary.contains("## 🎯 Immediate Next Step"));
    assert!(summary.contains("/tmp/demo"));
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
            message_kind: AgentMessageKind::Normal,
            compaction_strategy: None,
            compaction_payload: None,
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
            message_kind: AgentMessageKind::Normal,
            compaction_strategy: None,
            compaction_payload: None,
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
            message_kind: AgentMessageKind::Normal,
            compaction_strategy: None,
            compaction_payload: None,
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

#[test]
fn default_agent_config_exposes_heuristic_compaction_strategy_defaults() {
    let config = AgentConfig::default();

    assert_eq!(config.compaction.strategy, CompactionStrategy::Heuristic);
    assert_eq!(config.compaction.weles.reasoning_effort, "medium");
    assert_eq!(config.compaction.custom_model.reasoning_effort, "high");
}

#[test]
fn agent_config_roundtrip_preserves_nested_compaction_provider_settings() {
    let config: AgentConfig = serde_json::from_value(serde_json::json!({
        "provider": "openai",
        "model": "gpt-5.4",
        "compaction": {
            "strategy": "custom_model",
            "weles": {
                "provider": "openai",
                "model": "gpt-5.4-mini",
                "reasoning_effort": "low"
            },
            "custom_model": {
                "provider": "openrouter",
                "base_url": "https://openrouter.ai/api/v1",
                "model": "anthropic/claude-sonnet-4",
                "api_key": "router-key",
                "assistant_id": "",
                "auth_source": "api_key",
                "api_transport": "chat_completions",
                "reasoning_effort": "medium",
                "context_window_tokens": 256000
            }
        }
    }))
    .expect("config should deserialize");

    assert_eq!(config.compaction.strategy, CompactionStrategy::CustomModel);
    assert_eq!(config.compaction.weles.provider, "openai");
    assert_eq!(config.compaction.weles.model, "gpt-5.4-mini");
    assert_eq!(config.compaction.weles.reasoning_effort, "low");
    assert_eq!(config.compaction.custom_model.provider, "openrouter");
    assert_eq!(
        config.compaction.custom_model.base_url,
        "https://openrouter.ai/api/v1"
    );
    assert_eq!(
        config.compaction.custom_model.api_transport,
        ApiTransport::ChatCompletions
    );
    assert_eq!(config.compaction.custom_model.reasoning_effort, "medium");
    assert_eq!(
        config.compaction.custom_model.context_window_tokens,
        256_000
    );

    let serialized = serde_json::to_value(&config).expect("config should serialize");
    assert_eq!(serialized["compaction"]["strategy"], "custom_model");
    assert_eq!(serialized["compaction"]["weles"]["model"], "gpt-5.4-mini");
    assert_eq!(
        serialized["compaction"]["custom_model"]["provider"],
        "openrouter"
    );
    assert_eq!(
        serialized["compaction"]["custom_model"]["reasoning_effort"],
        "medium"
    );
}

#[test]
fn compaction_artifact_message_roundtrip_preserves_runtime_metadata() {
    let message = AgentMessage {
        id: "compaction-1".to_string(),
        role: MessageRole::Assistant,
        content: "rule based".to_string(),
        tool_calls: None,
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
        timestamp: 99,
        message_kind: AgentMessageKind::CompactionArtifact,
        compaction_strategy: Some(CompactionStrategy::Heuristic),
        compaction_payload: Some("Older context compacted for continuity".to_string()),
    };

    let encoded = serde_json::to_value(&message).expect("message should serialize");
    assert_eq!(encoded["message_kind"], "compaction_artifact");
    assert_eq!(encoded["compaction_strategy"], "heuristic");
    assert_eq!(
        encoded["compaction_payload"],
        "Older context compacted for continuity"
    );

    let decoded: AgentMessage =
        serde_json::from_value(encoded).expect("message should deserialize");
    assert_eq!(decoded.message_kind, AgentMessageKind::CompactionArtifact);
    assert_eq!(
        decoded.compaction_strategy,
        Some(CompactionStrategy::Heuristic)
    );
    assert_eq!(
        decoded.compaction_payload.as_deref(),
        Some("Older context compacted for continuity")
    );
    assert_eq!(decoded.content, "rule based");
}

#[test]
fn compaction_candidate_ignores_messages_before_latest_artifact() {
    let mut config = AgentConfig::default();
    config.max_context_messages = 2;
    config.keep_recent_on_compact = 1;
    let provider = sample_provider_config();
    let messages = vec![
        sample_message("older one"),
        sample_message("older two"),
        AgentMessage {
            id: "compaction-1".to_string(),
            role: MessageRole::Assistant,
            content: "rule based".to_string(),
            tool_calls: None,
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
            message_kind: AgentMessageKind::CompactionArtifact,
            compaction_strategy: Some(CompactionStrategy::Heuristic),
            compaction_payload: Some("Older context compacted for continuity".to_string()),
            timestamp: 3,
        },
        sample_message("recent one"),
        sample_message("recent two"),
    ];

    let candidate =
        compaction_candidate(&messages, &config, &provider).expect("candidate should exist");

    assert_eq!(candidate.split_at, 2);
}

#[tokio::test]
async fn heuristic_compaction_artifact_persists_and_request_uses_hidden_payload() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.auto_compact_context = true;
    config.max_context_messages = 2;
    config.keep_recent_on_compact = 1;
    config.compaction.strategy = CompactionStrategy::Heuristic;
    let engine = AgentEngine::new_test(manager, config.clone(), root.path()).await;
    let provider = sample_provider_config();
    let thread_id = "compaction-thread";

    {
        let mut threads = engine.threads.write().await;
        threads.insert(
            thread_id.to_string(),
            sample_thread(vec![
                sample_message("older one"),
                sample_message("older two"),
                sample_message("recent one"),
            ]),
        );
        let thread = threads.get_mut(thread_id).expect("thread should exist");
        thread.id = thread_id.to_string();
    }

    let inserted = engine
        .maybe_persist_compaction_artifact(thread_id, None, &config, &provider)
        .await
        .expect("compaction should succeed");
    assert!(inserted, "compaction artifact should be inserted");

    let thread = {
        let threads = engine.threads.read().await;
        threads
            .get(thread_id)
            .cloned()
            .expect("thread should exist after compaction")
    };
    assert_eq!(thread.messages.len(), 4);
    let artifact = &thread.messages[2];
    assert_eq!(artifact.message_kind, AgentMessageKind::CompactionArtifact);
    assert_eq!(artifact.content, "rule based");
    assert_eq!(
        artifact.compaction_strategy,
        Some(CompactionStrategy::Heuristic)
    );
    assert!(artifact
        .compaction_payload
        .as_deref()
        .is_some_and(|payload| payload.starts_with("# 🤖 Agent Context: State Checkpoint")));

    let compacted = compact_messages_for_request(&thread.messages, &config, &provider);
    assert_eq!(compacted.len(), 2);
    assert_eq!(
        compacted[0].content,
        artifact
            .compaction_payload
            .clone()
            .expect("artifact should carry hidden payload")
    );
    assert_eq!(compacted[1].content, "recent one");

    let manager = SessionManager::new_test(root.path()).await;
    let rehydrated = AgentEngine::new_test(manager, config, root.path()).await;
    rehydrated.hydrate().await.expect("hydrate should succeed");
    let rehydrated_thread = {
        let threads = rehydrated.threads.read().await;
        threads
            .get(thread_id)
            .cloned()
            .expect("rehydrated thread should exist")
    };
    let restored_artifact = &rehydrated_thread.messages[2];
    assert_eq!(
        restored_artifact.message_kind,
        AgentMessageKind::CompactionArtifact
    );
    assert_eq!(restored_artifact.content, "rule based");
    assert_eq!(
        restored_artifact.compaction_strategy,
        Some(CompactionStrategy::Heuristic)
    );
    assert_eq!(
        restored_artifact.compaction_payload,
        artifact.compaction_payload
    );
}
