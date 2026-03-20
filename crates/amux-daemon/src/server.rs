use std::sync::Arc;

use amux_protocol::{ClientMessage, DaemonMessage};
use anyhow::Result;
use futures::SinkExt;
use futures::StreamExt;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::sync::broadcast;
use tokio_util::codec::Framed;

use crate::agent::AgentEngine;
use crate::session_manager::SessionManager;

/// Socket path / pipe name for IPC.
#[cfg(unix)]
pub fn socket_path() -> std::path::PathBuf {
    let runtime_dir = std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/tmp".to_string());
    std::path::PathBuf::from(runtime_dir).join("tamux-daemon.sock")
}

/// Run the IPC server until a shutdown signal is received.
pub async fn run() -> Result<()> {
    let manager = SessionManager::new();
    let reaper_manager = manager.clone();

    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(5));
        loop {
            interval.tick().await;
            reaper_manager.reap_dead().await;
        }
    });

    // Start agent engine
    let agent_config = crate::agent::load_config().unwrap_or_default();
    let agent = AgentEngine::new(manager.clone(), agent_config);

    // Hydrate persisted state (threads, tasks, heartbeat, memory)
    if let Err(e) = agent.hydrate().await {
        tracing::warn!("failed to hydrate agent engine: {e}");
    }

    // Start background loop (tasks + heartbeat)
    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
    let loop_agent = agent.clone();
    tokio::spawn(async move {
        loop_agent.run_loop(shutdown_rx).await;
    });

    #[cfg(unix)]
    let result = run_unix(manager, agent.clone()).await;

    #[cfg(windows)]
    let result = run_windows(manager, agent.clone()).await;

    // Signal agent loop shutdown
    let _ = shutdown_tx.send(true);

    result
}

// ---------------------------------------------------------------------------
// Unix Domain Socket implementation
// ---------------------------------------------------------------------------

#[cfg(unix)]
async fn run_unix(manager: Arc<SessionManager>, agent: Arc<AgentEngine>) -> Result<()> {
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
        _ = accept_loop_unix(listener, manager, agent) => {}
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
) {
    loop {
        match listener.accept().await {
            Ok((stream, _addr)) => {
                let manager = manager.clone();
                let agent = agent.clone();
                tokio::spawn(async move {
                    if let Err(e) = handle_connection(stream, manager, agent).await {
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
async fn run_windows(manager: Arc<SessionManager>, agent: Arc<AgentEngine>) -> Result<()> {
    let addr = amux_protocol::default_tcp_addr();
    tracing::info!(%addr, "daemon listening on TCP");
    run_tcp_fallback(manager, agent).await
}

/// TCP server used for Windows IPC.
#[allow(dead_code)]
async fn run_tcp_fallback(manager: Arc<SessionManager>, agent: Arc<AgentEngine>) -> Result<()> {
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
        _ = accept_loop_tcp(listener, manager, agent) => {}
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
) {
    loop {
        match listener.accept().await {
            Ok((stream, addr)) => {
                tracing::info!(%addr, "client connected");
                let manager = manager.clone();
                let agent = agent.clone();
                tokio::spawn(async move {
                    if let Err(e) = handle_connection(stream, manager, agent).await {
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
) -> Result<()>
where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    use amux_protocol::DaemonCodec;
    let mut framed = Framed::new(stream, DaemonCodec);

    // Track which sessions this client is attached to so we can fan-out output.
    let mut attached_rxs: Vec<(amux_protocol::SessionId, broadcast::Receiver<DaemonMessage>)> =
        Vec::new();

    // Optional agent event subscription.
    let mut agent_event_rx: Option<broadcast::Receiver<crate::agent::types::AgentEvent>> = None;

    loop {
        // Drain agent events if subscribed.
        if let Some(ref mut rx) = agent_event_rx {
            loop {
                match rx.try_recv() {
                    Ok(event) => {
                        if let Ok(json) = serde_json::to_string(&event) {
                            framed
                                .send(DaemonMessage::AgentEvent { event_json: json })
                                .await?;
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
                            .handle_task_approval_resolution(&approval_id, decision)
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
                    match manager.search_history(&query, limit.unwrap_or(8).max(1)) {
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
                        Ok(entry) => match manager.append_command_log(&entry) {
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
                } => match manager.complete_command_log(&id, exit_code, duration_ms) {
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
                ) {
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

                ClientMessage::ClearCommandLog => match manager.clear_command_log() {
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
                        Ok(thread) => match manager.create_agent_thread(&thread) {
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
                    match manager.delete_agent_thread(&thread_id) {
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

                ClientMessage::ListAgentThreads => match manager.list_agent_threads() {
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
                    match manager.get_agent_thread(&thread_id) {
                        Ok(thread) => {
                            let messages = manager.list_agent_messages(&thread_id, None)?;
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
                        Ok(message) => match manager.add_agent_message(&message) {
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

                ClientMessage::ListAgentMessages { thread_id, limit } => {
                    match manager.list_agent_messages(&thread_id, limit) {
                        Ok(messages) => {
                            let thread = manager.get_agent_thread(&thread_id)?;
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
                        Ok(entry) => match manager.upsert_transcript_index(&entry) {
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
                    match manager.list_transcript_index(workspace_id.as_deref()) {
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
                        Ok(entry) => match manager.upsert_snapshot_index(&entry) {
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
                    match manager.list_snapshot_index(workspace_id.as_deref()) {
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
                        Ok(event) => match manager.upsert_agent_event(&event) {
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
                    match manager.generate_skill(query.as_deref(), title.as_deref()) {
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
                    match manager.list_snapshots(workspace_id.as_deref()) {
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
                    match manager.restore_snapshot(&snapshot_id) {
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
                    let agent = agent.clone();
                    let event_tx = agent.event_sender();
                    tokio::spawn(async move {
                        // If thread_id is None but we have context, generate a stable
                        // thread ID so seeding and sending use the same thread.
                        let mut thread_id = thread_id;
                        if thread_id.is_none() {
                            if let Some(ref json) = context_messages_json {
                                if json.len() > 2 {
                                    // non-empty JSON array
                                    thread_id = Some(format!("thread_{}", uuid::Uuid::new_v4()));
                                }
                            }
                        }
                        let has_context = context_messages_json.is_some();
                        tracing::info!(
                            thread_id = ?thread_id,
                            content_len = content.len(),
                            has_context_json = has_context,
                            "AgentSendMessage received"
                        );
                        // Seed context messages into the thread before the LLM turn
                        if let Some(ref json) = context_messages_json {
                            match serde_json::from_str::<Vec<amux_protocol::AgentDbMessage>>(json) {
                                Ok(ctx) if !ctx.is_empty() => {
                                    tracing::info!(count = ctx.len(), "seeding thread with context messages");
                                    agent.seed_thread_context(thread_id.as_deref(), &ctx).await;
                                }
                                Ok(_) => tracing::info!("context_messages_json was empty array"),
                                Err(e) => tracing::warn!(error = %e, json_len = json.len(), "failed to parse context_messages_json"),
                            }
                        }
                        if let Err(e) = agent
                            .send_message_with_session(
                                thread_id.as_deref(),
                                session_id.as_deref(),
                                &content,
                            )
                            .await
                        {
                            let _ = event_tx.send(crate::agent::types::AgentEvent::Error {
                                thread_id: thread_id.unwrap_or_default(),
                                message: e.to_string(),
                            });
                        }
                    });
                }

                ClientMessage::AgentStopStream { thread_id } => {
                    let _ = agent.stop_stream(&thread_id).await;
                }

                ClientMessage::AgentListThreads => {
                    let threads = agent.list_threads().await;
                    let json = serde_json::to_string(&threads).unwrap_or_default();
                    framed
                        .send(DaemonMessage::AgentThreadList { threads_json: json })
                        .await?;
                }

                ClientMessage::AgentGetThread { thread_id } => {
                    let thread = agent.get_thread(&thread_id).await;
                    let json = serde_json::to_string(&thread).unwrap_or_default();
                    framed
                        .send(DaemonMessage::AgentThreadDetail { thread_json: json })
                        .await?;
                }

                ClientMessage::AgentDeleteThread { thread_id } => {
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
                    let todos = agent.get_todos(&thread_id).await;
                    let json = serde_json::to_string(&todos).unwrap_or_default();
                    framed
                        .send(DaemonMessage::AgentTodoDetail {
                            thread_id,
                            todos_json: json,
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

                ClientMessage::AgentSetConfig { config_json } => {
                    match serde_json::from_str(&config_json) {
                        Ok(config) => agent.set_config(config).await,
                        Err(e) => {
                            framed
                                .send(DaemonMessage::Error {
                                    message: format!("Invalid config: {e}"),
                                })
                                .await?;
                        }
                    }
                }

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
                    agent.handle_task_approval_resolution(&approval_id, decision).await;
                }

                ClientMessage::AgentSubscribe => {
                    agent_event_rx = Some(agent.subscribe());
                    tracing::info!("client subscribed to agent events");
                }

                ClientMessage::AgentUnsubscribe => {
                    agent_event_rx = None;
                    tracing::info!("client unsubscribed from agent events");
                }
            }
        }
    }
}
