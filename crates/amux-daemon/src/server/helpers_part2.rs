async fn persist_gateway_health_update(
    agent: &Arc<AgentEngine>,
    update: GatewayHealthState,
) -> Result<()> {
    let updated_at_ms = current_time_ms();
    agent
        .history
        .upsert_gateway_health_snapshot(&update, updated_at_ms)
        .await?;

    let platform_key = update.platform.to_ascii_lowercase();
    let platform_label = match platform_key.as_str() {
        "slack" => "Slack",
        "discord" => "Discord",
        "telegram" => "Telegram",
        other => other,
    }
    .to_string();
    let new_status = match update.status {
        GatewayConnectionStatus::Connected => {
            crate::agent::RuntimeGatewayConnectionStatus::Connected
        }
        GatewayConnectionStatus::Disconnected => {
            crate::agent::RuntimeGatewayConnectionStatus::Disconnected
        }
        GatewayConnectionStatus::Error => crate::agent::RuntimeGatewayConnectionStatus::Error,
    };
    let mut previous_status = None;

    let mut gw_guard = agent.gateway_state.lock().await;
    if let Some(gateway_state) = gw_guard.as_mut() {
        match platform_key.as_str() {
            "slack" => {
                previous_status = Some(gateway_state.slack_health.status);
                gateway_state.slack_health.status = new_status;
                gateway_state.slack_health.last_success_at = update.last_success_at_ms;
                gateway_state.slack_health.last_error_at = update.last_error_at_ms;
                gateway_state.slack_health.consecutive_failure_count =
                    update.consecutive_failure_count;
                gateway_state.slack_health.last_error = update.last_error.clone();
                gateway_state.slack_health.current_backoff_secs = update.current_backoff_secs;
            }
            "discord" => {
                previous_status = Some(gateway_state.discord_health.status);
                gateway_state.discord_health.status = new_status;
                gateway_state.discord_health.last_success_at = update.last_success_at_ms;
                gateway_state.discord_health.last_error_at = update.last_error_at_ms;
                gateway_state.discord_health.consecutive_failure_count =
                    update.consecutive_failure_count;
                gateway_state.discord_health.last_error = update.last_error.clone();
                gateway_state.discord_health.current_backoff_secs = update.current_backoff_secs;
            }
            "telegram" => {
                previous_status = Some(gateway_state.telegram_health.status);
                gateway_state.telegram_health.status = new_status;
                gateway_state.telegram_health.last_success_at = update.last_success_at_ms;
                gateway_state.telegram_health.last_error_at = update.last_error_at_ms;
                gateway_state.telegram_health.consecutive_failure_count =
                    update.consecutive_failure_count;
                gateway_state.telegram_health.last_error = update.last_error.clone();
                gateway_state.telegram_health.current_backoff_secs = update.current_backoff_secs;
            }
            _ => {}
        }
    }

    drop(gw_guard);

    if previous_status != Some(new_status) {
        let status = match update.status {
            GatewayConnectionStatus::Connected => "connected",
            GatewayConnectionStatus::Disconnected => "disconnected",
            GatewayConnectionStatus::Error => "error",
        }
        .to_string();
        let _ = agent
            .event_sender()
            .send(crate::agent::types::AgentEvent::GatewayStatus {
                platform: platform_label.clone(),
                status: status.clone(),
                last_error: update.last_error.clone(),
                consecutive_failures: Some(update.consecutive_failure_count),
            });

        let description = match (status.as_str(), update.last_error.as_deref()) {
            ("connected", _) => format!("{platform_label} reconnected"),
            ("error", Some(err)) => format!(
                "{platform_label} disconnected after {} failures: {err}",
                update.consecutive_failure_count
            ),
            ("error", None) => format!("{platform_label} disconnected"),
            ("disconnected", _) => format!("{platform_label} disconnected"),
            _ => format!("{platform_label} status: {status}"),
        };

        let _ = agent
            .event_sender()
            .send(crate::agent::types::AgentEvent::HeartbeatDigest {
                cycle_id: format!("gateway_health_{}", uuid::Uuid::new_v4()),
                actionable: status != "connected",
                digest: description.clone(),
                items: vec![crate::agent::types::HeartbeatDigestItem {
                    priority: if status == "connected" { 3 } else { 1 },
                    check_type: crate::agent::types::HeartbeatCheckType::UnrepliedGatewayMessages,
                    title: description.clone(),
                    suggestion: if status == "connected" {
                        format!("{platform_label} is back online")
                    } else {
                        format!("Check {platform_label} API credentials and connectivity")
                    },
                }],
                checked_at: updated_at_ms,
                explanation: None,
                confidence: None,
            });

        let audit_entry = crate::history::AuditEntryRow {
            id: format!("gw_health_{}", uuid::Uuid::new_v4()),
            timestamp: updated_at_ms as i64,
            action_type: "gateway_health_transition".to_string(),
            summary: format!("{platform_label} -> {status}"),
            explanation: update.last_error.clone(),
            confidence: None,
            confidence_band: None,
            causal_trace_id: None,
            thread_id: None,
            goal_run_id: None,
            task_id: None,
            raw_data_json: Some(
                serde_json::json!({
                    "platform": platform_label,
                    "new_status": status,
                    "consecutive_failures": update.consecutive_failure_count,
                })
                .to_string(),
            ),
        };
        agent.history.insert_action_audit(&audit_entry).await?;
    }

    Ok(())
}

fn gateway_response_channel_key(platform: &str, channel_id: &str) -> Option<String> {
    let label = match platform.to_ascii_lowercase().as_str() {
        "slack" => "Slack",
        "discord" => "Discord",
        "telegram" => "Telegram",
        "whatsapp" => "WhatsApp",
        _ => return None,
    };
    Some(format!("{label}:{channel_id}"))
}

fn is_expected_disconnect_error(error: &anyhow::Error) -> bool {
    let expected_io_error = |kind: std::io::ErrorKind| {
        matches!(
            kind,
            std::io::ErrorKind::UnexpectedEof
                | std::io::ErrorKind::BrokenPipe
                | std::io::ErrorKind::ConnectionReset
                | std::io::ErrorKind::ConnectionAborted
                | std::io::ErrorKind::NotConnected
        )
    };
    if error.chain().any(|cause| {
        cause
            .downcast_ref::<std::io::Error>()
            .is_some_and(|io| expected_io_error(io.kind()))
    }) {
        return true;
    }

    let message = error.to_string().to_ascii_lowercase();
    message.contains("unexpected end of file")
        || message.contains("connection reset by peer")
        || message.contains("broken pipe")
}

fn normalize_session_tag(value: &str) -> Option<String> {
    let mut normalized = String::with_capacity(value.len());
    let mut last_was_dash = false;
    for ch in value.chars() {
        let mapped = if ch.is_ascii_alphanumeric() {
            last_was_dash = false;
            Some(ch.to_ascii_lowercase())
        } else if matches!(ch, '/' | '\\' | ' ' | ':' | '_' | '.' | '-') {
            if last_was_dash {
                None
            } else {
                last_was_dash = true;
                Some('-')
            }
        } else {
            None
        };
        if let Some(ch) = mapped {
            normalized.push(ch);
        }
    }

    let trimmed = normalized.trim_matches('-');
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

fn summarize_session_output(recent_output: Option<&str>) -> Option<String> {
    let line = recent_output?
        .lines()
        .rev()
        .map(str::trim)
        .find(|line| !line.is_empty())?;
    let condensed = line.split_whitespace().collect::<Vec<_>>().join(" ");
    if condensed.is_empty() {
        return None;
    }
    const MAX_CHARS: usize = 120;
    if condensed.chars().count() <= MAX_CHARS {
        return Some(condensed);
    }
    Some(condensed.chars().take(MAX_CHARS - 3).collect::<String>() + "...")
}

fn build_session_end_episode_payload(
    session_id: &str,
    info: Option<&SessionInfo>,
    recent_output: Option<&str>,
) -> (String, Vec<String>) {
    let title = info
        .and_then(|value| value.title.as_deref())
        .filter(|value| !value.is_empty());
    let active_command = info
        .and_then(|value| value.active_command.as_deref())
        .filter(|value| !value.is_empty());
    let cwd = info
        .and_then(|value| value.cwd.as_deref())
        .filter(|value| !value.is_empty());
    let cwd_label = cwd
        .and_then(|value| Path::new(value).file_name())
        .and_then(|value| value.to_str());
    let recent_output_summary = summarize_session_output(recent_output);

    let focus = title
        .map(ToOwned::to_owned)
        .or_else(|| active_command.map(ToOwned::to_owned))
        .or_else(|| cwd_label.map(|value| format!("workspace {value}")))
        .unwrap_or_else(|| format!("session {session_id}"));

    let mut summary = format!("{focus} ended");
    if let Some(cwd_label) = cwd_label {
        summary.push_str(&format!(" in {cwd_label}"));
    }
    if let Some(output) = recent_output_summary {
        summary.push_str(&format!(". Last output: {output}"));
    }

    let mut entities = vec![format!("session:{session_id}")];
    if let Some(workspace_id) = info.and_then(|value| value.workspace_id.as_deref()) {
        entities.push(format!("workspace:{workspace_id}"));
    }
    if let Some(tag) = cwd_label.and_then(normalize_session_tag) {
        entities.push(format!("cwd:{tag}"));
    }
    if let Some(command) = active_command
        .and_then(|value| value.split_whitespace().next())
        .and_then(normalize_session_tag)
    {
        entities.push(format!("command:{command}"));
    }
    if let Some(tag) = title.and_then(normalize_session_tag) {
        entities.push(format!("title:{tag}"));
    }
    entities.sort();
    entities.dedup();

    (summary, entities)
}

fn agent_event_thread_id(event: &crate::agent::types::AgentEvent) -> Option<&str> {
    use crate::agent::types::AgentEvent;

    match event {
        AgentEvent::Delta { thread_id, .. }
        | AgentEvent::Reasoning { thread_id, .. }
        | AgentEvent::ToolCall { thread_id, .. }
        | AgentEvent::ToolResult { thread_id, .. }
        | AgentEvent::Done { thread_id, .. }
        | AgentEvent::Error { thread_id, .. }
        | AgentEvent::ThreadCreated { thread_id, .. }
        | AgentEvent::ThreadReloadRequired { thread_id, .. }
        | AgentEvent::TodoUpdate { thread_id, .. }
        | AgentEvent::WorkContextUpdate { thread_id, .. }
        | AgentEvent::RetryStatus { thread_id, .. }
        | AgentEvent::WorkflowNotice { thread_id, .. }
        | AgentEvent::ModeShift { thread_id, .. }
        | AgentEvent::ConfidenceWarning { thread_id, .. }
        | AgentEvent::CounterWhoAlert { thread_id, .. } => Some(thread_id.as_str()),
        AgentEvent::TaskUpdate {
            task: Some(task), ..
        } => task
            .thread_id
            .as_deref()
            .or(task.parent_thread_id.as_deref()),
        AgentEvent::GoalRunUpdate {
            goal_run: Some(goal_run),
            ..
        } => goal_run.thread_id.as_deref(),
        _ => None,
    }
}

#[cfg(not(test))]
async fn start_whatsapp_link_backend(agent: Arc<AgentEngine>) -> Result<()> {
    crate::agent::start_whatsapp_link_native(agent).await
}

#[cfg(not(test))]
async fn maybe_autostart_whatsapp_link(agent: Arc<AgentEngine>) {
    let gateway = agent.config.read().await.gateway.clone();
    if !gateway.enabled {
        return;
    }

    let persisted_path = crate::agent::whatsapp_native_store_path(&agent.data_dir);
    if !persisted_path.exists() {
        tracing::info!(
            persisted_path = %persisted_path.display(),
            "whatsapp link autostart skipped (no persisted daemon state)"
        );
        return;
    }

    tracing::info!(
        persisted_path = %persisted_path.display(),
        "whatsapp link autostarting from persisted daemon state"
    );
    match agent.whatsapp_link.start_if_idle().await {
        Ok(false) => return,
        Ok(true) => {}
        Err(error) => {
            tracing::warn!(
                error = %error,
                "whatsapp link autostart: failed to transition runtime to starting"
            );
            return;
        }
    }
    if let Err(error) = start_whatsapp_link_backend(agent.clone()).await {
        tracing::warn!(
            error = %error,
            "whatsapp link autostart: failed to start backend"
        );
        agent
            .whatsapp_link
            .broadcast_error(error.to_string(), false)
            .await;
    }
}

