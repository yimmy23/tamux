//! Gateway initialization, background run loop, and platform message polling.

use super::heartbeat::is_peak_activity_hour;
use super::*;
use chrono::Timelike;

fn is_gateway_reset_command(trimmed_lower: &str) -> bool {
    matches!(trimmed_lower, "!reset" | "!new")
}

const GATEWAY_TRIAGE_TIMEOUT_SECS: u64 = 12;
const GATEWAY_AGENT_TIMEOUT_SECS: u64 = 120;

// ---------------------------------------------------------------------------
// Replay classification
// ---------------------------------------------------------------------------

/// Classification of a single replayed message envelope.
#[derive(Debug, PartialEq, Clone, Copy)]
pub(crate) enum ReplayMessageClassification {
    /// New message — route to agent and advance cursor.
    Accepted,
    /// Already in the seen-IDs ring buffer — advance cursor but skip routing.
    Duplicate,
    /// Filtered out (empty content, bot, etc.) — advance cursor but skip routing.
    Filtered,
}

/// Classify a replay envelope for cursor-advancement and routing decisions.
///
/// Returns `None` when the envelope is malformed (empty `cursor_value` or
/// `channel_id`); in that case replay should stop immediately.
fn classify_replay_envelope(
    env: &gateway::ReplayEnvelope,
    seen_ids: &[String],
) -> Option<ReplayMessageClassification> {
    if env.cursor_value.is_empty() || env.channel_id.is_empty() {
        return None;
    }
    if let Some(ref mid) = env.message.message_id {
        if seen_ids.contains(mid) {
            return Some(ReplayMessageClassification::Duplicate);
        }
    }
    if env.message.content.trim().is_empty() {
        return Some(ReplayMessageClassification::Filtered);
    }
    Some(ReplayMessageClassification::Accepted)
}

/// Update the in-memory replay cursor in `GatewayState` for a platform/channel.
fn update_in_memory_replay_cursor(
    platform: &str,
    gw: &mut gateway::GatewayState,
    channel_id: &str,
    cursor_value: &str,
) {
    match platform {
        "telegram" => {
            if let Ok(v) = cursor_value.parse::<i64>() {
                gw.telegram_replay_cursor = Some(v);
            }
        }
        "slack" => {
            gw.slack_replay_cursors
                .insert(channel_id.to_string(), cursor_value.to_string());
        }
        "discord" => {
            gw.discord_replay_cursors
                .insert(channel_id.to_string(), cursor_value.to_string());
        }
        _ => {}
    }
}

/// Process a `ReplayFetchResult` for a named platform.
///
/// - `InitializeBoundary`: persists the boundary cursor and returns immediately
///   with no messages to route (skip-backlog on first connect).
/// - `Replay(envelopes)`: classifies each envelope; advances the persisted
///   cursor for accepted / duplicate / filtered items; stops and returns
///   `completed = false` if a malformed (unclassified) envelope is encountered.
///
/// Returns `(messages_to_route, completed)`.
pub(crate) async fn process_replay_result(
    history: &crate::history::HistoryStore,
    platform: &str,
    result: gateway::ReplayFetchResult,
    gw: &mut gateway::GatewayState,
    seen_ids: &mut Vec<String>,
) -> (Vec<gateway::IncomingMessage>, bool) {
    match result {
        gateway::ReplayFetchResult::InitializeBoundary {
            channel_id,
            cursor_value,
            cursor_type,
        } => {
            if let Err(e) = history
                .save_gateway_replay_cursor(platform, &channel_id, &cursor_value, cursor_type)
                .await
            {
                tracing::warn!(
                    platform,
                    channel_id,
                    "replay: failed to persist init boundary: {e}"
                );
            }
            update_in_memory_replay_cursor(platform, gw, &channel_id, &cursor_value);
            (Vec::new(), true)
        }

        gateway::ReplayFetchResult::Replay(envelopes) => {
            let mut messages = Vec::new();
            for env in envelopes {
                match classify_replay_envelope(&env, seen_ids) {
                    None => {
                        tracing::warn!(
                            platform,
                            cursor_value = %env.cursor_value,
                            channel_id = %env.channel_id,
                            "replay: malformed envelope, stopping replay"
                        );
                        return (messages, false);
                    }
                    Some(
                        ReplayMessageClassification::Duplicate
                        | ReplayMessageClassification::Filtered,
                    ) => {
                        if let Err(e) = history
                            .save_gateway_replay_cursor(
                                platform,
                                &env.channel_id,
                                &env.cursor_value,
                                env.cursor_type,
                            )
                            .await
                        {
                            tracing::warn!(
                                platform,
                                channel_id = %env.channel_id,
                                "replay: cursor persist failed: {e}"
                            );
                        }
                        update_in_memory_replay_cursor(
                            platform,
                            gw,
                            &env.channel_id,
                            &env.cursor_value,
                        );
                    }
                    Some(ReplayMessageClassification::Accepted) => {
                        if let Some(ref mid) = env.message.message_id {
                            seen_ids.push(mid.clone());
                            if seen_ids.len() > 200 {
                                let excess = seen_ids.len() - 200;
                                seen_ids.drain(..excess);
                            }
                        }
                        if let Err(e) = history
                            .save_gateway_replay_cursor(
                                platform,
                                &env.channel_id,
                                &env.cursor_value,
                                env.cursor_type,
                            )
                            .await
                        {
                            tracing::warn!(
                                platform,
                                channel_id = %env.channel_id,
                                "replay: cursor persist failed: {e}"
                            );
                        }
                        update_in_memory_replay_cursor(
                            platform,
                            gw,
                            &env.channel_id,
                            &env.cursor_value,
                        );
                        messages.push(env.message);
                    }
                }
            }
            (messages, true)
        }
    }
}

impl AgentEngine {
    /// Apply a batch of pre-fetched replay results across one or more platforms,
    /// updating the shared `gateway_seen_ids` ring buffer with the IDs of every
    /// accepted message so that the live-path duplicate check sees them.
    ///
    /// `platform_results` is a list of `(platform_name, ReplayFetchResult)` pairs
    /// collected by the caller before holding the gateway-state lock.  Completed
    /// platforms are removed from `gw.replay_cycle_active`.
    ///
    /// Returns the accumulated messages to prepend to the live queue.
    pub(crate) async fn apply_replay_results(
        &self,
        platform_results: Vec<(String, gateway::ReplayFetchResult)>,
        gw: &mut gateway::GatewayState,
    ) -> Vec<gateway::IncomingMessage> {
        let mut seen_ids_snap = self.gateway_seen_ids.lock().await.clone();
        let mut replay_msgs: Vec<gateway::IncomingMessage> = Vec::new();

        for (platform, result) in platform_results {
            let (msgs, completed) =
                process_replay_result(&self.history, &platform, result, gw, &mut seen_ids_snap)
                    .await;
            if completed {
                gw.replay_cycle_active.remove(platform.as_str());
                tracing::info!(
                    platform = %platform,
                    replay_count = msgs.len(),
                    "gateway: replay cycle complete"
                );
            }
            replay_msgs.extend(msgs);
        }

        // Write the updated snapshot back to the shared ring buffer so that
        // live-path deduplication sees all IDs from this replay cycle.
        *self.gateway_seen_ids.lock().await = seen_ids_snap;

        replay_msgs
    }

    pub async fn enqueue_gateway_message(&self, msg: gateway::IncomingMessage) -> Result<()> {
        let enabled = {
            let guard = self.gateway_state.lock().await;
            guard
                .as_ref()
                .map(|state| state.config.enabled)
                .unwrap_or(false)
        };
        if !enabled {
            anyhow::bail!("gateway polling is not initialized");
        }

        let mut queue = self.gateway_injected_messages.lock().await;
        queue.push_back(msg);
        if queue.len() > 500 {
            let excess = queue.len() - 500;
            queue.drain(..excess);
        }
        Ok(())
    }

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

        let has_poll_tokens = !slack_token.is_empty()
            || !telegram_token.is_empty()
            || !discord_token.is_empty()
            || !gw.whatsapp_token.is_empty();
        if !has_poll_tokens {
            tracing::info!(
                "gateway: no platform poll tokens configured; enabling injected-only gateway mode"
            );
        }

        self.gateway_injected_messages.lock().await.clear();

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
            whatsapp_link_fallback_electron: gw.whatsapp_link_fallback_electron,
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

        let telegram_replay_cursor = match self.history.load_gateway_replay_cursors("telegram").await {
            Ok(rows) => rows
                .into_iter()
                .filter(|row| row.channel_id == "global")
                .find_map(|row| match row.cursor_value.parse::<i64>() {
                    Ok(cursor) => Some(cursor),
                    Err(error) => {
                        tracing::warn!(
                            channel_id = %row.channel_id,
                            cursor_value = %row.cursor_value,
                            %error,
                            "gateway: ignoring invalid telegram replay cursor"
                        );
                        None
                    }
                }),
            Err(error) => {
                tracing::warn!(%error, "gateway: failed to load telegram replay cursors");
                None
            }
        };
        let slack_replay_cursors = match self.history.load_gateway_replay_cursors("slack").await {
            Ok(rows) => rows
                .into_iter()
                .map(|row| (row.channel_id, row.cursor_value))
                .collect(),
            Err(error) => {
                tracing::warn!(%error, "gateway: failed to load slack replay cursors");
                HashMap::new()
            }
        };
        let discord_replay_cursors =
            match self.history.load_gateway_replay_cursors("discord").await {
                Ok(rows) => rows
                    .into_iter()
                    .map(|row| (row.channel_id, row.cursor_value))
                    .collect(),
                Err(error) => {
                    tracing::warn!(%error, "gateway: failed to load discord replay cursors");
                    HashMap::new()
                }
            };
        let whatsapp_replay_cursors =
            match self.history.load_gateway_replay_cursors("whatsapp").await {
                Ok(rows) => rows
                    .into_iter()
                    .map(|row| (row.channel_id, row.cursor_value))
                    .collect(),
                Err(error) => {
                    tracing::warn!(%error, "gateway: failed to load whatsapp replay cursors");
                    HashMap::new()
                }
            };

        let mut gateway_state = gateway::GatewayState::new(gw_config, self.http_client.clone());
        gateway_state.telegram_replay_cursor = telegram_replay_cursor;
        gateway_state.slack_replay_cursors = slack_replay_cursors;
        gateway_state.discord_replay_cursors = discord_replay_cursors;
        gateway_state.whatsapp_replay_cursors = whatsapp_replay_cursors;

        *self.gateway_state.lock().await = Some(gateway_state);

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

        // Collect messages from all platforms and externally injected sources.
        let mut incoming: Vec<gateway::IncomingMessage> = {
            let mut queue = self.gateway_injected_messages.lock().await;
            queue.drain(..).collect()
        };
        // Track status transitions to emit events after dropping the mutex
        let mut status_transitions: Vec<(String, String, Option<String>, Option<u32>)> = Vec::new();

        // --- Telegram ---
        if !gw.config.telegram_token.is_empty() {
            if gw.telegram_health.should_retry(now_ms) {
                let old_status = gw.telegram_health.status;
                match gateway::poll_telegram(gw).await {
                    Ok(telegram_msgs) => {
                        gw.telegram_health.on_success(now_ms);
                        if gw.telegram_health.is_reconnect_transition(old_status) {
                            gw.replay_cycle_active.insert("telegram".to_string());
                        }
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
                        // Persist live boundary so future reconnect replays start from here.
                        if gw.telegram_offset > 0 {
                            let cursor_val = (gw.telegram_offset - 1).to_string();
                            if let Err(e) = self
                                .history
                                .save_gateway_replay_cursor(
                                    "telegram",
                                    "global",
                                    &cursor_val,
                                    "update_id",
                                )
                                .await
                            {
                                tracing::warn!(
                                    "gateway: failed to persist telegram live cursor: {e}"
                                );
                            }
                        }
                    }
                    Err(e) => {
                        gw.telegram_health.on_failure(now_ms, e.to_string());
                        gw.replay_cycle_active.remove("telegram");
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
                        if gw.slack_health.is_reconnect_transition(old_status) {
                            gw.replay_cycle_active.insert("slack".to_string());
                        }
                        if !slack_msgs.is_empty() {
                            tracing::info!(
                                count = slack_msgs.len(),
                                "gateway: slack messages received"
                            );
                        }
                        for msg in &slack_msgs {
                            if let Some(ref tc) = msg.thread_context {
                                let key = format!("Slack:{}", msg.channel);
                                gw.reply_contexts.insert(key.clone(), tc.clone());
                                gw.last_incoming_at.insert(key, now_ms);
                            }
                        }
                        incoming.extend(slack_msgs);
                        // Persist live boundaries for all polled Slack channels.
                        for (channel_id, ts) in gw.slack_last_ts.clone() {
                            if let Err(e) = self
                                .history
                                .save_gateway_replay_cursor("slack", &channel_id, &ts, "message_ts")
                                .await
                            {
                                tracing::warn!(
                                    channel_id,
                                    "gateway: failed to persist slack live cursor: {e}"
                                );
                            }
                        }
                    }
                    Err(e) => {
                        gw.slack_health.on_failure(now_ms, e.to_string());
                        gw.replay_cycle_active.remove("slack");
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
                        if gw.discord_health.is_reconnect_transition(old_status) {
                            gw.replay_cycle_active.insert("discord".to_string());
                        }
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
                        // Persist live boundaries for all polled Discord channels.
                        for (channel_id, msg_id) in gw.discord_last_id.clone() {
                            if let Err(e) = self
                                .history
                                .save_gateway_replay_cursor(
                                    "discord",
                                    &channel_id,
                                    &msg_id,
                                    "message_id",
                                )
                                .await
                            {
                                tracing::warn!(
                                    channel_id,
                                    "gateway: failed to persist discord live cursor: {e}"
                                );
                            }
                        }
                    }
                    Err(e) => {
                        gw.discord_health.on_failure(now_ms, e.to_string());
                        gw.replay_cycle_active.remove("discord");
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

        // --- Replay orchestration ---
        // For each platform that just reconnected (replay_cycle_active), fetch and
        // process messages missed during the outage before routing live messages.
        {
            let platforms_for_replay: Vec<String> =
                gw.replay_cycle_active.clone().into_iter().collect();
            if !platforms_for_replay.is_empty() {
                let mut seen_ids_snap: Vec<String> =
                    self.gateway_seen_ids.lock().await.clone();
                let mut replay_msgs: Vec<gateway::IncomingMessage> = Vec::new();

                for platform in &platforms_for_replay {
                    let (msgs, completed) = match platform.as_str() {
                        "telegram" => {
                            match gateway::fetch_telegram_replay(gw).await {
                                Ok(result) => {
                                    process_replay_result(
                                        &self.history,
                                        platform,
                                        result,
                                        gw,
                                        &mut seen_ids_snap,
                                    )
                                    .await
                                }
                                Err(e) => {
                                    tracing::warn!("replay: telegram fetch failed: {e}");
                                    (Vec::new(), false)
                                }
                            }
                        }
                        "slack" => {
                            let mut all_msgs = Vec::new();
                            let mut all_completed = true;
                            for ch in &slack_channels {
                                match gateway::fetch_slack_replay(gw, ch).await {
                                    Ok(result) => {
                                        let (ch_msgs, ch_done) = process_replay_result(
                                            &self.history,
                                            platform,
                                            result,
                                            gw,
                                            &mut seen_ids_snap,
                                        )
                                        .await;
                                        all_msgs.extend(ch_msgs);
                                        if !ch_done {
                                            all_completed = false;
                                            break;
                                        }
                                    }
                                    Err(e) => {
                                        tracing::warn!(
                                            channel = ch,
                                            "replay: slack fetch failed: {e}"
                                        );
                                        all_completed = false;
                                        break;
                                    }
                                }
                            }
                            (all_msgs, all_completed)
                        }
                        "discord" => {
                            let mut all_msgs = Vec::new();
                            let mut all_completed = true;
                            for ch in &discord_channels {
                                match gateway::fetch_discord_replay(gw, ch).await {
                                    Ok(result) => {
                                        let (ch_msgs, ch_done) = process_replay_result(
                                            &self.history,
                                            platform,
                                            result,
                                            gw,
                                            &mut seen_ids_snap,
                                        )
                                        .await;
                                        all_msgs.extend(ch_msgs);
                                        if !ch_done {
                                            all_completed = false;
                                            break;
                                        }
                                    }
                                    Err(e) => {
                                        tracing::warn!(
                                            channel = ch,
                                            "replay: discord fetch failed: {e}"
                                        );
                                        all_completed = false;
                                        break;
                                    }
                                }
                            }
                            (all_msgs, all_completed)
                        }
                        _ => (Vec::new(), true),
                    };

                    if completed {
                        gw.replay_cycle_active.remove(platform.as_str());
                        tracing::info!(
                            platform,
                            replay_count = msgs.len(),
                            "gateway: replay cycle complete"
                        );
                    }
                    replay_msgs.extend(msgs);
                }

                // Bug 1 fix: write the updated seen-IDs snapshot back to the
                // shared ring buffer so that live-path deduplication sees all
                // IDs that were accepted during this replay cycle.
                *self.gateway_seen_ids.lock().await = seen_ids_snap;

                // Prepend replay messages so they are processed before live messages.
                if !replay_msgs.is_empty() {
                    replay_msgs.extend(std::mem::take(&mut incoming));
                    incoming = replay_msgs;
                }
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
            let is_connect_disconnect =
                status == "connected" || status == "disconnected" || status == "error";
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
                    raw_data_json: Some(
                        serde_json::json!({
                            "platform": platform,
                            "new_status": status,
                            "consecutive_failures": consecutive_failures,
                        })
                        .to_string(),
                    ),
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
            if is_gateway_reset_command(&trimmed) {
                self.gateway_threads.write().await.remove(&channel_key);
                if let Err(error) = self.history.delete_gateway_thread_binding(&channel_key).await {
                    tracing::warn!(channel_key = %channel_key, %error, "gateway: failed to persist reset binding");
                }
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
                 Recent channel history is auto-injected for continuity. \
                 If you need more context, call fetch_gateway_history with a larger count.\n\
                 YOU MUST CALL {reply_tool} to reply. Do NOT just write a text response — \
                 the user is on {platform} and will ONLY see messages sent via the tool. \
                 Your text response here is invisible to them.\n\
                 IMPORTANT: If you need to use tools before replying (bash, read_file, \
                 web_search, etc.), FIRST call {reply_tool_short} with a brief acknowledgment \
                 like \"On it, give me a moment...\" so the user knows you're working. \
                 Then do your work, then call {reply_tool_short} again with the full answer.\n\
                 Your FINAL action MUST be calling {reply_tool_short} to send the reply.",
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
            let triage = match tokio::time::timeout(
                std::time::Duration::from_secs(GATEWAY_TRIAGE_TIMEOUT_SECS),
                self.concierge.triage_gateway_message(
                    &msg.platform,
                    &msg.sender,
                    &msg.content,
                    &self.threads,
                    &self.tasks,
                ),
            )
            .await
            {
                Ok(result) => result,
                Err(_) => {
                    tracing::warn!(
                        platform = %msg.platform,
                        channel = %msg.channel,
                        timeout_secs = GATEWAY_TRIAGE_TIMEOUT_SECS,
                        "gateway: concierge triage timed out; falling back to full agent loop"
                    );
                    concierge::GatewayTriage::Complex
                }
            };

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
                    let tool_result = tool_executor::execute_tool(
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
                    if tool_result.is_error {
                        tracing::error!(
                            platform = %msg.platform,
                            channel = %msg.channel,
                            error = %tool_result.content,
                            "gateway: failed to send concierge simple response"
                        );
                    }
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
            let history_window = if let Some(ref tid) = existing_thread {
                match self.history.list_recent_messages(tid, 10).await {
                    Ok(messages) if !messages.is_empty() => {
                        let mut lines = Vec::with_capacity(messages.len() + 2);
                        lines.push(
                            "Previous 10 messages from this channel (oldest first):".to_string(),
                        );
                        for m in messages {
                            let role = m.role;
                            let content = m
                                .content
                                .replace('\n', " ")
                                .chars()
                                .take(240)
                                .collect::<String>();
                            lines.push(format!("- {role}: {content}"));
                        }
                        Some(lines.join("\n"))
                    }
                    Ok(_) => None,
                    Err(error) => {
                        tracing::warn!(channel_key = %channel_key, %error, "gateway: failed to load recent history");
                        None
                    }
                }
            } else {
                None
            };

            let enriched_prompt = if let Some(window) = history_window {
                format!("{prompt}\n\n{window}")
            } else {
                prompt
            };

            let send_result = tokio::time::timeout(
                std::time::Duration::from_secs(GATEWAY_AGENT_TIMEOUT_SECS),
                self.send_message(existing_thread.as_deref(), &enriched_prompt),
            )
            .await;

            match send_result {
                Err(_) => {
                    tracing::error!(
                        platform = %msg.platform,
                        channel = %msg.channel,
                        timeout_secs = GATEWAY_AGENT_TIMEOUT_SECS,
                        "gateway: full agent response timed out"
                    );

                    let fallback_text = "I’m still processing your message and hit a timeout. Please send it again, or use !new to start a fresh session.";
                    let fallback_args = match msg.platform.as_str() {
                        "Discord" => {
                            serde_json::json!({"channel_id": msg.channel, "message": fallback_text})
                        }
                        "Slack" => {
                            serde_json::json!({"channel": msg.channel, "message": fallback_text})
                        }
                        "Telegram" => {
                            serde_json::json!({"chat_id": msg.channel, "message": fallback_text})
                        }
                        "WhatsApp" => {
                            serde_json::json!({"phone": msg.channel, "message": fallback_text})
                        }
                        _ => serde_json::json!({"message": fallback_text}),
                    };
                    let fallback_tool = ToolCall {
                        id: format!("gateway_timeout_{}", uuid::Uuid::new_v4()),
                        function: ToolFunction {
                            name: reply_tool_name.to_string(),
                            arguments: fallback_args.to_string(),
                        },
                    };
                    let tool_result = tool_executor::execute_tool(
                        &fallback_tool,
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
                    if tool_result.is_error {
                        tracing::error!(
                            platform = %msg.platform,
                            channel = %msg.channel,
                            error = %tool_result.content,
                            "gateway: failed to send timeout fallback message"
                        );
                    }
                }
                Ok(Err(e)) => {
                    tracing::error!(
                        platform = %msg.platform,
                        error = %e,
                        "gateway: failed to process incoming message"
                    );
                }
                Ok(Ok(thread_id)) => {
                    // Store the mapping so follow-up messages use the same thread
                    self.gateway_threads
                        .write()
                        .await
                        .insert(channel_key.clone(), thread_id.clone());
                    if let Err(error) = self
                        .history
                        .upsert_gateway_thread_binding(&channel_key, &thread_id, now_millis())
                        .await
                    {
                        tracing::warn!(channel_key = %channel_key, %error, "gateway: failed to persist thread binding");
                    }

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
            }

            // Release per-channel processing lock
            self.gateway_inflight_channels
                .lock()
                .await
                .remove(&channel_key);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use uuid::Uuid;

    fn make_test_root(test_name: &str) -> std::path::PathBuf {
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join("test-artifacts")
            .join(format!("{test_name}-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).expect("failed to create test root");
        root
    }

    #[test]
    fn reset_commands_require_bang_prefix() {
        assert!(is_gateway_reset_command("!reset"));
        assert!(is_gateway_reset_command("!new"));
        assert!(!is_gateway_reset_command("reset"));
        assert!(!is_gateway_reset_command("new"));
        assert!(!is_gateway_reset_command("!renew"));
    }

    #[tokio::test]
    async fn gateway_init_loads_replay_cursors() {
        let root = make_test_root("gateway-init-loads-replay-cursors");
        let manager = SessionManager::new_test(&root).await;

        let mut config = AgentConfig::default();
        config.gateway.enabled = true;
        config.gateway.telegram_token = "telegram-token".to_string();
        config.gateway.slack_token = "slack-token".to_string();
        config.gateway.slack_channel_filter = "C123".to_string();
        config.gateway.discord_token = "discord-token".to_string();
        config.gateway.discord_channel_filter = "D456".to_string();
        config.gateway.whatsapp_token = "whatsapp-token".to_string();

        let engine = AgentEngine::new_test(manager, config, &root).await;

        engine
            .history
            .save_gateway_replay_cursor("telegram", "other", "99", "update_id")
            .await
            .expect("save other telegram cursor");
        engine
            .history
            .save_gateway_replay_cursor("telegram", "global", "42", "update_id")
            .await
            .expect("save telegram cursor");
        engine
            .history
            .save_gateway_replay_cursor("slack", "C123", "1712345678.000100", "message_ts")
            .await
            .expect("save slack cursor");
        engine
            .history
            .save_gateway_replay_cursor("discord", "D456", "998877665544", "message_id")
            .await
            .expect("save discord cursor");
        engine
            .history
            .save_gateway_replay_cursor("whatsapp", "15551234567", "wamid-1", "message_id")
            .await
            .expect("save whatsapp cursor");

        engine.init_gateway().await;

        let state_guard = engine.gateway_state.lock().await;
        let state = state_guard.as_ref().expect("gateway state should exist");
        assert_eq!(state.telegram_replay_cursor, Some(42));
        assert_eq!(
            state
                .slack_replay_cursors
                .get("C123")
                .map(std::string::String::as_str),
            Some("1712345678.000100")
        );
        assert_eq!(
            state
                .discord_replay_cursors
                .get("D456")
                .map(std::string::String::as_str),
            Some("998877665544")
        );
        assert_eq!(
            state
                .whatsapp_replay_cursors
                .get("15551234567")
                .map(std::string::String::as_str),
            Some("wamid-1")
        );
        assert!(state.replay_cycle_active.is_empty());

        drop(state_guard);
        fs::remove_dir_all(&root).expect("cleanup test root");
    }

    // -----------------------------------------------------------------------
    // Replay orchestration tests (Task 4)
    // -----------------------------------------------------------------------

    /// Build a minimal `GatewayConfig` suitable for replay tests.
    fn make_replay_gateway_config() -> super::types::GatewayConfig {
        use super::types::GatewayConfig;
        GatewayConfig {
            enabled: true,
            slack_token: String::new(),
            slack_channel_filter: String::new(),
            telegram_token: "tok".into(),
            telegram_allowed_chats: String::new(),
            discord_token: String::new(),
            discord_channel_filter: String::new(),
            discord_allowed_users: String::new(),
            whatsapp_allowed_contacts: String::new(),
            whatsapp_token: String::new(),
            whatsapp_phone_id: String::new(),
            command_prefix: "!".into(),
            gateway_electron_bridges_enabled: false,
            whatsapp_link_fallback_electron: false,
        }
    }

    fn make_telegram_replay_envelope(
        cursor_value: &str,
        channel_id: &str,
        content: &str,
        sender: &str,
    ) -> super::gateway::ReplayEnvelope {
        super::gateway::ReplayEnvelope {
            message: super::gateway::IncomingMessage {
                platform: "Telegram".into(),
                sender: sender.into(),
                content: content.into(),
                channel: channel_id.into(),
                message_id: Some(format!("tg:{cursor_value}")),
                thread_context: None,
            },
            channel_id: "global".into(),
            cursor_value: cursor_value.into(),
            cursor_type: "update_id",
        }
    }

    fn make_telegram_replay_envelope_with_id(
        cursor_value: &str,
        message_id: &str,
        channel_id: &str,
        content: &str,
        sender: &str,
    ) -> super::gateway::ReplayEnvelope {
        super::gateway::ReplayEnvelope {
            message: super::gateway::IncomingMessage {
                platform: "Telegram".into(),
                sender: sender.into(),
                content: content.into(),
                channel: channel_id.into(),
                message_id: Some(message_id.into()),
                thread_context: None,
            },
            channel_id: "global".into(),
            cursor_value: cursor_value.into(),
            cursor_type: "update_id",
        }
    }

    fn make_malformed_telegram_replay_envelope() -> super::gateway::ReplayEnvelope {
        // Empty cursor_value makes it unclassifiable.
        super::gateway::ReplayEnvelope {
            message: super::gateway::IncomingMessage {
                platform: "Telegram".into(),
                sender: "x".into(),
                content: "some content".into(),
                channel: "777".into(),
                message_id: None,
                thread_context: None,
            },
            channel_id: "global".into(),
            cursor_value: "".into(),
            cursor_type: "update_id",
        }
    }

    /// A completed replay batch removes the platform from `replay_cycle_active`,
    /// ensuring replay does not fire again until the next outage cycle.
    #[tokio::test]
    async fn reconnect_replay_runs_once_per_outage_cycle() {
        let root = make_test_root("replay-runs-once");
        let manager = SessionManager::new_test(&root).await;
        let engine = AgentEngine::new_test(manager, AgentConfig::default(), &root).await;

        let mut gw = super::gateway::GatewayState::new(
            make_replay_gateway_config(),
            reqwest::Client::new(),
        );
        gw.telegram_replay_cursor = Some(100);
        gw.replay_cycle_active.insert("telegram".to_string());

        let result = super::gateway::ReplayFetchResult::Replay(vec![
            make_telegram_replay_envelope("101", "777", "hello", "alice"),
        ]);
        let mut seen_ids: Vec<String> = Vec::new();

        let (messages, completed) = super::process_replay_result(
            &engine.history,
            "telegram",
            result,
            &mut gw,
            &mut seen_ids,
        )
        .await;

        // Caller (poll loop) clears the active flag on success.
        if completed {
            gw.replay_cycle_active.remove("telegram");
        }

        assert!(completed, "normal replay batch should complete");
        assert_eq!(messages.len(), 1, "one accepted message");
        assert!(
            !gw.replay_cycle_active.contains("telegram"),
            "replay_cycle_active cleared after completion"
        );

        let row = engine
            .history
            .load_gateway_replay_cursor("telegram", "global")
            .await
            .unwrap();
        assert_eq!(
            row.map(|r| r.cursor_value).as_deref(),
            Some("101"),
            "cursor persisted for the replayed message"
        );

        fs::remove_dir_all(&root).ok();
    }

    /// When `ReplayFetchResult::InitializeBoundary` is returned (first connect,
    /// no stored cursor), the boundary is persisted immediately and no messages
    /// are queued for the agent.
    #[tokio::test]
    async fn first_connect_without_cursor_still_skips_backlog() {
        let root = make_test_root("replay-skip-backlog");
        let manager = SessionManager::new_test(&root).await;
        let engine = AgentEngine::new_test(manager, AgentConfig::default(), &root).await;

        let mut gw = super::gateway::GatewayState::new(
            make_replay_gateway_config(),
            reqwest::Client::new(),
        );
        // No cursor set — simulates a first-connect scenario.
        assert!(gw.telegram_replay_cursor.is_none());

        let result = super::gateway::ReplayFetchResult::InitializeBoundary {
            channel_id: "global".to_string(),
            cursor_value: "500".to_string(),
            cursor_type: "update_id",
        };
        let mut seen_ids: Vec<String> = Vec::new();

        let (messages, completed) = super::process_replay_result(
            &engine.history,
            "telegram",
            result,
            &mut gw,
            &mut seen_ids,
        )
        .await;

        assert!(completed, "InitializeBoundary should complete immediately");
        assert!(messages.is_empty(), "no messages routed on first connect");

        let row = engine
            .history
            .load_gateway_replay_cursor("telegram", "global")
            .await
            .unwrap();
        assert_eq!(
            row.map(|r| r.cursor_value).as_deref(),
            Some("500"),
            "init boundary persisted to DB"
        );
        assert_eq!(
            gw.telegram_replay_cursor,
            Some(500),
            "init boundary updated in memory"
        );

        fs::remove_dir_all(&root).ok();
    }

    /// Duplicate messages advance the persisted cursor even though they are not
    /// routed to the agent.  Also verifies that the in-memory replay cursor is
    /// updated for duplicate/filtered classifications (Bug 2 regression).
    ///
    /// Uses a batch that is entirely duplicate/filtered with no accepted message
    /// following — if in-memory cursor update is missing for Duplicate/Filtered,
    /// `gw.telegram_replay_cursor` stays `None` after the batch.
    #[tokio::test]
    async fn classified_duplicate_or_filtered_message_advances_cursor() {
        let root = make_test_root("replay-cursor-advance");
        let manager = SessionManager::new_test(&root).await;
        let engine = AgentEngine::new_test(manager, AgentConfig::default(), &root).await;

        let mut gw = super::gateway::GatewayState::new(
            make_replay_gateway_config(),
            reqwest::Client::new(),
        );
        // Pre-seed two seen IDs so both envelopes are classified as duplicates.
        let mut seen_ids = vec!["tg:1001".to_string(), "tg:1002".to_string()];
        // Start with no in-memory cursor so the effect of the fix is visible.
        assert!(gw.telegram_replay_cursor.is_none());

        let result = super::gateway::ReplayFetchResult::Replay(vec![
            // Duplicate (already in seen_ids)
            make_telegram_replay_envelope_with_id("102", "tg:1001", "777", "dup text", "alice"),
            // Another duplicate — no accepted message follows
            make_telegram_replay_envelope_with_id("103", "tg:1002", "777", "dup text 2", "alice"),
        ]);

        let (messages, completed) = super::process_replay_result(
            &engine.history,
            "telegram",
            result,
            &mut gw,
            &mut seen_ids,
        )
        .await;

        assert!(completed);
        assert_eq!(messages.len(), 0, "no messages routed — all duplicates");

        // Both duplicates should have advanced the DB cursor to "103".
        let row = engine
            .history
            .load_gateway_replay_cursor("telegram", "global")
            .await
            .unwrap();
        assert_eq!(
            row.map(|r| r.cursor_value).as_deref(),
            Some("103"),
            "DB cursor advanced past duplicates"
        );

        // Bug 2 regression: in-memory cursor must be updated even when every
        // envelope is Duplicate/Filtered (not just when an Accepted follows).
        // Before the fix this assertion fails because update_in_memory_replay_cursor
        // was only called for the Accepted branch.
        assert_eq!(
            gw.telegram_replay_cursor,
            Some(103),
            "in-memory replay cursor must be updated for duplicate/filtered envelopes"
        );

        fs::remove_dir_all(&root).ok();
    }

    /// Replayed accepted messages must be written back to the shared
    /// `engine.gateway_seen_ids` ring buffer so that live-path deduplication
    /// sees them (Bug 1 regression).
    ///
    /// This test calls `apply_replay_results` — the production method that owns
    /// both the replay processing and the write-back.  If the write-back is
    /// removed from that method the final assertion will fail, proving the test
    /// catches the bug.
    #[tokio::test]
    async fn replay_accepted_messages_propagate_to_shared_seen_ids() {
        let root = make_test_root("replay-shared-seen-ids");
        let manager = SessionManager::new_test(&root).await;
        let engine = AgentEngine::new_test(manager, AgentConfig::default(), &root).await;

        // Shared ring-buffer starts empty.
        assert!(engine.gateway_seen_ids.lock().await.is_empty());

        let mut gw = super::gateway::GatewayState::new(
            make_replay_gateway_config(),
            reqwest::Client::new(),
        );

        let result = super::gateway::ReplayFetchResult::Replay(vec![
            make_telegram_replay_envelope("201", "777", "first replayed msg", "alice"),
            make_telegram_replay_envelope("202", "777", "second replayed msg", "bob"),
        ]);

        let messages = engine
            .apply_replay_results(vec![("telegram".to_string(), result)], &mut gw)
            .await;

        assert_eq!(messages.len(), 2, "both messages accepted");

        // Bug 1 regression: shared gateway_seen_ids must reflect IDs from the
        // replay so that a live message with the same ID is detected as a
        // duplicate and not re-routed to the agent.
        let seen = engine.gateway_seen_ids.lock().await;
        assert!(
            seen.contains(&"tg:201".to_string()),
            "tg:201 must be in shared seen_ids after replay"
        );
        assert!(
            seen.contains(&"tg:202".to_string()),
            "tg:202 must be in shared seen_ids after replay"
        );

        fs::remove_dir_all(&root).ok();
    }

    /// A malformed envelope (empty `cursor_value`) stops replay immediately and
    /// does NOT advance the persisted cursor past the last successfully classified
    /// message.
    #[tokio::test]
    async fn unclassified_failure_does_not_advance_cursor() {
        let root = make_test_root("replay-no-advance-on-failure");
        let manager = SessionManager::new_test(&root).await;
        let engine = AgentEngine::new_test(manager, AgentConfig::default(), &root).await;

        // Pre-set a known cursor in the DB.
        engine
            .history
            .save_gateway_replay_cursor("telegram", "global", "100", "update_id")
            .await
            .unwrap();

        let mut gw = super::gateway::GatewayState::new(
            make_replay_gateway_config(),
            reqwest::Client::new(),
        );
        let mut seen_ids: Vec<String> = Vec::new();

        // First envelope is valid; second is malformed (empty cursor_value).
        let result = super::gateway::ReplayFetchResult::Replay(vec![
            make_telegram_replay_envelope("101", "777", "good message", "alice"),
            make_malformed_telegram_replay_envelope(),
        ]);

        let (messages, completed) = super::process_replay_result(
            &engine.history,
            "telegram",
            result,
            &mut gw,
            &mut seen_ids,
        )
        .await;

        assert!(!completed, "replay should stop on malformed envelope");
        // The good message (101) advanced the cursor; the malformed one did not.
        let row = engine
            .history
            .load_gateway_replay_cursor("telegram", "global")
            .await
            .unwrap();
        assert_eq!(
            row.map(|r| r.cursor_value).as_deref(),
            Some("101"),
            "cursor at last successfully classified message"
        );
        // The good message is still returned for routing.
        assert_eq!(messages.len(), 1);

        fs::remove_dir_all(&root).ok();
    }
}
