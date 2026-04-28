#[derive(Clone)]
struct AllocatedTerminalLane {
    source_session_id: SessionId,
    source_active_command: Option<String>,
    workspace_id: String,
    session_id: SessionId,
    pane_name: String,
}

async fn allocate_terminal_lane(
    args: &serde_json::Value,
    session_manager: &Arc<SessionManager>,
    preferred_session_id: Option<SessionId>,
    event_tx: &broadcast::Sender<AgentEvent>,
    default_pane_name: &str,
) -> Result<AllocatedTerminalLane> {
    let sessions = session_manager.list().await;
    if sessions.is_empty() {
        anyhow::bail!("No active terminal sessions are available to allocate another terminal");
    }

    let source_session =
        if let Some(session_ref) = args.get("session").and_then(|value| value.as_str()) {
            sessions
                .iter()
                .find(|session| {
                    session.id.to_string() == session_ref
                        || session.id.to_string().contains(session_ref)
                })
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("session not found: {session_ref}"))?
        } else {
            let resolved_id = preferred_session_id.unwrap_or(sessions[0].id);
            sessions
                .iter()
                .find(|session| session.id == resolved_id)
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("session not found: {resolved_id}"))?
        };

    let workspace_id = source_session.workspace_id.clone().ok_or_else(|| {
        anyhow::anyhow!(
            "session {} is not attached to a workspace; cannot allocate another terminal lane",
            source_session.id
        )
    })?;
    let pane_name = args
        .get("pane_name")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| default_pane_name.to_string());
    let cwd = args
        .get("cwd")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .or_else(|| source_session.cwd.clone());

    let (new_session_id, _, source_active_command) = session_manager
        .clone_session(
            source_session.id,
            Some(workspace_id.clone()),
            None,
            None,
            false,
            cwd.clone(),
        )
        .await?;

    let _ = event_tx.send(AgentEvent::WorkspaceCommand {
        command: "attach_agent_terminal".to_string(),
        args: serde_json::json!({
            "workspace_id": workspace_id.clone(),
            "session_id": new_session_id.to_string(),
            "pane_name": pane_name.clone(),
            "cwd": cwd.clone(),
        }),
    });

    Ok(AllocatedTerminalLane {
        source_session_id: source_session.id,
        source_active_command,
        workspace_id,
        session_id: new_session_id,
        pane_name,
    })
}

async fn execute_allocate_terminal(
    args: &serde_json::Value,
    session_manager: &Arc<SessionManager>,
    preferred_session_id: Option<SessionId>,
    event_tx: &broadcast::Sender<AgentEvent>,
) -> Result<String> {
    let default_pane_name =
        if let Some(session_ref) = args.get("session").and_then(|value| value.as_str()) {
            let sessions = session_manager.list().await;
            let workspace_id = sessions
                .iter()
                .find(|session| {
                    session.id.to_string() == session_ref
                        || session.id.to_string().contains(session_ref)
                })
                .and_then(|session| session.workspace_id.as_ref())
                .cloned();
            if let Some(workspace_id) = workspace_id {
                format!(
                    "Work {}",
                    session_manager.list_workspace(&workspace_id).await.len() + 1
                )
            } else {
                "Work".to_string()
            }
        } else {
            "Work".to_string()
        };
    let lane = allocate_terminal_lane(
        args,
        session_manager,
        preferred_session_id,
        event_tx,
        &default_pane_name,
    )
    .await?;

    let source_command_suffix = lane
        .source_active_command
        .as_deref()
        .map(|command| format!("\nSource session active command: {command}"))
        .unwrap_or_default();
    Ok(format!(
        "Allocated terminal {} in workspace {} from source session {}. Frontend attachment requested for pane \"{}\". Use the new session ID for subsequent managed commands.{}",
        lane.session_id,
        lane.workspace_id,
        lane.source_session_id,
        lane.pane_name,
        source_command_suffix
    ))
}

fn normalize_task_runtime(value: Option<&str>) -> Result<String> {
    match value.unwrap_or("daemon").trim() {
        "" | "daemon" => Ok("daemon".to_string()),
        "hermes" => Ok("hermes".to_string()),
        "openclaw" => Ok("openclaw".to_string()),
        other => Err(anyhow::anyhow!("unsupported subagent runtime: {other}")),
    }
}
