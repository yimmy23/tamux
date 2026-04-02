//! Context compaction — token-aware message compression for LLM requests.

use super::*;
use super::llm_client::messages_to_api_format;

const HEURISTIC_COMPACTION_VISIBLE_TEXT: &str = "rule based";
const COMPACTION_NOTICE_KIND: &str = "auto-compaction";
const COMPACTION_EXACT_MESSAGE_MAX: usize = 24;
const COMPACTION_MODEL_SYSTEM_PROMPT: &str = "You compress older conversation context for future continuity. Preserve goals, constraints, decisions, tool outcomes, unresolved issues, and important factual state. Return only the compacted carry-forward text as plain text. Do not add markdown fences, headings, or commentary about the compaction process.";

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

fn active_request_messages(messages: &[AgentMessage]) -> Vec<AgentMessage> {
    let (_, active_messages) = active_compaction_window(messages);
    active_messages
        .iter()
        .map(materialize_compaction_message)
        .collect()
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
                        if config.provider == "github-copilot"
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
                        let trailing_messages = messages_to_api_format(trailing);
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
    let runtime_messages = active_request_messages(messages);
    let Some(candidate) = compaction_candidate(messages, config, provider_config) else {
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
        let summary = build_compaction_summary(&runtime_messages[..split_at], candidate.target_tokens);
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
                provider: None,
                model: None,
                api_transport: None,
                response_id: None,
                reasoning: None,
                message_kind: AgentMessageKind::Normal,
                compaction_strategy: None,
                compaction_payload: None,
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
    let (_, active_messages) = active_compaction_window(messages);
    if active_messages.is_empty() || !config.auto_compact_context {
        return None;
    }

    let max_messages = config.max_context_messages.max(1) as usize;
    let target_tokens = effective_context_target_tokens(config, provider_config);
    if active_messages.len() <= max_messages && estimate_message_tokens(active_messages) <= target_tokens {
        return None;
    }

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

fn select_compaction_transport(provider_id: &str, provider_config: &ProviderConfig) -> ApiTransport {
    if provider_id == "openai" && provider_config.auth_source == AuthSource::ChatgptSubscription {
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

fn build_llm_compaction_messages(
    messages: &[AgentMessage],
    target_tokens: usize,
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
        vec![ApiMessage {
            role: "user".to_string(),
            content: ApiContent::Text(format!(
                "Older context to compact:\n\n{}",
                build_compaction_summary(&source_messages, target_tokens.saturating_mul(2))
            )),
            tool_call_id: None,
            name: None,
            tool_calls: None,
        }]
    };

    api_messages.push(ApiMessage {
        role: "user".to_string(),
        content: ApiContent::Text(
            "Compress the supplied older context into a concise carry-forward note. Preserve requests, constraints, decisions, tool outcomes, errors worth remembering, and any unresolved next steps. Return only the compacted text."
                .to_string(),
        ),
        tool_call_id: None,
        name: None,
        tool_calls: None,
    });
    api_messages
}

impl AgentEngine {
    pub(super) async fn maybe_persist_compaction_artifact(
        &self,
        thread_id: &str,
        task_id: Option<&str>,
        config: &AgentConfig,
        provider_config: &ProviderConfig,
    ) -> Result<bool> {
        let snapshot = {
            let threads = self.threads.read().await;
            threads.get(thread_id).cloned()
        };
        let Some(thread) = snapshot else {
            return Ok(false);
        };
        let (window_start, _) = active_compaction_window(&thread.messages);
        let Some(candidate) = compaction_candidate(&thread.messages, config, provider_config) else {
            return Ok(false);
        };
        let split_at = window_start + candidate.split_at;
        let source_messages = thread.messages[window_start..split_at].to_vec();
        let message_count = thread.messages.len();

        let (artifact, strategy_used, fallback_notice) = self
            .build_compaction_artifact(&source_messages, candidate.target_tokens, config)
            .await?;

        {
            let mut threads = self.threads.write().await;
            let Some(thread) = threads.get_mut(thread_id) else {
                return Ok(false);
            };
            let (window_start, _) = active_compaction_window(&thread.messages);
            let Some(current_candidate) =
                compaction_candidate(&thread.messages, config, provider_config)
            else {
                return Ok(false);
            };
            let current_split_at = window_start + current_candidate.split_at;
            thread.messages.insert(current_split_at, artifact);
            thread.updated_at = now_millis();
        }

        self.persist_thread_by_id(thread_id).await;
        self.record_provenance_event(
            "context_compressed",
            "thread context was compacted for an LLM request",
            serde_json::json!({
                "thread_id": thread_id,
                "split_at": split_at,
                "target_tokens": candidate.target_tokens,
                "message_count": message_count,
                "strategy": strategy_used,
            }),
            None,
            task_id,
            Some(thread_id),
            None,
            None,
        )
        .await;
        let _ = self.event_tx.send(AgentEvent::ThreadReloadRequired {
            thread_id: thread_id.to_string(),
        });
        let _ = self.event_tx.send(AgentEvent::WorkflowNotice {
            thread_id: thread_id.to_string(),
            kind: COMPACTION_NOTICE_KIND.to_string(),
            message: format!(
                "Auto compaction applied using {}.",
                serde_json::to_string(&strategy_used)
                    .unwrap_or_else(|_| "\"heuristic\"".to_string())
                    .trim_matches('"')
            ),
            details: None,
        });
        if let Some(fallback_notice) = fallback_notice {
            let _ = self.event_tx.send(AgentEvent::WorkflowNotice {
                thread_id: thread_id.to_string(),
                kind: COMPACTION_NOTICE_KIND.to_string(),
                message: fallback_notice,
                details: None,
            });
        }
        Ok(true)
    }

    async fn build_compaction_artifact(
        &self,
        messages: &[AgentMessage],
        target_tokens: usize,
        config: &AgentConfig,
    ) -> Result<(AgentMessage, CompactionStrategy, Option<String>)> {
        let heuristic_payload = build_compaction_summary(messages, target_tokens);
        let heuristic_payload = if heuristic_payload.trim().is_empty() {
            "Older context compacted for continuity.".to_string()
        } else {
            heuristic_payload
        };

        let mut strategy_used = config.compaction.strategy;
        let mut fallback_notice = None;
        let payload = match strategy_used {
            CompactionStrategy::Heuristic => heuristic_payload.clone(),
            CompactionStrategy::Weles => {
                let (provider_id, provider_config) = self.resolve_weles_compaction_provider(config)?;
                match self
                    .run_llm_compaction(&provider_id, &provider_config, messages, target_tokens)
                    .await
                {
                    Ok(payload) if !payload.trim().is_empty() => payload,
                    Ok(_) | Err(_) => {
                        strategy_used = CompactionStrategy::Heuristic;
                        fallback_notice =
                            Some("WELES compaction failed; fell back to rule based compaction.".to_string());
                        heuristic_payload.clone()
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
                        fallback_notice = Some(
                            "Custom-model compaction failed; fell back to rule based compaction."
                                .to_string(),
                        );
                        heuristic_payload.clone()
                    }
                }
            }
        };

        let visible_content = match strategy_used {
            CompactionStrategy::Heuristic => HEURISTIC_COMPACTION_VISIBLE_TEXT.to_string(),
            CompactionStrategy::Weles | CompactionStrategy::CustomModel => payload.clone(),
        };

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
                provider: None,
                model: None,
                api_transport: None,
                response_id: None,
                reasoning: None,
                message_kind: AgentMessageKind::CompactionArtifact,
                compaction_strategy: Some(strategy_used),
                compaction_payload: Some(payload),
                timestamp: messages.last().map(|message| message.timestamp).unwrap_or_else(now_millis),
            },
            strategy_used,
            fallback_notice,
        ))
    }

    async fn run_llm_compaction(
        &self,
        provider_id: &str,
        provider_config: &ProviderConfig,
        messages: &[AgentMessage],
        target_tokens: usize,
    ) -> Result<String> {
        let transport = select_compaction_transport(provider_id, provider_config);
        let api_messages = build_llm_compaction_messages(messages, target_tokens);
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
        let provider_id = config
            .compaction
            .weles
            .provider
            .trim()
            .to_string();
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
