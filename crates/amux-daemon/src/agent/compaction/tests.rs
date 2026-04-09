use super::*;
use crate::agent::AgentEngine;
use crate::session_manager::SessionManager;
use amux_shared::providers::{
    PROVIDER_ID_ALIBABA_CODING_PLAN, PROVIDER_ID_GITHUB_COPILOT, PROVIDER_ID_OPENAI,
    PROVIDER_ID_OPENROUTER,
};
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
fn compaction_request_input_budget_uses_model_catalog_window() {
    let mut provider = sample_provider_config();
    provider.model = "glm-5".to_string();
    provider.context_window_tokens = 512_000;

    let budget = llm_compaction_input_budget(PROVIDER_ID_ALIBABA_CODING_PLAN, &provider);

    assert!(budget < 512_000);
    assert!(budget <= 202_752);
}

#[test]
fn build_llm_compaction_messages_trims_to_fit_model_budget() {
    let messages = (0..40)
        .map(|idx| AgentMessage::user(format!("message-{idx} {}", "x".repeat(1_200)), idx + 1))
        .collect::<Vec<_>>();

    let api_messages = build_llm_compaction_messages(&messages, 50_000, 2_048);

    assert!(
        estimate_api_messages_tokens(&api_messages) <= 2_048,
        "api messages exceeded budget"
    );
    let Some(ApiMessage {
        content: ApiContent::Text(instruction),
        ..
    }) = api_messages.last()
    else {
        panic!("expected final instruction message");
    };
    assert!(instruction.contains("Follow the mandatory thread-compaction protocol"));
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
                upstream_message: None,
                provider_final_result: None,
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
                upstream_message: None,
                provider_final_result: None,
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
fn llm_compaction_fallback_keeps_recent_question_context_for_short_user_replies() {
    let mut messages = Vec::new();
    for index in 0..25 {
        messages.push(AgentMessage::user(
            format!("Earlier context message {index}"),
            index as u64 + 1,
        ));
    }

    messages.push(AgentMessage {
        id: "assistant-question".to_string(),
        role: MessageRole::Assistant,
        content: "Pick the implementation approach: 1. Preserve recent dialogue context 2. Keep current summary format"
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
        upstream_message: None,
        provider_final_result: None,
        reasoning: None,
        message_kind: AgentMessageKind::Normal,
        compaction_strategy: None,
        compaction_payload: None,
        timestamp: 30,
    });
    messages.push(AgentMessage::user("1", 31));
    messages.push(AgentMessage {
        id: "assistant-tool-call".to_string(),
        role: MessageRole::Assistant,
        content: String::new(),
        tool_calls: Some(vec![ToolCall {
            id: "call_read".to_string(),
            function: ToolFunction {
                name: "read_file".to_string(),
                arguments: "{\"path\":\"/tmp/spec.md\"}".to_string(),
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
        upstream_message: None,
        provider_final_result: None,
        reasoning: None,
        message_kind: AgentMessageKind::Normal,
        compaction_strategy: None,
        compaction_payload: None,
        timestamp: 32,
    });
    messages.push(AgentMessage {
        id: "tool-read".to_string(),
        role: MessageRole::Tool,
        content: "Full file contents that should not be passed verbatim to the compactor"
            .to_string(),
        tool_calls: None,
        tool_call_id: Some("call_read".to_string()),
        tool_name: Some("read_file".to_string()),
        tool_arguments: Some("{\"path\":\"/tmp/spec.md\"}".to_string()),
        tool_status: Some("done".to_string()),
        weles_review: None,
        input_tokens: 0,
        output_tokens: 0,
        provider: None,
        model: None,
        api_transport: None,
        response_id: None,
        upstream_message: None,
        provider_final_result: None,
        reasoning: None,
        message_kind: AgentMessageKind::Normal,
        compaction_strategy: None,
        compaction_payload: None,
        timestamp: 33,
    });

    let api_messages = build_llm_compaction_messages(&messages, 512, 8_192);
    let flattened = api_messages
        .iter()
        .filter_map(|message| match &message.content {
            super::super::llm_client::ApiContent::Text(text) => Some(text.as_str()),
            super::super::llm_client::ApiContent::Blocks(_) => None,
        })
        .collect::<Vec<_>>()
        .join("\n---\n");

    assert!(
        flattened.contains("Pick the implementation approach"),
        "recent assistant question should remain visible to the compactor: {flattened}"
    );
    assert!(
        flattened.contains("\n1\n") || flattened.ends_with("\n1") || flattened.contains("USER: 1"),
        "recent terse user reply should remain visible to the compactor: {flattened}"
    );
    assert!(
        flattened.contains("read_file"),
        "recent tool usage should preserve tool names for context: {flattened}"
    );
    assert!(
        !flattened.contains("/tmp/spec.md\"}"),
        "tool arguments should not be forwarded verbatim in fallback context: {flattened}"
    );
}

#[test]
fn github_copilot_tool_follow_up_disables_previous_response_continuity() {
    let mut config = AgentConfig::default();
    config.provider = PROVIDER_ID_GITHUB_COPILOT.to_string();

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
            provider: Some(PROVIDER_ID_GITHUB_COPILOT.to_string()),
            model: Some("gpt-5.4".to_string()),
            api_transport: Some(ApiTransport::Responses),
            response_id: Some("resp_123".to_string()),
            upstream_message: None,
            provider_final_result: None,
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
            upstream_message: None,
            provider_final_result: None,
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
fn native_assistant_transport_falls_back_to_compacted_message_stack_when_compaction_is_active() {
    let mut config = AgentConfig::default();
    config.provider = PROVIDER_ID_OPENAI.to_string();
    config.auto_compact_context = true;
    config.max_context_messages = 2;
    config.keep_recent_on_compact = 1;

    let mut provider = sample_provider_config();
    provider.api_transport = ApiTransport::NativeAssistant;
    provider.assistant_id = "asst_test".to_string();

    let thread = AgentThread {
        upstream_thread_id: Some("thread_upstream_123".to_string()),
        upstream_transport: Some(ApiTransport::NativeAssistant),
        upstream_provider: Some(PROVIDER_ID_OPENAI.to_string()),
        upstream_model: Some(provider.model.clone()),
        upstream_assistant_id: Some(provider.assistant_id.clone()),
        ..sample_thread(vec![
            AgentMessage::user("earlier request", 1),
            AgentMessage {
                id: "assistant-1".to_string(),
                role: MessageRole::Assistant,
                content: "Earlier assistant state that should be compacted".to_string(),
                tool_calls: None,
                tool_call_id: None,
                tool_name: None,
                tool_arguments: None,
                tool_status: None,
                weles_review: None,
                input_tokens: 0,
                output_tokens: 0,
                provider: Some(PROVIDER_ID_OPENAI.to_string()),
                model: Some(provider.model.clone()),
                api_transport: Some(ApiTransport::NativeAssistant),
                response_id: None,
                upstream_message: None,
                provider_final_result: None,
                reasoning: None,
                message_kind: AgentMessageKind::Normal,
                compaction_strategy: None,
                compaction_payload: None,
                timestamp: 2,
            },
            AgentMessage::user("continue with more work", 3),
        ])
    };

    let prepared = prepare_llm_request(&thread, &config, &provider);

    assert_ne!(prepared.transport, ApiTransport::NativeAssistant);
    assert_eq!(prepared.upstream_thread_id, None);
    assert!(prepared.previous_response_id.is_none());
    assert!(prepared.messages.len() >= 2);
}

#[test]
fn github_copilot_responses_request_uses_previous_response_id_for_plain_follow_up_turns() {
    let mut config = AgentConfig::default();
    config.provider = PROVIDER_ID_GITHUB_COPILOT.to_string();

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
            provider: Some(PROVIDER_ID_GITHUB_COPILOT.to_string()),
            model: Some("gpt-5.4".to_string()),
            api_transport: Some(ApiTransport::Responses),
            response_id: Some("resp_123".to_string()),
            upstream_message: None,
            provider_final_result: None,
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
        "provider": PROVIDER_ID_OPENAI,
        "model": "gpt-5.4",
        "compaction": {
            "strategy": "custom_model",
            "weles": {
                "provider": PROVIDER_ID_OPENAI,
                "model": "gpt-5.4-mini",
                "reasoning_effort": "low"
            },
            "custom_model": {
                "provider": PROVIDER_ID_OPENROUTER,
                "base_url": "https://openrouter.ai/api/v1",
                "model": "arcee-ai/trinity-large-thinking",
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
    assert_eq!(config.compaction.weles.provider, PROVIDER_ID_OPENAI);
    assert_eq!(config.compaction.weles.model, "gpt-5.4-mini");
    assert_eq!(config.compaction.weles.reasoning_effort, "low");
    assert_eq!(
        config.compaction.custom_model.provider,
        PROVIDER_ID_OPENROUTER
    );
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
        PROVIDER_ID_OPENROUTER
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
        upstream_message: None,
        provider_final_result: None,
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
            upstream_message: None,
            provider_final_result: None,
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

#[tokio::test]
async fn compaction_persists_context_compression_causal_trace() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.auto_compact_context = true;
    config.max_context_messages = 2;
    config.keep_recent_on_compact = 1;
    config.compaction.strategy = CompactionStrategy::Heuristic;
    let engine = AgentEngine::new_test(manager, config.clone(), root.path()).await;
    let provider = sample_provider_config();
    let thread_id = "compaction-trace-thread";

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

    let records = engine
        .history
        .list_recent_causal_trace_records("context_compression", 1)
        .await
        .expect("list context compression traces");
    assert_eq!(
        records.len(),
        1,
        "compaction should persist one context compression causal trace"
    );
}
