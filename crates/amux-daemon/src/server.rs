use std::collections::HashSet;
use std::path::Path;
use std::sync::Arc;

use amux_protocol::{ClientMessage, DaemonMessage};
use anyhow::{Context, Result};
use futures::SinkExt;
use futures::StreamExt;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::sync::broadcast;
use tokio_util::codec::Framed;

use crate::agent::AgentEngine;
use crate::agent::skill_community::{export_skill, import_community_skill, prepare_publish, unpack_skill, ImportResult};
use crate::agent::skill_registry::{to_community_entry, RegistryClient};
use crate::session_manager::SessionManager;

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
        | AgentEvent::TodoUpdate { thread_id, .. }
        | AgentEvent::WorkContextUpdate { thread_id, .. }
        | AgentEvent::WorkflowNotice { thread_id, .. } => Some(thread_id.as_str()),
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

fn should_forward_agent_event(
    event: &crate::agent::types::AgentEvent,
    client_threads: &HashSet<String>,
) -> bool {
    match agent_event_thread_id(event) {
        Some(thread_id) => client_threads.contains(thread_id),
        None => matches!(
            event,
            crate::agent::types::AgentEvent::HeartbeatResult { .. }
                | crate::agent::types::AgentEvent::HeartbeatDigest { .. }
                | crate::agent::types::AgentEvent::Notification { .. }
                | crate::agent::types::AgentEvent::AnticipatoryUpdate { .. }
                | crate::agent::types::AgentEvent::ConciergeWelcome { .. }
                | crate::agent::types::AgentEvent::ProviderCircuitOpen { .. }
                | crate::agent::types::AgentEvent::ProviderCircuitRecovered { .. }
                | crate::agent::types::AgentEvent::AuditAction { .. }
                | crate::agent::types::AgentEvent::EscalationUpdate { .. }
                | crate::agent::types::AgentEvent::GatewayStatus { .. }
        ),
    }
}

/// Socket path / pipe name for IPC.
#[cfg(unix)]
pub fn socket_path() -> std::path::PathBuf {
    let runtime_dir = std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/tmp".to_string());
    std::path::PathBuf::from(runtime_dir).join("tamux-daemon.sock")
}

/// Run the IPC server until a shutdown signal is received.
pub async fn run() -> Result<()> {
    // Create shared history store (single connection for entire daemon)
    let history = crate::history::HistoryStore::new()
        .await
        .context("failed to initialize shared history store")?;
    let history = Arc::new(history);

    // load_config now takes &HistoryStore
    let agent_config = crate::agent::load_config_from_history(&history)
        .await
        .unwrap_or_default();

    let manager = SessionManager::new_with_history(
        history.clone(),
        agent_config.pty_channel_capacity,
    );
    let reaper_manager = manager.clone();

    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(5));
        loop {
            interval.tick().await;
            reaper_manager.reap_dead().await;
        }
    });

    // Start agent engine
    let agent = AgentEngine::new_with_shared_history(
        manager.clone(),
        agent_config,
        history.clone(),
    );

    // Initialize plugin manager
    let plugins_dir = dirs::home_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join(".tamux")
        .join("plugins");
    let plugin_manager = Arc::new(crate::plugin::PluginManager::new(history.clone(), plugins_dir));
    let (pm_loaded, pm_skipped) = plugin_manager.load_all_from_disk().await;
    tracing::info!(loaded = pm_loaded, skipped = pm_skipped, "plugin loader complete");

    // Wire plugin manager into agent engine for tool executor access (Phase 17)
    let _ = agent.plugin_manager.set(plugin_manager.clone());

    // Hydrate persisted state (threads, tasks, heartbeat, memory)
    if let Err(e) = agent.hydrate().await {
        tracing::warn!("failed to hydrate agent engine: {e}");
    }

    // Initialize the concierge (ensures pinned thread exists after hydration).
    agent.concierge.initialize(&agent.threads).await;

    // Start background loop (tasks + heartbeat)
    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
    let loop_agent = agent.clone();
    tokio::spawn(async move {
        loop_agent.run_loop(shutdown_rx).await;
    });

    #[cfg(unix)]
    let result = run_unix(manager, agent.clone(), plugin_manager.clone()).await;

    #[cfg(windows)]
    let result = run_windows(manager, agent.clone(), plugin_manager.clone()).await;

    // Signal agent loop shutdown
    let _ = shutdown_tx.send(true);

    result
}

// ---------------------------------------------------------------------------
// Unix Domain Socket implementation
// ---------------------------------------------------------------------------

#[cfg(unix)]
async fn run_unix(manager: Arc<SessionManager>, agent: Arc<AgentEngine>, plugin_manager: Arc<crate::plugin::PluginManager>) -> Result<()> {
    use tokio::net::UnixListener;

    let path = socket_path();

    // Remove stale socket file.
    let _ = std::fs::remove_file(&path);

    let listener = UnixListener::bind(&path)?;
    tracing::info!(?path, "daemon listening on Unix socket");

    // Graceful shutdown on SIGINT / SIGTERM.
    let shutdown = async {
        tokio::signal::ctrl_c().await.ok();
        tracing::info!("shutdown signal received");
    };

    tokio::select! {
        _ = accept_loop_unix(listener, manager, agent, plugin_manager) => {}
        _ = shutdown => {}
    }

    let _ = std::fs::remove_file(&path);
    tracing::info!("daemon shut down");
    Ok(())
}

#[cfg(unix)]
async fn accept_loop_unix(
    listener: tokio::net::UnixListener,
    manager: Arc<SessionManager>,
    agent: Arc<AgentEngine>,
    plugin_manager: Arc<crate::plugin::PluginManager>,
) {
    loop {
        match listener.accept().await {
            Ok((stream, _addr)) => {
                let manager = manager.clone();
                let agent = agent.clone();
                let plugin_manager = plugin_manager.clone();
                tokio::spawn(async move {
                    if let Err(e) = handle_connection(stream, manager, agent, plugin_manager).await {
                        tracing::error!(error = %e, "client connection error");
                    }
                });
            }
            Err(e) => {
                tracing::error!(error = %e, "accept error");
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Windows IPC implementation
// ---------------------------------------------------------------------------

#[cfg(windows)]
async fn run_windows(manager: Arc<SessionManager>, agent: Arc<AgentEngine>, plugin_manager: Arc<crate::plugin::PluginManager>) -> Result<()> {
    let addr = amux_protocol::default_tcp_addr();
    tracing::info!(%addr, "daemon listening on TCP");
    run_tcp_fallback(manager, agent, plugin_manager).await
}

/// TCP server used for Windows IPC.
#[allow(dead_code)]
async fn run_tcp_fallback(manager: Arc<SessionManager>, agent: Arc<AgentEngine>, plugin_manager: Arc<crate::plugin::PluginManager>) -> Result<()> {
    use tokio::net::TcpListener;

    let addr = amux_protocol::default_tcp_addr();
    let listener = TcpListener::bind(&addr).await.map_err(|error| {
        if error.kind() == std::io::ErrorKind::AddrInUse {
            anyhow::anyhow!(
                "daemon is already running on {addr}; stop the existing process before starting another instance"
            )
        } else {
            anyhow::Error::new(error).context(format!("failed to bind daemon TCP listener on {addr}"))
        }
    })?;
    tracing::info!(%addr, "daemon ready on TCP");

    let shutdown = async {
        tokio::signal::ctrl_c().await.ok();
        tracing::info!("shutdown signal received");
    };

    tokio::select! {
        _ = accept_loop_tcp(listener, manager, agent, plugin_manager) => {}
        _ = shutdown => {}
    }

    tracing::info!("daemon shut down");
    Ok(())
}

#[allow(dead_code)]
async fn accept_loop_tcp(
    listener: tokio::net::TcpListener,
    manager: Arc<SessionManager>,
    agent: Arc<AgentEngine>,
    plugin_manager: Arc<crate::plugin::PluginManager>,
) {
    loop {
        match listener.accept().await {
            Ok((stream, addr)) => {
                tracing::info!(%addr, "client connected");
                let manager = manager.clone();
                let agent = agent.clone();
                let plugin_manager = plugin_manager.clone();
                tokio::spawn(async move {
                    if let Err(e) = handle_connection(stream, manager, agent, plugin_manager).await {
                        tracing::error!(%addr, error = %e, "client connection error");
                    }
                });
            }
            Err(e) => {
                tracing::error!(error = %e, "accept error");
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Connection handler — generic over any AsyncRead + AsyncWrite stream
// ---------------------------------------------------------------------------

async fn handle_connection<S>(
    stream: S,
    manager: Arc<SessionManager>,
    agent: Arc<AgentEngine>,
    plugin_manager: Arc<crate::plugin::PluginManager>,
) -> Result<()>
where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    use amux_protocol::DaemonCodec;
    let mut framed = Framed::new(stream, DaemonCodec);

    // Track which sessions this client is attached to so we can fan-out output.
    let mut attached_rxs: Vec<(amux_protocol::SessionId, broadcast::Receiver<DaemonMessage>)> =
        Vec::new();
    let mut client_agent_threads: HashSet<String> = HashSet::new();

    // Optional agent event subscription.
    let mut agent_event_rx: Option<broadcast::Receiver<crate::agent::types::AgentEvent>> = None;

    loop {
        // Drain agent events if subscribed.
        if let Some(ref mut rx) = agent_event_rx {
            loop {
                match rx.try_recv() {
                    Ok(event) => {
                        if should_forward_agent_event(&event, &client_agent_threads) {
                            if let Ok(json) = serde_json::to_string(&event) {
                                framed
                                    .send(DaemonMessage::AgentEvent { event_json: json })
                                    .await?;
                            }
                        }
                    }
                    Err(broadcast::error::TryRecvError::Lagged(n)) => {
                        tracing::warn!(skipped = n, "agent event broadcast lagged");
                        break;
                    }
                    _ => break,
                }
            }
        }

        // We need to select between: incoming client messages and output from attached sessions.
        let has_subscriptions = !attached_rxs.is_empty() || agent_event_rx.is_some();
        let msg = if !has_subscriptions {
            // No attached sessions or agent subscription — just wait for client input.
            match framed.next().await {
                Some(Ok(msg)) => Some(msg),
                Some(Err(e)) => return Err(e.into()),
                None => return Ok(()), // client disconnected
            }
        } else {
            // Select between client input and all attached session outputs.
            // For simplicity we drain all pending broadcast messages first.
            let mut forwarded = false;
            let mut closed_sessions = Vec::new();
            for (sid, rx) in attached_rxs.iter_mut() {
                loop {
                    match rx.try_recv() {
                        Ok(daemon_msg) => {
                            framed.send(daemon_msg).await?;
                            forwarded = true;
                        }
                        Err(broadcast::error::TryRecvError::Empty) => break,
                        Err(broadcast::error::TryRecvError::Lagged(n)) => {
                            tracing::warn!(session = %sid, skipped = n, "broadcast lagged");
                            break;
                        }
                        Err(broadcast::error::TryRecvError::Closed) => {
                            framed
                                .send(DaemonMessage::SessionExited {
                                    id: *sid,
                                    exit_code: None,
                                })
                                .await?;
                            closed_sessions.push(*sid);
                            forwarded = true;
                            break;
                        }
                    }
                }
            }
            if !closed_sessions.is_empty() {
                attached_rxs.retain(|(sid, _)| !closed_sessions.contains(sid));
            }

            // Now try to read one client message with a short timeout so we
            // keep draining output.
            match tokio::time::timeout(
                std::time::Duration::from_millis(if forwarded { 10 } else { 50 }),
                framed.next(),
            )
            .await
            {
                Ok(Some(Ok(msg))) => Some(msg),
                Ok(Some(Err(e))) => return Err(e.into()),
                Ok(None) => return Ok(()),
                Err(_) => None, // timeout — loop back to drain output
            }
        };

        if let Some(msg) = msg {
            match msg {
                ClientMessage::Ping => {
                    framed.send(DaemonMessage::Pong).await?;
                }

                ClientMessage::SpawnSession {
                    shell,
                    cwd,
                    env,
                    workspace_id,
                    cols,
                    rows,
                } => {
                    match manager
                        .spawn(shell, cwd, workspace_id, env, cols, rows)
                        .await
                    {
                        Ok((id, rx)) => {
                            attached_rxs.push((id, rx));
                            framed.send(DaemonMessage::SessionSpawned { id }).await?;
                        }
                        Err(e) => {
                            framed
                                .send(DaemonMessage::Error {
                                    message: e.to_string(),
                                })
                                .await?;
                        }
                    }
                }

                ClientMessage::CloneSession {
                    source_id,
                    workspace_id,
                    cols,
                    rows,
                    replay_scrollback,
                    cwd,
                } => {
                    match manager
                        .clone_session(source_id, workspace_id, cols, rows, replay_scrollback, cwd)
                        .await
                    {
                        Ok((id, rx, active_command)) => {
                            attached_rxs.push((id, rx));
                            framed
                                .send(DaemonMessage::SessionCloned {
                                    source_id,
                                    id,
                                    active_command,
                                })
                                .await?;
                        }
                        Err(e) => {
                            framed
                                .send(DaemonMessage::Error {
                                    message: e.to_string(),
                                })
                                .await?;
                        }
                    }
                }

                ClientMessage::AttachSession { id } => match manager.subscribe(id).await {
                    Ok((rx, alive)) => {
                        attached_rxs.push((id, rx));
                        framed.send(DaemonMessage::SessionAttached { id }).await?;
                        if !alive {
                            framed
                                .send(DaemonMessage::SessionExited {
                                    id,
                                    exit_code: None,
                                })
                                .await?;
                        }
                    }
                    Err(e) => {
                        framed
                            .send(DaemonMessage::Error {
                                message: e.to_string(),
                            })
                            .await?;
                    }
                },

                ClientMessage::DetachSession { id } => {
                    attached_rxs.retain(|(sid, _)| *sid != id);
                    framed.send(DaemonMessage::SessionDetached { id }).await?;
                }

                ClientMessage::KillSession { id } => {
                    attached_rxs.retain(|(sid, _)| *sid != id);
                    match manager.kill(id).await {
                        Ok(()) => {
                            framed.send(DaemonMessage::SessionKilled { id }).await?;
                        }
                        Err(e) => {
                            framed
                                .send(DaemonMessage::Error {
                                    message: e.to_string(),
                                })
                                .await?;
                        }
                    }
                }

                ClientMessage::Input { id, data } => {
                    if let Err(e) = manager.write_input(id, &data).await {
                        framed
                            .send(DaemonMessage::Error {
                                message: e.to_string(),
                            })
                            .await?;
                    }
                }

                ClientMessage::ExecuteManagedCommand { id, request } => {
                    match manager.execute_managed_command(id, request).await {
                        Ok(message) => {
                            if let DaemonMessage::ApprovalRequired { approval, .. } = &message {
                                let pending = crate::agent::types::ToolPendingApproval {
                                    approval_id: approval.approval_id.clone(),
                                    execution_id: approval.execution_id.clone(),
                                    command: approval.command.clone(),
                                    rationale: approval.rationale.clone(),
                                    risk_level: approval.risk_level.clone(),
                                    blast_radius: approval.blast_radius.clone(),
                                    reasons: approval.reasons.clone(),
                                    session_id: Some(id.to_string()),
                                };
                                if let Err(error) =
                                    agent.record_operator_approval_requested(&pending).await
                                {
                                    tracing::warn!(
                                        approval_id = %approval.approval_id,
                                        "failed to record operator approval request: {error}"
                                    );
                                }
                                agent
                                    .record_provenance_event(
                                        "approval_requested",
                                        "managed command requested approval",
                                        serde_json::json!({
                                            "approval_id": approval.approval_id,
                                            "session_id": id.to_string(),
                                            "command": approval.command,
                                            "risk_level": approval.risk_level,
                                            "blast_radius": approval.blast_radius,
                                        }),
                                        None,
                                        None,
                                        None,
                                        Some(approval.approval_id.as_str()),
                                        None,
                                    )
                                    .await;
                            }
                            framed.send(message).await?;
                        }
                        Err(e) => {
                            framed
                                .send(DaemonMessage::Error {
                                    message: e.to_string(),
                                })
                                .await?;
                        }
                    }
                }

                ClientMessage::ResolveApproval {
                    id,
                    approval_id,
                    decision,
                } => match manager.resolve_approval(id, &approval_id, decision).await {
                    Ok(messages) => {
                        let _ = agent
                            .record_operator_approval_resolution(&approval_id, decision)
                            .await;
                        let _ = agent
                            .handle_task_approval_resolution(&approval_id, decision)
                            .await;
                        agent
                            .record_provenance_event(
                                match decision {
                                    amux_protocol::ApprovalDecision::ApproveOnce
                                    | amux_protocol::ApprovalDecision::ApproveSession => {
                                        "approval_granted"
                                    }
                                    amux_protocol::ApprovalDecision::Deny => "approval_denied",
                                },
                                "operator resolved approval request",
                                serde_json::json!({
                                    "approval_id": approval_id,
                                    "session_id": id.to_string(),
                                    "decision": format!("{decision:?}").to_lowercase(),
                                }),
                                None,
                                None,
                                None,
                                Some(approval_id.as_str()),
                                None,
                            )
                            .await;
                        for message in messages {
                            framed.send(message).await?;
                        }
                    }
                    Err(e) => {
                        framed
                            .send(DaemonMessage::Error {
                                message: e.to_string(),
                            })
                            .await?;
                    }
                },

                ClientMessage::Resize { id, cols, rows } => {
                    if let Err(e) = manager.resize(id, cols, rows).await {
                        framed
                            .send(DaemonMessage::Error {
                                message: e.to_string(),
                            })
                            .await?;
                    }
                }

                ClientMessage::ListSessions => {
                    let sessions = manager.list().await;
                    framed.send(DaemonMessage::SessionList { sessions }).await?;
                }

                ClientMessage::GetScrollback { id, max_lines } => {
                    match manager.get_scrollback(id, max_lines).await {
                        Ok(data) => {
                            framed.send(DaemonMessage::Scrollback { id, data }).await?;
                        }
                        Err(e) => {
                            framed
                                .send(DaemonMessage::Error {
                                    message: e.to_string(),
                                })
                                .await?;
                        }
                    }
                }

                ClientMessage::AnalyzeSession { id, max_lines } => {
                    match manager.get_analysis_text(id, max_lines).await {
                        Ok(text) => {
                            // TODO: Send to AI model. For now, return the raw text.
                            framed
                                .send(DaemonMessage::AnalysisResult { id, result: text })
                                .await?;
                        }
                        Err(e) => {
                            framed
                                .send(DaemonMessage::Error {
                                    message: e.to_string(),
                                })
                                .await?;
                        }
                    }
                }

                ClientMessage::SearchHistory { query, limit } => {
                    match manager.search_history(&query, limit.unwrap_or(8).max(1)).await {
                        Ok((summary, hits)) => {
                            framed
                                .send(DaemonMessage::HistorySearchResult {
                                    query,
                                    summary,
                                    hits,
                                })
                                .await?;
                        }
                        Err(e) => {
                            framed
                                .send(DaemonMessage::Error {
                                    message: e.to_string(),
                                })
                                .await?;
                        }
                    }
                }

                ClientMessage::AppendCommandLog { entry_json } => {
                    match serde_json::from_str::<amux_protocol::CommandLogEntry>(&entry_json) {
                        Ok(entry) => match manager.append_command_log(&entry).await {
                            Ok(()) => {
                                framed.send(DaemonMessage::CommandLogAck).await?;
                            }
                            Err(e) => {
                                framed
                                    .send(DaemonMessage::Error {
                                        message: e.to_string(),
                                    })
                                    .await?;
                            }
                        },
                        Err(e) => {
                            framed
                                .send(DaemonMessage::Error {
                                    message: format!("invalid command log payload: {e}"),
                                })
                                .await?;
                        }
                    }
                }

                ClientMessage::CompleteCommandLog {
                    id,
                    exit_code,
                    duration_ms,
                } => match manager.complete_command_log(&id, exit_code, duration_ms).await {
                    Ok(()) => {
                        framed.send(DaemonMessage::CommandLogAck).await?;
                    }
                    Err(e) => {
                        framed
                            .send(DaemonMessage::Error {
                                message: e.to_string(),
                            })
                            .await?;
                    }
                },

                ClientMessage::QueryCommandLog {
                    workspace_id,
                    pane_id,
                    limit,
                } => match manager.query_command_log(
                    workspace_id.as_deref(),
                    pane_id.as_deref(),
                    limit,
                ).await {
                    Ok(entries) => {
                        let entries_json = serde_json::to_string(&entries).unwrap_or_default();
                        framed
                            .send(DaemonMessage::CommandLogEntries { entries_json })
                            .await?;
                    }
                    Err(e) => {
                        framed
                            .send(DaemonMessage::Error {
                                message: e.to_string(),
                            })
                            .await?;
                    }
                },

                ClientMessage::ClearCommandLog => match manager.clear_command_log().await {
                    Ok(()) => {
                        framed.send(DaemonMessage::CommandLogAck).await?;
                    }
                    Err(e) => {
                        framed
                            .send(DaemonMessage::Error {
                                message: e.to_string(),
                            })
                            .await?;
                    }
                },

                ClientMessage::CreateAgentThread { thread_json } => {
                    match serde_json::from_str::<amux_protocol::AgentDbThread>(&thread_json) {
                        Ok(thread) => match manager.create_agent_thread(&thread).await {
                            Ok(()) => {
                                framed.send(DaemonMessage::AgentDbMessageAck).await?;
                            }
                            Err(e) => {
                                framed
                                    .send(DaemonMessage::Error {
                                        message: e.to_string(),
                                    })
                                    .await?;
                            }
                        },
                        Err(e) => {
                            framed
                                .send(DaemonMessage::Error {
                                    message: format!("invalid agent thread payload: {e}"),
                                })
                                .await?;
                        }
                    }
                }

                ClientMessage::DeleteAgentThread { thread_id } => {
                    match manager.delete_agent_thread(&thread_id).await {
                        Ok(()) => {
                            framed.send(DaemonMessage::AgentDbMessageAck).await?;
                        }
                        Err(e) => {
                            framed
                                .send(DaemonMessage::Error {
                                    message: e.to_string(),
                                })
                                .await?;
                        }
                    }
                }

                ClientMessage::ListAgentThreads => match manager.list_agent_threads().await {
                    Ok(threads) => {
                        let threads_json = serde_json::to_string(&threads).unwrap_or_default();
                        framed
                            .send(DaemonMessage::AgentDbThreadList { threads_json })
                            .await?;
                    }
                    Err(e) => {
                        framed
                            .send(DaemonMessage::Error {
                                message: e.to_string(),
                            })
                            .await?;
                    }
                },

                ClientMessage::GetAgentThread { thread_id } => {
                    match manager.get_agent_thread(&thread_id).await {
                        Ok(thread) => {
                            let messages = manager.list_agent_messages(&thread_id, None).await?;
                            let thread_json = serde_json::to_string(&thread).unwrap_or_default();
                            let messages_json =
                                serde_json::to_string(&messages).unwrap_or_default();
                            framed
                                .send(DaemonMessage::AgentDbThreadDetail {
                                    thread_json,
                                    messages_json,
                                })
                                .await?;
                        }
                        Err(e) => {
                            framed
                                .send(DaemonMessage::Error {
                                    message: e.to_string(),
                                })
                                .await?;
                        }
                    }
                }

                ClientMessage::AddAgentMessage { message_json } => {
                    match serde_json::from_str::<amux_protocol::AgentDbMessage>(&message_json) {
                        Ok(message) => match manager.add_agent_message(&message).await {
                            Ok(()) => {
                                framed.send(DaemonMessage::AgentDbMessageAck).await?;
                            }
                            Err(e) => {
                                framed
                                    .send(DaemonMessage::Error {
                                        message: e.to_string(),
                                    })
                                    .await?;
                            }
                        },
                        Err(e) => {
                            framed
                                .send(DaemonMessage::Error {
                                    message: format!("invalid agent message payload: {e}"),
                                })
                                .await?;
                        }
                    }
                }

                ClientMessage::DeleteAgentMessages {
                    thread_id,
                    message_ids,
                } => {
                    match agent.delete_thread_messages(&thread_id, &message_ids).await {
                        Ok(deleted) => {
                            tracing::info!(
                                thread_id = %thread_id,
                                deleted,
                                "deleted agent messages"
                            );
                            framed.send(DaemonMessage::AgentDbMessageAck).await?;
                        }
                        Err(e) => {
                            framed
                                .send(DaemonMessage::Error {
                                    message: e.to_string(),
                                })
                                .await?;
                        }
                    }
                }

                ClientMessage::ListAgentMessages { thread_id, limit } => {
                    match manager.list_agent_messages(&thread_id, limit).await {
                        Ok(messages) => {
                            let thread = manager.get_agent_thread(&thread_id).await?;
                            let thread_json = serde_json::to_string(&thread).unwrap_or_default();
                            let messages_json =
                                serde_json::to_string(&messages).unwrap_or_default();
                            framed
                                .send(DaemonMessage::AgentDbThreadDetail {
                                    thread_json,
                                    messages_json,
                                })
                                .await?;
                        }
                        Err(e) => {
                            framed
                                .send(DaemonMessage::Error {
                                    message: e.to_string(),
                                })
                                .await?;
                        }
                    }
                }

                ClientMessage::UpsertTranscriptIndex { entry_json } => {
                    match serde_json::from_str::<amux_protocol::TranscriptIndexEntry>(&entry_json) {
                        Ok(entry) => match manager.upsert_transcript_index(&entry).await {
                            Ok(()) => {
                                framed.send(DaemonMessage::AgentDbMessageAck).await?;
                            }
                            Err(e) => {
                                framed
                                    .send(DaemonMessage::Error {
                                        message: e.to_string(),
                                    })
                                    .await?;
                            }
                        },
                        Err(e) => {
                            framed
                                .send(DaemonMessage::Error {
                                    message: format!("invalid transcript index payload: {e}"),
                                })
                                .await?;
                        }
                    }
                }

                ClientMessage::ListTranscriptIndex { workspace_id } => {
                    match manager.list_transcript_index(workspace_id.as_deref()).await {
                        Ok(entries) => {
                            let entries_json = serde_json::to_string(&entries).unwrap_or_default();
                            framed
                                .send(DaemonMessage::TranscriptIndexEntries { entries_json })
                                .await?;
                        }
                        Err(e) => {
                            framed
                                .send(DaemonMessage::Error {
                                    message: e.to_string(),
                                })
                                .await?;
                        }
                    }
                }

                ClientMessage::UpsertSnapshotIndex { entry_json } => {
                    match serde_json::from_str::<amux_protocol::SnapshotIndexEntry>(&entry_json) {
                        Ok(entry) => match manager.upsert_snapshot_index(&entry).await {
                            Ok(()) => {
                                framed.send(DaemonMessage::AgentDbMessageAck).await?;
                            }
                            Err(e) => {
                                framed
                                    .send(DaemonMessage::Error {
                                        message: e.to_string(),
                                    })
                                    .await?;
                            }
                        },
                        Err(e) => {
                            framed
                                .send(DaemonMessage::Error {
                                    message: format!("invalid snapshot index payload: {e}"),
                                })
                                .await?;
                        }
                    }
                }

                ClientMessage::ListSnapshotIndex { workspace_id } => {
                    match manager.list_snapshot_index(workspace_id.as_deref()).await {
                        Ok(entries) => {
                            let entries_json = serde_json::to_string(&entries).unwrap_or_default();
                            framed
                                .send(DaemonMessage::SnapshotIndexEntries { entries_json })
                                .await?;
                        }
                        Err(e) => {
                            framed
                                .send(DaemonMessage::Error {
                                    message: e.to_string(),
                                })
                                .await?;
                        }
                    }
                }

                ClientMessage::UpsertAgentEvent { event_json } => {
                    match serde_json::from_str::<amux_protocol::AgentEventRow>(&event_json) {
                        Ok(event) => match manager.upsert_agent_event(&event).await {
                            Ok(()) => {
                                framed.send(DaemonMessage::AgentDbMessageAck).await?;
                            }
                            Err(e) => {
                                framed
                                    .send(DaemonMessage::Error {
                                        message: e.to_string(),
                                    })
                                    .await?;
                            }
                        },
                        Err(e) => {
                            framed
                                .send(DaemonMessage::Error {
                                    message: format!("invalid agent event payload: {e}"),
                                })
                                .await?;
                        }
                    }
                }

                ClientMessage::ListAgentEvents {
                    category,
                    pane_id,
                    limit,
                } => {
                    match manager.list_agent_events(category.as_deref(), pane_id.as_deref(), limit)
                        .await
                    {
                        Ok(events) => {
                            let events_json = serde_json::to_string(&events).unwrap_or_default();
                            framed
                                .send(DaemonMessage::AgentEventRows { events_json })
                                .await?;
                        }
                        Err(e) => {
                            framed
                                .send(DaemonMessage::Error {
                                    message: e.to_string(),
                                })
                                .await?;
                        }
                    }
                }

                ClientMessage::GenerateSkill { query, title } => {
                    match manager.generate_skill(query.as_deref(), title.as_deref()).await {
                        Ok((title, path)) => {
                            framed
                                .send(DaemonMessage::SkillGenerated { title, path })
                                .await?;
                        }
                        Err(e) => {
                            framed
                                .send(DaemonMessage::Error {
                                    message: e.to_string(),
                                })
                                .await?;
                        }
                    }
                }

                ClientMessage::FindSymbol {
                    workspace_root,
                    symbol,
                    limit,
                } => {
                    let matches = manager.find_symbol_matches(
                        &workspace_root,
                        &symbol,
                        limit.unwrap_or(16).max(1),
                    );
                    framed
                        .send(DaemonMessage::SymbolSearchResult { symbol, matches })
                        .await?;
                }

                ClientMessage::ListSnapshots { workspace_id } => {
                    match manager.list_snapshots(workspace_id.as_deref()).await {
                        Ok(snapshots) => {
                            framed
                                .send(DaemonMessage::SnapshotList { snapshots })
                                .await?;
                        }
                        Err(e) => {
                            framed
                                .send(DaemonMessage::Error {
                                    message: e.to_string(),
                                })
                                .await?;
                        }
                    }
                }

                ClientMessage::RestoreSnapshot { snapshot_id } => {
                    match manager.restore_snapshot(&snapshot_id).await {
                        Ok((ok, message)) => {
                            framed
                                .send(DaemonMessage::SnapshotRestored {
                                    snapshot_id,
                                    ok,
                                    message,
                                })
                                .await?;
                        }
                        Err(e) => {
                            framed
                                .send(DaemonMessage::Error {
                                    message: e.to_string(),
                                })
                                .await?;
                        }
                    }
                }

                ClientMessage::ListWorkspaceSessions { workspace_id } => {
                    let sessions = manager.list_workspace(&workspace_id).await;
                    framed.send(DaemonMessage::SessionList { sessions }).await?;
                }

                ClientMessage::GetGitStatus { path } => {
                    let info = crate::git::get_git_status(&path);
                    framed.send(DaemonMessage::GitStatus { path, info }).await?;
                }

                ClientMessage::GetGitDiff {
                    repo_path,
                    file_path,
                } => {
                    let diff = crate::git::git_diff(&repo_path, file_path.as_deref());
                    framed
                        .send(DaemonMessage::GitDiff {
                            repo_path,
                            file_path,
                            diff,
                        })
                        .await?;
                }

                ClientMessage::GetFilePreview { path, max_bytes } => {
                    let (content, truncated, is_text) =
                        crate::git::read_file_preview(&path, max_bytes.unwrap_or(65_536));
                    framed
                        .send(DaemonMessage::FilePreview {
                            path,
                            content,
                            truncated,
                            is_text,
                        })
                        .await?;
                }

                ClientMessage::SubscribeNotifications => {
                    // Acknowledged. The client will receive OscNotification
                    // messages via the output broadcast channel.
                    // No explicit state change needed here.
                }

                ClientMessage::ScrubSensitive { text } => {
                    let scrubbed = crate::scrub::scrub_sensitive(&text);
                    framed
                        .send(DaemonMessage::ScrubResult { text: scrubbed })
                        .await?;
                }

                ClientMessage::CheckpointSession { id } => {
                    let dump_dir = crate::criu::dump_dir_for_session(&id.to_string())
                        .unwrap_or_else(|_| std::path::PathBuf::from("/tmp/amux-criu"));

                    if !crate::criu::is_available() {
                        framed
                            .send(DaemonMessage::SessionCheckpointed {
                                id,
                                ok: false,
                                path: None,
                                message: "CRIU is not available on this system".to_string(),
                            })
                            .await?;
                    } else {
                        // Get the PID from the session - for now report unavailable
                        // as we'd need to track the child PID in PtySession
                        framed
                            .send(DaemonMessage::SessionCheckpointed {
                                id,
                                ok: false,
                                path: Some(dump_dir.to_string_lossy().into_owned()),
                                message: "CRIU checkpoint: session PID tracking not yet integrated"
                                    .to_string(),
                            })
                            .await?;
                    }
                }

                ClientMessage::VerifyTelemetryIntegrity => {
                    match manager.verify_telemetry_integrity() {
                        Ok(results) => {
                            framed
                                .send(DaemonMessage::TelemetryIntegrityResult { results })
                                .await?;
                        }
                        Err(e) => {
                            framed
                                .send(DaemonMessage::Error {
                                    message: e.to_string(),
                                })
                                .await?;
                        }
                    }
                }

                // -----------------------------------------------------------
                // Agent engine messages
                // -----------------------------------------------------------
                ClientMessage::AgentSendMessage {
                    thread_id,
                    content,
                    session_id,
                    context_messages_json,
                } => {
                    agent.mark_operator_present("send_message").await;
                    let effective_thread_id =
                        thread_id.or_else(|| Some(format!("thread_{}", uuid::Uuid::new_v4())));
                    if let Some(thread_id) = effective_thread_id.as_ref() {
                        client_agent_threads.insert(thread_id.clone());
                    }
                    let agent = agent.clone();
                    tokio::spawn(async move {
                        let has_context = context_messages_json.is_some();
                        tracing::info!(
                            thread_id = ?effective_thread_id,
                            content_len = content.len(),
                            has_context_json = has_context,
                            "AgentSendMessage received"
                        );
                        // Seed context messages into the thread before the LLM turn
                        if let Some(ref json) = context_messages_json {
                            match serde_json::from_str::<Vec<amux_protocol::AgentDbMessage>>(json) {
                                Ok(ctx) if !ctx.is_empty() => {
                                    tracing::info!(
                                        count = ctx.len(),
                                        "seeding thread with context messages"
                                    );
                                    agent
                                        .seed_thread_context(effective_thread_id.as_deref(), &ctx)
                                        .await;
                                }
                                Ok(_) => tracing::info!("context_messages_json was empty array"),
                                Err(e) => {
                                    tracing::warn!(error = %e, json_len = json.len(), "failed to parse context_messages_json")
                                }
                            }
                        }
                        if let Err(e) = agent
                            .send_message_with_session(
                                effective_thread_id.as_deref(),
                                session_id.as_deref(),
                                &content,
                            )
                            .await
                        {
                            tracing::warn!(error = %e, "agent send_message_with_session failed");
                        }
                    });
                }

                ClientMessage::AgentStopStream { thread_id } => {
                    client_agent_threads.insert(thread_id.clone());
                    let _ = agent.stop_stream(&thread_id).await;
                }

                ClientMessage::AgentRecordAttention {
                    surface,
                    thread_id,
                    goal_run_id,
                } => {
                    if let Err(e) = agent
                        .record_operator_attention(
                            &surface,
                            thread_id.as_deref(),
                            goal_run_id.as_deref(),
                        )
                        .await
                    {
                        framed
                            .send(DaemonMessage::AgentError {
                                message: format!("failed to record attention surface: {e}"),
                            })
                            .await
                            .ok();
                    }
                }

                ClientMessage::AgentListThreads => {
                    let threads = agent.list_threads().await;
                    let json = serde_json::to_string(&threads).unwrap_or_default();
                    framed
                        .send(DaemonMessage::AgentThreadList { threads_json: json })
                        .await?;
                }

                ClientMessage::AgentGetThread { thread_id } => {
                    client_agent_threads.insert(thread_id.clone());
                    let thread = agent.get_thread(&thread_id).await;
                    let json = serde_json::to_string(&thread).unwrap_or_default();
                    framed
                        .send(DaemonMessage::AgentThreadDetail { thread_json: json })
                        .await?;
                }

                ClientMessage::AgentDeleteThread { thread_id } => {
                    client_agent_threads.remove(&thread_id);
                    agent.delete_thread(&thread_id).await;
                }

                ClientMessage::AgentAddTask {
                    title,
                    description,
                    priority,
                    command,
                    session_id,
                    scheduled_at,
                    dependencies,
                } => {
                    let task = agent
                        .enqueue_task(
                            title,
                            description,
                            &priority,
                            command,
                            session_id,
                            dependencies,
                            scheduled_at,
                            "user",
                            None,
                            None,
                            None,
                            None,
                        )
                        .await;
                    tracing::info!(task_id = %task.id, "agent task added");
                    let json = serde_json::to_string(&task).unwrap_or_default();
                    framed
                        .send(DaemonMessage::AgentTaskEnqueued { task_json: json })
                        .await?;
                }

                ClientMessage::AgentCancelTask { task_id } => {
                    let cancelled = agent.cancel_task(&task_id).await;
                    framed
                        .send(DaemonMessage::AgentTaskCancelled { task_id, cancelled })
                        .await?;
                }

                ClientMessage::AgentListTasks => {
                    let tasks = agent.list_tasks().await;
                    let json = serde_json::to_string(&tasks).unwrap_or_default();
                    framed
                        .send(DaemonMessage::AgentTaskList { tasks_json: json })
                        .await?;
                }

                ClientMessage::AgentListRuns => {
                    let runs = agent.list_runs().await;
                    let json = serde_json::to_string(&runs).unwrap_or_default();
                    framed
                        .send(DaemonMessage::AgentRunList { runs_json: json })
                        .await?;
                }

                ClientMessage::AgentGetRun { run_id } => {
                    let run = agent.get_run(&run_id).await;
                    let json = serde_json::to_string(&run).unwrap_or_default();
                    framed
                        .send(DaemonMessage::AgentRunDetail { run_json: json })
                        .await?;
                }

                ClientMessage::AgentStartGoalRun {
                    goal,
                    title,
                    thread_id,
                    session_id,
                    priority,
                    client_request_id,
                } => {
                    let goal_run = agent
                        .start_goal_run(
                            goal,
                            title,
                            thread_id,
                            session_id,
                            priority.as_deref(),
                            client_request_id,
                        )
                        .await;
                    if let Some(thread_id) = goal_run.thread_id.clone() {
                        client_agent_threads.insert(thread_id);
                    }
                    let json = serde_json::to_string(&goal_run).unwrap_or_default();
                    framed
                        .send(DaemonMessage::AgentGoalRunStarted {
                            goal_run_json: json,
                        })
                        .await?;
                }

                ClientMessage::AgentListGoalRuns => {
                    let goal_runs = agent.list_goal_runs().await;
                    let json = serde_json::to_string(&goal_runs).unwrap_or_default();
                    framed
                        .send(DaemonMessage::AgentGoalRunList {
                            goal_runs_json: json,
                        })
                        .await?;
                }

                ClientMessage::AgentGetGoalRun { goal_run_id } => {
                    let goal_run = agent.get_goal_run(&goal_run_id).await;
                    let json = serde_json::to_string(&goal_run).unwrap_or_default();
                    framed
                        .send(DaemonMessage::AgentGoalRunDetail {
                            goal_run_json: json,
                        })
                        .await?;
                }

                ClientMessage::AgentControlGoalRun {
                    goal_run_id,
                    action,
                    step_index,
                } => {
                    let ok = agent
                        .control_goal_run(&goal_run_id, &action, step_index)
                        .await;
                    framed
                        .send(DaemonMessage::AgentGoalRunControlled { goal_run_id, ok })
                        .await?;
                }

                ClientMessage::AgentListTodos => {
                    let todos = agent.list_todos().await;
                    let json = serde_json::to_string(&todos).unwrap_or_default();
                    framed
                        .send(DaemonMessage::AgentTodoList { todos_json: json })
                        .await?;
                }

                ClientMessage::AgentGetTodos { thread_id } => {
                    client_agent_threads.insert(thread_id.clone());
                    let todos = agent.get_todos(&thread_id).await;
                    let json = serde_json::to_string(&todos).unwrap_or_default();
                    framed
                        .send(DaemonMessage::AgentTodoDetail {
                            thread_id,
                            todos_json: json,
                        })
                        .await?;
                }

                ClientMessage::AgentGetWorkContext { thread_id } => {
                    client_agent_threads.insert(thread_id.clone());
                    let context = agent.get_work_context(&thread_id).await;
                    let json = serde_json::to_string(&context).unwrap_or_default();
                    framed
                        .send(DaemonMessage::AgentWorkContextDetail {
                            thread_id,
                            context_json: json,
                        })
                        .await?;
                }

                ClientMessage::AgentGetConfig => {
                    let config = agent.get_config().await;
                    let json = serde_json::to_string(&config).unwrap_or_default();
                    framed
                        .send(DaemonMessage::AgentConfigResponse { config_json: json })
                        .await?;
                }

                ClientMessage::AgentSetConfigItem {
                    key_path,
                    value_json,
                } => match agent.set_config_item_json(&key_path, &value_json).await {
                    Ok(_) => {}
                    Err(e) => {
                        tracing::warn!(error = %e, key_path, "server: AgentSetConfigItem rejected");
                        framed
                            .send(DaemonMessage::Error {
                                message: format!("Invalid config item: {e}"),
                            })
                            .await?;
                    }
                },

                ClientMessage::AgentFetchModels {
                    provider_id,
                    base_url,
                    api_key,
                } => {
                    match crate::agent::llm_client::fetch_models(&provider_id, &base_url, &api_key)
                        .await
                    {
                        Ok(models) => {
                            let json = serde_json::to_string(&models).unwrap_or_default();
                            framed
                                .send(DaemonMessage::AgentModelsResponse { models_json: json })
                                .await?;
                        }
                        Err(e) => {
                            framed
                                .send(DaemonMessage::AgentError {
                                    message: e.to_string(),
                                })
                                .await?;
                        }
                    }
                }

                ClientMessage::AgentHeartbeatGetItems => {
                    let items = agent.get_heartbeat_items().await;
                    let json = serde_json::to_string(&items).unwrap_or_default();
                    framed
                        .send(DaemonMessage::AgentHeartbeatItems { items_json: json })
                        .await?;
                }

                ClientMessage::AgentHeartbeatSetItems { items_json } => {
                    match serde_json::from_str(&items_json) {
                        Ok(items) => agent.set_heartbeat_items(items).await,
                        Err(e) => {
                            framed
                                .send(DaemonMessage::Error {
                                    message: format!("Invalid heartbeat items: {e}"),
                                })
                                .await?;
                        }
                    }
                }

                ClientMessage::AgentResolveTaskApproval {
                    approval_id,
                    decision,
                } => {
                    let decision = match decision.as_str() {
                        "approve-session" => amux_protocol::ApprovalDecision::ApproveSession,
                        "deny" | "denied" => amux_protocol::ApprovalDecision::Deny,
                        _ => amux_protocol::ApprovalDecision::ApproveOnce,
                    };
                    tracing::info!(%approval_id, ?decision, "resolving task approval");
                    let _ = agent
                        .record_operator_approval_resolution(&approval_id, decision)
                        .await;
                    agent
                        .handle_task_approval_resolution(&approval_id, decision)
                        .await;
                }

                ClientMessage::AgentSubscribe => {
                    agent_event_rx = Some(agent.subscribe());
                    tracing::info!("client subscribed to agent events");
                    agent.mark_operator_present("client_subscribe").await;
                    agent.run_anticipatory_tick().await;
                    agent.emit_anticipatory_snapshot().await;
                }

                ClientMessage::AgentUnsubscribe => {
                    agent_event_rx = None;
                    tracing::info!("client unsubscribed from agent events");
                }

                ClientMessage::AgentGetSubagentMetrics { task_id } => {
                    let metrics_json = match agent.history.get_subagent_metrics(&task_id).await {
                        Ok(Some(metrics)) => serde_json::to_string(&serde_json::json!({
                            "task_id": metrics.task_id,
                            "parent_task_id": metrics.parent_task_id,
                            "thread_id": metrics.thread_id,
                            "tool_calls_total": metrics.tool_calls_total,
                            "tool_calls_succeeded": metrics.tool_calls_succeeded,
                            "tool_calls_failed": metrics.tool_calls_failed,
                            "tokens_consumed": metrics.tokens_consumed,
                            "context_budget_tokens": metrics.context_budget_tokens,
                            "progress_rate": metrics.progress_rate,
                            "last_progress_at": metrics.last_progress_at,
                            "stuck_score": metrics.stuck_score,
                            "health_state": metrics.health_state,
                            "created_at": metrics.created_at,
                            "updated_at": metrics.updated_at,
                        }))
                        .unwrap_or_else(|_| "null".to_string()),
                        Ok(None) => "null".to_string(),
                        Err(e) => {
                            framed
                                .send(DaemonMessage::AgentError {
                                    message: format!("failed to fetch subagent metrics: {e}"),
                                })
                                .await
                                .ok();
                            continue;
                        }
                    };
                    framed
                        .send(DaemonMessage::AgentSubagentMetrics { metrics_json })
                        .await
                        .ok();
                }

                ClientMessage::AgentListCheckpoints { goal_run_id } => {
                    let checkpoints_json =
                        match agent.history.list_checkpoints_for_goal_run(&goal_run_id).await {
                            Ok(jsons) => {
                                let summaries =
                                    crate::agent::liveness::checkpoint::checkpoint_list(&jsons);
                                serde_json::to_string(&summaries).unwrap_or_else(|_| "[]".into())
                            }
                            Err(e) => {
                                framed
                                    .send(DaemonMessage::AgentError {
                                        message: format!("failed to list checkpoints: {e}"),
                                    })
                                    .await
                                    .ok();
                                continue;
                            }
                        };
                    framed
                        .send(DaemonMessage::AgentCheckpointList { checkpoints_json })
                        .await
                        .ok();
                }

                ClientMessage::AgentRestoreCheckpoint { checkpoint_id } => {
                    let outcome_json = match agent.restore_checkpoint(&checkpoint_id).await {
                        Ok(outcome) => {
                            serde_json::to_string(&outcome).unwrap_or_else(|_| "null".into())
                        }
                        Err(e) => {
                            framed
                                .send(DaemonMessage::AgentError {
                                    message: format!("failed to restore checkpoint: {e}"),
                                })
                                .await
                                .ok();
                            continue;
                        }
                    };
                    framed
                        .send(DaemonMessage::AgentCheckpointRestored { outcome_json })
                        .await
                        .ok();
                }

                ClientMessage::AgentGetHealthStatus => {
                    let status_json = agent.health_status_snapshot().await.to_string();
                    framed
                        .send(DaemonMessage::AgentHealthStatus { status_json })
                        .await
                        .ok();
                }

                ClientMessage::AgentListHealthLog { limit } => {
                    let entries_json = match agent.health_log_entries(limit.unwrap_or(50).max(1)).await {
                        Ok(entries) => {
                            serde_json::to_string(&entries).unwrap_or_else(|_| "[]".into())
                        }
                        Err(e) => {
                            framed
                                .send(DaemonMessage::AgentError {
                                    message: format!("failed to list health log: {e}"),
                                })
                                .await
                                .ok();
                            continue;
                        }
                    };
                    framed
                        .send(DaemonMessage::AgentHealthLog { entries_json })
                        .await
                        .ok();
                }

                ClientMessage::AgentGetOperatorModel => match agent.operator_model_json().await {
                    Ok(model_json) => {
                        framed
                            .send(DaemonMessage::AgentOperatorModel { model_json })
                            .await
                            .ok();
                    }
                    Err(e) => {
                        framed
                            .send(DaemonMessage::AgentError {
                                message: format!("failed to load operator model: {e}"),
                            })
                            .await
                            .ok();
                    }
                },

                ClientMessage::AgentResetOperatorModel => {
                    match agent.reset_operator_model().await {
                        Ok(()) => {
                            framed
                                .send(DaemonMessage::AgentOperatorModelReset { ok: true })
                                .await
                                .ok();
                        }
                        Err(e) => {
                            framed
                                .send(DaemonMessage::AgentError {
                                    message: format!("failed to reset operator model: {e}"),
                                })
                                .await
                                .ok();
                        }
                    }
                }

                ClientMessage::AgentGetCausalTraceReport { option_type, limit } => {
                    match agent
                        .causal_trace_report(&option_type, limit.unwrap_or(20))
                        .await
                    {
                        Ok(report) => {
                            let report_json =
                                serde_json::to_string(&report).unwrap_or_else(|_| "{}".into());
                            framed
                                .send(DaemonMessage::AgentCausalTraceReport { report_json })
                                .await
                                .ok();
                        }
                        Err(e) => {
                            framed
                                .send(DaemonMessage::AgentError {
                                    message: format!("failed to build causal trace report: {e}"),
                                })
                                .await
                                .ok();
                        }
                    }
                }

                ClientMessage::AgentGetCounterfactualReport {
                    option_type,
                    command_family,
                    limit,
                } => match agent.counterfactual_report(
                    &option_type,
                    &command_family,
                    limit.unwrap_or(20),
                ).await {
                    Ok(report) => {
                        let report_json =
                            serde_json::to_string(&report).unwrap_or_else(|_| "{}".into());
                        framed
                            .send(DaemonMessage::AgentCounterfactualReport { report_json })
                            .await
                            .ok();
                    }
                    Err(e) => {
                        framed
                            .send(DaemonMessage::AgentError {
                                message: format!("failed to build counterfactual report: {e}"),
                            })
                            .await
                            .ok();
                    }
                },

                ClientMessage::AgentGetMemoryProvenanceReport { target, limit } => {
                    match agent
                        .history
                        .memory_provenance_report(target.as_deref(), limit.unwrap_or(25) as usize)
                        .await
                    {
                        Ok(report) => {
                            let report_json =
                                serde_json::to_string(&report).unwrap_or_else(|_| "{}".into());
                            framed
                                .send(DaemonMessage::AgentMemoryProvenanceReport { report_json })
                                .await
                                .ok();
                        }
                        Err(e) => {
                            framed
                                .send(DaemonMessage::AgentError {
                                    message: format!(
                                        "failed to build memory provenance report: {e}"
                                    ),
                                })
                                .await
                                .ok();
                        }
                    }
                }

                ClientMessage::AgentGetProvenanceReport { limit } => {
                    match agent
                        .provenance_report_json(limit.unwrap_or(50) as usize)
                        .await
                    {
                        Ok(report_json) => {
                            framed
                                .send(DaemonMessage::AgentProvenanceReport { report_json })
                                .await
                                .ok();
                        }
                        Err(e) => {
                            framed
                                .send(DaemonMessage::AgentError {
                                    message: format!("failed to build provenance report: {e}"),
                                })
                                .await
                                .ok();
                        }
                    }
                }

                ClientMessage::AgentGenerateSoc2Artifact { period_days } => {
                    match agent
                        .generate_soc2_artifact(period_days.unwrap_or(30))
                        .await
                    {
                        Ok(artifact_path) => {
                            framed
                                .send(DaemonMessage::AgentSoc2Artifact { artifact_path })
                                .await
                                .ok();
                        }
                        Err(e) => {
                            framed
                                .send(DaemonMessage::AgentError {
                                    message: format!("failed to generate SOC2 artifact: {e}"),
                                })
                                .await
                                .ok();
                        }
                    }
                }

                ClientMessage::AgentGetCollaborationSessions { parent_task_id } => {
                    match agent
                        .collaboration_sessions_json(parent_task_id.as_deref())
                        .await
                    {
                        Ok(sessions) => {
                            framed
                                .send(DaemonMessage::AgentCollaborationSessions {
                                    sessions_json: sessions.to_string(),
                                })
                                .await
                                .ok();
                        }
                        Err(error) => {
                            framed
                                .send(DaemonMessage::AgentError {
                                    message: format!(
                                        "failed to read collaboration sessions: {error}"
                                    ),
                                })
                                .await
                                .ok();
                        }
                    }
                }

                ClientMessage::AgentListGeneratedTools => {
                    match agent.list_generated_tools_json().await {
                        Ok(tools_json) => {
                            framed
                                .send(DaemonMessage::AgentGeneratedTools { tools_json })
                                .await
                                .ok();
                        }
                        Err(e) => {
                            framed
                                .send(DaemonMessage::AgentError {
                                    message: format!("failed to list generated tools: {e}"),
                                })
                                .await
                                .ok();
                        }
                    }
                }

                ClientMessage::AgentSynthesizeTool { request_json } => {
                    match agent.synthesize_tool_json(&request_json).await {
                        Ok(result_json) => {
                            framed
                                .send(DaemonMessage::AgentGeneratedToolResult {
                                    tool_name: None,
                                    result_json,
                                })
                                .await
                                .ok();
                        }
                        Err(e) => {
                            framed
                                .send(DaemonMessage::AgentError {
                                    message: format!("failed to synthesize generated tool: {e}"),
                                })
                                .await
                                .ok();
                        }
                    }
                }

                ClientMessage::AgentRunGeneratedTool {
                    tool_name,
                    args_json,
                } => match agent
                    .run_generated_tool_json(&tool_name, &args_json, None)
                    .await
                {
                    Ok(result_json) => {
                        framed
                            .send(DaemonMessage::AgentGeneratedToolResult {
                                tool_name: Some(tool_name),
                                result_json,
                            })
                            .await
                            .ok();
                    }
                    Err(e) => {
                        framed
                            .send(DaemonMessage::AgentError {
                                message: format!("failed to run generated tool: {e}"),
                            })
                            .await
                            .ok();
                    }
                },

                ClientMessage::AgentPromoteGeneratedTool { tool_name } => {
                    match agent.promote_generated_tool_json(&tool_name).await {
                        Ok(result_json) => {
                            framed
                                .send(DaemonMessage::AgentGeneratedToolResult {
                                    tool_name: Some(tool_name),
                                    result_json,
                                })
                                .await
                                .ok();
                        }
                        Err(e) => {
                            framed
                                .send(DaemonMessage::AgentError {
                                    message: format!("failed to promote generated tool: {e}"),
                                })
                                .await
                                .ok();
                        }
                    }
                }

                ClientMessage::AgentActivateGeneratedTool { tool_name } => {
                    match agent.activate_generated_tool_json(&tool_name).await {
                        Ok(result_json) => {
                            framed
                                .send(DaemonMessage::AgentGeneratedToolResult {
                                    tool_name: Some(tool_name),
                                    result_json,
                                })
                                .await
                                .ok();
                        }
                        Err(e) => {
                            framed
                                .send(DaemonMessage::AgentError {
                                    message: format!("failed to activate generated tool: {e}"),
                                })
                                .await
                                .ok();
                        }
                    }
                }

                ClientMessage::AgentGetProviderAuthStates => {
                    let states = agent.get_provider_auth_states().await;
                    let json = serde_json::to_string(&states).unwrap_or_default();
                    framed
                        .send(DaemonMessage::AgentProviderAuthStates { states_json: json })
                        .await?;
                }

                ClientMessage::AgentLoginProvider {
                    provider_id,
                    api_key,
                    base_url,
                } => {
                    // Surgical update: modify only the target provider's key.
                    let mut config = agent.get_config().await;
                    let entry = config
                        .providers
                        .entry(provider_id.clone())
                        .or_insert_with(|| {
                            let def = crate::agent::types::get_provider_definition(&provider_id);
                            crate::agent::types::ProviderConfig {
                                base_url: if base_url.is_empty() {
                                    def.map(|d| d.default_base_url.to_string())
                                        .unwrap_or_default()
                                } else {
                                    base_url.clone()
                                },
                                model: def.map(|d| d.default_model.to_string()).unwrap_or_default(),
                                api_key: String::new(),
                                assistant_id: String::new(),
                                auth_source: crate::agent::types::AuthSource::ApiKey,
                                api_transport:
                                    crate::agent::types::default_api_transport_for_provider(
                                        &provider_id,
                                    ),
                                reasoning_effort: "high".into(),
                                context_window_tokens: 128_000,
                                response_schema: None,
                            }
                        });
                    entry.api_key = api_key;
                    if !base_url.is_empty() {
                        entry.base_url = base_url;
                    }
                    agent.set_config(config).await;

                    let states = agent.get_provider_auth_states().await;
                    let json = serde_json::to_string(&states).unwrap_or_default();
                    framed
                        .send(DaemonMessage::AgentProviderAuthStates { states_json: json })
                        .await?;
                }

                ClientMessage::AgentLogoutProvider { provider_id } => {
                    let mut config = agent.get_config().await;
                    if let Some(entry) = config.providers.get_mut(&provider_id) {
                        entry.api_key.clear();
                    }
                    if config.provider == provider_id {
                        config.api_key.clear();
                    }
                    agent.set_config(config).await;

                    let states = agent.get_provider_auth_states().await;
                    let json = serde_json::to_string(&states).unwrap_or_default();
                    framed
                        .send(DaemonMessage::AgentProviderAuthStates { states_json: json })
                        .await?;
                }

                ClientMessage::AgentValidateProvider {
                    provider_id,
                    base_url,
                    api_key,
                    auth_source,
                } => {
                    // Resolve credentials: if the client didn't provide them,
                    // look up stored credentials from the agent config.
                    let (resolved_url, resolved_key) = {
                        let config = agent.config.read().await;
                        let url = if base_url.is_empty() {
                            config
                                .providers
                                .get(&provider_id)
                                .map(|pc| pc.base_url.clone())
                                .filter(|u| !u.is_empty())
                                .or_else(|| {
                                    if config.provider == provider_id {
                                        Some(config.base_url.clone())
                                    } else {
                                        crate::agent::types::get_provider_definition(&provider_id)
                                            .map(|d| d.default_base_url.to_string())
                                    }
                                })
                                .unwrap_or_default()
                        } else {
                            base_url
                        };
                        let key = if api_key.is_empty() {
                            config
                                .providers
                                .get(&provider_id)
                                .map(|pc| pc.api_key.clone())
                                .filter(|k| !k.is_empty())
                                .or_else(|| {
                                    if config.provider == provider_id {
                                        Some(config.api_key.clone())
                                    } else {
                                        None
                                    }
                                })
                                .unwrap_or_default()
                        } else {
                            api_key
                        };
                        (url, key)
                    };
                    let auth_source = if auth_source == "chatgpt_subscription" {
                        crate::agent::types::AuthSource::ChatgptSubscription
                    } else {
                        crate::agent::types::AuthSource::ApiKey
                    };
                    tracing::info!(
                        provider = %provider_id,
                        url = %resolved_url,
                        has_key = !resolved_key.is_empty(),
                        "validating provider connection"
                    );
                    let (valid, error) =
                        match crate::agent::llm_client::validate_provider_connection(
                            &provider_id,
                            &resolved_url,
                            &resolved_key,
                            auth_source,
                        )
                        .await
                        {
                            Ok(_) => (true, None),
                            Err(e) => {
                                tracing::warn!(provider = %provider_id, error = %e, "provider validation failed");
                                (false, Some(e.to_string()))
                            }
                        };
                    framed
                        .send(DaemonMessage::AgentProviderValidation {
                            provider_id,
                            valid,
                            error,
                            models_json: None,
                        })
                        .await?;
                }

                ClientMessage::AgentSetSubAgent { sub_agent_json } => {
                    match serde_json::from_str(&sub_agent_json) {
                        Ok(def) => {
                            agent.set_sub_agent(def).await;
                            framed
                                .send(DaemonMessage::AgentSubAgentUpdated { sub_agent_json })
                                .await?;
                        }
                        Err(e) => {
                            framed
                                .send(DaemonMessage::AgentError {
                                    message: format!("Invalid sub-agent: {e}"),
                                })
                                .await?;
                        }
                    }
                }

                ClientMessage::AgentRemoveSubAgent { sub_agent_id } => {
                    agent.remove_sub_agent(&sub_agent_id).await;
                    framed
                        .send(DaemonMessage::AgentSubAgentRemoved { sub_agent_id })
                        .await?;
                }

                ClientMessage::AgentListSubAgents => {
                    let list = agent.list_sub_agents().await;
                    let json = serde_json::to_string(&list).unwrap_or_default();
                    framed
                        .send(DaemonMessage::AgentSubAgentList {
                            sub_agents_json: json,
                        })
                        .await?;
                }

                ClientMessage::AgentGetConciergeConfig => {
                    let concierge = agent.get_concierge_config().await;
                    let json = serde_json::to_string(&concierge).unwrap_or_default();
                    framed
                        .send(DaemonMessage::AgentConciergeConfig { config_json: json })
                        .await?;
                }

                ClientMessage::AgentSetConciergeConfig { config_json } => {
                    match serde_json::from_str::<crate::agent::types::ConciergeConfig>(&config_json)
                    {
                        Ok(concierge_config) => {
                            agent.set_concierge_config(concierge_config).await;
                        }
                        Err(e) => {
                            framed
                                .send(DaemonMessage::AgentError {
                                    message: format!("Invalid concierge config: {e}"),
                                })
                                .await?;
                        }
                    }
                }

                ClientMessage::AgentRequestConciergeWelcome => {
                    tracing::info!("server: received AgentRequestConciergeWelcome");

                    // If first-time user (onboarding not completed), deliver tier-adapted onboarding
                    let (onboarding_done, tier) = {
                        let cfg = agent.config.read().await;
                        let done = cfg.tier.onboarding_completed;
                        let t = cfg.tier.user_self_assessment
                            .unwrap_or(crate::agent::capability_tier::CapabilityTier::Newcomer);
                        (done, t)
                    };
                    if !onboarding_done {
                        if let Err(e) = agent.concierge.deliver_onboarding(tier).await {
                            tracing::warn!("onboarding delivery failed, falling back to generic welcome: {e}");
                        }
                        // Mark onboarding as completed so it doesn't re-trigger on reconnect
                        {
                            let mut cfg = agent.config.write().await;
                            cfg.tier.onboarding_completed = true;
                            // Config will be persisted on next heartbeat or config change
                        }
                    }

                    // Generate welcome inline (awaits LLM call for non-Minimal levels).
                    // We send the result directly as a DaemonMessage rather than going
                    // through the broadcast event channel, because the connection handler's
                    // try_recv loop won't drain until the next client message arrives.
                    let welcome = agent
                        .concierge
                        .generate_welcome(&agent.threads, &agent.tasks)
                        .await;
                    if let Some((content, detail_level, actions)) = welcome {
                        let event = crate::agent::types::AgentEvent::ConciergeWelcome {
                            thread_id: crate::agent::concierge::CONCIERGE_THREAD_ID.to_string(),
                            content,
                            detail_level,
                            actions,
                        };
                        if let Ok(json) = serde_json::to_string(&event) {
                            framed
                                .send(DaemonMessage::AgentEvent { event_json: json })
                                .await
                                .ok();
                        }
                    }
                }

                ClientMessage::AgentDismissConciergeWelcome => {
                    agent.concierge.prune_welcome_messages(&agent.threads).await;
                    framed
                        .send(DaemonMessage::AgentConciergeWelcomeDismissed)
                        .await?;
                }

                ClientMessage::AuditQuery {
                    action_types,
                    since,
                    limit,
                } => {
                    let action_types_ref = action_types.as_deref();
                    let since_i64 = since.map(|s| s as i64);
                    let limit = limit.unwrap_or(100);
                    match agent
                        .history
                        .list_action_audit(action_types_ref, since_i64, limit)
                        .await
                    {
                        Ok(rows) => {
                            let public_entries: Vec<amux_protocol::AuditEntryPublic> = rows
                                .into_iter()
                                .map(|r| amux_protocol::AuditEntryPublic {
                                    id: r.id,
                                    timestamp: r.timestamp,
                                    action_type: r.action_type,
                                    summary: r.summary,
                                    explanation: r.explanation,
                                    confidence: r.confidence,
                                    confidence_band: r.confidence_band,
                                    causal_trace_id: r.causal_trace_id,
                                    thread_id: r.thread_id,
                                    goal_run_id: r.goal_run_id,
                                    task_id: r.task_id,
                                })
                                .collect();
                            let entries_json =
                                serde_json::to_string(&public_entries).unwrap_or_default();
                            framed
                                .send(DaemonMessage::AuditList { entries_json })
                                .await?;
                        }
                        Err(e) => {
                            framed
                                .send(DaemonMessage::Error {
                                    message: format!("audit query failed: {e}"),
                                })
                                .await?;
                        }
                    }
                }

                ClientMessage::AuditDismiss { entry_id } => {
                    tracing::info!(entry_id = %entry_id, "Audit dismiss requested");
                    let result = agent.history.dismiss_audit_entry(&entry_id).await;
                    let msg = match result {
                        Ok(()) => DaemonMessage::AuditDismissResult {
                            success: true,
                            message: format!("Dismissed audit entry {}", entry_id),
                        },
                        Err(e) => DaemonMessage::AuditDismissResult {
                            success: false,
                            message: format!("Failed to dismiss: {}", e),
                        },
                    };
                    framed.send(msg).await?;
                }

                ClientMessage::EscalationCancel { thread_id } => {
                    tracing::info!(thread_id = %thread_id, "escalation cancel requested by user (D-13)");

                    // Create an audit entry for the cancellation.
                    let now_ms = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_millis() as i64;

                    let audit_id = format!("audit-esc-cancel-{}", uuid::Uuid::new_v4());
                    let summary = format!("User cancelled escalation for thread {thread_id}");

                    let audit_entry = crate::history::AuditEntryRow {
                        id: audit_id.clone(),
                        timestamp: now_ms,
                        action_type: "escalation".to_string(),
                        summary: summary.clone(),
                        explanation: Some(summary.clone()),
                        confidence: None,
                        confidence_band: None,
                        causal_trace_id: None,
                        thread_id: Some(thread_id.clone()),
                        goal_run_id: None,
                        task_id: None,
                        raw_data_json: Some(serde_json::json!({
                            "action": "cancel",
                            "thread_id": thread_id,
                            "outcome": "cancelled_by_user",
                        }).to_string()),
                    };

                    if let Err(e) = agent.history.insert_action_audit(&audit_entry).await {
                        tracing::warn!("failed to record escalation cancel audit: {e}");
                    }

                    // Broadcast EscalationUpdate event so all clients see the cancel.
                    let _ = agent.event_tx.send(
                        crate::agent::types::AgentEvent::EscalationUpdate {
                            thread_id: thread_id.clone(),
                            from_level: "unknown".to_string(),
                            to_level: "L0".to_string(),
                            reason: "User took over (I'll handle this)".to_string(),
                            attempts: 0,
                            audit_id: Some(audit_id.clone()),
                        },
                    );

                    // Broadcast AuditAction event.
                    let _ = agent.event_tx.send(
                        crate::agent::types::AgentEvent::AuditAction {
                            id: audit_id,
                            timestamp: now_ms as u64,
                            action_type: "escalation".to_string(),
                            summary: summary.clone(),
                            explanation: Some(summary.clone()),
                            confidence: None,
                            confidence_band: None,
                            causal_trace_id: None,
                            thread_id: Some(thread_id.clone()),
                        },
                    );

                    framed
                        .send(DaemonMessage::EscalationCancelResult {
                            success: true,
                            message: format!("Escalation cancelled for thread {thread_id}. You now have control."),
                        })
                        .await?;
                }

                ClientMessage::SkillList { status, limit } => {
                    let limit = limit.clamp(1, 200);
                    let result = if let Some(ref st) = status {
                        agent.history.list_skill_variants_by_status(st, limit).await
                    } else {
                        agent.history.list_skill_variants(None, limit).await
                    };
                    match result {
                        Ok(records) => {
                            let variants: Vec<amux_protocol::SkillVariantPublic> = records
                                .into_iter()
                                .map(|r| amux_protocol::SkillVariantPublic {
                                    variant_id: r.variant_id,
                                    skill_name: r.skill_name,
                                    variant_name: r.variant_name,
                                    relative_path: r.relative_path,
                                    status: r.status,
                                    use_count: r.use_count,
                                    success_count: r.success_count,
                                    failure_count: r.failure_count,
                                    context_tags: r.context_tags,
                                    created_at: r.created_at,
                                    updated_at: r.updated_at,
                                })
                                .collect();
                            framed
                                .send(DaemonMessage::SkillListResult { variants })
                                .await?;
                        }
                        Err(e) => {
                            framed
                                .send(DaemonMessage::Error {
                                    message: format!("skill list failed: {e}"),
                                })
                                .await?;
                        }
                    }
                }

                ClientMessage::SkillInspect { identifier } => {
                    // Try variant_id first, then fall back to skill name search
                    let variant = match agent.history.get_skill_variant(&identifier).await {
                        Ok(Some(v)) => Some(v),
                        _ => {
                            // Search by skill name
                            match agent.history.list_skill_variants(Some(&identifier), 1).await {
                                Ok(variants) => variants.into_iter().next(),
                                Err(_) => None,
                            }
                        }
                    };

                    let (public, content) = if let Some(ref v) = variant {
                        // Read SKILL.md content from disk
                        let skill_path = agent.data_dir
                            .parent()
                            .unwrap_or(std::path::Path::new("."))
                            .join("skills")
                            .join(&v.relative_path);
                        let content = tokio::fs::read_to_string(&skill_path).await.ok();
                        let public = amux_protocol::SkillVariantPublic {
                            variant_id: v.variant_id.clone(),
                            skill_name: v.skill_name.clone(),
                            variant_name: v.variant_name.clone(),
                            relative_path: v.relative_path.clone(),
                            status: v.status.clone(),
                            use_count: v.use_count,
                            success_count: v.success_count,
                            failure_count: v.failure_count,
                            context_tags: v.context_tags.clone(),
                            created_at: v.created_at,
                            updated_at: v.updated_at,
                        };
                        (Some(public), content)
                    } else {
                        (None, None)
                    };

                    framed
                        .send(DaemonMessage::SkillInspectResult {
                            variant: public,
                            content,
                        })
                        .await?;
                }

                ClientMessage::SkillReject { identifier } => {
                    // Find the variant
                    let variant = match agent.history.get_skill_variant(&identifier).await {
                        Ok(Some(v)) => Some(v),
                        _ => {
                            match agent.history.list_skill_variants(Some(&identifier), 1).await {
                                Ok(variants) => variants.into_iter().next(),
                                Err(_) => None,
                            }
                        }
                    };

                    let msg = if let Some(v) = variant {
                        // Only draft/testing skills can be rejected
                        if v.status != "draft" && v.status != "testing" {
                            DaemonMessage::SkillActionResult {
                                success: false,
                                message: format!(
                                    "Cannot reject skill '{}' with status '{}' -- only draft/testing skills can be rejected.",
                                    v.skill_name, v.status
                                ),
                            }
                        } else {
                            // Delete the SKILL.md file from disk
                            let skill_path = agent.data_dir
                                .parent()
                                .unwrap_or(std::path::Path::new("."))
                                .join("skills")
                                .join(&v.relative_path);
                            let _ = tokio::fs::remove_file(&skill_path).await;

                            // Update status to archived
                            match agent.history.update_skill_variant_status(&v.variant_id, "archived").await {
                                Ok(()) => DaemonMessage::SkillActionResult {
                                    success: true,
                                    message: format!("Rejected and archived skill '{}'.", v.skill_name),
                                },
                                Err(e) => DaemonMessage::SkillActionResult {
                                    success: false,
                                    message: format!("Failed to archive skill: {e}"),
                                },
                            }
                        }
                    } else {
                        DaemonMessage::SkillActionResult {
                            success: false,
                            message: format!("Skill not found: {identifier}"),
                        }
                    };
                    framed.send(msg).await?;
                }

                ClientMessage::SkillPromote { identifier, target_status } => {
                    // Validate target status
                    let valid_statuses = ["draft", "testing", "active", "proven", "promoted_to_canonical"];
                    if !valid_statuses.contains(&target_status.as_str()) {
                        framed
                            .send(DaemonMessage::SkillActionResult {
                                success: false,
                                message: format!(
                                    "Invalid target status '{}'. Valid: {}",
                                    target_status,
                                    valid_statuses.join(", ")
                                ),
                            })
                            .await?;
                    } else {
                        // Find the variant
                        let variant = match agent.history.get_skill_variant(&identifier).await {
                            Ok(Some(v)) => Some(v),
                            _ => {
                                match agent.history.list_skill_variants(Some(&identifier), 1).await {
                                    Ok(variants) => variants.into_iter().next(),
                                    Err(_) => None,
                                }
                            }
                        };

                        let msg = if let Some(v) = variant {
                            match agent.history.update_skill_variant_status(&v.variant_id, &target_status).await {
                                Ok(()) => {
                                    // Record provenance
                                    agent.record_provenance_event(
                                        "skill_lifecycle_promotion",
                                        &format!(
                                            "Skill '{}' fast-promoted {} -> {} via CLI",
                                            v.skill_name, v.status, target_status
                                        ),
                                        serde_json::json!({
                                            "variant_id": v.variant_id,
                                            "skill_name": v.skill_name,
                                            "from_status": v.status,
                                            "to_status": target_status,
                                            "trigger": "cli_promote",
                                        }),
                                        None, None, None, None, None,
                                    ).await;

                                    DaemonMessage::SkillActionResult {
                                        success: true,
                                        message: format!(
                                            "Skill '{}' promoted from {} to {}.",
                                            v.skill_name, v.status, target_status
                                        ),
                                    }
                                }
                                Err(e) => DaemonMessage::SkillActionResult {
                                    success: false,
                                    message: format!("Failed to promote skill: {e}"),
                                },
                            }
                        } else {
                            DaemonMessage::SkillActionResult {
                                success: false,
                                message: format!("Skill not found: {identifier}"),
                            }
                        };
                        framed.send(msg).await?;
                    }
                }

                ClientMessage::SkillSearch { query } => {
                    let config = agent.config.read().await;
                    let registry_url = config
                        .extra
                        .get("registry_url")
                        .and_then(|value| value.as_str())
                        .unwrap_or("https://registry.tamux.dev")
                        .to_string();
                    drop(config);

                    let client = RegistryClient::new(registry_url, agent.history.data_dir());
                    let entries: Vec<amux_protocol::CommunitySkillEntry> = match client.search(&query).await {
                        Ok(entries) => entries
                            .into_iter()
                            .map(|entry| to_community_entry(&entry))
                            .collect(),
                        Err(_) => Vec::new(),
                    };
                    framed
                        .send(DaemonMessage::SkillSearchResult { entries })
                        .await?;
                }

                ClientMessage::SkillImport {
                    source,
                    force,
                    publisher_verified,
                } => {
                    let config = agent.config.read().await;
                    let registry_url = config
                        .extra
                        .get("registry_url")
                        .and_then(|value| value.as_str())
                        .unwrap_or("https://registry.tamux.dev")
                        .to_string();
                    drop(config);

                    let client = RegistryClient::new(registry_url, agent.history.data_dir());
                    let whitelist = vec![
                        "read_file".to_string(),
                        "write_file".to_string(),
                        "list_files".to_string(),
                        "create_directory".to_string(),
                        "search_history".to_string(),
                    ];
                    let skills_root = agent.history.data_dir().join("skills");

                    let import_result: Result<(String, String), anyhow::Error> = async {
                        if source.starts_with("http://") || source.starts_with("https://") {
                            let archive_name = source
                                .rsplit('/')
                                .next()
                                .unwrap_or("community-skill.tar.gz")
                                .trim_end_matches(".tar.gz")
                                .to_string();
                            let archive_path = client.fetch_skill(&archive_name).await?;
                            let extract_dir = std::env::temp_dir().join(format!(
                                "tamux-community-import-{}-{}",
                                archive_name,
                                uuid::Uuid::new_v4()
                            ));
                            if extract_dir.exists() {
                                let _ = tokio::fs::remove_dir_all(&extract_dir).await;
                            }
                            tokio::fs::create_dir_all(&extract_dir).await?;
                            unpack_skill(&archive_path, &extract_dir)?;
                            let skill_path = extract_dir.join("SKILL.md");
                            let content = tokio::fs::read_to_string(&skill_path).await?;
                            Ok((archive_name, content))
                        } else {
                            let archive_path = client.fetch_skill(&source).await?;
                            let extract_dir = std::env::temp_dir().join(format!(
                                "tamux-community-import-{}-{}",
                                source,
                                uuid::Uuid::new_v4()
                            ));
                            if extract_dir.exists() {
                                let _ = tokio::fs::remove_dir_all(&extract_dir).await;
                            }
                            tokio::fs::create_dir_all(&extract_dir).await?;
                            unpack_skill(&archive_path, &extract_dir)?;
                            let skill_path = extract_dir.join("SKILL.md");
                            let content = tokio::fs::read_to_string(&skill_path).await?;
                            Ok((source.clone(), content))
                        }
                    }
                    .await;

                    let msg = match import_result {
                        Ok((skill_name, content)) => match import_community_skill(
                            &agent.history,
                            &content,
                            &skill_name,
                            &source,
                            &whitelist,
                            force,
                            publisher_verified,
                            &skills_root,
                        )
                        .await
                        {
                            Ok(ImportResult::Success {
                                variant_id,
                                scan_verdict,
                            }) => DaemonMessage::SkillImportResult {
                                success: true,
                                message: format!("Imported community skill '{skill_name}' as draft."),
                                variant_id: Some(variant_id),
                                scan_verdict: Some(scan_verdict),
                                findings_count: 0,
                            },
                            Ok(ImportResult::Blocked {
                                report_summary,
                                findings_count,
                            }) => DaemonMessage::SkillImportResult {
                                success: false,
                                message: report_summary,
                                variant_id: None,
                                scan_verdict: Some("block".to_string()),
                                findings_count,
                            },
                            Ok(ImportResult::NeedsForce {
                                report_summary,
                                findings_count,
                            }) => DaemonMessage::SkillImportResult {
                                success: false,
                                message: report_summary,
                                variant_id: None,
                                scan_verdict: Some("warn".to_string()),
                                findings_count,
                            },
                            Err(e) => DaemonMessage::SkillImportResult {
                                success: false,
                                message: format!("community skill import failed: {e}"),
                                variant_id: None,
                                scan_verdict: None,
                                findings_count: 0,
                            },
                        },
                        Err(e) => DaemonMessage::SkillImportResult {
                            success: false,
                            message: format!("community skill fetch failed: {e}"),
                            variant_id: None,
                            scan_verdict: None,
                            findings_count: 0,
                        },
                    };

                    framed.send(msg).await?;
                }

                ClientMessage::SkillExport {
                    identifier,
                    format,
                    output_dir,
                } => {
                    let variant = match agent.history.get_skill_variant(&identifier).await {
                        Ok(Some(v)) => Some(v),
                        _ => match agent.history.list_skill_variants(Some(&identifier), 1).await {
                            Ok(variants) => variants.into_iter().next(),
                            Err(_) => None,
                        },
                    };

                    let msg = if let Some(v) = variant {
                        let skill_path = agent.history.data_dir().join("skills").join(&v.relative_path);
                        match tokio::fs::read_to_string(&skill_path).await {
                            Ok(content) => match export_skill(
                                &content,
                                &format,
                                Path::new(&output_dir),
                                &v.skill_name,
                            ) {
                                Ok(path) => DaemonMessage::SkillExportResult {
                                    success: true,
                                    message: format!("Exported skill '{}' to {}.", v.skill_name, path),
                                    output_path: Some(path),
                                },
                                Err(e) => DaemonMessage::SkillExportResult {
                                    success: false,
                                    message: format!("community skill export failed: {e}"),
                                    output_path: None,
                                },
                            },
                            Err(e) => DaemonMessage::SkillExportResult {
                                success: false,
                                message: format!("failed to read skill for export: {e}"),
                                output_path: None,
                            },
                        }
                    } else {
                        DaemonMessage::SkillExportResult {
                            success: false,
                            message: format!("Skill not found: {identifier}"),
                            output_path: None,
                        }
                    };
                    framed.send(msg).await?;
                }

                ClientMessage::SkillPublish { identifier } => {
                    let variant = match agent.history.get_skill_variant(&identifier).await {
                        Ok(Some(v)) => Some(v),
                        _ => match agent.history.list_skill_variants(Some(&identifier), 1).await {
                            Ok(variants) => variants.into_iter().next(),
                            Err(_) => None,
                        },
                    };

                    let msg = if let Some(v) = variant {
                        if v.status != "proven" && v.status != "canonical" {
                            DaemonMessage::SkillPublishResult {
                                success: false,
                                message: format!(
                                    "Only proven or canonical skills can be published; '{}' is {}.",
                                    v.skill_name, v.status
                                ),
                            }
                        } else {
                            let config = agent.config.read().await;
                            let registry_url = config
                                .extra
                                .get("registry_url")
                                .and_then(|value| value.as_str())
                                .unwrap_or("https://registry.tamux.dev")
                                .to_string();
                            drop(config);

                            let skill_dir = agent
                                .history
                                .data_dir()
                                .join("skills")
                                .join(Path::new(&v.relative_path).parent().unwrap_or(Path::new(".")));
                            let machine_id = agent.history.data_dir().to_string_lossy().to_string();
                            match prepare_publish(&skill_dir, &v, &machine_id) {
                                Ok((tarball, metadata)) => {
                                    let client = RegistryClient::new(registry_url, agent.history.data_dir());
                                    match client.publish_skill(&tarball, &metadata).await {
                                        Ok(()) => DaemonMessage::SkillPublishResult {
                                            success: true,
                                            message: format!("Published skill '{}'.", v.skill_name),
                                        },
                                        Err(e) => DaemonMessage::SkillPublishResult {
                                            success: false,
                                            message: format!("community skill publish failed: {e}"),
                                        },
                                    }
                                }
                                Err(e) => DaemonMessage::SkillPublishResult {
                                    success: false,
                                    message: format!("failed to prepare skill publish: {e}"),
                                },
                            }
                        }
                    } else {
                        DaemonMessage::SkillPublishResult {
                            success: false,
                            message: format!("Skill not found: {identifier}"),
                        }
                    };

                    framed.send(msg).await?;
                }

                ClientMessage::AgentStatusQuery => {
                    let msg = agent.get_status_snapshot().await;
                    framed.send(msg).await?;
                }

                ClientMessage::AgentSetTierOverride { tier } => {
                    use crate::agent::capability_tier::CapabilityTier;
                    let parsed = tier.as_deref().and_then(CapabilityTier::from_str_loose);
                    // If a tier string was provided but failed to parse, return error.
                    if tier.is_some() && parsed.is_none() {
                        framed
                            .send(DaemonMessage::Error {
                                message: format!(
                                    "Invalid tier '{}'. Expected: newcomer, familiar, power_user, expert.",
                                    tier.unwrap_or_default()
                                ),
                            })
                            .await?;
                    } else {
                        agent.set_tier_override(parsed).await;
                        // No explicit response -- a TierChanged event is broadcast if tier
                        // actually changed, and the caller can query status afterward.
                    }
                }

                // Plugin operations (Plan 14-02).
                ClientMessage::PluginList {} => {
                    let plugins = plugin_manager.list_plugins().await;
                    framed
                        .send(DaemonMessage::PluginListResult { plugins })
                        .await?;
                }
                ClientMessage::PluginGet { name } => {
                    match plugin_manager.get_plugin(&name).await {
                        Some((info, settings_schema)) => {
                            framed
                                .send(DaemonMessage::PluginGetResult {
                                    plugin: Some(info),
                                    settings_schema,
                                })
                                .await?;
                        }
                        None => {
                            framed
                                .send(DaemonMessage::PluginGetResult {
                                    plugin: None,
                                    settings_schema: None,
                                })
                                .await?;
                        }
                    }
                }
                ClientMessage::PluginEnable { name } => {
                    let result = plugin_manager.set_enabled(&name, true).await;
                    let (success, message) = match result {
                        Ok(()) => (true, format!("Plugin '{}' enabled", name)),
                        Err(e) => (false, format!("Failed to enable plugin '{}': {}", name, e)),
                    };
                    framed
                        .send(DaemonMessage::PluginActionResult { success, message })
                        .await?;
                }
                ClientMessage::PluginDisable { name } => {
                    let result = plugin_manager.set_enabled(&name, false).await;
                    let (success, message) = match result {
                        Ok(()) => (true, format!("Plugin '{}' disabled", name)),
                        Err(e) => (false, format!("Failed to disable plugin '{}': {}", name, e)),
                    };
                    framed
                        .send(DaemonMessage::PluginActionResult { success, message })
                        .await?;
                }
                ClientMessage::PluginInstall { dir_name, install_source } => {
                    let result = plugin_manager.register_plugin(&dir_name, &install_source).await;
                    let (success, message) = match result {
                        Ok(info) => (true, format!("Plugin '{}' v{} registered successfully", info.name, info.version)),
                        Err(e) => (false, format!("Failed to register plugin: {}", e)),
                    };
                    framed
                        .send(DaemonMessage::PluginActionResult { success, message })
                        .await?;
                }
                ClientMessage::PluginUninstall { name } => {
                    let result = plugin_manager.unregister_plugin(&name).await;
                    let (success, message) = match result {
                        Ok(()) => (true, format!("Plugin '{}' unregistered", name)),
                        Err(e) => (false, format!("Failed to unregister plugin '{}': {}", name, e)),
                    };
                    framed
                        .send(DaemonMessage::PluginActionResult { success, message })
                        .await?;
                }

                // Plugin settings operations (Plan 16-01).
                ClientMessage::PluginGetSettings { name } => {
                    let settings = plugin_manager.get_settings(&name).await;
                    framed
                        .send(DaemonMessage::PluginSettingsResult {
                            plugin_name: name,
                            settings,
                        })
                        .await?;
                }
                ClientMessage::PluginUpdateSettings {
                    plugin_name,
                    key,
                    value,
                    is_secret,
                } => {
                    let result = plugin_manager
                        .update_setting(&plugin_name, &key, &value, is_secret)
                        .await;
                    let (success, message) = match result {
                        Ok(()) => (
                            true,
                            format!(
                                "Setting '{}' updated for plugin '{}'",
                                key, plugin_name
                            ),
                        ),
                        Err(e) => (false, format!("Failed to update setting: {}", e)),
                    };
                    framed
                        .send(DaemonMessage::PluginActionResult { success, message })
                        .await?;
                }
                ClientMessage::PluginTestConnection { name } => {
                    let (success, message) = plugin_manager.test_connection(&name).await;
                    framed
                        .send(DaemonMessage::PluginTestConnectionResult {
                            plugin_name: name,
                            success,
                            message,
                        })
                        .await?;
                }

                ClientMessage::PluginListCommands {} => {
                    let commands = plugin_manager.list_commands().await;
                    framed
                        .send(DaemonMessage::PluginCommandsResult { commands })
                        .await?;
                }

                // Plugin API proxy call: orchestrates full proxy flow through PluginManager.
                ClientMessage::PluginApiCall {
                    plugin_name,
                    endpoint_name,
                    params,
                } => {
                    let params_json: serde_json::Value = serde_json::from_str(&params)
                        .unwrap_or(serde_json::Value::Object(Default::default()));
                    match plugin_manager.api_call(&plugin_name, &endpoint_name, params_json).await {
                        Ok(result_text) => {
                            framed
                                .send(DaemonMessage::PluginApiCallResult {
                                    plugin_name,
                                    endpoint_name,
                                    success: true,
                                    result: result_text,
                                    error_type: None,
                                })
                                .await?;
                        }
                        Err(e) => {
                            let error_type = match &e {
                                crate::plugin::PluginApiError::SsrfBlocked { .. } => "ssrf_blocked",
                                crate::plugin::PluginApiError::RateLimited { .. } => "rate_limited",
                                crate::plugin::PluginApiError::Timeout => "timeout",
                                crate::plugin::PluginApiError::HttpError { .. } => "http_error",
                                crate::plugin::PluginApiError::TemplateError { .. } => "template_error",
                                crate::plugin::PluginApiError::EndpointNotFound { .. } => "endpoint_not_found",
                                crate::plugin::PluginApiError::PluginNotFound { .. } => "plugin_not_found",
                                crate::plugin::PluginApiError::PluginDisabled { .. } => "plugin_disabled",
                            };
                            framed
                                .send(DaemonMessage::PluginApiCallResult {
                                    plugin_name,
                                    endpoint_name,
                                    success: false,
                                    result: e.to_string(),
                                    error_type: Some(error_type.to_string()),
                                })
                                .await?;
                        }
                    }
                }
            }
        }
    }
}
