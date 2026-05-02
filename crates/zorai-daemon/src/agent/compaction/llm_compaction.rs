use super::*;

pub(crate) fn select_compaction_transport(
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

    if let Some(fixed_transport) =
        fixed_api_transport_for_model(provider_id, &provider_config.model)
    {
        return fixed_transport;
    }

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

pub(crate) fn message_has_contentful_dialogue(message: &AgentMessage) -> bool {
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

pub(crate) fn sanitize_recent_compaction_message(message: &AgentMessage) -> AgentMessage {
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

pub(crate) fn select_recent_llm_compaction_messages(
    messages: &[AgentMessage],
) -> (usize, Vec<AgentMessage>) {
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

pub(crate) fn build_llm_compaction_messages(
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
                reasoning: None,
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
        reasoning: None,
        tool_call_id: None,
        name: None,
        tool_calls: None,
    });
    trim_llm_compaction_messages_to_fit(&mut api_messages, max_input_tokens.max(1));
    api_messages
}

pub(crate) fn llm_compaction_input_budget(
    provider_id: &str,
    provider_config: &ProviderConfig,
) -> usize {
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

pub(crate) fn estimate_api_messages_tokens(messages: &[ApiMessage]) -> usize {
    messages.iter().map(estimate_api_message_tokens).sum()
}

pub(crate) fn estimate_api_message_tokens(message: &ApiMessage) -> usize {
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

pub(crate) fn trim_llm_compaction_messages_to_fit(
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

pub(crate) fn truncate_api_message_to_fit(message: &mut ApiMessage, max_tokens: usize) {
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
