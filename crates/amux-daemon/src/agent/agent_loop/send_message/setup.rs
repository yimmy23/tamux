use super::*;

impl<'a> SendMessageRunner<'a> {
    pub(super) async fn initialize(
        engine: &'a AgentEngine,
        thread_id: Option<&'a str>,
        stored_user_content: &'a str,
        llm_user_content: &'a str,
        task_id: Option<&'a str>,
        preferred_session_hint: Option<&'a str>,
        stream_chunk_timeout_override: Option<std::time::Duration>,
        record_operator: bool,
    ) -> Result<Self> {
        let config = engine.config.read().await.clone();

        let (tid, is_new_thread) = engine
            .get_or_create_thread(thread_id, stored_user_content)
            .await;
        {
            let mut threads = engine.threads.write().await;
            if let Some(thread) = threads.get_mut(&tid) {
                thread
                    .messages
                    .push(AgentMessage::user(stored_user_content, now_millis()));
                thread.updated_at = now_millis();
            }
        }
        engine.persist_thread_by_id(&tid).await;
        if record_operator {
            engine
                .record_operator_message(&tid, stored_user_content, is_new_thread)
                .await?;
            if let Err(error) = engine.maybe_sync_thread_to_honcho(&tid).await {
                tracing::warn!(thread_id = %tid, error = %error, "failed to sync thread to Honcho");
            }
        }

        if let Some(ack_message) = engine.take_continuity_acknowledgment(&tid).await {
            let mut threads = engine.threads.write().await;
            if let Some(thread) = threads.get_mut(&tid) {
                let mut msg = AgentMessage::user(&ack_message, now_millis());
                msg.role = MessageRole::System;
                thread.messages.push(msg);
            }
        }
        if let Some(hint) = engine.try_augment_plugin_command(stored_user_content).await {
            let mut threads = engine.threads.write().await;
            if let Some(thread) = threads.get_mut(&tid) {
                let mut msg = AgentMessage::user(&hint, now_millis());
                msg.role = MessageRole::System;
                thread.messages.push(msg);
            }
        }

        let task_provider_override = {
            let tasks = engine.tasks.lock().await;
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
                let mut pc = engine.resolve_sub_agent_provider_config(&config, sub_provider)?;
                if let Some(model) = sub_model {
                    pc.model = model.clone();
                }
                Ok(pc)
            } else {
                engine.resolve_provider_config(&config)
            } {
                Ok(provider_config) => provider_config,
                Err(error) => {
                    let error_text = sanitize_upstream_failure_message(&error.to_string());
                    engine
                        .add_assistant_message(
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
                    engine.persist_threads().await;
                    engine
                        .emit_turn_error_completion(&tid, &error_text, None, None)
                        .await;
                    return Err(error);
                }
            };

        let (stream_generation, stream_cancel_token) = engine.begin_stream_cancellation(&tid).await;
        let onecontext_bootstrap = if is_new_thread {
            engine
                .onecontext_bootstrap_for_new_thread(stored_user_content)
                .await
        } else {
            None
        };
        let preferred_session_id =
            resolve_preferred_session_id(&engine.session_manager, preferred_session_hint).await;
        let skill_preflight = engine
            .build_skill_preflight_context(stored_user_content, preferred_session_id.clone())
            .await?;

        let agent_scope_id = current_agent_scope_id();
        let memory = engine.current_memory_snapshot().await;
        let memory_paths = memory_paths_for_scope(&engine.data_dir, &agent_scope_id);
        let base_prompt = if let Some((_, _, Some(ref override_prompt))) = task_provider_override {
            format!("{}\n\n{}", override_prompt, config.system_prompt)
        } else {
            config.system_prompt.clone()
        };
        let operator_model_summary = engine.build_operator_model_prompt_summary().await;
        let operational_context = engine.build_operational_context_summary().await;
        let causal_guidance = engine.build_causal_guidance_summary().await;
        let learned_patterns = {
            let hs = engine.heuristic_store.read().await;
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
            None,
            None,
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
        match engine
            .maybe_build_honcho_context(&tid, stored_user_content)
            .await
        {
            Ok(Some(honcho_context)) => {
                system_prompt.push_str("\n\n## Cross-Session Memory\n");
                system_prompt.push_str(&honcho_context);
            }
            Ok(None) => {}
            Err(error) => {
                tracing::warn!(thread_id = %tid, error = %error, "failed to build Honcho context");
            }
        }
        engine.emit_workflow_notice(
            &tid,
            "memory-consulted",
            "Loaded persistent memory, user profile, and local skill paths for this turn.",
            Some(format!(
                "memory_dir={}; skills_dir={}",
                memory_paths.memory_dir.display(),
                skills_dir(&engine.data_dir).display()
            )),
        );
        if skill_preflight.is_some() {
            engine.emit_workflow_notice(
                &tid,
                "skill-preflight",
                "Preloaded relevant local skills for this turn before tool execution.",
                None,
            );
        }

        let has_workspace_topology = engine.session_manager.read_workspace_topology().is_some();
        let mut tools = get_available_tools(&config, &engine.data_dir, has_workspace_topology);
        let (
            current_task_snapshot,
            is_durable_goal_task,
            task_tool_filter,
            task_context_budget,
            task_termination_eval,
            task_type_for_trace,
        ) = {
            let tasks = engine.tasks.lock().await;
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
        if !task_type_for_trace.is_empty() {
            let hs = engine.heuristic_store.read().await;
            super::tool_executor::reorder_tools_by_heuristics(
                &mut tools,
                &hs,
                &task_type_for_trace,
            );
        }
        if let Some(task) = current_task_snapshot.as_ref() {
            engine.ensure_subagent_runtime(task, Some(&tid)).await;
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
        let max_loops = if is_durable_goal_task {
            0
        } else {
            config.max_tool_loops
        };

        Ok(Self {
            engine,
            task_id,
            stored_user_content,
            llm_user_content,
            stream_chunk_timeout_override,
            tid,
            config,
            provider_config,
            preferred_session_id,
            onecontext_bootstrap,
            skill_preflight,
            agent_scope_id,
            active_provider_id,
            memory_paths,
            base_prompt,
            operator_model_summary,
            operational_context,
            learned_patterns,
            system_prompt,
            current_task_snapshot,
            is_durable_goal_task,
            task_tool_filter,
            task_context_budget,
            task_termination_eval,
            task_type_for_trace: task_type_for_trace.clone(),
            tools,
            retry_strategy,
            max_loops,
            stream_generation,
            stream_cancel_token,
            loop_count: 0,
            was_cancelled: false,
            interrupted_for_approval: false,
            policy_aborted_retry: false,
            previous_tool_signature: None,
            previous_tool_outcome: None,
            last_tool_error: None,
            consecutive_same_tool_calls: 0,
            last_pre_compaction_flush_signature: None,
            recorded_compaction_provenance: false,
            trace_collector: crate::agent::learning::traces::TraceCollector::new(
                &task_type_for_trace,
                now_millis(),
            ),
            termination_tool_calls: 0,
            termination_tool_successes: 0,
            termination_consecutive_errors: 0,
            termination_total_errors: 0,
            loop_started_at: now_millis(),
            stream_timeout_count: 0,
            tool_ack_emitted: false,
            tool_sequence_repaired: false,
            retry_status_visible: false,
            assistant_output_visible: false,
            tool_side_effect_committed: false,
            attempted_recovery_signatures: std::collections::HashSet::new(),
            recent_policy_tool_outcomes: VecDeque::new(),
        })
    }
}
