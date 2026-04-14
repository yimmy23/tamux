#![allow(dead_code)]

use super::*;

fn normalize_no_suggestion_candidate(value: &str) -> String {
    let mut current = value.trim();
    loop {
        let trimmed = current.trim_start_matches(|c: char| {
            c.is_whitespace() || matches!(c, '*' | '`' | '_' | '-' | '•' | '>' | '#')
        });
        let lowercase = trimmed.to_ascii_lowercase();
        let Some(colon_index) = lowercase.find(':') else {
            current = trimmed;
            break;
        };
        let label = lowercase[..colon_index].trim();
        if matches!(label, "response" | "answer" | "result" | "final" | "status") {
            current = &trimmed[colon_index + 1..];
            continue;
        }
        current = trimmed;
        break;
    }

    current
        .trim_matches(|c: char| {
            c.is_whitespace()
                || matches!(
                    c,
                    '*' | '`' | '_' | '"' | '\'' | '[' | ']' | '(' | ')' | '.'
                )
        })
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == '_')
        .collect::<String>()
        .to_ascii_lowercase()
}

pub(super) fn participant_response_is_no_suggestion(value: &str) -> bool {
    matches!(
        normalize_no_suggestion_candidate(value).as_str(),
        "nosuggestion" | "no_suggestion"
    )
}

pub(super) fn parse_participant_suggestion_response(response: &str) -> Option<(bool, String)> {
    let trimmed = response.trim();
    if trimmed.is_empty() || participant_response_is_no_suggestion(trimmed) {
        return None;
    }

    let mut force_send = false;
    let mut message_line: Option<String> = None;
    for line in trimmed.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("FORCE:") {
            force_send = matches!(rest.trim().to_ascii_lowercase().as_str(), "yes" | "true");
            continue;
        }
        if let Some(rest) = line.strip_prefix("MESSAGE:") {
            let message = rest.trim();
            if !message.is_empty() {
                message_line = Some(message.to_string());
            }
        }
    }

    if let Some(message) = message_line {
        if participant_response_is_no_suggestion(&message) {
            return None;
        }
        return Some((force_send, message));
    }

    if trimmed
        .lines()
        .any(|line| participant_response_is_no_suggestion(line))
    {
        return None;
    }

    Some((force_send, trimmed.to_string()))
}

fn should_hide_participant_prompt_message(message: &AgentMessage) -> bool {
    matches!(message.role, MessageRole::System | MessageRole::Tool)
        || message
            .tool_name
            .as_deref()
            .map(|name| name == "internal_delegate")
            .unwrap_or(false)
}

fn latest_visible_message_allows_participant_observers(
    visible_messages: &[AgentMessage],
    participants: &[ThreadParticipantState],
) -> bool {
    let Some(latest_message) = visible_messages.last() else {
        return false;
    };

    match latest_message.role {
        MessageRole::User => true,
        MessageRole::Assistant => latest_message
            .author_agent_id
            .as_ref()
            .is_none_or(|author_id| {
                !participants
                    .iter()
                    .any(|participant| participant.agent_id.eq_ignore_ascii_case(author_id))
            }),
        MessageRole::System | MessageRole::Tool => false,
    }
}

fn build_participant_prompt_from_snapshot(
    participant: &ThreadParticipantState,
    visible_messages: &[AgentMessage],
) -> String {
    let mut prompt = String::new();
    prompt.push_str("Role: participant observer\n");
    prompt.push_str(&format!("Participant: {}\n", participant.agent_name));
    prompt.push_str(&format!("Instruction: {}\n\n", participant.instruction));
    prompt.push_str("Respond with either:\n");
    prompt.push_str("- NO_SUGGESTION\n");
    prompt.push_str("- FORCE: yes|no\n  MESSAGE: <text>\n\n");
    prompt.push_str(
        "Evaluate the thread after the latest visible message, even if that message is from the assistant.\n",
    );
    prompt.push_str(
        "If the operator asked for autonomous progress and work is still pending, you should suggest the next concrete participant action instead of NO_SUGGESTION.\n",
    );
    prompt.push_str(
        "If the latest assistant message proposes next steps, ongoing work, or an unfinished plan, continue that flow when your participant role can help.\n",
    );
    prompt.push_str(
        "Return NO_SUGGESTION only when the visible thread is naturally complete, blocked on external input, or the participant truly has nothing useful to add.\n\n",
    );
    prompt.push_str("Visible thread:\n");
    for message in visible_messages {
        let role = match message.role {
            MessageRole::Assistant => "assistant",
            MessageRole::Tool => "tool",
            MessageRole::System => "system",
            MessageRole::User => "user",
        };
        if !message.content.trim().is_empty() {
            prompt.push_str(&format!("- {role}: {}\n", message.content.trim()));
        }
    }
    prompt
}

fn build_visible_participant_message_prompt(
    participant: &ThreadParticipantState,
    visible_messages: &[AgentMessage],
    request: &str,
) -> String {
    let mut prompt = String::new();
    prompt.push_str("Role: visible thread participant\n");
    prompt.push_str(&format!("Participant: {}\n", participant.agent_name));
    prompt.push_str(&format!(
        "Participant registration instruction: {}\n\n",
        participant.instruction
    ));
    prompt.push_str("Generate the one visible thread message you want to post now.\n");
    prompt.push_str("Respond with only the exact message text to post.\n");
    prompt.push_str("Do not include analysis, XML, tool calls, or extra framing.\n\n");
    prompt.push_str("Current request:\n");
    prompt.push_str(request.trim());
    prompt.push_str("\n\nVisible thread:\n");
    for message in visible_messages {
        let role = match message.role {
            MessageRole::Assistant => "assistant",
            MessageRole::Tool => "tool",
            MessageRole::System => "system",
            MessageRole::User => "user",
        };
        if !message.content.trim().is_empty() {
            prompt.push_str(&format!("- {role}: {}\n", message.content.trim()));
        }
    }
    prompt
}

fn trim_participant_playground_thread_messages(
    thread: &mut AgentThread,
    max_messages: usize,
    updated_at: u64,
) -> bool {
    if thread.messages.len() <= max_messages {
        return false;
    }

    let drop_count = thread.messages.len().saturating_sub(max_messages);
    thread.messages.drain(0..drop_count);
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
    thread.updated_at = updated_at;
    true
}

#[derive(Clone)]
struct ParticipantObserverResponderConfig {
    provider_id: String,
    provider_config: ProviderConfig,
    base_prompt: String,
}

impl AgentEngine {
    async fn participant_playground_message_limit(&self) -> usize {
        let config = self.config.read().await;
        config
            .max_context_messages
            .max(config.keep_recent_on_compact)
            .max(1) as usize
    }

    async fn trim_participant_playground_thread_by_id(&self, thread_id: &str) -> bool {
        if !crate::agent::agent_identity::is_participant_playground_thread(thread_id) {
            return false;
        }

        let max_messages = self.participant_playground_message_limit().await;
        let trimmed = {
            let mut threads = self.threads.write().await;
            let Some(thread) = threads.get_mut(thread_id) else {
                return false;
            };
            trim_participant_playground_thread_messages(thread, max_messages, now_millis())
        };

        if trimmed {
            self.persist_thread_by_id(thread_id).await;
        }

        trimmed
    }

    pub(crate) async fn restore_participant_observer_state_after_hydrate(&self) {
        let thread_ids = {
            let participants = self.thread_participants.read().await;
            participants.keys().cloned().collect::<Vec<_>>()
        };

        for thread_id in thread_ids {
            if crate::agent::agent_identity::is_participant_playground_thread(&thread_id) {
                continue;
            }
            if let Err(error) = self.run_participant_observers(&thread_id).await {
                tracing::warn!(
                    thread_id = %thread_id,
                    %error,
                    "failed to restore participant observer state during hydrate"
                );
            }
        }
    }

    pub(crate) async fn trim_persisted_participant_playground_threads_on_hydrate(&self) -> usize {
        let max_messages = self.participant_playground_message_limit().await;
        let trimmed_thread_ids = {
            let mut threads = self.threads.write().await;
            let updated_at = now_millis();
            let mut trimmed = Vec::new();

            for (thread_id, thread) in threads.iter_mut() {
                if !crate::agent::agent_identity::is_participant_playground_thread(thread_id) {
                    continue;
                }
                if trim_participant_playground_thread_messages(thread, max_messages, updated_at) {
                    trimmed.push(thread_id.clone());
                }
            }

            trimmed
        };

        for thread_id in &trimmed_thread_ids {
            self.persist_thread_by_id(thread_id).await;
        }

        trimmed_thread_ids.len()
    }

    pub(crate) async fn trim_participant_playground_threads_for_visible_thread(
        &self,
        visible_thread_id: &str,
    ) {
        let suffix = format!(":{visible_thread_id}");
        let playground_thread_ids = {
            let threads = self.threads.read().await;
            threads
                .keys()
                .filter(|thread_id| {
                    crate::agent::agent_identity::is_participant_playground_thread(thread_id)
                        && thread_id.ends_with(&suffix)
                })
                .cloned()
                .collect::<Vec<_>>()
        };

        if playground_thread_ids.is_empty() {
            return;
        }

        let max_messages = self.participant_playground_message_limit().await;
        let updated_at = now_millis();
        let mut trimmed_thread_ids = Vec::new();
        {
            let mut threads = self.threads.write().await;
            for thread_id in &playground_thread_ids {
                if let Some(thread) = threads.get_mut(thread_id) {
                    if trim_participant_playground_thread_messages(thread, max_messages, updated_at)
                    {
                        trimmed_thread_ids.push(thread_id.clone());
                    }
                }
            }
        }

        for thread_id in trimmed_thread_ids {
            self.persist_thread_by_id(&thread_id).await;
            let _ = self
                .event_tx
                .send(AgentEvent::ThreadReloadRequired { thread_id });
        }
    }

    async fn prepare_participant_playground_thread(
        &self,
        visible_thread_id: &str,
        target_agent_id: &str,
        wrapped_prompt: &str,
    ) -> String {
        let playground_thread_id =
            participant_playground_thread_id(visible_thread_id, target_agent_id);
        let _ = self
            .get_or_create_thread_with_target(
                Some(&playground_thread_id),
                wrapped_prompt,
                Some(target_agent_id),
            )
            .await;
        self.set_thread_handoff_state(
            &playground_thread_id,
            initial_thread_handoff_state(
                &playground_thread_id,
                Some(canonical_agent_name(target_agent_id)),
                now_millis(),
            ),
        )
        .await;
        self.ensure_thread_identity(
            &playground_thread_id,
            &participant_playground_thread_title(visible_thread_id, target_agent_id),
            false,
        )
        .await;
        playground_thread_id
    }

    async fn compact_participant_prompt_messages(
        &self,
        target_agent_id: &str,
        visible_messages: &[AgentMessage],
    ) -> Result<Vec<AgentMessage>> {
        let responder = self
            .participant_observer_responder_config(target_agent_id)
            .await?;
        let mut request_config = self.config.read().await.clone();
        request_config.provider = responder.provider_id;
        Ok(compact_messages_for_request(
            visible_messages,
            &request_config,
            &responder.provider_config,
        ))
    }

    async fn participant_observer_responder_config(
        &self,
        target_agent_id: &str,
    ) -> Result<ParticipantObserverResponderConfig> {
        let config = self.config.read().await.clone();
        let agent_scope_id = canonical_agent_id(target_agent_id).to_string();
        let sub_agents = self.list_sub_agents().await;

        if agent_scope_id == CONCIERGE_AGENT_ID {
            let provider_id = config
                .concierge
                .provider
                .as_deref()
                .unwrap_or(&config.provider)
                .to_string();
            let concierge_provider = crate::agent::concierge::resolve_concierge_provider(&config)?;
            let mut provider_config =
                crate::agent::concierge::fast_concierge_provider_config(&concierge_provider);
            provider_config.api_key = concierge_provider.api_key;
            provider_config.base_url = concierge_provider.base_url;
            provider_config.auth_source = concierge_provider.auth_source;
            provider_config.assistant_id = concierge_provider.assistant_id;
            provider_config.api_transport = concierge_provider.api_transport;

            return Ok(ParticipantObserverResponderConfig {
                provider_id,
                provider_config,
                base_prompt: crate::agent::concierge::concierge_system_prompt(),
            });
        }

        let matched_def = if agent_scope_id == WELES_AGENT_ID {
            sub_agents
                .iter()
                .find(|def| def.id == WELES_BUILTIN_SUBAGENT_ID)
                .cloned()
        } else {
            None
        };
        let builtin_persona_overrides = builtin_persona_overrides(&config, &agent_scope_id);
        if is_explicit_builtin_persona_scope(&agent_scope_id)
            && builtin_persona_requires_setup(&config, &agent_scope_id)
        {
            return Err(builtin_persona_setup_error(&agent_scope_id));
        }
        let persona_prompt = if agent_scope_id == WELES_AGENT_ID {
            build_weles_persona_prompt(WELES_GOVERNANCE_SCOPE)
        } else if agent_scope_id == MAIN_AGENT_ID {
            String::new()
        } else {
            build_spawned_persona_prompt(&agent_scope_id)
        };
        let provider_id = matched_def
            .as_ref()
            .map(|def| def.provider.clone())
            .filter(|value| !value.trim().is_empty())
            .or_else(|| {
                builtin_persona_overrides
                    .and_then(|overrides| overrides.provider.clone())
                    .filter(|value| !value.trim().is_empty())
            })
            .unwrap_or_else(|| config.provider.clone());
        let mut provider_config = if matched_def.is_some() || agent_scope_id != MAIN_AGENT_ID {
            self.resolve_sub_agent_provider_config(&config, &provider_id)?
        } else {
            self.resolve_provider_config(&config)?
        };
        if let Some(model) = matched_def
            .as_ref()
            .map(|def| def.model.clone())
            .filter(|value| !value.trim().is_empty())
            .or_else(|| {
                builtin_persona_overrides
                    .and_then(|overrides| overrides.model.clone())
                    .filter(|value| !value.trim().is_empty())
            })
        {
            provider_config.model = model;
        }
        if let Some(reasoning_effort) = matched_def
            .as_ref()
            .and_then(|def| def.reasoning_effort.clone())
            .filter(|value| !value.trim().is_empty())
            .or_else(|| {
                builtin_persona_overrides
                    .and_then(|overrides| overrides.reasoning_effort.clone())
                    .filter(|value| !value.trim().is_empty())
            })
        {
            provider_config.reasoning_effort = reasoning_effort;
        }
        let system_prompt = matched_def
            .as_ref()
            .and_then(|def| def.system_prompt.clone())
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| config.system_prompt.clone());
        let base_prompt = if persona_prompt.is_empty() {
            system_prompt
        } else {
            format!("{persona_prompt}\n\n{system_prompt}")
        };

        Ok(ParticipantObserverResponderConfig {
            provider_id,
            provider_config,
            base_prompt,
        })
    }

    async fn run_hidden_participant_prompt(
        &self,
        visible_thread_id: &str,
        target_agent_id: &str,
        prompt: &str,
    ) -> Result<String> {
        let target_agent_id = canonical_agent_id(target_agent_id).to_string();
        let prompt = prompt.trim().to_string();
        if prompt.is_empty() {
            anyhow::bail!("participant playground prompt cannot be empty");
        }
        let playground_thread_id = self
            .prepare_participant_playground_thread(visible_thread_id, &target_agent_id, &prompt)
            .await;
        let playground_thread_id_for_run = playground_thread_id.clone();
        let output = Box::pin(run_with_agent_scope(target_agent_id.clone(), async move {
            let outcome = self
                .run_internal_send_loop(
                    Some(&playground_thread_id_for_run),
                    &prompt,
                    &prompt,
                    None,
                    None,
                    None,
                    None,
                    false,
                    false,
                )
                .await?;
            self.threads
                .read()
                .await
                .get(&outcome.thread_id)
                .and_then(|thread| {
                    thread
                        .messages
                        .iter()
                        .rev()
                        .find(|message| {
                            message.role == MessageRole::Assistant
                                && message.tool_calls.as_ref().is_none_or(Vec::is_empty)
                        })
                        .and_then(|message| {
                            if !message.content.trim().is_empty() {
                                Some(message.content.trim().to_string())
                            } else {
                                message
                                    .reasoning
                                    .as_deref()
                                    .map(str::trim)
                                    .filter(|value| !value.is_empty())
                                    .map(str::to_string)
                            }
                        })
                })
                .ok_or_else(|| anyhow::anyhow!("participant playground returned empty output"))
        }))
        .await?;
        let _ = self
            .trim_participant_playground_thread_by_id(&playground_thread_id)
            .await;
        Ok(output)
    }

    pub async fn generate_visible_thread_participant_message(
        &self,
        thread_id: &str,
        target_agent_id: &str,
        request: &str,
    ) -> Result<String> {
        let participants = self.list_thread_participants(thread_id).await;
        let participant = participants
            .iter()
            .find(|participant| {
                participant.agent_id.eq_ignore_ascii_case(target_agent_id)
                    && participant.status == ThreadParticipantStatus::Active
            })
            .ok_or_else(|| {
                anyhow::anyhow!("participant is not active on thread: {target_agent_id}")
            })?;
        let thread = self
            .get_thread(thread_id)
            .await
            .ok_or_else(|| anyhow::anyhow!("thread not found: {thread_id}"))?;
        let visible_messages = thread
            .messages
            .into_iter()
            .filter(|message| !should_hide_participant_prompt_message(message))
            .collect::<Vec<_>>();
        let compacted_messages = self
            .compact_participant_prompt_messages(target_agent_id, &visible_messages)
            .await?;
        let prompt =
            build_visible_participant_message_prompt(participant, &compacted_messages, request);
        self.run_hidden_participant_prompt(thread_id, target_agent_id, &prompt)
            .await
    }

    pub async fn build_participant_prompt(
        &self,
        thread_id: &str,
        target_agent_id: &str,
    ) -> Result<String> {
        let participants = self.list_thread_participants(thread_id).await;
        let participant = participants
            .iter()
            .find(|participant| {
                participant.agent_id.eq_ignore_ascii_case(target_agent_id)
                    && participant.status == ThreadParticipantStatus::Active
            })
            .ok_or_else(|| {
                anyhow::anyhow!("participant is not active on thread: {target_agent_id}")
            })?;
        let thread = self
            .get_thread(thread_id)
            .await
            .ok_or_else(|| anyhow::anyhow!("thread not found: {thread_id}"))?;
        let visible_messages = thread
            .messages
            .into_iter()
            .filter(|message| !should_hide_participant_prompt_message(message))
            .collect::<Vec<_>>();
        let compacted_messages = self
            .compact_participant_prompt_messages(target_agent_id, &visible_messages)
            .await?;
        Ok(build_participant_prompt_from_snapshot(
            participant,
            &compacted_messages,
        ))
    }

    pub async fn append_internal_delegate_message(
        &self,
        thread_id: &str,
        content: &str,
    ) -> Result<()> {
        let mut threads = self.threads.write().await;
        let thread = threads
            .get_mut(thread_id)
            .ok_or_else(|| anyhow::anyhow!("thread not found: {thread_id}"))?;
        thread.messages.push(AgentMessage {
            id: generate_message_id(),
            role: MessageRole::System,
            content: content.to_string(),
            tool_calls: None,
            tool_call_id: None,
            tool_name: Some("internal_delegate".to_string()),
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
            timestamp: now_millis(),
        });
        thread.updated_at = now_millis();
        drop(threads);
        self.persist_thread_by_id(thread_id).await;
        Ok(())
    }

    pub async fn run_participant_observers(&self, thread_id: &str) -> Result<()> {
        let participants = self.list_thread_participants(thread_id).await;
        if participants.is_empty() {
            return Ok(());
        }
        let active_responder_agent_id = self.active_agent_id_for_thread(thread_id).await;
        let visible_messages = self
            .get_thread(thread_id)
            .await
            .ok_or_else(|| anyhow::anyhow!("thread not found: {thread_id}"))?
            .messages
            .into_iter()
            .filter(|message| !should_hide_participant_prompt_message(message))
            .collect::<Vec<_>>();
        if !latest_visible_message_allows_participant_observers(&visible_messages, &participants) {
            tracing::info!(
                thread_id = %thread_id,
                "skipping participant observers because the latest visible message is participant-authored"
            );
            return Ok(());
        }
        let latest_visible_message_timestamp = visible_messages
            .last()
            .map(|message| message.timestamp)
            .unwrap_or(0);
        let queued_suggestions = self.list_thread_participant_suggestions(thread_id).await;
        let mut participant_state_changed = false;

        for participant in participants.into_iter().filter(|participant| {
            participant.status == ThreadParticipantStatus::Active
                && active_responder_agent_id.as_deref() != Some(participant.agent_id.as_str())
        }) {
            if participant
                .last_observed_visible_message_at
                .is_some_and(|timestamp| timestamp >= latest_visible_message_timestamp)
            {
                continue;
            }
            if queued_suggestions.iter().any(|suggestion| {
                suggestion
                    .target_agent_id
                    .eq_ignore_ascii_case(&participant.agent_id)
                    && suggestion.status == ThreadParticipantSuggestionStatus::Queued
            }) {
                continue;
            }
            let compacted_messages = self
                .compact_participant_prompt_messages(&participant.agent_id, &visible_messages)
                .await?;
            tracing::info!(
                thread_id = %thread_id,
                participant = %participant.agent_id,
                visible_message_count = visible_messages.len(),
                compacted_message_count = compacted_messages.len(),
                visible_est_tokens = estimate_message_tokens(&visible_messages),
                compacted_est_tokens = estimate_message_tokens(&compacted_messages),
                "running participant observer"
            );
            let prompt = build_participant_prompt_from_snapshot(&participant, &compacted_messages);
            let response = self
                .run_hidden_participant_prompt(thread_id, &participant.agent_id, &prompt)
                .await?;
            let Some((force_send, message)) = parse_participant_suggestion_response(&response)
            else {
                tracing::info!(
                    thread_id = %thread_id,
                    participant = %participant.agent_id,
                    "participant observer returned no suggestion"
                );
                participant_state_changed |= self
                    .mark_thread_participant_observed_visible_message(
                        thread_id,
                        &participant.agent_id,
                        latest_visible_message_timestamp,
                    )
                    .await;
                continue;
            };
            tracing::info!(
                thread_id = %thread_id,
                participant = %participant.agent_id,
                force_send,
                "participant observer produced suggestion"
            );
            if force_send {
                self.append_visible_thread_participant_message(
                    thread_id,
                    &participant.agent_id,
                    &message,
                )
                .await?;
                participant_state_changed |= self
                    .mark_thread_participant_observed_visible_message(
                        thread_id,
                        &participant.agent_id,
                        latest_visible_message_timestamp,
                    )
                    .await;
                self.continue_thread_after_participant_post_or_notice(thread_id)
                    .await;
            } else {
                self.queue_thread_participant_suggestion(
                    thread_id,
                    &participant.agent_id,
                    &message,
                    false,
                )
                .await?;
                participant_state_changed |= self
                    .mark_thread_participant_observed_visible_message(
                        thread_id,
                        &participant.agent_id,
                        latest_visible_message_timestamp,
                    )
                    .await;
            }
        }

        if participant_state_changed {
            self.persist_thread_by_id(thread_id).await;
        }

        Ok(())
    }
}
