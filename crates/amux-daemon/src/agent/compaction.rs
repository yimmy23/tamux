#![allow(dead_code)]

//! Context compaction — token-aware message compression for LLM requests.

use super::llm_client::{messages_to_api_format, ApiToolCall, ApiToolCallFunction};
use super::*;
use crate::agent::context::structural_memory::{StructuralContextEntry, ThreadStructuralMemory};
use amux_shared::providers::{PROVIDER_ID_GITHUB_COPILOT, PROVIDER_ID_OPENAI};

const HEURISTIC_COMPACTION_VISIBLE_TEXT: &str = "rule based";
const COMPACTION_NOTICE_KIND: &str = "auto-compaction";
const MANUAL_COMPACTION_NOTICE_KIND: &str = "manual-compaction";
const COMPACTION_EXACT_MESSAGE_MAX: usize = 24;
const COMPACTION_MODEL_RECENT_CONTENT_MESSAGES: usize = 6;
const COMPACTION_MODEL_REQUEST_HEADROOM_TOKENS: usize = 8_192;
const COMPACTION_RECENT_SIGNAL_MESSAGES: usize = 8;
const CODING_COMPACTION_STRUCTURAL_ENTRY_LIMIT: usize = 6;
const CODING_COMPACTION_OFFLOAD_REFERENCE_LIMIT: usize = 4;
const COMPACTION_MESSAGE_TRUNCATION_NOTICE: &str =
    "\n\n[Older compaction input truncated to fit the model budget.]";
const COMPACTION_MODEL_SYSTEM_PROMPT: &str = "You compress older conversation context into a deterministic execution checkpoint for future continuity. Follow the mandatory thread compaction protocol exactly. Preserve goals, constraints, decisions, tool outcomes, unresolved issues, failed paths, and the immediate next step. Return exactly one markdown block matching the required schema. Do not add commentary outside the schema.";
const COMPACTION_CHECKPOINT_SCHEMA: &str = r#"# 🤖 Agent Context: State Checkpoint

## 🎯 Primary Objective
> [1-2 sentences strictly defining the end goal.]

## 🗺️ Execution Map
* **✅ Completed Phase:** [...]
* **⏳ Current Phase:** [...]
* **⏭️ Pending Phases:** [...]

## 📁 Working Environment State
* **Active Directory:** `...`
* **Files Modified (Uncommitted/Pending):**
    * `...` - (...)
* **Read-Only Context Files:**
    * `...` - (...)

## 🧠 Acquired Knowledge & Constraints
* [...]

## 🚫 Dead Ends & Resolved Errors
* **Failed:** [...]
    * *Resolution:* [...]

## 🛠️ Recent Action Summary (Last 3-5 Turns)
1.  `tool_or_step(...)` -> [...]

## 🎯 Immediate Next Step
[Strict single-action instruction]
"#;
const COMPACTION_UNKNOWN_DIRECTORY: &str = "unknown (not captured in older context)";
const COMPACTION_NO_FILES_CAPTURED: &str =
    "* `none` - (No explicit file edits were captured in the compacted slice.)\n";
const COMPACTION_NO_READONLY_CAPTURED: &str =
    "* `none` - (No explicit reference files were captured in the compacted slice.)\n";
const COMPACTION_NO_DEAD_ENDS_CAPTURED: &str = "* **Failed:** No earlier failed path was preserved in this compacted slice.\n    * *Resolution:* Continue from the retained recent context instead of re-expanding discarded history.\n";
const CODING_COMPACTION_FALLBACK_NOTICE: &str =
    "Structured coding compaction failed; fell back to checkpoint summary.";

pub(super) struct PreparedLlmRequest {
    pub messages: Vec<ApiMessage>,
    pub transport: ApiTransport,
    pub previous_response_id: Option<String>,
    pub upstream_thread_id: Option<String>,
    pub force_connection_close: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct CompactionCandidate {
    pub split_at: usize,
    pub target_tokens: usize,
    pub trigger: CompactionTrigger,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum CompactionTrigger {
    MessageCount,
    TokenThreshold,
    MessageCountAndTokenThreshold,
    ManualRequest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CompactionCandidateMode {
    Automatic,
    Forced,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RuleBasedCompactionMode {
    Conversational,
    Coding,
}

struct RuleBasedCompactionPayload {
    payload: String,
    structural_refs: Vec<String>,
    fallback_notice: Option<String>,
}

pub(super) fn message_is_compaction_summary(message: &AgentMessage) -> bool {
    message.message_kind == AgentMessageKind::CompactionArtifact
        || message.content.starts_with("[Compacted earlier context]")
}

fn latest_compaction_artifact_index(messages: &[AgentMessage]) -> Option<usize> {
    messages.iter().rposition(message_is_compaction_summary)
}

pub(super) fn active_compaction_window(messages: &[AgentMessage]) -> (usize, &[AgentMessage]) {
    match latest_compaction_artifact_index(messages) {
        Some(index) => (index, &messages[index..]),
        None => (0, messages),
    }
}

fn compaction_runtime_content<'a>(message: &'a AgentMessage) -> &'a str {
    if message_is_compaction_summary(message) {
        message
            .compaction_payload
            .as_deref()
            .filter(|payload| !payload.trim().is_empty())
            .unwrap_or_else(|| message.content.as_str())
    } else {
        message.content.as_str()
    }
}

fn materialize_compaction_message(message: &AgentMessage) -> AgentMessage {
    let mut materialized = message.clone();
    materialized.content = compaction_runtime_content(message).to_string();
    materialized
}

fn trailing_dangling_tool_turn_start(messages: &[AgentMessage]) -> Option<usize> {
    let (assistant_index, assistant_message) =
        messages.iter().enumerate().rev().find(|(_, message)| {
            message.role == MessageRole::Assistant
                && message
                    .tool_calls
                    .as_ref()
                    .is_some_and(|tool_calls| !tool_calls.is_empty())
        })?;

    let tool_calls = assistant_message.tool_calls.as_ref()?;
    let expected_ids: std::collections::HashSet<&str> = tool_calls
        .iter()
        .map(|tool_call| tool_call.id.as_str())
        .collect();
    if expected_ids.is_empty() {
        return None;
    }

    let trailing = &messages[assistant_index + 1..];
    if trailing
        .iter()
        .any(|message| message.role != MessageRole::Tool)
    {
        return None;
    }

    let matched_ids: std::collections::HashSet<&str> = trailing
        .iter()
        .filter_map(|message| message.tool_call_id.as_deref())
        .filter(|tool_call_id| expected_ids.contains(*tool_call_id))
        .collect();

    if !trailing.is_empty() && matched_ids.len() == expected_ids.len() {
        None
    } else {
        Some(assistant_index)
    }
}

fn hidden_dangling_tool_turn(messages: &[AgentMessage], window_start: usize) -> Vec<AgentMessage> {
    if window_start == 0 {
        return Vec::new();
    }

    let hidden_messages = &messages[..window_start];
    let Some(start) = trailing_dangling_tool_turn_start(hidden_messages) else {
        return Vec::new();
    };

    hidden_messages[start..]
        .iter()
        .map(materialize_compaction_message)
        .collect()
}

fn active_request_messages(messages: &[AgentMessage]) -> Vec<AgentMessage> {
    let (window_start, active_messages) = active_compaction_window(messages);
    let repaired_hidden_turn = hidden_dangling_tool_turn(messages, window_start);

    if repaired_hidden_turn.is_empty() {
        return active_messages
            .iter()
            .map(materialize_compaction_message)
            .collect();
    }

    let mut active_iter = active_messages.iter();
    let mut request_messages = Vec::new();

    if let Some(first_message) = active_iter.next() {
        request_messages.push(materialize_compaction_message(first_message));
    }

    request_messages.extend(repaired_hidden_turn);
    request_messages.extend(active_iter.map(materialize_compaction_message));
    request_messages
}

pub(super) fn prepare_llm_request(
    thread: &AgentThread,
    config: &AgentConfig,
    provider_config: &ProviderConfig,
) -> PreparedLlmRequest {
    prepare_llm_request_with_reused_user_message(thread, config, provider_config, None)
}

pub(super) fn prepare_llm_request_with_reused_user_message(
    thread: &AgentThread,
    config: &AgentConfig,
    provider_config: &ProviderConfig,
    reused_user_message: Option<&str>,
) -> PreparedLlmRequest {
    let mut selected_transport =
        if provider_supports_transport(&config.provider, provider_config.api_transport) {
            provider_config.api_transport
        } else {
            default_api_transport_for_provider(&config.provider)
        };
    if config.provider == PROVIDER_ID_OPENAI
        && provider_config.auth_source == crate::agent::types::AuthSource::ChatgptSubscription
    {
        selected_transport = ApiTransport::Responses;
    }
    let messages = &thread.messages;
    let compacted = compact_messages_for_request(messages, config, provider_config);
    let compaction_active =
        compacted.len() != messages.len() || compacted.iter().any(message_is_compaction_summary);

    if !compaction_active
        && selected_transport == ApiTransport::NativeAssistant
        && !provider_config.assistant_id.trim().is_empty()
    {
        let latest_user_message = messages
            .iter()
            .rev()
            .find(|message| message.role == MessageRole::User)
            .cloned();
        if let Some(user_message) = latest_user_message {
            return PreparedLlmRequest {
                messages: messages_to_api_format(&[user_message]),
                transport: ApiTransport::NativeAssistant,
                previous_response_id: None,
                upstream_thread_id: if thread.upstream_transport
                    == Some(ApiTransport::NativeAssistant)
                    && thread.upstream_provider.as_deref() == Some(config.provider.as_str())
                    && thread.upstream_model.as_deref() == Some(provider_config.model.as_str())
                    && thread.upstream_assistant_id.as_deref()
                        == Some(provider_config.assistant_id.as_str())
                {
                    thread.upstream_thread_id.clone()
                } else {
                    None
                },
                force_connection_close: false,
            };
        }
    }

    if selected_transport == ApiTransport::Responses {
        let previous_response_id =
            if !compaction_active && supports_response_continuity(&config.provider) {
                messages
                    .iter()
                    .enumerate()
                    .rev()
                    .find(|(_, message)| {
                        message.role == MessageRole::Assistant
                            && message.response_id.is_some()
                            && message.provider.as_deref() == Some(config.provider.as_str())
                            && message.model.as_deref() == Some(provider_config.model.as_str())
                            && message.api_transport == Some(ApiTransport::Responses)
                    })
                    .and_then(|(anchor_index, anchor_message)| {
                        let trailing = &messages[anchor_index + 1..];
                        if config.provider == PROVIDER_ID_GITHUB_COPILOT
                            && trailing.iter().any(|message| {
                                message.role == MessageRole::Tool
                                    || message
                                        .tool_calls
                                        .as_ref()
                                        .is_some_and(|tool_calls| !tool_calls.is_empty())
                            })
                        {
                            return None;
                        }
                        let trailing_messages =
                            continuation_api_messages(trailing, reused_user_message);
                        if trailing_messages.is_empty() {
                            None
                        } else {
                            Some((trailing_messages, anchor_message.response_id.clone()))
                        }
                    })
            } else {
                None
            };

        if let Some((messages, previous_response_id)) = previous_response_id {
            let mut messages = messages;
            inject_reused_user_message_if_missing(&mut messages, reused_user_message);
            return PreparedLlmRequest {
                messages,
                transport: ApiTransport::Responses,
                previous_response_id,
                upstream_thread_id: None,
                force_connection_close: false,
            };
        }

        return PreparedLlmRequest {
            messages: messages_to_api_format(&compacted),
            transport: ApiTransport::Responses,
            previous_response_id: None,
            upstream_thread_id: None,
            force_connection_close: false,
        };
    }

    PreparedLlmRequest {
        messages: messages_to_api_format(&compacted),
        transport: ApiTransport::ChatCompletions,
        previous_response_id: None,
        upstream_thread_id: None,
        force_connection_close: false,
    }
}

fn inject_reused_user_message_if_missing(
    messages: &mut Vec<ApiMessage>,
    reused_user_message: Option<&str>,
) {
    if messages.iter().any(|message| message.role == "user") {
        return;
    }

    let Some(reused_user_message) = reused_user_message
        .map(str::trim)
        .filter(|message| !message.is_empty())
    else {
        return;
    };

    messages.push(ApiMessage {
        role: "user".to_string(),
        content: ApiContent::Text(reused_user_message.to_string()),
        tool_call_id: None,
        name: None,
        tool_calls: None,
    });
}

fn continuation_api_messages(
    messages: &[AgentMessage],
    reused_user_message: Option<&str>,
) -> Vec<ApiMessage> {
    if reused_user_message.is_none() {
        return messages_to_api_format(messages);
    }

    let mut pending_tool_results = std::collections::VecDeque::new();
    let mut api_messages = messages
        .iter()
        .filter(|message| {
            matches!(
                message.role,
                MessageRole::System
                    | MessageRole::User
                    | MessageRole::Assistant
                    | MessageRole::Tool
            )
        })
        .filter_map(|message| {
            let mut normalized_tool_calls = None;
            if message.role == MessageRole::Assistant {
                pending_tool_results.clear();
                if let Some(tool_calls) = &message.tool_calls {
                    let normalized: Vec<ApiToolCall> = tool_calls
                        .iter()
                        .enumerate()
                        .map(|(index, tool_call)| {
                            let normalized_id = if tool_call.id.trim().is_empty() {
                                format!(
                                    "synthetic_tool_call_{}_{}_{}",
                                    message.timestamp, index, tool_call.function.name
                                )
                            } else {
                                tool_call.id.clone()
                            };
                            pending_tool_results.push_back(normalized_id.clone());
                            ApiToolCall {
                                id: normalized_id,
                                call_type: "function".into(),
                                function: ApiToolCallFunction {
                                    name: tool_call.function.name.clone(),
                                    arguments: tool_call.function.arguments.clone(),
                                },
                            }
                        })
                        .collect();
                    normalized_tool_calls = Some(normalized);
                }
            }

            let mut normalized_tool_call_id = message.tool_call_id.clone();
            if message.role == MessageRole::Tool {
                let resolved_tool_call_id = if let Some(tool_call_id) = message
                    .tool_call_id
                    .as_ref()
                    .filter(|value| !value.trim().is_empty())
                {
                    let position = pending_tool_results
                        .iter()
                        .position(|pending_id| pending_id == tool_call_id);
                    if let Some(position) = position {
                        pending_tool_results.remove(position).unwrap_or_default()
                    } else {
                        tool_call_id.clone()
                    }
                } else {
                    let Some(next_pending) = pending_tool_results.pop_front() else {
                        return None;
                    };
                    next_pending
                };

                normalized_tool_call_id = Some(resolved_tool_call_id);
                if normalized_tool_call_id.as_deref().is_none() {
                    return None;
                }
            } else if message.role != MessageRole::Assistant && !pending_tool_results.is_empty() {
                pending_tool_results.clear();
            }

            Some(ApiMessage {
                role: match message.role {
                    MessageRole::System => "system".into(),
                    MessageRole::User => "user".into(),
                    MessageRole::Assistant => "assistant".into(),
                    MessageRole::Tool => "tool".into(),
                },
                content: ApiContent::Text(message.content.clone()),
                tool_call_id: normalized_tool_call_id,
                name: message.tool_name.clone(),
                tool_calls: normalized_tool_calls.or_else(|| {
                    message.tool_calls.as_ref().map(|tool_calls| {
                        tool_calls
                            .iter()
                            .map(|tool_call| ApiToolCall {
                                id: tool_call.id.clone(),
                                call_type: "function".into(),
                                function: ApiToolCallFunction {
                                    name: tool_call.function.name.clone(),
                                    arguments: tool_call.function.arguments.clone(),
                                },
                            })
                            .collect()
                    })
                }),
            })
        })
        .collect::<Vec<_>>();

    inject_reused_user_message_if_missing(&mut api_messages, reused_user_message);
    api_messages
}

pub(super) fn compact_messages_for_request(
    messages: &[AgentMessage],
    config: &AgentConfig,
    provider_config: &ProviderConfig,
) -> Vec<AgentMessage> {
    let runtime_messages = active_request_messages(messages);
    let Some(candidate) = compaction_candidate(&runtime_messages, config, provider_config) else {
        // Even when compaction is disabled, enforce a hard token limit
        // so we never exceed the model's context window.
        let model_window = model_context_window(
            &config.provider,
            &provider_config.model,
            provider_config
                .context_window_tokens
                .max(config.context_window_tokens),
        ) as usize;
        let current = estimate_message_tokens(&runtime_messages);
        if current > model_window {
            return hard_truncate_to_fit(&runtime_messages, model_window);
        }
        return runtime_messages;
    };
    let max_messages = config.max_context_messages.max(1) as usize;
    let split_at = candidate.split_at;
    let mut compacted = Vec::new();
    let mut has_summary = false;

    if split_at > 0 {
        let summary =
            build_compaction_summary(&runtime_messages[..split_at], candidate.target_tokens);
        if !summary.is_empty() {
            has_summary = true;
            compacted.push(AgentMessage {
                id: generate_message_id(),
                role: MessageRole::Assistant,
                content: summary,
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
                structural_refs: Vec::new(),
                timestamp: messages[split_at - 1].timestamp,
            });
        }
    }

    compacted.extend(runtime_messages[split_at..].iter().cloned());
    trim_compacted_messages(
        &mut compacted,
        max_messages,
        candidate.target_tokens,
        has_summary,
    );
    compacted
}

pub(super) fn compaction_candidate(
    messages: &[AgentMessage],
    config: &AgentConfig,
    provider_config: &ProviderConfig,
) -> Option<CompactionCandidate> {
    compaction_candidate_with_mode(
        messages,
        config,
        provider_config,
        CompactionCandidateMode::Automatic,
    )
}

pub(super) fn forced_compaction_candidate(
    messages: &[AgentMessage],
    config: &AgentConfig,
    provider_config: &ProviderConfig,
) -> Option<CompactionCandidate> {
    compaction_candidate_with_mode(
        messages,
        config,
        provider_config,
        CompactionCandidateMode::Forced,
    )
}

fn compaction_candidate_with_mode(
    messages: &[AgentMessage],
    config: &AgentConfig,
    provider_config: &ProviderConfig,
    mode: CompactionCandidateMode,
) -> Option<CompactionCandidate> {
    let (_, active_messages) = active_compaction_window(messages);
    if active_messages.is_empty()
        || (mode == CompactionCandidateMode::Automatic && !config.auto_compact_context)
    {
        return None;
    }

    let max_messages = config.max_context_messages.max(1) as usize;
    let target_tokens = effective_context_target_tokens(config, provider_config);
    let trigger = match mode {
        CompactionCandidateMode::Forced => CompactionTrigger::ManualRequest,
        CompactionCandidateMode::Automatic => {
            let over_message_limit = config.compaction.strategy == CompactionStrategy::Heuristic
                && active_messages.len() > max_messages;
            let over_token_limit = estimate_message_tokens(active_messages) > target_tokens;
            match (over_message_limit, over_token_limit) {
                (false, false) => return None,
                (true, false) => CompactionTrigger::MessageCount,
                (false, true) => CompactionTrigger::TokenThreshold,
                (true, true) => CompactionTrigger::MessageCountAndTokenThreshold,
            }
        }
    };

    let keep_recent = config
        .keep_recent_on_compact
        .max(1)
        .min(active_messages.len() as u32) as usize;
    let mut split_at = active_messages.len().saturating_sub(keep_recent);
    if split_at == 0 {
        return None;
    }

    // Never split inside a tool-call / tool-result pair.
    // If the first kept message is a tool result, move split_at back to include
    // the assistant message that made the tool call.
    while split_at > 0 && active_messages[split_at].role == MessageRole::Tool {
        split_at -= 1;
    }

    while split_at > 0 {
        let Some(dangling_start) = trailing_dangling_tool_turn_start(&active_messages[..split_at])
        else {
            break;
        };
        split_at = dangling_start;
    }

    if split_at == 0 {
        return None;
    }

    Some(CompactionCandidate {
        split_at,
        target_tokens,
        trigger,
    })
}

fn trim_compacted_messages(
    messages: &mut Vec<AgentMessage>,
    max_messages: usize,
    target_tokens: usize,
    has_summary: bool,
) {
    let removable_floor = if has_summary { 2 } else { 1 };
    let mut total_tokens = estimate_message_tokens(messages);
    while (messages.len() > max_messages || total_tokens > target_tokens)
        && messages.len() > removable_floor
    {
        let remove_index = if has_summary { 1 } else { 0 };

        // Don't remove an assistant message if the next message is a tool
        // result — that would orphan the tool result.
        if remove_index < messages.len()
            && messages[remove_index].role == MessageRole::Assistant
            && messages[remove_index].tool_calls.is_some()
        {
            // Remove the entire tool-call/result group together.
            let mut end = remove_index + 1;
            while end < messages.len() && messages[end].role == MessageRole::Tool {
                end += 1;
            }
            for i in (remove_index..end).rev() {
                total_tokens -= estimate_single_message_tokens(&messages[i]);
                messages.remove(i);
            }
        } else {
            total_tokens -= estimate_single_message_tokens(&messages[remove_index]);
            messages.remove(remove_index);
        }
    }

    if has_summary
        && messages.len() > 1
        && (messages.len() > max_messages || total_tokens > target_tokens)
    {
        messages.remove(0);
    }
}

pub(super) fn effective_context_target_tokens(
    config: &AgentConfig,
    provider_config: &ProviderConfig,
) -> usize {
    let primary_context_window = primary_context_window_tokens(config, provider_config);
    let threshold_pct = config.compact_threshold_pct.clamp(1, 100) as usize;
    let primary_target = primary_context_window.saturating_mul(threshold_pct) / 100;
    let strategy_target_cap =
        strategy_target_cap_tokens(config, primary_context_window as u32, threshold_pct as u32)
            .unwrap_or(primary_target);

    primary_target
        .min(strategy_target_cap)
        .max(MIN_CONTEXT_TARGET_TOKENS)
}

fn primary_context_window_tokens(config: &AgentConfig, provider_config: &ProviderConfig) -> usize {
    provider_config
        .context_window_tokens
        .max(config.context_window_tokens)
        .max(1) as usize
}

fn effective_compaction_window_tokens(
    config: &AgentConfig,
    provider_config: &ProviderConfig,
) -> usize {
    let primary_context_window = primary_context_window_tokens(config, provider_config);
    match config.compaction.strategy {
        CompactionStrategy::Heuristic => primary_context_window,
        CompactionStrategy::Weles => {
            let (provider_id, model_id) = resolved_weles_compaction_model(config);
            model_context_window(&provider_id, &model_id, primary_context_window as u32) as usize
        }
        CompactionStrategy::CustomModel => {
            config.compaction.custom_model.context_window_tokens.max(1) as usize
        }
    }
    .min(primary_context_window)
    .max(1)
}

fn format_token_count(value: usize) -> String {
    let digits = value.to_string();
    let mut formatted = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, ch) in digits.chars().rev().enumerate() {
        if index > 0 && index % 3 == 0 {
            formatted.push(',');
        }
        formatted.push(ch);
    }
    formatted.chars().rev().collect()
}

fn compaction_visible_strategy_label(strategy: CompactionStrategy) -> &'static str {
    match strategy {
        CompactionStrategy::Heuristic => HEURISTIC_COMPACTION_VISIBLE_TEXT,
        CompactionStrategy::Weles => "model generated",
        CompactionStrategy::CustomModel => "custom model generated",
    }
}

fn compaction_visible_trigger_label(trigger: CompactionTrigger) -> &'static str {
    match trigger {
        CompactionTrigger::MessageCount => "message-count",
        CompactionTrigger::TokenThreshold => "token-threshold",
        CompactionTrigger::MessageCountAndTokenThreshold => "message-count + token-threshold",
        CompactionTrigger::ManualRequest => "manual-request",
    }
}

fn compaction_trigger_detail_value(trigger: CompactionTrigger) -> &'static str {
    match trigger {
        CompactionTrigger::MessageCount => "message_count",
        CompactionTrigger::TokenThreshold => "token_threshold",
        CompactionTrigger::MessageCountAndTokenThreshold => "message_count_and_token_threshold",
        CompactionTrigger::ManualRequest => "manual_request",
    }
}

fn build_compaction_visible_content(
    pre_compaction_total_tokens: usize,
    effective_context_window_tokens: usize,
    target_tokens: usize,
    trigger: CompactionTrigger,
    strategy_used: CompactionStrategy,
    payload: &str,
) -> String {
    let header = format!(
        "Pre-compaction context: ~{} / {} tokens (threshold {})\nTrigger: {}\nStrategy: {}",
        format_token_count(pre_compaction_total_tokens),
        format_token_count(effective_context_window_tokens),
        format_token_count(target_tokens),
        compaction_visible_trigger_label(trigger),
        compaction_visible_strategy_label(strategy_used),
    );

    if payload.trim().is_empty() {
        header
    } else {
        format!("{header}\n\nContent:\n{payload}")
    }
}

fn strategy_target_cap_tokens(
    config: &AgentConfig,
    primary_context_window: u32,
    threshold_pct: u32,
) -> Option<usize> {
    let threshold_pct = threshold_pct.clamp(1, 100) as usize;
    match config.compaction.strategy {
        CompactionStrategy::Heuristic => None,
        CompactionStrategy::Weles => {
            let (provider_id, model_id) = resolved_weles_compaction_model(config);
            Some(
                model_context_window(&provider_id, &model_id, primary_context_window) as usize
                    * threshold_pct
                    / 100,
            )
        }
        CompactionStrategy::CustomModel => Some(
            config.compaction.custom_model.context_window_tokens.max(1) as usize * threshold_pct
                / 100,
        ),
    }
}

fn resolved_weles_compaction_model(config: &AgentConfig) -> (String, String) {
    let provider_id = config.compaction.weles.provider.trim().to_string();
    let provider_id = if provider_id.is_empty() {
        config
            .builtin_sub_agents
            .weles
            .provider
            .clone()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| config.provider.clone())
    } else {
        provider_id
    };

    let model_id = config.compaction.weles.model.trim().to_string();
    let model_id = if model_id.is_empty() {
        config
            .builtin_sub_agents
            .weles
            .model
            .clone()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| config.model.clone())
    } else {
        model_id
    };

    (provider_id, model_id)
}

pub(super) fn estimate_message_tokens(messages: &[AgentMessage]) -> usize {
    messages.iter().map(estimate_single_message_tokens).sum()
}

pub(super) fn estimate_single_message_tokens(message: &AgentMessage) -> usize {
    let mut chars = compaction_runtime_content(message).chars().count();

    if let Some(tool_calls) = &message.tool_calls {
        chars += tool_calls
            .iter()
            .map(|call| {
                call.function.name.chars().count() + call.function.arguments.chars().count()
            })
            .sum::<usize>();
    }

    chars += message
        .tool_name
        .as_deref()
        .map(str::chars)
        .map(Iterator::count)
        .unwrap_or(0);
    chars += message
        .tool_arguments
        .as_deref()
        .map(str::chars)
        .map(Iterator::count)
        .unwrap_or(0);

    chars.div_ceil(APPROX_CHARS_PER_TOKEN) + 12
}

fn build_checkpoint_compaction_payload(messages: &[AgentMessage], target_tokens: usize) -> String {
    let summary = build_compaction_summary(messages, target_tokens);
    if summary.trim().is_empty() {
        "Older context compacted for continuity.".to_string()
    } else {
        summary
    }
}

fn determine_rule_based_compaction_mode(
    structural_memory: Option<&ThreadStructuralMemory>,
    messages: &[AgentMessage],
) -> RuleBasedCompactionMode {
    if structural_memory.is_none_or(|memory| !memory.has_structural_nodes()) {
        return RuleBasedCompactionMode::Conversational;
    }

    let recent_messages = messages
        .iter()
        .rev()
        .take(COMPACTION_RECENT_SIGNAL_MESSAGES);
    for message in recent_messages {
        if message_uses_coding_tool(message) || message_contains_coding_signal(message) {
            return RuleBasedCompactionMode::Coding;
        }
    }

    RuleBasedCompactionMode::Conversational
}

fn message_uses_coding_tool(message: &AgentMessage) -> bool {
    message
        .tool_name
        .as_deref()
        .is_some_and(is_coding_tool_name)
        || message.tool_calls.as_ref().is_some_and(|tool_calls| {
            tool_calls
                .iter()
                .any(|call| is_coding_tool_name(call.function.name.as_str()))
        })
}

fn is_coding_tool_name(tool_name: &str) -> bool {
    matches!(
        tool_name,
        "read_file"
            | "replace_in_file"
            | "apply_patch"
            | "create_file"
            | "list_files"
            | "list_dir"
            | "write_file"
            | "append_to_file"
            | "apply_file_patch"
    )
}

fn message_contains_coding_signal(message: &AgentMessage) -> bool {
    text_contains_coding_signal(compaction_runtime_content(message))
        || message
            .tool_arguments
            .as_deref()
            .is_some_and(text_contains_coding_signal)
}

fn text_contains_coding_signal(text: &str) -> bool {
    let lowered = text.to_ascii_lowercase();
    if text.contains("```")
        || text.contains("*** Begin Patch")
        || text.contains("diff --git")
        || text.contains("\n@@")
        || text
            .lines()
            .any(|line| line.starts_with("+++ ") || line.starts_with("--- "))
    {
        return true;
    }

    if [
        "error[",
        "test result:",
        "assertion failed",
        "failures:",
        "cargo test",
        "cargo check",
        "npm test",
        "build failed",
        "compiling ",
    ]
    .iter()
    .any(|needle| lowered.contains(needle))
    {
        return true;
    }

    contains_path_like_token(text)
}

fn contains_path_like_token(text: &str) -> bool {
    const CODE_EXTENSIONS: &[&str] = &[
        ".rs", ".toml", ".ts", ".tsx", ".js", ".jsx", ".py", ".json", ".md", ".yaml", ".yml",
        ".cjs", ".mjs",
    ];

    text.split_whitespace().any(|token| {
        let token = token.trim_matches(|ch: char| {
            matches!(
                ch,
                '`' | '"' | '\'' | '(' | ')' | '[' | ']' | '{' | '}' | ',' | ';' | ':'
            )
        });
        if token.starts_with('/') && token.len() > 1 {
            return true;
        }
        token.contains('/')
            && CODE_EXTENSIONS
                .iter()
                .any(|extension| token.contains(extension))
    })
}

fn coding_execution_map(messages: &[AgentMessage]) -> String {
    let active_directory = checkpoint_active_directory(messages)
        .unwrap_or_else(|| COMPACTION_UNKNOWN_DIRECTORY.to_string());
    format!(
        "- Completed: {}\n- Current: {}\n- Pending: {}\n- Active directory: `{}`",
        checkpoint_completed_phase(messages),
        checkpoint_current_phase(messages),
        checkpoint_pending_phases(messages),
        active_directory,
    )
}

fn collect_message_structural_refs(messages: &[AgentMessage]) -> Vec<String> {
    let mut refs = Vec::new();
    for message in messages {
        for structural_ref in &message.structural_refs {
            if !refs.iter().any(|existing| existing == structural_ref) {
                refs.push(structural_ref.clone());
            }
        }
    }
    refs
}

fn render_structural_context(entries: &[StructuralContextEntry]) -> String {
    if entries.is_empty() {
        return "- none (No structural nodes were available for this compacted slice.)\n"
            .to_string();
    }

    entries
        .iter()
        .map(|entry| {
            format!(
                "- `{}` - {}\n",
                entry.node_id,
                super::goal_parsing::summarize_text(entry.summary.as_str(), 220)
            )
        })
        .collect()
}

fn collect_referenced_offloaded_payload_ids(messages: &[AgentMessage]) -> Vec<String> {
    let mut payload_ids = Vec::new();
    for message in messages {
        let Some(payload_id) = message.offloaded_payload_id.as_deref() else {
            continue;
        };
        if !payload_ids.iter().any(|existing| existing == payload_id) {
            payload_ids.push(payload_id.to_string());
        }
    }
    payload_ids
}

fn summarize_offloaded_metadata_summary(summary: &str) -> String {
    let normalized = summary.split_whitespace().collect::<Vec<_>>().join(" ");
    if normalized.is_empty() {
        "summary unavailable".to_string()
    } else {
        super::goal_parsing::summarize_text(normalized.as_str(), 220)
    }
}

fn render_offloaded_payload_references(
    metadata_rows: &[crate::history::OffloadedPayloadMetadataRow],
) -> String {
    let mut rendered = String::new();

    for metadata in metadata_rows {
        rendered.push_str(&format!(
            "- `{}` (`{}`, {} bytes) - {}\n",
            metadata.payload_id,
            metadata.tool_name,
            metadata.byte_size,
            summarize_offloaded_metadata_summary(metadata.summary.as_str())
        ));
    }

    if rendered.is_empty() {
        "- none (No referenced offloaded payload metadata was available for this compacted slice.)\n"
            .to_string()
    } else {
        rendered
    }
}

async fn load_referenced_offloaded_payload_metadata(
    history: &crate::history::HistoryStore,
    thread_id: &str,
    messages: &[AgentMessage],
) -> Result<Vec<crate::history::OffloadedPayloadMetadataRow>> {
    let mut rows = Vec::new();

    for payload_id in collect_referenced_offloaded_payload_ids(messages)
        .into_iter()
        .take(CODING_COMPACTION_OFFLOAD_REFERENCE_LIMIT)
    {
        let Some(metadata) = history
            .get_offloaded_payload_metadata(payload_id.as_str())
            .await?
        else {
            continue;
        };
        if metadata.thread_id == thread_id {
            rows.push(metadata);
        }
    }

    Ok(rows)
}

fn coding_compaction_payload_max_chars(target_tokens: usize) -> usize {
    (target_tokens / 4)
        .saturating_mul(APPROX_CHARS_PER_TOKEN)
        .clamp(4096, 8192)
}

fn merge_compaction_fallback_notice(
    primary: Option<String>,
    secondary: Option<String>,
) -> Option<String> {
    match (primary, secondary) {
        (Some(primary), Some(secondary)) if primary == secondary => Some(primary),
        (Some(primary), Some(secondary)) => Some(format!("{primary} {secondary}")),
        (Some(primary), None) => Some(primary),
        (None, Some(secondary)) => Some(secondary),
        (None, None) => None,
    }
}

pub(super) fn build_compaction_summary(messages: &[AgentMessage], target_tokens: usize) -> String {
    if messages.is_empty() {
        return String::new();
    }

    let max_chars = (target_tokens / 4)
        .saturating_mul(APPROX_CHARS_PER_TOKEN)
        .clamp(4096, 8192);
    let primary_objective = checkpoint_primary_objective(messages);
    let completed_phase = checkpoint_completed_phase(messages);
    let current_phase = checkpoint_current_phase(messages);
    let pending_phases = checkpoint_pending_phases(messages);
    let active_directory = checkpoint_active_directory(messages)
        .unwrap_or_else(|| COMPACTION_UNKNOWN_DIRECTORY.to_string());
    let files_modified = checkpoint_files_modified(messages);
    let read_only_context = checkpoint_read_only_context(messages);
    let acquired_knowledge = checkpoint_acquired_knowledge(messages);
    let dead_ends = checkpoint_dead_ends(messages);
    let recent_actions = checkpoint_recent_actions(messages);
    let immediate_next_step = checkpoint_immediate_next_step(messages);

    let mut summary = format!(
        "# 🤖 Agent Context: State Checkpoint\n\n## 🎯 Primary Objective\n> {}\n\n## 🗺️ Execution Map\n* **✅ Completed Phase:** {}\n* **⏳ Current Phase:** {}\n* **⏭️ Pending Phases:** {}\n\n## 📁 Working Environment State\n* **Active Directory:** `{}`\n* **Files Modified (Uncommitted/Pending):**\n{}* **Read-Only Context Files:**\n{}## 🧠 Acquired Knowledge & Constraints\n{}## 🚫 Dead Ends & Resolved Errors\n{}## 🛠️ Recent Action Summary (Last 3-5 Turns)\n{}\n## 🎯 Immediate Next Step\n{}\n",
        primary_objective,
        completed_phase,
        current_phase,
        pending_phases,
        active_directory,
        files_modified,
        read_only_context,
        acquired_knowledge,
        dead_ends,
        recent_actions,
        immediate_next_step,
    );

    if summary.len() > max_chars {
        summary.truncate(max_chars);
    }

    summary
}

#[cfg(test)]
#[path = "compaction/tests.rs"]
mod tests;

fn summarize_compacted_message(message: &AgentMessage) -> String {
    let role = match message.role {
        MessageRole::System => "SYSTEM",
        MessageRole::User => "USER",
        MessageRole::Assistant => "ASSISTANT",
        MessageRole::Tool => "TOOL",
    };

    let mut details = String::new();
    if let Some(name) = message
        .tool_name
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        details.push_str(name);
        if let Some(status) = message
            .tool_status
            .as_deref()
            .filter(|value| !value.trim().is_empty())
        {
            details.push(' ');
            details.push_str(status);
        }
    } else if let Some(tool_calls) = &message.tool_calls {
        let names = tool_calls
            .iter()
            .map(|call| call.function.name.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        if !names.is_empty() {
            details.push_str(&format!("calls: {names}"));
        }
    }

    let content = super::goal_parsing::summarize_text(compaction_runtime_content(message), 160);
    if details.is_empty() {
        format!("{role}: {content}")
    } else {
        format!("{role} [{details}]: {content}")
    }
}

fn checkpoint_primary_objective(messages: &[AgentMessage]) -> String {
    let first_user = messages
        .iter()
        .find(|message| message.role == MessageRole::User)
        .map(|message| {
            super::goal_parsing::summarize_text(compaction_runtime_content(message), 180)
        });
    let latest_user = messages
        .iter()
        .rev()
        .find(|message| message.role == MessageRole::User)
        .map(|message| {
            super::goal_parsing::summarize_text(compaction_runtime_content(message), 180)
        });

    match (first_user, latest_user) {
        (Some(first), Some(latest)) if first != latest => {
            format!("{} Latest carried-forward ask: {}", first, latest)
        }
        (Some(first), _) => first,
        (_, Some(latest)) => latest,
        _ => "Continue the active workstream using the retained recent context and preserved checkpoint facts.".to_string(),
    }
}

fn checkpoint_completed_phase(messages: &[AgentMessage]) -> String {
    messages
        .iter()
        .rev()
        .find(|message| message.role != MessageRole::User)
        .map(|message| format!("Captured prior progress: {}", summarize_compacted_message(message)))
        .unwrap_or_else(|| "Captured the earlier slice of conversation so the active work can continue without replaying raw history.".to_string())
}

fn checkpoint_current_phase(messages: &[AgentMessage]) -> String {
    messages
        .last()
        .map(|message| match message.role {
            MessageRole::User => format!(
                "Resume from the latest carried-forward user request: {}",
                super::goal_parsing::summarize_text(compaction_runtime_content(message), 180)
            ),
            MessageRole::Assistant => format!(
                "Continue from the latest assistant state: {}",
                super::goal_parsing::summarize_text(compaction_runtime_content(message), 180)
            ),
            MessageRole::Tool => format!(
                "Continue after the last tool outcome: {}",
                summarize_compacted_message(message)
            ),
            MessageRole::System => format!(
                "Honor the preserved system guidance: {}",
                super::goal_parsing::summarize_text(compaction_runtime_content(message), 180)
            ),
        })
        .unwrap_or_else(|| {
            "Resume execution from the retained recent context without replaying the discarded raw history.".to_string()
        })
}

fn checkpoint_pending_phases(messages: &[AgentMessage]) -> String {
    let latest_user = messages
        .iter()
        .rev()
        .find(|message| message.role == MessageRole::User)
        .map(|message| {
            super::goal_parsing::summarize_text(compaction_runtime_content(message), 140)
        });
    match latest_user {
        Some(latest_user) => format!(
            "Continue the active request, validate the affected slice, and close any unresolved risks around: {}",
            latest_user
        ),
        None => "Continue the active task, validate the affected slice, and surface any unresolved risks before expanding scope.".to_string(),
    }
}

fn checkpoint_active_directory(messages: &[AgentMessage]) -> Option<String> {
    messages
        .iter()
        .rev()
        .find_map(|message| extract_labeled_path(compaction_runtime_content(message)))
}

fn checkpoint_files_modified(messages: &[AgentMessage]) -> String {
    let files = collect_context_paths(messages, true);
    if files.is_empty() {
        return COMPACTION_NO_FILES_CAPTURED.to_string();
    }

    files
        .into_iter()
        .map(|file| format!("* `{file}` - (Referenced as part of the active compacted work.)\n"))
        .collect()
}

fn checkpoint_read_only_context(messages: &[AgentMessage]) -> String {
    let files = collect_context_paths(messages, false);
    if files.is_empty() {
        return COMPACTION_NO_READONLY_CAPTURED.to_string();
    }

    files
        .into_iter()
        .map(|file| format!("* `{file}` - (Context referenced in the compacted history.)\n"))
        .collect()
}

fn checkpoint_acquired_knowledge(messages: &[AgentMessage]) -> String {
    let items = messages
        .iter()
        .filter(|message| message.role != MessageRole::System)
        .map(|message| summarize_compacted_message(message))
        .filter(|summary| !summary.trim().is_empty())
        .collect::<Vec<_>>();

    unique_bullets(
        &items,
        4,
        "Continue from the retained recent context; no additional older constraints were preserved in this slice.",
    )
}

fn checkpoint_dead_ends(messages: &[AgentMessage]) -> String {
    let dead_ends = messages
        .iter()
        .filter_map(|message| {
            let content = compaction_runtime_content(message);
            let lowered = content.to_ascii_lowercase();
            let is_failure = lowered.contains("error")
                || lowered.contains("failed")
                || lowered.contains("timeout")
                || lowered.contains("blocked")
                || lowered.contains("unsupported");
            is_failure.then(|| {
                format!(
                    "* **Failed:** {}\n    * *Resolution:* Preserve the failure and avoid replaying the discarded path without new evidence.\n",
                    summarize_compacted_message(message)
                )
            })
        })
        .take(3)
        .collect::<String>();

    if dead_ends.is_empty() {
        COMPACTION_NO_DEAD_ENDS_CAPTURED.to_string()
    } else {
        dead_ends
    }
}

fn checkpoint_recent_actions(messages: &[AgentMessage]) -> String {
    let actions = messages.iter().rev().take(5).collect::<Vec<_>>();
    let mut ordered = actions;
    ordered.reverse();

    ordered
        .into_iter()
        .enumerate()
        .map(|(index, message)| {
            let action = match message.role {
                MessageRole::Tool => format!(
                    "{}({})",
                    message.tool_name.as_deref().unwrap_or("tool"),
                    super::goal_parsing::summarize_text(
                        message.tool_arguments.as_deref().unwrap_or("{}"),
                        80,
                    )
                ),
                MessageRole::Assistant => "assistant_step(...)".to_string(),
                MessageRole::User => "user_request(...)".to_string(),
                MessageRole::System => "system_context(...)".to_string(),
            };
            format!(
                "{}. `{}` -> {}\n",
                index + 1,
                action,
                summarize_compacted_message(message)
            )
        })
        .collect()
}

fn checkpoint_immediate_next_step(messages: &[AgentMessage]) -> String {
    messages
        .iter()
        .rev()
        .find(|message| message.role == MessageRole::User)
        .map(|message| {
            format!(
                "Answer the latest carried-forward user request: {}",
                super::goal_parsing::summarize_text(compaction_runtime_content(message), 180)
            )
        })
        .unwrap_or_else(|| {
            "Read the retained recent messages and continue the active task without replaying discarded history.".to_string()
        })
}

fn unique_bullets(items: &[String], max_items: usize, fallback: &str) -> String {
    let mut deduped = Vec::new();
    for item in items {
        if deduped.iter().any(|existing: &String| existing == item) {
            continue;
        }
        deduped.push(item.clone());
        if deduped.len() >= max_items {
            break;
        }
    }

    if deduped.is_empty() {
        return format!("* {}\n", fallback);
    }

    deduped
        .into_iter()
        .map(|item| format!("* {}\n", item))
        .collect()
}

fn collect_context_paths(messages: &[AgentMessage], prefer_modified: bool) -> Vec<String> {
    let mut paths = Vec::new();
    for message in messages {
        let content = compaction_runtime_content(message);
        if let Some(path) = extract_labeled_path(content) {
            if !paths.iter().any(|existing| existing == &path) {
                paths.push(path);
            }
        }

        let tool_name = message.tool_name.as_deref().unwrap_or_default();
        let modified_tool = matches!(
            tool_name,
            "write_file" | "create_file" | "apply_patch" | "rename" | "delete"
        );
        if modified_tool != prefer_modified {
            continue;
        }
        if let Some(path) =
            extract_path_token(message.tool_arguments.as_deref().unwrap_or_default())
        {
            if !paths.iter().any(|existing| existing == &path) {
                paths.push(path);
            }
        }
        if paths.len() >= 3 {
            break;
        }
    }
    paths
}

fn extract_labeled_path(text: &str) -> Option<String> {
    for label in [
        "Working directory:",
        "working directory:",
        "Active Directory:",
        "active directory:",
        "Dir:",
        "dir:",
        "Cwd:",
        "cwd:",
    ] {
        if let Some(index) = text.find(label) {
            let remainder = text[index + label.len()..].trim_start();
            if let Some(path) = extract_path_token(remainder) {
                return Some(path);
            }
        }
    }
    None
}

fn extract_path_token(text: &str) -> Option<String> {
    let trimmed = text.trim_start();
    let trimmed = trimmed.strip_prefix('`').unwrap_or(trimmed);
    let mut path = String::new();
    for ch in trimmed.chars() {
        if ch.is_whitespace() || matches!(ch, ',' | ';' | ')' | ']' | '|' | '*') {
            break;
        }
        path.push(ch);
    }
    let path = path
        .trim_matches(|ch| matches!(ch, '`' | '"' | '\''))
        .to_string();
    path.starts_with('/').then_some(path)
}

fn select_compaction_transport(
    provider_id: &str,
    provider_config: &ProviderConfig,
) -> ApiTransport {
    if provider_id == PROVIDER_ID_OPENAI
        && provider_config.auth_source == AuthSource::ChatgptSubscription
    {
        return ApiTransport::Responses;
    }

    let selected = if provider_supports_transport(provider_id, provider_config.api_transport) {
        provider_config.api_transport
    } else {
        default_api_transport_for_provider(provider_id)
    };

    match selected {
        ApiTransport::NativeAssistant => {
            if provider_supports_transport(provider_id, ApiTransport::Responses) {
                ApiTransport::Responses
            } else {
                ApiTransport::ChatCompletions
            }
        }
        other => other,
    }
}

fn message_has_contentful_dialogue(message: &AgentMessage) -> bool {
    match message.role {
        MessageRole::Tool => false,
        MessageRole::Assistant => {
            !compaction_runtime_content(message).trim().is_empty()
                || message
                    .tool_calls
                    .as_ref()
                    .is_some_and(|tool_calls| !tool_calls.is_empty())
        }
        MessageRole::User | MessageRole::System => {
            !compaction_runtime_content(message).trim().is_empty()
        }
    }
}

fn sanitize_recent_compaction_message(message: &AgentMessage) -> AgentMessage {
    let mut sanitized = materialize_compaction_message(message);
    match sanitized.role {
        MessageRole::Assistant => {
            if let Some(tool_calls) = &sanitized.tool_calls {
                let tool_names = tool_calls
                    .iter()
                    .map(|call| call.function.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ");
                let content = sanitized.content.trim();
                sanitized.content = match (content.is_empty(), tool_names.is_empty()) {
                    (_, true) => content.to_string(),
                    (true, false) => format!("Assistant requested tools: {tool_names}"),
                    (false, false) => format!("{content}\nTools used: {tool_names}"),
                };
                sanitized.tool_calls = None;
            }
        }
        MessageRole::Tool => {
            sanitized.content = sanitized
                .tool_name
                .clone()
                .filter(|name| !name.trim().is_empty())
                .unwrap_or_else(|| "tool".to_string());
            sanitized.tool_arguments = None;
            sanitized.tool_status = None;
        }
        MessageRole::System | MessageRole::User => {}
    }
    sanitized
}

fn select_recent_llm_compaction_messages(messages: &[AgentMessage]) -> (usize, Vec<AgentMessage>) {
    if messages.is_empty() {
        return (0, Vec::new());
    }

    let mut contentful_seen = 0usize;
    let mut start_index = messages.len();
    for (index, message) in messages.iter().enumerate().rev() {
        if message_has_contentful_dialogue(message) {
            contentful_seen += 1;
        }
        start_index = index;
        if contentful_seen >= COMPACTION_MODEL_RECENT_CONTENT_MESSAGES {
            break;
        }
    }

    let recent = messages[start_index..]
        .iter()
        .map(sanitize_recent_compaction_message)
        .collect::<Vec<_>>();
    (start_index, recent)
}

fn build_llm_compaction_messages(
    messages: &[AgentMessage],
    target_tokens: usize,
    max_input_tokens: usize,
) -> Vec<ApiMessage> {
    let source_messages = messages
        .iter()
        .map(materialize_compaction_message)
        .collect::<Vec<_>>();
    let use_exact_messages = source_messages.len() <= COMPACTION_EXACT_MESSAGE_MAX
        && estimate_message_tokens(&source_messages) <= target_tokens.saturating_mul(4).max(2_048);

    let mut api_messages = if use_exact_messages {
        messages_to_api_format(&source_messages)
    } else {
        let (recent_start, recent_messages) =
            select_recent_llm_compaction_messages(&source_messages);
        let mut reduced_messages = Vec::new();
        if recent_start > 0 {
            reduced_messages.push(ApiMessage {
                role: "user".to_string(),
                content: ApiContent::Text(format!(
                    "Older context to compact:\n\n{}",
                    build_compaction_summary(
                        &source_messages[..recent_start],
                        target_tokens.saturating_mul(2),
                    )
                )),
                tool_call_id: None,
                name: None,
                tool_calls: None,
            });
        }
        reduced_messages.extend(messages_to_api_format(&recent_messages));
        reduced_messages
    };

    api_messages.push(ApiMessage {
        role: "user".to_string(),
        content: ApiContent::Text(
            format!(
                "Follow the mandatory thread-compaction protocol and return exactly one markdown checkpoint block matching this schema:\n\n{}\n\nCompress the supplied older context into that checkpoint. Preserve requests, constraints, decisions, tool outcomes, errors worth remembering, and any unresolved next steps.",
                COMPACTION_CHECKPOINT_SCHEMA
            ),
        ),
        tool_call_id: None,
        name: None,
        tool_calls: None,
    });
    trim_llm_compaction_messages_to_fit(&mut api_messages, max_input_tokens.max(1));
    api_messages
}

fn llm_compaction_input_budget(provider_id: &str, provider_config: &ProviderConfig) -> usize {
    let model_window = model_context_window(
        provider_id,
        &provider_config.model,
        provider_config.context_window_tokens,
    ) as usize;
    let minimum = model_window.min(MIN_CONTEXT_TARGET_TOKENS);
    model_window
        .saturating_sub(COMPACTION_MODEL_REQUEST_HEADROOM_TOKENS)
        .max(minimum)
        .max(1)
}

fn estimate_api_messages_tokens(messages: &[ApiMessage]) -> usize {
    messages.iter().map(estimate_api_message_tokens).sum()
}

fn estimate_api_message_tokens(message: &ApiMessage) -> usize {
    let mut chars = message.role.chars().count();
    chars += match &message.content {
        ApiContent::Text(text) => text.chars().count(),
        ApiContent::Blocks(blocks) => blocks
            .iter()
            .map(|block| {
                serde_json::to_string(block)
                    .unwrap_or_default()
                    .chars()
                    .count()
            })
            .sum(),
    };
    chars += message
        .tool_call_id
        .as_deref()
        .map(str::chars)
        .map(Iterator::count)
        .unwrap_or(0);
    chars += message
        .name
        .as_deref()
        .map(str::chars)
        .map(Iterator::count)
        .unwrap_or(0);
    chars += message
        .tool_calls
        .as_ref()
        .map(|tool_calls| {
            tool_calls
                .iter()
                .map(|tool_call| {
                    tool_call.id.chars().count()
                        + tool_call.call_type.chars().count()
                        + tool_call.function.name.chars().count()
                        + tool_call.function.arguments.chars().count()
                })
                .sum::<usize>()
        })
        .unwrap_or(0);
    chars.div_ceil(APPROX_CHARS_PER_TOKEN) + 12
}

fn trim_llm_compaction_messages_to_fit(
    api_messages: &mut Vec<ApiMessage>,
    max_input_tokens: usize,
) {
    if api_messages.len() <= 1 {
        return;
    }

    while estimate_api_messages_tokens(api_messages) > max_input_tokens && api_messages.len() > 2 {
        api_messages.remove(0);
    }

    while estimate_api_messages_tokens(api_messages) > max_input_tokens && api_messages.len() > 1 {
        let instruction_tokens = estimate_api_message_tokens(
            api_messages
                .last()
                .expect("instruction message should remain present"),
        );
        let available = max_input_tokens.saturating_sub(instruction_tokens).max(64);
        truncate_api_message_to_fit(&mut api_messages[0], available);
        if estimate_api_message_tokens(&api_messages[0]) <= available {
            break;
        }
        api_messages.remove(0);
    }
}

fn truncate_api_message_to_fit(message: &mut ApiMessage, max_tokens: usize) {
    message.tool_call_id = None;
    message.name = None;
    message.tool_calls = None;

    let ApiContent::Text(text) = &mut message.content else {
        return;
    };
    if text.chars().count() <= max_tokens {
        return;
    }

    let available = max_tokens.saturating_sub(COMPACTION_MESSAGE_TRUNCATION_NOTICE.chars().count());
    let suffix = text
        .chars()
        .rev()
        .take(available)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<String>();
    *text = format!("{}{}", COMPACTION_MESSAGE_TRUNCATION_NOTICE, suffix);
}

impl AgentEngine {
    pub(super) async fn maybe_persist_compaction_artifact(
        &self,
        thread_id: &str,
        task_id: Option<&str>,
        config: &AgentConfig,
        provider_config: &ProviderConfig,
    ) -> Result<bool> {
        self.persist_compaction_artifact_with_mode(
            thread_id,
            task_id,
            config,
            provider_config,
            CompactionCandidateMode::Automatic,
        )
        .await
    }

    pub(super) async fn force_persist_compaction_artifact(
        &self,
        thread_id: &str,
        task_id: Option<&str>,
        config: &AgentConfig,
        provider_config: &ProviderConfig,
    ) -> Result<bool> {
        self.persist_compaction_artifact_with_mode(
            thread_id,
            task_id,
            config,
            provider_config,
            CompactionCandidateMode::Forced,
        )
        .await
    }

    async fn persist_compaction_artifact_with_mode(
        &self,
        thread_id: &str,
        task_id: Option<&str>,
        config: &AgentConfig,
        provider_config: &ProviderConfig,
        mode: CompactionCandidateMode,
    ) -> Result<bool> {
        let snapshot = {
            let threads = self.threads.read().await;
            threads.get(thread_id).cloned()
        };
        let Some(thread) = snapshot else {
            return Ok(false);
        };
        let (window_start, _) = active_compaction_window(&thread.messages);
        let Some(candidate) = (match mode {
            CompactionCandidateMode::Automatic => {
                compaction_candidate(&thread.messages, config, provider_config)
            }
            CompactionCandidateMode::Forced => {
                forced_compaction_candidate(&thread.messages, config, provider_config)
            }
        }) else {
            return Ok(false);
        };
        let pre_compaction_total_tokens = estimate_message_tokens(&thread.messages[window_start..]);
        let effective_context_window_tokens =
            effective_compaction_window_tokens(config, provider_config);
        let split_at = window_start + candidate.split_at;
        let source_messages = thread.messages[window_start..split_at].to_vec();
        let message_count = thread.messages.len();
        let structural_memory = self.get_thread_structural_memory(thread_id).await;

        let (artifact, strategy_used, fallback_notice) = self
            .build_compaction_artifact(
                thread_id,
                &source_messages,
                candidate.target_tokens,
                candidate.trigger,
                pre_compaction_total_tokens,
                effective_context_window_tokens,
                config,
                structural_memory.as_ref(),
            )
            .await?;
        let compaction_trigger_summary = build_compaction_visible_content(
            pre_compaction_total_tokens,
            effective_context_window_tokens,
            candidate.target_tokens,
            candidate.trigger,
            strategy_used,
            artifact.compaction_payload.as_deref().unwrap_or(""),
        );

        let (current_split_at, total_message_count) = {
            let mut threads = self.threads.write().await;
            let Some(thread) = threads.get_mut(thread_id) else {
                return Ok(false);
            };
            let (window_start, _) = active_compaction_window(&thread.messages);
            let Some(current_candidate) = (match mode {
                CompactionCandidateMode::Automatic => {
                    compaction_candidate(&thread.messages, config, provider_config)
                }
                CompactionCandidateMode::Forced => {
                    forced_compaction_candidate(&thread.messages, config, provider_config)
                }
            }) else {
                return Ok(false);
            };
            let current_split_at = window_start + current_candidate.split_at;
            thread.messages.insert(current_split_at, artifact);
            thread.messages.drain(..current_split_at);
            thread.updated_at = now_millis();
            thread.total_input_tokens = thread
                .messages
                .iter()
                .map(|message| message.input_tokens)
                .sum();
            thread.total_output_tokens = thread
                .messages
                .iter()
                .map(|message| message.output_tokens)
                .sum();
            (current_split_at, thread.messages.len())
        };
        let compaction_notice_details = serde_json::json!({
            "split_at": current_split_at,
            "total_message_count": total_message_count,
            "pre_compaction_total_tokens": pre_compaction_total_tokens,
            "effective_context_window_tokens": effective_context_window_tokens,
            "target_tokens": candidate.target_tokens,
            "trigger": compaction_trigger_detail_value(candidate.trigger),
            "strategy": strategy_used,
        })
        .to_string();

        self.persist_thread_by_id(thread_id).await;
        self.trim_participant_playground_threads_for_visible_thread(thread_id)
            .await;
        self.record_provenance_event(
            "context_compressed",
            match mode {
                CompactionCandidateMode::Automatic => {
                    "thread context was compacted for an LLM request"
                }
                CompactionCandidateMode::Forced => {
                    "thread context was compacted by operator request"
                }
            },
            serde_json::json!({
                "thread_id": thread_id,
                "split_at": split_at,
                "target_tokens": candidate.target_tokens,
                "trigger": compaction_trigger_detail_value(candidate.trigger),
                "message_count": message_count,
                "strategy": strategy_used,
                "forced": mode == CompactionCandidateMode::Forced,
            }),
            None,
            task_id,
            Some(thread_id),
            None,
            None,
        )
        .await;
        self.persist_context_compression_causal_trace(
            thread_id,
            task_id,
            split_at,
            message_count,
            candidate.target_tokens,
            strategy_used,
        )
        .await;
        let _ = self.event_tx.send(AgentEvent::ThreadReloadRequired {
            thread_id: thread_id.to_string(),
        });
        let _ = self.event_tx.send(AgentEvent::WorkflowNotice {
            thread_id: thread_id.to_string(),
            kind: match mode {
                CompactionCandidateMode::Automatic => COMPACTION_NOTICE_KIND,
                CompactionCandidateMode::Forced => MANUAL_COMPACTION_NOTICE_KIND,
            }
            .to_string(),
            message: format!(
                "{} compaction applied using {}. {}",
                match mode {
                    CompactionCandidateMode::Automatic => "Auto",
                    CompactionCandidateMode::Forced => "Manual",
                },
                serde_json::to_string(&strategy_used)
                    .unwrap_or_else(|_| "\"heuristic\"".to_string())
                    .trim_matches('"'),
                compaction_trigger_summary,
            ),
            details: Some(compaction_notice_details.clone()),
        });
        if let Some(fallback_notice) = fallback_notice {
            let _ = self.event_tx.send(AgentEvent::WorkflowNotice {
                thread_id: thread_id.to_string(),
                kind: match mode {
                    CompactionCandidateMode::Automatic => COMPACTION_NOTICE_KIND,
                    CompactionCandidateMode::Forced => MANUAL_COMPACTION_NOTICE_KIND,
                }
                .to_string(),
                message: fallback_notice,
                details: Some(compaction_notice_details),
            });
        }
        Ok(true)
    }

    pub async fn force_compact_and_continue(self: &Arc<Self>, thread_id: &str) -> Result<bool> {
        if !self.threads.read().await.contains_key(thread_id) {
            anyhow::bail!("thread not found: {thread_id}");
        }

        let latest_user_content = self.latest_visible_user_message_content(thread_id).await;
        let latest_user_content = latest_user_content
            .as_deref()
            .filter(|content| !content.trim().is_empty())
            .map(str::to_string)
            .ok_or_else(|| {
                anyhow::anyhow!("no user message available to continue after compaction")
            })?;
        let agent_id = self
            .active_agent_id_for_thread(thread_id)
            .await
            .unwrap_or_else(|| MAIN_AGENT_ID.to_string());
        let continuation = DeferredVisibleThreadContinuation {
            agent_id,
            preferred_session_hint: None,
            llm_user_content: latest_user_content,
            force_compaction: true,
            internal_delegate_sender: None,
            internal_delegate_message: None,
        };

        let was_streaming = {
            let streams = self.stream_cancellations.lock().await;
            streams.contains_key(thread_id)
        };
        self.enqueue_visible_thread_continuation(thread_id, continuation)
            .await;

        if was_streaming && self.stop_stream(thread_id).await {
            let _ = self.event_tx.send(AgentEvent::WorkflowNotice {
                thread_id: thread_id.to_string(),
                kind: MANUAL_COMPACTION_NOTICE_KIND.to_string(),
                message: "Manual compaction requested; waiting for the current stream to stop."
                    .to_string(),
                details: None,
            });
            return Ok(true);
        }

        self.flush_deferred_visible_thread_continuations(thread_id)
            .await?;
        Ok(true)
    }

    async fn build_compaction_artifact(
        &self,
        thread_id: &str,
        messages: &[AgentMessage],
        target_tokens: usize,
        trigger: CompactionTrigger,
        pre_compaction_total_tokens: usize,
        effective_context_window_tokens: usize,
        config: &AgentConfig,
        structural_memory: Option<&ThreadStructuralMemory>,
    ) -> Result<(AgentMessage, CompactionStrategy, Option<String>)> {
        let mut strategy_used = config.compaction.strategy;
        let mut fallback_notice = None;
        let mut structural_refs = Vec::new();
        let payload = match strategy_used {
            CompactionStrategy::Heuristic => {
                let rule_based = self
                    .build_rule_based_compaction_payload(
                        thread_id,
                        messages,
                        target_tokens,
                        structural_memory,
                    )
                    .await;
                structural_refs = rule_based.structural_refs;
                fallback_notice = rule_based.fallback_notice;
                rule_based.payload
            }
            CompactionStrategy::Weles => {
                let (provider_id, provider_config) =
                    self.resolve_weles_compaction_provider(config)?;
                match self
                    .run_llm_compaction(&provider_id, &provider_config, messages, target_tokens)
                    .await
                {
                    Ok(payload) if !payload.trim().is_empty() => payload,
                    Ok(_) | Err(_) => {
                        strategy_used = CompactionStrategy::Heuristic;
                        let rule_based = self
                            .build_rule_based_compaction_payload(
                                thread_id,
                                messages,
                                target_tokens,
                                structural_memory,
                            )
                            .await;
                        structural_refs = rule_based.structural_refs;
                        fallback_notice = merge_compaction_fallback_notice(
                            rule_based.fallback_notice,
                            Some(
                                "WELES compaction failed; fell back to rule based compaction."
                                    .to_string(),
                            ),
                        );
                        rule_based.payload
                    }
                }
            }
            CompactionStrategy::CustomModel => {
                let (provider_id, provider_config) =
                    self.resolve_custom_model_compaction_provider(config)?;
                match self
                    .run_llm_compaction(&provider_id, &provider_config, messages, target_tokens)
                    .await
                {
                    Ok(payload) if !payload.trim().is_empty() => payload,
                    Ok(_) | Err(_) => {
                        strategy_used = CompactionStrategy::Heuristic;
                        let rule_based = self
                            .build_rule_based_compaction_payload(
                                thread_id,
                                messages,
                                target_tokens,
                                structural_memory,
                            )
                            .await;
                        structural_refs = rule_based.structural_refs;
                        fallback_notice = merge_compaction_fallback_notice(
                            rule_based.fallback_notice,
                            Some(
                                "Custom-model compaction failed; fell back to rule based compaction."
                                    .to_string(),
                            ),
                        );
                        rule_based.payload
                    }
                }
            }
        };

        let visible_content = build_compaction_visible_content(
            pre_compaction_total_tokens,
            effective_context_window_tokens,
            target_tokens,
            trigger,
            strategy_used,
            &payload,
        );

        Ok((
            AgentMessage {
                id: generate_message_id(),
                role: MessageRole::Assistant,
                content: visible_content,
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
                compaction_strategy: Some(strategy_used),
                compaction_payload: Some(payload),
                offloaded_payload_id: None,
                structural_refs,
                timestamp: messages
                    .last()
                    .map(|message| message.timestamp)
                    .unwrap_or_else(now_millis),
            },
            strategy_used,
            fallback_notice,
        ))
    }

    async fn build_rule_based_compaction_payload(
        &self,
        thread_id: &str,
        messages: &[AgentMessage],
        target_tokens: usize,
        structural_memory: Option<&ThreadStructuralMemory>,
    ) -> RuleBasedCompactionPayload {
        let checkpoint_payload = build_checkpoint_compaction_payload(messages, target_tokens);

        if crate::agent::agent_identity::is_internal_dm_thread(thread_id)
            || crate::agent::agent_identity::is_participant_playground_thread(thread_id)
            || super::thread_handoffs::is_internal_handoff_thread(thread_id)
        {
            return RuleBasedCompactionPayload {
                payload: checkpoint_payload,
                structural_refs: Vec::new(),
                fallback_notice: None,
            };
        }

        match determine_rule_based_compaction_mode(structural_memory, messages) {
            RuleBasedCompactionMode::Conversational => RuleBasedCompactionPayload {
                payload: checkpoint_payload,
                structural_refs: Vec::new(),
                fallback_notice: None,
            },
            RuleBasedCompactionMode::Coding => {
                let Some(structural_memory) = structural_memory else {
                    return RuleBasedCompactionPayload {
                        payload: checkpoint_payload,
                        structural_refs: Vec::new(),
                        fallback_notice: None,
                    };
                };

                match self
                    .build_coding_compaction_payload(
                        thread_id,
                        messages,
                        target_tokens,
                        structural_memory,
                    )
                    .await
                {
                    Ok((payload, structural_refs)) => RuleBasedCompactionPayload {
                        payload,
                        structural_refs,
                        fallback_notice: None,
                    },
                    Err(error) => {
                        tracing::warn!(
                            thread_id = %thread_id,
                            %error,
                            "structured coding compaction assembly failed"
                        );
                        RuleBasedCompactionPayload {
                            payload: checkpoint_payload,
                            structural_refs: Vec::new(),
                            fallback_notice: Some(CODING_COMPACTION_FALLBACK_NOTICE.to_string()),
                        }
                    }
                }
            }
        }
    }

    async fn build_coding_compaction_payload(
        &self,
        thread_id: &str,
        messages: &[AgentMessage],
        target_tokens: usize,
        structural_memory: &ThreadStructuralMemory,
    ) -> Result<(String, Vec<String>)> {
        let structural_entries = structural_memory.concise_context_entries(
            &collect_message_structural_refs(messages),
            CODING_COMPACTION_STRUCTURAL_ENTRY_LIMIT,
        );
        if structural_entries.is_empty() {
            anyhow::bail!("no structural context entries available for coding compaction");
        }

        let offloaded_metadata =
            load_referenced_offloaded_payload_metadata(&self.history, thread_id, messages).await?;
        let structural_refs = structural_entries
            .iter()
            .map(|entry| entry.node_id.clone())
            .collect::<Vec<_>>();
        let mut payload = format!(
            "## Primary Objective\n{}\n\n## Execution Map\n{}\n\n## Structural Context\n{}\n\n## Offloaded Payload References\n{}\n\n## Immediate Next Step\n{}\n",
            checkpoint_primary_objective(messages),
            coding_execution_map(messages),
            render_structural_context(&structural_entries),
            render_offloaded_payload_references(&offloaded_metadata),
            checkpoint_immediate_next_step(messages),
        );
        payload.truncate(coding_compaction_payload_max_chars(target_tokens));

        Ok((payload, structural_refs))
    }

    async fn run_llm_compaction(
        &self,
        provider_id: &str,
        provider_config: &ProviderConfig,
        messages: &[AgentMessage],
        target_tokens: usize,
    ) -> Result<String> {
        let transport = select_compaction_transport(provider_id, provider_config);
        let api_messages = build_llm_compaction_messages(
            messages,
            target_tokens,
            llm_compaction_input_budget(provider_id, provider_config),
        );
        self.check_circuit_breaker(provider_id).await?;

        let mut stream = send_completion_request(
            &self.http_client,
            provider_id,
            provider_config,
            COMPACTION_MODEL_SYSTEM_PROMPT,
            &api_messages,
            &[],
            transport,
            None,
            None,
            RetryStrategy::DurableRateLimited,
        );
        let mut content = String::new();
        let mut reasoning = String::new();
        while let Some(chunk) = stream.next().await {
            let chunk = match chunk {
                Ok(value) => value,
                Err(error) => {
                    self.record_llm_outcome(provider_id, false).await;
                    return Err(error);
                }
            };
            match chunk {
                CompletionChunk::Delta {
                    content: delta,
                    reasoning: reasoning_delta,
                } => {
                    content.push_str(&delta);
                    if let Some(reasoning_delta) = reasoning_delta {
                        reasoning.push_str(&reasoning_delta);
                    }
                }
                CompletionChunk::Done {
                    content: done,
                    reasoning: done_reasoning,
                    ..
                } => {
                    self.record_llm_outcome(provider_id, true).await;
                    if let Some(done_reasoning) = done_reasoning {
                        reasoning = done_reasoning;
                    }
                    let final_content = if done.is_empty() { content } else { done };
                    let trimmed = final_content.trim();
                    if !trimmed.is_empty() {
                        return Ok(trimmed.to_string());
                    }
                    if !reasoning.trim().is_empty() {
                        return Ok(reasoning.trim().to_string());
                    }
                    anyhow::bail!("compaction LLM returned empty output");
                }
                CompletionChunk::Error { message } => {
                    self.record_llm_outcome(provider_id, false).await;
                    anyhow::bail!(message);
                }
                CompletionChunk::ToolCalls { .. } => {
                    self.record_llm_outcome(provider_id, true).await;
                    anyhow::bail!("compaction LLM unexpectedly returned tool calls");
                }
                CompletionChunk::TransportFallback { .. } | CompletionChunk::Retry { .. } => {}
            }
        }

        if !content.trim().is_empty() {
            return Ok(content.trim().to_string());
        }
        anyhow::bail!("compaction LLM returned empty output")
    }

    fn resolve_weles_compaction_provider(
        &self,
        config: &AgentConfig,
    ) -> Result<(String, ProviderConfig)> {
        let provider_id = config.compaction.weles.provider.trim().to_string();
        let provider_id = if provider_id.is_empty() {
            config
                .builtin_sub_agents
                .weles
                .provider
                .clone()
                .filter(|value| !value.trim().is_empty())
                .unwrap_or_else(|| config.provider.clone())
        } else {
            provider_id
        };
        let model = config.compaction.weles.model.trim().to_string();
        let model = if model.is_empty() {
            config
                .builtin_sub_agents
                .weles
                .model
                .clone()
                .filter(|value| !value.trim().is_empty())
                .unwrap_or_else(|| config.model.clone())
        } else {
            model
        };
        let reasoning_effort = config.compaction.weles.reasoning_effort.trim().to_string();
        let reasoning_effort = if reasoning_effort.is_empty() {
            config
                .builtin_sub_agents
                .weles
                .reasoning_effort
                .clone()
                .filter(|value| !value.trim().is_empty())
                .unwrap_or_else(|| "medium".to_string())
        } else {
            reasoning_effort
        };
        let mut provider_config =
            resolve_provider_config_for(config, &provider_id, Some(model.as_str()))?;
        provider_config.reasoning_effort = reasoning_effort;
        provider_config.response_schema = None;
        Ok((provider_id, provider_config))
    }

    fn resolve_custom_model_compaction_provider(
        &self,
        config: &AgentConfig,
    ) -> Result<(String, ProviderConfig)> {
        let custom = &config.compaction.custom_model;
        let mut runtime_config = config.clone();
        runtime_config.providers.clear();
        if !custom.provider.trim().is_empty() {
            runtime_config.provider = custom.provider.trim().to_string();
        }
        if !custom.base_url.trim().is_empty() {
            runtime_config.base_url = custom.base_url.trim().to_string();
        }
        if !custom.model.trim().is_empty() {
            runtime_config.model = custom.model.trim().to_string();
        }
        if !custom.api_key.trim().is_empty() {
            runtime_config.api_key = custom.api_key.clone();
        }
        if !custom.assistant_id.trim().is_empty() {
            runtime_config.assistant_id = custom.assistant_id.clone();
        }
        runtime_config.auth_source = custom.auth_source;
        runtime_config.api_transport = custom.api_transport;
        if !custom.reasoning_effort.trim().is_empty() {
            runtime_config.reasoning_effort = custom.reasoning_effort.clone();
        }
        if custom.context_window_tokens > 0 {
            runtime_config.context_window_tokens = custom.context_window_tokens;
        }

        let provider_id = runtime_config.provider.trim().to_string();
        if provider_id.is_empty() {
            anyhow::bail!("custom compaction provider is not configured");
        }
        let model = runtime_config.model.trim().to_string();
        if model.is_empty() {
            anyhow::bail!("custom compaction model is not configured");
        }

        let mut provider_config =
            resolve_provider_config_for(&runtime_config, &provider_id, Some(model.as_str()))?;
        provider_config.response_schema = None;
        Ok((provider_id, provider_config))
    }
}

/// Hard-truncate messages from the front to fit within a token limit.
/// Keeps the most recent messages. Respects tool-call/result pairs.
fn hard_truncate_to_fit(messages: &[AgentMessage], max_tokens: usize) -> Vec<AgentMessage> {
    // Walk backwards, accumulating tokens until we hit the limit.
    let mut kept: Vec<AgentMessage> = Vec::new();
    let mut total = 0usize;
    for msg in messages.iter().rev() {
        let msg_tokens = estimate_single_message_tokens(msg);
        if total + msg_tokens > max_tokens && !kept.is_empty() {
            break;
        }
        total += msg_tokens;
        kept.push(msg.clone());
    }
    kept.reverse();

    // Ensure we don't start with orphaned tool results.
    while !kept.is_empty() && kept[0].role == super::types::MessageRole::Tool {
        kept.remove(0);
    }

    tracing::warn!(
        "hard_truncate_to_fit: trimmed {} -> {} messages ({} -> {} est tokens)",
        messages.len(),
        kept.len(),
        estimate_message_tokens(messages),
        estimate_message_tokens(&kept),
    );

    kept
}

/// Get the model's context window from its definition, falling back to config.
pub(super) fn model_context_window(provider_id: &str, model_id: &str, config_fallback: u32) -> u32 {
    super::types::get_provider_definition(provider_id)
        .and_then(|def| def.models.iter().find(|m| m.id == model_id))
        .map(|m| m.context_window)
        .unwrap_or(config_fallback)
        .max(1)
}
