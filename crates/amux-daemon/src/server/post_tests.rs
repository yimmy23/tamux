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
                | crate::agent::types::AgentEvent::NotificationInboxUpsert { .. }
                | crate::agent::types::AgentEvent::AnticipatoryUpdate { .. }
                | crate::agent::types::AgentEvent::ConciergeWelcome { .. }
                | crate::agent::types::AgentEvent::ProviderCircuitOpen { .. }
                | crate::agent::types::AgentEvent::ProviderCircuitRecovered { .. }
                | crate::agent::types::AgentEvent::AuditAction { .. }
                | crate::agent::types::AgentEvent::EscalationUpdate { .. }
                | crate::agent::types::AgentEvent::GatewayStatus { .. }
                | crate::agent::types::AgentEvent::GatewayIncoming { .. }
                | crate::agent::types::AgentEvent::BudgetAlert { .. }
                | crate::agent::types::AgentEvent::TrajectoryUpdate { .. }
                | crate::agent::types::AgentEvent::EpisodeRecorded { .. }
                | crate::agent::types::AgentEvent::OperatorQuestion { .. }
                | crate::agent::types::AgentEvent::OperatorQuestionResolved { .. }
        ),
    }
}

fn concierge_welcome_fingerprint(event: &crate::agent::types::AgentEvent) -> Option<String> {
    match event {
        crate::agent::types::AgentEvent::ConciergeWelcome {
            thread_id,
            content,
            detail_level,
            actions,
        } => serde_json::to_string(&(thread_id, content, detail_level, actions)).ok(),
        _ => None,
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

    let manager =
        SessionManager::new_with_history(history.clone(), agent_config.pty_channel_capacity);
    let reaper_manager = manager.clone();

    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(5));
        loop {
            interval.tick().await;
            reaper_manager.reap_dead().await;
        }
    });

    // Start agent engine
    let agent =
        AgentEngine::new_with_shared_history(manager.clone(), agent_config, history.clone());

    // Initialize plugin manager
    let plugins_dir = dirs::home_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join(".tamux")
        .join("plugins");
    let plugin_manager = Arc::new(crate::plugin::PluginManager::new(
        history.clone(),
        plugins_dir,
    ));
    let (pm_loaded, pm_skipped) = plugin_manager.load_all_from_disk().await;
    tracing::info!(
        loaded = pm_loaded,
        skipped = pm_skipped,
        "plugin loader complete"
    );

    // Wire plugin manager into agent engine for tool executor access (Phase 17)
    let _ = agent.plugin_manager.set(plugin_manager.clone());

    // Hydrate persisted state (threads, tasks, heartbeat, memory)
    if let Err(e) = agent.hydrate().await {
        tracing::warn!("failed to hydrate agent engine: {e}");
    }

    // Initialize the concierge (ensures pinned thread exists after hydration).
    agent.concierge.initialize(&agent.threads).await;

    #[cfg(not(test))]
    maybe_autostart_whatsapp_link(agent.clone()).await;

    // Start background loop (tasks + heartbeat)
    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
    let loop_agent = agent.clone();
    tokio::spawn(async move {
        Box::pin(loop_agent.run_loop(shutdown_rx)).await;
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
async fn run_unix(
    manager: Arc<SessionManager>,
    agent: Arc<AgentEngine>,
    plugin_manager: Arc<crate::plugin::PluginManager>,
) -> Result<()> {
    let path = socket_path();
    let listener = bind_unix_listener(&path)?;
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
fn bind_unix_listener(path: &std::path::Path) -> Result<tokio::net::UnixListener> {
    use std::io::ErrorKind;
    use std::os::unix::fs::FileTypeExt;

    if let Ok(metadata) = std::fs::symlink_metadata(path) {
        if !metadata.file_type().is_socket() {
            anyhow::bail!(
                "refusing to remove non-socket daemon path {}; delete it manually",
                path.display()
            );
        }

        match std::os::unix::net::UnixStream::connect(path) {
            Ok(stream) => {
                drop(stream);
                anyhow::bail!(
                    "daemon is already running on {}; stop the existing process before starting another instance",
                    path.display()
                );
            }
            Err(error)
                if matches!(
                    error.kind(),
                    ErrorKind::ConnectionRefused | ErrorKind::NotFound
                ) =>
            {
                std::fs::remove_file(path).with_context(|| {
                    format!("failed to remove stale daemon socket {}", path.display())
                })?;
            }
            Err(error) => {
                return Err(anyhow::Error::new(error).context(format!(
                    "failed to probe existing daemon socket {}",
                    path.display()
                )));
            }
        }
    }

    tokio::net::UnixListener::bind(path).map_err(|error| {
        if error.kind() == std::io::ErrorKind::AddrInUse {
            anyhow::anyhow!(
                "daemon is already running on {}; stop the existing process before starting another instance",
                path.display()
            )
        } else {
            anyhow::Error::new(error)
                .context(format!("failed to bind daemon Unix socket {}", path.display()))
        }
    })
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
                    if let Err(e) =
                        Box::pin(handle_connection(stream, manager, agent, plugin_manager)).await
                    {
                        if is_expected_disconnect_error(&e) {
                            tracing::debug!(error = %e, "client disconnected");
                        } else {
                            tracing::error!(error = %e, "client connection error");
                        }
                    }
                });
            }
            Err(e) => {
                tracing::error!(error = %e, "accept error");
            }
        }
    }
}

#[cfg(all(test, unix))]
mod unix_socket_tests {
    use super::bind_unix_listener;
    use std::os::unix::net::UnixListener;

    fn with_io_runtime<T>(f: impl FnOnce() -> T) -> T {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_io()
            .build()
            .expect("build tokio runtime");
        runtime.block_on(async move { f() })
    }

    #[test]
    fn bind_unix_listener_rejects_second_live_daemon() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("tamux-daemon.sock");
        let listener = UnixListener::bind(&path).expect("bind first daemon socket");

        let error = with_io_runtime(|| {
            bind_unix_listener(&path).expect_err("second daemon should be rejected")
        });
        let message = error.to_string();
        assert!(
            message.contains("already running"),
            "expected already-running error, got: {message}"
        );
        assert!(path.exists(), "live daemon socket path should remain intact");

        drop(listener);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn bind_unix_listener_replaces_stale_socket_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("tamux-daemon.sock");
        let listener = UnixListener::bind(&path).expect("bind initial socket");
        drop(listener);

        let rebound = with_io_runtime(|| {
            bind_unix_listener(&path).expect("stale socket should be replaced")
        });
        assert!(path.exists(), "rebound daemon socket should exist");

        drop(rebound);
        let _ = std::fs::remove_file(&path);
    }
}

// ---------------------------------------------------------------------------
// Windows IPC implementation
// ---------------------------------------------------------------------------

#[cfg(windows)]
async fn run_windows(
    manager: Arc<SessionManager>,
    agent: Arc<AgentEngine>,
    plugin_manager: Arc<crate::plugin::PluginManager>,
) -> Result<()> {
    let addr = amux_protocol::default_tcp_addr();
    tracing::info!(%addr, "daemon listening on TCP");
    run_tcp_fallback(manager, agent, plugin_manager).await
}

/// TCP server used for Windows IPC.
#[allow(dead_code)]
async fn run_tcp_fallback(
    manager: Arc<SessionManager>,
    agent: Arc<AgentEngine>,
    plugin_manager: Arc<crate::plugin::PluginManager>,
) -> Result<()> {
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
                    if let Err(e) =
                        Box::pin(handle_connection(stream, manager, agent, plugin_manager)).await
                    {
                        if is_expected_disconnect_error(&e) {
                            tracing::debug!(%addr, error = %e, "client disconnected");
                        } else {
                            tracing::error!(%addr, error = %e, "client connection error");
                        }
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
