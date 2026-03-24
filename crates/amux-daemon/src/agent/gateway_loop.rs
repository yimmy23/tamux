//! Gateway initialization, background run loop, and platform message polling.

use super::heartbeat::is_peak_activity_hour;
use super::*;
use chrono::Timelike;

impl AgentEngine {
    /// Initialize gateway connections for receiving messages.
    pub(crate) async fn init_gateway(&self) {
        let config = self.config.read().await.clone();
        let gw = &config.gateway;

        if !gw.enabled {
            tracing::info!("gateway: disabled in config, skipping initialization");
            return;
        }
        // D-02: Env var fallback for token migration from Electron
        let slack_token = if gw.slack_token.is_empty() {
            let val = std::env::var("AMUX_SLACK_TOKEN").unwrap_or_default();
            if !val.is_empty() {
                tracing::info!("gateway: using AMUX_SLACK_TOKEN env var (config.json empty)");
            }
            val
        } else {
            gw.slack_token.clone()
        };
        let telegram_token = if gw.telegram_token.is_empty() {
            let val = std::env::var("AMUX_TELEGRAM_TOKEN").unwrap_or_default();
            if !val.is_empty() {
                tracing::info!("gateway: using AMUX_TELEGRAM_TOKEN env var (config.json empty)");
            }
            val
        } else {
            gw.telegram_token.clone()
        };
        let discord_token = if gw.discord_token.is_empty() {
            let val = std::env::var("AMUX_DISCORD_TOKEN").unwrap_or_default();
            if !val.is_empty() {
                tracing::info!("gateway: using AMUX_DISCORD_TOKEN env var (config.json empty)");
            }
            val
        } else {
            gw.discord_token.clone()
        };
        let discord_channel_filter = gw.discord_channel_filter.clone();
        let slack_channel_filter = gw.slack_channel_filter.clone();

        let has_any = !slack_token.is_empty()
            || !telegram_token.is_empty()
            || !discord_token.is_empty()
            || !gw.whatsapp_token.is_empty();
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
            slack_channel_filter,
            telegram_token,
            telegram_allowed_chats: gw.telegram_allowed_chats.clone(),
            discord_token,
            discord_channel_filter,
            discord_allowed_users: gw.discord_allowed_users.clone(),
            whatsapp_allowed_contacts: gw.whatsapp_allowed_contacts.clone(),
            whatsapp_token: gw.whatsapp_token.clone(),
            whatsapp_phone_id: gw.whatsapp_phone_id.clone(),
            command_prefix: gw.command_prefix.clone(),
            gateway_electron_bridges_enabled: gw.gateway_electron_bridges_enabled,
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

    /// Reinitialize the gateway after a config change.
    /// Clears existing state, then re-initializes from current config.
    pub async fn reinit_gateway(&self) {
        let config = self.config.read().await.clone();
        let gw = &config.gateway;

        if !gw.enabled {
            tracing::info!("gateway: disabled by config, clearing state");
            *self.gateway_state.lock().await = None;
            *self.gateway_discord_channels.write().await = Vec::new();
            *self.gateway_slack_channels.write().await = Vec::new();
            return;
        }

        // Clear existing state before re-init so stale credentials don't persist
        *self.gateway_state.lock().await = None;
        *self.gateway_discord_channels.write().await = Vec::new();
        *self.gateway_slack_channels.write().await = Vec::new();

        self.init_gateway().await;
    }

    /// Spawn the tamux-gateway process if gateway tokens are configured.
    pub async fn maybe_spawn_gateway(&self) {
        let config = self.config.read().await.clone();
        let gw = &config.gateway;
        let slack_token = gw.slack_token.clone();
        let telegram_token = gw.telegram_token.clone();
        let discord_token = gw.discord_token.clone();

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

        let task_interval = std::time::Duration::from_secs(config.task_poll_interval_secs);
        let gateway_poll_interval = std::time::Duration::from_secs(3);
        let mut watcher_refresh_rx = self.watcher_refresh_rx.lock().await.take();

        let mut task_tick = tokio::time::interval(task_interval);
        let mut gateway_tick = tokio::time::interval(gateway_poll_interval);
        let mut watcher_tick =
            tokio::time::interval(std::time::Duration::from_millis(FILE_WATCH_TICK_MS));
        let mut supervisor_tick = tokio::time::interval(std::time::Duration::from_secs(30));
        let mut anticipatory_tick =
            tokio::time::interval(std::time::Duration::from_secs(ANTICIPATORY_TICK_SECS));
        let mut pending_watcher_refreshes: HashMap<String, Instant> = HashMap::new();

        // Cron-based heartbeat scheduling (D-06, BEAT-01)
        let heartbeat_cron_expr = super::heartbeat::resolve_cron_from_config(&config);
        let agent_backend = config.agent_backend;
        let mut heartbeat_cron: croner::Cron = heartbeat_cron_expr
            .parse()
            .unwrap_or_else(|_| "*/15 * * * *".parse().unwrap());
        let mut next_heartbeat = {
            let now_local = chrono::Local::now();
            heartbeat_cron
                .find_next_occurrence(&now_local, false)
                .map(|dt| {
                    let dur = (dt - now_local)
                        .to_std()
                        .unwrap_or(std::time::Duration::from_secs(900));
                    tokio::time::Instant::now() + dur
                })
                .unwrap_or_else(|_| {
                    tokio::time::Instant::now() + std::time::Duration::from_secs(900)
                })
        };

        // Ephemeral heartbeat cycle counter for priority-weight gating (per D-06).
        // Resets on daemon restart (Open Question 2: ephemeral is sufficient).
        let mut heartbeat_cycle_count: u64 = 0;

        tracing::info!(
            task_poll_secs = config.task_poll_interval_secs,
            heartbeat_cron = %heartbeat_cron_expr,
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
                _ = tokio::time::sleep_until(next_heartbeat) => {
                    heartbeat_cycle_count += 1;
                    let is_quiet = self.is_quiet_hours().await;
                    if !is_quiet {
                        let config_snap = self.config.read().await.clone();
                        let model = self.operator_model.read().await;
                        let current_hour = chrono::Utc::now().hour() as u8;
                        let session_count = model.session_rhythm.session_count;

                        // Cold start protection (Pitfall 1): skip adaptive scheduling
                        // until enough sessions have been observed.
                        let in_peak = if session_count < 5 {
                            true // Treat all hours as peak during cold start
                        } else {
                            is_peak_activity_hour(
                                current_hour,
                                &model.session_rhythm.peak_activity_hours_utc,
                                &model.session_rhythm.smoothed_activity_histogram,
                                config_snap.ema_activity_threshold,
                            )
                        };
                        drop(model); // Release read lock before heartbeat execution

                        if in_peak {
                            // Full frequency: run heartbeat normally during peak activity
                            if let Err(e) = self.run_structured_heartbeat_adaptive(heartbeat_cycle_count).await {
                                tracing::error!("agent heartbeat error: {e}");
                            }
                        } else {
                            // Reduced frequency: skip cycles during low-activity periods (per D-03)
                            let skip_factor = config_snap.low_activity_frequency_factor;
                            if skip_factor == 0 || heartbeat_cycle_count % skip_factor == 0 {
                                if let Err(e) = self.run_structured_heartbeat_adaptive(heartbeat_cycle_count).await {
                                    tracing::error!("agent heartbeat error: {e}");
                                }
                            } else {
                                tracing::debug!(
                                    cycle = heartbeat_cycle_count,
                                    skip_factor = skip_factor,
                                    "heartbeat skipped (low-activity period)"
                                );
                            }
                        }
                    } else {
                        tracing::debug!("heartbeat suppressed (quiet hours/DND)");
                    }

                    // Check for tier changes and disclosure (Phase 10 Plan 03)
                    if let Err(e) = self.check_tier_change().await {
                        tracing::warn!(error = %e, "tier change check failed");
                    }

                    // Deliver next feature disclosure if pending
                    {
                        let session_count = self.operator_model.read().await.session_count;
                        let mut queue = self.disclosure_queue.write().await;
                        if let Err(e) = self.concierge.deliver_next_disclosure(&mut queue, session_count).await {
                            tracing::warn!(error = %e, "feature disclosure delivery failed");
                        }
                    }

                    // Recompute next occurrence AFTER heartbeat completes (Pitfall 1)
                    let now_local = chrono::Local::now();
                    next_heartbeat = heartbeat_cron
                        .find_next_occurrence(&now_local, false)
                        .map(|dt| {
                            let dur = (dt - now_local)
                                .to_std()
                                .unwrap_or(std::time::Duration::from_secs(900));
                            tokio::time::Instant::now() + dur
                        })
                        .unwrap_or_else(|_| {
                            tokio::time::Instant::now() + std::time::Duration::from_secs(900)
                        });
                }
                // Config hot-reload: recompute heartbeat schedule (Pitfall 5)
                _ = self.config_notify.notified() => {
                    let new_cron_expr = self.resolve_heartbeat_cron().await;
                    if let Ok(new_cron) = new_cron_expr.parse::<croner::Cron>() {
                        heartbeat_cron = new_cron;
                        let now_local = chrono::Local::now();
                        next_heartbeat = heartbeat_cron
                            .find_next_occurrence(&now_local, false)
                            .map(|dt| {
                                let dur = (dt - now_local)
                                    .to_std()
                                    .unwrap_or(std::time::Duration::from_secs(900));
                                tokio::time::Instant::now() + dur
                            })
                            .unwrap_or_else(|_| {
                                tokio::time::Instant::now() + std::time::Duration::from_secs(900)
                            });
                        tracing::info!(cron = %new_cron_expr, "heartbeat schedule updated");
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
                            ).await {
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
    ///
    /// Each platform poll is wrapped with health tracking (Plan 02):
    /// - Check `should_retry` before polling (backoff skip)
    /// - Call `on_success`/`on_failure` based on result
    /// - Emit `GatewayStatus` event on status transitions
    /// - Emit `HeartbeatDigest` on connected/disconnected transitions (D-05)
    /// - Store thread contexts from incoming messages into `reply_contexts`
    /// - Track `last_incoming_at` per channel
    /// - Respect Slack 60s poll interval (Pitfall 1)
    async fn poll_gateway_messages(&self) {
        use super::gateway_health::GatewayConnectionStatus;

        let mut gw_guard = self.gateway_state.lock().await;
        let gw = match gw_guard.as_mut() {
            Some(g) => g,
            None => return,
        };

        let now_ms = now_millis();

        // Use cached channel lists (populated by init_gateway) instead of
        // repeatedly touching config storage every poll cycle.
        let discord_channels = self.gateway_discord_channels.read().await.clone();
        let slack_channels = self.gateway_slack_channels.read().await.clone();

        // Collect messages from all platforms
        let mut incoming = Vec::new();
        // Track status transitions to emit events after dropping the mutex
        let mut status_transitions: Vec<(String, String, Option<String>, Option<u32>)> = Vec::new();

        // --- Telegram ---
        if !gw.config.telegram_token.is_empty() {
            if gw.telegram_health.should_retry(now_ms) {
                let old_status = gw.telegram_health.status;
                match gateway::poll_telegram(gw).await {
                    Ok(telegram_msgs) => {
                        gw.telegram_health.on_success(now_ms);
                        if !telegram_msgs.is_empty() {
                            tracing::info!(
                                count = telegram_msgs.len(),
                                "gateway: telegram messages received"
                            );
                        }
                        // Store thread contexts and update last_incoming_at
                        for msg in &telegram_msgs {
                            if let Some(ref tc) = msg.thread_context {
                                let key = format!("Telegram:{}", msg.channel);
                                gw.reply_contexts.insert(key.clone(), tc.clone());
                                gw.last_incoming_at.insert(key, now_ms);
                            }
                        }
                        incoming.extend(telegram_msgs);
                    }
                    Err(e) => {
                        gw.telegram_health.on_failure(now_ms, e.to_string());
                        tracing::warn!("gateway: telegram poll error: {e}");
                    }
                }
                if gw.telegram_health.status_changed(old_status) {
                    status_transitions.push((
                        "Telegram".to_string(),
                        format!("{:?}", gw.telegram_health.status).to_lowercase(),
                        gw.telegram_health.last_error.clone(),
                        Some(gw.telegram_health.consecutive_failure_count),
                    ));
                }
            } else {
                tracing::debug!(
                    backoff_secs = gw.telegram_health.current_backoff_secs,
                    "gateway: telegram poll skipped (backoff active)"
                );
            }
        }

        // --- Slack (60s interval per Pitfall 1) ---
        if !slack_channels.is_empty() && !gw.config.slack_token.is_empty() {
            let slack_interval_ms = gw.slack_poll_interval_secs * 1000;
            let should_poll_slack = match gw.last_slack_poll_ms {
                Some(last) => now_ms.saturating_sub(last) >= slack_interval_ms,
                None => true, // First poll
            };

            if should_poll_slack && gw.slack_health.should_retry(now_ms) {
                gw.last_slack_poll_ms = Some(now_ms);
                let old_status = gw.slack_health.status;
                match gateway::poll_slack(gw, &slack_channels).await {
                    Ok(slack_msgs) => {
                        gw.slack_health.on_success(now_ms);
                        if !slack_msgs.is_empty() {
                            tracing::info!(count = slack_msgs.len(), "gateway: slack messages received");
                        }
                        for msg in &slack_msgs {
                            if let Some(ref tc) = msg.thread_context {
                                let key = format!("Slack:{}", msg.channel);
                                gw.reply_contexts.insert(key.clone(), tc.clone());
                                gw.last_incoming_at.insert(key, now_ms);
                            }
                        }
                        incoming.extend(slack_msgs);
                    }
                    Err(e) => {
                        gw.slack_health.on_failure(now_ms, e.to_string());
                        tracing::warn!("gateway: slack poll error: {e}");
                    }
                }
                if gw.slack_health.status_changed(old_status) {
                    status_transitions.push((
                        "Slack".to_string(),
                        format!("{:?}", gw.slack_health.status).to_lowercase(),
                        gw.slack_health.last_error.clone(),
                        Some(gw.slack_health.consecutive_failure_count),
                    ));
                }
            } else if !should_poll_slack {
                tracing::debug!(
                    interval_secs = gw.slack_poll_interval_secs,
                    "gateway: slack poll skipped (interval not elapsed)"
                );
            } else {
                tracing::debug!(
                    backoff_secs = gw.slack_health.current_backoff_secs,
                    "gateway: slack poll skipped (backoff active)"
                );
            }
        }

        // --- Discord ---
        if !discord_channels.is_empty() && !gw.config.discord_token.is_empty() {
            if gw.discord_health.should_retry(now_ms) {
                let old_status = gw.discord_health.status;
                match gateway::poll_discord(gw, &discord_channels).await {
                    Ok(discord_msgs) => {
                        gw.discord_health.on_success(now_ms);
                        if !discord_msgs.is_empty() {
                            tracing::info!(
                                count = discord_msgs.len(),
                                "gateway: discord messages received"
                            );
                        }
                        for msg in &discord_msgs {
                            if let Some(ref tc) = msg.thread_context {
                                let key = format!("Discord:{}", msg.channel);
                                gw.reply_contexts.insert(key.clone(), tc.clone());
                                gw.last_incoming_at.insert(key, now_ms);
                            }
                        }
                        incoming.extend(discord_msgs);
                    }
                    Err(e) => {
                        gw.discord_health.on_failure(now_ms, e.to_string());
                        tracing::warn!("gateway: discord poll error: {e}");
                    }
                }
                if gw.discord_health.status_changed(old_status) {
                    status_transitions.push((
                        "Discord".to_string(),
                        format!("{:?}", gw.discord_health.status).to_lowercase(),
                        gw.discord_health.last_error.clone(),
                        Some(gw.discord_health.consecutive_failure_count),
                    ));
                }
            } else {
                tracing::debug!(
                    backoff_secs = gw.discord_health.current_backoff_secs,
                    "gateway: discord poll skipped (backoff active)"
                );
            }
        }

        // Drop the mutex before dispatching events (send_message needs it indirectly)
        drop(gw_guard);

        // Emit GatewayStatus events and HeartbeatDigest for status transitions
        for (platform, status, last_error, consecutive_failures) in &status_transitions {
            // Emit GatewayStatus event to all connected clients
            let _ = self.event_tx.send(AgentEvent::GatewayStatus {
                platform: platform.clone(),
                status: status.clone(),
                last_error: last_error.clone(),
                consecutive_failures: *consecutive_failures,
            });

            // D-05: Emit HeartbeatDigest on connected/disconnected transitions
            let is_connect_disconnect = status == "connected" || status == "disconnected" || status == "error";
            if is_connect_disconnect {
                let description = match (status.as_str(), last_error) {
                    ("connected", _) => format!("{platform} reconnected"),
                    ("error", Some(err)) => {
                        let fail_count = consecutive_failures.unwrap_or(0);
                        format!("{platform} disconnected after {fail_count} failures: {err}")
                    }
                    ("error", None) => format!("{platform} disconnected"),
                    ("disconnected", _) => format!("{platform} disconnected"),
                    _ => format!("{platform} status: {status}"),
                };

                let _ = self.event_tx.send(AgentEvent::HeartbeatDigest {
                    cycle_id: format!("gateway_health_{}", Uuid::new_v4()),
                    actionable: status != "connected",
                    digest: description.clone(),
                    items: vec![HeartbeatDigestItem {
                        priority: if status == "connected" { 3 } else { 1 },
                        check_type: HeartbeatCheckType::UnrepliedGatewayMessages,
                        title: description,
                        suggestion: if status == "connected" {
                            format!("{platform} is back online")
                        } else {
                            format!("Check {platform} API credentials and connectivity")
                        },
                    }],
                    checked_at: now_ms,
                    explanation: None,
                    confidence: None,
                });

                // Audit entry for health transition per D-05
                let audit_entry = crate::history::AuditEntryRow {
                    id: format!("gw_health_{}", Uuid::new_v4()),
                    timestamp: now_ms as i64,
                    action_type: "gateway_health_transition".to_string(),
                    summary: format!("{platform} -> {status}"),
                    explanation: last_error.clone(),
                    confidence: None,
                    confidence_band: None,
                    causal_trace_id: None,
                    thread_id: None,
                    goal_run_id: None,
                    task_id: None,
                    raw_data_json: Some(serde_json::json!({
                        "platform": platform,
                        "new_status": status,
                        "consecutive_failures": consecutive_failures,
                    }).to_string()),
                };
                if let Err(e) = self.history.insert_action_audit(&audit_entry).await {
                    tracing::warn!(platform = %platform, "gateway: failed to persist health audit: {e}");
                }
            }
        }

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
                self.gateway_inflight_channels
                    .lock()
                    .await
                    .remove(&channel_key);
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

            // Triage via concierge — simple messages get a direct response,
            // complex ones fall through to the full agent loop.
            let triage = self
                .concierge
                .triage_gateway_message(
                    &msg.platform,
                    &msg.sender,
                    &msg.content,
                    &self.threads,
                    &self.tasks,
                )
                .await;

            match triage {
                concierge::GatewayTriage::Simple(response_text) => {
                    tracing::info!(
                        platform = %msg.platform,
                        sender = %msg.sender,
                        "gateway: concierge handled simple message"
                    );
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
                        id: format!("concierge_{}", uuid::Uuid::new_v4()),
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
                        None,
                    )
                    .await;
                    self.gateway_inflight_channels
                        .lock()
                        .await
                        .remove(&channel_key);
                    continue;
                }
                concierge::GatewayTriage::Complex => {
                    // Fall through to full agent loop below
                }
            }

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
                                    None,
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
            self.gateway_inflight_channels
                .lock()
                .await
                .remove(&channel_key);
        }
    }
}
