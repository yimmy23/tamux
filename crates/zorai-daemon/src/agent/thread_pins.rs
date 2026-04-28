use super::*;
use serde::{Deserialize, Serialize};

const PINNED_CONTEXT_BUDGET_DENOMINATOR: usize = 4;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ThreadMessagePinMutationResult {
    pub ok: bool,
    pub thread_id: String,
    pub message_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub current_pinned_chars: usize,
    pub pinned_budget_chars: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub candidate_pinned_chars: Option<usize>,
}

impl ThreadMessagePinMutationResult {
    pub(crate) fn success(
        thread_id: &str,
        message_id: &str,
        current_pinned_chars: usize,
        pinned_budget_chars: usize,
    ) -> Self {
        Self {
            ok: true,
            thread_id: thread_id.to_string(),
            message_id: message_id.to_string(),
            error: None,
            current_pinned_chars,
            pinned_budget_chars,
            candidate_pinned_chars: None,
        }
    }

    pub(crate) fn failure(
        thread_id: &str,
        message_id: &str,
        error: impl Into<String>,
        current_pinned_chars: usize,
        pinned_budget_chars: usize,
        candidate_pinned_chars: Option<usize>,
    ) -> Self {
        Self {
            ok: false,
            thread_id: thread_id.to_string(),
            message_id: message_id.to_string(),
            error: Some(error.into()),
            current_pinned_chars,
            pinned_budget_chars,
            candidate_pinned_chars,
        }
    }
}

pub(crate) fn pinned_for_compaction_message_chars(message: &AgentMessage) -> usize {
    message.content.chars().count()
}

pub(crate) fn pinned_for_compaction_budget_chars(
    config: &AgentConfig,
    provider_config: &ProviderConfig,
) -> usize {
    model_context_window(
        &config.provider,
        &provider_config.model,
        provider_config
            .context_window_tokens
            .max(config.context_window_tokens),
    )
    .saturating_mul(APPROX_CHARS_PER_TOKEN as u32)
    .saturating_div(PINNED_CONTEXT_BUDGET_DENOMINATOR as u32) as usize
}

pub(crate) fn pinned_for_compaction_chars_used(thread: &AgentThread) -> usize {
    thread
        .messages
        .iter()
        .filter(|message| message.pinned_for_compaction && !message_is_compaction_summary(message))
        .map(pinned_for_compaction_message_chars)
        .sum()
}

pub(crate) fn owner_only_pins(thread: &AgentThread) -> Vec<AgentMessage> {
    if current_agent_scope_id() != MAIN_AGENT_ID {
        return Vec::new();
    }

    thread
        .messages
        .iter()
        .filter(|message| message.pinned_for_compaction && !message_is_compaction_summary(message))
        .cloned()
        .collect()
}

pub(crate) fn owner_only_pins_within_budget(
    thread: &AgentThread,
    config: &AgentConfig,
    provider_config: &ProviderConfig,
) -> Vec<AgentMessage> {
    let budget_chars = pinned_for_compaction_budget_chars(config, provider_config);
    let mut used_chars = 0usize;
    let mut included = Vec::new();

    for message in owner_only_pins(thread) {
        let message_chars = pinned_for_compaction_message_chars(&message);
        if used_chars.saturating_add(message_chars) > budget_chars {
            break;
        }
        used_chars = used_chars.saturating_add(message_chars);
        included.push(message);
    }

    included
}

pub(crate) fn pin_thread_message_for_compaction(
    thread: &mut AgentThread,
    message_id: &str,
    config: &AgentConfig,
    provider_config: &ProviderConfig,
) -> ThreadMessagePinMutationResult {
    let budget_chars = pinned_for_compaction_budget_chars(config, provider_config);
    let current_chars = pinned_for_compaction_chars_used(thread);

    let Some(message_index) = thread
        .messages
        .iter()
        .position(|message| message.id == message_id)
    else {
        return ThreadMessagePinMutationResult::failure(
            &thread.id,
            message_id,
            "message_not_found",
            current_chars,
            budget_chars,
            None,
        );
    };

    if thread.messages[message_index].pinned_for_compaction {
        return ThreadMessagePinMutationResult::success(
            &thread.id,
            message_id,
            current_chars,
            budget_chars,
        );
    }

    let candidate_chars = current_chars.saturating_add(pinned_for_compaction_message_chars(
        &thread.messages[message_index],
    ));
    if candidate_chars > budget_chars {
        return ThreadMessagePinMutationResult::failure(
            &thread.id,
            message_id,
            "pinned_budget_exceeded",
            current_chars,
            budget_chars,
            Some(candidate_chars),
        );
    }

    thread.messages[message_index].pinned_for_compaction = true;
    ThreadMessagePinMutationResult::success(&thread.id, message_id, candidate_chars, budget_chars)
}

pub(crate) fn unpin_thread_message_for_compaction(
    thread: &mut AgentThread,
    message_id: &str,
    config: &AgentConfig,
    provider_config: &ProviderConfig,
) -> ThreadMessagePinMutationResult {
    let budget_chars = pinned_for_compaction_budget_chars(config, provider_config);
    let Some(message_index) = thread
        .messages
        .iter()
        .position(|message| message.id == message_id)
    else {
        return ThreadMessagePinMutationResult::failure(
            &thread.id,
            message_id,
            "message_not_found",
            pinned_for_compaction_chars_used(thread),
            budget_chars,
            None,
        );
    };

    thread.messages[message_index].pinned_for_compaction = false;
    ThreadMessagePinMutationResult::success(
        &thread.id,
        message_id,
        pinned_for_compaction_chars_used(thread),
        budget_chars,
    )
}
