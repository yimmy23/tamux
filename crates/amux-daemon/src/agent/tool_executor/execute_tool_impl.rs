async fn maybe_bootstrap_todo_plan_for_background_tool(
    agent: &AgentEngine,
    thread_id: &str,
    task_id: Option<&str>,
    tool_name: &str,
    args: &serde_json::Value,
) -> bool {
    let (content, status, notice_message) = match tool_name {
        "spawn_subagent" => {
            let title = args
                .get("title")
                .and_then(|value| value.as_str())
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .unwrap_or("Track delegated child work");
            (
                title.to_string(),
                TodoStatus::InProgress,
                format!("Bootstrapped plan tracking for delegated work: {title}"),
            )
        }
        "enqueue_task" => {
            let summary = args
                .get("title")
                .and_then(|value| value.as_str())
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .or_else(|| {
                    args.get("description")
                        .and_then(|value| value.as_str())
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                })
                .unwrap_or("Track queued background work");
            (
                summary.to_string(),
                TodoStatus::Pending,
                format!("Bootstrapped plan tracking for queued work: {summary}"),
            )
        }
        _ => return false,
    };

    let now = super::now_millis();
    agent
        .replace_thread_todos(
            thread_id,
            vec![TodoItem {
                id: format!("todo_{}", uuid::Uuid::new_v4()),
                content,
                status,
                position: 0,
                step_index: None,
                created_at: now,
                updated_at: now,
            }],
            task_id,
        )
        .await;
    agent.emit_workflow_notice(thread_id, "plan_bootstrap", notice_message, None);
    true
}

fn should_scrub_successful_tool_result(tool_name: &str) -> bool {
    !matches!(tool_name, "read_offloaded_payload")
}

struct PreparedToolExecution {
    tool_name: String,
    args: serde_json::Value,
    dispatch_tool_name: String,
    dispatch_args: serde_json::Value,
    governance_decision: crate::agent::weles_governance::WelesExecutionDecision,
    critique_session_id: Option<String>,
}

async fn prepare_tool_execution(
    tool_call: &ToolCall,
    agent: &AgentEngine,
    thread_id: &str,
    task_id: Option<&str>,
) -> Result<PreparedToolExecution, ToolResult> {
    let args = match parse_tool_args(
        tool_call.function.name.as_str(),
        &tool_call.function.arguments,
    ) {
        Ok(args) => args,
        Err(error) => {
            tracing::warn!(
                tool = %tool_call.function.name,
                error = %error,
                "agent tool argument parse failed"
            );
            return Err(ToolResult {
                tool_call_id: tool_call.id.clone(),
                name: tool_call.function.name.clone(),
                content: error,
                is_error: true,
                weles_review: tool_call.weles_review.clone(),
                pending_approval: None,
            });
        }
    };
    let security_level = {
        let config = agent.config.read().await;
        crate::agent::weles_governance::security_level_for_tool_call(
            &config,
            tool_call.function.name.as_str(),
            &args,
        )
    };
    let active_scope_id = crate::agent::agent_identity::current_agent_scope_id();
    let current_task = if let Some(task_id) = task_id {
        agent.list_tasks().await.into_iter().find(|task| task.id == task_id)
    } else {
        None
    };
    let trusted_weles_internal_task = if let Some(task) = current_task.as_ref() {
        task.sub_agent_def_id.as_deref()
            == Some(crate::agent::agent_identity::WELES_BUILTIN_SUBAGENT_ID)
            && agent.trusted_weles_tasks.read().await.contains(&task.id)
    } else {
        false
    };
    let classification =
        crate::agent::weles_governance::classify_tool_call(tool_call.function.name.as_str(), &args);
    let critique_session_id = if agent
        .should_run_critique_preflight(tool_call.function.name.as_str(), &classification)
        .await
    {
        let action_summary = crate::agent::summarize_text(&tool_call.function.arguments, 240);
        match agent
            .run_critique_preflight(
                &tool_call.id,
                tool_call.function.name.as_str(),
                &action_summary,
                &classification.reasons,
                Some(thread_id),
                task_id,
            )
            .await
        {
            Ok(session) => {
                let risk_tolerance = agent
                    .operator_model
                    .read()
                    .await
                    .risk_fingerprint
                    .risk_tolerance;
                if let Some(resolution) = session.resolution.as_ref() {
                    if agent.critique_requires_blocking_review(resolution, risk_tolerance) {
                        let decision = resolution.decision.as_str();
                        return Err(ToolResult {
                            tool_call_id: tool_call.id.clone(),
                            name: tool_call.function.name.clone(),
                            content: format!(
                                "Blocked by critique preflight ({decision}). critique_session_id={} :: {}",
                                session.id, resolution.synthesis
                            ),
                            is_error: true,
                            weles_review: Some(crate::agent::types::WelesReviewMeta {
                                weles_reviewed: true,
                                verdict: crate::agent::types::WelesVerdict::Block,
                                reasons: vec![format!(
                                    "critique_preflight:{}:{}",
                                    session.id, decision
                                )],
                                audit_id: Some(session.id.clone()),
                                security_override_mode: None,
                            }),
                            pending_approval: None,
                        });
                    }
                }
                Some(session.id)
            }
            Err(error) => {
                tracing::warn!(tool = %tool_call.function.name, error = %error, "critique preflight failed; continuing without critique enforcement");
                None
            }
        }
    } else {
        None
    };
    let governance_decision = if !crate::agent::weles_governance::should_guard_classification(
        &classification,
    ) {
        crate::agent::weles_governance::direct_allow_decision(classification.class)
    } else if crate::agent::agent_identity::is_weles_agent_scope(&active_scope_id) {
        crate::agent::weles_governance::internal_runtime_decision(&classification, security_level)
    } else if trusted_weles_internal_task {
        crate::agent::weles_governance::internal_runtime_decision(&classification, security_level)
    } else {
        let config = agent.config.read().await;
        if !crate::agent::weles_governance::review_available(&config) {
            crate::agent::weles_governance::guarded_fallback_decision(
                &classification,
                security_level,
            )
        } else {
            drop(config);
            match spawn_weles_internal_subagent(
                agent,
                thread_id,
                task_id,
                crate::agent::agent_identity::WELES_GOVERNANCE_SCOPE,
                tool_call.function.name.as_str(),
                &args,
                security_level,
                &classification.reasons,
            )
            .await
            {
                Ok(weles_task) => match agent
                    .send_internal_task_message(
                        &active_scope_id,
                        crate::agent::agent_identity::WELES_AGENT_ID,
                        &weles_task.id,
                        None,
                        Some("daemon"),
                        &crate::agent::weles_governance::build_weles_runtime_review_message(
                            &classification,
                            security_level,
                        ),
                    )
                    .await
                {
                    Ok(outcome) => {
                        let response = agent
                            .latest_assistant_message_text(&outcome.thread_id)
                            .await
                            .unwrap_or_default();
                        if let Some(runtime_review) =
                            crate::agent::weles_governance::parse_weles_runtime_review_response(
                                &response,
                            )
                        {
                            let runtime_review = crate::agent::weles_governance::normalize_runtime_verdict_for_classification(
                                &classification,
                                security_level,
                                runtime_review,
                            );
                            crate::agent::weles_governance::reviewed_runtime_decision(
                                &classification,
                                security_level,
                                runtime_review,
                            )
                        } else {
                            crate::agent::weles_governance::guarded_fallback_decision(
                                &classification,
                                security_level,
                            )
                        }
                    }
                    Err(_) => crate::agent::weles_governance::guarded_fallback_decision(
                        &classification,
                        security_level,
                    ),
                },
                Err(_) => crate::agent::weles_governance::guarded_fallback_decision(
                    &classification,
                    security_level,
                ),
            }
        }
    };
    if !governance_decision.should_execute {
        return Err(ToolResult {
            tool_call_id: tool_call.id.clone(),
            name: tool_call.function.name.clone(),
            content: governance_decision.block_message.unwrap_or_else(|| {
                "Blocked by WELES governance before tool execution.".to_string()
            }),
            is_error: true,
            weles_review: Some(governance_decision.review),
            pending_approval: None,
        });
    }
    let mut runtime_args = args.clone();
    if matches!(
        governance_decision.class,
        crate::agent::weles_governance::WelesGovernanceClass::RejectBypass
    ) && matches!(security_level, SecurityLevel::Yolo)
    {
        if let serde_json::Value::Object(ref mut map) = runtime_args {
            map.insert(
                "security_level".to_string(),
                serde_json::Value::String("moderate".to_string()),
            );
            map.insert(
                "__weles_force_headless".to_string(),
                serde_json::Value::Bool(true),
            );
        }
    }
    let (dispatch_tool_name, dispatch_args) =
        normalize_tool_dispatch(tool_call.function.name.as_str(), &runtime_args);

    if !thread_id.trim().is_empty()
        && matches!(
            tool_call.function.name.as_str(),
            "bash_command" | "execute_managed_command" | "enqueue_task" | "spawn_subagent"
        )
        && !trusted_weles_internal_task
        && agent.get_todos(thread_id).await.is_empty()
        && task_id.is_some()
    {
        let bootstrapped = maybe_bootstrap_todo_plan_for_background_tool(
            agent,
            thread_id,
            task_id,
            tool_call.function.name.as_str(),
            &dispatch_args,
        )
        .await;
        if !bootstrapped {
            return Err(ToolResult {
                tool_call_id: tool_call.id.clone(),
                name: tool_call.function.name.clone(),
                content: "Plan required: call update_todo first so tamux can track the live execution plan before running commands or spawning tasks.".to_string(),
                is_error: true,
                weles_review: Some(governance_decision.review.clone()),
                pending_approval: None,
            });
        }
    }

    Ok(PreparedToolExecution {
        tool_name: tool_call.function.name.clone(),
        args,
        dispatch_tool_name,
        dispatch_args,
        governance_decision,
        critique_session_id,
    })
}

async fn dispatch_tool_execution(
    prepared: &PreparedToolExecution,
    agent: &AgentEngine,
    thread_id: &str,
    task_id: Option<&str>,
    session_manager: &Arc<SessionManager>,
    session_id: Option<SessionId>,
    event_tx: &broadcast::Sender<AgentEvent>,
    agent_data_dir: &std::path::Path,
    http_client: &reqwest::Client,
    cancel_token: Option<CancellationToken>,
) -> (Result<String>, Option<ToolPendingApproval>) {
    let args = &prepared.args;
    let dispatch_args = &prepared.dispatch_args;
    let mut pending_approval = None;

    let result = match prepared.dispatch_tool_name.as_str() {
        // Terminal/session tools (daemon owns sessions directly)
        "list_terminals" | "list_sessions" => execute_list_sessions(session_manager).await,
        "read_active_terminal_content" => execute_read_terminal(args, session_manager).await,
        "run_terminal_command" => {
            match execute_run_terminal_command(
                dispatch_args,
                agent,
                session_manager,
                session_id,
                event_tx,
                thread_id,
                cancel_token.clone(),
            )
            .await
            {
                Ok((content, approval)) => {
                    pending_approval = approval;
                    Ok(content)
                }
                Err(error) => Err(error),
            }
        }
        "execute_managed_command" => {
            match execute_managed_command(
                dispatch_args,
                agent,
                session_manager,
                session_id,
                event_tx,
                thread_id,
                cancel_token.clone(),
            )
            .await
            {
                Ok((content, approval)) => {
                    pending_approval = approval;
                    Ok(content)
                }
                Err(error) => Err(error),
            }
        }
        "get_operation_status" => execute_get_operation_status(args, session_manager).await,
        "get_background_task_status" => {
            execute_get_background_task_status(args, session_manager).await
        }
        "allocate_terminal" => execute_allocate_terminal(args, session_manager, session_id, event_tx).await,
        "fetch_authenticated_providers" => execute_fetch_authenticated_providers(agent).await,
        "list_providers" => execute_list_providers(agent).await,
        "fetch_provider_models" => execute_fetch_provider_models(args, agent).await,
        "list_models" => execute_list_models(args, agent).await,
        "list_agents" => execute_list_agents(agent).await,
        "list_participants" => execute_list_participants(agent, thread_id).await,
        "switch_model" => execute_switch_model(args, agent).await,
        "spawn_subagent" => {
            execute_spawn_subagent(
                args,
                agent,
                thread_id,
                task_id,
                session_manager,
                session_id,
                event_tx,
            )
            .await
        }
        "handoff_thread_agent" => {
            match execute_handoff_thread_agent(args, agent, thread_id).await {
                Ok((content, approval)) => {
                    pending_approval = approval;
                    Ok(content)
                }
                Err(error) => Err(error),
            }
        }
        "list_subagents" => execute_list_subagents(args, agent, thread_id, task_id).await,
        "message_agent" => {
            Box::pin(execute_message_agent(args, agent, thread_id, task_id, session_id)).await
        }
        "route_to_specialist" => {
            execute_route_to_specialist(args, agent, thread_id, task_id).await
        }
        "run_divergent" => execute_run_divergent(args, agent, thread_id, task_id).await,
        "get_divergent_session" => execute_get_divergent_session(args, agent).await,
        "run_debate" => execute_run_debate(args, agent, thread_id, task_id).await,
        "get_debate_session" => execute_get_debate_session(args, agent).await,
        "get_critique_session" => execute_get_critique_session(args, agent).await,
        "lookup_emergent_protocol" => execute_lookup_emergent_protocol(args, agent, thread_id).await,
        "reload_emergent_protocol_registry" => {
            execute_reload_emergent_protocol_registry(args, agent, thread_id).await
        }
        "get_emergent_protocol_usage_log" => {
            execute_get_emergent_protocol_usage_log(args, agent).await
        }
        "append_debate_argument" => execute_append_debate_argument(args, agent).await,
        "advance_debate_round" => execute_advance_debate_round(args, agent).await,
        "complete_debate_session" => execute_complete_debate_session(args, agent).await,
        "broadcast_contribution" => {
            execute_broadcast_contribution(args, agent, thread_id, task_id).await
        }
        "read_peer_memory" => execute_read_peer_memory(args, agent, task_id).await,
        "vote_on_disagreement" => {
            execute_vote_on_disagreement(args, agent, thread_id, task_id).await
        }
        "list_collaboration_sessions" => {
            execute_list_collaboration_sessions(args, agent, task_id).await
        }
        "list_threads" => execute_list_threads(args, agent).await,
        "get_thread" => execute_get_thread(args, agent).await,
        "read_offloaded_payload" => execute_read_offloaded_payload(args, agent, thread_id).await,
        "enqueue_task" => execute_enqueue_task(args, agent).await,
        "list_tasks" => execute_list_tasks(args, agent).await,
        "get_todos" => execute_get_todos(args, agent, task_id).await,
        "cancel_task" => execute_cancel_task(args, agent).await,
        "type_in_terminal" => execute_type_in_terminal(args, session_manager).await,
        "send_slack_message"
        | "send_discord_message"
        | "send_telegram_message"
        | "send_whatsapp_message" => {
            execute_gateway_message(prepared.tool_name.as_str(), args, agent, http_client).await
        }
        "list_workspaces"
        | "create_workspace"
        | "set_active_workspace"
        | "create_surface"
        | "set_active_surface"
        | "split_pane"
        | "rename_pane"
        | "set_layout_preset"
        | "equalize_layout"
        | "list_snippets"
        | "create_snippet"
        | "run_snippet" => execute_workspace_tool(prepared.tool_name.as_str(), args, event_tx).await,
        "bash_command" => {
            match execute_bash_command(
                dispatch_args,
                agent,
                session_manager,
                session_id,
                event_tx,
                thread_id,
                cancel_token.clone(),
            )
            .await
            {
                Ok((content, approval)) => {
                    pending_approval = approval;
                    Ok(content)
                }
                Err(error) => Err(error),
            }
        }
        "python_execute" => {
            execute_python_execute(dispatch_args, session_manager, session_id, cancel_token.clone())
                .await
        }
        "list_files" => execute_list_files(args, session_manager, session_id).await,
        "read_file" => execute_read_file(args).await,
        "get_git_line_statuses" => execute_get_git_line_statuses(args).await,
        "write_file" => execute_write_file(args, session_manager, session_id).await,
        "create_file" => execute_create_file(args).await,
        "append_to_file" => execute_append_to_file(args).await,
        "replace_in_file" => execute_replace_in_file(args).await,
        "apply_file_patch" => execute_apply_file_patch(args).await,
        "apply_patch" => execute_apply_patch(args).await,
        "search_files" => execute_search_files(args).await,
        "get_system_info" => execute_system_info().await,
        "get_current_datetime" => execute_current_datetime().await,
        "list_processes" => execute_list_processes(args).await,
        "search_history" => execute_search_history(args, session_manager).await,
        "fetch_gateway_history" => execute_fetch_gateway_history(args, agent, thread_id).await,
        "session_search" => execute_session_search(args, session_manager).await,
        "agent_query_memory" => execute_agent_query_memory(args, agent).await,
        "onecontext_search" => execute_onecontext_search(args).await,
        "notify_user" => execute_notify(args, agent).await,
        "update_todo" => execute_update_todo(args, agent, thread_id, task_id).await,
        "update_memory" => {
            execute_update_memory(args, agent, thread_id, task_id, agent_data_dir).await
        }
        "read_memory" => {
            execute_read_memory(args, agent, Some(thread_id), task_id, agent_data_dir).await
        }
        "read_user" => {
            execute_read_user(args, agent, Some(thread_id), task_id, agent_data_dir).await
        }
        "read_soul" => {
            execute_read_soul(args, agent, Some(thread_id), task_id, agent_data_dir).await
        }
        "search_memory" => {
            execute_search_memory(args, agent, Some(thread_id), task_id, agent_data_dir).await
        }
        "search_user" => {
            execute_search_user(args, agent, Some(thread_id), task_id, agent_data_dir).await
        }
        "search_soul" => {
            execute_search_soul(args, agent, Some(thread_id), task_id, agent_data_dir).await
        }
        "list_tools" => execute_list_tools(args, agent, session_manager, agent_data_dir).await,
        "tool_search" => {
            execute_tool_search(args, agent, session_manager, agent_data_dir).await
        }
        "list_skills" => execute_list_skills(args, agent_data_dir, &agent.history).await,
        "discover_skills" => execute_discover_skills(args, agent, session_id).await,
        "semantic_query" => {
            execute_semantic_query(
                dispatch_args,
                session_manager,
                session_id,
                &agent.history,
                agent_data_dir,
            )
            .await
        }
        "read_skill" => {
            execute_read_skill(
                args,
                agent,
                agent_data_dir,
                &agent.history,
                session_manager,
                session_id,
                thread_id,
                task_id,
            )
            .await
        }
        "ask_questions" => {
            let parsed = (|| -> Result<(
                String,
                Vec<String>,
                Option<String>,
                Option<String>,
            )> {
                let content = dispatch_args
                    .get("content")
                    .and_then(|value| value.as_str())
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .ok_or_else(|| anyhow::anyhow!("missing 'content' argument"))?
                    .to_string();
                let options = dispatch_args
                    .get("options")
                    .and_then(|value| value.as_array())
                    .ok_or_else(|| anyhow::anyhow!("missing 'options' argument"))?
                    .iter()
                    .map(|value| {
                        value
                            .as_str()
                            .map(str::trim)
                            .filter(|option| !option.is_empty())
                            .map(ToOwned::to_owned)
                            .ok_or_else(|| {
                                anyhow::anyhow!(
                                    "'options' must be an array of compact non-empty strings"
                                )
                            })
                    })
                    .collect::<Result<Vec<_>>>()?;
                let session = dispatch_args
                    .get("session")
                    .and_then(|value| value.as_str())
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(ToOwned::to_owned)
                    .or_else(|| session_id.map(|value| value.to_string()));
                let thread = (!thread_id.trim().is_empty()).then(|| thread_id.to_string());
                Ok((content, options, session, thread))
            })();

            match parsed {
                Ok((content, options, session, thread)) => agent
                    .ask_operator_question(&content, options, session, thread)
                    .await
                    .map(|(_, answer)| answer),
                Err(error) => Err(error),
            }
        }
        "justify_skill_skip" => execute_justify_skill_skip(args, agent, thread_id).await,
        "synthesize_tool" => synthesize_tool(args, agent, agent_data_dir, http_client).await,
        "list_generated_tools" => list_generated_tools(agent_data_dir),
        "promote_generated_tool" => {
            args.get("tool")
                .and_then(|value| value.as_str())
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| anyhow::anyhow!("missing 'tool' argument"))
                .and_then(|tool| promote_generated_tool(agent_data_dir, tool))
        }
        "activate_generated_tool" => {
            args.get("tool")
                .and_then(|value| value.as_str())
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| anyhow::anyhow!("missing 'tool' argument"))
                .and_then(|tool| activate_generated_tool(agent_data_dir, tool))
        }
        "web_search" => {
            let config = agent.config.read().await;
            let search_provider = config
                .extra
                .get("search_provider")
                .and_then(|v| v.as_str())
                .unwrap_or("none")
                .to_string();
            let exa_api_key = config
                .extra
                .get("exa_api_key")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let tavily_api_key = config
                .extra
                .get("tavily_api_key")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            drop(config);
            execute_web_search(
                args,
                http_client,
                &search_provider,
                &exa_api_key,
                &tavily_api_key,
            )
            .await
        }
        "fetch_url" => {
            let config = agent.config.read().await;
            let browse_provider = config
                .extra
                .get("browse_provider")
                .and_then(|v| v.as_str())
                .unwrap_or("auto")
                .to_string();
            drop(config);
            execute_fetch_url(args, http_client, &browse_provider).await
        }
        "setup_web_browsing" => execute_setup_web_browsing(args, agent).await,
        "plugin_api_call" => {
            let plugin_name = match get_string_arg(args, &["plugin_name"]) {
                Some(name) => name.to_string(),
                None => return (
                    Err(anyhow::anyhow!("Error: missing 'plugin_name' argument")),
                    pending_approval,
                ),
            };
            let endpoint_name = match get_string_arg(args, &["endpoint_name"]) {
                Some(name) => name.to_string(),
                None => return (
                    Err(anyhow::anyhow!("Error: missing 'endpoint_name' argument")),
                    pending_approval,
                ),
            };
            let params = args
                .get("params")
                .cloned()
                .unwrap_or(serde_json::Value::Object(Default::default()));

            match agent.plugin_manager.get() {
                Some(pm) => match pm.api_call(&plugin_name, &endpoint_name, params).await {
                    Ok(text) => Ok(text),
                    Err(e) => Err(anyhow::anyhow!("{}", e)),
                },
                None => Err(anyhow::anyhow!("Plugin system not available")),
            }
        }
        other => match execute_generated_tool(
            other,
            args,
            agent,
            agent_data_dir,
            http_client,
            Some(thread_id),
        )
        .await
        {
            Ok(Some(content)) => Ok(content),
            Ok(None) => Err(anyhow::anyhow!("Unknown tool: {other}")),
            Err(error) => Err(error),
        },
    };

    (result, pending_approval)
}

pub fn execute_tool<'a>(
    tool_call: &'a ToolCall,
    agent: &'a AgentEngine,
    thread_id: &'a str,
    task_id: Option<&'a str>,
    session_manager: &'a Arc<SessionManager>,
    session_id: Option<SessionId>,
    event_tx: &'a broadcast::Sender<AgentEvent>,
    agent_data_dir: &'a std::path::Path,
    http_client: &'a reqwest::Client,
    cancel_token: Option<CancellationToken>,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = ToolResult> + Send + 'a>> {
    Box::pin(async move {
        let redacted_arguments = scrub_sensitive(&tool_call.function.arguments);
        tracing::info!(
            tool = %tool_call.function.name,
            args = %redacted_arguments,
            "agent tool call"
        );

        let prepared = match Box::pin(prepare_tool_execution(tool_call, agent, thread_id, task_id)).await {
            Ok(prepared) => prepared,
            Err(result) => return result,
        };

        let tool_domain =
            crate::agent::uncertainty::domains::classify_domain(tool_call.function.name.as_str());
        if tool_domain == crate::agent::uncertainty::domains::DomainClassification::Safety {
            let evidence = format!(
                "Safety-domain tool '{}' with blast-radius uncertainty. Args: {}",
                tool_call.function.name,
                tool_call
                    .function
                    .arguments
                    .chars()
                    .take(200)
                    .collect::<String>()
            );
            let _ = event_tx.send(AgentEvent::ConfidenceWarning {
                thread_id: thread_id.to_string(),
                action_type: "tool_call".to_string(),
                band: "medium".to_string(),
                evidence,
                domain: "safety".to_string(),
                blocked: false,
            });
        }

        let (result, pending_approval) = Box::pin(dispatch_tool_execution(
            &prepared,
            agent,
            thread_id,
            task_id,
            session_manager,
            session_id,
            event_tx,
            agent_data_dir,
            http_client,
            cancel_token,
        ))
        .await;

        match result {
            Ok(content) => {
                let content = if should_scrub_successful_tool_result(prepared.dispatch_tool_name.as_str()) {
                    scrub_sensitive(&content)
                } else {
                    content
                };
                let mut review = prepared.governance_decision.review.clone();
                if let Some(session_id) = prepared.critique_session_id.as_ref() {
                    review.weles_reviewed = true;
                    if !review
                        .reasons
                        .iter()
                        .any(|reason| reason.contains("critique_preflight:"))
                    {
                        review
                            .reasons
                            .push(format!("critique_preflight:{}:proceed", session_id));
                    }
                    if review.audit_id.is_none() {
                        review.audit_id = Some(session_id.clone());
                    }
                }
                emit_workflow_notice_for_tool(
                    event_tx,
                    thread_id,
                    prepared.dispatch_tool_name.as_str(),
                    &prepared.dispatch_args,
                );
                tracing::info!(tool = %prepared.tool_name, result_len = content.len(), "agent tool result: ok");
                ToolResult {
                    tool_call_id: tool_call.id.clone(),
                    name: tool_call.function.name.clone(),
                    content,
                    is_error: false,
                    weles_review: Some(review),
                    pending_approval,
                }
            }
            Err(e) => {
                let content = scrub_sensitive(&format!("Error: {e}"));
                tracing::warn!(tool = %prepared.tool_name, error = %content, "agent tool result: error");
                ToolResult {
                    tool_call_id: tool_call.id.clone(),
                    name: tool_call.function.name.clone(),
                    content,
                    is_error: true,
                    weles_review: Some(prepared.governance_decision.review),
                    pending_approval: None,
                }
            }
        }
    })
}
