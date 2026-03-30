//! Core agent loop — LLM streaming, tool execution, and turn management.

use super::*;

fn unexpected_stream_end_message(accumulated_content: &str) -> String {
    let trimmed = accumulated_content.trim();
    if trimmed.is_empty() {
        "Error: provider stream ended without yielding a response.".to_string()
    } else {
        accumulated_content.to_string()
    }
}

const DEFAULT_LLM_STREAM_CHUNK_TIMEOUT_SECS: u64 = 120;

impl AgentEngine {
    pub(super) async fn send_message_inner(
        &self,
        thread_id: Option<&str>,
        content: &str,
        task_id: Option<&str>,
        preferred_session_hint: Option<&str>,
        backend_override: Option<&str>,
        llm_user_content_override: Option<&str>,
        stream_chunk_timeout_override: Option<std::time::Duration>,
        record_operator: bool,
    ) -> Result<SendMessageOutcome> {
        let stored_user_content = content;
        let llm_user_content = llm_user_content_override.unwrap_or(content);
        let agent_scope_id = if let Some(current_task_id) = task_id {
            let tasks = self.tasks.lock().await;
            agent_scope_id_for_task(tasks.iter().find(|task| task.id == current_task_id))
        } else {
            MAIN_AGENT_ID.to_string()
        };

        Box::pin(run_with_agent_scope(agent_scope_id, async {
        if thread_id == Some(crate::agent::concierge::CONCIERGE_THREAD_ID) {
            self.send_concierge_message_on_thread(
                crate::agent::concierge::CONCIERGE_THREAD_ID,
                stored_user_content,
                preferred_session_hint,
                record_operator,
                true,
            )
            .await?;
            return Ok(SendMessageOutcome {
                thread_id: crate::agent::concierge::CONCIERGE_THREAD_ID.to_string(),
                interrupted_for_approval: false,
            });
        }

        let config = self.config.read().await.clone();
        let selected_backend = backend_override
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(AgentBackend::parse)
            .unwrap_or(config.agent_backend.clone());

        // Route through external agent if backend is "openclaw" or "hermes"
        match selected_backend {
            AgentBackend::Openclaw | AgentBackend::Hermes => {
                let mut runtime_config = config.clone();
                runtime_config.agent_backend = selected_backend;
                return self
                    .send_message_external(&runtime_config, thread_id, llm_user_content)
                    .await
                    .map(|thread_id| SendMessageOutcome {
                        thread_id,
                        interrupted_for_approval: false,
                    });
            }
            _ => {} // Fall through to built-in daemon LLM client
        }

        // Get or create thread
        let (tid, is_new_thread) = self
            .get_or_create_thread(thread_id, stored_user_content)
            .await;

        // Add user message
        {
            let mut threads = self.threads.write().await;
            if let Some(thread) = threads.get_mut(&tid) {
                thread
                    .messages
                    .push(AgentMessage::user(stored_user_content, now_millis()));
                thread.updated_at = now_millis();
            }
        }
        self.persist_thread_by_id(&tid).await;
        if record_operator {
            self.record_operator_message(&tid, stored_user_content, is_new_thread)
                .await?;
            if let Err(error) = self.maybe_sync_thread_to_honcho(&tid).await {
                tracing::warn!(thread_id = %tid, error = %error, "failed to sync thread to Honcho");
            }
        }

        // Inject continuity acknowledgment if pending (D-10 / MEMO-09)
        if let Some(ack_message) = self.take_continuity_acknowledgment(&tid).await {
            let mut threads = self.threads.write().await;
            if let Some(thread) = threads.get_mut(&tid) {
                let mut msg = AgentMessage::user(&ack_message, now_millis());
                msg.role = MessageRole::System;
                thread.messages.push(msg);
            }
            tracing::info!(thread_id = %tid, "injected continuity acknowledgment");
        }

        // Augment plugin command messages with system hints (PSKL-05)
        if let Some(hint) = self.try_augment_plugin_command(content).await {
            let mut threads = self.threads.write().await;
            if let Some(thread) = threads.get_mut(&tid) {
                let mut msg = AgentMessage::user(&hint, now_millis());
                msg.role = MessageRole::System;
                thread.messages.push(msg);
            }
            tracing::info!(thread_id = %tid, "injected plugin command hint");
        }

        // Resolve provider config after the user message is attached so
        // startup/config failures still persist a complete thread history.
        // If the current task has a sub-agent provider override, use that instead.
        let task_provider_override = {
            let tasks = self.tasks.lock().await;
            task_id.and_then(|tid| {
                tasks.iter().find(|t| t.id == tid).and_then(|t| {
                    t.override_provider.as_ref().map(|p| {
                        (
                            p.clone(),
                            t.override_model.clone(),
                            t.override_system_prompt.clone(),
                        )
                    })
                })
            })
        };
        let active_provider_id = task_provider_override
            .as_ref()
            .map(|(provider_id, _, _)| provider_id.as_str())
            .unwrap_or(config.provider.as_str())
            .to_string();
        let provider_config =
            match if let Some((ref sub_provider, ref sub_model, _)) = task_provider_override {
                let mut pc = self.resolve_sub_agent_provider_config(&config, sub_provider)?;
                if let Some(model) = sub_model {
                    pc.model = model.clone();
                }
                Ok(pc)
            } else {
                self.resolve_provider_config(&config)
            } {
                Ok(provider_config) => provider_config,
                Err(error) => {
                    let error_text = error.to_string();
                    self.add_assistant_message(
                        &tid,
                        &format!("Error: {error_text}"),
                        0,
                        0,
                        None,
                        None,
                        None,
                        None,
                        None,
                    )
                    .await;
                    self.persist_threads().await;
                    self.emit_turn_error_completion(&tid, &error_text, None, None)
                        .await;
                    return Err(error);
                }
            };

        let (stream_generation, stream_cancel_token) = self.begin_stream_cancellation(&tid).await;

        let onecontext_bootstrap = if is_new_thread {
            self.onecontext_bootstrap_for_new_thread(stored_user_content).await
        } else {
            None
        };
        let preferred_session_id =
            resolve_preferred_session_id(&self.session_manager, preferred_session_hint).await;
        let skill_preflight = self
            .build_skill_preflight_context(stored_user_content, preferred_session_id.clone())
            .await?;

        // Build system prompt with memory.
        // If this task has a sub-agent system prompt override, prepend it.
        let agent_scope_id = current_agent_scope_id();
        let memory = self.current_memory_snapshot().await;
        let memory_paths = memory_paths_for_scope(&self.data_dir, &agent_scope_id);
        let base_prompt = if let Some((_, _, Some(ref override_prompt))) = task_provider_override {
            format!("{}\n\n{}", override_prompt, config.system_prompt)
        } else {
            config.system_prompt.clone()
        };
        let operator_model_summary = self.build_operator_model_prompt_summary().await;
        let operational_context = self.build_operational_context_summary().await;
        let causal_guidance = self.build_causal_guidance_summary().await;
        // D-08: Build learned patterns from HeuristicStore for system prompt injection
        let learned_patterns = {
            let hs = self.heuristic_store.read().await;
            let patterns = build_learned_patterns_section(&hs);
            if patterns.is_empty() {
                None
            } else {
                Some(patterns)
            }
        };
        let mut system_prompt = build_system_prompt(
            &config,
            &base_prompt,
            &memory,
            &memory_paths,
            &agent_scope_id,
            &config.sub_agents,
            operator_model_summary.as_deref(),
            operational_context.as_deref(),
            causal_guidance.as_deref(),
            learned_patterns.as_deref(),
            None, // episodic_context — injected via goal planning path, not agent loop
            None, // negative_constraints — injected via goal planning path
        );
        let runtime_agent_name = task_provider_override
            .as_ref()
            .and_then(|(_, _, prompt)| extract_persona_name(prompt.as_deref()))
            .unwrap_or_else(|| MAIN_AGENT_NAME.to_string());
        system_prompt.push_str("\n\n");
        system_prompt.push_str(&build_runtime_identity_prompt(
            &runtime_agent_name,
            &active_provider_id,
            &provider_config.model,
        ));
        if let Some(recall) = onecontext_bootstrap.as_deref() {
            system_prompt.push_str("\n\n## OneContext Recall\n");
            system_prompt
                .push_str("Use this as historical context from prior sessions when relevant:\n");
            system_prompt.push_str(recall);
        }
        if let Some(skill_preflight) = skill_preflight.as_deref() {
            system_prompt.push_str("\n\n## Preloaded Skills\n");
            system_prompt.push_str(skill_preflight);
        }
        match self.maybe_build_honcho_context(&tid, content).await {
            Ok(Some(honcho_context)) => {
                system_prompt.push_str("\n\n## Cross-Session Memory\n");
                system_prompt.push_str(&honcho_context);
            }
            Ok(None) => {}
            Err(error) => {
                tracing::warn!(thread_id = %tid, error = %error, "failed to build Honcho context");
            }
        }
        self.emit_workflow_notice(
            &tid,
            "memory-consulted",
            "Loaded persistent memory, user profile, and local skill paths for this turn.",
            Some(format!(
                "memory_dir={}; skills_dir={}",
                memory_paths.memory_dir.display(),
                skills_dir(&self.data_dir).display()
            )),
        );
        if skill_preflight.is_some() {
            self.emit_workflow_notice(
                &tid,
                "skill-preflight",
                "Preloaded relevant local skills for this turn before tool execution.",
                None,
            );
        }

        // Get tools, applying per-task tool filters if configured
        let has_workspace_topology = self.session_manager.read_workspace_topology().is_some();
        let mut tools = get_available_tools(&config, &self.data_dir, has_workspace_topology);
        let (
            current_task_snapshot,
            is_durable_goal_task,
            task_tool_filter,
            mut task_context_budget,
            task_termination_eval,
            task_type_for_trace,
        ) = {
            let tasks = self.tasks.lock().await;
            let current_task = task_id
                .and_then(|current_task_id| tasks.iter().find(|task| task.id == current_task_id))
                .cloned();
            let is_goal = current_task
                .as_ref()
                .and_then(|task| task.goal_run_id.as_ref())
                .is_some();
            let filter = current_task.as_ref().and_then(|task| {
                if task.tool_whitelist.is_some() || task.tool_blacklist.is_some() {
                    crate::agent::subagent::tool_filter::ToolFilter::new(
                        task.tool_whitelist.clone(),
                        task.tool_blacklist.clone(),
                    )
                    .ok()
                } else {
                    None
                }
            });
            let budget = current_task.as_ref().and_then(|task| {
                task.context_budget_tokens.map(|max_tokens| {
                    crate::agent::subagent::context_budget::ContextBudget::new(
                        max_tokens,
                        task.context_overflow_action
                            .unwrap_or(crate::agent::types::ContextOverflowAction::Compress),
                    )
                })
            });
            let termination = current_task
                .as_ref()
                .and_then(|task| task.termination_conditions.as_deref())
                .and_then(|dsl| {
                    crate::agent::subagent::termination::TerminationEvaluator::parse(dsl).ok()
                });
            let task_type = current_task
                .as_ref()
                .map(|task| classify_task(task).to_string())
                .unwrap_or_default();
            (
                current_task,
                is_goal,
                filter,
                budget,
                termination,
                task_type,
            )
        };
        if let Some(filter) = &task_tool_filter {
            tools = filter.filtered_tools(tools);
        }
        // D-08 part 2: Reorder tools by learned heuristic effectiveness for this task type
        if !task_type_for_trace.is_empty() {
            let hs = self.heuristic_store.read().await;
            super::tool_executor::reorder_tools_by_heuristics(
                &mut tools,
                &hs,
                &task_type_for_trace,
            );
        }
        if let Some(task) = current_task_snapshot.as_ref() {
            self.ensure_subagent_runtime(task, Some(&tid)).await;
        }
        let retry_strategy = if !config.auto_retry {
            RetryStrategy::Bounded {
                max_retries: 0,
                retry_delay_ms: config.retry_delay_ms,
            }
        } else if is_durable_goal_task {
            RetryStrategy::DurableRateLimited
        } else {
            RetryStrategy::Bounded {
                max_retries: config.max_retries,
                retry_delay_ms: config.retry_delay_ms,
            }
        };

        // Run the agent loop
        // Goal runner tasks get unlimited tool loops — only the loop-detection
        // guard (consecutive identical calls) protects against infinite loops.
        let max_loops = if is_durable_goal_task {
            0
        } else {
            config.max_tool_loops
        };
        let mut loop_count = 0u32;
        let mut was_cancelled = false;
        let mut interrupted_for_approval = false;
        let mut previous_tool_signature: Option<String> = None;
        let mut previous_tool_outcome: Option<(String, bool)> = None;
        let mut last_tool_error: Option<(String, String)> = None;
        let mut consecutive_same_tool_calls = 0u32;
        let mut last_pre_compaction_flush_signature: Option<u64> = None;
        let mut recorded_compaction_provenance = false;

        // Trace collection for learning
        let mut trace_collector =
            crate::agent::learning::traces::TraceCollector::new(&task_type_for_trace, now_millis());
        // Termination metrics tracked per-loop
        let mut termination_tool_calls: u32 = 0;
        let mut termination_tool_successes: u32 = 0;
        let mut termination_consecutive_errors: u32 = 0;
        let mut termination_total_errors: u32 = 0;
        let loop_started_at = now_millis();
        let mut stream_timeout_count = 0u32;
        let mut tool_ack_emitted = false;
        let mut tool_sequence_repaired = false;
        let mut retry_status_visible = false;

        'agent_loop: while max_loops == 0 || loop_count < max_loops {
            if stream_cancel_token.is_cancelled() {
                was_cancelled = true;
                break;
            }
            loop_count += 1;

            if self
                .maybe_run_pre_compaction_memory_flush(
                    &tid,
                    task_id,
                    &config,
                    &provider_config,
                    &system_prompt,
                    preferred_session_id,
                    retry_strategy,
                    &mut last_pre_compaction_flush_signature,
                )
                .await?
            {
                let memory = self.current_memory_snapshot().await;
                let causal_guidance = self.build_causal_guidance_summary().await;
                system_prompt = build_system_prompt(
                    &config,
                    &base_prompt,
                    &memory,
                    &memory_paths,
                    &agent_scope_id,
                    &config.sub_agents,
                    operator_model_summary.as_deref(),
                    operational_context.as_deref(),
                    causal_guidance.as_deref(),
                    learned_patterns.as_deref(),
                    None, // episodic_context — injected via goal planning path
                    None, // negative_constraints — injected via goal planning path
                );
                if let Some(recall) = onecontext_bootstrap.as_deref() {
                    system_prompt.push_str("\n\n## OneContext Recall\n");
                    system_prompt.push_str(
                        "Use this as historical context from prior sessions when relevant:\n",
                    );
                    system_prompt.push_str(recall);
                }
                if let Some(skill_preflight) = skill_preflight.as_deref() {
                    system_prompt.push_str("\n\n## Preloaded Skills\n");
                    system_prompt.push_str(skill_preflight);
                }
            }

            // Build request payload from thread history.
            let prepared_request = {
                let threads = self.threads.read().await;
                let thread = match threads.get(&tid) {
                    Some(thread) => thread,
                    None => {
                        self.finish_stream_cancellation(&tid, stream_generation)
                            .await;
                        anyhow::bail!("thread not found");
                    }
                };
                let mut request_thread = thread.clone();
                if llm_user_content != stored_user_content {
                    if let Some(last_user_message) = request_thread
                        .messages
                        .iter_mut()
                        .rev()
                        .find(|message| message.role == MessageRole::User)
                    {
                        last_user_message.content = llm_user_content.to_string();
                    }
                }
                let prepared = prepare_llm_request(&request_thread, &config, &provider_config);
                if !recorded_compaction_provenance {
                    if let Some(candidate) =
                        compaction_candidate(&request_thread.messages, &config, &provider_config)
                    {
                        self.record_provenance_event(
                            "context_compressed",
                            "thread context was compacted for an LLM request",
                            serde_json::json!({
                                "thread_id": tid.as_str(),
                                "split_at": candidate.split_at,
                                "target_tokens": candidate.target_tokens,
                                "message_count": thread.messages.len(),
                            }),
                            None,
                            task_id,
                            Some(tid.as_str()),
                            None,
                            None,
                        )
                        .await;
                        recorded_compaction_provenance = true;
                    }
                }
                tracing::info!(
                    thread_id = %tid,
                    thread_messages = thread.messages.len(),
                    api_messages = prepared.messages.len(),
                    transport = ?prepared.transport,
                    loop_count,
                    "building LLM request"
                );
                prepared
            };

            // Rate-limit pause: avoid hammering the provider between loop iterations.
            if loop_count > 1 {
                tokio::time::sleep(std::time::Duration::from_millis(200)).await;
            }

            // Call LLM
            // Circuit breaker check: reject fast if the provider is unhealthy.
            if let Err(e) = self.check_circuit_breaker(&config.provider).await {
                let outage_context = self
                    .suggest_alternative_provider(&config.provider)
                    .await
                    .unwrap_or_else(|| {
                        "No healthy fallback providers are currently available.".to_string()
                    });
                let error_msg = format!(
                    "Provider '{}' is temporarily unavailable (circuit breaker open). {}",
                    config.provider, outage_context
                );
                let _ = self.event_tx.send(AgentEvent::Error {
                    thread_id: tid.clone(),
                    message: error_msg.clone(),
                });
                self.finish_stream_cancellation(&tid, stream_generation)
                    .await;
                return Err(e.context(error_msg));
            }
            let llm_started_at = Instant::now();
            let mut first_token_at: Option<Instant> = None;
            let mut effective_transport_for_turn = prepared_request.transport;
            let mut stream = send_completion_request(
                &self.http_client,
                &config.provider,
                &provider_config,
                &system_prompt,
                &prepared_request.messages,
                &tools,
                prepared_request.transport,
                prepared_request.previous_response_id.clone(),
                prepared_request.upstream_thread_id.clone(),
                retry_strategy,
            );

            let mut accumulated_content = String::new();
            let mut accumulated_reasoning = String::new();
            let mut final_chunk: Option<CompletionChunk> = None;

            // Timeout for individual chunk reads — if a provider stops sending
            // data mid-stream, we don't hang forever. The agent retries automatically.
            let llm_stream_chunk_timeout = stream_chunk_timeout_override.unwrap_or_else(|| {
                std::time::Duration::from_secs(DEFAULT_LLM_STREAM_CHUNK_TIMEOUT_SECS)
            });
            const MAX_STREAM_TIMEOUTS: u32 = 3;
            let mut stream_timed_out = false;

            loop {
                tokio::select! {
                    _ = stream_cancel_token.cancelled() => {
                        was_cancelled = true;
                        break;
                    }
                    _ = tokio::time::sleep(llm_stream_chunk_timeout) => {
                        tracing::warn!("LLM stream timeout — no data for {}s", llm_stream_chunk_timeout.as_secs());
                        self.record_llm_outcome(&config.provider, false).await;
                        stream_timed_out = true;
                        break;
                    }
                    maybe_chunk = stream.next() => {
                        let Some(chunk_result) = maybe_chunk else {
                            break;
                        };

                        let chunk = match chunk_result {
                            Ok(chunk) => chunk,
                            Err(e) => {
                                let err_str = e.to_string();
                                // Detect "tool call result does not follow tool call" —
                                // repair the thread's message sequence and retry.
                                if err_str.contains("tool call result does not follow tool call")
                                    && !tool_sequence_repaired
                                {
                                    tracing::warn!(
                                        "detected broken tool call sequence — repairing thread and retrying"
                                    );
                                    tool_sequence_repaired = true;
                                    self.repair_tool_call_sequence(&tid).await;
                                    let _ = self.event_tx.send(AgentEvent::WorkflowNotice {
                                        thread_id: tid.clone(),
                                        kind: "tool-repair".to_string(),
                                        message: "Repairing message sequence, retrying...".to_string(),
                                        details: None,
                                    });
                                    continue 'agent_loop;
                                }
                                self.record_llm_outcome(&config.provider, false).await;
                                self.finish_stream_cancellation(&tid, stream_generation).await;
                                return Err(e);
                            }
                        };

                        match chunk {
                            CompletionChunk::Delta { content, reasoning } => {
                                if retry_status_visible {
                                    let _ = self.event_tx.send(AgentEvent::RetryStatus {
                                        thread_id: tid.clone(),
                                        phase: "cleared".to_string(),
                                        attempt: 0,
                                        max_retries: 0,
                                        delay_ms: 0,
                                        failure_class: String::new(),
                                        message: String::new(),
                                    });
                                    retry_status_visible = false;
                                }
                                if first_token_at.is_none()
                                    && (!content.is_empty()
                                        || reasoning
                                            .as_ref()
                                            .map(|s| !s.is_empty())
                                            .unwrap_or(false))
                                {
                                    first_token_at = Some(Instant::now());
                                }
                                if !content.is_empty() {
                                    accumulated_content.push_str(&content);
                                    let _ = self.event_tx.send(AgentEvent::Delta {
                                        thread_id: tid.clone(),
                                        content,
                                    });
                                }
                                if let Some(r) = reasoning {
                                    accumulated_reasoning.push_str(&r);
                                    let _ = self.event_tx.send(AgentEvent::Reasoning {
                                        thread_id: tid.clone(),
                                        content: r,
                                    });
                                }
                            }
                            CompletionChunk::Retry {
                                attempt,
                                max_retries,
                                delay_ms,
                                failure_class,
                                message,
                            } => {
                                let _ = self.event_tx.send(AgentEvent::RetryStatus {
                                    thread_id: tid.clone(),
                                    phase: "retrying".to_string(),
                                    attempt,
                                    max_retries,
                                    delay_ms,
                                    failure_class,
                                    message,
                                });
                                retry_status_visible = true;
                            }
                            CompletionChunk::TransportFallback { from, to, message } => {
                                if retry_status_visible {
                                    let _ = self.event_tx.send(AgentEvent::RetryStatus {
                                        thread_id: tid.clone(),
                                        phase: "cleared".to_string(),
                                        attempt: 0,
                                        max_retries: 0,
                                        delay_ms: 0,
                                        failure_class: String::new(),
                                        message: String::new(),
                                    });
                                    retry_status_visible = false;
                                }
                                effective_transport_for_turn = to;
                                {
                                    let mut stored_config = self.config.write().await;
                                    stored_config.api_transport = to;
                                    if let Some(provider_entry) =
                                        stored_config.providers.get_mut(&config.provider)
                                    {
                                        provider_entry.api_transport = to;
                                    }
                                }
                                self.persist_config().await;
                                self.emit_workflow_notice(
                                    &tid,
                                    "transport-fallback",
                                    "Responses API was incompatible for this provider. Switched to legacy chat completions.",
                                    Some(
                                        serde_json::json!({
                                            "provider": config.provider,
                                            "from": from,
                                            "to": to,
                                            "reason": message,
                                        })
                                        .to_string(),
                                    ),
                                );
                            }
                            chunk @ CompletionChunk::Done { .. } => {
                                final_chunk = Some(chunk);
                                break;
                            }
                            chunk @ CompletionChunk::ToolCalls { .. } => {
                                final_chunk = Some(chunk);
                                break;
                            }
                            CompletionChunk::Error { message } => {
                                if config.auto_retry
                                    && is_transient_retry_message(&message)
                                {
                                    let delay_ms = 30_000u64;
                                    let _ = self.event_tx.send(AgentEvent::RetryStatus {
                                        thread_id: tid.clone(),
                                        phase: "waiting".to_string(),
                                        attempt: config.max_retries,
                                        max_retries: config.max_retries,
                                        delay_ms,
                                        failure_class: retry_failure_class_from_message(&message)
                                            .to_string(),
                                        message: message.clone(),
                                    });
                                    retry_status_visible = true;
                                    tokio::select! {
                                        _ = stream_cancel_token.cancelled() => {
                                            was_cancelled = true;
                                            break;
                                        }
                                        _ = tokio::time::sleep(std::time::Duration::from_millis(delay_ms)) => {
                                            continue 'agent_loop;
                                        }
                                    }
                                }
                                if retry_status_visible {
                                    let _ = self.event_tx.send(AgentEvent::RetryStatus {
                                        thread_id: tid.clone(),
                                        phase: "cleared".to_string(),
                                        attempt: 0,
                                        max_retries: 0,
                                        delay_ms: 0,
                                        failure_class: String::new(),
                                        message: String::new(),
                                    });
                                    retry_status_visible = false;
                                }
                                self.record_llm_outcome(&config.provider, false).await;
                                // Add error as assistant message
                                self.add_assistant_message(
                                    &tid,
                                    &format!("Error: {message}"),
                                    0,
                                    0,
                                    None,
                                    None,
                                    None,
                                    None,
                                    None,
                                )
                                    .await;
                                self.persist_threads().await;
                                self.emit_turn_error_completion(
                                    &tid,
                                    &message,
                                    Some(config.provider.clone()),
                                    Some(provider_config.model.clone()),
                                )
                                .await;
                                self.finish_stream_cancellation(&tid, stream_generation).await;
                                return Err(anyhow::anyhow!("LLM error: {message}"));
                            }
                        }
                    }
                }
            }

            if was_cancelled {
                break 'agent_loop;
            }

            // On stream timeout, notify the user and retry (up to MAX_STREAM_TIMEOUTS).
            if stream_timed_out {
                stream_timeout_count += 1;
                if stream_timeout_count >= MAX_STREAM_TIMEOUTS && !config.auto_retry {
                    let msg = format!(
                        "Connection timed out {} times \u{2014} giving up. The provider may be overloaded.",
                        stream_timeout_count
                    );
                    let _ = self.event_tx.send(AgentEvent::WorkflowNotice {
                        thread_id: tid.clone(),
                        kind: "stream-timeout".to_string(),
                        message: msg.clone(),
                        details: None,
                    });
                    self.finish_stream_cancellation(&tid, stream_generation)
                        .await;
                    return Err(anyhow::anyhow!("{}", msg));
                }
                let delay_ms = if stream_timeout_count >= MAX_STREAM_TIMEOUTS {
                    30_000u64
                } else {
                    2_000u64
                };
                let phase = if stream_timeout_count >= MAX_STREAM_TIMEOUTS {
                    "waiting"
                } else {
                    "retrying"
                };
                let _ = self.event_tx.send(AgentEvent::RetryStatus {
                    thread_id: tid.clone(),
                    phase: phase.to_string(),
                    attempt: stream_timeout_count,
                    max_retries: MAX_STREAM_TIMEOUTS,
                    delay_ms,
                    failure_class: "timeout".to_string(),
                    message: "Connection timed out while waiting for streamed output".to_string(),
                });
                retry_status_visible = true;
                tokio::select! {
                    _ = stream_cancel_token.cancelled() => {
                        was_cancelled = true;
                        break 'agent_loop;
                    }
                    _ = tokio::time::sleep(std::time::Duration::from_millis(delay_ms)) => {
                        continue 'agent_loop;
                    }
                }
            }

            // Record successful LLM outcome for circuit breaker tracking.
            if final_chunk.is_some() {
                self.record_llm_outcome(&config.provider, true).await;
            }

            match final_chunk {
                Some(CompletionChunk::Done {
                    content,
                    reasoning,
                    input_tokens,
                    output_tokens,
                    response_id,
                    upstream_thread_id,
                }) => {
                    let mut final_content = if content.is_empty() {
                        accumulated_content
                    } else {
                        content
                    };
                    if let Some((tool_name, error_message)) = last_tool_error.as_ref() {
                        let lower = final_content.to_ascii_lowercase();
                        if !lower.contains("failed")
                            && !lower.contains("error")
                            && !lower.contains("could not")
                            && !lower.contains("unable")
                        {
                            final_content = format!(
                                "The last tool call failed (`{tool_name}`): {error_message}\n\n{final_content}"
                            );
                        }
                    }
                    let final_reasoning = reasoning.or(if accumulated_reasoning.is_empty() {
                        None
                    } else {
                        Some(accumulated_reasoning)
                    });
                    if retry_status_visible {
                        let _ = self.event_tx.send(AgentEvent::RetryStatus {
                            thread_id: tid.clone(),
                            phase: "cleared".to_string(),
                            attempt: 0,
                            max_retries: 0,
                            delay_ms: 0,
                            failure_class: String::new(),
                            message: String::new(),
                        });
                        retry_status_visible = false;
                    }

                    self.add_assistant_message(
                        &tid,
                        &final_content,
                        input_tokens,
                        output_tokens,
                        final_reasoning.clone(),
                        Some(config.provider.clone()),
                        Some(provider_config.model.clone()),
                        Some(effective_transport_for_turn),
                        response_id,
                    )
                    .await;
                    self.update_thread_upstream_state(
                        &tid,
                        &config.provider,
                        &provider_config.model,
                        effective_transport_for_turn,
                        Some(provider_config.assistant_id.as_str()),
                        upstream_thread_id,
                    )
                    .await;

                    let generation_secs = first_token_at
                        .unwrap_or(llm_started_at)
                        .elapsed()
                        .as_secs_f64();
                    let (generation_ms, tps) =
                        compute_generation_stats(generation_secs, output_tokens);

                    // Cost accumulation (COST-01) — no-tool-call path
                    self.accumulate_goal_run_cost(
                        &tid,
                        input_tokens,
                        output_tokens,
                        &config.provider,
                        &provider_config.model,
                    )
                    .await;

                    let _ = self.event_tx.send(AgentEvent::Done {
                        thread_id: tid.clone(),
                        input_tokens,
                        output_tokens,
                        cost: None,
                        provider: Some(config.provider.clone()),
                        model: Some(provider_config.model.clone()),
                        tps,
                        generation_ms,
                        reasoning: final_reasoning,
                    });
                    break; // No tool calls, conversation turn is done
                }
                Some(CompletionChunk::ToolCalls {
                    tool_calls,
                    content,
                    reasoning,
                    input_tokens,
                    output_tokens,
                    response_id,
                    upstream_thread_id,
                }) => {
                    // If this is the first tool-call turn and no content was
                    // streamed yet, emit a quick acknowledgment via WorkflowNotice
                    // so the user knows the agent is working. WorkflowNotice is
                    // display-only (status line) — never saved to thread messages,
                    // so it won't pollute the Anthropic message sequence.
                    if !tool_ack_emitted && accumulated_content.trim().is_empty() {
                        tool_ack_emitted = true;
                        let tool_names: Vec<&str> = tool_calls
                            .iter()
                            .map(|tc| tc.function.name.as_str())
                            .collect();
                        let ack = match tool_names.as_slice() {
                            [single] => format!("On it \u{2014} using {single}..."),
                            names if names.len() <= 3 => {
                                format!("Working on it \u{2014} using {}...", names.join(", "))
                            }
                            names => {
                                format!("Working on it \u{2014} running {} tools...", names.len())
                            }
                        };
                        let _ = self.event_tx.send(AgentEvent::WorkflowNotice {
                            thread_id: tid.clone(),
                            kind: "tool-ack".to_string(),
                            message: ack,
                            details: None,
                        });
                    }

                    // Add assistant message with tool calls
                    let msg_content = content.unwrap_or(accumulated_content.clone());
                    let msg_reasoning = reasoning.or(if accumulated_reasoning.is_empty() {
                        None
                    } else {
                        Some(accumulated_reasoning.clone())
                    });
                    let decision_reasoning = msg_reasoning
                        .as_deref()
                        .filter(|value| !value.trim().is_empty())
                        .map(ToOwned::to_owned)
                        .or_else(|| {
                            (!msg_content.trim().is_empty()).then_some(msg_content.clone())
                        });

                    {
                        let mut threads = self.threads.write().await;
                        if let Some(thread) = threads.get_mut(&tid) {
                            thread.messages.push(AgentMessage {
                                id: generate_message_id(),
                                role: MessageRole::Assistant,
                                content: msg_content,
                                tool_calls: Some(tool_calls.clone()),
                                tool_call_id: None,
                                tool_name: None,
                                tool_arguments: None,
                                tool_status: None,
                                input_tokens: input_tokens.unwrap_or(0),
                                output_tokens: output_tokens.unwrap_or(0),
                                provider: Some(config.provider.clone()),
                                model: Some(provider_config.model.clone()),
                                api_transport: Some(effective_transport_for_turn),
                                response_id,
                                reasoning: msg_reasoning,
                                timestamp: now_millis(),
                            });
                            thread.total_input_tokens += input_tokens.unwrap_or(0);
                            thread.total_output_tokens += output_tokens.unwrap_or(0);
                        }
                    }
                    // Cost accumulation (COST-01) — tool-call path
                    self.accumulate_goal_run_cost(
                        &tid,
                        input_tokens.unwrap_or(0),
                        output_tokens.unwrap_or(0),
                        &config.provider,
                        &provider_config.model,
                    )
                    .await;

                    self.persist_thread_by_id(&tid).await;
                    self.update_thread_upstream_state(
                        &tid,
                        &config.provider,
                        &provider_config.model,
                        effective_transport_for_turn,
                        Some(provider_config.assistant_id.as_str()),
                        upstream_thread_id,
                    )
                    .await;

                    // Execute each tool call
                    for tc in &tool_calls {
                        if stream_cancel_token.is_cancelled() {
                            was_cancelled = true;
                            break;
                        }

                        let _ = self.event_tx.send(AgentEvent::ToolCall {
                            thread_id: tid.clone(),
                            call_id: tc.id.clone(),
                            name: tc.function.name.clone(),
                            arguments: tc.function.arguments.clone(),
                        });

                        // Enforce tool filter — deny calls to tools not allowed
                        // for this sub-agent before attempting execution.
                        if let Some(ref filter) = task_tool_filter {
                            if let Some(reason) = filter.deny_reason(&tc.function.name) {
                                let denied_content = format!("Tool call denied: {reason}");
                                let _ = self.event_tx.send(AgentEvent::ToolResult {
                                    thread_id: tid.clone(),
                                    call_id: tc.id.clone(),
                                    name: tc.function.name.clone(),
                                    content: denied_content.clone(),
                                    is_error: true,
                                });
                                {
                                    let mut threads = self.threads.write().await;
                                    if let Some(thread) = threads.get_mut(&tid) {
                                        thread.messages.push(AgentMessage {
                                            id: generate_message_id(),
                                            role: MessageRole::Tool,
                                            content: denied_content,
                                            tool_calls: None,
                                            tool_call_id: Some(tc.id.clone()),
                                            tool_name: Some(tc.function.name.clone()),
                                            tool_arguments: Some(tc.function.arguments.clone()),
                                            tool_status: Some("error".to_string()),
                                            input_tokens: 0,
                                            output_tokens: 0,
                                            provider: None,
                                            model: None,
                                            api_transport: None,
                                            response_id: None,
                                            reasoning: None,
                                            timestamp: now_millis(),
                                        });
                                    }
                                }
                                continue;
                            }
                        }

                        let current_tool_signature = normalized_tool_signature(tc);
                        let result = if previous_tool_signature
                            .as_deref()
                            .is_some_and(|value| value == current_tool_signature.as_str())
                        {
                            consecutive_same_tool_calls =
                                consecutive_same_tool_calls.saturating_add(1);
                            if consecutive_same_tool_calls >= 3 {
                                self.emit_workflow_notice(
                                    &tid,
                                    "tool-stall",
                                    "Repeated identical tool call suppressed; inspect fresh state or choose a different action.",
                                    Some(format!(
                                        "tool={} signature={}",
                                        tc.function.name, current_tool_signature
                                    )),
                                );
                                ToolResult {
                                    tool_call_id: tc.id.clone(),
                                    name: tc.function.name.clone(),
                                    content: "Repeated identical tool call suppressed because the agent appears stuck. Inspect current state or continue with a different action instead of repeating the same tool input.".to_string(),
                                    is_error: true,
                                    pending_approval: None,
                                }
                            } else {
                                execute_tool(
                                    tc,
                                    self,
                                    &tid,
                                    task_id,
                                    &self.session_manager,
                                    preferred_session_id,
                                    &self.event_tx,
                                    &self.data_dir,
                                    &self.http_client,
                                    Some(stream_cancel_token.clone()),
                                )
                                .await
                            }
                        } else {
                            consecutive_same_tool_calls = 1;
                            execute_tool(
                                tc,
                                self,
                                &tid,
                                task_id,
                                &self.session_manager,
                                preferred_session_id,
                                &self.event_tx,
                                &self.data_dir,
                                &self.http_client,
                                Some(stream_cancel_token.clone()),
                            )
                            .await
                        };
                        previous_tool_signature = Some(current_tool_signature);

                        // Record step for trace collection and update termination metrics
                        termination_tool_calls += 1;
                        if result.is_error {
                            termination_consecutive_errors += 1;
                            termination_total_errors += 1;
                        } else {
                            termination_consecutive_errors = 0;
                            termination_tool_successes += 1;
                        }
                        if task_id.is_some() {
                            trace_collector.record_step(
                                &tc.function.name,
                                &crate::agent::learning::traces::hash_arguments(
                                    &tc.function.arguments,
                                ),
                                !result.is_error,
                                0, // duration not tracked at this level
                                0, // tokens tracked at message level
                                if result.is_error {
                                    Some(result.content.clone())
                                } else {
                                    None
                                },
                                now_millis(),
                            );
                        }

                        if tc.function.name == "update_memory" && !result.is_error {
                            self.refresh_memory_cache().await;
                        }

                        if let Some((previous_tool_name, previous_was_error)) =
                            previous_tool_outcome.as_ref()
                        {
                            if let Err(error) = self
                                .record_tool_hesitation(
                                    previous_tool_name,
                                    tc.function.name.as_str(),
                                    *previous_was_error,
                                    result.is_error,
                                )
                                .await
                            {
                                tracing::warn!(error = %error, "failed to record implicit tool fallback feedback");
                            }
                        }
                        previous_tool_outcome = Some((tc.function.name.clone(), result.is_error));
                        if result.is_error {
                            last_tool_error =
                                Some((tc.function.name.clone(), result.content.clone()));
                        } else {
                            last_tool_error = None;
                        }

                        if !result.is_error {
                            self.capture_tool_work_context(
                                &tid,
                                task_id,
                                tc.function.name.as_str(),
                                tc.function.arguments.as_str(),
                            )
                            .await;
                        }

                        self.persist_tool_selection_causal_trace(
                            &tid,
                            current_task_snapshot
                                .as_ref()
                                .and_then(|task| task.goal_run_id.as_deref()),
                            task_id,
                            tc,
                            decision_reasoning.as_deref(),
                            &result,
                            &trace_collector,
                            &config,
                            &provider_config,
                        )
                        .await;
                        self.record_provenance_event(
                            "tool_call",
                            "agent executed tool call",
                            serde_json::json!({
                                "tool": tc.function.name.as_str(),
                                "arguments": tc.function.arguments.as_str(),
                                "is_error": result.is_error,
                            }),
                            current_task_snapshot
                                .as_ref()
                                .and_then(|task| task.goal_run_id.as_deref()),
                            task_id,
                            Some(tid.as_str()),
                            None,
                            None,
                        )
                        .await;

                        // Update counter-who self-model with tool result (Phase 1: Memory Foundation - CWHO-01)
                        {
                            let args_summary: String =
                                tc.function.arguments.chars().take(100).collect();
                            self.update_counter_who_on_tool_result(
                                &tid,
                                &tc.function.name,
                                &args_summary,
                                !result.is_error,
                            )
                            .await;
                        }

                        // Record outcome for situational awareness (Phase 2: AWAR-01)
                        {
                            let args_summary: String =
                                tc.function.arguments.chars().take(100).collect();
                            let args_hash = super::episodic::counter_who::compute_approach_hash(
                                &tc.function.name,
                                &args_summary,
                            );
                            let is_progress = !result.is_error && result.content.len() > 50;
                            self.record_awareness_outcome(
                                &tid,
                                "thread",
                                &tc.function.name,
                                &args_hash,
                                !result.is_error,
                                is_progress,
                            )
                            .await;
                            // Check for mode shift (AWAR-02 + AWAR-03)
                            self.check_awareness_mode_shift(&tid, &tid).await;
                        }

                        let _ = self.event_tx.send(AgentEvent::ToolResult {
                            thread_id: tid.clone(),
                            call_id: tc.id.clone(),
                            name: result.name.clone(),
                            content: result.content.clone(),
                            is_error: result.is_error,
                        });

                        // Add tool result message
                        {
                            let tool_status = if result.is_error { "error" } else { "done" };
                            let mut threads = self.threads.write().await;
                            if let Some(thread) = threads.get_mut(&tid) {
                                thread.messages.push(AgentMessage {
                                    id: generate_message_id(),
                                    role: MessageRole::Tool,
                                    content: result.content,
                                    tool_calls: None,
                                    tool_call_id: Some(result.tool_call_id),
                                    tool_name: Some(result.name),
                                    tool_arguments: Some(tc.function.arguments.clone()),
                                    tool_status: Some(tool_status.to_string()),
                                    input_tokens: 0,
                                    output_tokens: 0,
                                    provider: None,
                                    model: None,
                                    api_transport: None,
                                    response_id: None,
                                    reasoning: None,
                                    timestamp: now_millis(),
                                });
                            }
                        }
                        let current_tokens = {
                            let threads = self.threads.read().await;
                            threads
                                .get(&tid)
                                .map(|thread| estimate_message_tokens(&thread.messages))
                                .unwrap_or(0) as u32
                        };
                        if let Some(task) = current_task_snapshot.as_ref() {
                            self.record_subagent_tool_result(
                                task,
                                &tid,
                                &tc.function.name,
                                result.is_error,
                                current_tokens,
                            )
                            .await;
                            self.persist_subagent_runtime_metrics(&task.id).await;
                        }

                        if let Some(pending_approval) = result.pending_approval.as_ref() {
                            let _ = self
                                .record_operator_approval_requested(pending_approval)
                                .await;
                            self.record_provenance_event(
                                "approval_requested",
                                "tool execution requested operator approval",
                                serde_json::json!({
                                    "approval_id": pending_approval.approval_id,
                                    "command": pending_approval.command,
                                    "risk_level": pending_approval.risk_level,
                                    "blast_radius": pending_approval.blast_radius,
                                }),
                                current_task_snapshot
                                    .as_ref()
                                    .and_then(|task| task.goal_run_id.as_deref()),
                                task_id,
                                Some(tid.as_str()),
                                Some(pending_approval.approval_id.as_str()),
                                None,
                            )
                            .await;
                            interrupted_for_approval = true;
                            if let Some(task_id) = task_id {
                                self.mark_task_awaiting_approval(task_id, &tid, pending_approval)
                                    .await;
                            }
                            break 'agent_loop;
                        }

                        if stream_cancel_token.is_cancelled() {
                            was_cancelled = true;
                            break;
                        }
                    }

                    if was_cancelled {
                        break 'agent_loop;
                    }

                    // Check termination conditions (DSL-based)
                    if let Some(ref evaluator) = task_termination_eval {
                        let elapsed = now_millis().saturating_sub(loop_started_at) / 1000;
                        let metrics = crate::agent::subagent::termination::TerminationMetrics {
                            elapsed_secs: elapsed,
                            tool_calls_total: termination_tool_calls,
                            tool_calls_succeeded: termination_tool_successes,
                            consecutive_errors: termination_consecutive_errors,
                            total_errors: termination_total_errors,
                        };
                        let (should_stop, reason) = evaluator.should_terminate(&metrics);
                        if should_stop {
                            tracing::info!(
                                thread_id = %tid,
                                reason = ?reason,
                                "sub-agent terminated by condition"
                            );
                            self.emit_workflow_notice(
                                &tid,
                                "termination-triggered",
                                &format!(
                                    "Sub-agent terminated: {}",
                                    reason.as_deref().unwrap_or("condition met")
                                ),
                                None,
                            );
                            break 'agent_loop;
                        }
                    }

                    // Check context budget
                    // Check budget every 5 tool calls to avoid full-message scan on each iteration
                    if termination_tool_calls.is_multiple_of(5) {
                        if let Some(ref mut budget) = task_context_budget {
                            let current_tokens = {
                                let threads = self.threads.read().await;
                                threads
                                    .get(&tid)
                                    .map(|t| estimate_message_tokens(&t.messages))
                                    .unwrap_or(0) as u32
                            };
                            budget.set_consumed(current_tokens);
                            match budget.check() {
                            crate::agent::subagent::context_budget::BudgetStatus::Exceeded { overflow_action, .. } => {
                                match overflow_action {
                                    crate::agent::types::ContextOverflowAction::Error => {
                                        tracing::warn!(thread_id = %tid, "context budget exceeded — stopping");
                                        self.emit_workflow_notice(&tid, "budget-exceeded", "Context budget exceeded, execution stopped.", None);
                                        break 'agent_loop;
                                    }
                                    _ => {
                                        // Compress/Truncate: the existing compaction in prepare_llm_request handles this
                                        tracing::info!(thread_id = %tid, "context budget exceeded — relying on compaction");
                                    }
                                }
                            }
                            crate::agent::subagent::context_budget::BudgetStatus::Warning { consumed, max } => {
                                tracing::debug!(thread_id = %tid, consumed, max, "context budget warning");
                            }
                            _ => {}
                        }
                        }
                    } // end budget check every 5 tool calls

                    // Loop continues — next iteration will include tool results in context
                }
                _ => {
                    // Stream ended unexpectedly
                    self.record_llm_outcome(&config.provider, false).await;
                    let fallback_message =
                        unexpected_stream_end_message(&accumulated_content);
                    self.add_assistant_message(
                        &tid,
                        &fallback_message,
                        0,
                        0,
                        None,
                        Some(config.provider.clone()),
                        Some(provider_config.model.clone()),
                        Some(provider_config.api_transport),
                        None,
                    )
                    .await;
                    break;
                }
            }
        }

        if !was_cancelled && max_loops > 0 && loop_count >= max_loops {
            let _ = self.event_tx.send(AgentEvent::Error {
                thread_id: tid.clone(),
                message: "Tool execution limit reached".into(),
            });
        }

        // Finalize execution trace and persist (only for task-driven turns)
        if task_id.is_some() {
            let trace_outcome = if interrupted_for_approval {
                crate::agent::learning::traces::TraceOutcome::Partial {
                    completed_pct: 50.0,
                }
            } else if was_cancelled {
                crate::agent::learning::traces::TraceOutcome::Cancelled
            } else {
                crate::agent::learning::traces::TraceOutcome::Success
            };
            let final_success_rate = trace_collector.success_rate();
            let trace = trace_collector.finalize(
                trace_outcome,
                None,
                task_id.map(str::to_string),
                None,
                now_millis(),
            );
            if !trace.steps.is_empty() {
                let tool_seq = crate::agent::learning::traces::extract_tool_sequence(&trace);
                let tool_seq_json = serde_json::to_string(&tool_seq).unwrap_or_default();
                let metrics_json = serde_json::to_string(&serde_json::json!({
                    "total_duration_ms": trace.total_duration_ms,
                    "total_tokens_used": trace.total_tokens_used,
                    "step_count": trace.steps.len(),
                    "success_rate": final_success_rate,
                }))
                .unwrap_or_default();
                let outcome_str = match &trace.outcome {
                    crate::agent::learning::traces::TraceOutcome::Success => "success",
                    crate::agent::learning::traces::TraceOutcome::Failure { .. } => "failure",
                    crate::agent::learning::traces::TraceOutcome::Partial { .. } => "partial",
                    crate::agent::learning::traces::TraceOutcome::Cancelled => "cancelled",
                };
                if let Err(e) = self
                    .history
                    .insert_execution_trace(
                        &trace.trace_id,
                        None,
                        task_id,
                        &trace.task_type,
                        outcome_str,
                        trace.quality_score,
                        &tool_seq_json,
                        &metrics_json,
                        trace.total_duration_ms,
                        trace.total_tokens_used,
                        trace.created_at,
                    )
                    .await
                {
                    tracing::warn!(task_id, "failed to persist execution trace: {e}");
                }
            }
        }

        if let Some(task) = current_task_snapshot.as_ref() {
            self.persist_subagent_runtime_metrics(&task.id).await;
        }

        self.persist_threads().await;
        self.finish_stream_cancellation(&tid, stream_generation)
            .await;
        Ok(SendMessageOutcome {
            thread_id: tid,
            interrupted_for_approval,
        })
        }))
        .await
    }

    pub(super) fn resolve_provider_config(&self, config: &AgentConfig) -> Result<ProviderConfig> {
        resolve_active_provider_config(config)
    }

    /// Resolve provider config for a named sub-agent's provider.
    /// Falls back to the normal resolve_provider_config if the sub-agent's
    /// provider matches the current top-level provider.
    pub(super) fn resolve_sub_agent_provider_config(
        &self,
        config: &AgentConfig,
        sub_agent_provider: &str,
    ) -> Result<ProviderConfig> {
        resolve_provider_config_for(config, sub_agent_provider, None)
    }

    /// Repair a thread's message sequence by removing broken tool-call/result
    /// pairs. Walks messages and ensures every Assistant with tool_calls is
    /// immediately followed by matching Tool results. Drops orphaned messages.
    async fn repair_tool_call_sequence(&self, thread_id: &str) {
        let removed = {
            let mut threads = self.threads.write().await;
            let Some(thread) = threads.get_mut(thread_id) else {
                return;
            };
            let before = thread.messages.len();
            let mut repaired = Vec::with_capacity(before);
            let mut i = 0;
            while i < thread.messages.len() {
                let msg = &thread.messages[i];
                if msg.role == MessageRole::Assistant && msg.tool_calls.is_some() {
                    let tool_calls = msg.tool_calls.as_ref().unwrap();
                    let expected: std::collections::HashSet<&str> =
                        tool_calls.iter().map(|tc| tc.id.as_str()).collect();
                    // Scan forward for matching tool results.
                    let mut results = Vec::new();
                    let mut matched = std::collections::HashSet::new();
                    let mut j = i + 1;
                    while j < thread.messages.len() && thread.messages[j].role == MessageRole::Tool
                    {
                        if thread.messages[j]
                            .tool_call_id
                            .as_deref()
                            .map(|id| expected.contains(id))
                            .unwrap_or(false)
                        {
                            results.push(thread.messages[j].clone());
                            if let Some(id) = thread.messages[j].tool_call_id.as_deref() {
                                matched.insert(id);
                            }
                        }
                        j += 1;
                    }
                    let has_complete_batch = matched.len() == expected.len();
                    let saw_no_followup_messages = j == i + 1;
                    let is_unanswered_latest_tool_turn =
                        saw_no_followup_messages && j == thread.messages.len();
                    if has_complete_batch || is_unanswered_latest_tool_turn {
                        repaired.push(msg.clone());
                        if has_complete_batch {
                            repaired.extend(results);
                        }
                    }
                    i = j;
                } else if msg.role == MessageRole::Tool {
                    // Orphaned tool result — skip.
                    i += 1;
                } else {
                    repaired.push(msg.clone());
                    i += 1;
                }
            }
            let removed = before - repaired.len();
            if removed > 0 {
                tracing::info!(
                    "repair_tool_call_sequence: removed {} broken messages from thread {}",
                    removed,
                    thread_id
                );
                thread.messages = repaired;
                thread.updated_at = now_millis();
                thread.total_input_tokens = thread.messages.iter().map(|m| m.input_tokens).sum();
                thread.total_output_tokens = thread.messages.iter().map(|m| m.output_tokens).sum();
            }
            removed
        };

        if removed > 0 {
            self.persist_thread_by_id(thread_id).await;
        }
    }

    pub(super) async fn add_assistant_message(
        &self,
        thread_id: &str,
        content: &str,
        input_tokens: u64,
        output_tokens: u64,
        reasoning: Option<String>,
        provider: Option<String>,
        model: Option<String>,
        api_transport: Option<ApiTransport>,
        response_id: Option<String>,
    ) {
        let mut threads = self.threads.write().await;
        if let Some(thread) = threads.get_mut(thread_id) {
            thread.messages.push(AgentMessage {
                id: generate_message_id(),
                role: MessageRole::Assistant,
                content: content.into(),
                tool_calls: None,
                tool_call_id: None,
                tool_name: None,
                tool_arguments: None,
                tool_status: None,
                input_tokens,
                output_tokens,
                provider,
                model,
                api_transport,
                response_id,
                reasoning,
                timestamp: now_millis(),
            });
            thread.total_input_tokens += input_tokens;
            thread.total_output_tokens += output_tokens;
            thread.updated_at = now_millis();
        }
        drop(threads);
        self.persist_thread_by_id(thread_id).await;
        if let Err(error) = self.maybe_sync_thread_to_honcho(thread_id).await {
            tracing::warn!(thread_id = %thread_id, error = %error, "failed to sync assistant message to Honcho");
        }
    }

    pub(super) async fn emit_turn_error_completion(
        &self,
        thread_id: &str,
        message: &str,
        provider: Option<String>,
        model: Option<String>,
    ) {
        let _ = self.event_tx.send(AgentEvent::Delta {
            thread_id: thread_id.to_string(),
            content: format!("Error: {message}"),
        });
        let _ = self.event_tx.send(AgentEvent::Done {
            thread_id: thread_id.to_string(),
            input_tokens: 0,
            output_tokens: 0,
            cost: None,
            provider,
            model,
            tps: None,
            generation_ms: None,
            reasoning: None,
        });
    }

    pub(super) async fn update_thread_upstream_state(
        &self,
        thread_id: &str,
        provider: &str,
        model: &str,
        transport: ApiTransport,
        assistant_id: Option<&str>,
        upstream_thread_id: Option<String>,
    ) {
        let mut threads = self.threads.write().await;
        if let Some(thread) = threads.get_mut(thread_id) {
            thread.upstream_transport = Some(transport);
            thread.upstream_provider = Some(provider.to_string());
            thread.upstream_model = Some(model.to_string());
            thread.upstream_assistant_id = assistant_id
                .filter(|value| !value.trim().is_empty())
                .map(|value| value.to_string());
            thread.upstream_thread_id = upstream_thread_id;
            thread.updated_at = now_millis();
        }
        drop(threads);
        self.persist_thread_by_id(thread_id).await;
    }
}

impl AgentEngine {
    /// Check if user content is a plugin command and return a system hint for the LLM.
    ///
    /// Does NOT bypass the LLM -- instead augments the conversation with context
    /// so the agent naturally uses the plugin API tool.
    pub(super) async fn try_augment_plugin_command(&self, content: &str) -> Option<String> {
        let (command_key, args) = parse_plugin_command(content)?;
        let pm = self.plugin_manager.get()?;
        let entry = pm.resolve_command(command_key).await?;
        let endpoint = entry.api_endpoint.as_deref().unwrap_or("default");
        let args_part = if args.is_empty() {
            String::new()
        } else {
            format!(" with arguments: {}", args)
        };
        Some(format!(
            "[Plugin command: {}]\n\
             The user invoked plugin command `{}`. \
             Plugin: '{}'. Description: {}. \
             Call the plugin API endpoint '{}' for plugin '{}'{} to fulfill this request.",
            entry.command_key,
            entry.command_key,
            entry.plugin_name,
            entry.description,
            endpoint,
            entry.plugin_name,
            args_part,
        ))
    }
}

// ---------------------------------------------------------------------------
// Cost accumulation helpers (COST-01 through COST-03)
// ---------------------------------------------------------------------------

impl AgentEngine {
    /// Find a running goal run that is using the given thread_id.
    pub(super) async fn find_active_goal_run_for_thread(&self, thread_id: &str) -> Option<String> {
        let goal_runs = self.goal_runs.lock().await;
        goal_runs
            .iter()
            .find(|gr| {
                matches!(gr.status, GoalRunStatus::Running | GoalRunStatus::Planning)
                    && gr.thread_id.as_deref() == Some(thread_id)
            })
            .map(|gr| gr.id.clone())
    }

    /// Accumulate cost for a goal run after an LLM API call.
    ///
    /// Called from exactly two places in `send_message_inner`:
    /// 1. After `CompletionChunk::Done` (no tool calls)
    /// 2. After `CompletionChunk::ToolCalls` (has tool calls)
    ///
    /// This is the ONLY cost accumulation point to prevent double-counting.
    pub(super) async fn accumulate_goal_run_cost(
        &self,
        thread_id: &str,
        input_tokens: u64,
        output_tokens: u64,
        provider: &str,
        model: &str,
    ) {
        let goal_run_id = match self.find_active_goal_run_for_thread(thread_id).await {
            Some(id) => id,
            None => return,
        };

        let config = self.config.read().await;
        if !config.cost.enabled {
            return;
        }
        let rate_cards = config.cost.rate_cards.clone();
        let threshold = config.cost.budget_alert_threshold_usd;
        drop(config);

        let mut trackers = self.cost_trackers.lock().await;
        let tracker = trackers
            .entry(goal_run_id.clone())
            .or_insert_with(super::cost::CostTracker::new);
        tracker.accumulate(input_tokens, output_tokens, provider, model, &rate_cards);

        if tracker.budget_alert_needed(threshold) {
            if let Some(cost) = tracker.summary().estimated_cost_usd {
                let _ = self.event_tx.send(AgentEvent::BudgetAlert {
                    goal_run_id: goal_run_id.clone(),
                    current_cost_usd: cost,
                    threshold_usd: threshold.unwrap_or(0.0),
                });
            }
        }
    }
}

fn retry_failure_class_from_message(message: &str) -> &'static str {
    let lower = message.to_ascii_lowercase();
    if lower.contains("429") || lower.contains("rate limit") || lower.contains("too many requests")
    {
        "rate_limit"
    } else if lower.contains("timed out") || lower.contains("timeout") {
        "timeout"
    } else if lower.contains("connection")
        || lower.contains("broken pipe")
        || lower.contains("unexpected eof")
        || lower.contains("reset")
    {
        "transport"
    } else {
        "upstream"
    }
}

fn is_transient_retry_message(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    lower.contains("429")
        || lower.contains("rate limit")
        || lower.contains("too many requests")
        || lower.contains("timed out")
        || lower.contains("timeout")
        || lower.contains("connection")
        || lower.contains("broken pipe")
        || lower.contains("unexpected eof")
        || lower.contains("overloaded")
        || lower.contains("unavailable")
        || lower.contains("try again later")
}

/// Parse a plugin command from user input.
///
/// A plugin command starts with `/`, contains a `.` before any space
/// (e.g., `/gmail-calendar.inbox`, `/weather.forecast London`).
///
/// Returns `Some((command_key, args))` where `command_key` is the `/plugin.command`
/// portion and `args` is the remaining text after the command.
/// Returns `None` if the input is not a plugin command.
pub(super) fn parse_plugin_command(content: &str) -> Option<(&str, &str)> {
    let trimmed = content.trim();
    if !trimmed.starts_with('/') {
        return None;
    }

    // Find the first space (if any) to separate command from args
    let (command_part, args) = match trimmed.find(' ') {
        Some(pos) => (&trimmed[..pos], trimmed[pos..].trim_start()),
        None => (trimmed, ""),
    };

    // Command part must contain a dot (plugin.command separator)
    if !command_part.contains('.') {
        return None;
    }

    Some((command_part, args))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session_manager::SessionManager;
    use tempfile::tempdir;

    #[test]
    fn parse_plugin_command_basic() {
        let result = parse_plugin_command("/gmail-calendar.inbox");
        assert_eq!(result, Some(("/gmail-calendar.inbox", "")));
    }

    #[test]
    fn parse_plugin_command_with_args() {
        let result = parse_plugin_command("/gmail-calendar.inbox check today");
        assert_eq!(result, Some(("/gmail-calendar.inbox", "check today")));
    }

    #[test]
    fn parse_plugin_command_regular_message() {
        let result = parse_plugin_command("regular message");
        assert_eq!(result, None);
    }

    #[test]
    fn parse_plugin_command_no_dot() {
        // /help has no dot separator -- not a plugin command
        let result = parse_plugin_command("/help");
        assert_eq!(result, None);
    }

    #[test]
    fn parse_plugin_command_with_whitespace() {
        let result = parse_plugin_command("  /weather.forecast London  ");
        assert_eq!(result, Some(("/weather.forecast", "London")));
    }

    #[test]
    fn parse_plugin_command_slash_no_dot_with_args() {
        let result = parse_plugin_command("/help me please");
        assert_eq!(result, None);
    }

    #[tokio::test]
    async fn repair_tool_call_sequence_updates_persisted_history() {
        let root = tempdir().unwrap();
        let manager = SessionManager::new_test(root.path()).await;
        let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
        let thread_id = "thread_repair";

        {
            let mut threads = engine.threads.write().await;
            threads.insert(
                thread_id.to_string(),
                AgentThread {
                    id: thread_id.to_string(),
                    title: "Repair test".to_string(),
                    created_at: 1,
                    updated_at: 1,
                    pinned: false,
                    upstream_thread_id: None,
                    upstream_transport: None,
                    upstream_provider: None,
                    upstream_model: None,
                    upstream_assistant_id: None,
                    total_input_tokens: 0,
                    total_output_tokens: 0,
                    messages: vec![
                        AgentMessage::user("start", 1),
                        AgentMessage {
                            id: "assistant-tool-turn".to_string(),
                            role: MessageRole::Assistant,
                            content: "checking".to_string(),
                            tool_calls: Some(vec![
                                ToolCall {
                                    id: "2013".to_string(),
                                    function: ToolFunction {
                                        name: "tool_a".to_string(),
                                        arguments: "{}".to_string(),
                                    },
                                },
                                ToolCall {
                                    id: "2014".to_string(),
                                    function: ToolFunction {
                                        name: "tool_b".to_string(),
                                        arguments: "{}".to_string(),
                                    },
                                },
                            ]),
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
                            timestamp: 2,
                        },
                        AgentMessage {
                            id: "tool-result-2013".to_string(),
                            role: MessageRole::Tool,
                            content: "partial".to_string(),
                            tool_calls: None,
                            tool_call_id: Some("2013".to_string()),
                            tool_name: Some("tool_a".to_string()),
                            tool_arguments: Some("{}".to_string()),
                            tool_status: Some("done".to_string()),
                            input_tokens: 0,
                            output_tokens: 0,
                            provider: None,
                            model: None,
                            api_transport: None,
                            response_id: None,
                            reasoning: None,
                            timestamp: 3,
                        },
                        AgentMessage::user("continue", 4),
                    ],
                },
            );
        }
        engine.persist_thread_by_id(thread_id).await;

        engine.repair_tool_call_sequence(thread_id).await;

        let live = engine.threads.read().await;
        let thread = live.get(thread_id).expect("thread should still exist");
        assert_eq!(thread.messages.len(), 2);
        assert_eq!(thread.messages[0].content, "start");
        assert_eq!(thread.messages[1].content, "continue");
        drop(live);

        let persisted = engine
            .history
            .list_messages(thread_id, Some(10))
            .await
            .unwrap();
        assert_eq!(persisted.len(), 2);
        assert_eq!(persisted[0].content, "start");
        assert_eq!(persisted[1].content, "continue");
    }
}
