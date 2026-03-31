async fn execute_spawn_subagent(
    args: &serde_json::Value,
    agent: &AgentEngine,
    thread_id: &str,
    task_id: Option<&str>,
    session_manager: &Arc<SessionManager>,
    preferred_session_id: Option<SessionId>,
    event_tx: &broadcast::Sender<AgentEvent>,
) -> Result<String> {
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
        let config = agent.config.read().await;
        let title_lower = title.to_lowercase();
        let matched_def = config.sub_agents.iter().find(|sa| {
            sa.enabled
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

    let persona_prompt = build_spawned_persona_prompt(&subagent.id);
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

