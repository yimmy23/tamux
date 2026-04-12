use super::*;

fn parse_participant_suggestion_response(response: &str) -> Option<(bool, String)> {
    let trimmed = response.trim();
    if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("NO_SUGGESTION") {
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
        if message.eq_ignore_ascii_case("NO_SUGGESTION") {
            return None;
        }
        return Some((force_send, message));
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

#[derive(Clone)]
struct ParticipantObserverResponderConfig {
    provider_id: String,
    provider_config: ProviderConfig,
    base_prompt: String,
}

impl AgentEngine {
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

    async fn run_participant_observer_prompt(
        &self,
        target_agent_id: &str,
        prompt: &str,
    ) -> Result<String> {
        let target_agent_id = canonical_agent_id(target_agent_id).to_string();
        let prompt = prompt.to_string();
        Box::pin(run_with_agent_scope(target_agent_id.clone(), async move {
            let responder = self
                .participant_observer_responder_config(&target_agent_id)
                .await?;
            let sub_agents = self.list_sub_agents().await;
            let memory = self.current_memory_snapshot().await;
            let memory_paths = memory_paths_for_scope(&self.data_dir, &target_agent_id);
            let config = self.config.read().await.clone();
            let system_prompt = build_system_prompt(
                &config,
                &responder.base_prompt,
                &memory,
                &memory_paths,
                &target_agent_id,
                &sub_agents,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
            );
            let messages = vec![ApiMessage {
                role: "user".into(),
                content: ApiContent::Text(prompt),
                tool_call_id: None,
                name: None,
                tool_calls: None,
            }];

            self.check_circuit_breaker(&responder.provider_id).await?;
            let mut stream = send_completion_request(
                &self.http_client,
                &responder.provider_id,
                &responder.provider_config,
                &system_prompt,
                &messages,
                &[],
                responder.provider_config.api_transport,
                None,
                None,
                RetryStrategy::Bounded {
                    max_retries: 1,
                    retry_delay_ms: 500,
                },
            );

            let mut content = String::new();
            let mut reasoning = String::new();
            while let Some(chunk) = stream.next().await {
                let chunk = match chunk {
                    Ok(value) => value,
                    Err(error) => {
                        self.record_llm_outcome(&responder.provider_id, false).await;
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
                        self.record_llm_outcome(&responder.provider_id, true).await;
                        if let Some(done_reasoning) = done_reasoning {
                            reasoning = done_reasoning;
                        }
                        let final_content = if done.is_empty() { content } else { done };
                        if !final_content.trim().is_empty() {
                            return Ok(final_content.trim().to_string());
                        }
                        if !reasoning.trim().is_empty() {
                            return Ok(reasoning.trim().to_string());
                        }
                        anyhow::bail!("participant observer returned empty output");
                    }
                    CompletionChunk::Error { message } => {
                        self.record_llm_outcome(&responder.provider_id, false).await;
                        anyhow::bail!(message);
                    }
                    CompletionChunk::ToolCalls { .. } => {
                        self.record_llm_outcome(&responder.provider_id, true).await;
                        anyhow::bail!("participant observer unexpectedly returned tool calls");
                    }
                    CompletionChunk::TransportFallback { .. } | CompletionChunk::Retry { .. } => {}
                }
            }

            if !content.trim().is_empty() {
                return Ok(content.trim().to_string());
            }

            anyhow::bail!("participant observer returned empty output")
        }))
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
        let visible_messages = self
            .get_thread(thread_id)
            .await
            .ok_or_else(|| anyhow::anyhow!("thread not found: {thread_id}"))?
            .messages
            .into_iter()
            .filter(|message| !should_hide_participant_prompt_message(message))
            .collect::<Vec<_>>();

        for participant in participants
            .into_iter()
            .filter(|participant| participant.status == ThreadParticipantStatus::Active)
        {
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
                .run_participant_observer_prompt(&participant.agent_id, &prompt)
                .await?;
            let Some((force_send, message)) = parse_participant_suggestion_response(&response)
            else {
                tracing::info!(
                    thread_id = %thread_id,
                    participant = %participant.agent_id,
                    "participant observer returned no suggestion"
                );
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
            }
        }

        Ok(())
    }
}
