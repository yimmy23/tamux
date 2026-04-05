impl DaemonClient {
    pub fn new(event_tx: mpsc::Sender<ClientEvent>) -> Self {
        let (request_tx, request_rx) = mpsc::unbounded_channel();
        Self {
            event_tx,
            request_tx,
            request_rx: Mutex::new(Some(request_rx)),
        }
    }

    pub async fn connect(&self) -> Result<()> {
        let event_tx = self.event_tx.clone();
        let Some(mut request_rx) = self
            .request_rx
            .lock()
            .expect("request mutex poisoned")
            .take()
        else {
            return Ok(());
        };

        tokio::spawn(async move {
            let retry_delay = Duration::from_secs(5);

            loop {
                let mut connected = false;
                #[cfg(unix)]
                {
                    for socket_path in Self::unix_socket_candidates() {
                        info!(path = %socket_path.display(), "Attempting daemon unix socket");
                        match UnixStream::connect(&socket_path).await {
                            Ok(stream) => {
                                info!("Connected to daemon via unix socket");
                                let _ = event_tx.send(ClientEvent::Connected).await;
                                let framed = Framed::new(stream, AmuxCodec);
                                Self::handle_connection(framed, event_tx.clone(), &mut request_rx)
                                    .await;
                                connected = true;
                                break;
                            }
                            Err(err) => {
                                debug!(path = %socket_path.display(), error = %err, "Unix socket connect failed");
                            }
                        }
                    }
                }

                #[cfg(not(unix))]
                {
                    let addr = Self::resolve_daemon_addr(&default_tcp_addr());
                    info!(%addr, "Attempting daemon tcp socket");
                    match tokio::net::TcpStream::connect(&addr).await {
                        Ok(stream) => {
                            info!("Connected to daemon via tcp {}", addr);
                            let _ = event_tx.send(ClientEvent::Connected).await;
                            let framed = Framed::new(stream, AmuxCodec);
                            Self::handle_connection(framed, event_tx.clone(), &mut request_rx)
                                .await;
                            connected = true;
                        }
                        Err(err) => {
                            warn!("Cannot connect to daemon at {} ({})", addr, err);
                            let _ = event_tx.send(ClientEvent::Disconnected).await;
                        }
                    }
                }

                let _ = event_tx
                    .send(ClientEvent::Reconnecting {
                        delay_secs: retry_delay.as_secs(),
                    })
                    .await;

                if connected {
                    info!(
                        "Daemon connection closed; retrying in {}s",
                        retry_delay.as_secs()
                    );
                }
                tokio::time::sleep(retry_delay).await;
            }
        });

        Ok(())
    }

    #[cfg(not(unix))]
    fn resolve_daemon_addr(default_addr: &str) -> String {
        #[cfg(target_os = "linux")]
        {
            if std::path::Path::new("/proc/sys/fs/binfmt_misc/WSLInterop").exists()
                || std::path::Path::new("/run/WSL").exists()
            {
                if let Ok(contents) = std::fs::read_to_string("/etc/resolv.conf") {
                    for line in contents.lines() {
                        if line.starts_with("nameserver") {
                            if let Some(host_ip) = line.split_whitespace().nth(1) {
                                let port = default_addr.split(':').nth(1).unwrap_or("17563");
                                return format!("{}:{}", host_ip, port);
                            }
                        }
                    }
                }
            }
        }

        default_addr.to_string()
    }

    #[cfg(unix)]
    fn unix_socket_candidates() -> Vec<std::path::PathBuf> {
        let mut candidates = Vec::new();

        if let Ok(runtime_dir) = std::env::var("XDG_RUNTIME_DIR") {
            candidates.push(std::path::PathBuf::from(runtime_dir).join("tamux-daemon.sock"));
        }

        candidates.push(std::path::PathBuf::from("/tmp").join("tamux-daemon.sock"));
        candidates.dedup();
        candidates
    }

    async fn handle_connection<S>(
        framed: Framed<S, AmuxCodec>,
        event_tx: mpsc::Sender<ClientEvent>,
        request_rx: &mut mpsc::UnboundedReceiver<ClientMessage>,
    ) where
        S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + 'static,
    {
        let (mut sink, mut stream) = framed.split();
        let keepalive_interval = Duration::from_secs(5);
        let keepalive_timeout = Duration::from_secs(10);
        let mut ping_tick = tokio::time::interval(keepalive_interval);
        ping_tick.set_missed_tick_behavior(MissedTickBehavior::Delay);
        let mut last_inbound_at = Instant::now();
        let mut awaiting_pong_since: Option<Instant> = None;

        for request in [
            ClientMessage::AgentSubscribe,
            ClientMessage::AgentDeclareAsyncCommandCapability {
                capability: amux_protocol::AsyncCommandCapability {
                    version: 1,
                    supports_operation_acceptance: true,
                },
            },
            ClientMessage::AgentListThreads,
        ] {
            if let Err(err) = sink.send(request).await {
                error!("Failed initial daemon request: {}", err);
                let _ = event_tx
                    .send(ClientEvent::Error(format!("Protocol error: {}", err)))
                    .await;
                let _ = event_tx.send(ClientEvent::Disconnected).await;
                return;
            }
        }

        loop {
            tokio::select! {
                inbound = stream.next() => {
                    match inbound {
                        Some(Ok(message)) => {
                            last_inbound_at = Instant::now();
                            awaiting_pong_since = None;
                            if !Self::handle_daemon_message(message, &event_tx).await {
                                break;
                            }
                        }
                        Some(Err(err)) => {
                            let _ = event_tx.send(ClientEvent::Error(format!("Connection error: {}", err))).await;
                            break;
                        }
                        None => break,
                    }
                }
                _ = ping_tick.tick() => {
                    let now = Instant::now();
                    if let Some(pending_since) = awaiting_pong_since {
                        if now.duration_since(pending_since) >= keepalive_timeout {
                            let _ = event_tx
                                .send(ClientEvent::Error(
                                    "Connection lost: daemon health-check timed out".to_string(),
                                ))
                                .await;
                            break;
                        }
                    }

                    if now.duration_since(last_inbound_at) >= keepalive_interval {
                        if let Err(err) = sink.send(ClientMessage::Ping).await {
                            let _ = event_tx
                                .send(ClientEvent::Error(format!("Keepalive send error: {}", err)))
                                .await;
                            break;
                        }
                        awaiting_pong_since = Some(now);
                    }
                }
                outbound = request_rx.recv() => {
                    match outbound {
                        Some(request) => {
                            if let Err(err) = sink.send(request).await {
                                let _ = event_tx.send(ClientEvent::Error(format!("Send error: {}", err))).await;
                                break;
                            }
                        }
                        None => break,
                    }
                }
            }
        }

        let _ = event_tx.send(ClientEvent::Disconnected).await;
    }

    async fn handle_daemon_message(
        message: DaemonMessage,
        event_tx: &mpsc::Sender<ClientEvent>,
    ) -> bool {
        match message {
            message @ (DaemonMessage::AgentEvent { .. }
            | DaemonMessage::AgentThreadList { .. }
            | DaemonMessage::AgentThreadDetail { .. }
            | DaemonMessage::AgentTaskList { .. }
            | DaemonMessage::AgentGoalRunList { .. }
            | DaemonMessage::AgentGoalRunStarted { .. }
            | DaemonMessage::AgentGoalRunDetail { .. }
            | DaemonMessage::AgentCheckpointList { .. }
            | DaemonMessage::AgentCheckpointRestored { .. }
            | DaemonMessage::AgentTodoDetail { .. }
            | DaemonMessage::AgentWorkContextDetail { .. }
            | DaemonMessage::GitDiff { .. }
            | DaemonMessage::FilePreview { .. }
            | DaemonMessage::AgentConfigResponse { .. }
            | DaemonMessage::AgentModelsResponse { .. }
            | DaemonMessage::AgentHeartbeatItems { .. }
            | DaemonMessage::AgentEventRows { .. }
            | DaemonMessage::AgentDbMessageAck
            | DaemonMessage::SessionSpawned { .. }
            | DaemonMessage::ApprovalRequired { .. }
            | DaemonMessage::ApprovalResolved { .. }) => {
                Self::handle_daemon_message_part1(message, event_tx).await
            }
            message @ (DaemonMessage::AgentProviderAuthStates { .. }
            | DaemonMessage::AgentOpenAICodexAuthStatus { .. }
            | DaemonMessage::AgentOpenAICodexAuthLoginResult { .. }
            | DaemonMessage::AgentOpenAICodexAuthLogoutResult { .. }
            | DaemonMessage::AgentProviderValidation { .. }
            | DaemonMessage::AgentSubAgentList { .. }
            | DaemonMessage::AgentSubAgentUpdated { .. }
            | DaemonMessage::AgentSubAgentRemoved { .. }
            | DaemonMessage::AgentConciergeConfig { .. }
            | DaemonMessage::PluginListResult { .. }
            | DaemonMessage::PluginGetResult { .. }
            | DaemonMessage::PluginSettingsResult { .. }
            | DaemonMessage::PluginTestConnectionResult { .. }
            | DaemonMessage::PluginActionResult { .. }
            | DaemonMessage::PluginCommandsResult { .. }
            | DaemonMessage::PluginOAuthUrl { .. }
            | DaemonMessage::PluginOAuthComplete { .. }) => {
                Self::handle_daemon_message_part2(message, event_tx).await
            }
            message @ (DaemonMessage::AgentWhatsAppLinkStatus { .. }
            | DaemonMessage::AgentWhatsAppLinkQr { .. }
            | DaemonMessage::AgentWhatsAppLinked { .. }
            | DaemonMessage::AgentWhatsAppLinkError { .. }
            | DaemonMessage::AgentWhatsAppLinkDisconnected { .. }
            | DaemonMessage::AgentExplanation { .. }
            | DaemonMessage::AgentDivergentSessionStarted { .. }
            | DaemonMessage::AgentDivergentSession { .. }
            | DaemonMessage::AgentStatusResponse { .. }
            | DaemonMessage::AgentOperatorProfileSessionStarted { .. }
            | DaemonMessage::AgentOperatorProfileQuestion { .. }
            | DaemonMessage::AgentOperatorProfileProgress { .. }
            | DaemonMessage::AgentOperatorProfileSummary { .. }
            | DaemonMessage::AgentOperatorModel { .. }
            | DaemonMessage::AgentOperatorModelReset { .. }
            | DaemonMessage::AgentCollaborationSessions { .. }
            | DaemonMessage::AgentGeneratedTools { .. }
            | DaemonMessage::AgentOperatorProfileSessionCompleted { .. }
            | DaemonMessage::AgentError { .. }
            | DaemonMessage::GatewayBootstrap { .. }
            | DaemonMessage::GatewaySendRequest { .. }
            | DaemonMessage::GatewayReloadCommand { .. }
            | DaemonMessage::GatewayShutdownCommand { .. }
            | DaemonMessage::Error { .. }) => {
                Self::handle_daemon_message_part3(message, event_tx).await
            }
            other => {
                debug!("Ignoring daemon message: {:?}", other);
            }
        }

        true
    }

}
