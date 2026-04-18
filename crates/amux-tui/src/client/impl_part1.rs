impl DaemonClient {
    fn next_bootstrap_attempted(bootstrap_attempted: bool, connected: bool) -> bool {
        if connected {
            false
        } else {
            bootstrap_attempted
        }
    }

    async fn probe_daemon_once() -> bool {
        #[cfg(unix)]
        {
            for socket_path in Self::unix_socket_candidates() {
                if let Ok(stream) = UnixStream::connect(&socket_path).await {
                    drop(stream);
                    return true;
                }
            }
            false
        }

        #[cfg(not(unix))]
        {
            let addr = Self::resolve_daemon_addr(&default_tcp_addr());
            tokio::net::TcpStream::connect(&addr).await.is_ok()
        }
    }

    fn daemon_spawn_candidates() -> Vec<std::ffi::OsString> {
        let mut candidates = Vec::new();

        if let Some(path) = std::env::var_os("TAMUX_DAEMON_BIN") {
            if !path.is_empty() {
                candidates.push(path);
            }
        }

        if let Ok(current_exe) = std::env::current_exe() {
            let exe_name = if cfg!(windows) {
                "tamux-daemon.exe"
            } else {
                "tamux-daemon"
            };
            candidates.push(current_exe.with_file_name(exe_name).into_os_string());
        }

        candidates.push(std::ffi::OsString::from(if cfg!(windows) {
            "tamux-daemon.exe"
        } else {
            "tamux-daemon"
        }));
        candidates
    }

    async fn attempt_spawn_daemon() -> bool {
        for candidate in Self::daemon_spawn_candidates() {
            let candidate_display = candidate.to_string_lossy().to_string();
            info!(candidate = %candidate_display, "Attempting daemon bootstrap");

            let spawn_result = tokio::process::Command::new(&candidate)
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn();

            match spawn_result {
                Ok(child) => {
                    drop(child);
                    for _ in 0..20 {
                        tokio::time::sleep(Duration::from_millis(250)).await;
                        if Self::probe_daemon_once().await {
                            info!(candidate = %candidate_display, "Daemon bootstrap succeeded");
                            return true;
                        }
                    }
                    warn!(candidate = %candidate_display, "Daemon bootstrap command ran but daemon never became ready");
                }
                Err(err) => {
                    debug!(candidate = %candidate_display, error = %err, "Daemon bootstrap spawn failed");
                }
            }
        }

        false
    }

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
            let mut bootstrap_attempted = false;

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

                bootstrap_attempted = Self::next_bootstrap_attempted(bootstrap_attempted, connected);

                if !connected && !bootstrap_attempted {
                    bootstrap_attempted = true;
                    if Self::attempt_spawn_daemon().await {
                        continue;
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
        let mut thread_detail_chunks = None;

        for request in [
            ClientMessage::AgentSubscribe,
            ClientMessage::AgentDeclareAsyncCommandCapability {
                capability: amux_protocol::AsyncCommandCapability {
                    version: 1,
                    supports_operation_acceptance: true,
                },
            },
            ClientMessage::AgentListThreads {
                limit: None,
                offset: None,
            },
        ] {
            if let Err(err) = amux_protocol::validate_client_message_size(&request) {
                error!("Rejected initial oversized daemon request: {}", err);
                let _ = event_tx
                    .send(ClientEvent::Error(format!("Protocol error: {}", err)))
                    .await;
                let _ = event_tx.send(ClientEvent::Disconnected).await;
                return;
            }
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
                            if !Self::handle_daemon_message(
                                message,
                                &event_tx,
                                &mut thread_detail_chunks,
                            )
                            .await
                            {
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
                            if let Err(err) = amux_protocol::validate_client_message_size(&request) {
                                let _ = event_tx
                                    .send(ClientEvent::Error(format!("Send error: {}", err)))
                                    .await;
                                continue;
                            }
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
        thread_detail_chunks: &mut Option<ThreadDetailChunkBuffer>,
    ) -> bool {
        match message {
            message @ (DaemonMessage::AgentEvent { .. }
            | DaemonMessage::AgentThreadList { .. }
            | DaemonMessage::AgentThreadDetail { .. }
            | DaemonMessage::AgentThreadDetailChunk { .. }
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
            | DaemonMessage::AgentTaskApprovalRules { .. }
            | DaemonMessage::ApprovalResolved { .. }) => {
                Self::handle_daemon_message_part1(message, event_tx, thread_detail_chunks).await
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
            | DaemonMessage::AgentThreadMessagePinResult { .. }
            | DaemonMessage::AgentWhatsAppLinkQr { .. }
            | DaemonMessage::AgentWhatsAppLinked { .. }
            | DaemonMessage::AgentWhatsAppLinkError { .. }
            | DaemonMessage::AgentWhatsAppLinkDisconnected { .. }
            | DaemonMessage::AgentExplanation { .. }
            | DaemonMessage::AgentDivergentSessionStarted { .. }
            | DaemonMessage::AgentDivergentSession { .. }
            | DaemonMessage::AgentStatusResponse { .. }
            | DaemonMessage::AgentStatisticsResponse { .. }
            | DaemonMessage::AgentPromptInspection { .. }
            | DaemonMessage::AgentOperatorProfileSessionStarted { .. }
            | DaemonMessage::AgentOperatorProfileQuestion { .. }
            | DaemonMessage::AgentOperatorProfileProgress { .. }
            | DaemonMessage::AgentOperatorProfileSummary { .. }
            | DaemonMessage::AgentOperatorModel { .. }
            | DaemonMessage::AgentOperatorModelReset { .. }
            | DaemonMessage::AgentCollaborationSessions { .. }
            | DaemonMessage::AgentCollaborationVoteResult { .. }
            | DaemonMessage::AgentGeneratedTools { .. }
            | DaemonMessage::AgentSpeechToTextResult { .. }
            | DaemonMessage::AgentTextToSpeechResult { .. }
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
