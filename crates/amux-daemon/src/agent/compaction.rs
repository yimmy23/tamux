//! Context compaction — token-aware message compression for LLM requests.

use super::llm_client::{messages_to_api_format, ApiMessage};
use super::types::*;
use super::{APPROX_CHARS_PER_TOKEN, MIN_CONTEXT_TARGET_TOKENS};

pub(super) struct PreparedLlmRequest {
    pub messages: Vec<ApiMessage>,
    pub transport: ApiTransport,
    pub previous_response_id: Option<String>,
    pub upstream_thread_id: Option<String>,
}

pub(super) fn message_is_compaction_summary(message: &AgentMessage) -> bool {
    message.content.starts_with("[Compacted earlier context]")
}

pub(super) fn prepare_llm_request(
    thread: &AgentThread,
    config: &AgentConfig,
    provider_config: &ProviderConfig,
) -> PreparedLlmRequest {
    let mut selected_transport =
        if provider_supports_transport(&config.provider, provider_config.api_transport) {
            provider_config.api_transport
        } else {
            default_api_transport_for_provider(&config.provider)
        };
    if config.provider == "openai"
        && provider_config.auth_source == crate::agent::types::AuthSource::ChatgptSubscription
    {
        selected_transport = ApiTransport::Responses;
    }
    let messages = &thread.messages;
    let compacted = compact_messages_for_request(messages, config, provider_config);
    let compaction_active =
        compacted.len() != messages.len() || compacted.iter().any(message_is_compaction_summary);

    if selected_transport == ApiTransport::NativeAssistant
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
            };
        }
    }

    if selected_transport == ApiTransport::Responses {
        let previous_response_id = if !compaction_active
            && supports_response_continuity(&config.provider)
        {
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
                    let trailing_messages = messages_to_api_format(&messages[anchor_index + 1..]);
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
            return PreparedLlmRequest {
                messages,
                transport: ApiTransport::Responses,
                previous_response_id,
                upstream_thread_id: None,
            };
        }

        return PreparedLlmRequest {
            messages: messages_to_api_format(&compacted),
            transport: ApiTransport::Responses,
            previous_response_id: None,
            upstream_thread_id: None,
        };
    }

    PreparedLlmRequest {
        messages: messages_to_api_format(&compacted),
        transport: ApiTransport::ChatCompletions,
        previous_response_id: None,
        upstream_thread_id: None,
    }
}

pub(super) fn build_api_messages_for_request(
    messages: &[AgentMessage],
    config: &AgentConfig,
    provider_config: &ProviderConfig,
) -> Vec<ApiMessage> {
    let compacted = compact_messages_for_request(messages, config, provider_config);
    messages_to_api_format(&compacted)
}

pub(super) fn compact_messages_for_request(
    messages: &[AgentMessage],
    config: &AgentConfig,
    provider_config: &ProviderConfig,
) -> Vec<AgentMessage> {
    if messages.is_empty() || !config.auto_compact_context {
        return messages.to_vec();
    }

    let max_messages = config.max_context_messages.max(1) as usize;
    let target_tokens = effective_context_target_tokens(config, provider_config);
    if messages.len() <= max_messages && estimate_message_tokens(messages) <= target_tokens {
        return messages.to_vec();
    }

    let keep_recent = config
        .keep_recent_on_compact
        .max(1)
        .min(messages.len() as u32) as usize;
    let split_at = messages.len().saturating_sub(keep_recent);
    let mut compacted = Vec::new();
    let mut has_summary = false;

    if split_at > 0 {
        let summary = build_compaction_summary(&messages[..split_at], target_tokens);
        if !summary.is_empty() {
            has_summary = true;
            compacted.push(AgentMessage {
                role: MessageRole::Assistant,
                content: summary,
                tool_calls: None,
                tool_call_id: None,
                tool_name: None,
                tool_arguments: None,
                tool_status: None,
                input_tokens: 0,
                output_tokens: 0,
                provider: None,
                model: None,
                api_transport: None,
                response_id: None,
                reasoning: None,
                timestamp: messages[split_at - 1].timestamp,
            });
        }
    }

    compacted.extend(messages[split_at..].iter().cloned());
    trim_compacted_messages(&mut compacted, max_messages, target_tokens, has_summary);
    compacted
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
        total_tokens -= estimate_single_message_tokens(&messages[remove_index]);
        messages.remove(remove_index);
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
    let context_window = provider_config
        .context_window_tokens
        .max(config.context_window_tokens)
        .max(1) as usize;
    let threshold_pct = config.compact_threshold_pct.clamp(1, 100) as usize;
    let threshold_target = context_window.saturating_mul(threshold_pct) / 100;
    let configured_budget = config
        .context_budget_tokens
        .max(MIN_CONTEXT_TARGET_TOKENS as u32) as usize;
    threshold_target
        .max(MIN_CONTEXT_TARGET_TOKENS)
        .min(configured_budget)
}

pub(super) fn estimate_message_tokens(messages: &[AgentMessage]) -> usize {
    messages.iter().map(estimate_single_message_tokens).sum()
}

pub(super) fn estimate_single_message_tokens(message: &AgentMessage) -> usize {
    let mut chars = message.content.chars().count();

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

fn build_compaction_summary(messages: &[AgentMessage], target_tokens: usize) -> String {
    if messages.is_empty() {
        return String::new();
    }

    let max_chars = (target_tokens / 8)
        .saturating_mul(APPROX_CHARS_PER_TOKEN)
        .clamp(512, 4096);
    let mut summary = String::from(
        "[Compacted earlier context]\nSummary of older messages retained for continuity:\n",
    );

    for (index, message) in messages.iter().enumerate() {
        let line = format!("- {}\n", summarize_compacted_message(message));
        if summary.len() + line.len() > max_chars {
            let omitted = messages.len().saturating_sub(index);
            if omitted > 0 {
                summary.push_str(&format!("- ... {} earlier messages omitted\n", omitted));
            }
            break;
        }
        summary.push_str(&line);
    }

    summary
}

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

    let content = super::goal_parsing::summarize_text(&message.content, 160);
    if details.is_empty() {
        format!("{role}: {content}")
    } else {
        format!("{role} [{details}]: {content}")
    }
}
