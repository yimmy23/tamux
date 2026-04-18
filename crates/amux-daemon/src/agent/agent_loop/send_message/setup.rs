use super::*;
use crate::agent::llm_client::CopilotInitiator;
use amux_protocol::SecurityLevel;

const COMMUNITY_SCOUT_RESULT_LIMIT: usize = 5;
const PARTICIPANT_AGENT_FANOUT_TOOLS: &[&str] = &[
    "spawn_subagent",
    "message_agent",
    "route_to_specialist",
    "run_divergent",
];

#[derive(Clone)]
struct DirectThreadResponderConfig {
    agent_name: String,
    provider_id: String,
    model: Option<String>,
    reasoning_effort: Option<String>,
    system_prompt: String,
    persona_prompt: String,
    tool_filter: Option<crate::agent::subagent::tool_filter::ToolFilter>,
}
fn build_direct_thread_responder_config(
    config: &AgentConfig,
    agent_scope_id: &str,
    sub_agents: &[SubAgentDefinition],
) -> Result<Option<DirectThreadResponderConfig>> {
    let canonical_scope = canonical_agent_id(agent_scope_id);
    if canonical_scope == MAIN_AGENT_ID {
        return Ok(None);
    }
    if canonical_scope == CONCIERGE_AGENT_ID {
        let provider_id = config
            .concierge
            .provider
            .as_deref()
            .unwrap_or(&config.provider)
            .to_string();
        let Some(provider_config) =
            crate::agent::concierge::resolve_concierge_provider(config).ok()
        else {
            return Ok(None);
        };
        return Ok(Some(DirectThreadResponderConfig {
            agent_name: CONCIERGE_AGENT_NAME.to_string(),
            provider_id,
            model: Some(provider_config.model.clone()),
            reasoning_effort: Some(provider_config.reasoning_effort.clone()),
            system_prompt: crate::agent::concierge::concierge_system_prompt(),
            persona_prompt: String::new(),
            tool_filter: None,
        }));
    }
    let matched_def = if canonical_scope == crate::agent::agent_identity::WELES_AGENT_ID {
        sub_agents
            .iter()
            .find(|def| def.id == crate::agent::agent_identity::WELES_BUILTIN_SUBAGENT_ID)
            .cloned()
    } else {
        None
    };
    let builtin_persona_overrides = builtin_persona_overrides(config, canonical_scope);
    if is_explicit_builtin_persona_scope(canonical_scope)
        && builtin_persona_requires_setup(config, canonical_scope)
    {
        return Err(builtin_persona_setup_error(canonical_scope));
    }
    let persona_prompt = if canonical_scope == crate::agent::agent_identity::WELES_AGENT_ID {
        crate::agent::agent_identity::build_weles_persona_prompt(
            crate::agent::agent_identity::WELES_GOVERNANCE_SCOPE,
        )
    } else {
        build_spawned_persona_prompt(canonical_scope)
    };
    Ok(Some(DirectThreadResponderConfig {
        agent_name: canonical_agent_name(canonical_scope).to_string(),
        provider_id: matched_def
            .as_ref()
            .map(|def| def.provider.clone())
            .filter(|value| !value.trim().is_empty())
            .or_else(|| {
                builtin_persona_overrides
                    .and_then(|overrides| overrides.provider.clone())
                    .filter(|value| !value.trim().is_empty())
            })
            .unwrap_or_else(|| config.provider.clone()),
        model: matched_def
            .as_ref()
            .map(|def| def.model.clone())
            .filter(|value| !value.trim().is_empty())
            .or_else(|| {
                builtin_persona_overrides
                    .and_then(|overrides| overrides.model.clone())
                    .filter(|value| !value.trim().is_empty())
            }),
        reasoning_effort: matched_def
            .as_ref()
            .and_then(|def| def.reasoning_effort.clone())
            .filter(|value| !value.trim().is_empty())
            .or_else(|| {
                builtin_persona_overrides
                    .and_then(|overrides| overrides.reasoning_effort.clone())
                    .filter(|value| !value.trim().is_empty())
            }),
        system_prompt: matched_def
            .as_ref()
            .and_then(|def| def.system_prompt.clone())
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| config.system_prompt.clone()),
        persona_prompt,
        tool_filter: matched_def.as_ref().and_then(|def| {
            if def.tool_whitelist.is_some() || def.tool_blacklist.is_some() {
                crate::agent::subagent::tool_filter::ToolFilter::new(
                    def.tool_whitelist.clone(),
                    def.tool_blacklist.clone(),
                )
                .ok()
            } else {
                None
            }
        }),
    }))
}

async fn current_visible_thread_responder_is_active_participant(
    engine: &AgentEngine,
    thread_id: &str,
) -> bool {
    if is_internal_dm_thread(thread_id)
        || is_participant_playground_thread(thread_id)
        || is_internal_handoff_thread(thread_id)
    {
        return false;
    }
    let Some(active_agent_id) = engine.active_agent_id_for_thread(thread_id).await else {
        return false;
    };
    engine
        .list_thread_participants(thread_id)
        .await
        .into_iter()
        .any(|participant| {
            participant.status == ThreadParticipantStatus::Active
                && participant.agent_id.eq_ignore_ascii_case(&active_agent_id)
        })
}

async fn visible_thread_has_participants(engine: &AgentEngine, thread_id: &str) -> bool {
    if is_internal_dm_thread(thread_id)
        || is_participant_playground_thread(thread_id)
        || is_internal_handoff_thread(thread_id)
    {
        return false;
    }
    !engine.list_thread_participants(thread_id).await.is_empty()
}

fn spawn_background_community_scout(
    engine: &AgentEngine,
    thread_id: &str,
    query: &str,
    config: &AgentConfig,
) {
    if !config.skill_recommendation.background_community_search {
        return;
    }

    let event_tx = engine.event_tx.clone();
    let data_dir = engine
        .data_dir
        .parent()
        .unwrap_or(engine.data_dir.as_path())
        .to_path_buf();
    let registry_url = config
        .extra
        .get("registry_url")
        .and_then(|value| value.as_str())
        .unwrap_or("https://registry.tamux.dev")
        .to_string();
    let community_preapprove_timeout_secs = config
        .skill_recommendation
        .community_preapprove_timeout_secs;
    let suggest_global_enable_after_approvals = config
        .skill_recommendation
        .suggest_global_enable_after_approvals;
    let thread_id = thread_id.to_string();
    let query = query.to_string();

    tokio::spawn(async move {
        let (candidates, error) =
            match crate::agent::skill_recommendation::discover_community_skills(
                &data_dir,
                &registry_url,
                &query,
                COMMUNITY_SCOUT_RESULT_LIMIT,
            )
            .await
            {
                Ok(candidates) => (candidates, None),
                Err(error) => (Vec::new(), Some(error.to_string())),
            };

        let message = if candidates.is_empty() {
            "Background community scout found no additional install candidates.".to_string()
        } else {
            format!(
                "Background community scout found {} install candidate(s).",
                candidates.len()
            )
        };
        let details = serde_json::json!({
            "query": query,
            "candidates": candidates,
            "community_preapprove_timeout_secs": community_preapprove_timeout_secs,
            "suggest_global_enable_after_approvals": suggest_global_enable_after_approvals,
            "error": error,
        });

        let _ = event_tx.send(AgentEvent::WorkflowNotice {
            thread_id,
            kind: "skill-community-scout".to_string(),
            message,
            details: Some(details.to_string()),
        });
    });
}

impl<'a> SendMessageRunner<'a> {
    pub(super) async fn initialize(
        engine: &'a AgentEngine,
        thread_id: Option<&'a str>,
        stored_user_content: &'a str,
        llm_user_content: &'a str,
        task_id: Option<&'a str>,
        preferred_session_hint: Option<&'a str>,
        stream_chunk_timeout_override: Option<std::time::Duration>,
        client_surface: Option<amux_protocol::ClientSurface>,
        record_operator: bool,
        reuse_existing_user_message: bool,
        initial_scheduled_retry_cycles: u32,
    ) -> Result<Self> {
        let mut config = engine.config.read().await.clone();
        let (tid, is_new_thread) = engine
            .get_or_create_thread(thread_id, stored_user_content)
            .await;
        engine.ensure_thread_messages_loaded(&tid).await;
        if let Some(client_surface) = client_surface {
            engine.set_thread_client_surface(&tid, client_surface).await;
        }
        if !reuse_existing_user_message {
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
                            t.sub_agent_def_id.clone(),
                        )
                    })
                })
            })
        };
        let agent_scope_id = current_agent_scope_id();
        let sub_agents = engine.list_sub_agents().await;
        let direct_thread_responder = task_id
            .is_none()
            .then(|| build_direct_thread_responder_config(&config, &agent_scope_id, &sub_agents))
            .transpose()?
            .flatten();
        let active_provider_id = task_provider_override
            .as_ref()
            .map(|(provider_id, _, _, _)| provider_id.as_str())
            .or_else(|| {
                direct_thread_responder
                    .as_ref()
                    .map(|responder| responder.provider_id.as_str())
            })
            .unwrap_or(config.provider.as_str())
            .to_string();
        let provider_config =
            match if let Some((ref sub_provider, ref sub_model, _, _)) = task_provider_override {
                let mut pc = engine.resolve_sub_agent_provider_config(&config, sub_provider)?;
                if let Some(model) = sub_model {
                    pc.model = model.clone();
                }
                Ok(pc)
            } else if let Some(responder) = direct_thread_responder.as_ref() {
                let mut pc =
                    engine.resolve_sub_agent_provider_config(&config, &responder.provider_id)?;
                if let Some(model) = responder.model.as_ref() {
                    pc.model = model.clone();
                }
                if let Some(reasoning_effort) = responder.reasoning_effort.as_ref() {
                    pc.reasoning_effort = reasoning_effort.clone();
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
                    engine.persist_thread_by_id(&tid).await;
                    engine
                        .emit_turn_error_completion(&tid, &error_text, None, None)
                        .await;
                    return Err(error);
                }
            };
        // The active responder can override the provider/model selection, so the
        // runtime config used by the send loop must reflect that effective provider.
        config.provider = active_provider_id.clone();
        config.base_url = provider_config.base_url.clone();
        config.model = provider_config.model.clone();
        config.api_key = provider_config.api_key.clone();
        config.assistant_id = provider_config.assistant_id.clone();
        config.auth_source = provider_config.auth_source;
        config.api_transport = provider_config.api_transport;
        config.reasoning_effort = provider_config.reasoning_effort.clone();
        config.context_window_tokens = provider_config.context_window_tokens;
        let (stream_generation, stream_cancel_token, stream_retry_now) =
            engine.begin_stream_cancellation(&tid).await;
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
            .build_skill_preflight_context(&tid, stored_user_content, preferred_session_id.clone())
            .await?;
        let mut skill_preflight = match skill_preflight {
            Some(context) => Some(context),
            None => engine
                .get_thread_skill_discovery_state(&tid)
                .await
                .filter(|state| !state.compliant)
                .map(|state| super::skill_preflight::SkillPreflightContext {
                    prompt_context: super::skill_preflight::build_skill_gate_override_prompt(
                        &state,
                    ),
                    workflow_message: super::skill_preflight::skill_preflight_workflow_message(
                        &state,
                    ),
                    workflow_details: serde_json::to_string(&state).ok(),
                    state,
                }),
        };
        if let Some(skill_preflight) = skill_preflight.as_mut() {
            let mut next_state = skill_preflight.state.clone();
            if let Some(previous_state) = engine.get_thread_skill_discovery_state(&tid).await {
                if super::skill_preflight::preserve_noncompliant_mesh_state(
                    &previous_state,
                    &mut next_state,
                ) {
                    skill_preflight.workflow_message =
                        super::skill_preflight::skill_preflight_workflow_message(&next_state);
                    skill_preflight.workflow_details = serde_json::to_string(&next_state).ok();
                    skill_preflight.prompt_context =
                        super::skill_preflight::build_skill_gate_override_prompt(&next_state);
                }
            }
            skill_preflight.state = next_state.clone();
            engine
                .set_thread_skill_discovery_state(&tid, next_state)
                .await;
        }
        let memory = engine.current_memory_snapshot().await;
        let memory_paths = memory_paths_for_scope(&engine.data_dir, &agent_scope_id);
        let base_prompt = if let Some((_, _, Some(ref override_prompt), _)) = task_provider_override
        {
            format!("{}\n\n{}", override_prompt, config.system_prompt)
        } else if let Some(responder) = direct_thread_responder.as_ref() {
            format!(
                "{}\n\n{}",
                responder.persona_prompt, responder.system_prompt
            )
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
        let (
            current_task_snapshot,
            is_durable_goal_task,
            mut task_tool_filter,
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
        let internal_dm_thread = is_internal_dm_thread(&tid);
        let participant_playground_thread = is_participant_playground_thread(&tid);
        if internal_dm_thread && !participant_playground_thread {
            task_tool_filter = Some(crate::agent::subagent::tool_filter::ToolFilter::deny_all());
        }
        let initial_copilot_initiator = if record_operator {
            CopilotInitiator::User
        } else {
            CopilotInitiator::Agent
        };
        let weles_runtime_override = current_task_snapshot.as_ref().and_then(|task| {
            (task.sub_agent_def_id.as_deref()
                == Some(crate::agent::agent_identity::WELES_BUILTIN_SUBAGENT_ID))
            .then_some(task)
            .and_then(|task| {
                task.override_system_prompt
                    .as_deref()
                    .and_then(|override_prompt| {
                        crate::agent::weles_governance::parse_weles_internal_override_payload(
                            override_prompt,
                        )
                    })
            })
        });
        let runtime_context_query = select_runtime_context_query(
            current_task_snapshot.as_ref().and_then(|task| {
                task.goal_step_title
                    .as_deref()
                    .or(Some(task.title.as_str()))
            }),
            current_task_snapshot.as_ref().and_then(|task| {
                task.goal_run_title
                    .as_deref()
                    .or(Some(task.description.as_str()))
            }),
            Some(stored_user_content),
        );
        let runtime_work_scope = format_runtime_work_scope_label(
            current_task_snapshot
                .as_ref()
                .and_then(|task| task.goal_run_title.as_deref()),
            current_task_snapshot
                .as_ref()
                .and_then(|task| task.goal_step_title.as_deref()),
            current_task_snapshot
                .as_ref()
                .map(|task| task.title.as_str()),
        );
        let runtime_continuity = build_runtime_continuity_context(
            engine,
            runtime_work_scope.as_deref(),
            runtime_context_query.as_deref(),
        )
        .await;
        let structured_memory_summary =
            crate::agent::memory_context::build_structured_memory_summary(
                &memory,
                &memory_paths,
                runtime_continuity.continuity_summary.as_deref(),
                runtime_continuity.negative_constraints_context.as_deref(),
            );
        let existing_memory_injection_state = engine.get_thread_memory_injection_state(&tid).await;
        let mut system_prompt = if let Some((scope, _marker, inspection_context)) =
            weles_runtime_override.as_ref()
        {
            let tool_name = inspection_context
                .get("tool_name")
                .and_then(|value| value.as_str())
                .unwrap_or_default();
            let tool_args = inspection_context
                .get("tool_args")
                .unwrap_or(&serde_json::Value::Null);
            let security_level = match inspection_context
                .get("security_level")
                .and_then(|value| value.as_str())
                .unwrap_or("moderate")
            {
                "highest" => SecurityLevel::Highest,
                "lowest" => SecurityLevel::Lowest,
                "yolo" => SecurityLevel::Yolo,
                _ => SecurityLevel::Moderate,
            };
            let suspicion_reasons = inspection_context
                .get("suspicion_reasons")
                .and_then(|value| value.as_array())
                .map(|items| {
                    items
                        .iter()
                        .filter_map(|item| item.as_str())
                        .map(ToOwned::to_owned)
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            let task_health_signals = inspection_context.get("task_health_signals");
            let mut prompt = build_weles_governance_runtime_prompt(
                &config,
                tool_name,
                tool_args,
                security_level,
                &suspicion_reasons,
                current_task_snapshot.as_ref(),
                task_health_signals,
            );
            if scope == crate::agent::agent_identity::WELES_VITALITY_SCOPE {
                prompt.push_str("\n\n## WELES Vitality Mode\n- This run is an internal vitality/self-health check.");
            }
            prompt
        } else {
            build_system_prompt(
                &config,
                &base_prompt,
                &memory,
                &memory_paths,
                &agent_scope_id,
                &sub_agents,
                operator_model_summary.as_deref(),
                operational_context.as_deref(),
                causal_guidance.as_deref(),
                learned_patterns.as_deref(),
                None,
                runtime_continuity.continuity_summary.as_deref(),
                runtime_continuity.negative_constraints_context.as_deref(),
            )
        };
        let runtime_agent_name = task_provider_override
            .as_ref()
            .and_then(|(_, _, prompt, sub_agent_def_id)| {
                if sub_agent_def_id.as_deref()
                    == Some(crate::agent::agent_identity::WELES_BUILTIN_SUBAGENT_ID)
                {
                    Some(crate::agent::agent_identity::WELES_AGENT_NAME.to_string())
                } else {
                    extract_persona_name(prompt.as_deref())
                }
            })
            .or_else(|| {
                direct_thread_responder
                    .as_ref()
                    .map(|responder| responder.agent_name.clone())
            })
            .unwrap_or_else(|| MAIN_AGENT_NAME.to_string());
        system_prompt.push_str("\n\n");
        system_prompt.push_str(&build_runtime_identity_prompt(
            &runtime_agent_name,
            &active_provider_id,
            &provider_config.model,
        ));
        if let Some(injection_state) =
            crate::agent::memory_context::append_structured_memory_summary_if_needed(
                &mut system_prompt,
                existing_memory_injection_state.as_ref(),
                &structured_memory_summary,
                false,
            )
        {
            engine
                .set_thread_memory_injection_state(&tid, injection_state)
                .await;
        }
        if internal_dm_thread {
            system_prompt.push_str(
                "\n\n## Internal DM Constraints\n- This thread is an internal DM between agents.\n- Internal DMs are for discussion and coordination only.\n- Do not continue visible-thread work here.\n- Do not call tools in this thread.\n- If a visible thread continuation was explicitly requested, reply briefly here and stop. The daemon will continue the visible thread separately.\n",
            );
        }
        if let Some(recall) = onecontext_bootstrap.as_deref() {
            system_prompt.push_str("\n\n## OneContext Recall\n");
            system_prompt
                .push_str("Use this as historical context from prior sessions when relevant:\n");
            system_prompt.push_str(recall);
        }
        if let Some(skill_preflight) = skill_preflight.as_ref() {
            system_prompt.push_str("\n\n## Preloaded Skills\n");
            system_prompt.push_str(&skill_preflight.prompt_context);
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
            let skill_preflight = skill_preflight.as_ref().expect("checked Some");
            engine.emit_workflow_notice(
                &tid,
                "skill-preflight",
                skill_preflight.workflow_message.clone(),
                skill_preflight.workflow_details.clone(),
            );
            spawn_background_community_scout(engine, &tid, stored_user_content, &config);
        }
        let has_workspace_topology = engine.session_manager.read_workspace_topology().is_some();
        let mut tools = get_available_tools(&config, &engine.data_dir, has_workspace_topology);
        if let Some(filter) = &task_tool_filter {
            tools = filter.filtered_tools(tools);
        }
        if let Some(filter) = direct_thread_responder
            .as_ref()
            .and_then(|responder| responder.tool_filter.as_ref())
        {
            tools = filter.filtered_tools(tools);
        }
        let participant_managed_thread = visible_thread_has_participants(engine, &tid).await;
        if participant_managed_thread {
            tools.retain(|tool| tool.function.name != "list_agents");
        } else {
            tools.retain(|tool| tool.function.name != "list_participants");
        }
        if current_visible_thread_responder_is_active_participant(engine, &tid).await {
            tools.retain(|tool| {
                !PARTICIPANT_AGENT_FANOUT_TOOLS.contains(&tool.function.name.as_str())
            });
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
            reuse_existing_user_message,
            config,
            provider_config,
            preferred_session_id,
            onecontext_bootstrap,
            skill_preflight,
            agent_scope_id,
            runtime_agent_name,
            active_provider_id,
            memory_paths,
            base_prompt,
            operator_model_summary,
            operational_context,
            learned_patterns,
            continuity_summary: runtime_continuity.continuity_summary,
            negative_constraints_context: runtime_continuity.negative_constraints_context,
            system_prompt,
            current_task_snapshot,
            is_durable_goal_task,
            task_tool_filter,
            task_context_budget,
            task_termination_eval,
            task_type_for_trace: task_type_for_trace.clone(),
            initial_copilot_initiator,
            tools,
            retry_strategy,
            max_loops,
            stream_generation,
            stream_cancel_token,
            stream_retry_now,
            loop_count: 0,
            was_cancelled: false,
            interrupted_for_approval: false,
            terminated_for_budget: false,
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
            retry_status_visible: initial_scheduled_retry_cycles > 0,
            scheduled_retry_cycles: initial_scheduled_retry_cycles,
            assistant_output_visible: false,
            tool_side_effect_committed: false,
            attempted_recovery_signatures: std::collections::HashSet::new(),
            recent_policy_tool_outcomes: VecDeque::new(),
            provider_final_result: None,
            fresh_runner_retry: None,
            handoff_restart: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::agent_identity::{
        MAIN_AGENT_ID, MAIN_AGENT_NAME, WELES_AGENT_ID, WELES_AGENT_NAME,
    };
    use crate::session_manager::SessionManager;
    use tempfile::tempdir;

    #[tokio::test]
    async fn active_participant_responder_cannot_use_agent_fanout_tools() {
        let root = tempdir().expect("tempdir");
        let manager = SessionManager::new_test(root.path()).await;
        let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
        let thread_id = "thread_active_participant_tool_blacklist";

        engine.threads.write().await.insert(
            thread_id.to_string(),
            AgentThread {
                id: thread_id.to_string(),
                agent_name: Some(WELES_AGENT_NAME.to_string()),
                title: "Participant tool blacklist".to_string(),
                messages: vec![AgentMessage::user("check this thread", 1)],
                pinned: false,
                upstream_thread_id: None,
                upstream_transport: None,
                upstream_provider: None,
                upstream_model: None,
                upstream_assistant_id: None,
                total_input_tokens: 0,
                total_output_tokens: 0,
                created_at: 1,
                updated_at: 1,
            },
        );
        engine
            .set_thread_handoff_state(
                thread_id,
                ThreadHandoffState {
                    origin_agent_id: MAIN_AGENT_ID.to_string(),
                    active_agent_id: WELES_AGENT_ID.to_string(),
                    responder_stack: vec![
                        ThreadResponderFrame {
                            agent_id: MAIN_AGENT_ID.to_string(),
                            agent_name: MAIN_AGENT_NAME.to_string(),
                            entered_at: 1,
                            entered_via_handoff_event_id: None,
                            linked_thread_id: None,
                        },
                        ThreadResponderFrame {
                            agent_id: WELES_AGENT_ID.to_string(),
                            agent_name: WELES_AGENT_NAME.to_string(),
                            entered_at: 2,
                            entered_via_handoff_event_id: Some("handoff-1".to_string()),
                            linked_thread_id: Some("dm:svarog:weles".to_string()),
                        },
                    ],
                    events: Vec::new(),
                    pending_approval_id: None,
                },
            )
            .await;
        engine
            .upsert_thread_participant(thread_id, "weles", "verify claims")
            .await
            .expect("participant should register");

        let runner = SendMessageRunner::initialize(
            &engine,
            Some(thread_id),
            "check this thread",
            "check this thread",
            None,
            None,
            None,
            None,
            true,
            true,
            0,
        )
        .await
        .expect("runner should initialize");

        let tool_names = runner
            .tools
            .iter()
            .map(|tool| tool.function.name.as_str())
            .collect::<Vec<_>>();

        for forbidden_tool in PARTICIPANT_AGENT_FANOUT_TOOLS {
            assert!(
                !tool_names.contains(forbidden_tool),
                "active participant responder should not see {forbidden_tool}"
            );
        }
        assert!(
            tool_names.contains(&"handoff_thread_agent"),
            "active participant responder should still see handoff_thread_agent"
        );
    }

    #[tokio::test]
    async fn participant_managed_thread_replaces_list_agents_with_list_participants() {
        let root = tempdir().expect("tempdir");
        let manager = SessionManager::new_test(root.path()).await;
        let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
        let thread_id = "thread_participant_managed_tool_substitution";

        engine.threads.write().await.insert(
            thread_id.to_string(),
            AgentThread {
                id: thread_id.to_string(),
                agent_name: Some(MAIN_AGENT_NAME.to_string()),
                title: "Participant-managed thread".to_string(),
                messages: vec![AgentMessage::user("check this thread", 1)],
                pinned: false,
                upstream_thread_id: None,
                upstream_transport: None,
                upstream_provider: None,
                upstream_model: None,
                upstream_assistant_id: None,
                total_input_tokens: 0,
                total_output_tokens: 0,
                created_at: 1,
                updated_at: 1,
            },
        );
        engine
            .upsert_thread_participant(thread_id, "weles", "watch this thread")
            .await
            .expect("participant should register");

        let runner = SendMessageRunner::initialize(
            &engine,
            Some(thread_id),
            "check this thread",
            "check this thread",
            None,
            None,
            None,
            None,
            true,
            true,
            0,
        )
        .await
        .expect("runner should initialize");

        let tool_names = runner
            .tools
            .iter()
            .map(|tool| tool.function.name.as_str())
            .collect::<Vec<_>>();

        assert!(
            !tool_names.contains(&"list_agents"),
            "participant-managed thread should hide list_agents"
        );
        assert!(
            tool_names.contains(&"list_participants"),
            "participant-managed thread should expose list_participants"
        );
    }

    #[tokio::test]
    async fn non_participant_responder_keeps_agent_fanout_tools() {
        let root = tempdir().expect("tempdir");
        let manager = SessionManager::new_test(root.path()).await;
        let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
        let thread_id = "thread_non_participant_tool_access";

        engine.threads.write().await.insert(
            thread_id.to_string(),
            AgentThread {
                id: thread_id.to_string(),
                agent_name: Some(WELES_AGENT_NAME.to_string()),
                title: "Non-participant tool access".to_string(),
                messages: vec![AgentMessage::user("check this thread", 1)],
                pinned: false,
                upstream_thread_id: None,
                upstream_transport: None,
                upstream_provider: None,
                upstream_model: None,
                upstream_assistant_id: None,
                total_input_tokens: 0,
                total_output_tokens: 0,
                created_at: 1,
                updated_at: 1,
            },
        );
        engine
            .set_thread_handoff_state(
                thread_id,
                ThreadHandoffState {
                    origin_agent_id: MAIN_AGENT_ID.to_string(),
                    active_agent_id: WELES_AGENT_ID.to_string(),
                    responder_stack: vec![
                        ThreadResponderFrame {
                            agent_id: MAIN_AGENT_ID.to_string(),
                            agent_name: MAIN_AGENT_NAME.to_string(),
                            entered_at: 1,
                            entered_via_handoff_event_id: None,
                            linked_thread_id: None,
                        },
                        ThreadResponderFrame {
                            agent_id: WELES_AGENT_ID.to_string(),
                            agent_name: WELES_AGENT_NAME.to_string(),
                            entered_at: 2,
                            entered_via_handoff_event_id: Some("handoff-1".to_string()),
                            linked_thread_id: Some("dm:svarog:weles".to_string()),
                        },
                    ],
                    events: Vec::new(),
                    pending_approval_id: None,
                },
            )
            .await;

        let runner = SendMessageRunner::initialize(
            &engine,
            Some(thread_id),
            "check this thread",
            "check this thread",
            None,
            None,
            None,
            None,
            true,
            true,
            0,
        )
        .await
        .expect("runner should initialize");

        let tool_names = runner
            .tools
            .iter()
            .map(|tool| tool.function.name.as_str())
            .collect::<Vec<_>>();

        assert!(
            tool_names.contains(&"spawn_subagent"),
            "non-participant responder should still see spawn_subagent"
        );
        assert!(
            tool_names.contains(&"message_agent"),
            "non-participant responder should still see message_agent"
        );
    }
}
