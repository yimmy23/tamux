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
            return ToolResult {
                tool_call_id: tool_call.id.clone(),
                name: tool_call.function.name.clone(),
                content: error,
                is_error: true,
                weles_review: tool_call.weles_review.clone(),
                pending_approval: None,
            };
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
    let planner_required_before_governance = if !thread_id.trim().is_empty() && task_id.is_none() {
        agent.planner_required_for_thread(thread_id).await
    } else {
        false
    };
    let classification =
        crate::agent::weles_governance::classify_tool_call(tool_call.function.name.as_str(), &args);
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
            crate::agent::weles_governance::guarded_fallback_decision(&classification, security_level)
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
                Ok(weles_task) => {
                    match agent
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
                }
                }
                Err(_) => crate::agent::weles_governance::guarded_fallback_decision(
                    &classification,
                    security_level,
                ),
            }
        }
    };
    if !governance_decision.should_execute {
        return ToolResult {
            tool_call_id: tool_call.id.clone(),
            name: tool_call.function.name.clone(),
            content: governance_decision.block_message.unwrap_or_else(|| {
                "Blocked by WELES governance before tool execution.".to_string()
            }),
            is_error: true,
            weles_review: Some(governance_decision.review),
            pending_approval: None,
        };
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
        && (task_id.is_some() || planner_required_before_governance)
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
            return ToolResult {
                tool_call_id: tool_call.id.clone(),
                name: tool_call.function.name.clone(),
                content: "Plan required: call update_todo first so tamux can track the live execution plan before running commands or spawning tasks.".to_string(),
                is_error: true,
                weles_review: Some(governance_decision.review.clone()),
                pending_approval: None,
            };
        }
    }

    // UNCR-02: Pre-execution confidence warning for Safety-domain tools.
    // Emits a ConfidenceWarning event so clients can display blast-radius
    // uncertainty before the tool runs. Does NOT block -- existing policy.rs
    // approval flow handles blocking for dangerous commands.
    {
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
    }

    let mut pending_approval = None;

    let result = match dispatch_tool_name.as_str() {
        // Terminal/session tools (daemon owns sessions directly)
        "list_terminals" | "list_sessions" => execute_list_sessions(session_manager).await,
        "read_active_terminal_content" => execute_read_terminal(&args, session_manager).await,
        "run_terminal_command" => {
            match execute_run_terminal_command(
                &dispatch_args,
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
                &dispatch_args,
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
        "allocate_terminal" => {
            execute_allocate_terminal(&args, session_manager, session_id, event_tx).await
        }
        "spawn_subagent" => {
            execute_spawn_subagent(
                &args,
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
            match execute_handoff_thread_agent(&args, agent, thread_id).await {
                Ok((content, approval)) => {
                    pending_approval = approval;
                    Ok(content)
                }
                Err(error) => Err(error),
            }
        }
        "list_subagents" => execute_list_subagents(&args, agent, thread_id, task_id).await,
        "message_agent" => Box::pin(execute_message_agent(&args, agent, task_id, session_id)).await,
        "route_to_specialist" => {
            execute_route_to_specialist(&args, agent, thread_id, task_id).await
        }
        "run_divergent" => execute_run_divergent(&args, agent, thread_id, task_id).await,
        "get_divergent_session" => execute_get_divergent_session(&args, agent).await,
        "broadcast_contribution" => {
            execute_broadcast_contribution(&args, agent, thread_id, task_id).await
        }
        "read_peer_memory" => execute_read_peer_memory(&args, agent, task_id).await,
        "vote_on_disagreement" => {
            execute_vote_on_disagreement(&args, agent, thread_id, task_id).await
        }
        "list_collaboration_sessions" => {
            execute_list_collaboration_sessions(&args, agent, task_id).await
        }
        "enqueue_task" => execute_enqueue_task(&args, agent).await,
        "list_tasks" => execute_list_tasks(&args, agent).await,
        "cancel_task" => execute_cancel_task(&args, agent).await,
        "type_in_terminal" => execute_type_in_terminal(&args, session_manager).await,
        // Gateway messaging (execute via CLI)
        "send_slack_message"
        | "send_discord_message"
        | "send_telegram_message"
        | "send_whatsapp_message" => {
            execute_gateway_message(tool_call.function.name.as_str(), &args, agent, http_client)
                .await
        }
        // Workspace/snippet tools (read/write persistence files directly)
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
        | "run_snippet" => {
            execute_workspace_tool(tool_call.function.name.as_str(), &args, event_tx).await
        }
        // Daemon-native tools
        "bash_command" => {
            match execute_bash_command(
                &dispatch_args,
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
            execute_python_execute(&dispatch_args, session_manager, session_id, cancel_token.clone())
                .await
        }
        "list_files" => execute_list_files(&args, session_manager, session_id).await,
        "read_file" => execute_read_file(&args).await,
        "get_git_line_statuses" => execute_get_git_line_statuses(&args).await,
        "write_file" => execute_write_file(&args, session_manager, session_id).await,
        "create_file" => execute_create_file(&args).await,
        "append_to_file" => execute_append_to_file(&args).await,
        "replace_in_file" => execute_replace_in_file(&args).await,
        "apply_file_patch" => execute_apply_file_patch(&args).await,
        "apply_patch" => execute_apply_patch(&args).await,
        "search_files" => execute_search_files(&args).await,
        "get_system_info" => execute_system_info().await,
        "get_current_datetime" => execute_current_datetime().await,
        "list_processes" => execute_list_processes(&args).await,
        "search_history" => execute_search_history(&args, session_manager).await,
        "fetch_gateway_history" => execute_fetch_gateway_history(&args, agent, thread_id).await,
        "session_search" => execute_session_search(&args, session_manager).await,
        "agent_query_memory" => execute_agent_query_memory(&args, agent).await,
        "onecontext_search" => execute_onecontext_search(&args).await,
        "notify_user" => execute_notify(&args, agent).await,
        "update_todo" => execute_update_todo(&args, agent, thread_id, task_id).await,
        "update_memory" => {
            execute_update_memory(&args, agent, thread_id, task_id, agent_data_dir).await
        }
        "list_skills" => execute_list_skills(&args, agent_data_dir, &agent.history).await,
        "semantic_query" => {
            execute_semantic_query(
                &dispatch_args,
                session_manager,
                session_id,
                &agent.history,
                agent_data_dir,
            )
            .await
        }
        "read_skill" => {
            execute_read_skill(
                &args,
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
        "synthesize_tool" => synthesize_tool(&args, agent, agent_data_dir, http_client).await,
        "list_generated_tools" => list_generated_tools(agent_data_dir),
        "promote_generated_tool" => {
            let tool = args
                .get("tool")
                .and_then(|value| value.as_str())
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| anyhow::anyhow!("missing 'tool' argument"))
                .and_then(|tool| promote_generated_tool(agent_data_dir, tool));
            tool
        }
        "activate_generated_tool" => {
            let tool = args
                .get("tool")
                .and_then(|value| value.as_str())
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| anyhow::anyhow!("missing 'tool' argument"))
                .and_then(|tool| activate_generated_tool(agent_data_dir, tool));
            tool
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
                &args,
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
            execute_fetch_url(&args, http_client, &browse_provider).await
        }
        "setup_web_browsing" => execute_setup_web_browsing(&args, agent).await,
        "plugin_api_call" => {
            let plugin_name = match get_string_arg(&args, &["plugin_name"]) {
                Some(name) => name.to_string(),
                None => {
                    return ToolResult {
                        tool_call_id: tool_call.id.clone(),
                        name: tool_call.function.name.clone(),
                        content: "Error: missing 'plugin_name' argument".to_string(),
                        is_error: true,
                        weles_review: Some(governance_decision.review.clone()),
                        pending_approval: None,
                    }
                }
            };
            let endpoint_name = match get_string_arg(&args, &["endpoint_name"]) {
                Some(name) => name.to_string(),
                None => {
                    return ToolResult {
                        tool_call_id: tool_call.id.clone(),
                        name: tool_call.function.name.clone(),
                        content: "Error: missing 'endpoint_name' argument".to_string(),
                        is_error: true,
                        weles_review: Some(governance_decision.review.clone()),
                        pending_approval: None,
                    }
                }
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
            &args,
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

    match result {
        Ok(content) => {
            let content = scrub_sensitive(&content);
            emit_workflow_notice_for_tool(
                event_tx,
                thread_id,
                dispatch_tool_name.as_str(),
                &dispatch_args,
            );
            tracing::info!(tool = %tool_call.function.name, result_len = content.len(), "agent tool result: ok");
            ToolResult {
                tool_call_id: tool_call.id.clone(),
                name: tool_call.function.name.clone(),
                content,
                is_error: false,
                weles_review: Some(governance_decision.review.clone()),
                pending_approval,
            }
        }
        Err(e) => {
            let content = scrub_sensitive(&format!("Error: {e}"));
            tracing::warn!(tool = %tool_call.function.name, error = %content, "agent tool result: error");
            ToolResult {
                tool_call_id: tool_call.id.clone(),
                name: tool_call.function.name.clone(),
                content,
                is_error: true,
                weles_review: Some(governance_decision.review),
                pending_approval: None,
            }
        }
    }
    })
}
