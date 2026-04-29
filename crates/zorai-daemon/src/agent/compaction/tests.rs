use super::*;
use crate::agent::AgentEngine;
use crate::session_manager::SessionManager;
use std::fs;
use tempfile::tempdir;
use zorai_shared::providers::{
    PROVIDER_ID_ALIBABA_CODING_PLAN, PROVIDER_ID_GITHUB_COPILOT, PROVIDER_ID_OPENAI,
    PROVIDER_ID_OPENROUTER,
};

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

fn compaction_artifact_message(thread: &AgentThread) -> &AgentMessage {
    thread
        .messages
        .iter()
        .find(|message| message.message_kind == AgentMessageKind::CompactionArtifact)
        .expect("thread should contain a compaction artifact")
}

fn assert_markdown_section_order(payload: &str, sections: &[&str]) {
    let mut search_start = 0usize;
    for section in sections {
        let relative_index = payload[search_start..]
            .find(section)
            .unwrap_or_else(|| panic!("missing section {section} in payload: {payload}"));
        search_start += relative_index + section.len();
    }
}

fn api_text_contents(messages: &[ApiMessage]) -> Vec<String> {
    messages
        .iter()
        .map(|message| match &message.content {
            ApiContent::Text(text) => text.clone(),
            other => panic!("expected text content, got {other:?}"),
        })
        .collect()
}

fn collect_workflow_notice_messages(
    receiver: &mut tokio::sync::broadcast::Receiver<AgentEvent>,
    thread_id: &str,
) -> Vec<String> {
    let mut notices = Vec::new();
    while let Ok(event) = receiver.try_recv() {
        if let AgentEvent::WorkflowNotice {
            thread_id: event_thread_id,
            message,
            ..
        } = event
        {
            if event_thread_id == thread_id {
                notices.push(message);
            }
        }
    }
    notices
}

fn collect_workflow_notices(
    receiver: &mut tokio::sync::broadcast::Receiver<AgentEvent>,
    thread_id: &str,
) -> Vec<(String, String, Option<String>)> {
    let mut notices = Vec::new();
    while let Ok(event) = receiver.try_recv() {
        if let AgentEvent::WorkflowNotice {
            thread_id: event_thread_id,
            kind,
            message,
            details,
        } = event
        {
            if event_thread_id == thread_id {
                notices.push((kind, message, details));
            }
        }
    }
    notices
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
fn compaction_candidate_uses_known_model_window_over_larger_inherited_config_window() {
    let mut config = AgentConfig::default();
    config.provider = PROVIDER_ID_ALIBABA_CODING_PLAN.to_string();
    config.context_window_tokens = 400_000;
    config.compact_threshold_pct = 80;
    config.max_context_messages = 500;
    config.keep_recent_on_compact = 1;

    let mut provider = sample_provider_config();
    provider.model = "glm-5".to_string();
    provider.context_window_tokens = 983_616;

    let payload = "x".repeat(30_000);
    let messages = (0..30)
        .map(|idx| AgentMessage::user(payload.clone(), idx as u64 + 1))
        .collect::<Vec<_>>();

    let candidate =
        compaction_candidate(&messages, &config, &provider).expect("candidate should exist");

    assert_eq!(candidate.trigger, CompactionTrigger::TokenThreshold);
    assert!(
        candidate.target_tokens <= 162_201,
        "expected glm-5 catalog window to cap compaction target, got {}",
        candidate.target_tokens
    );
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
fn api_message_token_estimate_uses_token_units_not_raw_chars() {
    let message = ApiMessage {
        role: "user".to_string(),
        content: ApiContent::Text("x".repeat(APPROX_CHARS_PER_TOKEN * 100)),
        reasoning: None,
        tool_call_id: None,
        name: None,
        tool_calls: None,
    };

    let estimated = estimate_api_message_tokens(&message);

    assert!(estimated < 200, "expected token estimate, got {estimated}");
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
    assert_eq!(candidate.trigger, CompactionTrigger::MessageCount);
}

#[test]
fn heuristic_message_count_alone_still_triggers_compaction() {
    let mut config = AgentConfig::default();
    config.compaction.strategy = CompactionStrategy::Heuristic;
    config.max_context_messages = 100;
    config.keep_recent_on_compact = 10;
    config.context_window_tokens = 400_000;
    config.compact_threshold_pct = 80;

    let mut provider = sample_provider_config();
    provider.context_window_tokens = 400_000;

    let messages = (0..101)
        .map(|idx| AgentMessage::user(format!("m{idx}"), idx as u64 + 1))
        .collect::<Vec<_>>();

    let candidate =
        compaction_candidate(&messages, &config, &provider).expect("candidate should exist");

    assert_eq!(candidate.target_tokens, 320_000);
    assert_eq!(candidate.trigger, CompactionTrigger::MessageCount);
}

#[test]
fn custom_model_message_count_alone_does_not_trigger_compaction() {
    let mut config = AgentConfig::default();
    config.compaction.strategy = CompactionStrategy::CustomModel;
    config.max_context_messages = 100;
    config.keep_recent_on_compact = 10;
    config.context_window_tokens = 400_000;
    config.compact_threshold_pct = 80;
    config.compaction.custom_model.context_window_tokens = 1_000_000;

    let mut provider = sample_provider_config();
    provider.context_window_tokens = 400_000;

    let messages = (0..101)
        .map(|idx| AgentMessage::user(format!("m{idx}"), idx as u64 + 1))
        .collect::<Vec<_>>();

    assert_eq!(compaction_candidate(&messages, &config, &provider), None);
}

#[test]
fn weles_message_count_alone_does_not_trigger_compaction() {
    let mut config = AgentConfig::default();
    config.compaction.strategy = CompactionStrategy::Weles;
    config.max_context_messages = 100;
    config.keep_recent_on_compact = 10;
    config.context_window_tokens = 400_000;
    config.compact_threshold_pct = 80;
    config.compaction.weles.provider = PROVIDER_ID_ALIBABA_CODING_PLAN.to_string();
    config.compaction.weles.model = "qwen3.6-plus".to_string();

    let mut provider = sample_provider_config();
    provider.context_window_tokens = 400_000;

    let messages = (0..101)
        .map(|idx| AgentMessage::user(format!("m{idx}"), idx as u64 + 1))
        .collect::<Vec<_>>();

    assert_eq!(compaction_candidate(&messages, &config, &provider), None);
}

#[test]
fn forced_compaction_candidate_exists_below_threshold() {
    let mut config = AgentConfig::default();
    config.compaction.strategy = CompactionStrategy::CustomModel;
    config.max_context_messages = 100;
    config.keep_recent_on_compact = 1;
    config.context_window_tokens = 400_000;
    config.compact_threshold_pct = 80;
    config.compaction.custom_model.context_window_tokens = 1_000_000;

    let mut provider = sample_provider_config();
    provider.context_window_tokens = 400_000;

    let messages = vec![
        AgentMessage::user("short one", 1),
        AgentMessage {
            role: MessageRole::Assistant,
            content: "short two".to_string(),
            timestamp: 2,
            ..sample_message("placeholder")
        },
        AgentMessage::user("short three", 3),
    ];

    let candidate = forced_compaction_candidate(&messages, &config, &provider)
        .expect("forced candidate should exist");

    assert_eq!(candidate.split_at, 2);
    assert_eq!(candidate.target_tokens, 320_000);
    assert_eq!(candidate.trigger, CompactionTrigger::ManualRequest);
}

#[test]
fn heuristic_compaction_summary_uses_checkpoint_schema() {
    let summary = build_compaction_summary(
        &[
            AgentMessage::user(
                "Session: zorai-landing website updates. Working directory: /tmp/demo. Completed: Added Agents nav link.",
                1,
            ),
            AgentMessage {
                id: "assistant-1".to_string(),
                role: MessageRole::Assistant,
                content: "Verified HTML and CSS. Status: checking responsive layout next."
                    .to_string(),
                content_blocks: Vec::new(),
                tool_calls: None,
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
                message_kind: AgentMessageKind::Normal,
                compaction_strategy: None,
                compaction_payload: None,
                offloaded_payload_id: None,
                tool_output_preview_path: None,
                structural_refs: Vec::new(),
                pinned_for_compaction: false,
                timestamp: 2,
            },
            AgentMessage {
                id: "tool-1".to_string(),
                role: MessageRole::Tool,
                content: "styles.css read complete".to_string(),
                content_blocks: Vec::new(),
                tool_calls: None,
                tool_call_id: Some("call_1".to_string()),
                tool_name: Some("read_file".to_string()),
                tool_arguments: Some("{\"path\":\"styles.css\"}".to_string()),
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
                message_kind: AgentMessageKind::Normal,
                compaction_strategy: None,
                compaction_payload: None,
                offloaded_payload_id: None,
                tool_output_preview_path: None,
                structural_refs: Vec::new(),
                pinned_for_compaction: false,
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
        content_blocks: Vec::new(),
        tool_calls: None,
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
        message_kind: AgentMessageKind::Normal,
        compaction_strategy: None,
        compaction_payload: None,
        offloaded_payload_id: None,
        tool_output_preview_path: None,
        structural_refs: Vec::new(),
        pinned_for_compaction: false,
        timestamp: 30,
    });
    messages.push(AgentMessage::user("1", 31));
    messages.push(AgentMessage {
        id: "assistant-tool-call".to_string(),
        role: MessageRole::Assistant,
        content: String::new(),
        content_blocks: Vec::new(),
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
        message_kind: AgentMessageKind::Normal,
        compaction_strategy: None,
        compaction_payload: None,
        offloaded_payload_id: None,
        tool_output_preview_path: None,
        structural_refs: Vec::new(),
        pinned_for_compaction: false,
        timestamp: 32,
    });
    messages.push(AgentMessage {
        id: "tool-read".to_string(),
        role: MessageRole::Tool,
        content: "Full file contents that should not be passed verbatim to the compactor"
            .to_string(),
        content_blocks: Vec::new(),
        tool_calls: None,
        tool_call_id: Some("call_read".to_string()),
        tool_name: Some("read_file".to_string()),
        tool_arguments: Some("{\"path\":\"/tmp/spec.md\"}".to_string()),
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
        message_kind: AgentMessageKind::Normal,
        compaction_strategy: None,
        compaction_payload: None,
        offloaded_payload_id: None,
        tool_output_preview_path: None,
        structural_refs: Vec::new(),
        pinned_for_compaction: false,
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
            content_blocks: Vec::new(),
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
            cost: None,
            provider: Some(PROVIDER_ID_GITHUB_COPILOT.to_string()),
            model: Some("gpt-5.4".to_string()),
            api_transport: Some(ApiTransport::Responses),
            response_id: Some("resp_123".to_string()),
            upstream_message: None,
            provider_final_result: None,
            author_agent_id: None,
            author_agent_name: None,
            reasoning: Some("reasoned".to_string()),
            message_kind: AgentMessageKind::Normal,
            compaction_strategy: None,
            compaction_payload: None,
            offloaded_payload_id: None,
            tool_output_preview_path: None,
            structural_refs: Vec::new(),
            pinned_for_compaction: false,
            timestamp: 2,
        },
        AgentMessage {
            id: "tool-1".to_string(),
            role: MessageRole::Tool,
            content: "file list".to_string(),
            content_blocks: Vec::new(),
            tool_calls: None,
            tool_call_id: Some("call_1".to_string()),
            tool_name: Some("list_files".to_string()),
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
            message_kind: AgentMessageKind::Normal,
            compaction_strategy: None,
            compaction_payload: None,
            offloaded_payload_id: None,
            tool_output_preview_path: None,
            structural_refs: Vec::new(),
            pinned_for_compaction: false,
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
                content_blocks: Vec::new(),
                tool_calls: None,
                tool_call_id: None,
                tool_name: None,
                tool_arguments: None,
                tool_status: None,
                weles_review: None,
                input_tokens: 0,
                output_tokens: 0,
                cost: None,
                provider: Some(PROVIDER_ID_OPENAI.to_string()),
                model: Some(provider.model.clone()),
                api_transport: Some(ApiTransport::NativeAssistant),
                response_id: None,
                upstream_message: None,
                provider_final_result: None,
                author_agent_id: None,
                author_agent_name: None,
                reasoning: None,
                message_kind: AgentMessageKind::Normal,
                compaction_strategy: None,
                compaction_payload: None,
                offloaded_payload_id: None,
                tool_output_preview_path: None,
                structural_refs: Vec::new(),
                pinned_for_compaction: false,
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
fn github_copilot_responses_request_does_not_use_previous_response_id_for_plain_follow_up_turns() {
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
            content_blocks: Vec::new(),
            tool_calls: None,
            tool_call_id: None,
            tool_name: None,
            tool_arguments: None,
            tool_status: None,
            weles_review: None,
            input_tokens: 11,
            output_tokens: 7,
            cost: None,
            provider: Some(PROVIDER_ID_GITHUB_COPILOT.to_string()),
            model: Some("gpt-5.4".to_string()),
            api_transport: Some(ApiTransport::Responses),
            response_id: Some("resp_123".to_string()),
            upstream_message: None,
            provider_final_result: None,
            author_agent_id: None,
            author_agent_name: None,
            reasoning: Some("reasoned".to_string()),
            message_kind: AgentMessageKind::Normal,
            compaction_strategy: None,
            compaction_payload: None,
            offloaded_payload_id: None,
            tool_output_preview_path: None,
            structural_refs: Vec::new(),
            pinned_for_compaction: false,
            timestamp: 2,
        },
        AgentMessage::user("continue", 3),
    ]);

    let prepared = prepare_llm_request(&thread, &config, &provider);

    assert_eq!(prepared.transport, ApiTransport::Responses);
    assert!(prepared.previous_response_id.is_none());
    assert_eq!(prepared.messages.len(), 3);
    assert_eq!(prepared.messages[0].role, "user");
    assert_eq!(prepared.messages[1].role, "assistant");
    assert_eq!(prepared.messages[2].role, "user");
}

#[test]
fn chatgpt_subscription_responses_request_uses_local_thread_id_instead_of_previous_response_id() {
    let mut config = AgentConfig::default();
    config.provider = PROVIDER_ID_OPENAI.to_string();

    let provider = ProviderConfig {
        base_url: "https://api.openai.com/v1".to_string(),
        model: "gpt-5.4".to_string(),
        api_key: String::new(),
        assistant_id: String::new(),
        auth_source: AuthSource::ChatgptSubscription,
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
            content_blocks: Vec::new(),
            tool_calls: None,
            tool_call_id: None,
            tool_name: None,
            tool_arguments: None,
            tool_status: None,
            weles_review: None,
            input_tokens: 11,
            output_tokens: 7,
            cost: None,
            provider: Some(PROVIDER_ID_OPENAI.to_string()),
            model: Some("gpt-5.4".to_string()),
            api_transport: Some(ApiTransport::Responses),
            response_id: Some("resp_123".to_string()),
            upstream_message: None,
            provider_final_result: None,
            author_agent_id: None,
            author_agent_name: None,
            reasoning: None,
            message_kind: AgentMessageKind::Normal,
            compaction_strategy: None,
            compaction_payload: None,
            offloaded_payload_id: None,
            tool_output_preview_path: None,
            structural_refs: Vec::new(),
            pinned_for_compaction: false,
            timestamp: 2,
        },
        AgentMessage::user("continue", 3),
    ]);

    let prepared = prepare_llm_request(&thread, &config, &provider);

    assert_eq!(prepared.transport, ApiTransport::Responses);
    assert!(prepared.previous_response_id.is_none());
    assert_eq!(prepared.upstream_thread_id.as_deref(), Some("thread-1"));
    assert_eq!(prepared.messages.len(), 3);
    assert_eq!(prepared.messages[0].role, "user");
    assert_eq!(prepared.messages[1].role, "assistant");
    assert_eq!(prepared.messages[2].role, "user");
}

#[test]
fn openai_responses_request_keeps_previous_response_id_when_compaction_artifact_exists() {
    let mut config = AgentConfig::default();
    config.provider = PROVIDER_ID_OPENAI.to_string();

    let provider = ProviderConfig {
        base_url: "https://api.openai.com/v1".to_string(),
        model: "gpt-5.4".to_string(),
        api_key: String::new(),
        assistant_id: String::new(),
        auth_source: AuthSource::ApiKey,
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
        AgentMessage::user("older request", 1),
        AgentMessage {
            id: "compaction-1".to_string(),
            role: MessageRole::Assistant,
            content: "rule based".to_string(),
            content_blocks: Vec::new(),
            tool_calls: None,
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
            message_kind: AgentMessageKind::CompactionArtifact,
            compaction_strategy: Some(CompactionStrategy::Heuristic),
            compaction_payload: Some("Older context compacted for continuity".to_string()),
            offloaded_payload_id: None,
            tool_output_preview_path: None,
            structural_refs: Vec::new(),
            pinned_for_compaction: false,
            timestamp: 2,
        },
        AgentMessage::user("latest request", 3),
        AgentMessage {
            id: "assistant-1".to_string(),
            role: MessageRole::Assistant,
            content: "Latest answer".to_string(),
            content_blocks: Vec::new(),
            tool_calls: None,
            tool_call_id: None,
            tool_name: None,
            tool_arguments: None,
            tool_status: None,
            weles_review: None,
            input_tokens: 11,
            output_tokens: 7,
            cost: None,
            provider: Some(PROVIDER_ID_OPENAI.to_string()),
            model: Some("gpt-5.4".to_string()),
            api_transport: Some(ApiTransport::Responses),
            response_id: Some("resp_123".to_string()),
            upstream_message: None,
            provider_final_result: None,
            author_agent_id: None,
            author_agent_name: None,
            reasoning: None,
            message_kind: AgentMessageKind::Normal,
            compaction_strategy: None,
            compaction_payload: None,
            offloaded_payload_id: None,
            tool_output_preview_path: None,
            structural_refs: Vec::new(),
            pinned_for_compaction: false,
            timestamp: 4,
        },
        AgentMessage::user("continue", 5),
    ]);

    let prepared = prepare_llm_request(&thread, &config, &provider);

    assert_eq!(prepared.transport, ApiTransport::Responses);
    assert_eq!(prepared.previous_response_id.as_deref(), Some("resp_123"));
    assert_eq!(prepared.messages.len(), 1);
    assert_eq!(prepared.messages[0].role, "user");
}

#[test]
fn reused_user_turn_is_injected_when_responses_continuation_only_has_weles_notice() {
    let mut config = AgentConfig::default();
    config.provider = PROVIDER_ID_OPENAI.to_string();

    let provider = ProviderConfig {
        base_url: "https://api.openai.com/v1".to_string(),
        model: "gpt-5.4".to_string(),
        api_key: String::new(),
        assistant_id: String::new(),
        auth_source: AuthSource::ApiKey,
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

    let mut recovery_notice = AgentMessage::user(
        "WELES stalled-turn recovery: Your stream went idle before completion. Resume the unfinished turn now.",
        3,
    );
    recovery_notice.role = MessageRole::System;

    let thread = sample_thread(vec![
        AgentMessage::user("finish the refactor", 1),
        AgentMessage {
            id: "assistant-1".to_string(),
            role: MessageRole::Assistant,
            content: "I started the refactor but stopped early.".to_string(),
            content_blocks: Vec::new(),
            tool_calls: None,
            tool_call_id: None,
            tool_name: None,
            tool_arguments: None,
            tool_status: None,
            weles_review: None,
            input_tokens: 11,
            output_tokens: 7,
            cost: None,
            provider: Some(PROVIDER_ID_OPENAI.to_string()),
            model: Some("gpt-5.4".to_string()),
            api_transport: Some(ApiTransport::Responses),
            response_id: Some("resp_123".to_string()),
            upstream_message: None,
            provider_final_result: None,
            author_agent_id: None,
            author_agent_name: None,
            reasoning: None,
            message_kind: AgentMessageKind::Normal,
            compaction_strategy: None,
            compaction_payload: None,
            offloaded_payload_id: None,
            tool_output_preview_path: None,
            structural_refs: Vec::new(),
            pinned_for_compaction: false,
            timestamp: 2,
        },
        recovery_notice,
    ]);

    let prepared = prepare_llm_request_with_reused_user_message(
        &thread,
        &config,
        &provider,
        Some("finish the refactor"),
    );

    assert_eq!(prepared.transport, ApiTransport::Responses);
    assert_eq!(prepared.previous_response_id.as_deref(), Some("resp_123"));
    assert_eq!(prepared.messages.len(), 2);
    assert_eq!(prepared.messages[0].role, "system");
    assert_eq!(prepared.messages[1].role, "user");
    match &prepared.messages[1].content {
        ApiContent::Text(content) => assert_eq!(content, "finish the refactor"),
        other => panic!("expected reused user text content, got {other:?}"),
    }
}

#[test]
fn default_agent_config_exposes_heuristic_compaction_strategy_defaults() {
    let config = AgentConfig::default();

    assert_eq!(config.compaction.strategy, CompactionStrategy::Heuristic);
    assert_eq!(config.compaction.weles.reasoning_effort, "medium");
    assert_eq!(config.compaction.custom_model.reasoning_effort, "high");
}

#[test]
fn reused_user_message_is_injected_for_chat_completions_when_continuation_has_no_user_turn() {
    let mut config = AgentConfig::default();
    config.provider = PROVIDER_ID_ALIBABA_CODING_PLAN.to_string();

    let mut provider = sample_provider_config();
    provider.model = "kimi-k2.5".to_string();
    provider.api_transport = ApiTransport::ChatCompletions;

    let thread = sample_thread(vec![
        AgentMessage {
            id: "assistant-1".to_string(),
            role: MessageRole::Assistant,
            content: String::new(),
            content_blocks: Vec::new(),
            tool_calls: Some(vec![ToolCall {
                id: "call-1".to_string(),
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
            input_tokens: 0,
            output_tokens: 0,
            cost: None,
            provider: Some(PROVIDER_ID_ALIBABA_CODING_PLAN.to_string()),
            model: Some("kimi-k2.5".to_string()),
            api_transport: Some(ApiTransport::ChatCompletions),
            response_id: None,
            upstream_message: None,
            provider_final_result: None,
            author_agent_id: None,
            author_agent_name: None,
            reasoning: None,
            message_kind: AgentMessageKind::Normal,
            compaction_strategy: None,
            compaction_payload: None,
            offloaded_payload_id: None,
            tool_output_preview_path: None,
            structural_refs: Vec::new(),
            pinned_for_compaction: false,
            timestamp: 1,
        },
        AgentMessage {
            id: "tool-1".to_string(),
            role: MessageRole::Tool,
            content: "[]".to_string(),
            content_blocks: Vec::new(),
            tool_calls: None,
            tool_call_id: Some("call-1".to_string()),
            tool_name: Some("list_files".to_string()),
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
            message_kind: AgentMessageKind::Normal,
            compaction_strategy: None,
            compaction_payload: None,
            offloaded_payload_id: None,
            tool_output_preview_path: None,
            structural_refs: Vec::new(),
            pinned_for_compaction: false,
            timestamp: 2,
        },
    ]);

    let prepared = prepare_llm_request_with_reused_user_message(
        &thread,
        &config,
        &provider,
        Some("continue with the file review"),
    );

    assert_eq!(prepared.transport, ApiTransport::ChatCompletions);
    assert!(
        prepared
            .messages
            .iter()
            .any(|message| message.role == "user"),
        "expected reused user message to be injected into chat completions requests"
    );
    assert_eq!(
        prepared
            .messages
            .iter()
            .find(|message| message.role == "user")
            .and_then(|message| match &message.content {
                ApiContent::Text(content) => Some(content.as_str()),
                ApiContent::Blocks(_) => None,
            }),
        Some("continue with the file review")
    );
}

#[test]
fn heuristic_effective_context_target_uses_only_primary_model_threshold() {
    let mut config = AgentConfig::default();
    config.compaction.strategy = CompactionStrategy::Heuristic;
    config.context_window_tokens = 400_000;
    config.compact_threshold_pct = 80;
    config.compaction.custom_model.context_window_tokens = 128_000;
    config.compaction.weles.provider = PROVIDER_ID_ALIBABA_CODING_PLAN.to_string();
    config.compaction.weles.model = "MiniMax-M2.7".to_string();

    let mut provider = sample_provider_config();
    provider.context_window_tokens = 400_000;

    assert_eq!(effective_context_target_tokens(&config, &provider), 320_000);
}

#[test]
fn weles_effective_context_target_caps_primary_threshold_by_weles_window() {
    let mut config = AgentConfig::default();
    config.compaction.strategy = CompactionStrategy::Weles;
    config.context_window_tokens = 400_000;
    config.compact_threshold_pct = 80;
    config.compaction.weles.provider = "minimax-coding-plan".to_string();
    config.compaction.weles.model = "MiniMax-M2.7".to_string();

    let mut provider = sample_provider_config();
    provider.context_window_tokens = 400_000;

    assert_eq!(effective_context_target_tokens(&config, &provider), 164_000);
}

#[test]
fn custom_model_effective_context_target_caps_primary_threshold_by_custom_window() {
    let mut config = AgentConfig::default();
    config.compaction.strategy = CompactionStrategy::CustomModel;
    config.context_window_tokens = 400_000;
    config.compact_threshold_pct = 80;
    config.compaction.custom_model.context_window_tokens = 160_000;

    let mut provider = sample_provider_config();
    provider.context_window_tokens = 400_000;

    assert_eq!(effective_context_target_tokens(&config, &provider), 128_000);
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
        content:
            "Pre-compaction context: ~182,400 / 200,000 tokens (threshold 160,000)\nTrigger: message-count\nStrategy: rule based\n\nContent:\nOlder context compacted for continuity"
                .to_string(),
        content_blocks: Vec::new(),
        tool_calls: None,
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
        timestamp: 99,
        message_kind: AgentMessageKind::CompactionArtifact,
        compaction_strategy: Some(CompactionStrategy::Heuristic),
        compaction_payload: Some("Older context compacted for continuity".to_string()),
        offloaded_payload_id: None,
        tool_output_preview_path: None,
        structural_refs: Vec::new(),
        pinned_for_compaction: false,
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
    assert_eq!(decoded.content, message.content);
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
            content_blocks: Vec::new(),
            tool_calls: None,
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
            message_kind: AgentMessageKind::CompactionArtifact,
            compaction_strategy: Some(CompactionStrategy::Heuristic),
            compaction_payload: Some("Older context compacted for continuity".to_string()),
            offloaded_payload_id: None,
            tool_output_preview_path: None,
            structural_refs: Vec::new(),
            pinned_for_compaction: false,
            timestamp: 3,
        },
        sample_message("recent one"),
        sample_message("recent two"),
    ];

    let candidate =
        compaction_candidate(&messages, &config, &provider).expect("candidate should exist");

    assert_eq!(candidate.split_at, 2);
}

#[test]
fn owner_compaction_appends_pinned_messages_after_artifact() {
    let mut config = AgentConfig::default();
    config.auto_compact_context = true;
    config.max_context_messages = 3;
    config.keep_recent_on_compact = 1;
    let provider = sample_provider_config();

    let mut pinned_user = sample_message("owner pin alpha");
    pinned_user.pinned_for_compaction = true;

    let mut pinned_assistant = sample_message("owner pin beta");
    pinned_assistant.role = MessageRole::Assistant;
    pinned_assistant.pinned_for_compaction = true;

    let thread = sample_thread(vec![
        sample_message("older context that should be compacted"),
        pinned_user,
        pinned_assistant,
        sample_message("recent tail"),
    ]);

    let prepared = prepare_llm_request(&thread, &config, &provider);

    assert_eq!(
        prepared.messages.len(),
        4,
        "pinned messages should be injected after the compaction artifact"
    );
    assert_eq!(prepared.messages[1].role, "user");
    assert_eq!(prepared.messages[2].role, "assistant");
    assert_eq!(api_text_contents(&prepared.messages)[1], "owner pin alpha");
    assert_eq!(api_text_contents(&prepared.messages)[2], "owner pin beta");
    assert_eq!(api_text_contents(&prepared.messages)[3], "recent tail");
}

#[test]
fn non_compacted_request_ignores_pinned_messages() {
    let mut config = AgentConfig::default();
    config.auto_compact_context = false;
    let provider = sample_provider_config();

    let mut pinned = sample_message("owner pin placeholder");
    pinned.pinned_for_compaction = true;

    let thread = sample_thread(vec![
        sample_message("base context"),
        pinned,
        sample_message("recent tail"),
    ]);

    let prepared = prepare_llm_request(&thread, &config, &provider);

    assert_eq!(
        api_text_contents(&prepared.messages),
        vec![
            "base context".to_string(),
            "owner pin placeholder".to_string(),
            "recent tail".to_string(),
        ],
        "pinned messages should not be injected separately when compaction is inactive"
    );
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
    assert_eq!(thread.messages[0].content, "older one");
    assert_eq!(thread.messages[1].content, "older two");
    let artifact = compaction_artifact_message(&thread);
    assert_eq!(
        thread.messages[2].message_kind,
        AgentMessageKind::CompactionArtifact
    );
    assert_eq!(thread.messages[3].content, "recent one");
    assert_eq!(artifact.message_kind, AgentMessageKind::CompactionArtifact);
    assert_eq!(
        artifact.compaction_strategy,
        Some(CompactionStrategy::Heuristic)
    );
    assert!(
        artifact.content.contains("Pre-compaction context:"),
        "expected a visible trigger summary on the compaction artifact"
    );
    assert!(
        artifact.content.contains("Strategy: rule based"),
        "expected the visible content to mention the compaction strategy: {}",
        artifact.content
    );
    assert!(
        artifact.content.contains("Trigger:"),
        "expected the visible content to mention the compaction trigger: {}",
        artifact.content
    );
    assert!(
        !artifact.content.contains("\n\nContent:\n"),
        "visible content should stay as the short provenance header: {}",
        artifact.content
    );
    assert!(artifact
        .compaction_payload
        .as_deref()
        .is_some_and(|payload| payload.starts_with("# 🤖 Agent Context: State Checkpoint")));
    let visible_payload = artifact
        .compaction_payload
        .as_deref()
        .expect("artifact should carry hidden payload");
    assert!(
        !artifact.content.contains(visible_payload),
        "hidden payload should remain separate from the visible artifact header: {}",
        artifact.content,
    );

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
    let rehydrated_thread = rehydrated
        .get_thread(thread_id)
        .await
        .expect("rehydrated thread should exist");
    assert_eq!(rehydrated_thread.messages.len(), 4);
    let restored_artifact = compaction_artifact_message(&rehydrated_thread);
    assert_eq!(
        restored_artifact.message_kind,
        AgentMessageKind::CompactionArtifact
    );
    assert_eq!(restored_artifact.content, artifact.content);
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
async fn auto_compaction_notice_includes_artifact_location_details() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.auto_compact_context = true;
    config.max_context_messages = 500;
    config.keep_recent_on_compact = 1;
    config.context_window_tokens = 50_000;
    config.compaction.strategy = CompactionStrategy::Heuristic;
    let engine = AgentEngine::new_test(manager, config.clone(), root.path()).await;
    let thread_id = "thread-compaction-notice-details";
    let mut events = engine.subscribe();
    let mut thread = sample_thread(Vec::new());
    thread.id = thread_id.to_string();
    thread.title = "Compaction Notice".to_string();
    thread.messages.push(AgentMessage::user("short intro", 1));
    for idx in 0..80 {
        let mut message = AgentMessage::user(
            format!("message {idx} {}", "x".repeat(2_000)),
            idx as u64 + 2,
        );
        if idx % 2 != 0 {
            message.role = MessageRole::Assistant;
        }
        thread.messages.push(message);
    }
    engine
        .threads
        .write()
        .await
        .insert(thread_id.to_string(), thread);

    let mut provider = sample_provider_config();
    provider.context_window_tokens = 50_000;
    let persisted = engine
        .maybe_persist_compaction_artifact(thread_id, None, &config, &provider)
        .await
        .expect("compaction should succeed");
    assert!(persisted, "expected compaction artifact to be persisted");

    let notices = collect_workflow_notices(&mut events, thread_id);
    let (_, message, details) = notices
        .into_iter()
        .find(|(kind, _, _)| kind == "auto-compaction")
        .expect("expected auto-compaction workflow notice");
    let details = details.expect("auto-compaction notice should include details");
    let parsed: serde_json::Value =
        serde_json::from_str(&details).expect("details should be valid json");

    assert!(
        parsed
            .get("split_at")
            .and_then(serde_json::Value::as_u64)
            .is_some(),
        "expected split_at in auto-compaction details: {parsed}"
    );
    assert!(
        parsed
            .get("total_message_count")
            .and_then(serde_json::Value::as_u64)
            .is_some(),
        "expected total_message_count in auto-compaction details: {parsed}"
    );
    assert!(
        parsed
            .get("pre_compaction_total_tokens")
            .and_then(serde_json::Value::as_u64)
            .is_some(),
        "expected pre_compaction_total_tokens in auto-compaction details: {parsed}"
    );
    assert!(
        parsed
            .get("effective_context_window_tokens")
            .and_then(serde_json::Value::as_u64)
            .is_some(),
        "expected effective_context_window_tokens in auto-compaction details: {parsed}"
    );
    assert!(
        parsed
            .get("target_tokens")
            .and_then(serde_json::Value::as_u64)
            .is_some(),
        "expected target_tokens in auto-compaction details: {parsed}"
    );
    assert_eq!(
        parsed.get("trigger").and_then(serde_json::Value::as_str),
        Some("token_threshold"),
        "expected machine-readable trigger in auto-compaction details: {parsed}"
    );
    assert!(
        message.contains("Pre-compaction context:"),
        "expected trigger summary in auto-compaction message: {message}"
    );
    assert!(
        message.contains("Trigger: token-threshold"),
        "expected trigger line in auto-compaction message: {message}"
    );
    assert!(
        !message.contains("\n\nContent:\n"),
        "workflow notice should stay concise for live delivery: {message}"
    );
}

#[tokio::test]
async fn coding_compaction_payload_prefers_structural_digest_and_offload_refs() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.auto_compact_context = true;
    config.max_context_messages = 2;
    config.keep_recent_on_compact = 1;
    config.compaction.strategy = CompactionStrategy::Heuristic;
    let engine = AgentEngine::new_test(manager, config.clone(), root.path()).await;
    let provider = sample_provider_config();
    let thread_id = "coding-compaction-thread";

    engine.thread_structural_memories.write().await.insert(
        thread_id.to_string(),
        crate::agent::context::structural_memory::ThreadStructuralMemory {
            workspace_seed_scan_complete: true,
            language_hints: vec!["rust".to_string()],
            workspace_seeds: vec![crate::agent::context::structural_memory::WorkspaceSeed {
                node_id: "node:file:Cargo.toml".to_string(),
                relative_path: "Cargo.toml".to_string(),
                kind: "cargo_manifest".to_string(),
            }],
            observed_files: vec![
                crate::agent::context::structural_memory::ObservedFileNode {
                    node_id: "node:file:src/lib.rs".to_string(),
                    relative_path: "src/lib.rs".to_string(),
                },
                crate::agent::context::structural_memory::ObservedFileNode {
                    node_id: "node:file:src/parser.rs".to_string(),
                    relative_path: "src/parser.rs".to_string(),
                },
            ],
            edges: vec![
                crate::agent::context::structural_memory::StructuralEdge {
                    from: "node:file:Cargo.toml".to_string(),
                    to: "node:file:.".to_string(),
                    kind: "crate_path".to_string(),
                },
                crate::agent::context::structural_memory::StructuralEdge {
                    from: "node:file:src/lib.rs".to_string(),
                    to: "node:file:src/parser.rs".to_string(),
                    kind: "imported_file".to_string(),
                },
            ],
        },
    );
    engine
        .history
        .upsert_offloaded_payload_metadata(
            "payload-1",
            thread_id,
            "read_file",
            Some("call-1"),
            "text/plain",
            2_048,
            "Tool result offloaded\n- tool: read_file\n- status: done\n- bytes: 2048\n- payload_id: payload-1\n- key findings:\n  - parse() now returns Result\n",
            1_717_170_000,
        )
        .await
        .expect("offloaded payload metadata should be stored");

    {
        let mut threads = engine.threads.write().await;
        threads.insert(
            thread_id.to_string(),
            sample_thread(vec![
                AgentMessage::user(
                    "Update the Rust parser flow and keep the coding context compact.",
                    1,
                ),
                AgentMessage {
                    id: "tool-older".to_string(),
                    role: MessageRole::Tool,
                    content: "Tool result offloaded".to_string(),
                    content_blocks: Vec::new(),
                    tool_calls: None,
                    tool_call_id: Some("call-1".to_string()),
                    tool_name: Some("read_file".to_string()),
                    tool_arguments: Some(
                        serde_json::json!({"filePath":"/repo/src/lib.rs"}).to_string(),
                    ),
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
                    message_kind: AgentMessageKind::Normal,
                    compaction_strategy: None,
                    compaction_payload: None,
                    offloaded_payload_id: Some("payload-1".to_string()),
                    tool_output_preview_path: None,
                    structural_refs: vec!["node:file:src/lib.rs".to_string()],
                    pinned_for_compaction: false,
                    timestamp: 2,
                },
                AgentMessage::user("Continue with the fix and validate it.", 3),
            ]),
        );
        let thread = threads.get_mut(thread_id).expect("thread should exist");
        thread.id = thread_id.to_string();
    }

    let inserted = engine
        .maybe_persist_compaction_artifact(thread_id, None, &config, &provider)
        .await
        .expect("coding compaction should succeed");
    assert!(inserted, "compaction artifact should be inserted");

    let thread = {
        let threads = engine.threads.read().await;
        threads
            .get(thread_id)
            .cloned()
            .expect("thread should exist after compaction")
    };
    let artifact = compaction_artifact_message(&thread);
    let payload = artifact
        .compaction_payload
        .as_deref()
        .expect("coding artifact should carry payload");

    assert_markdown_section_order(
        payload,
        &[
            "## Primary Objective",
            "## Execution Map",
            "## Structural Context",
            "## Offloaded Payload References",
            "## Immediate Next Step",
        ],
    );
    assert!(payload.contains("node:file:src/lib.rs"));
    assert!(payload.contains("src/lib.rs"));
    assert!(payload.contains("node:file:src/parser.rs"));
    assert!(payload.contains("payload-1"));
    println!("PAYLOAD:\n{payload}");
    assert!(payload.contains("parse() now returns Result"));
    assert!(
        !payload.starts_with("# 🤖 Agent Context: State Checkpoint"),
        "coding payload should not fall back to checkpoint prose when structured assembly succeeds: {payload}"
    );
    assert!(
        artifact
            .structural_refs
            .iter()
            .any(|node_id| node_id == "node:file:src/lib.rs"),
        "coding artifact should preserve structural refs for represented nodes: {:?}",
        artifact.structural_refs
    );
}

#[tokio::test]
async fn coding_compaction_payload_renders_offload_refs_from_metadata_fields() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.auto_compact_context = true;
    config.max_context_messages = 2;
    config.keep_recent_on_compact = 1;
    config.compaction.strategy = CompactionStrategy::Heuristic;
    let engine = AgentEngine::new_test(manager, config.clone(), root.path()).await;
    let provider = sample_provider_config();
    let thread_id = "coding-compaction-offload-metadata-thread";

    engine.thread_structural_memories.write().await.insert(
        thread_id.to_string(),
        crate::agent::context::structural_memory::ThreadStructuralMemory {
            workspace_seed_scan_complete: true,
            language_hints: vec!["rust".to_string()],
            workspace_seeds: vec![crate::agent::context::structural_memory::WorkspaceSeed {
                node_id: "node:file:Cargo.toml".to_string(),
                relative_path: "Cargo.toml".to_string(),
                kind: "cargo_manifest".to_string(),
            }],
            observed_files: vec![crate::agent::context::structural_memory::ObservedFileNode {
                node_id: "node:file:src/lib.rs".to_string(),
                relative_path: "src/lib.rs".to_string(),
            }],
            edges: Vec::new(),
        },
    );
    engine
        .history
        .upsert_offloaded_payload_metadata(
            "payload-1",
            thread_id,
            "read_file",
            Some("call-1"),
            "text/plain",
            2_048,
            "Captured parser notes\n- tool: write_file\n- bytes: 9999\n- payload_id: payload-from-summary\n- key findings:\n  - parse() now returns Result\n",
            1_717_170_000,
        )
        .await
        .expect("offloaded payload metadata should be stored");

    {
        let mut threads = engine.threads.write().await;
        threads.insert(
            thread_id.to_string(),
            sample_thread(vec![
                AgentMessage::user(
                    "Update the parser and keep the compacted context deterministic.",
                    1,
                ),
                AgentMessage {
                    id: "tool-older".to_string(),
                    role: MessageRole::Tool,
                    content: "Tool result offloaded".to_string(),
                    content_blocks: Vec::new(),
                    tool_calls: None,
                    tool_call_id: Some("call-1".to_string()),
                    tool_name: Some("read_file".to_string()),
                    tool_arguments: Some(
                        serde_json::json!({"filePath":"/repo/src/lib.rs"}).to_string(),
                    ),
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
                    message_kind: AgentMessageKind::Normal,
                    compaction_strategy: None,
                    compaction_payload: None,
                    offloaded_payload_id: Some("payload-1".to_string()),
                    tool_output_preview_path: None,
                    structural_refs: vec!["node:file:src/lib.rs".to_string()],
                    pinned_for_compaction: false,
                    timestamp: 2,
                },
                AgentMessage::user("Continue with the fix.", 3),
            ]),
        );
        let thread = threads.get_mut(thread_id).expect("thread should exist");
        thread.id = thread_id.to_string();
    }

    let inserted = engine
        .maybe_persist_compaction_artifact(thread_id, None, &config, &provider)
        .await
        .expect("coding compaction should succeed");
    assert!(inserted, "compaction artifact should be inserted");

    let thread = {
        let threads = engine.threads.read().await;
        threads
            .get(thread_id)
            .cloned()
            .expect("thread should exist after compaction")
    };
    let artifact = compaction_artifact_message(&thread);
    let payload = artifact
        .compaction_payload
        .as_deref()
        .expect("coding artifact should carry payload");

    assert!(
        payload.contains("`payload-1` (`read_file`, 2048 bytes)"),
        "offload references should use stable metadata fields: {payload}"
    );
    assert!(payload.contains("Captured parser notes"));
    assert!(payload.contains("parse() now returns Result"));
}

#[test]
fn coding_signals_without_structural_state_still_use_conversational_compaction() {
    let mode = determine_rule_based_compaction_mode(
        None,
        &[AgentMessage {
            id: "tool-result".to_string(),
            role: MessageRole::Tool,
            content: "cargo test failed in crates/zorai-daemon/src/agent/compaction.rs".to_string(),
            content_blocks: Vec::new(),
            tool_calls: None,
            tool_call_id: Some("call-1".to_string()),
            tool_name: Some("read_file".to_string()),
            tool_arguments: Some(
                serde_json::json!({"filePath":"/repo/crates/zorai-daemon/src/agent/compaction.rs"})
                    .to_string(),
            ),
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
            message_kind: AgentMessageKind::Normal,
            compaction_strategy: None,
            compaction_payload: None,
            offloaded_payload_id: None,
            tool_output_preview_path: None,
            structural_refs: Vec::new(),
            pinned_for_compaction: false,
            timestamp: 1,
        }],
    );

    assert_eq!(mode, RuleBasedCompactionMode::Conversational);
}

#[test]
fn stale_structural_state_without_recent_coding_signals_uses_conversational_compaction() {
    let mode = determine_rule_based_compaction_mode(
        Some(
            &crate::agent::context::structural_memory::ThreadStructuralMemory {
                workspace_seed_scan_complete: true,
                language_hints: vec!["rust".to_string()],
                workspace_seeds: vec![crate::agent::context::structural_memory::WorkspaceSeed {
                    node_id: "node:file:Cargo.toml".to_string(),
                    relative_path: "Cargo.toml".to_string(),
                    kind: "cargo_manifest".to_string(),
                }],
                observed_files: vec![crate::agent::context::structural_memory::ObservedFileNode {
                    node_id: "node:file:src/lib.rs".to_string(),
                    relative_path: "src/lib.rs".to_string(),
                }],
                edges: vec![crate::agent::context::structural_memory::StructuralEdge {
                    from: "node:file:Cargo.toml".to_string(),
                    to: "node:file:src".to_string(),
                    kind: "cargo_manifest_to_crate_path".to_string(),
                }],
            },
        ),
        &[
            AgentMessage::user("Please summarize our next meeting agenda.", 1),
            AgentMessage {
                id: "assistant-summary".to_string(),
                role: MessageRole::Assistant,
                content: "We already covered the budget update and owner list.".to_string(),
                content_blocks: Vec::new(),
                tool_calls: None,
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
                message_kind: AgentMessageKind::Normal,
                compaction_strategy: None,
                compaction_payload: None,
                offloaded_payload_id: None,
                tool_output_preview_path: None,
                structural_refs: Vec::new(),
                pinned_for_compaction: false,
                timestamp: 2,
            },
        ],
    );

    assert_eq!(mode, RuleBasedCompactionMode::Conversational);
}

#[tokio::test]
async fn conversational_compaction_still_uses_checkpoint_summary_path() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.auto_compact_context = true;
    config.max_context_messages = 2;
    config.keep_recent_on_compact = 1;
    config.compaction.strategy = CompactionStrategy::Heuristic;
    let engine = AgentEngine::new_test(manager, config.clone(), root.path()).await;
    let provider = sample_provider_config();
    let thread_id = "conversational-compaction-thread";

    {
        let mut threads = engine.threads.write().await;
        threads.insert(
            thread_id.to_string(),
            sample_thread(vec![
                AgentMessage::user("Help me summarize the meeting decisions.", 1),
                AgentMessage {
                    id: "assistant-older".to_string(),
                    role: MessageRole::Assistant,
                    content: "We already covered the budget and timeline updates.".to_string(),
                    content_blocks: Vec::new(),
                    tool_calls: None,
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
                    message_kind: AgentMessageKind::Normal,
                    compaction_strategy: None,
                    compaction_payload: None,
                    offloaded_payload_id: None,
                    tool_output_preview_path: None,
                    structural_refs: Vec::new(),
                    pinned_for_compaction: false,
                    timestamp: 2,
                },
                AgentMessage::user("Continue the summary for the next participant.", 3),
            ]),
        );
        let thread = threads.get_mut(thread_id).expect("thread should exist");
        thread.id = thread_id.to_string();
    }

    let inserted = engine
        .maybe_persist_compaction_artifact(thread_id, None, &config, &provider)
        .await
        .expect("conversational compaction should succeed");
    assert!(inserted, "compaction artifact should be inserted");

    let thread = {
        let threads = engine.threads.read().await;
        threads
            .get(thread_id)
            .cloned()
            .expect("thread should exist after compaction")
    };
    assert_eq!(thread.messages.len(), 4);
    assert_eq!(
        thread.messages[0].content,
        "Help me summarize the meeting decisions."
    );
    assert_eq!(
        thread.messages[1].content,
        "We already covered the budget and timeline updates."
    );
    let artifact = compaction_artifact_message(&thread);
    assert_eq!(
        thread.messages[2].message_kind,
        AgentMessageKind::CompactionArtifact
    );
    assert_eq!(
        thread.messages[3].content,
        "Continue the summary for the next participant."
    );
    let compacted = compact_messages_for_request(&thread.messages, &config, &provider);
    assert_eq!(compacted.len(), 2);
    assert_eq!(
        compacted[0].content,
        artifact
            .compaction_payload
            .clone()
            .expect("artifact should carry hidden payload")
    );
    assert_eq!(
        compacted[1].content,
        "Continue the summary for the next participant."
    );
    let payload = artifact
        .compaction_payload
        .as_deref()
        .expect("conversational artifact should carry payload");

    assert!(payload.starts_with("# 🤖 Agent Context: State Checkpoint"));
    assert!(payload.contains("## 🎯 Primary Objective"));
    assert!(payload.contains("## 🛠️ Recent Action Summary (Last 3-5 Turns)"));
    assert!(artifact.structural_refs.is_empty());
}

#[tokio::test]
async fn internal_dm_thread_uses_checkpoint_compaction_even_with_structural_state() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.auto_compact_context = true;
    config.max_context_messages = 2;
    config.keep_recent_on_compact = 1;
    config.compaction.strategy = CompactionStrategy::Heuristic;
    let engine = AgentEngine::new_test(manager, config.clone(), root.path()).await;
    let provider = sample_provider_config();
    let thread_id = crate::agent::agent_identity::internal_dm_thread_id(
        crate::agent::agent_identity::MAIN_AGENT_ID,
        crate::agent::agent_identity::WELES_AGENT_ID,
    );

    engine.thread_structural_memories.write().await.insert(
        thread_id.clone(),
        crate::agent::context::structural_memory::ThreadStructuralMemory {
            workspace_seed_scan_complete: true,
            language_hints: vec!["rust".to_string()],
            workspace_seeds: vec![crate::agent::context::structural_memory::WorkspaceSeed {
                node_id: "node:file:Cargo.toml".to_string(),
                relative_path: "Cargo.toml".to_string(),
                kind: "cargo_manifest".to_string(),
            }],
            observed_files: vec![crate::agent::context::structural_memory::ObservedFileNode {
                node_id: "node:file:src/lib.rs".to_string(),
                relative_path: "src/lib.rs".to_string(),
            }],
            edges: vec![crate::agent::context::structural_memory::StructuralEdge {
                from: "node:file:src/lib.rs".to_string(),
                to: "node:file:src/parser.rs".to_string(),
                kind: "imported_file".to_string(),
            }],
        },
    );

    {
        let mut threads = engine.threads.write().await;
        threads.insert(
            thread_id.clone(),
            sample_thread(vec![
                AgentMessage::user("Review the recent governance exchange.", 1),
                AgentMessage {
                    id: "tool-older".to_string(),
                    role: MessageRole::Tool,
                    content: "Tool result offloaded".to_string(),
                    content_blocks: Vec::new(),
                    tool_calls: None,
                    tool_call_id: Some("call-1".to_string()),
                    tool_name: Some("read_file".to_string()),
                    tool_arguments: Some(
                        serde_json::json!({"filePath":"/repo/src/lib.rs"}).to_string(),
                    ),
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
                    message_kind: AgentMessageKind::Normal,
                    compaction_strategy: None,
                    compaction_payload: None,
                    offloaded_payload_id: Some("payload-1".to_string()),
                    tool_output_preview_path: None,
                    structural_refs: vec!["node:file:src/lib.rs".to_string()],
                    pinned_for_compaction: false,
                    timestamp: 2,
                },
                AgentMessage::user("Continue the discussion.", 3),
            ]),
        );
        let thread = threads.get_mut(&thread_id).expect("thread should exist");
        thread.id = thread_id.clone();
    }

    let inserted = engine
        .maybe_persist_compaction_artifact(&thread_id, None, &config, &provider)
        .await
        .expect("internal dm compaction should succeed");
    assert!(inserted);

    let thread = {
        let threads = engine.threads.read().await;
        threads
            .get(&thread_id)
            .cloned()
            .expect("thread should exist after compaction")
    };
    assert_eq!(thread.messages.len(), 4);
    assert_eq!(
        thread.messages[0].content,
        "Review the recent governance exchange."
    );
    assert_eq!(thread.messages[1].content, "Tool result offloaded");
    let artifact = compaction_artifact_message(&thread);
    assert_eq!(
        thread.messages[2].message_kind,
        AgentMessageKind::CompactionArtifact
    );
    assert_eq!(thread.messages[3].content, "Continue the discussion.");
    let compacted = compact_messages_for_request(&thread.messages, &config, &provider);
    assert_eq!(compacted.len(), 2);
    assert_eq!(
        compacted[0].content,
        artifact
            .compaction_payload
            .clone()
            .expect("artifact should carry hidden payload")
    );
    assert_eq!(compacted[1].content, "Continue the discussion.");
    let payload = artifact
        .compaction_payload
        .as_deref()
        .expect("artifact should carry payload");

    assert!(payload.starts_with("# 🤖 Agent Context: State Checkpoint"));
    assert!(artifact.structural_refs.is_empty());
}

#[tokio::test]
async fn coding_compaction_falls_back_to_checkpoint_summary_when_structured_assembly_fails() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.auto_compact_context = true;
    config.max_context_messages = 2;
    config.keep_recent_on_compact = 1;
    config.compaction.strategy = CompactionStrategy::Heuristic;
    let engine = AgentEngine::new_test(manager, config.clone(), root.path()).await;
    let provider = sample_provider_config();
    let thread_id = "coding-compaction-fallback-thread";
    let mut events = engine.subscribe();

    engine.thread_structural_memories.write().await.insert(
        thread_id.to_string(),
        crate::agent::context::structural_memory::ThreadStructuralMemory {
            workspace_seed_scan_complete: true,
            language_hints: vec!["rust".to_string()],
            workspace_seeds: Vec::new(),
            observed_files: vec![crate::agent::context::structural_memory::ObservedFileNode {
                node_id: "node:file:src/lib.rs".to_string(),
                relative_path: "src/lib.rs".to_string(),
            }],
            edges: Vec::new(),
        },
    );

    {
        let mut threads = engine.threads.write().await;
        threads.insert(
            thread_id.to_string(),
            sample_thread(vec![
                AgentMessage::user("Fix the parser implementation.", 1),
                AgentMessage {
                    id: "tool-older".to_string(),
                    role: MessageRole::Tool,
                    content: "Tool result offloaded".to_string(),
                    content_blocks: Vec::new(),
                    tool_calls: None,
                    tool_call_id: Some("call-1".to_string()),
                    tool_name: Some("read_file".to_string()),
                    tool_arguments: Some(
                        serde_json::json!({"filePath":"/repo/src/lib.rs"}).to_string(),
                    ),
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
                    message_kind: AgentMessageKind::Normal,
                    compaction_strategy: None,
                    compaction_payload: None,
                    offloaded_payload_id: Some("payload-missing-table".to_string()),
                    tool_output_preview_path: None,
                    structural_refs: vec!["node:file:src/lib.rs".to_string()],
                    pinned_for_compaction: false,
                    timestamp: 2,
                },
                AgentMessage::user("Continue with validation.", 3),
            ]),
        );
        let thread = threads.get_mut(thread_id).expect("thread should exist");
        thread.id = thread_id.to_string();
    }

    engine
        .history
        .conn
        .call(|conn| {
            conn.execute("DROP TABLE offloaded_payloads", [])?;
            Ok(())
        })
        .await
        .expect("drop offloaded payload metadata table");

    let inserted = engine
        .maybe_persist_compaction_artifact(thread_id, None, &config, &provider)
        .await
        .expect("fallback compaction should still succeed");
    assert!(inserted, "compaction artifact should be inserted");

    let thread = {
        let threads = engine.threads.read().await;
        threads
            .get(thread_id)
            .cloned()
            .expect("thread should exist after compaction")
    };
    let artifact = compaction_artifact_message(&thread);
    let payload = artifact
        .compaction_payload
        .as_deref()
        .expect("fallback artifact should carry payload");
    assert!(payload.starts_with("# 🤖 Agent Context: State Checkpoint"));

    let notices = collect_workflow_notice_messages(&mut events, thread_id);
    assert!(
        notices
            .iter()
            .any(|message| message.contains("Structured coding compaction failed")),
        "expected structured fallback notice, got {notices:?}"
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

mod structural_memory {
    use super::*;

    #[test]
    fn coding_thread_workspace_seed_detects_repo_manifests() {
        let root = tempdir().expect("tempdir");
        fs::create_dir_all(root.path().join("frontend/src")).expect("frontend src dir");
        fs::create_dir_all(root.path().join("scripts")).expect("scripts dir");
        fs::write(
            root.path().join("Cargo.toml"),
            "[workspace]\nmembers = []\n",
        )
        .expect("write Cargo.toml");
        fs::write(
            root.path().join("frontend/package.json"),
            "{\"name\":\"frontend\"}\n",
        )
        .expect("write package.json");
        fs::write(
            root.path().join("frontend/tsconfig.json"),
            "{\"compilerOptions\":{\"rootDir\":\"src\"}}\n",
        )
        .expect("write tsconfig.json");
        fs::write(
            root.path().join("pyproject.toml"),
            "[project]\nname='demo'\n",
        )
        .expect("write pyproject");
        fs::write(root.path().join("scripts/dev.sh"), "#!/usr/bin/env bash\n")
            .expect("write shell script");

        let memory =
            crate::agent::context::structural_memory::discover_workspace_seeds(root.path())
                .expect("workspace seed discovery should succeed");

        assert!(memory
            .workspace_seeds
            .iter()
            .any(|seed| seed.node_id == "node:file:Cargo.toml"));
        assert!(memory
            .workspace_seeds
            .iter()
            .any(|seed| seed.node_id == "node:file:frontend/package.json"));
        assert!(memory
            .workspace_seeds
            .iter()
            .any(|seed| seed.node_id == "node:file:frontend/tsconfig.json"));
        assert_eq!(
            memory.language_hints,
            vec![
                "python".to_string(),
                "rust".to_string(),
                "shell".to_string(),
                "typescript".to_string(),
            ]
        );
        assert!(memory.edges.iter().any(|edge| {
            edge.from == "node:file:Cargo.toml"
                && edge.to == "node:file:."
                && edge.kind == "crate_path"
        }));
        assert!(memory.edges.iter().any(|edge| {
            edge.from == "node:file:frontend/package.json"
                && edge.to == "node:file:frontend"
                && edge.kind == "package_root"
        }));
        assert!(memory.edges.iter().any(|edge| {
            edge.from == "node:file:frontend/tsconfig.json"
                && edge.to == "node:file:frontend/src"
                && edge.kind == "source_root"
        }));
    }
}

#[test]
fn compaction_candidate_keeps_unanswered_tool_turn_out_of_summary_boundary() {
    let mut config = AgentConfig::default();
    config.auto_compact_context = true;
    config.max_context_messages = 1;
    config.keep_recent_on_compact = 1;
    let provider = sample_provider_config();
    let messages = vec![
        AgentMessage::user("older", 1),
        AgentMessage {
            id: "assistant-tool-call".to_string(),
            role: MessageRole::Assistant,
            content: String::new(),
            content_blocks: Vec::new(),
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
            message_kind: AgentMessageKind::Normal,
            compaction_strategy: None,
            compaction_payload: None,
            offloaded_payload_id: None,
            tool_output_preview_path: None,
            structural_refs: Vec::new(),
            pinned_for_compaction: false,
            timestamp: 2,
        },
        AgentMessage::user("latest", 3),
    ];

    let candidate =
        compaction_candidate(&messages, &config, &provider).expect("candidate should exist");

    assert_eq!(candidate.split_at, 1);
}

#[test]
fn prepare_llm_request_repairs_hidden_tool_turn_after_compaction_artifact() {
    let mut config = AgentConfig::default();
    config.provider = PROVIDER_ID_GITHUB_COPILOT.to_string();
    config.auto_compact_context = false;

    let mut provider = sample_provider_config();
    provider.base_url = "https://api.githubcopilot.com".to_string();
    provider.model = "gpt-5.4".to_string();
    provider.auth_source = AuthSource::GithubCopilot;
    provider.api_transport = ApiTransport::Responses;
    provider.reasoning_effort = "high".to_string();

    let thread = sample_thread(vec![
        AgentMessage::user("older", 1),
        AgentMessage {
            id: "assistant-tool-call".to_string(),
            role: MessageRole::Assistant,
            content: String::new(),
            content_blocks: Vec::new(),
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
            cost: None,
            provider: Some(PROVIDER_ID_GITHUB_COPILOT.to_string()),
            model: Some("gpt-5.4".to_string()),
            api_transport: Some(ApiTransport::Responses),
            response_id: None,
            upstream_message: None,
            provider_final_result: None,
            author_agent_id: None,
            author_agent_name: None,
            reasoning: None,
            message_kind: AgentMessageKind::Normal,
            compaction_strategy: None,
            compaction_payload: None,
            offloaded_payload_id: None,
            tool_output_preview_path: None,
            structural_refs: Vec::new(),
            pinned_for_compaction: false,
            timestamp: 2,
        },
        AgentMessage {
            id: "compaction-1".to_string(),
            role: MessageRole::Assistant,
            content: "rule based".to_string(),
            content_blocks: Vec::new(),
            tool_calls: None,
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
            message_kind: AgentMessageKind::CompactionArtifact,
            compaction_strategy: Some(CompactionStrategy::Heuristic),
            compaction_payload: Some("Older context compacted for continuity".to_string()),
            offloaded_payload_id: None,
            tool_output_preview_path: None,
            structural_refs: Vec::new(),
            pinned_for_compaction: false,
            timestamp: 3,
        },
        AgentMessage {
            id: "tool-read".to_string(),
            role: MessageRole::Tool,
            content: "file contents".to_string(),
            content_blocks: Vec::new(),
            tool_calls: None,
            tool_call_id: Some("call_read".to_string()),
            tool_name: Some("read_file".to_string()),
            tool_arguments: Some("{\"path\":\"/tmp/spec.md\"}".to_string()),
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
            message_kind: AgentMessageKind::Normal,
            compaction_strategy: None,
            compaction_payload: None,
            offloaded_payload_id: None,
            tool_output_preview_path: None,
            structural_refs: Vec::new(),
            pinned_for_compaction: false,
            timestamp: 4,
        },
        AgentMessage::user("continue", 5),
    ]);

    let prepared = prepare_llm_request(&thread, &config, &provider);

    assert_eq!(prepared.transport, ApiTransport::Responses);
    assert!(prepared.previous_response_id.is_none());
    assert_eq!(prepared.messages.len(), 4);
    assert_eq!(prepared.messages[0].role, "assistant");
    assert_eq!(prepared.messages[1].role, "assistant");
    assert_eq!(
        prepared.messages[1]
            .tool_calls
            .as_ref()
            .and_then(|tool_calls| tool_calls.first())
            .map(|tool_call| tool_call.id.as_str()),
        Some("call_read")
    );
    assert_eq!(prepared.messages[2].role, "tool");
    assert_eq!(
        prepared.messages[2].tool_call_id.as_deref(),
        Some("call_read")
    );
    assert_eq!(prepared.messages[3].role, "user");
}

#[tokio::test]
async fn coding_compaction_payload_includes_memory_graph_neighbors() {
    let root = tempdir().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.auto_compact_context = true;
    config.max_context_messages = 2;
    config.keep_recent_on_compact = 1;
    config.compaction.strategy = CompactionStrategy::Heuristic;
    let engine = AgentEngine::new_test(manager, config.clone(), root.path()).await;
    let provider = sample_provider_config();
    let thread_id = "coding-compaction-graph-neighbors-thread";

    engine.thread_structural_memories.write().await.insert(
        thread_id.to_string(),
        crate::agent::context::structural_memory::ThreadStructuralMemory {
            workspace_seed_scan_complete: true,
            language_hints: vec!["rust".to_string()],
            workspace_seeds: vec![crate::agent::context::structural_memory::WorkspaceSeed {
                node_id: "node:file:Cargo.toml".to_string(),
                relative_path: "Cargo.toml".to_string(),
                kind: "cargo_manifest".to_string(),
            }],
            observed_files: vec![crate::agent::context::structural_memory::ObservedFileNode {
                node_id: "node:file:src/lib.rs".to_string(),
                relative_path: "src/lib.rs".to_string(),
            }],
            edges: Vec::new(),
        },
    );
    engine
        .history
        .upsert_memory_node(
            "node:file:src/lib.rs",
            "src/lib.rs",
            "file",
            Some("observed file"),
            1_717_180_201,
        )
        .await
        .expect("persist file node");
    engine
        .history
        .upsert_memory_node(
            "node:package:cargo:demo",
            "demo",
            "package",
            Some("cargo package from Cargo.toml"),
            1_717_180_202,
        )
        .await
        .expect("persist package node");
    engine
        .history
        .upsert_memory_edge(
            "node:file:src/lib.rs",
            "node:package:cargo:demo",
            "file_in_package",
            2.0,
            1_717_180_203,
        )
        .await
        .expect("persist file/package edge");

    {
        let mut threads = engine.threads.write().await;
        threads.insert(
            thread_id.to_string(),
            sample_thread(vec![
                AgentMessage::user("Update the parser and keep code context compact.", 1),
                AgentMessage {
                    id: "tool-older".to_string(),
                    role: MessageRole::Tool,
                    content: "Tool result offloaded".to_string(),
                    content_blocks: Vec::new(),
                    tool_calls: None,
                    tool_call_id: Some("call-1".to_string()),
                    tool_name: Some("read_file".to_string()),
                    tool_arguments: Some(
                        serde_json::json!({"filePath":"/repo/src/lib.rs"}).to_string(),
                    ),
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
                    message_kind: AgentMessageKind::Normal,
                    compaction_strategy: None,
                    compaction_payload: None,
                    offloaded_payload_id: None,
                    tool_output_preview_path: None,
                    structural_refs: vec!["node:file:src/lib.rs".to_string()],
                    pinned_for_compaction: false,
                    timestamp: 2,
                },
                AgentMessage::user("Continue with the fix and validate it.", 3),
            ]),
        );
        let thread = threads.get_mut(thread_id).expect("thread should exist");
        thread.id = thread_id.to_string();
    }

    let inserted = engine
        .maybe_persist_compaction_artifact(thread_id, None, &config, &provider)
        .await
        .expect("coding compaction should succeed");
    assert!(inserted, "compaction artifact should be inserted");

    let thread = {
        let threads = engine.threads.read().await;
        threads
            .get(thread_id)
            .cloned()
            .expect("thread should exist after compaction")
    };
    let artifact = compaction_artifact_message(&thread);
    let payload = artifact
        .compaction_payload
        .as_deref()
        .expect("coding artifact should carry payload");

    assert!(payload.contains("node:package:cargo:demo"));
    assert!(payload.contains("graph neighbor `demo` (package) via file in package"));
    assert!(artifact
        .structural_refs
        .iter()
        .any(|node_id| node_id == "node:package:cargo:demo"));
}
