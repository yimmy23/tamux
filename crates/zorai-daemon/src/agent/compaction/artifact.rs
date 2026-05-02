use super::*;

impl AgentEngine {
    pub(crate) async fn build_compaction_artifact(
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

        let (payload, payload_was_capped) =
            fit_compaction_payload_to_budget(payload, target_tokens);
        if payload_was_capped {
            fallback_notice = merge_compaction_fallback_notice(
                fallback_notice,
                Some(
                    "Compaction checkpoint exceeded the continuity budget and was truncated."
                        .to_string(),
                ),
            );
        }

        let visible_content = build_compaction_visible_content(
            pre_compaction_total_tokens,
            effective_context_window_tokens,
            target_tokens,
            trigger,
            strategy_used,
        );

        Ok((
            AgentMessage {
                id: generate_message_id(),
                role: MessageRole::Assistant,
                content: visible_content,
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
                compaction_strategy: Some(strategy_used),
                compaction_payload: Some(payload),
                offloaded_payload_id: None,
                tool_output_preview_path: None,
                structural_refs,
                pinned_for_compaction: false,
                timestamp: messages
                    .last()
                    .map(|message| message.timestamp)
                    .unwrap_or_else(now_millis),
            },
            strategy_used,
            fallback_notice,
        ))
    }

    pub(crate) async fn build_rule_based_compaction_payload(
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

    pub(crate) async fn build_coding_compaction_payload(
        &self,
        thread_id: &str,
        messages: &[AgentMessage],
        target_tokens: usize,
        structural_memory: &ThreadStructuralMemory,
    ) -> Result<(String, Vec<String>)> {
        let seed_structural_refs = collect_message_structural_refs(messages);
        let structural_entries = structural_memory.concise_context_entries(
            &seed_structural_refs,
            CODING_COMPACTION_STRUCTURAL_ENTRY_LIMIT,
        );
        if structural_entries.is_empty() {
            anyhow::bail!("no structural context entries available for coding compaction");
        }

        let graph_neighbors = load_memory_graph_neighbors(
            &self.history,
            &seed_structural_refs,
            CODING_COMPACTION_GRAPH_NEIGHBOR_LIMIT,
        )
        .await?;
        let merged_structural_entries = merge_structural_context_entries(
            &structural_entries,
            &graph_neighbors,
            CODING_COMPACTION_STRUCTURAL_ENTRY_LIMIT + CODING_COMPACTION_GRAPH_NEIGHBOR_LIMIT,
        );

        let offloaded_metadata =
            load_referenced_offloaded_payload_metadata(&self.history, thread_id, messages).await?;
        let structural_refs = merged_structural_entries
            .iter()
            .map(|entry| entry.node_id.clone())
            .collect::<Vec<_>>();
        let mut payload = format!(
            "## Primary Objective\n{}\n\n## Execution Map\n{}\n\n## Structural Context\n{}\n\n## Offloaded Payload References\n{}\n\n## Immediate Next Step\n{}\n",
            checkpoint_primary_objective(messages),
            coding_execution_map(messages),
            render_structural_context(&merged_structural_entries),
            render_offloaded_payload_references(&offloaded_metadata),
            checkpoint_immediate_next_step(messages),
        );
        payload.truncate(coding_compaction_payload_max_chars(target_tokens));

        Ok((payload, structural_refs))
    }

    pub(crate) async fn run_llm_compaction(
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

    pub(crate) fn resolve_weles_compaction_provider(
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

    pub(crate) fn resolve_custom_model_compaction_provider(
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
