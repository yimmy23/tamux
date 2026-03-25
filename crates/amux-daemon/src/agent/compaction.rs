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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct CompactionCandidate {
    pub split_at: usize,
    pub target_tokens: usize,
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

pub(super) fn compact_messages_for_request(
    messages: &[AgentMessage],
    config: &AgentConfig,
    provider_config: &ProviderConfig,
) -> Vec<AgentMessage> {
    let Some(candidate) = compaction_candidate(messages, config, provider_config) else {
        // Even when compaction is disabled, enforce a hard token limit
        // so we never exceed the model's context window.
        let model_window = model_context_window(
            &config.provider,
            &provider_config.model,
            provider_config.context_window_tokens.max(config.context_window_tokens),
        ) as usize;
        let current = estimate_message_tokens(messages);
        if current > model_window {
            return hard_truncate_to_fit(messages, model_window);
        }
        return messages.to_vec();
    };
    let max_messages = config.max_context_messages.max(1) as usize;
    let split_at = candidate.split_at;
    let mut compacted = Vec::new();
    let mut has_summary = false;

    if split_at > 0 {
        let summary = build_compaction_summary(&messages[..split_at], candidate.target_tokens);
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
    if messages.is_empty() || !config.auto_compact_context {
        return None;
    }

    let max_messages = config.max_context_messages.max(1) as usize;
    let target_tokens = effective_context_target_tokens(config, provider_config);
    if messages.len() <= max_messages && estimate_message_tokens(messages) <= target_tokens {
        return None;
    }

    let keep_recent = config
        .keep_recent_on_compact
        .max(1)
        .min(messages.len() as u32) as usize;
    let mut split_at = messages.len().saturating_sub(keep_recent);
    if split_at == 0 {
        return None;
    }

    // Never split inside a tool-call / tool-result pair.
    // If the first kept message is a tool result, move split_at back to include
    // the assistant message that made the tool call.
    while split_at > 0 && messages[split_at].role == MessageRole::Tool {
        split_at -= 1;
    }
    if split_at == 0 {
        return None;
    }

    Some(CompactionCandidate {
        split_at,
        target_tokens,
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

pub(super) fn build_compaction_summary(messages: &[AgentMessage], target_tokens: usize) -> String {
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

#[cfg(test)]
mod tests {
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
