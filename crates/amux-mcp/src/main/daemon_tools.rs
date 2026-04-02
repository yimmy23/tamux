use super::*;

pub(super) async fn tool_execute_command(args: &Value) -> Result<Value> {
    let session_id: uuid::Uuid = args
        .get("session_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("missing required parameter: session_id"))?
        .parse()
        .context("invalid session_id UUID")?;

    let command = args
        .get("command")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("missing required parameter: command"))?
        .to_string();

    let rationale = args
        .get("rationale")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("missing required parameter: rationale"))?
        .to_string();

    let msg = ClientMessage::ExecuteManagedCommand {
        id: session_id,
        request: ManagedCommandRequest {
            command: command.clone(),
            rationale,
            allow_network: false,
            sandbox_enabled: true,
            security_level: SecurityLevel::Moderate,
            cwd: None,
            language_hint: None,
            source: ManagedCommandSource::Agent,
        },
        client_surface: None,
    };

    let mut framed = connect_daemon().await?;
    framed.send(msg).await.context("failed to send to daemon")?;

    let mut events: Vec<Value> = Vec::new();

    while let Some(resp) = framed.next().await {
        let resp = resp.context("error reading from daemon")?;
        match resp {
            DaemonMessage::ManagedCommandQueued {
                execution_id,
                position,
                snapshot,
                ..
            } => {
                events.push(serde_json::json!({
                    "event": "queued",
                    "execution_id": execution_id,
                    "position": position,
                    "snapshot": snapshot,
                }));
            }
            DaemonMessage::ManagedCommandStarted {
                execution_id,
                command: cmd,
                ..
            } => {
                events.push(serde_json::json!({
                    "event": "started",
                    "execution_id": execution_id,
                    "command": cmd,
                }));
            }
            DaemonMessage::ManagedCommandFinished {
                execution_id,
                command: cmd,
                exit_code,
                duration_ms,
                snapshot,
                ..
            } => {
                events.push(serde_json::json!({
                    "event": "finished",
                    "execution_id": execution_id,
                    "command": cmd,
                    "exit_code": exit_code,
                    "duration_ms": duration_ms,
                    "snapshot": snapshot,
                }));
                break;
            }
            DaemonMessage::ManagedCommandRejected {
                execution_id,
                message,
                ..
            } => {
                events.push(serde_json::json!({
                    "event": "rejected",
                    "execution_id": execution_id,
                    "message": message,
                }));
                break;
            }
            DaemonMessage::ApprovalRequired { approval, .. } => {
                events.push(serde_json::json!({
                    "event": "approval_required",
                    "approval_id": approval.approval_id,
                    "risk_level": approval.risk_level,
                    "blast_radius": approval.blast_radius,
                    "reasons": approval.reasons,
                }));
                break;
            }
            DaemonMessage::Error { message } => {
                anyhow::bail!("daemon error: {message}");
            }
            DaemonMessage::GatewayBootstrap { .. }
            | DaemonMessage::GatewaySendRequest { .. }
            | DaemonMessage::GatewayReloadCommand { .. }
            | DaemonMessage::GatewayShutdownCommand { .. } => {}
            _ => {}
        }
    }

    Ok(serde_json::json!({
        "command": command,
        "session_id": session_id.to_string(),
        "events": events,
    }))
}

pub(super) async fn tool_search_history(args: &Value) -> Result<Value> {
    let query = args
        .get("query")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("missing required parameter: query"))?
        .to_string();

    let limit = args
        .get("limit")
        .and_then(|v| v.as_u64())
        .map(|v| v as usize);

    let resp = daemon_roundtrip(ClientMessage::SearchHistory { query, limit }).await?;

    match resp {
        DaemonMessage::HistorySearchResult {
            query,
            summary,
            hits,
        } => Ok(serde_json::json!({
            "query": query,
            "summary": summary,
            "hits": hits,
        })),
        DaemonMessage::Error { message } => anyhow::bail!("daemon error: {message}"),
        other => anyhow::bail!("unexpected daemon response: {other:?}"),
    }
}

pub(super) async fn tool_find_symbol(args: &Value) -> Result<Value> {
    let workspace_root = args
        .get("workspace_root")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("missing required parameter: workspace_root"))?
        .to_string();

    let symbol = args
        .get("symbol")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("missing required parameter: symbol"))?
        .to_string();

    let limit = args
        .get("limit")
        .and_then(|v| v.as_u64())
        .map(|v| v as usize);

    let resp = daemon_roundtrip(ClientMessage::FindSymbol {
        workspace_root,
        symbol,
        limit,
    })
    .await?;

    match resp {
        DaemonMessage::SymbolSearchResult { symbol, matches } => Ok(serde_json::json!({
            "symbol": symbol,
            "matches": matches,
        })),
        DaemonMessage::Error { message } => anyhow::bail!("daemon error: {message}"),
        other => anyhow::bail!("unexpected daemon response: {other:?}"),
    }
}

pub(super) async fn tool_list_snapshots(args: &Value) -> Result<Value> {
    let workspace_id = args
        .get("workspace_id")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let resp = daemon_roundtrip(ClientMessage::ListSnapshots { workspace_id }).await?;

    match resp {
        DaemonMessage::SnapshotList { snapshots } => Ok(serde_json::json!({
            "snapshots": snapshots,
        })),
        DaemonMessage::Error { message } => anyhow::bail!("daemon error: {message}"),
        other => anyhow::bail!("unexpected daemon response: {other:?}"),
    }
}

pub(super) async fn tool_restore_snapshot(args: &Value) -> Result<Value> {
    let snapshot_id = args
        .get("snapshot_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("missing required parameter: snapshot_id"))?
        .to_string();

    let resp = daemon_roundtrip(ClientMessage::RestoreSnapshot { snapshot_id }).await?;

    match resp {
        DaemonMessage::SnapshotRestored {
            snapshot_id,
            ok,
            message,
        } => Ok(serde_json::json!({
            "snapshot_id": snapshot_id,
            "ok": ok,
            "message": message,
        })),
        DaemonMessage::Error { message } => anyhow::bail!("daemon error: {message}"),
        other => anyhow::bail!("unexpected daemon response: {other:?}"),
    }
}

pub(super) async fn tool_scrub_sensitive(args: &Value) -> Result<Value> {
    let text = args
        .get("text")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("missing required parameter: text"))?
        .to_string();

    let resp = daemon_roundtrip(ClientMessage::ScrubSensitive { text }).await?;

    match resp {
        DaemonMessage::ScrubResult { text } => Ok(serde_json::json!({
            "scrubbed_text": text,
        })),
        DaemonMessage::Error { message } => anyhow::bail!("daemon error: {message}"),
        other => anyhow::bail!("unexpected daemon response: {other:?}"),
    }
}

pub(super) async fn tool_list_sessions(_args: &Value) -> Result<Value> {
    let resp = daemon_roundtrip(ClientMessage::ListSessions).await?;

    let sessions = match resp {
        DaemonMessage::SessionList { sessions } => sessions,
        DaemonMessage::Error { message } => anyhow::bail!("daemon error: {message}"),
        other => anyhow::bail!("unexpected daemon response: {other:?}"),
    };

    let topology_path = amux_protocol::ensure_amux_data_dir()
        .ok()
        .map(|dir| dir.join("workspace-topology.json"));
    let topology: Option<amux_protocol::WorkspaceTopology> = topology_path
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|data| serde_json::from_str(&data).ok());

    if let Some(topology) = topology {
        let formatted = amux_protocol::format_topology(&topology, &sessions);
        if !formatted.is_empty() {
            return Ok(serde_json::json!({ "topology": formatted }));
        }
    }

    Ok(serde_json::json!({ "sessions": sessions }))
}

pub(super) async fn tool_verify_integrity(_args: &Value) -> Result<Value> {
    let resp = daemon_roundtrip(ClientMessage::VerifyTelemetryIntegrity).await?;

    match resp {
        DaemonMessage::TelemetryIntegrityResult { results } => Ok(serde_json::json!({
            "results": results,
        })),
        DaemonMessage::Error { message } => anyhow::bail!("daemon error: {message}"),
        other => anyhow::bail!("unexpected daemon response: {other:?}"),
    }
}

pub(super) async fn tool_get_terminal_content(args: &Value) -> Result<Value> {
    let session_id: uuid::Uuid = args
        .get("session_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("missing required parameter: session_id"))?
        .parse()
        .context("invalid session_id UUID")?;

    let max_lines = args
        .get("max_lines")
        .and_then(|v| v.as_u64())
        .map(|v| v as usize);

    let resp = daemon_roundtrip(ClientMessage::GetScrollback {
        id: session_id,
        max_lines: Some(max_lines.unwrap_or(100)),
    })
    .await?;

    match resp {
        DaemonMessage::Scrollback { id, data } => {
            let text = String::from_utf8_lossy(&data);
            let clean = strip_ansi_basic(&text);
            Ok(serde_json::json!({
                "session_id": id.to_string(),
                "content": clean,
                "lines": clean.lines().count(),
            }))
        }
        DaemonMessage::Error { message } => anyhow::bail!("daemon error: {message}"),
        other => anyhow::bail!("unexpected daemon response: {other:?}"),
    }
}

pub(super) async fn tool_type_in_terminal(args: &Value) -> Result<Value> {
    let session_id: uuid::Uuid = args
        .get("session_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("missing required parameter: session_id"))?
        .parse()
        .context("invalid session_id UUID")?;

    let input = args
        .get("input")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("missing required parameter: input"))?;

    let data = input
        .replace("\\n", "\n")
        .replace("\\r", "\r")
        .replace("\\t", "\t")
        .into_bytes();

    let mut framed = connect_daemon().await?;
    framed
        .send(ClientMessage::Input {
            id: session_id,
            data: data.clone(),
        })
        .await
        .context("failed to send input to daemon")?;

    match timeout(Duration::from_millis(250), framed.next()).await {
        Ok(Some(Ok(DaemonMessage::Error { message }))) => {
            anyhow::bail!("daemon error: {message}");
        }
        Ok(Some(Err(e))) => {
            return Err(e.into());
        }
        Ok(None) => {
            anyhow::bail!("daemon connection closed while sending input");
        }
        Ok(Some(Ok(_))) | Err(_) => {}
    }

    Ok(serde_json::json!({
        "session_id": session_id.to_string(),
        "bytes_sent": data.len(),
        "status": "ok",
    }))
}

pub(super) async fn tool_get_git_status(args: &Value) -> Result<Value> {
    let path = args
        .get("path")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("missing required parameter: path"))?
        .to_string();

    let resp = daemon_roundtrip(ClientMessage::GetGitStatus { path }).await?;

    match resp {
        DaemonMessage::GitStatus {
            path: repo_path,
            info,
        } => Ok(serde_json::json!({
            "path": repo_path,
            "branch": info.branch,
            "is_dirty": info.is_dirty,
            "ahead": info.ahead,
            "behind": info.behind,
            "untracked": info.untracked,
            "modified": info.modified,
            "staged": info.staged,
        })),
        DaemonMessage::Error { message } => anyhow::bail!("daemon error: {message}"),
        other => anyhow::bail!("unexpected daemon response: {other:?}"),
    }
}
