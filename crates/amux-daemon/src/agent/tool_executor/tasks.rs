async fn execute_list_subagents(
    args: &serde_json::Value,
    agent: &AgentEngine,
    thread_id: &str,
    task_id: Option<&str>,
) -> Result<String> {
    fn is_descendant_of(task: &AgentTask, ancestor_task_id: &str, all_tasks: &[AgentTask]) -> bool {
        let mut current_parent_id = task.parent_task_id.as_deref();
        while let Some(parent_id) = current_parent_id {
            if parent_id == ancestor_task_id {
                return true;
            }
            current_parent_id = all_tasks
                .iter()
                .find(|candidate| candidate.id == parent_id)
                .and_then(|parent| parent.parent_task_id.as_deref());
        }
        false
    }

    let all_tasks = agent.list_tasks().await;
    let fallback_parent_task_id = if let Some(task_id) = task_id {
        all_tasks
            .iter()
            .find(|task| task.id == task_id)
            .and_then(|task| task.parent_task_id.clone().or_else(|| Some(task.id.clone())))
    } else {
        None
    };

    let status_filter = args
        .get("status")
        .and_then(|value| value.as_str())
        .map(|value| value.trim().to_ascii_lowercase());
    let parent_task_id = args
        .get("parent_task_id")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .or(fallback_parent_task_id);
    let parent_thread_id = args
        .get("parent_thread_id")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .or_else(|| Some(thread_id.to_string()));
    let limit = args
        .get("limit")
        .and_then(|value| value.as_u64())
        .map(|value| value as usize)
        .unwrap_or(20);

    let mut subagents = all_tasks
        .clone()
        .into_iter()
        .filter(|task| {
            if task.source != "subagent" {
                return false;
            }
            if let Some(parent_task_id) = parent_task_id.as_deref() {
                return is_descendant_of(task, parent_task_id, &all_tasks);
            }

            parent_thread_id
                .as_deref()
                .map(|value| task.parent_thread_id.as_deref() == Some(value))
                .unwrap_or(false)
        })
        .collect::<Vec<_>>();

    if let Some(status_filter) = status_filter {
        subagents.retain(|task| {
            serde_json::to_value(task.status)
                .ok()
                .and_then(|value| value.as_str().map(ToOwned::to_owned))
                .map(|value| value == status_filter)
                .unwrap_or(false)
        });
    }

    subagents.truncate(limit);
    let mut payload = Vec::with_capacity(subagents.len());
    for task in subagents {
        let depth = compute_task_delegation_depth(&task, &all_tasks);
        let max_depth = parse_subagent_containment_scope(task.containment_scope.as_deref())
            .map(|(_, max_depth)| max_depth)
            .unwrap_or_else(|| effective_subagent_max_depth(&task, &all_tasks));
        let metrics = agent
            .history
            .get_subagent_metrics(&task.id)
            .await
            .ok()
            .flatten();
        let tool_call_limit = extract_tool_call_limit(task.termination_conditions.as_deref());

        let tokens_remaining_fraction = match (task.context_budget_tokens, metrics.as_ref()) {
            (Some(max_tokens), Some(metrics)) if max_tokens > 0 => {
                let consumed = metrics.tokens_consumed.max(0) as u64;
                let remaining = max_tokens as u64 - consumed.min(max_tokens as u64);
                Some(remaining as f64 / max_tokens as f64)
            }
            (Some(_), None) => Some(1.0),
            _ => None,
        };
        let time_remaining_fraction = match task.max_duration_secs {
            Some(max_duration_secs) if max_duration_secs > 0 => {
                let started_at = task.started_at.unwrap_or(task.created_at);
                let elapsed_secs = crate::agent::now_millis().saturating_sub(started_at) / 1000;
                let remaining = max_duration_secs.saturating_sub(elapsed_secs);
                Some(remaining as f64 / max_duration_secs as f64)
            }
            _ => None,
        };
        let tool_calls_remaining = match (tool_call_limit, metrics.as_ref()) {
            (Some(limit), Some(metrics)) => {
                let limit: u32 = limit;
                let used = (metrics.tool_calls_total.max(0) as i64).min(u32::MAX as i64) as u32;
                Some::<u32>(limit.saturating_sub(used))
            }
            (Some(limit), None) => Some::<u32>(limit),
            _ => None,
        };

        let mut exhausted_limits = Vec::new();
        if tokens_remaining_fraction.is_some_and(|value| value <= 0.0) {
            exhausted_limits.push("tokens");
        }
        if time_remaining_fraction.is_some_and(|value| value <= 0.0) {
            exhausted_limits.push("time");
        }
        if tool_calls_remaining == Some(0) {
            exhausted_limits.push("tool_calls");
        }
        let budget_exhausted = !exhausted_limits.is_empty();

        let effective_status = if budget_exhausted {
            "budget_exhausted".to_string()
        } else {
            serde_json::to_value(task.status)
                .ok()
                .and_then(|value| value.as_str().map(ToOwned::to_owned))
                .unwrap_or_else(|| "unknown".to_string())
        };

        let mut value = serde_json::to_value(&task).unwrap_or_else(|_| serde_json::json!({}));
        if let Some(obj) = value.as_object_mut() {
            obj.insert("depth".to_string(), serde_json::json!(depth));
            obj.insert("max_depth".to_string(), serde_json::json!(max_depth));
            obj.insert(
                "effective_status".to_string(),
                serde_json::json!(effective_status),
            );
            obj.insert(
                "budget_remaining".to_string(),
                serde_json::json!({
                    "tokens_pct": tokens_remaining_fraction,
                    "time_pct": time_remaining_fraction,
                    "tool_calls_remaining": tool_calls_remaining,
                }),
            );
            obj.insert(
                "budget_exhausted".to_string(),
                serde_json::json!(budget_exhausted),
            );
            obj.insert(
                "exhausted_limits".to_string(),
                serde_json::json!(exhausted_limits),
            );
        }
        payload.push(value);
    }
    Ok(serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "[]".to_string()))
}

async fn execute_broadcast_contribution(
    args: &serde_json::Value,
    agent: &AgentEngine,
    thread_id: &str,
    task_id: Option<&str>,
) -> Result<String> {
    if !agent.config.read().await.collaboration.enabled {
        anyhow::bail!("collaboration capability is disabled in agent config");
    }
    let explicit_parent_task_id = args
        .get("parent_task_id")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);
    let task = if let Some(task_id) = task_id {
        Some(
            agent
                .list_tasks()
                .await
                .into_iter()
                .find(|task| task.id == task_id)
                .ok_or_else(|| anyhow::anyhow!("task {task_id} not found"))?,
        )
    } else {
        None
    };
    let parent_task_id = explicit_parent_task_id
        .or_else(|| task.as_ref().and_then(|task| task.parent_task_id.clone()))
        .ok_or_else(|| {
            anyhow::anyhow!(
                "broadcast_contribution requires a current task or explicit parent_task_id"
            )
        })?;
    let contributor_task_id = task
        .as_ref()
        .map(|task| task.id.clone())
        .unwrap_or_else(|| "operator".to_string());
    let topic = args
        .get("topic")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow::anyhow!("missing 'topic' argument"))?;
    let position = args
        .get("position")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow::anyhow!("missing 'position' argument"))?;
    let evidence = args
        .get("evidence")
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
    let confidence = args
        .get("confidence")
        .and_then(|value| value.as_f64())
        .unwrap_or(0.6);
    let report = agent
        .record_collaboration_contribution(
            &parent_task_id,
            &contributor_task_id,
            topic,
            position,
            evidence,
            confidence,
        )
        .await?;
    agent
        .record_provenance_event(
            "collaboration_contribution",
            "subagent broadcast a collaboration contribution",
            serde_json::json!({
                "parent_task_id": parent_task_id,
                "task_id": contributor_task_id,
                "topic": topic,
                "position": position,
                "thread_id": thread_id,
            }),
            task.as_ref().and_then(|task| task.goal_run_id.as_deref()),
            task.as_ref().map(|task| task.id.as_str()),
            Some(thread_id),
            None,
            None,
        )
        .await;
    Ok(serde_json::to_string_pretty(&report).unwrap_or_else(|_| "{}".to_string()))
}

async fn execute_read_peer_memory(
    args: &serde_json::Value,
    agent: &AgentEngine,
    task_id: Option<&str>,
) -> Result<String> {
    if !agent.config.read().await.collaboration.enabled {
        anyhow::bail!("collaboration capability is disabled in agent config");
    }
    let explicit_parent_task_id = args
        .get("parent_task_id")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);
    let task = if let Some(task_id) = task_id {
        Some(
            agent
                .list_tasks()
                .await
                .into_iter()
                .find(|task| task.id == task_id)
                .ok_or_else(|| anyhow::anyhow!("task {task_id} not found"))?,
        )
    } else {
        None
    };
    let parent_task_id = explicit_parent_task_id
        .or_else(|| task.as_ref().and_then(|task| task.parent_task_id.clone()))
        .ok_or_else(|| {
            anyhow::anyhow!("read_peer_memory requires a current task or explicit parent_task_id")
        })?;
    let requester_task_id = task
        .as_ref()
        .map(|task| task.id.as_str())
        .unwrap_or("operator");
    let report = agent
        .collaboration_peer_memory_json(&parent_task_id, requester_task_id)
        .await?;
    Ok(serde_json::to_string_pretty(&report).unwrap_or_else(|_| "{}".to_string()))
}

async fn execute_vote_on_disagreement(
    args: &serde_json::Value,
    agent: &AgentEngine,
    thread_id: &str,
    task_id: Option<&str>,
) -> Result<String> {
    if !agent.config.read().await.collaboration.enabled {
        anyhow::bail!("collaboration capability is disabled in agent config");
    }
    let task_id =
        task_id.ok_or_else(|| anyhow::anyhow!("vote_on_disagreement requires a current task"))?;
    let task = agent
        .list_tasks()
        .await
        .into_iter()
        .find(|task| task.id == task_id)
        .ok_or_else(|| anyhow::anyhow!("task {task_id} not found"))?;
    let parent_task_id = task.parent_task_id.clone().ok_or_else(|| {
        anyhow::anyhow!("vote_on_disagreement is only available inside subagents")
    })?;
    let disagreement_id = args
        .get("disagreement_id")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow::anyhow!("missing 'disagreement_id' argument"))?;
    let position = args
        .get("position")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow::anyhow!("missing 'position' argument"))?;
    let confidence = args.get("confidence").and_then(|value| value.as_f64());
    let report = agent
        .vote_on_collaboration_disagreement(
            &parent_task_id,
            disagreement_id,
            task_id,
            position,
            confidence,
        )
        .await?;
    agent
        .record_provenance_event(
            "collaboration_vote",
            "subagent voted on a disagreement",
            serde_json::json!({
                "parent_task_id": parent_task_id,
                "task_id": task_id,
                "disagreement_id": disagreement_id,
                "position": position,
                "thread_id": thread_id,
            }),
            task.goal_run_id.as_deref(),
            Some(task_id),
            Some(thread_id),
            None,
            None,
        )
        .await;
    Ok(serde_json::to_string_pretty(&report).unwrap_or_else(|_| "{}".to_string()))
}

async fn execute_dispatch_via_bid_protocol(
    args: &serde_json::Value,
    agent: &AgentEngine,
) -> Result<String> {
    if !agent.config.read().await.collaboration.enabled {
        anyhow::bail!("collaboration capability is disabled in agent config");
    }
    let parent_task_id = args
        .get("parent_task_id")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow::anyhow!("missing 'parent_task_id' argument"))?;
    let bids = args
        .get("bids")
        .and_then(|value| value.as_array())
        .ok_or_else(|| anyhow::anyhow!("missing 'bids' argument"))?
        .iter()
        .map(|bid| {
            let task_id = bid
                .get("task_id")
                .and_then(|value| value.as_str())
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| anyhow::anyhow!("each bid requires 'task_id'"))?
                .to_string();
            let confidence = bid
                .get("confidence")
                .and_then(|value| value.as_f64())
                .ok_or_else(|| anyhow::anyhow!("each bid requires numeric 'confidence'"))?;
            let availability = match bid
                .get("availability")
                .and_then(|value| value.as_str())
                .map(|value| value.trim().to_ascii_lowercase())
                .as_deref()
            {
                Some("available") => crate::agent::collaboration::BidAvailability::Available,
                Some("busy") => crate::agent::collaboration::BidAvailability::Busy,
                Some("unavailable") => crate::agent::collaboration::BidAvailability::Unavailable,
                _ => anyhow::bail!(
                    "each bid requires availability in [available, busy, unavailable]"
                ),
            };
            Ok(crate::agent::collaboration::DispatchBidRequest {
                task_id,
                confidence,
                availability,
            })
        })
        .collect::<Result<Vec<_>>>()?;

    let report = agent
        .dispatch_via_bid_protocol(parent_task_id, &bids)
        .await?;
    Ok(serde_json::to_string_pretty(&report).unwrap_or_else(|_| "{}".to_string()))
}

async fn execute_list_collaboration_sessions(
    args: &serde_json::Value,
    agent: &AgentEngine,
    task_id: Option<&str>,
) -> Result<String> {
    if !agent.config.read().await.collaboration.enabled {
        anyhow::bail!("collaboration capability is disabled in agent config");
    }
    let fallback_parent = if let Some(task_id) = task_id {
        agent
            .list_tasks()
            .await
            .into_iter()
            .find(|task| task.id == task_id)
            .and_then(|task| task.parent_task_id.or_else(|| Some(task.id)))
    } else {
        None
    };
    let parent_task_id = args
        .get("parent_task_id")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .or(fallback_parent);
    let report = agent
        .collaboration_sessions_json(parent_task_id.as_deref())
        .await?;
    Ok(serde_json::to_string_pretty(&report).unwrap_or_else(|_| "[]".to_string()))
}

async fn execute_enqueue_task(args: &serde_json::Value, agent: &AgentEngine) -> Result<String> {
    let description = args
        .get("description")
        .and_then(|value| value.as_str())
        .ok_or_else(|| anyhow::anyhow!("missing 'description' argument"))?
        .trim()
        .to_string();
    if description.is_empty() {
        anyhow::bail!("'description' must not be empty");
    }

    let command = args
        .get("command")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);
    let title = args
        .get("title")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| default_task_title(&description, command.as_deref()));
    let priority = args
        .get("priority")
        .and_then(|value| value.as_str())
        .unwrap_or("normal");
    let session = args
        .get("session")
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
    let scheduled_at = parse_scheduled_at(args)?;

    let task = agent
        .enqueue_task(
            title,
            description,
            priority,
            command,
            session,
            dependencies,
            scheduled_at,
            "agent",
            None,
            None,
            None,
            None,
        )
        .await;

    Ok(serde_json::to_string_pretty(&task).unwrap_or_else(|_| format!("queued task {}", task.id)))
}

async fn execute_start_goal_run(
    args: &serde_json::Value,
    agent: &AgentEngine,
    current_thread_id: &str,
    current_session_id: Option<SessionId>,
) -> Result<String> {
    let goal = args
        .get("goal")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow::anyhow!("missing 'goal' argument"))?
        .to_string();
    let title = args
        .get("title")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);
    let thread_id = args
        .get("thread_id")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .or_else(|| Some(current_thread_id.to_string()));
    let session_id = args
        .get("session_id")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .or_else(|| current_session_id.map(|value| value.to_string()));
    let priority = args
        .get("priority")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let autonomy_level = args
        .get("autonomy_level")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);
    let requires_approval = args
        .get("requires_approval")
        .and_then(|value| value.as_bool())
        .unwrap_or(false);
    let launch_assignments = parse_goal_launch_assignments(args)?;

    let goal_run = agent
        .start_goal_run_with_surface_and_approval_policy(
            goal,
            title,
            thread_id,
            session_id,
            priority,
            None,
            autonomy_level,
            None,
            requires_approval,
            launch_assignments,
        )
        .await;

    Ok(serde_json::to_string_pretty(&goal_run).unwrap_or_else(|_| "{}".to_string()))
}

fn parse_goal_launch_assignments(
    args: &serde_json::Value,
) -> Result<Option<Vec<crate::agent::types::GoalAgentAssignment>>> {
    let Some(raw) = args.get("launch_assignments") else {
        return Ok(None);
    };
    let assignments = raw
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("'launch_assignments' must be an array"))?;
    if assignments.is_empty() {
        return Ok(None);
    }

    assignments
        .iter()
        .enumerate()
        .map(|(index, value)| {
            let role_id = required_assignment_string(value, index, "role_id")?;
            let provider = required_assignment_string(value, index, "provider")?;
            let model = required_assignment_string(value, index, "model")?;
            let reasoning_effort = value
                .get("reasoning_effort")
                .and_then(|value| value.as_str())
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned);
            let enabled = value
                .get("enabled")
                .and_then(|value| value.as_bool())
                .unwrap_or(true);
            let inherit_from_main = value
                .get("inherit_from_main")
                .and_then(|value| value.as_bool())
                .unwrap_or(false);
            Ok(crate::agent::types::GoalAgentAssignment {
                role_id,
                enabled,
                provider,
                model,
                reasoning_effort,
                inherit_from_main,
            })
        })
        .collect::<Result<Vec<_>>>()
        .map(Some)
}

fn required_assignment_string(
    value: &serde_json::Value,
    index: usize,
    field: &str,
) -> Result<String> {
    value
        .get(field)
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .ok_or_else(|| {
            anyhow::anyhow!("launch_assignments[{index}].{field} must be a non-empty string")
        })
}

async fn execute_list_tasks(args: &serde_json::Value, agent: &AgentEngine) -> Result<String> {
    let status_filter = args
        .get("status")
        .and_then(|value| value.as_str())
        .map(|value| value.trim().to_ascii_lowercase());
    let limit = args
        .get("limit")
        .and_then(|value| value.as_u64())
        .map(|value| value as usize);

    let mut tasks = agent.list_tasks().await;
    if let Some(status_filter) = status_filter {
        tasks.retain(|task| {
            serde_json::to_value(task.status)
                .ok()
                .and_then(|value| value.as_str().map(ToOwned::to_owned))
                .map(|value| value == status_filter)
                .unwrap_or(false)
        });
    }
    if let Some(limit) = limit {
        tasks.truncate(limit);
    }

    Ok(serde_json::to_string_pretty(&tasks).unwrap_or_else(|_| "[]".to_string()))
}

async fn execute_list_goal_runs(agent: &AgentEngine) -> Result<String> {
    let goal_runs = agent.list_goal_runs().await;
    Ok(serde_json::to_string_pretty(&goal_runs).unwrap_or_else(|_| "[]".to_string()))
}

async fn execute_submit_goal_step_verdict(
    args: &serde_json::Value,
    agent: &AgentEngine,
    current_task_id: Option<&str>,
) -> Result<String> {
    let explicit_task_id = args
        .get("task_id")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let task_id = current_task_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .or(explicit_task_id)
        .ok_or_else(|| {
            anyhow::anyhow!("submit_goal_step_verdict requires a current verification task")
        })?;
    if let (Some(current_task_id), Some(explicit_task_id)) = (
        current_task_id
            .map(str::trim)
            .filter(|value| !value.is_empty()),
        explicit_task_id,
    ) {
        if current_task_id != explicit_task_id {
            anyhow::bail!(
                "task_id mismatch: current task is '{}' but tool received '{}'",
                current_task_id,
                explicit_task_id
            );
        }
    }
    let verdict = match args
        .get("verdict")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("pass") => GoalStepReviewVerdict::Pass,
        Some("fail") => GoalStepReviewVerdict::Fail,
        Some(other) => anyhow::bail!("unsupported verdict '{other}'; expected 'pass' or 'fail'"),
        None => anyhow::bail!("missing 'verdict' argument"),
    };
    let explanation = args
        .get("explanation")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow::anyhow!("missing non-empty 'explanation' argument"))?
        .to_string();

    let task = {
        let tasks = agent.tasks.lock().await;
        tasks
            .iter()
            .find(|task| task.id == task_id)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("task {task_id} not found"))?
    };
    if task.source != super::GOAL_VERIFICATION_SOURCE {
        anyhow::bail!(
            "submit_goal_step_verdict can only be used by goal verification tasks; current task source is '{}'.",
            task.source
        );
    }

    let goal_run_id = task.goal_run_id.as_deref().ok_or_else(|| {
        anyhow::anyhow!("current verification task is missing goal_run_id context")
    })?;
    let goal_step_id = task.goal_step_id.as_deref().ok_or_else(|| {
        anyhow::anyhow!("current verification task is missing goal_step_id context")
    })?;
    if let Some(provided_goal_run_id) = args
        .get("goal_run_id")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        if provided_goal_run_id != goal_run_id {
            anyhow::bail!(
                "goal_run_id mismatch: current task is bound to '{}' but tool received '{}'",
                goal_run_id,
                provided_goal_run_id
            );
        }
    }
    if let Some(provided_goal_step_id) = args
        .get("goal_step_id")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        if provided_goal_step_id != goal_step_id {
            anyhow::bail!(
                "goal_step_id mismatch: current task is bound to '{}' but tool received '{}'",
                goal_step_id,
                provided_goal_step_id
            );
        }
    }

    let goal_run = agent
        .get_goal_run(goal_run_id)
        .await
        .ok_or_else(|| anyhow::anyhow!("goal run {goal_run_id} not found"))?;
    let active_step_id = goal_run
        .steps
        .get(goal_run.current_step_index)
        .map(|step| step.id.as_str())
        .ok_or_else(|| anyhow::anyhow!("goal run {goal_run_id} has no active step"))?;
    if active_step_id != goal_step_id {
        anyhow::bail!(
            "current goal step is '{}' but verification task is bound to '{}'",
            active_step_id,
            goal_step_id
        );
    }

    let record = GoalStepReviewRecord {
        task_id: task.id.clone(),
        goal_run_id: goal_run_id.to_string(),
        goal_step_id: goal_step_id.to_string(),
        verdict,
        explanation,
        submitted_at: crate::agent::now_millis(),
    };
    let record_json = serde_json::to_string(&record)?;
    agent
        .history
        .set_consolidation_state(
            &super::goal_step_verdict_state_key(task_id),
            &record_json,
            record.submitted_at,
        )
        .await?;

    let updated = {
        let mut tasks = agent.tasks.lock().await;
        let Some(task) = tasks.iter_mut().find(|task| task.id == task_id) else {
            anyhow::bail!("task {task_id} disappeared while recording verdict");
        };
        task.result = Some(format!(
            "Structured verdict: {:?}\n{}",
            record.verdict, record.explanation
        ));
        task.logs.push(make_task_log_entry(
            task.retry_count,
            TaskLogLevel::Info,
            "verification",
            "structured goal-step verdict submitted",
            Some(record_json.clone()),
        ));
        task.clone()
    };
    agent.persist_tasks().await;
    agent.emit_task_update(&updated, Some("Goal-step verdict submitted".into()));
    agent
        .record_provenance_event(
            "goal_step_verdict_submitted",
            "structured goal-step verification verdict submitted",
            serde_json::to_value(&record).unwrap_or_else(|_| serde_json::json!({})),
            Some(goal_run_id),
            Some(task_id),
            task.thread_id.as_deref(),
            None,
            None,
        )
        .await;

    Ok(serde_json::to_string_pretty(&record).unwrap_or_else(|_| "{}".to_string()))
}

async fn execute_list_triggers(_args: &serde_json::Value, agent: &AgentEngine) -> Result<String> {
    agent.ensure_default_event_triggers().await?;
    let payload = agent.list_event_triggers_json().await?;
    Ok(serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "[]".to_string()))
}

async fn execute_ingest_webhook_event(
    args: &serde_json::Value,
    agent: &AgentEngine,
) -> Result<String> {
    agent.ensure_default_event_triggers().await?;
    let payload = agent.ingest_webhook_event_json(args).await?;
    Ok(serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "{}".to_string()))
}

async fn execute_add_trigger(args: &serde_json::Value, agent: &AgentEngine) -> Result<String> {
    let payload = agent.add_event_trigger_from_args(args).await?;
    Ok(serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "{}".to_string()))
}

async fn execute_show_dreams(args: &serde_json::Value, agent: &AgentEngine) -> Result<String> {
    let limit = args
        .get("limit")
        .and_then(|value| value.as_u64())
        .map(|value| value as usize)
        .unwrap_or(10);
    let payload = agent.show_dreams_payload(limit).await?;
    Ok(serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "{}".to_string()))
}

async fn execute_show_harness_state(
    args: &serde_json::Value,
    agent: &AgentEngine,
    current_thread_id: &str,
    current_task_id: Option<&str>,
) -> Result<String> {
    let requested_thread_id = args
        .get("thread_id")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .or_else(|| Some(current_thread_id.to_string()));
    let requested_task_id = args
        .get("task_id")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .or_else(|| current_task_id.map(ToOwned::to_owned));
    let requested_goal_run_id = args
        .get("goal_run_id")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);
    let limit = args
        .get("limit")
        .and_then(|value| value.as_u64())
        .map(|value| value as usize)
        .unwrap_or(5);

    let resolved_task = if let Some(task_id) = requested_task_id.as_deref() {
        Some(
            agent
                .list_tasks()
                .await
                .into_iter()
                .find(|task| task.id == task_id)
                .ok_or_else(|| anyhow::anyhow!("task {task_id} not found"))?,
        )
    } else {
        None
    };
    let goal_run_id = requested_goal_run_id.or_else(|| {
        resolved_task
            .as_ref()
            .and_then(|task| task.goal_run_id.clone())
    });
    let task_id = resolved_task
        .as_ref()
        .map(|task| task.id.clone())
        .or(requested_task_id);

    let projection = crate::agent::harness::load_harness_state_projection(
        &agent.history,
        requested_thread_id.as_deref(),
        goal_run_id.as_deref(),
        task_id.as_deref(),
    )
    .await?;
    let payload = crate::agent::harness::build_harness_state_payload(
        &projection,
        requested_thread_id.as_deref(),
        goal_run_id.as_deref(),
        task_id.as_deref(),
        limit,
    );
    Ok(serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "{}".to_string()))
}

async fn execute_get_todos(
    args: &serde_json::Value,
    agent: &AgentEngine,
    current_task_id: Option<&str>,
) -> Result<String> {
    let thread_id = args
        .get("thread_id")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow::anyhow!("missing 'thread_id' argument"))?;
    let requested_task_id = args
        .get("task_id")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let resolved_task = if let Some(task_id) = requested_task_id.or(current_task_id) {
        Some(
            agent
                .list_tasks()
                .await
                .into_iter()
                .find(|task| task.id == task_id)
                .ok_or_else(|| anyhow::anyhow!("task {task_id} not found"))?,
        )
    } else {
        None
    };
    let items = agent.get_todos(thread_id).await;

    Ok(serde_json::json!({
        "thread_id": thread_id,
        "task_id": resolved_task.as_ref().map(|task| task.id.as_str()),
        "goal_run_id": resolved_task.as_ref().and_then(|task| task.goal_run_id.as_deref()),
        "items": items,
    })
    .to_string())
}

async fn execute_cancel_task(args: &serde_json::Value, agent: &AgentEngine) -> Result<String> {
    let task_id = args
        .get("task_id")
        .and_then(|value| value.as_str())
        .ok_or_else(|| anyhow::anyhow!("missing 'task_id' argument"))?;
    let cancelled = agent.cancel_task(task_id).await;
    Ok(serde_json::json!({
        "task_id": task_id,
        "cancelled": cancelled,
    })
    .to_string())
}
