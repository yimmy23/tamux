async fn execute_spawn_subagent(
    args: &serde_json::Value,
    agent: &AgentEngine,
    thread_id: &str,
    task_id: Option<&str>,
    session_manager: &Arc<SessionManager>,
    preferred_session_id: Option<SessionId>,
    event_tx: &broadcast::Sender<AgentEvent>,
) -> Result<String> {
    fn contains_hidden_weles_fields(args: &serde_json::Value) -> bool {
        [
            "weles_internal_scope",
            "weles_tool_name",
            "weles_tool_args",
            "weles_security_level",
            "weles_suspicion_reasons",
        ]
        .iter()
        .any(|key| args.get(key).is_some())
    }

    if contains_hidden_weles_fields(args) {
        anyhow::bail!(
            "daemon-owned WELES governance fields are unavailable to normal spawn_subagent callers"
        );
    }

    let title = args
        .get("title")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow::anyhow!("missing 'title' argument"))?
        .to_string();
    let description = args
        .get("description")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow::anyhow!("missing 'description' argument"))?
        .to_string();
    let runtime = normalize_task_runtime(args.get("runtime").and_then(|value| value.as_str()))?;
    if runtime != "daemon" {
        let status = agent
            .external_agent_status(&runtime)
            .await
            .ok_or_else(|| anyhow::anyhow!("runtime {runtime} is not configured"))?;
        if !status.available {
            anyhow::bail!("runtime {runtime} is not available on this machine");
        }
    }

    let priority = args
        .get("priority")
        .and_then(|value| value.as_str())
        .unwrap_or("normal");
    let command = args
        .get("command")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);
    let dependencies = args
        .get("dependencies")
        .and_then(|value| value.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str())
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let task_snapshot = if let Some(current_task_id) = task_id {
        agent
            .list_tasks()
            .await
            .into_iter()
            .find(|task| task.id == current_task_id)
    } else {
        None
    };

    let mut chosen_session = args
        .get("session")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);
    let mut allocated_lane_summary = None;
    if chosen_session.is_none() {
        let default_source_session = task_snapshot
            .as_ref()
            .and_then(|task| task.session_id.as_deref())
            .map(ToOwned::to_owned);
        let lane_request = serde_json::json!({
            "session": default_source_session,
            "cwd": args.get("cwd").and_then(|value| value.as_str()),
            "pane_name": format!("Subagent · {}", title.chars().take(24).collect::<String>()),
        });
        if let Ok(lane) = allocate_terminal_lane(
            &lane_request,
            session_manager,
            preferred_session_id,
            event_tx,
            "Subagent",
        )
        .await
        {
            chosen_session = Some(lane.session_id.to_string());
            allocated_lane_summary = Some(format!(
                "allocated terminal {} in workspace {} as \"{}\"",
                lane.session_id, lane.workspace_id, lane.pane_name
            ));
        }
    }

    let mut subagent = agent
        .enqueue_task(
            title.clone(),
            description,
            priority,
            command,
            chosen_session,
            dependencies,
            None,
            "subagent",
            task_snapshot
                .as_ref()
                .and_then(|task| task.goal_run_id.clone()),
            task_id.map(ToOwned::to_owned),
            Some(thread_id.to_string()),
            Some(runtime.clone()),
        )
        .await;

    // Look up a matching SubAgentDefinition by title/name and apply overrides.
    {
        let effective_sub_agents = agent.list_sub_agents().await;
        let title_lower = title.to_lowercase();
        let matched_def = effective_sub_agents.iter().find(|sa| {
            sa.enabled
                && sa.id != crate::agent::agent_identity::WELES_BUILTIN_SUBAGENT_ID
                && (sa.name.to_lowercase() == title_lower
                    || sa.role.as_deref().map(|r| r.to_lowercase()) == Some(title_lower.clone()))
        });
        if let Some(def) = matched_def {
            subagent.override_provider = Some(def.provider.clone());
            subagent.override_model = Some(def.model.clone());
            subagent.override_system_prompt = def.system_prompt.clone();
            subagent.sub_agent_def_id = Some(def.id.clone());

            if def.tool_whitelist.is_some() {
                subagent.tool_whitelist = def.tool_whitelist.clone();
            }
            if def.tool_blacklist.is_some() {
                subagent.tool_blacklist = def.tool_blacklist.clone();
            }
            if def.context_budget_tokens.is_some() {
                subagent.context_budget_tokens = def.context_budget_tokens;
            }
            if def.max_duration_secs.is_some() {
                subagent.max_duration_secs = def.max_duration_secs;
            }
            if def.supervisor_config.is_some() {
                subagent.supervisor_config = def.supervisor_config.clone();
            }
            // Persist the updated task fields.
            let mut tasks = agent.tasks.lock().await;
            if let Some(existing) = tasks.iter_mut().find(|t| t.id == subagent.id) {
                *existing = subagent.clone();
            }
            drop(tasks);
            agent.persist_tasks().await;
        }
    }
    if let Some(parent_task_id) = task_id {
        agent
            .register_subagent_collaboration(parent_task_id, &subagent)
            .await;
    }

    let persona_prompt = if subagent.sub_agent_def_id.as_deref()
        == Some(crate::agent::agent_identity::WELES_BUILTIN_SUBAGENT_ID)
    {
        let scope = subagent
            .override_system_prompt
            .as_deref()
            .and_then(crate::agent::weles_governance::parse_weles_internal_override_payload)
            .map(|(scope, _, _)| scope)
            .unwrap_or_else(|| crate::agent::agent_identity::WELES_GOVERNANCE_SCOPE.to_string());
        crate::agent::agent_identity::build_weles_persona_prompt(&scope)
    } else {
        build_spawned_persona_prompt(&subagent.id)
    };
    subagent.override_system_prompt = Some(match subagent.override_system_prompt.take() {
        Some(existing) if !existing.trim().is_empty() => {
            format!("{persona_prompt}\n\n{existing}")
        }
        _ => persona_prompt.clone(),
    });
    {
        let mut tasks = agent.tasks.lock().await;
        if let Some(existing) = tasks.iter_mut().find(|t| t.id == subagent.id) {
            *existing = subagent.clone();
        }
    }
    {
        let mut trusted = agent.trusted_weles_tasks.write().await;
        trusted.insert(subagent.id.clone());
    }
    agent.persist_tasks().await;

    let lane_suffix = allocated_lane_summary
        .map(|value| format!("\nDedicated lane: {value}"))
        .unwrap_or_default();
    let persona_suffix = extract_persona_name(subagent.override_system_prompt.as_deref())
        .map(|name| format!("\nAssigned persona: {name}"))
        .unwrap_or_default();
    let def_suffix = subagent
        .sub_agent_def_id
        .as_ref()
        .map(|id| format!("\nMatched sub-agent definition: {id}"))
        .unwrap_or_default();
    Ok(format!(
        "Spawned subagent {} with runtime {}.{}{}{def_suffix}",
        subagent.id, runtime, lane_suffix, persona_suffix
    ))
}

pub(in crate::agent) async fn spawn_weles_internal_subagent(
    agent: &AgentEngine,
    thread_id: &str,
    parent_task_id: Option<&str>,
    scope: &str,
    tool_name: &str,
    tool_args: &serde_json::Value,
    security_level: SecurityLevel,
    suspicion_reasons: &[String],
) -> Result<crate::agent::types::AgentTask> {
    if !crate::agent::agent_identity::is_weles_internal_scope(scope) {
        anyhow::bail!("invalid WELES internal scope: {scope}");
    }

    let title = "WELES".to_string();
    let description = match scope {
        crate::agent::agent_identity::WELES_VITALITY_SCOPE => {
            "Internal vitality/self-health review".to_string()
        }
        _ => format!("Internal governance review for {tool_name}"),
    };
    let task_snapshot = if let Some(current_task_id) = parent_task_id {
        let tasks = agent.tasks.lock().await;
        tasks.iter().find(|task| task.id == current_task_id).cloned()
    } else {
        None
    };
    let effective_sub_agents = agent.list_sub_agents().await;
    let def = effective_sub_agents
        .iter()
        .find(|sa| sa.id == crate::agent::agent_identity::WELES_BUILTIN_SUBAGENT_ID)
        .cloned()
            .ok_or_else(|| anyhow::anyhow!("missing daemon-owned WELES definition"))?;
    let task_health_signals = crate::agent::weles_governance::build_task_health_signals(
        task_snapshot.as_ref(),
    );

    let inspection_context = serde_json::json!({
        "tool_name": tool_name,
        "tool_args": tool_args,
        "security_level": match security_level {
            SecurityLevel::Highest => "highest",
            SecurityLevel::Moderate => "moderate",
            SecurityLevel::Lowest => "lowest",
            SecurityLevel::Yolo => "yolo",
        },
        "suspicion_reasons": suspicion_reasons,
        "task_health_signals": task_health_signals,
    });
    let internal_payload = crate::agent::weles_governance::build_weles_internal_override_payload(
        scope,
        &inspection_context,
    )
    .ok_or_else(|| anyhow::anyhow!("failed to build WELES internal payload"))?;
    let mut subagent = agent
        .enqueue_task(
            title,
            description,
            "high",
            None,
            task_snapshot.as_ref().and_then(|task| task.session_id.clone()),
            Vec::new(),
            None,
            "subagent",
            task_snapshot
                .as_ref()
                .and_then(|task| task.goal_run_id.clone()),
            parent_task_id.map(ToOwned::to_owned),
            Some(thread_id.to_string()),
            Some("daemon".to_string()),
        )
        .await;
    subagent.override_provider = Some(def.provider.clone());
    subagent.override_model = Some(def.model.clone());
    subagent.sub_agent_def_id = Some(def.id.clone());
    subagent.override_system_prompt = Some(format!(
        "{}\n\n{}",
        crate::agent::agent_identity::build_weles_persona_prompt(scope),
        internal_payload
    ));
    if def.tool_whitelist.is_some() {
        subagent.tool_whitelist = def.tool_whitelist.clone();
    }
    if def.tool_blacklist.is_some() {
        subagent.tool_blacklist = def.tool_blacklist.clone();
    }
    if def.context_budget_tokens.is_some() {
        subagent.context_budget_tokens = def.context_budget_tokens;
    }
    if def.max_duration_secs.is_some() {
        subagent.max_duration_secs = def.max_duration_secs;
    }
    if def.supervisor_config.is_some() {
        subagent.supervisor_config = def.supervisor_config.clone();
    }

    {
        let mut tasks = agent.tasks.lock().await;
        if let Some(existing) = tasks.iter_mut().find(|t| t.id == subagent.id) {
            *existing = subagent.clone();
        }
    }
    {
        let mut trusted = agent.trusted_weles_tasks.write().await;
        trusted.insert(subagent.id.clone());
    }
    agent.persist_tasks().await;

    if let Some(parent_task_id) = parent_task_id {
        agent
            .register_subagent_collaboration(parent_task_id, &subagent)
            .await;
    }

    Ok(subagent)
}

async fn execute_route_to_specialist(
    args: &serde_json::Value,
    agent: &AgentEngine,
    thread_id: &str,
    task_id: Option<&str>,
) -> Result<String> {
    let task_description = args
        .get("task_description")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .ok_or_else(|| anyhow::anyhow!("missing 'task_description' argument"))?
        .to_string();
    let capability_tags: Vec<String> = args
        .get("capability_tags")
        .and_then(|v| v.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str())
                .map(str::trim)
                .filter(|v| !v.is_empty())
                .map(ToOwned::to_owned)
                .collect()
        })
        .unwrap_or_default();
    if capability_tags.is_empty() {
        anyhow::bail!("'capability_tags' must be a non-empty array of strings");
    }
    let acceptance_criteria = args
        .get("acceptance_criteria")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .unwrap_or("non_empty")
        .to_string();
    let current_depth: u8 = args
        .get("current_depth")
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as u8;

    match agent
        .route_handoff(
            &task_description,
            &capability_tags,
            task_id,
            None, // goal_run_id
            thread_id,
            &acceptance_criteria,
            current_depth,
        )
        .await
    {
        Ok(result) => {
            let response = serde_json::json!({
                "status": "dispatched",
                "task_id": result.task_id,
                "specialist_name": result.specialist_name,
                "specialist_profile_id": result.specialist_profile_id,
                "handoff_log_id": result.handoff_log_id,
                "context_bundle_tokens": result.context_bundle_tokens,
            });
            Ok(serde_json::to_string_pretty(&response).unwrap_or_else(|_| "{}".to_string()))
        }
        Err(e) => Ok(format!("Handoff failed: {e}")),
    }
}

async fn execute_run_divergent(
    args: &serde_json::Value,
    agent: &AgentEngine,
    thread_id: &str,
    task_id: Option<&str>,
) -> Result<String> {
    let problem_statement = args
        .get("problem_statement")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .ok_or_else(|| anyhow::anyhow!("missing 'problem_statement' argument"))?
        .to_string();

    // Parse optional custom framings
    let custom_framings = args
        .get("custom_framings")
        .and_then(|v| v.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    let label = item.get("label")?.as_str()?.trim().to_string();
                    let prompt = item
                        .get("system_prompt_override")?
                        .as_str()?
                        .trim()
                        .to_string();
                    if label.is_empty() || prompt.is_empty() {
                        return None;
                    }
                    Some(super::handoff::divergent::Framing {
                        label,
                        system_prompt_override: prompt,
                        task_id: None,
                        contribution_id: None,
                    })
                })
                .collect::<Vec<_>>()
        })
        .filter(|v| v.len() >= 2);

    // Derive goal_run_id from context if available
    let goal_run_id = task_id.and_then(|_tid| {
        // Convention: goal-sourced tasks have source "goal_run"
        // but we don't have direct access here; pass None
        // and let start_divergent_session work without it
        None::<&str>
    });

    match agent
        .start_divergent_session(&problem_statement, custom_framings, thread_id, goal_run_id)
        .await
    {
        Ok(session_id) => {
            let response = serde_json::json!({
                "status": "started",
                "session_id": session_id,
                "problem_statement": problem_statement,
                "message": "Divergent session started. Parallel framings are being processed. Use get_divergent_session with this session_id to retrieve progress, tensions, and mediator output."
            });
            Ok(serde_json::to_string_pretty(&response).unwrap_or_else(|_| "{}".to_string()))
        }
        Err(e) => Ok(format!("Divergent session failed: {e}")),
    }
}

async fn execute_get_divergent_session(
    args: &serde_json::Value,
    agent: &AgentEngine,
) -> Result<String> {
    let session_id = args
        .get("session_id")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .ok_or_else(|| anyhow::anyhow!("missing 'session_id' argument"))?;

    match agent.get_divergent_session(session_id).await {
        Ok(payload) => {
            Ok(serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "{}".to_string()))
        }
        Err(error) => Ok(format!("Failed to fetch divergent session: {error}")),
    }
}

async fn execute_handoff_thread_agent(
    args: &serde_json::Value,
    agent: &AgentEngine,
    thread_id: &str,
) -> Result<(String, Option<ToolPendingApproval>)> {
    if thread_id.trim().is_empty() {
        anyhow::bail!("handoff_thread_agent requires a thread context");
    }

    let action = args
        .get("action")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow::anyhow!("missing 'action' argument"))?;
    let reason = args
        .get("reason")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow::anyhow!("missing 'reason' argument"))?;
    let summary = args
        .get("summary")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow::anyhow!("missing 'summary' argument"))?;
    let requested_by = match args
        .get("requested_by")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .unwrap_or("agent")
    {
        "user" => crate::agent::ThreadHandoffRequestedBy::User,
        "agent" => crate::agent::ThreadHandoffRequestedBy::Agent,
        other => anyhow::bail!("invalid 'requested_by' argument: {other}"),
    };

    let request = match action {
        "push_handoff" => crate::agent::PendingThreadHandoffActivation {
            thread_id: thread_id.to_string(),
            kind: crate::agent::ThreadHandoffKind::Push,
            target_agent_id: Some(
                args.get("target_agent_id")
                    .and_then(|value| value.as_str())
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .ok_or_else(|| anyhow::anyhow!("push_handoff requires 'target_agent_id'"))?
                    .to_string(),
            ),
            requested_by,
            reason: reason.to_string(),
            summary: summary.to_string(),
        },
        "return_handoff" => crate::agent::PendingThreadHandoffActivation {
            thread_id: thread_id.to_string(),
            kind: crate::agent::ThreadHandoffKind::Return,
            target_agent_id: None,
            requested_by,
            reason: reason.to_string(),
            summary: summary.to_string(),
        },
        other => anyhow::bail!("unsupported handoff action: {other}"),
    };

    let requires_approval = matches!(
        (request.kind, request.requested_by),
        (
            crate::agent::ThreadHandoffKind::Push,
            crate::agent::ThreadHandoffRequestedBy::Agent
        )
    ) && !matches!(
        agent.config.read().await.managed_execution.security_level,
        SecurityLevel::Yolo
    );

    if requires_approval {
        let pending_approval = agent.thread_handoff_pending_approval(&request, "medium")?;
        agent
            .queue_thread_handoff_approval(&request, &pending_approval)
            .await?;
        let target_name = request
            .target_agent_id
            .as_deref()
            .map(canonical_agent_name)
            .unwrap_or(MAIN_AGENT_NAME);
        return Ok((
            format!("Queued operator approval to hand off this thread to {target_name}."),
            Some(pending_approval),
        ));
    }

    let event = agent.apply_thread_handoff_activation(&request, None).await?;
    Ok((
        format!(
            "Thread handoff complete: {} -> {}.",
            canonical_agent_name(&event.from_agent_id),
            canonical_agent_name(&event.to_agent_id)
        ),
        None,
    ))
}

async fn execute_message_agent(
    args: &serde_json::Value,
    agent: &AgentEngine,
    task_id: Option<&str>,
    preferred_session_id: Option<SessionId>,
) -> Result<String> {
    let target = args
        .get("target")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow::anyhow!("missing 'target' argument"))?;
    let message = args
        .get("message")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow::anyhow!("missing 'message' argument"))?;

    let sender = if let Some(current_task_id) = task_id {
        let tasks = agent.tasks.lock().await;
        sender_name_for_task(tasks.iter().find(|task| task.id == current_task_id))
    } else {
        MAIN_AGENT_NAME.to_string()
    };

    let preferred_session_hint = preferred_session_id.as_ref().map(|value| value.to_string());
    let (thread_id, response) = agent
        .send_internal_agent_message(&sender, target, message, preferred_session_hint.as_deref())
        .await?;
    Ok(serde_json::to_string_pretty(&serde_json::json!({
        "target": canonical_agent_name(target),
        "thread_id": thread_id,
        "response": response,
    }))
    .unwrap_or_else(|_| "{}".to_string()))
}
