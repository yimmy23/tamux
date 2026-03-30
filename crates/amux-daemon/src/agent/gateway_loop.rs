//! Gateway initialization, background run loop, and platform message polling.

use super::gateway_health::{GatewayConnectionStatus, PlatformHealthState};
use super::heartbeat::is_peak_activity_hour;
use super::*;
use chrono::Timelike;
use std::sync::OnceLock;

pub(crate) fn platform_health_from_snapshot(
    snapshot: &amux_protocol::GatewayHealthState,
) -> PlatformHealthState {
    let mut health = PlatformHealthState::new();
    health.status = match snapshot.status {
        amux_protocol::GatewayConnectionStatus::Connected => GatewayConnectionStatus::Connected,
        amux_protocol::GatewayConnectionStatus::Disconnected => {
            GatewayConnectionStatus::Disconnected
        }
        amux_protocol::GatewayConnectionStatus::Error => GatewayConnectionStatus::Error,
    };
    health.last_success_at = snapshot.last_success_at_ms;
    health.last_error_at = snapshot.last_error_at_ms;
    health.consecutive_failure_count = snapshot.consecutive_failure_count;
    health.last_error = snapshot.last_error.clone();
    health.current_backoff_secs = snapshot.current_backoff_secs;
    health
}

pub(crate) fn snapshot_from_platform_health(
    platform: &str,
    health: &PlatformHealthState,
) -> amux_protocol::GatewayHealthState {
    amux_protocol::GatewayHealthState {
        platform: platform.to_string(),
        status: match health.status {
            GatewayConnectionStatus::Connected => amux_protocol::GatewayConnectionStatus::Connected,
            GatewayConnectionStatus::Disconnected => {
                amux_protocol::GatewayConnectionStatus::Disconnected
            }
            GatewayConnectionStatus::Error => amux_protocol::GatewayConnectionStatus::Error,
        },
        last_success_at_ms: health.last_success_at,
        last_error_at_ms: health.last_error_at,
        consecutive_failure_count: health.consecutive_failure_count,
        last_error: health.last_error.clone(),
        current_backoff_secs: health.current_backoff_secs,
    }
}

pub(crate) fn apply_health_snapshot(
    gateway_state: &mut gateway::GatewayState,
    snapshot: &amux_protocol::GatewayHealthState,
) {
    let health = platform_health_from_snapshot(snapshot);
    match snapshot.platform.as_str() {
        "slack" => gateway_state.slack_health = health,
        "discord" => gateway_state.discord_health = health,
        "telegram" => gateway_state.telegram_health = health,
        _ => {}
    }
}

#[derive(Default)]
struct GatewayRuntimeControl {
    restart_attempts: u32,
    restart_not_before_ms: Option<u64>,
}

fn gateway_runtime_control() -> &'static Mutex<GatewayRuntimeControl> {
    static CONTROL: OnceLock<Mutex<GatewayRuntimeControl>> = OnceLock::new();
    CONTROL.get_or_init(|| Mutex::new(GatewayRuntimeControl::default()))
}

fn is_gateway_reset_command(trimmed_lower: &str) -> bool {
    matches!(trimmed_lower, "!reset" | "!new")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct GatewayRouteRequest {
    mode: gateway::GatewayRouteMode,
    ack_only: bool,
}

fn gateway_switch_command(trimmed_lower: &str) -> Option<gateway::GatewayRouteMode> {
    match trimmed_lower {
        "!swarog" | "!main" => Some(gateway::GatewayRouteMode::Swarog),
        "!rarog" | "!concierge" => Some(gateway::GatewayRouteMode::Rarog),
        _ => None,
    }
}

fn classify_gateway_route_request(content: &str) -> Option<GatewayRouteRequest> {
    let trimmed = content.trim();
    let trimmed_lower = trimmed.to_ascii_lowercase();
    if let Some(mode) = gateway_switch_command(&trimmed_lower) {
        return Some(GatewayRouteRequest {
            mode,
            ack_only: true,
        });
    }

    let normalized = trimmed_lower
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");

    let exact_swarog = [
        "switch to swarog",
        "talk to swarog",
        "connect me to swarog",
        "put swarog on",
        "use swarog",
        "hand me to swarog",
        "route me to swarog",
        "switch me to swarog",
        "i want swarog",
    ];
    let exact_rarog = [
        "switch to rarog",
        "switch back to rarog",
        "switch back to concierge",
        "talk to rarog",
        "connect me to rarog",
        "put rarog on",
        "use rarog",
        "hand me back to rarog",
        "route me to rarog",
        "switch me to rarog",
        "i want rarog",
    ];
    if exact_swarog.contains(&normalized.as_str()) {
        return Some(GatewayRouteRequest {
            mode: gateway::GatewayRouteMode::Swarog,
            ack_only: true,
        });
    }
    if exact_rarog.contains(&normalized.as_str()) {
        return Some(GatewayRouteRequest {
            mode: gateway::GatewayRouteMode::Rarog,
            ack_only: true,
        });
    }

    let swarog_phrases = [
        "switch to swarog",
        "switch me to swarog",
        "talk to swarog",
        "let swarog handle",
        "have swarog handle",
        "route this to swarog",
        "swarog take over",
        "swarog, take over",
    ];
    if swarog_phrases
        .iter()
        .any(|phrase| normalized.contains(phrase))
    {
        return Some(GatewayRouteRequest {
            mode: gateway::GatewayRouteMode::Swarog,
            ack_only: false,
        });
    }

    let rarog_phrases = [
        "switch to rarog",
        "switch back to rarog",
        "switch back to concierge",
        "talk to rarog",
        "let rarog handle",
        "have rarog handle",
        "route this to rarog",
        "rarog take this back",
        "rarog, take this back",
    ];
    if rarog_phrases
        .iter()
        .any(|phrase| normalized.contains(phrase))
    {
        return Some(GatewayRouteRequest {
            mode: gateway::GatewayRouteMode::Rarog,
            ack_only: false,
        });
    }

    None
}

fn gateway_route_confirmation(mode: gateway::GatewayRouteMode) -> String {
    match mode {
        gateway::GatewayRouteMode::Swarog => format!(
            "Switched this channel to {}. I will keep routing here to {} until you ask for {} back.",
            MAIN_AGENT_NAME, MAIN_AGENT_NAME, CONCIERGE_AGENT_NAME
        ),
        gateway::GatewayRouteMode::Rarog => format!(
            "Switched this channel back to {}. I will keep routing here to {} until you ask for {}.",
            CONCIERGE_AGENT_NAME, CONCIERGE_AGENT_NAME, MAIN_AGENT_NAME
        ),
    }
}

fn gateway_reply_args(platform: &str, channel: &str, message: &str) -> serde_json::Value {
    match platform {
        "Discord" => serde_json::json!({"channel_id": channel, "message": message}),
        "Slack" => serde_json::json!({"channel": channel, "message": message}),
        "Telegram" => serde_json::json!({"chat_id": channel, "message": message}),
        "WhatsApp" => serde_json::json!({"phone": channel, "message": message}),
        _ => serde_json::json!({"message": message}),
    }
}

const GATEWAY_TRIAGE_TIMEOUT_SECS: u64 = 12;
const GATEWAY_AGENT_TIMEOUT_SECS: u64 = 120;
// Allow enough headroom for provider-side rate limiting and chunked deliveries.
const GATEWAY_SEND_RESULT_TIMEOUT_SECS: u64 = 180;
const GATEWAY_EVENT_DRAIN_INTERVAL_MS: u64 = 150;

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
        "whatsapp" => {
            gw.whatsapp_replay_cursors
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
    /// Apply a batch of pre-fetched replay results across one or more platforms.
    ///
    /// Accepted replay message IDs are tracked only within this replay batch.
    /// The shared `gateway_seen_ids` ring buffer is updated later, when the
    /// returned messages are actually routed through the normal incoming queue.
    /// Pre-seeding the shared ring here would cause those queued messages to be
    /// discarded as duplicates before they ever reached the agent.
    ///
    /// `platform_results` is a list of `(platform_name, channel_results,
    /// fetch_complete)` tuples collected by the caller before holding the
    /// gateway-state lock.  `fetch_complete` is `true` only when **all**
    /// channels for the platform were fetched without error; it is `false` when
    /// a later channel fetch failed after earlier ones already succeeded (partial
    /// fetch).  The platform is removed from `gw.replay_cycle_active` only when
    /// every channel result processed successfully **and** `fetch_complete` is
    /// `true`.  Passing `fetch_complete = false` with partial channel results
    /// preserves those results for routing while keeping the platform active so
    /// the remaining channels are retried on the next cycle.
    ///
    /// Returns the accumulated messages to prepend to the live queue.
    pub(crate) async fn apply_replay_results(
        &self,
        platform_results: Vec<(String, Vec<gateway::ReplayFetchResult>, bool)>,
        gw: &mut gateway::GatewayState,
    ) -> Vec<gateway::IncomingMessage> {
        let mut seen_ids_snap = self.gateway_seen_ids.lock().await.clone();
        let mut replay_msgs: Vec<gateway::IncomingMessage> = Vec::new();

        for (platform, channel_results, fetch_complete) in platform_results {
            let mut all_completed = true;
            let mut platform_msgs: Vec<gateway::IncomingMessage> = Vec::new();

            for result in channel_results {
                let (msgs, completed) =
                    process_replay_result(&self.history, &platform, result, gw, &mut seen_ids_snap)
                        .await;
                platform_msgs.extend(msgs);
                if !completed {
                    all_completed = false;
                    break;
                }
            }

            if all_completed && fetch_complete {
                gw.replay_cycle_active.remove(platform.as_str());
                tracing::info!(
                    platform = %platform,
                    replay_count = platform_msgs.len(),
                    "gateway: replay cycle complete"
                );
            }
            replay_msgs.extend(platform_msgs);
        }
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

    /// Initialize gateway runtime state from persisted daemon state.
    pub(crate) async fn init_gateway(&self) {
        let config = self.config.read().await.clone();
        let gw = &config.gateway;

        if !gw.enabled {
            tracing::info!("gateway: disabled in config, skipping initialization");
            return;
        }

        let slack_token = gw.slack_token.clone();
        let telegram_token = gw.telegram_token.clone();
        let discord_token = gw.discord_token.clone();
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

        let telegram_replay_cursor =
            match self.history.load_gateway_replay_cursors("telegram").await {
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
        let discord_replay_cursors = match self.history.load_gateway_replay_cursors("discord").await
        {
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
        let health_snapshots = match self.history.list_gateway_health_snapshots().await {
            Ok(rows) => rows,
            Err(error) => {
                tracing::warn!(%error, "gateway: failed to load health snapshots");
                Vec::new()
            }
        };

        let mut gateway_state = gateway::GatewayState::new(gw_config, self.http_client.clone());
        gateway_state.telegram_replay_cursor = telegram_replay_cursor;
        gateway_state.slack_replay_cursors = slack_replay_cursors;
        gateway_state.discord_replay_cursors = discord_replay_cursors;
        gateway_state.whatsapp_replay_cursors = whatsapp_replay_cursors;
        for row in health_snapshots {
            match serde_json::from_str::<amux_protocol::GatewayHealthState>(&row.state_json) {
                Ok(snapshot) => apply_health_snapshot(&mut gateway_state, &snapshot),
                Err(error) => {
                    tracing::warn!(
                        platform = %row.platform,
                        %error,
                        "gateway: ignoring invalid health snapshot"
                    );
                }
            }
        }

        *self.gateway_state.lock().await = Some(gateway_state);

        tracing::info!("gateway: runtime state initialized in daemon");
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
            self.stop_gateway().await;
            return;
        }

        // Clear existing state before re-init so stale credentials don't persist.
        *self.gateway_state.lock().await = None;
        *self.gateway_discord_channels.write().await = Vec::new();
        *self.gateway_slack_channels.write().await = Vec::new();

        if let Err(error) = self
            .request_gateway_reload(Some("config reloaded".to_string()))
            .await
        {
            tracing::debug!(error = %error, "gateway: reload command not delivered");
        }

        self.maybe_spawn_gateway().await;
    }

    pub(crate) async fn set_gateway_ipc_sender(
        &self,
        sender: Option<mpsc::UnboundedSender<amux_protocol::DaemonMessage>>,
    ) {
        *self.gateway_ipc_sender.lock().await = sender;
    }

    pub(crate) async fn clear_gateway_ipc_sender(&self) {
        *self.gateway_ipc_sender.lock().await = None;
        self.gateway_pending_send_results.lock().await.clear();
    }

    pub(crate) async fn request_gateway_send(
        &self,
        request: amux_protocol::GatewaySendRequest,
    ) -> Result<amux_protocol::GatewaySendResult> {
        let correlation_id = request.correlation_id.clone();
        let (result_tx, result_rx) = tokio::sync::oneshot::channel();

        let sender = {
            let Some(sender) = self.gateway_ipc_sender.lock().await.clone() else {
                return Err(anyhow::anyhow!(
                    "standalone gateway runtime is not connected"
                ));
            };
            let mut pending = self.gateway_pending_send_results.lock().await;
            if pending.contains_key(&correlation_id) {
                return Err(anyhow::anyhow!(
                    "duplicate gateway send correlation id: {correlation_id}"
                ));
            }
            pending.insert(correlation_id.clone(), result_tx);
            sender
        };

        if let Err(error) =
            sender.send(amux_protocol::DaemonMessage::GatewaySendRequest { request })
        {
            self.gateway_pending_send_results
                .lock()
                .await
                .remove(&correlation_id);
            return Err(anyhow::anyhow!(
                "failed to deliver gateway send request: {error}"
            ));
        }

        match tokio::time::timeout(
            std::time::Duration::from_secs(GATEWAY_SEND_RESULT_TIMEOUT_SECS),
            result_rx,
        )
        .await
        {
            Ok(Ok(result)) => Ok(result),
            Ok(Err(_)) => Err(anyhow::anyhow!(
                "gateway send request was interrupted before a result arrived"
            )),
            Err(_) => {
                self.gateway_pending_send_results
                    .lock()
                    .await
                    .remove(&correlation_id);
                Err(anyhow::anyhow!("timed out waiting for gateway send result"))
            }
        }
    }

    pub(crate) async fn complete_gateway_send_result(
        &self,
        result: amux_protocol::GatewaySendResult,
    ) -> bool {
        let waiter = self
            .gateway_pending_send_results
            .lock()
            .await
            .remove(&result.correlation_id);
        if let Some(waiter) = waiter {
            let _ = waiter.send(result);
            true
        } else {
            false
        }
    }

    pub(crate) async fn gateway_health_snapshots(&self) -> Vec<amux_protocol::GatewayHealthState> {
        let gw = self.gateway_state.lock().await;
        let Some(state) = gw.as_ref() else {
            return Vec::new();
        };

        vec![
            snapshot_from_platform_health("slack", &state.slack_health),
            snapshot_from_platform_health("discord", &state.discord_health),
            snapshot_from_platform_health("telegram", &state.telegram_health),
        ]
    }

    async fn reset_gateway_restart_backoff(&self) {
        let mut control = gateway_runtime_control().lock().await;
        control.restart_attempts = 0;
        control.restart_not_before_ms = None;
    }

    async fn schedule_gateway_restart_backoff(&self, reason: &str) {
        let mut control = gateway_runtime_control().lock().await;
        control.restart_attempts = control.restart_attempts.saturating_add(1);
        let delay_secs = crate::agent::liveness::recovery::RecoveryPlanner::default()
            .compute_backoff_secs(control.restart_attempts.saturating_sub(1));
        let next_restart_at = now_millis().saturating_add(delay_secs.saturating_mul(1000));
        control.restart_not_before_ms = Some(next_restart_at);
        tracing::warn!(
            attempts = control.restart_attempts,
            delay_secs,
            next_restart_at,
            %reason,
            "gateway: scheduled restart backoff"
        );
    }

    async fn spawn_gateway_process_at(&self, gateway_path: &std::path::Path) -> Result<()> {
        let mut proc = self.gateway_process.lock().await;
        if let Some(ref mut child) = *proc {
            let _ = child.kill().await;
        }
        *proc = None;

        tracing::info!(?gateway_path, "spawning gateway process");
        let mut cmd = tokio::process::Command::new(gateway_path);
        cmd.stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());

        match cmd.spawn() {
            Ok(child) => {
                tracing::info!(pid = ?child.id(), "gateway process started");
                *proc = Some(child);
                drop(proc);
                self.reset_gateway_restart_backoff().await;
                self.clear_gateway_ipc_sender().await;
                Ok(())
            }
            Err(error) => {
                tracing::error!(error = %error, "failed to spawn gateway process");
                drop(proc);
                self.schedule_gateway_restart_backoff("gateway spawn failed")
                    .await;
                Err(error.into())
            }
        }
    }

    /// Spawn the tamux-gateway process if gateway tokens are configured.
    pub async fn maybe_spawn_gateway(&self) {
        let config = self.config.read().await.clone();
        let gw = &config.gateway;
        let slack_token = gw.slack_token.clone();
        let telegram_token = gw.telegram_token.clone();
        let discord_token = gw.discord_token.clone();

        self.init_gateway().await;

        if slack_token.is_empty() && telegram_token.is_empty() && discord_token.is_empty() {
            tracing::info!("gateway: no platform tokens configured, skipping");
            return;
        }

        // Find the gateway binary next to the daemon binary
        let gateway_path_opt = std::env::current_exe().ok().and_then(|p| {
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

        let gateway_path = match gateway_path_opt {
            Some(p) => p,
            None => {
                tracing::warn!("gateway binary not found next to daemon executable");
                return;
            }
        };

        if let Err(error) = self.spawn_gateway_process_at(&gateway_path).await {
            tracing::error!(error = %error, "failed to spawn gateway process");
        }
    }

    /// Stop the gateway process.
    pub async fn stop_gateway(&self) {
        if let Some(sender) = self.gateway_ipc_sender.lock().await.clone() {
            let _ = sender.send(amux_protocol::DaemonMessage::GatewayShutdownCommand {
                command: amux_protocol::GatewayShutdownCommand {
                    correlation_id: format!("gateway-shutdown-{}", uuid::Uuid::new_v4()),
                    reason: Some("daemon shutdown".to_string()),
                    requested_at_ms: now_millis(),
                },
            });
        }

        let mut proc = self.gateway_process.lock().await;
        if let Some(ref mut child) = *proc {
            tracing::info!("stopping gateway process");
            let _ = child.kill().await;
        }
        *proc = None;
        drop(proc);
        self.clear_gateway_ipc_sender().await;
        self.reset_gateway_restart_backoff().await;
    }

    pub(crate) async fn request_gateway_reload(&self, reason: Option<String>) -> Result<bool> {
        let sender = self.gateway_ipc_sender.lock().await.clone();
        let Some(sender) = sender else {
            return Ok(false);
        };
        sender
            .send(amux_protocol::DaemonMessage::GatewayReloadCommand {
                command: amux_protocol::GatewayReloadCommand {
                    correlation_id: format!("gateway-reload-{}", uuid::Uuid::new_v4()),
                    reason,
                    requested_at_ms: now_millis(),
                },
            })
            .map_err(|error| anyhow::anyhow!(error.to_string()))?;
        Ok(true)
    }

    pub(crate) async fn record_gateway_ipc_loss(&self, reason: &str) {
        tracing::warn!(reason, "gateway: ipc connection lost");
        let mut proc = self.gateway_process.lock().await;
        if let Some(ref mut child) = *proc {
            let _ = child.kill().await;
        }
        *proc = None;
        drop(proc);
        self.clear_gateway_ipc_sender().await;
        self.schedule_gateway_restart_backoff(reason).await;
    }

    #[cfg(test)]
    pub(crate) async fn maybe_spawn_gateway_with_path(
        &self,
        gateway_path: &std::path::Path,
    ) -> Result<()> {
        self.init_gateway().await;
        self.spawn_gateway_process_at(gateway_path).await
    }

    #[cfg(test)]
    pub(crate) async fn reinit_gateway_with_path(
        &self,
        gateway_path: &std::path::Path,
    ) -> Result<()> {
        let config = self.config.read().await.clone();
        let gw = &config.gateway;

        if !gw.enabled {
            *self.gateway_state.lock().await = None;
            *self.gateway_discord_channels.write().await = Vec::new();
            *self.gateway_slack_channels.write().await = Vec::new();
            self.stop_gateway().await;
            return Ok(());
        }

        *self.gateway_state.lock().await = None;
        *self.gateway_discord_channels.write().await = Vec::new();
        *self.gateway_slack_channels.write().await = Vec::new();
        let _ = self
            .request_gateway_reload(Some("config reloaded".to_string()))
            .await?;
        self.maybe_spawn_gateway_with_path(gateway_path).await
    }

    /// Main background loop — processes tasks, runs heartbeats, and supervises gateway runtime.
    pub async fn run_loop(self: Arc<Self>, mut shutdown: tokio::sync::watch::Receiver<bool>) {
        let config = self.config.read().await.clone();

        let task_interval = std::time::Duration::from_secs(config.task_poll_interval_secs);
        let mut watcher_refresh_rx = self.watcher_refresh_rx.lock().await.take();

        let mut task_tick = tokio::time::interval(task_interval);
        let mut watcher_tick =
            tokio::time::interval(std::time::Duration::from_millis(FILE_WATCH_TICK_MS));
        let mut supervisor_tick = tokio::time::interval(std::time::Duration::from_secs(30));
        let mut anticipatory_tick =
            tokio::time::interval(std::time::Duration::from_secs(ANTICIPATORY_TICK_SECS));
        let mut gateway_event_tick = tokio::time::interval(std::time::Duration::from_millis(
            GATEWAY_EVENT_DRAIN_INTERVAL_MS,
        ));
        let mut pending_watcher_refreshes: HashMap<String, Instant> = HashMap::new();

        // Cron-based heartbeat scheduling (D-06, BEAT-01)
        let heartbeat_cron_expr = super::heartbeat::resolve_cron_from_config(&config);
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
                _ = gateway_event_tick.tick() => {
                    self.process_gateway_messages().await;
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
                    if let Err(error) = self.supervise_gateway_runtime().await {
                        tracing::warn!(error = %error, "gateway supervision tick failed");
                    }

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

    pub(crate) async fn supervise_gateway_runtime(&self) -> Result<()> {
        let config = self.config.read().await.clone();
        let gw = &config.gateway;
        if !gw.enabled {
            return Ok(());
        }

        let restart_deadline = gateway_runtime_control().lock().await.restart_not_before_ms;
        if let Some(deadline_ms) = restart_deadline {
            if now_millis() < deadline_ms {
                return Ok(());
            }
        }

        let child_exited = {
            let mut proc = self.gateway_process.lock().await;
            if let Some(child) = proc.as_mut() {
                match child.try_wait() {
                    Ok(Some(status)) => {
                        tracing::warn!(?status, "gateway child exited");
                        *proc = None;
                        true
                    }
                    Ok(None) => false,
                    Err(error) => {
                        tracing::warn!(error = %error, "gateway child status check failed");
                        *proc = None;
                        true
                    }
                }
            } else {
                false
            }
        };

        if child_exited {
            self.clear_gateway_ipc_sender().await;
            self.schedule_gateway_restart_backoff("gateway child exited")
                .await;
            return Ok(());
        }

        let process_running = self.gateway_process.lock().await.is_some();
        if process_running {
            return Ok(());
        }

        self.maybe_spawn_gateway().await;
        Ok(())
    }

    #[cfg(test)]
    pub(crate) async fn gateway_restart_attempts(&self) -> u32 {
        gateway_runtime_control().lock().await.restart_attempts
    }

    #[cfg(test)]
    pub(crate) async fn gateway_restart_not_before_ms(&self) -> Option<u64> {
        gateway_runtime_control().lock().await.restart_not_before_ms
    }

    /// Process gateway IPC messages already normalized by `tamux-gateway`.
    ///
    /// The daemon no longer polls platform APIs directly. Its runtime boundary
    /// here is limited to:
    /// - draining inbound IPC events queued by the server,
    /// - updating reply-routing continuity state,
    /// - routing those messages through the normal agent path.
    async fn process_gateway_messages(&self) {
        let now_ms = now_millis();
        let incoming = {
            let mut gw_guard = self.gateway_state.lock().await;
            let Some(gw) = gw_guard.as_mut() else {
                return;
            };

            let mut queue = self.gateway_injected_messages.lock().await;
            let incoming: Vec<gateway::IncomingMessage> = queue.drain(..).collect();
            if incoming.is_empty() {
                return;
            }

            for msg in &incoming {
                let key = format!("{}:{}", msg.platform, msg.channel);
                gw.last_incoming_at.insert(key.clone(), now_ms);
                if let Some(ref tc) = msg.thread_context {
                    gw.reply_contexts.insert(key, tc.clone());
                }
            }
            incoming
        };

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
                self.gateway_route_modes.write().await.remove(&channel_key);
                if let Err(error) = self
                    .history
                    .delete_gateway_thread_binding(&channel_key)
                    .await
                {
                    tracing::warn!(channel_key = %channel_key, %error, "gateway: failed to persist reset binding");
                }
                if let Err(error) = self.history.delete_gateway_route_mode(&channel_key).await {
                    tracing::warn!(channel_key = %channel_key, %error, "gateway: failed to persist reset route mode");
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

            let route_request = classify_gateway_route_request(&msg.content);
            if let Some(request) = route_request {
                self.gateway_route_modes
                    .write()
                    .await
                    .insert(channel_key.clone(), request.mode);
                if let Err(error) = self
                    .history
                    .upsert_gateway_route_mode(&channel_key, request.mode.as_str(), now_millis())
                    .await
                {
                    tracing::warn!(channel_key = %channel_key, %error, "gateway: failed to persist route mode");
                }

                if request.ack_only {
                    let response_text = gateway_route_confirmation(request.mode);
                    let auto_tool = ToolCall {
                        id: format!("gateway_route_{}", uuid::Uuid::new_v4()),
                        function: ToolFunction {
                            name: reply_tool_name.to_string(),
                            arguments: gateway_reply_args(
                                &msg.platform,
                                &msg.channel,
                                &response_text,
                            )
                            .to_string(),
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
                            "gateway: failed to send route switch confirmation"
                        );
                    }
                    self.gateway_inflight_channels
                        .lock()
                        .await
                        .remove(&channel_key);
                    continue;
                }
            }

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

            let existing_thread = self.gateway_threads.read().await.get(&channel_key).cloned();
            let route_mode = if let Some(request) = route_request {
                request.mode
            } else {
                self.gateway_route_modes
                    .read()
                    .await
                    .get(&channel_key)
                    .copied()
                    .unwrap_or_default()
            };
            let history_window = if let Some(ref tid) = existing_thread {
                match self.history.list_recent_messages(tid, 10).await {
                    Ok(messages) if !messages.is_empty() => {
                        let mut lines = Vec::with_capacity(messages.len() + 1);
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

            if route_mode == gateway::GatewayRouteMode::Rarog {
                // Triage via concierge — simple messages get a direct response,
                // complex ones fall through to the full agent loop.
                let triage = match tokio::time::timeout(
                    std::time::Duration::from_secs(GATEWAY_TRIAGE_TIMEOUT_SECS),
                    self.concierge.triage_gateway_message(
                        self,
                        &msg.platform,
                        &msg.sender,
                        &msg.content,
                        history_window.as_deref(),
                        existing_thread.as_deref(),
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
                        let auto_tool = ToolCall {
                            id: format!("concierge_{}", uuid::Uuid::new_v4()),
                            function: ToolFunction {
                                name: reply_tool_name.to_string(),
                                arguments: gateway_reply_args(
                                    &msg.platform,
                                    &msg.channel,
                                    &response_text,
                                )
                                .to_string(),
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
            } else {
                tracing::info!(
                    platform = %msg.platform,
                    channel = %msg.channel,
                    "gateway: sticky route is set to Swarog; bypassing concierge triage"
                );
            }

            let enriched_prompt = if let Some(window) = history_window {
                format!(
                    "{prompt}\n\nPrevious 10 messages from this channel (oldest first):\n{window}"
                )
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
    use std::path::PathBuf;
    use uuid::Uuid;

    fn make_test_root(test_name: &str) -> std::path::PathBuf {
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join("test-artifacts")
            .join(format!("{test_name}-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).expect("failed to create test root");
        root
    }

    fn repo_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("daemon crate dir")
            .parent()
            .expect("workspace root")
            .to_path_buf()
    }

    #[cfg(unix)]
    fn make_gateway_test_binary(root: &std::path::Path, name: &str) -> std::path::PathBuf {
        use std::os::unix::fs::PermissionsExt;

        let path = root.join(name);
        fs::write(&path, "#!/bin/sh\nsleep 60\n").expect("failed to write gateway test binary");
        let mut permissions = fs::metadata(&path).expect("metadata").permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&path, permissions)
            .expect("failed to mark gateway test binary executable");
        path
    }

    #[test]
    fn reset_commands_require_bang_prefix() {
        assert!(is_gateway_reset_command("!reset"));
        assert!(is_gateway_reset_command("!new"));
        assert!(!is_gateway_reset_command("reset"));
        assert!(!is_gateway_reset_command("new"));
        assert!(!is_gateway_reset_command("!renew"));
    }

    #[test]
    fn gateway_route_requests_support_commands_and_natural_language() {
        assert_eq!(
            classify_gateway_route_request("!swarog"),
            Some(GatewayRouteRequest {
                mode: gateway::GatewayRouteMode::Swarog,
                ack_only: true,
            })
        );
        assert_eq!(
            classify_gateway_route_request("switch to swarog"),
            Some(GatewayRouteRequest {
                mode: gateway::GatewayRouteMode::Swarog,
                ack_only: true,
            })
        );
        assert_eq!(
            classify_gateway_route_request("switch to swarog and take over this channel"),
            Some(GatewayRouteRequest {
                mode: gateway::GatewayRouteMode::Swarog,
                ack_only: false,
            })
        );
        assert_eq!(
            classify_gateway_route_request("switch back to rarog"),
            Some(GatewayRouteRequest {
                mode: gateway::GatewayRouteMode::Rarog,
                ack_only: true,
            })
        );
        assert_eq!(
            classify_gateway_route_request("rarog, take this back and answer directly"),
            Some(GatewayRouteRequest {
                mode: gateway::GatewayRouteMode::Rarog,
                ack_only: false,
            })
        );
        assert_eq!(
            classify_gateway_route_request("what does swarog think?"),
            None
        );
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

    #[tokio::test]
    async fn gateway_state_updates_survive_gateway_restart() {
        let root = make_test_root("gateway-state-updates-survive-restart");
        let manager = SessionManager::new_test(&root).await;

        let mut config = AgentConfig::default();
        config.gateway.enabled = true;
        let engine = AgentEngine::new_test(manager, config, &root).await;

        engine
            .history
            .save_gateway_replay_cursor("slack", "C123", "1712345678.000100", "message_ts")
            .await
            .expect("save slack cursor");
        engine
            .history
            .upsert_gateway_thread_binding("Slack:C123", "thread-123", 1111)
            .await
            .expect("save thread binding");
        engine
            .history
            .upsert_gateway_route_mode("Slack:C123", "swarog", 2222)
            .await
            .expect("save route mode");

        engine.init_gateway().await;
        *engine.gateway_state.lock().await = None;
        engine.init_gateway().await;

        let state_guard = engine.gateway_state.lock().await;
        let state = state_guard.as_ref().expect("gateway state should exist");
        assert_eq!(
            state
                .slack_replay_cursors
                .get("C123")
                .map(std::string::String::as_str),
            Some("1712345678.000100")
        );
        drop(state_guard);

        let bindings = engine
            .history
            .list_gateway_thread_bindings()
            .await
            .expect("list thread bindings");
        assert!(bindings.iter().any(
            |(channel_key, thread_id)| channel_key == "Slack:C123" && thread_id == "thread-123"
        ));

        let modes = engine
            .history
            .list_gateway_route_modes()
            .await
            .expect("list route modes");
        assert!(
            modes
                .iter()
                .any(|(channel_key, route_mode)| channel_key == "Slack:C123"
                    && route_mode == "swarog")
        );

        fs::remove_dir_all(&root).expect("cleanup test root");
    }

    #[tokio::test]
    async fn gateway_health_snapshots_survive_gateway_restart() {
        let root = make_test_root("gateway-health-snapshots-survive-restart");
        let manager = SessionManager::new_test(&root).await;

        let mut config = AgentConfig::default();
        config.gateway.enabled = true;
        let engine = AgentEngine::new_test(manager, config, &root).await;

        let snapshot = amux_protocol::GatewayHealthState {
            platform: "slack".to_string(),
            status: amux_protocol::GatewayConnectionStatus::Error,
            last_success_at_ms: Some(111),
            last_error_at_ms: Some(222),
            consecutive_failure_count: 3,
            last_error: Some("timeout".to_string()),
            current_backoff_secs: 30,
        };
        engine
            .history
            .upsert_gateway_health_snapshot(&snapshot, 333)
            .await
            .expect("save health snapshot");

        engine.init_gateway().await;
        *engine.gateway_state.lock().await = None;
        engine.init_gateway().await;

        let state_guard = engine.gateway_state.lock().await;
        let state = state_guard.as_ref().expect("gateway state should exist");
        assert_eq!(state.slack_health.status, GatewayConnectionStatus::Error);
        assert_eq!(state.slack_health.last_success_at, Some(111));
        assert_eq!(state.slack_health.last_error_at, Some(222));
        assert_eq!(state.slack_health.consecutive_failure_count, 3);
        assert_eq!(state.slack_health.last_error.as_deref(), Some("timeout"));
        assert_eq!(state.slack_health.current_backoff_secs, 30);

        fs::remove_dir_all(&root).expect("cleanup test root");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn daemon_respawns_gateway_process_when_enabled() {
        let root = make_test_root("daemon-respawns-gateway-process");
        let manager = SessionManager::new_test(&root).await;
        let mut config = AgentConfig::default();
        config.gateway.enabled = true;
        let engine = AgentEngine::new_test(manager, config, &root).await;
        let gateway_path = make_gateway_test_binary(&root, "tamux-gateway-test");

        engine
            .maybe_spawn_gateway_with_path(&gateway_path)
            .await
            .expect("spawn gateway");

        let proc_guard = engine.gateway_process.lock().await;
        assert!(proc_guard.is_some(), "gateway process should be running");
        drop(proc_guard);

        let attempts = engine.gateway_restart_attempts().await;
        assert_eq!(attempts, 0);

        engine.stop_gateway().await;
        let _ = fs::remove_dir_all(&root);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn daemon_gateway_restart_backoff_applies_after_ipc_loss() {
        let root = make_test_root("daemon-gateway-restart-backoff");
        let manager = SessionManager::new_test(&root).await;
        let mut config = AgentConfig::default();
        config.gateway.enabled = true;
        let engine = AgentEngine::new_test(manager, config, &root).await;
        let gateway_path = make_gateway_test_binary(&root, "tamux-gateway-test");

        engine
            .maybe_spawn_gateway_with_path(&gateway_path)
            .await
            .expect("spawn gateway");
        engine.record_gateway_ipc_loss("ipc lost").await;

        assert!(engine.gateway_process.lock().await.is_none());
        let deadline = engine.gateway_restart_not_before_ms().await;
        assert!(deadline.is_some(), "restart deadline should be scheduled");

        engine.stop_gateway().await;
        let _ = fs::remove_dir_all(&root);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn daemon_gateway_reload_requests_clean_restart() {
        let root = make_test_root("daemon-gateway-reload");
        let manager = SessionManager::new_test(&root).await;
        let mut config = AgentConfig::default();
        config.gateway.enabled = true;
        let engine = AgentEngine::new_test(manager, config, &root).await;
        let gateway_path = make_gateway_test_binary(&root, "tamux-gateway-test");
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        engine.set_gateway_ipc_sender(Some(tx)).await;

        engine
            .reinit_gateway_with_path(&gateway_path)
            .await
            .expect("reinit gateway");

        let msg = rx.recv().await.expect("expected reload command");
        assert!(matches!(
            msg,
            amux_protocol::DaemonMessage::GatewayReloadCommand { .. }
        ));
        assert!(engine.gateway_process.lock().await.is_some());

        engine.stop_gateway().await;
        let _ = fs::remove_dir_all(&root);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn gateway_config_reload_uses_spawn_restart_path_not_init_gateway() {
        let root = make_test_root("gateway-config-reload");
        let manager = SessionManager::new_test(&root).await;
        let mut config = AgentConfig::default();
        config.gateway.enabled = true;
        config.gateway.slack_token = "slack-token".to_string();
        let engine = AgentEngine::new_test(manager, config, &root).await;
        let gateway_path = make_gateway_test_binary(&root, "tamux-gateway-test");
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        engine.set_gateway_ipc_sender(Some(tx)).await;

        engine
            .reinit_gateway_with_path(&gateway_path)
            .await
            .expect("reinit gateway");

        let msg = rx.recv().await.expect("expected reload command");
        assert!(matches!(
            msg,
            amux_protocol::DaemonMessage::GatewayReloadCommand { .. }
        ));
        assert!(engine.gateway_state.lock().await.is_some());
        assert!(engine.gateway_process.lock().await.is_some());

        engine.stop_gateway().await;
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn daemon_gateway_loop_no_longer_polls_slack_discord_or_telegram() {
        let source =
            fs::read_to_string(repo_root().join("crates/amux-daemon/src/agent/gateway_loop.rs"))
                .expect("read gateway_loop.rs");
        let production_source = source
            .split("\n#[cfg(test)]")
            .next()
            .unwrap_or(source.as_str());
        for forbidden in [
            "gateway::poll_telegram(gw).await",
            "gateway::poll_slack(gw, &slack_channels).await",
            "gateway::poll_discord(gw, &discord_channels).await",
            "gateway::fetch_telegram_replay(gw).await",
            "gateway::fetch_slack_replay(gw, ch).await",
            "gateway::fetch_discord_replay(gw, ch).await",
        ] {
            assert!(
                !production_source.contains(forbidden),
                "gateway loop still contains local transport ownership seam: {forbidden}"
            );
        }
    }

    #[test]
    fn daemon_gateway_send_path_no_longer_issues_platform_http_requests() {
        let gateway_source =
            fs::read_to_string(repo_root().join("crates/amux-daemon/src/agent/gateway.rs"))
                .expect("read gateway.rs");
        let tool_source =
            fs::read_to_string(repo_root().join("crates/amux-daemon/src/agent/tool_executor.rs"))
                .expect("read tool_executor.rs");
        for forbidden in [
            "https://slack.com/api",
            "https://discord.com/api/v10",
            "https://api.telegram.org",
            "conversations.history",
            "users/@me/channels",
            "getUpdates?offset=",
        ] {
            assert!(
                !gateway_source.contains(forbidden) && !tool_source.contains(forbidden),
                "daemon transport source still contains platform HTTP path: {forbidden}"
            );
        }
        assert!(
            !tool_source.contains("gateway_format::"),
            "daemon send path still depends on daemon-owned gateway formatting"
        );
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

        let mut gw =
            super::gateway::GatewayState::new(make_replay_gateway_config(), reqwest::Client::new());
        gw.telegram_replay_cursor = Some(100);
        gw.replay_cycle_active.insert("telegram".to_string());

        let result =
            super::gateway::ReplayFetchResult::Replay(vec![make_telegram_replay_envelope(
                "101", "777", "hello", "alice",
            )]);
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

        let mut gw =
            super::gateway::GatewayState::new(make_replay_gateway_config(), reqwest::Client::new());
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

    /// Discord replay initialization must also seed the live poll boundary.
    ///
    /// Without this, reconnect replay persists `discord_replay_cursors` but
    /// leaves the live continuity cursor empty, so the next reconnect would
    /// skip backlog handling again.
    #[tokio::test]
    async fn discord_initialize_boundary_seeds_live_poll_cursor() {
        let root = make_test_root("discord-replay-init-seeds-live-cursor");
        let manager = SessionManager::new_test(&root).await;
        let engine = AgentEngine::new_test(manager, AgentConfig::default(), &root).await;

        let mut gw =
            super::gateway::GatewayState::new(make_replay_gateway_config(), reqwest::Client::new());
        assert!(
            gw.discord_replay_cursors.is_empty(),
            "test starts with no live discord cursor"
        );

        let result = super::gateway::ReplayFetchResult::InitializeBoundary {
            channel_id: "D456".to_string(),
            cursor_value: "998877665544".to_string(),
            cursor_type: "message_id",
        };
        let mut seen_ids: Vec<String> = Vec::new();

        let (messages, completed) = super::process_replay_result(
            &engine.history,
            "discord",
            result,
            &mut gw,
            &mut seen_ids,
        )
        .await;

        assert!(
            completed,
            "discord init boundary should complete immediately"
        );
        assert!(
            messages.is_empty(),
            "discord init boundary should not route messages"
        );
        assert_eq!(
            gw.discord_replay_cursors.get("D456").map(String::as_str),
            Some("998877665544"),
            "discord continuity cursor should be seeded in memory"
        );

        let row = engine
            .history
            .load_gateway_replay_cursor("discord", "D456")
            .await
            .unwrap();
        assert_eq!(
            row.map(|r| r.cursor_value).as_deref(),
            Some("998877665544"),
            "discord init boundary should persist to DB"
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

        let mut gw =
            super::gateway::GatewayState::new(make_replay_gateway_config(), reqwest::Client::new());
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

    /// Replayed accepted messages must NOT be pre-written into the shared
    /// `engine.gateway_seen_ids` ring buffer before they are routed.
    ///
    /// They are prepended to the incoming queue and the normal routing path
    /// records their IDs when it actually processes them. Pre-seeding the
    /// shared ring early causes the queue consumer to skip them as duplicates.
    #[tokio::test]
    async fn replay_accepted_messages_do_not_preseed_shared_seen_ids() {
        let root = make_test_root("replay-no-preseed-shared-seen-ids");
        let manager = SessionManager::new_test(&root).await;
        let engine = AgentEngine::new_test(manager, AgentConfig::default(), &root).await;

        // Shared ring-buffer starts empty.
        assert!(engine.gateway_seen_ids.lock().await.is_empty());

        let mut gw =
            super::gateway::GatewayState::new(make_replay_gateway_config(), reqwest::Client::new());

        let result = super::gateway::ReplayFetchResult::Replay(vec![
            make_telegram_replay_envelope("201", "777", "first replayed msg", "alice"),
            make_telegram_replay_envelope("202", "777", "second replayed msg", "bob"),
        ]);

        let messages = engine
            .apply_replay_results(vec![("telegram".to_string(), vec![result], true)], &mut gw)
            .await;

        assert_eq!(messages.len(), 2, "both messages accepted");

        // Regression: replay accepts must not poison the shared seen-id ring
        // before the queue consumer has a chance to route them.
        let seen = engine.gateway_seen_ids.lock().await;
        assert!(
            seen.is_empty(),
            "shared seen_ids must stay unchanged until queued messages are routed"
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

        let mut gw =
            super::gateway::GatewayState::new(make_replay_gateway_config(), reqwest::Client::new());
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

    /// Regression: Task 4 refactor — partial multi-channel fetch must preserve
    /// already-fetched channel results and keep the platform active for retry.
    ///
    /// When the first channel of a multi-channel platform (e.g. Slack/Discord)
    /// succeeds but a later channel fetch would fail, `apply_replay_results` must
    /// still route the messages from the already-fetched channels.  The platform
    /// must remain in `replay_cycle_active` so the missing channels are retried
    /// on the next cycle.
    ///
    /// This test directly exercises `apply_replay_results` with
    /// `fetch_complete = false` to prove both properties.
    #[tokio::test]
    async fn partial_multichannel_fetch_preserves_earlier_results_and_keeps_platform_active() {
        let root = make_test_root("replay-partial-multichannel");
        let manager = SessionManager::new_test(&root).await;
        let engine = AgentEngine::new_test(manager, AgentConfig::default(), &root).await;

        let mut gw =
            super::gateway::GatewayState::new(make_replay_gateway_config(), reqwest::Client::new());
        // Platform is active for replay (simulates a reconnect cycle).
        gw.replay_cycle_active.insert("slack".to_string());

        // Only the first channel result is present — the second channel fetch
        // "failed" so it was never added to the vec.  fetch_complete=false
        // signals that the fetch was partial.
        let ch1_result =
            super::gateway::ReplayFetchResult::Replay(vec![make_telegram_replay_envelope(
                "301",
                "C1",
                "msg from ch1",
                "alice",
            )]);

        let messages = engine
            .apply_replay_results(
                vec![("slack".to_string(), vec![ch1_result], false)],
                &mut gw,
            )
            .await;

        // Messages from the successfully-fetched channel must be routed.
        assert_eq!(
            messages.len(),
            1,
            "messages from the successful channel must not be dropped"
        );

        // Platform must remain active so the failed channel is retried next cycle.
        assert!(
            gw.replay_cycle_active.contains("slack"),
            "platform must stay in replay_cycle_active when fetch was partial"
        );

        fs::remove_dir_all(&root).ok();
    }

    /// Regression: complementary case — when all channels fetch successfully
    /// (`fetch_complete = true`) the platform IS removed from `replay_cycle_active`.
    #[tokio::test]
    async fn complete_multichannel_fetch_removes_platform_from_active() {
        let root = make_test_root("replay-complete-multichannel");
        let manager = SessionManager::new_test(&root).await;
        let engine = AgentEngine::new_test(manager, AgentConfig::default(), &root).await;

        let mut gw =
            super::gateway::GatewayState::new(make_replay_gateway_config(), reqwest::Client::new());
        gw.replay_cycle_active.insert("slack".to_string());

        let ch1_result =
            super::gateway::ReplayFetchResult::Replay(vec![make_telegram_replay_envelope(
                "401",
                "C1",
                "msg from ch1",
                "alice",
            )]);
        let ch2_result =
            super::gateway::ReplayFetchResult::Replay(vec![make_telegram_replay_envelope(
                "402",
                "C2",
                "msg from ch2",
                "bob",
            )]);

        let messages = engine
            .apply_replay_results(
                vec![("slack".to_string(), vec![ch1_result, ch2_result], true)],
                &mut gw,
            )
            .await;

        assert_eq!(messages.len(), 2, "both channel messages must be routed");
        assert!(
            !gw.replay_cycle_active.contains("slack"),
            "platform must be removed from replay_cycle_active after complete fetch"
        );

        fs::remove_dir_all(&root).ok();
    }
}
