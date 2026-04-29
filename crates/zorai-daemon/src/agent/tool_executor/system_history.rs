async fn execute_search_files(args: &serde_json::Value) -> Result<String> {
    execute_search_files_with_runner(args, run_search_files_subprocess).await
}

async fn execute_system_info() -> Result<String> {
    use sysinfo::System;

    let mut sys = System::new_all();
    sys.refresh_all();

    let total_mem = sys.total_memory();
    let used_mem = sys.used_memory();
    let cpu_count = sys.cpus().len();
    let load_avg = System::load_average();

    Ok(format!(
        "CPU cores: {cpu_count}\n\
         Load average: {:.2} {:.2} {:.2}\n\
         Memory: {:.1} GB / {:.1} GB ({:.0}% used)\n\
         OS: {} {}\n\
         Hostname: {}",
        load_avg.one,
        load_avg.five,
        load_avg.fifteen,
        used_mem as f64 / 1_073_741_824.0,
        total_mem as f64 / 1_073_741_824.0,
        (used_mem as f64 / total_mem as f64) * 100.0,
        System::name().unwrap_or_default(),
        System::os_version().unwrap_or_default(),
        System::host_name().unwrap_or_default(),
    ))
}

async fn execute_current_datetime() -> Result<String> {
    let local_now = chrono::Local::now();
    let utc_now = chrono::Utc::now();

    Ok(format!(
        "Current datetime:\n\
         - Local: {}\n\
         - UTC: {}\n\
         - Unix timestamp (ms): {}",
        local_now.to_rfc3339(),
        utc_now.to_rfc3339(),
        utc_now.timestamp_millis(),
    ))
}

async fn execute_list_processes(args: &serde_json::Value) -> Result<String> {
    let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(20) as usize;

    use sysinfo::System;
    let mut sys = System::new_all();
    sys.refresh_all();

    let mut procs: Vec<(u32, String, f32, u64)> = sys
        .processes()
        .values()
        .map(|p| {
            (
                p.pid().as_u32(),
                p.name().to_string(),
                p.cpu_usage(),
                p.memory(),
            )
        })
        .collect();

    procs.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));

    let header = format!(
        "{:<8} {:<30} {:>8} {:>12}",
        "PID", "NAME", "CPU%", "MEM(MB)"
    );
    let rows: Vec<String> = procs
        .iter()
        .take(limit)
        .map(|(pid, name, cpu, mem)| {
            format!(
                "{:<8} {:<30} {:>7.1}% {:>12.1}",
                pid,
                if name.len() > 30 { &name[..30] } else { name },
                cpu,
                *mem as f64 / 1_048_576.0
            )
        })
        .collect();

    Ok(format!("{header}\n{}", rows.join("\n")))
}

async fn execute_search_history(
    args: &serde_json::Value,
    agent: &AgentEngine,
) -> Result<String> {
    let query = args
        .get("query")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("missing 'query' argument"))?;

    let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(20) as usize;

    let (summary, hits) = agent.search_history_semantic_first(query, limit).await?;

    if hits.is_empty() {
        Ok("No matching history entries.".into())
    } else {
        let lines: Vec<String> = hits
            .iter()
            .map(|h| {
                format!(
                    "[{:.1}] {} — {}",
                    h.score,
                    h.title,
                    h.excerpt.chars().take(120).collect::<String>(),
                )
            })
            .collect();
        Ok(format!("{summary}\n\n{}", lines.join("\n")))
    }
}

async fn execute_fetch_gateway_history(
    args: &serde_json::Value,
    agent: &AgentEngine,
    thread_id: &str,
) -> Result<String> {
    let count = args
        .get("count")
        .and_then(|v| v.as_u64())
        .unwrap_or(10)
        .clamp(1, 100) as usize;

    let messages = agent.history.list_recent_messages(thread_id, count).await?;
    if messages.is_empty() {
        return Ok("No prior messages found for this gateway thread.".to_string());
    }

    let mut lines = Vec::with_capacity(messages.len() + 1);
    lines.push(format!(
        "Recent gateway thread history ({} messages):",
        messages.len()
    ));
    for msg in messages {
        let role = msg.role;
        let content = msg
            .content
            .replace('\n', " ")
            .chars()
            .take(240)
            .collect::<String>();
        lines.push(format!("- {role}: {content}"));
    }
    Ok(lines.join("\n"))
}

async fn execute_session_search(
    args: &serde_json::Value,
    session_manager: &Arc<SessionManager>,
) -> Result<String> {
    let query = args
        .get("query")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("missing 'query' argument"))?
        .trim();
    if query.is_empty() {
        return Err(anyhow::anyhow!("'query' must not be empty"));
    }

    let limit = args
        .get("limit")
        .and_then(|v| v.as_u64())
        .unwrap_or(8)
        .clamp(1, 20) as usize;
    let body = run_session_search(session_manager, query, limit).await?;
    if body.chars().count() > SESSION_SEARCH_OUTPUT_MAX_CHARS {
        Ok(body
            .chars()
            .take(SESSION_SEARCH_OUTPUT_MAX_CHARS)
            .collect::<String>())
    } else {
        Ok(body)
    }
}

async fn execute_agent_query_memory(
    args: &serde_json::Value,
    agent: &AgentEngine,
) -> Result<String> {
    let query = args
        .get("query")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("missing 'query' argument"))?
        .trim();
    if query.is_empty() {
        anyhow::bail!("'query' must not be empty");
    }
    agent.query_honcho_memory(query).await
}

async fn execute_onecontext_search(args: &serde_json::Value) -> Result<String> {
    execute_onecontext_search_with_runner(args, super::aline_available(), |request| async move {
        run_onecontext_search_subprocess(request).await
    })
    .await
}

async fn execute_list_sessions(session_manager: &Arc<SessionManager>) -> Result<String> {
    // If we have frontend topology, use it for a richer view that includes
    // browser panels and workspace/surface hierarchy.
    if let Some(topology) = session_manager.read_workspace_topology() {
        let sessions = session_manager.list().await;
        let formatted = zorai_protocol::format_topology(&topology, &sessions);
        if !formatted.is_empty() {
            return Ok(formatted);
        }
        return Ok("No active sessions or panes.".into());
    }

    // Fallback: no topology reported, list raw sessions.
    let sessions = session_manager.list().await;

    if sessions.is_empty() {
        Ok("No active sessions.".into())
    } else {
        let lines: Vec<String> = sessions
            .iter()
            .map(|s| {
                let mut line = format!(
                    "{} cols={} rows={} alive={} cwd={}",
                    s.id,
                    s.cols,
                    s.rows,
                    s.is_alive,
                    s.cwd.as_deref().unwrap_or("?"),
                );
                if let Some(cmd) = s.active_command.as_deref() {
                    line.push_str(&format!(" cmd={cmd}"));
                }
                if let Some(ws) = s.workspace_id.as_deref() {
                    line.push_str(&format!(" workspace={ws}"));
                }
                line
            })
            .collect();
        Ok(lines.join("\n"))
    }
}

async fn execute_notify(
    args: &serde_json::Value,
    agent: &AgentEngine,
) -> Result<String> {
    let title = args
        .get("title")
        .and_then(|v| v.as_str())
        .unwrap_or("Notification");
    let message = args.get("message").and_then(|v| v.as_str()).unwrap_or("");
    let severity = match args.get("severity").and_then(|v| v.as_str()) {
        Some("warning") => NotificationSeverity::Warning,
        Some("alert") => NotificationSeverity::Alert,
        Some("error") => NotificationSeverity::Error,
        _ => NotificationSeverity::Info,
    };

    let _ = agent.event_tx.send(AgentEvent::Notification {
        title: title.into(),
        body: message.into(),
        severity,
        channels: vec!["in-app".into()],
    });

    let now = crate::agent::now_millis() as i64;
    let _ = agent
        .upsert_inbox_notification(zorai_protocol::InboxNotification {
            id: format!("tool-notify:{}", uuid::Uuid::new_v4()),
            source: "tool".to_string(),
            kind: "tool_notify_user".to_string(),
            title: title.to_string(),
            body: message.to_string(),
            subtitle: Some("agent tool".to_string()),
            severity: match severity {
                NotificationSeverity::Info => "info",
                NotificationSeverity::Warning => "warning",
                NotificationSeverity::Alert => "alert",
                NotificationSeverity::Error => "error",
            }
            .to_string(),
            created_at: now,
            updated_at: now,
            read_at: None,
            archived_at: None,
            deleted_at: None,
            actions: Vec::new(),
            metadata_json: None,
        })
        .await;

    Ok(format!("Notification sent: {title}"))
}

async fn execute_update_memory(
    args: &serde_json::Value,
    agent: &AgentEngine,
    thread_id: &str,
    task_id: Option<&str>,
    agent_data_dir: &std::path::Path,
) -> Result<String> {
    let target = MemoryTarget::parse(
        args.get("target")
            .and_then(|v| v.as_str())
            .unwrap_or("memory"),
    )?;
    let mode = MemoryUpdateMode::parse(
        args.get("mode")
            .and_then(|v| v.as_str())
            .unwrap_or("replace"),
    )?;
    let content = args
        .get("content")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("missing 'content' argument"))?;
    let goal_run_id = if let Some(current_task_id) = task_id {
        let tasks = agent.tasks.lock().await;
        tasks
            .iter()
            .find(|task| task.id == current_task_id)
            .and_then(|task| task.goal_run_id.clone())
    } else {
        None
    };
    let acting_scope_id = if let Some(current_task_id) = task_id {
        let tasks = agent.tasks.lock().await;
        crate::agent::agent_scope_id_for_task(tasks.iter().find(|task| task.id == current_task_id))
    } else {
        MAIN_AGENT_ID.to_string()
    };
    if target == MemoryTarget::User && !crate::agent::is_main_agent_scope(&acting_scope_id) {
        let sender = if let Some(current_task_id) = task_id {
            let tasks = agent.tasks.lock().await;
            sender_name_for_task(tasks.iter().find(|task| task.id == current_task_id))
        } else {
            canonical_agent_name(&acting_scope_id).to_string()
        };
        let mediation_request = format!(
            "A non-main agent is requesting a shared USER.md update.\n\
             Requesting agent: {} ({})\n\
             Source thread: {}\n\
             Goal run: {}\n\
             Requested mode: {}\n\
             Proposed content:\n{}\n\n\
             Evaluate whether this belongs in shared USER.md. If yes, apply it yourself with the appropriate memory update tool. If not, reject it and explain briefly.",
            sender,
            acting_scope_id,
            thread_id,
            goal_run_id.as_deref().unwrap_or("none"),
            match mode {
                MemoryUpdateMode::Replace => "replace",
                MemoryUpdateMode::Append => "append",
                MemoryUpdateMode::Remove => "remove",
            },
            content.trim(),
        );
        let result = agent
            .send_internal_agent_message(&sender, MAIN_AGENT_ID, &mediation_request, None)
            .await?;
        return Ok(result.response);
    }
    apply_memory_update(
        agent_data_dir,
        &agent.history,
        target,
        mode,
        content,
        MemoryWriteContext {
            source_kind: "tool",
            thread_id: Some(thread_id),
            task_id,
            goal_run_id: goal_run_id.as_deref(),
        },
    )
    .await
}
