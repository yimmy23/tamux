//! Gateway initialization, background run loop, and platform message polling.

use super::*;

impl AgentEngine {
    /// Initialize gateway connections for receiving messages.
    pub(crate) async fn init_gateway(&self) {
        let config = self.config.read().await.clone();
        let gw = &config.gateway;

        // Read settings.json once and extract all gateway-related values
        let (
            slack_token,
            telegram_token,
            discord_token,
            discord_channel_filter,
            slack_channel_filter,
        ) = if !gw.slack_token.is_empty()
            || !gw.telegram_token.is_empty()
            || !gw.discord_token.is_empty()
        {
            (
                gw.slack_token.clone(),
                gw.telegram_token.clone(),
                gw.discord_token.clone(),
                String::new(),
                String::new(),
            )
        } else {
            let settings_path = self
                .data_dir
                .parent()
                .unwrap_or(std::path::Path::new("."))
                .join("settings.json");
            match tokio::fs::read_to_string(&settings_path).await {
                Ok(raw) => {
                    let v: serde_json::Value = serde_json::from_str(&raw).unwrap_or_default();
                    (
                        read_setting_str(&v, "slackToken"),
                        read_setting_str(&v, "telegramToken"),
                        read_setting_str(&v, "discordToken"),
                        read_setting_str(&v, "discordChannelFilter"),
                        read_setting_str(&v, "slackChannelFilter"),
                    )
                }
                Err(_) => (
                    String::new(),
                    String::new(),
                    String::new(),
                    String::new(),
                    String::new(),
                ),
            }
        };

        let has_any =
            !slack_token.is_empty() || !telegram_token.is_empty() || !discord_token.is_empty();
        if !has_any {
            tracing::info!("gateway: no platform tokens, polling disabled");
            return;
        }

        // Parse channel lists from the already-read settings
        if !discord_channel_filter.is_empty() {
            tracing::info!(discord_filter = %discord_channel_filter, "gateway: discordChannelFilter");
            let channels: Vec<String> = discord_channel_filter
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
            *self.gateway_discord_channels.write().await = channels;
        }
        if !slack_channel_filter.is_empty() {
            let channels: Vec<String> = slack_channel_filter
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
            *self.gateway_slack_channels.write().await = channels;
        }

        let gw_config = GatewayConfig {
            enabled: true,
            slack_token,
            telegram_token,
            discord_token,
            command_prefix: gw.command_prefix.clone(),
        };

        let dc = self.gateway_discord_channels.read().await.clone();
        let sc = self.gateway_slack_channels.read().await.clone();

        tracing::info!(
            has_slack = !gw_config.slack_token.is_empty(),
            has_telegram = !gw_config.telegram_token.is_empty(),
            has_discord = !gw_config.discord_token.is_empty(),
            discord_channels = ?dc,
            slack_channels = ?sc,
            "gateway: config loaded"
        );

        *self.gateway_state.lock().await = Some(gateway::GatewayState::new(
            gw_config,
            self.http_client.clone(),
        ));

        tracing::info!("gateway: polling initialized in daemon");
    }

    /// Spawn the tamux-gateway process if gateway tokens are configured.
    pub async fn maybe_spawn_gateway(&self) {
        let config = self.config.read().await.clone();
        let gw = &config.gateway;

        // Also try reading tokens from the frontend settings.json as fallback
        let (slack_token, telegram_token, discord_token) = if !gw.slack_token.is_empty()
            || !gw.telegram_token.is_empty()
            || !gw.discord_token.is_empty()
        {
            (
                gw.slack_token.clone(),
                gw.telegram_token.clone(),
                gw.discord_token.clone(),
            )
        } else {
            // Read from ~/.tamux/settings.json (frontend persistence)
            let settings_path = self
                .data_dir
                .parent()
                .unwrap_or(std::path::Path::new("."))
                .join("settings.json");
            match tokio::fs::read_to_string(&settings_path).await {
                Ok(raw) => {
                    let v: serde_json::Value = serde_json::from_str(&raw).unwrap_or_default();
                    (
                        read_setting_str(&v, "slackToken"),
                        read_setting_str(&v, "telegramToken"),
                        read_setting_str(&v, "discordToken"),
                    )
                }
                Err(_) => (String::new(), String::new(), String::new()),
            }
        };

        if slack_token.is_empty() && telegram_token.is_empty() && discord_token.is_empty() {
            tracing::info!("gateway: no platform tokens configured, skipping");
            return;
        }

        // Find the gateway binary next to the daemon binary
        let gateway_path = std::env::current_exe().ok().and_then(|p| {
            let dir = p.parent()?;
            let name = if cfg!(windows) {
                "tamux-gateway.exe"
            } else {
                "tamux-gateway"
            };
            let path = dir.join(name);
            if path.exists() {
                Some(path)
            } else {
                None
            }
        });

        let gateway_path = match gateway_path {
            Some(p) => p,
            None => {
                tracing::warn!("gateway binary not found next to daemon executable");
                return;
            }
        };

        // Kill existing gateway process if any
        {
            let mut proc = self.gateway_process.lock().await;
            if let Some(ref mut child) = *proc {
                let _ = child.kill().await;
            }
            *proc = None;
        }

        tracing::info!(?gateway_path, "spawning gateway process");

        let mut cmd = tokio::process::Command::new(&gateway_path);
        cmd.stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());

        if !slack_token.is_empty() {
            cmd.env("AMUX_SLACK_TOKEN", &slack_token);
        }
        if !telegram_token.is_empty() {
            cmd.env("AMUX_TELEGRAM_TOKEN", &telegram_token);
        }
        if !discord_token.is_empty() {
            cmd.env("AMUX_DISCORD_TOKEN", &discord_token);
        }

        match cmd.spawn() {
            Ok(child) => {
                tracing::info!(pid = ?child.id(), "gateway process started");
                *self.gateway_process.lock().await = Some(child);
            }
            Err(e) => {
                tracing::error!(error = %e, "failed to spawn gateway process");
            }
        }
    }

    /// Stop the gateway process.
    pub async fn stop_gateway(&self) {
        let mut proc = self.gateway_process.lock().await;
        if let Some(ref mut child) = *proc {
            tracing::info!("stopping gateway process");
            let _ = child.kill().await;
        }
        *proc = None;
    }

    /// Main background loop — processes tasks, runs heartbeats, polls gateway.
    pub async fn run_loop(self: Arc<Self>, mut shutdown: tokio::sync::watch::Receiver<bool>) {
        let config = self.config.read().await.clone();
        let agent_backend = config.agent_backend;

        let task_interval = std::time::Duration::from_secs(config.task_poll_interval_secs);
        let heartbeat_interval =
            std::time::Duration::from_secs(config.heartbeat_interval_mins * 60);
        let gateway_poll_interval = std::time::Duration::from_secs(3);
        let mut watcher_refresh_rx = self.watcher_refresh_rx.lock().await.take();

        let mut task_tick = tokio::time::interval(task_interval);
        let mut heartbeat_tick = tokio::time::interval(heartbeat_interval);
        let mut gateway_tick = tokio::time::interval(gateway_poll_interval);
        let mut watcher_tick =
            tokio::time::interval(std::time::Duration::from_millis(FILE_WATCH_TICK_MS));
        let mut supervisor_tick = tokio::time::interval(std::time::Duration::from_secs(30));
        let mut anticipatory_tick =
            tokio::time::interval(std::time::Duration::from_secs(ANTICIPATORY_TICK_SECS));
        let mut pending_watcher_refreshes: HashMap<String, Instant> = HashMap::new();

        tracing::info!(
            task_poll_secs = config.task_poll_interval_secs,
            heartbeat_mins = config.heartbeat_interval_mins,
            "agent background loop started"
        );

        loop {
            tokio::select! {
                _ = task_tick.tick() => {
                    self.clone().dispatch_goal_runs().await;
                    if let Err(e) = self.clone().dispatch_ready_tasks().await {
                        tracing::error!("agent task error: {e}");
                    }
                }
                _ = heartbeat_tick.tick() => {
                    if let Err(e) = self.run_heartbeat().await {
                        tracing::error!("agent heartbeat error: {e}");
                    }
                }
                _ = anticipatory_tick.tick() => {
                    self.run_anticipatory_tick().await;
                }
                _ = gateway_tick.tick() => {
                    // Skip built-in gateway polling when using an external agent
                    // — the external agent handles its own gateway connections
                    if !matches!(agent_backend, AgentBackend::Openclaw | AgentBackend::Hermes) {
                        self.poll_gateway_messages().await;
                    }
                }
                maybe_thread_id = async {
                    match watcher_refresh_rx.as_mut() {
                        Some(rx) => rx.recv().await,
                        None => std::future::pending::<Option<String>>().await,
                    }
                } => {
                    if let Some(thread_id) = maybe_thread_id {
                        pending_watcher_refreshes.insert(
                            thread_id,
                            Instant::now() + Duration::from_millis(FILE_WATCH_DEBOUNCE_MS),
                        );
                    }
                }
                _ = watcher_tick.tick() => {
                    if pending_watcher_refreshes.is_empty() {
                        continue;
                    }

                    let now = Instant::now();
                    let due_threads = pending_watcher_refreshes
                        .iter()
                        .filter_map(|(thread_id, deadline)| {
                            if *deadline <= now {
                                Some(thread_id.clone())
                            } else {
                                None
                            }
                        })
                        .collect::<Vec<_>>();

                    for thread_id in due_threads {
                        pending_watcher_refreshes.remove(&thread_id);
                        self.refresh_thread_repo_context(&thread_id).await;
                    }
                }
                _ = supervisor_tick.tick() => {
                    let supervised: Vec<_> = {
                        let tasks = self.tasks.lock().await;
                        tasks.iter()
                            .filter(|t| t.status == TaskStatus::InProgress && t.supervisor_config.is_some())
                            .cloned()
                            .collect()
                    };

                    let now_secs = now_millis() / 1000;
                    for task in supervised {
                        self.ensure_subagent_runtime(&task, task.thread_id.as_deref()).await;
                        let Some(snapshot) = self.subagent_snapshot(&task).await else {
                            continue;
                        };
                        let action = crate::agent::subagent::supervisor::check_health(
                            &snapshot,
                            task.supervisor_config
                                .as_ref()
                                .expect("supervised task must have config"),
                            now_secs,
                        );
                        let new_state = action
                            .as_ref()
                            .map(|value| value.health_state)
                            .unwrap_or(SubagentHealthState::Healthy);
                        let previous_state = {
                            let runtime = self.subagent_runtime.read().await;
                            runtime
                                .get(&task.id)
                                .map(|entry| entry.health_state)
                                .unwrap_or(SubagentHealthState::Healthy)
                        };

                        if previous_state != new_state {
                            self.update_subagent_health(&task.id, new_state).await;
                            let runtime = {
                                let runtime = self.subagent_runtime.read().await;
                                runtime.get(&task.id).cloned()
                            };
                            let indicators_json = runtime.as_ref().map(|entry| {
                                serde_json::json!({
                                    "last_progress_at": entry.last_progress_at,
                                    "tool_call_frequency": if now_secs > entry.started_at / 1000 {
                                        entry.tool_calls_total as f64 / ((now_secs - entry.started_at / 1000) as f64 / 60.0).max(1.0)
                                    } else {
                                        0.0
                                    },
                                    "error_rate": if entry.tool_calls_total == 0 {
                                        0.0
                                    } else {
                                        entry.tool_calls_failed as f64 / entry.tool_calls_total as f64
                                    },
                                    "context_growth_rate": 0.0,
                                    "context_utilization_pct": entry.context_utilization_pct,
                                    "consecutive_errors": entry.consecutive_errors,
                                    "total_tool_calls": entry.tool_calls_total,
                                    "successful_tool_calls": entry.tool_calls_succeeded,
                                })
                                .to_string()
                            });
                            if let Err(e) = self.history.insert_health_log(
                                &format!("health_{}", Uuid::new_v4()),
                                "task",
                                &task.id,
                                match new_state {
                                    SubagentHealthState::Healthy => "healthy",
                                    SubagentHealthState::Degraded => "degraded",
                                    SubagentHealthState::Stuck => "stuck",
                                    SubagentHealthState::Crashed => "crashed",
                                },
                                indicators_json.as_deref(),
                                action
                                    .as_ref()
                                    .map(|value| format!("{:?}", value.action))
                                    .as_deref(),
                                now_millis(),
                            ) {
                                tracing::warn!(task_id = %task.id, "failed to persist health log: {e}");
                            }
                            let _ = self.event_tx.send(AgentEvent::SubagentHealthChange {
                                task_id: task.id.clone(),
                                previous_state,
                                new_state,
                                reason: action.as_ref().and_then(|value| value.reason),
                                intervention: action.as_ref().map(|value| value.action),
                            });
                        }
                        self.persist_subagent_runtime_metrics(&task.id).await;
                    }
                }
                _ = shutdown.changed() => {
                    tracing::info!("agent background loop shutting down");
                    self.stop_gateway().await;
                    self.stop_external_agents().await;
                    break;
                }
            }
        }
    }

    /// Poll all gateway platforms for incoming messages and route to agent.
    async fn poll_gateway_messages(&self) {
        let mut gw_guard = self.gateway_state.lock().await;
        let gw = match gw_guard.as_mut() {
            Some(g) => g,
            None => return,
        };

        // Use cached channel lists (populated by init_gateway) instead of
        // re-reading settings.json from disk every poll cycle.
        let discord_channels = self.gateway_discord_channels.read().await.clone();
        let slack_channels = self.gateway_slack_channels.read().await.clone();

        // Collect messages from all platforms
        let mut incoming = Vec::new();

        if !gw.config.telegram_token.is_empty() {
            let telegram_msgs = gateway::poll_telegram(gw).await;
            if !telegram_msgs.is_empty() {
                tracing::info!(
                    count = telegram_msgs.len(),
                    "gateway: telegram messages received"
                );
            }
            incoming.extend(telegram_msgs);
        }

        if !slack_channels.is_empty() && !gw.config.slack_token.is_empty() {
            let slack_msgs = gateway::poll_slack(gw, &slack_channels).await;
            if !slack_msgs.is_empty() {
                tracing::info!(count = slack_msgs.len(), "gateway: slack messages received");
            }
            incoming.extend(slack_msgs);
        }

        if !discord_channels.is_empty() && !gw.config.discord_token.is_empty() {
            let discord_msgs = gateway::poll_discord(gw, &discord_channels).await;
            if !discord_msgs.is_empty() {
                tracing::info!(
                    count = discord_msgs.len(),
                    "gateway: discord messages received"
                );
            }
            incoming.extend(discord_msgs);
        }

        // Drop the mutex before processing (send_message needs it indirectly)
        drop(gw_guard);

        // Route each message to the agent
        for msg in incoming {
            // --- Deduplication: skip messages we've already processed ---
            if let Some(ref mid) = msg.message_id {
                let mut seen = self.gateway_seen_ids.lock().await;
                if seen.contains(mid) {
                    tracing::debug!(
                        message_id = %mid,
                        platform = %msg.platform,
                        "gateway: skipping duplicate message"
                    );
                    continue;
                }
                seen.push(mid.clone());
                // Cap at 200 entries to prevent unbounded growth
                if seen.len() > 200 {
                    let excess = seen.len() - 200;
                    seen.drain(..excess);
                }
            }

            // --- Per-channel lock: prevent concurrent processing of same channel ---
            let channel_key = format!("{}:{}", msg.platform, msg.channel);
            {
                let mut inflight = self.gateway_inflight_channels.lock().await;
                if inflight.contains(&channel_key) {
                    tracing::warn!(
                        channel_key = %channel_key,
                        "gateway: channel already being processed, skipping"
                    );
                    continue;
                }
                inflight.insert(channel_key.clone());
            }

            tracing::info!(
                platform = %msg.platform,
                sender = %msg.sender,
                channel = %msg.channel,
                content = %msg.content,
                message_id = ?msg.message_id,
                "gateway: incoming message"
            );

            // Handle control commands (reset/new conversation)
            let trimmed = msg.content.trim().to_lowercase();
            if trimmed == "!reset" || trimmed == "!new" || trimmed == "reset" || trimmed == "new" {
                self.gateway_threads.write().await.remove(&channel_key);
                tracing::info!(channel_key = %channel_key, "gateway: conversation reset");

                // Send confirmation back
                let prompt = format!(
                    "The user typed '{}' in {} channel {}. \
                     This means they want to start a fresh conversation. \
                     Send a brief confirmation back using {} saying the conversation has been reset.",
                    msg.content, msg.platform, msg.channel,
                    match msg.platform.as_str() {
                        "Discord" => format!("send_discord_message with channel_id=\"{}\"", msg.channel),
                        "Slack" => format!("send_slack_message with channel=\"{}\"", msg.channel),
                        "Telegram" => format!("send_telegram_message with chat_id=\"{}\"", msg.channel),
                        _ => "the appropriate gateway tool".to_string(),
                    }
                );

                if let Err(e) = self.send_message(None, &prompt).await {
                    tracing::error!(error = %e, "gateway: failed to send reset confirmation");
                }
                self.gateway_inflight_channels.lock().await.remove(&channel_key);
                continue;
            }

            let (reply_tool, reply_tool_name) = match msg.platform.as_str() {
                "Discord" => (
                    format!("send_discord_message with channel_id=\"{}\"", msg.channel),
                    "send_discord_message",
                ),
                "Slack" => (
                    format!("send_slack_message with channel=\"{}\"", msg.channel),
                    "send_slack_message",
                ),
                "Telegram" => (
                    format!("send_telegram_message with chat_id=\"{}\"", msg.channel),
                    "send_telegram_message",
                ),
                "WhatsApp" => (
                    format!("send_whatsapp_message with phone=\"{}\"", msg.channel),
                    "send_whatsapp_message",
                ),
                _ => (
                    "the appropriate gateway tool".to_string(),
                    "send_discord_message",
                ),
            };

            let prompt = format!(
                "[{platform} message from {sender}]: {content}\n\n\
                 YOU MUST CALL {reply_tool} to reply. Do NOT just write a text response — \
                 the user is on {platform} and will ONLY see messages sent via the tool. \
                 Your text response here is invisible to them. \
                 If you use other tools first (bash, read_file, etc), that's fine, \
                 but your FINAL action MUST be calling {reply_tool_short} to send the reply.",
                platform = msg.platform,
                sender = msg.sender,
                content = msg.content,
                reply_tool = reply_tool,
                reply_tool_short = reply_tool_name,
            );

            // Notify frontend about the incoming message (full content)
            let _ = self.event_tx.send(AgentEvent::GatewayIncoming {
                platform: msg.platform.clone(),
                sender: msg.sender.clone(),
                content: msg.content.clone(),
                channel: msg.channel.clone(),
            });

            // Use persistent thread per channel for conversation continuity
            let existing_thread = self.gateway_threads.read().await.get(&channel_key).cloned();

            match self.send_message(existing_thread.as_deref(), &prompt).await {
                Ok(thread_id) => {
                    // Store the mapping so follow-up messages use the same thread
                    self.gateway_threads
                        .write()
                        .await
                        .insert(channel_key.clone(), thread_id.clone());

                    // Safety net: if the agent didn't call the gateway send tool,
                    // auto-send the last assistant message to the platform
                    let threads = self.threads.read().await;
                    if let Some(thread) = threads.get(&thread_id) {
                        let used_gateway_tool = thread.messages.iter().any(|m| {
                            m.role == MessageRole::Tool
                                && m.tool_name
                                    .as_deref()
                                    .map(|n| n.starts_with("send_"))
                                    .unwrap_or(false)
                        });

                        if !used_gateway_tool {
                            // Find the last assistant text response
                            let last_response = thread
                                .messages
                                .iter()
                                .rev()
                                .find(|m| m.role == MessageRole::Assistant && !m.content.is_empty())
                                .map(|m| m.content.clone());

                            if let Some(response_text) = last_response {
                                tracing::info!(
                                    platform = %msg.platform,
                                    "gateway: agent forgot to call send tool, auto-sending response"
                                );
                                drop(threads);

                                // Auto-send via the gateway tool
                                let auto_args = match msg.platform.as_str() {
                                    "Discord" => {
                                        serde_json::json!({"channel_id": msg.channel, "message": response_text})
                                    }
                                    "Slack" => {
                                        serde_json::json!({"channel": msg.channel, "message": response_text})
                                    }
                                    "Telegram" => {
                                        serde_json::json!({"chat_id": msg.channel, "message": response_text})
                                    }
                                    "WhatsApp" => {
                                        serde_json::json!({"phone": msg.channel, "message": response_text})
                                    }
                                    _ => serde_json::json!({"message": response_text}),
                                };

                                let auto_tool = ToolCall {
                                    id: format!("auto_{}", uuid::Uuid::new_v4()),
                                    function: ToolFunction {
                                        name: reply_tool_name.to_string(),
                                        arguments: auto_args.to_string(),
                                    },
                                };

                                let _ = tool_executor::execute_tool(
                                    &auto_tool,
                                    self,
                                    "",
                                    None,
                                    &self.session_manager,
                                    None,
                                    &self.event_tx,
                                    &self.data_dir,
                                    &self.http_client,
                                )
                                .await;
                            }
                        }
                    }
                }
                Err(e) => {
                    tracing::error!(
                        platform = %msg.platform,
                        error = %e,
                        "gateway: failed to process incoming message"
                    );
                }
            }

            // Release per-channel processing lock
            self.gateway_inflight_channels.lock().await.remove(&channel_key);
        }
    }
}
