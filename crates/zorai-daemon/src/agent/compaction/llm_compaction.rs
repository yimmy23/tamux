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
            !compaction_summary_content(message)
                .as_ref()
                .trim()
                .is_empty()
                || message
                    .tool_calls
                    .as_ref()
                    .is_some_and(|tool_calls| !tool_calls.is_empty())
        }
        MessageRole::User | MessageRole::System => !compaction_summary_content(message)
            .as_ref()
            .trim()
            .is_empty(),
    }
}

pub(crate) fn materialize_llm_compaction_source_message(message: &AgentMessage) -> AgentMessage {
    materialize_compaction_message(message)
}

pub(crate) fn project_recent_compaction_message(message: &AgentMessage) -> AgentMessage {
    let mut projected = materialize_compaction_message(message);
    match projected.role {
        MessageRole::Assistant => {
            if let Some(tool_calls) = &projected.tool_calls {
                let tool_names = tool_calls
                    .iter()
                    .map(|call| call.function.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ");
                let content = projected.content.trim();
                projected.content = match (content.is_empty(), tool_names.is_empty()) {
                    (_, true) => content.to_string(),
                    (true, false) => format!("Assistant requested tools: {tool_names}"),
                    (false, false) => format!("{content}\nTools used: {tool_names}"),
                };
                projected.tool_calls = None;
            }
        }
        MessageRole::Tool => {
            projected.content = projected
                .tool_name
                .clone()
                .filter(|name| !name.trim().is_empty())
                .unwrap_or_else(|| "tool".to_string());
            projected.tool_arguments = None;
            projected.tool_status = None;
        }
        MessageRole::System | MessageRole::User => {}
    }
    projected
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
        .map(project_recent_compaction_message)
        .collect::<Vec<_>>();
    (start_index, recent)
}

pub(crate) fn build_llm_compaction_messages(
    messages: &[AgentMessage],
    target_tokens: usize,
    max_input_tokens: usize,
) -> Vec<ApiMessage> {
    build_llm_compaction_messages_with_scope(messages, target_tokens, max_input_tokens, None)
}

pub(crate) fn build_llm_compaction_messages_with_scope(
    messages: &[AgentMessage],
    target_tokens: usize,
    max_input_tokens: usize,
    scope: Option<&CompactionScopeSnapshot>,
) -> Vec<ApiMessage> {
    let source_messages = materialize_compaction_messages_with_scope(messages, scope);
    let packet = build_compaction_input_packet_markdown(&source_messages, target_tokens, scope);

    let mut api_messages = vec![ApiMessage {
        role: "user".to_string(),
        content: ApiContent::Text(packet),
        reasoning: None,
        tool_call_id: None,
        name: None,
        tool_calls: None,
    }];
    trim_llm_compaction_messages_to_fit(&mut api_messages, max_input_tokens.max(1));
    api_messages
}

pub(crate) fn build_compaction_input_packet_markdown(
    messages: &[AgentMessage],
    _target_tokens: usize,
    scope: Option<&CompactionScopeSnapshot>,
) -> String {
    let scope_packet = render_compaction_scope_packet(scope)
        .unwrap_or_else(|| "Compaction Scope Packet\n- thread_id: `unknown`".to_string());
    let authoritative_state = packet_authoritative_task_state(messages, scope);
    let tool_evidence = packet_tool_evidence_pointers(messages);
    let role_evidence = messages
        .iter()
        .enumerate()
        .map(|(index, message)| packet_role_labeled_turn(index + 1, message))
        .collect::<Vec<_>>()
        .join("\n\n");

    format!(
        "# Compaction Input Packet\n\n## Output Contract\nReturn exactly one markdown checkpoint matching this schema:\n\n{}\n\n## Scope Identity\n{}\n\n## Authoritative Task State\n{}\n\n## Tool Evidence Pointers\n{}\n\n## Role-Labeled Conversation Evidence\n{}\n\n## Compression Instructions\nPreserve scope identity, latest user intent, current task state, decisions, blockers, verification state, raw evidence pointers, and the immediate next action. Treat global tool listings and offloaded payload reads as evidence pointers, not active objectives. Do not replay the conversation; reconstruct the smallest high-signal checkpoint state.\n",
        COMPACTION_CHECKPOINT_SCHEMA,
        scope_packet,
        authoritative_state,
        tool_evidence,
        role_evidence,
    )
}

pub(crate) fn packet_authoritative_task_state(
    messages: &[AgentMessage],
    scope: Option<&CompactionScopeSnapshot>,
) -> String {
    if let Some(scope) = scope {
        let mut lines = Vec::new();
        if let Some(goal) = scope
            .goal
            .as_deref()
            .filter(|value| !value.trim().is_empty())
        {
            lines.push(format!(
                "- objective: {}",
                crate::agent::goal_parsing::summarize_text(goal, 260)
            ));
        }
        if let Some(step) = scope
            .current_step_title
            .as_deref()
            .filter(|value| !value.trim().is_empty())
        {
            lines.push(format!("- current_step: `{step}`"));
        }
        if let Some(status) = scope
            .current_step_status
            .as_deref()
            .filter(|value| !value.trim().is_empty())
        {
            lines.push(format!("- current_step_status: `{status}`"));
        }
        if let Some(summary) = scope
            .current_step_summary
            .as_deref()
            .filter(|value| !value.trim().is_empty())
        {
            lines.push(format!(
                "- current_step_summary: {}",
                crate::agent::goal_parsing::summarize_text(summary, 240)
            ));
        }
        if let Some(plan) = scope
            .plan_summary
            .as_deref()
            .filter(|value| !value.trim().is_empty())
        {
            lines.push(format!(
                "- plan_summary: {}",
                crate::agent::goal_parsing::summarize_text(plan, 240)
            ));
        }
        if let Some(error) = scope
            .latest_error
            .as_deref()
            .filter(|value| !value.trim().is_empty())
        {
            lines.push(format!(
                "- latest_error: {}",
                crate::agent::goal_parsing::summarize_text(error, 220)
            ));
        }
        if !lines.is_empty() {
            return lines.join("\n");
        }
    }

    let latest_user = messages
        .iter()
        .rev()
        .find(|message| message.role == MessageRole::User)
        .map(|message| {
            crate::agent::goal_parsing::summarize_text(compaction_runtime_content(message), 260)
        });
    let latest_non_user = messages
        .iter()
        .rev()
        .find(|message| message.role != MessageRole::User)
        .map(|message| {
            crate::agent::goal_parsing::summarize_text(&summarize_compacted_message(message), 260)
        });

    let mut lines = Vec::new();
    if let Some(latest_user) = latest_user {
        lines.push(format!("- latest_user_intent: {latest_user}"));
    }
    if let Some(latest_non_user) = latest_non_user {
        lines.push(format!("- latest_non_user_state: {latest_non_user}"));
    }
    if lines.is_empty() {
        "- no authoritative task state captured".to_string()
    } else {
        lines.join("\n")
    }
}

pub(crate) fn packet_tool_evidence_pointers(messages: &[AgentMessage]) -> String {
    let pointers = messages
        .iter()
        .filter(|message| message.role == MessageRole::Tool)
        .filter_map(|message| {
            let content = compaction_runtime_content(message);
            content.starts_with("Tool Evidence Pointer").then(|| {
                format!(
                    "- {}",
                    crate::agent::goal_parsing::summarize_text(content, 320)
                )
            })
        })
        .collect::<Vec<_>>();

    if pointers.is_empty() {
        "- none captured".to_string()
    } else {
        pointers.join("\n")
    }
}

pub(crate) fn packet_role_labeled_turn(index: usize, message: &AgentMessage) -> String {
    let role = match message.role {
        MessageRole::System => "system",
        MessageRole::User => "user",
        MessageRole::Assistant => "assistant",
        MessageRole::Tool => "tool",
    };
    let mut lines = vec![format!("### Turn {index}"), format!("- role: {role}")];

    if let Some(tool_name) = message
        .tool_name
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        lines.push(format!("- tool: `{tool_name}`"));
    }
    if let Some(status) = message
        .tool_status
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        lines.push(format!("- status: `{status}`"));
    }
    if let Some(call_id) = message
        .tool_call_id
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        lines.push(format!("- call_id: `{call_id}`"));
    }
    if let Some(tool_calls) = &message.tool_calls {
        let tool_names = tool_calls
            .iter()
            .map(|call| call.function.name.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        if !tool_names.is_empty() {
            lines.push(format!("- requested_tools: `{tool_names}`"));
        }
    }

    let content =
        crate::agent::goal_parsing::summarize_text(compaction_runtime_content(message), 700);
    if !content.trim().is_empty() {
        lines.push(format!("- content: {content}"));
    }

    lines.join("\n")
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
    if api_messages.is_empty() {
        return;
    }

    if api_messages.len() == 1 {
        truncate_single_packet_api_message_to_fit(&mut api_messages[0], max_input_tokens);
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

pub(crate) fn truncate_single_packet_api_message_to_fit(
    message: &mut ApiMessage,
    max_tokens: usize,
) {
    message.tool_call_id = None;
    message.name = None;
    message.tool_calls = None;

    if estimate_api_message_tokens(message) <= max_tokens {
        return;
    }

    let ApiContent::Text(text) = &mut message.content else {
        return;
    };
    let max_chars = max_tokens
        .saturating_sub(24)
        .max(64)
        .saturating_mul(APPROX_CHARS_PER_TOKEN);
    let notice = COMPACTION_MESSAGE_TRUNCATION_NOTICE;
    if text.chars().count() <= max_chars {
        return;
    }

    let available = max_chars.saturating_sub(notice.chars().count()).max(64);
    let head_chars = available / 2;
    let tail_chars = available.saturating_sub(head_chars);
    let head = text.chars().take(head_chars).collect::<String>();
    let tail = text
        .chars()
        .rev()
        .take(tail_chars)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<String>();
    *text = format!("{head}{notice}{tail}");
}
