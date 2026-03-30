use std::collections::HashSet;
use std::path::Path;
use std::sync::Arc;

use amux_protocol::{
    ClientMessage, DaemonMessage, GatewayBootstrapPayload, GatewayConnectionStatus,
    GatewayContinuityState, GatewayCursorState, GatewayHealthState, GatewayIncomingEvent,
    GatewayProviderBootstrap, GatewayRegistration, GatewayRouteMode, GatewayRouteModeState,
    GatewayThreadBindingState, SessionInfo, GATEWAY_IPC_PROTOCOL_VERSION,
};
use anyhow::{Context, Result};
use futures::SinkExt;
use futures::StreamExt;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::sync::{broadcast, mpsc};
use tokio_util::codec::Framed;

use crate::agent::skill_community::{
    export_skill, import_community_skill, prepare_publish, unpack_skill, ImportResult,
};
use crate::agent::skill_registry::{to_community_entry, RegistryClient};
use crate::agent::AgentEngine;
use crate::session_manager::SessionManager;

struct WhatsAppLinkSubscriberGuard {
    agent: Arc<AgentEngine>,
    subscriber_id: Option<u64>,
}

impl WhatsAppLinkSubscriberGuard {
    fn new(agent: Arc<AgentEngine>) -> Self {
        Self {
            agent,
            subscriber_id: None,
        }
    }

    async fn set(&mut self, subscriber_id: u64) {
        if let Some(previous) = self.subscriber_id.replace(subscriber_id) {
            self.agent.whatsapp_link.unsubscribe(previous).await;
        }
    }

    async fn clear(&mut self) {
        if let Some(subscriber_id) = self.subscriber_id.take() {
            self.agent.whatsapp_link.unsubscribe(subscriber_id).await;
        }
    }
}

impl Drop for WhatsAppLinkSubscriberGuard {
    fn drop(&mut self) {
        if let Some(subscriber_id) = self.subscriber_id.take() {
            let agent = self.agent.clone();
            tokio::spawn(async move {
                agent.whatsapp_link.unsubscribe(subscriber_id).await;
            });
        }
    }
}

#[derive(Debug, Clone)]
enum GatewayConnectionState {
    Unregistered,
    AwaitingBootstrapAck {
        registration: GatewayRegistration,
        bootstrap_correlation_id: String,
    },
    Active {
        registration: GatewayRegistration,
    },
}

fn current_time_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

fn gateway_feature_flags(config: &crate::agent::types::GatewayConfig) -> Vec<String> {
    let mut flags = Vec::new();
    if config.enabled {
        flags.push("gateway_enabled".to_string());
    }
    if config.gateway_electron_bridges_enabled {
        flags.push("gateway_electron_bridges_enabled".to_string());
    }
    if config.whatsapp_link_fallback_electron {
        flags.push("whatsapp_link_fallback_electron".to_string());
    }
    flags
}

fn gateway_provider_bootstrap(
    platform: &str,
    enabled: bool,
    credentials: serde_json::Value,
    config: serde_json::Value,
) -> GatewayProviderBootstrap {
    GatewayProviderBootstrap {
        platform: platform.to_string(),
        enabled,
        credentials_json: credentials.to_string(),
        config_json: config.to_string(),
    }
}

fn gateway_bootstrap_providers(
    config: &crate::agent::types::GatewayConfig,
) -> Vec<GatewayProviderBootstrap> {
    vec![
        gateway_provider_bootstrap(
            "slack",
            config.enabled && !config.slack_token.trim().is_empty(),
            serde_json::json!({ "token": config.slack_token }),
            serde_json::json!({
                "channel_filter": config.slack_channel_filter,
                "command_prefix": config.command_prefix,
            }),
        ),
        gateway_provider_bootstrap(
            "discord",
            config.enabled && !config.discord_token.trim().is_empty(),
            serde_json::json!({ "token": config.discord_token }),
            serde_json::json!({
                "channel_filter": config.discord_channel_filter,
                "allowed_users": config.discord_allowed_users,
                "command_prefix": config.command_prefix,
            }),
        ),
        gateway_provider_bootstrap(
            "telegram",
            config.enabled && !config.telegram_token.trim().is_empty(),
            serde_json::json!({ "token": config.telegram_token }),
            serde_json::json!({
                "allowed_chats": config.telegram_allowed_chats,
                "command_prefix": config.command_prefix,
            }),
        ),
        gateway_provider_bootstrap(
            "whatsapp",
            config.enabled
                && (!config.whatsapp_token.trim().is_empty()
                    || !config.whatsapp_allowed_contacts.trim().is_empty()),
            serde_json::json!({
                "token": config.whatsapp_token,
                "phone_id": config.whatsapp_phone_id,
            }),
            serde_json::json!({
                "allowed_contacts": config.whatsapp_allowed_contacts,
                "command_prefix": config.command_prefix,
                "fallback_electron": config.whatsapp_link_fallback_electron,
            }),
        ),
    ]
}

fn parse_gateway_route_mode(value: &str) -> GatewayRouteMode {
    match value.trim().to_ascii_lowercase().as_str() {
        "swarog" | "main" => GatewayRouteMode::Swarog,
        _ => GatewayRouteMode::Rarog,
    }
}

async fn build_gateway_bootstrap_payload(
    agent: &AgentEngine,
    bootstrap_correlation_id: String,
) -> GatewayBootstrapPayload {
    let gateway = agent.config.read().await.gateway.clone();

    let mut cursors = Vec::new();
    for platform in ["slack", "discord", "telegram", "whatsapp"] {
        match agent.history.load_gateway_replay_cursors(platform).await {
            Ok(rows) => cursors.extend(rows.into_iter().map(|row| GatewayCursorState {
                platform: row.platform,
                channel_id: row.channel_id,
                cursor_value: row.cursor_value,
                cursor_type: row.cursor_type,
                updated_at_ms: row.updated_at,
            })),
            Err(error) => {
                tracing::warn!(platform, %error, "gateway: failed to load replay cursors for bootstrap");
            }
        }
    }

    let thread_bindings = match agent.history.list_gateway_thread_bindings().await {
        Ok(rows) => rows
            .into_iter()
            .map(|(channel_key, thread_id)| GatewayThreadBindingState {
                channel_key,
                thread_id: Some(thread_id),
                updated_at_ms: 0,
            })
            .collect(),
        Err(error) => {
            tracing::warn!(%error, "gateway: failed to load thread bindings for bootstrap");
            Vec::new()
        }
    };

    let route_modes = match agent.history.list_gateway_route_modes().await {
        Ok(rows) => rows
            .into_iter()
            .map(|(channel_key, route_mode)| GatewayRouteModeState {
                channel_key,
                route_mode: parse_gateway_route_mode(&route_mode),
                updated_at_ms: 0,
            })
            .collect(),
        Err(error) => {
            tracing::warn!(%error, "gateway: failed to load route modes for bootstrap");
            Vec::new()
        }
    };
    let health_snapshots = match agent.history.list_gateway_health_snapshots().await {
        Ok(rows) => rows
            .into_iter()
            .filter_map(|row| {
                serde_json::from_str::<GatewayHealthState>(&row.state_json)
                    .map_err(|error| {
                        tracing::warn!(
                            platform = %row.platform,
                            %error,
                            "gateway: failed to parse health snapshot for bootstrap"
                        );
                    })
                    .ok()
            })
            .collect(),
        Err(error) => {
            tracing::warn!(%error, "gateway: failed to load health snapshots for bootstrap");
            Vec::new()
        }
    };

    GatewayBootstrapPayload {
        bootstrap_correlation_id,
        feature_flags: gateway_feature_flags(&gateway),
        providers: gateway_bootstrap_providers(&gateway),
        continuity: GatewayContinuityState {
            cursors,
            thread_bindings,
            route_modes,
            health_snapshots,
        },
    }
}

fn gateway_connection_is_active(state: &GatewayConnectionState) -> bool {
    matches!(state, GatewayConnectionState::Active { .. })
}

fn gateway_connection_is_tracked(state: &GatewayConnectionState) -> bool {
    !matches!(state, GatewayConnectionState::Unregistered)
}

fn gateway_thread_context_from_event(
    platform: &str,
    thread_id: Option<&str>,
) -> Option<crate::agent::gateway::ThreadContext> {
    let thread_id = thread_id?.trim();
    if thread_id.is_empty() {
        return None;
    }

    match platform.to_ascii_lowercase().as_str() {
        "slack" => Some(crate::agent::gateway::ThreadContext {
            slack_thread_ts: Some(thread_id.to_string()),
            ..Default::default()
        }),
        "discord" => Some(crate::agent::gateway::ThreadContext {
            discord_message_id: Some(thread_id.to_string()),
            ..Default::default()
        }),
        "telegram" => {
            thread_id
                .parse::<i64>()
                .ok()
                .map(|message_id| crate::agent::gateway::ThreadContext {
                    telegram_message_id: Some(message_id),
                    ..Default::default()
                })
        }
        _ => None,
    }
}

async fn enqueue_gateway_incoming_event(
    agent: &Arc<AgentEngine>,
    event: GatewayIncomingEvent,
) -> Result<()> {
    let gateway_message = crate::agent::gateway::IncomingMessage {
        thread_context: gateway_thread_context_from_event(
            &event.platform,
            event.thread_id.as_deref(),
        ),
        platform: event.platform,
        sender: event.sender_display.unwrap_or(event.sender_id),
        content: event.content,
        channel: event.channel_id,
        message_id: event.message_id,
    };
    agent.enqueue_gateway_message(gateway_message).await
}

async fn persist_gateway_health_update(
    agent: &Arc<AgentEngine>,
    update: GatewayHealthState,
) -> Result<()> {
    let updated_at_ms = current_time_ms();
    agent
        .history
        .upsert_gateway_health_snapshot(&update, updated_at_ms)
        .await?;

    let platform_key = update.platform.to_ascii_lowercase();
    let platform_label = match platform_key.as_str() {
        "slack" => "Slack",
        "discord" => "Discord",
        "telegram" => "Telegram",
        other => other,
    }
    .to_string();
    let new_status = match update.status {
        GatewayConnectionStatus::Connected => {
            crate::agent::RuntimeGatewayConnectionStatus::Connected
        }
        GatewayConnectionStatus::Disconnected => {
            crate::agent::RuntimeGatewayConnectionStatus::Disconnected
        }
        GatewayConnectionStatus::Error => crate::agent::RuntimeGatewayConnectionStatus::Error,
    };
    let mut previous_status = None;

    let mut gw_guard = agent.gateway_state.lock().await;
    if let Some(gateway_state) = gw_guard.as_mut() {
        match platform_key.as_str() {
            "slack" => {
                previous_status = Some(gateway_state.slack_health.status);
                gateway_state.slack_health.status = new_status;
                gateway_state.slack_health.last_success_at = update.last_success_at_ms;
                gateway_state.slack_health.last_error_at = update.last_error_at_ms;
                gateway_state.slack_health.consecutive_failure_count =
                    update.consecutive_failure_count;
                gateway_state.slack_health.last_error = update.last_error.clone();
                gateway_state.slack_health.current_backoff_secs = update.current_backoff_secs;
            }
            "discord" => {
                previous_status = Some(gateway_state.discord_health.status);
                gateway_state.discord_health.status = new_status;
                gateway_state.discord_health.last_success_at = update.last_success_at_ms;
                gateway_state.discord_health.last_error_at = update.last_error_at_ms;
                gateway_state.discord_health.consecutive_failure_count =
                    update.consecutive_failure_count;
                gateway_state.discord_health.last_error = update.last_error.clone();
                gateway_state.discord_health.current_backoff_secs = update.current_backoff_secs;
            }
            "telegram" => {
                previous_status = Some(gateway_state.telegram_health.status);
                gateway_state.telegram_health.status = new_status;
                gateway_state.telegram_health.last_success_at = update.last_success_at_ms;
                gateway_state.telegram_health.last_error_at = update.last_error_at_ms;
                gateway_state.telegram_health.consecutive_failure_count =
                    update.consecutive_failure_count;
                gateway_state.telegram_health.last_error = update.last_error.clone();
                gateway_state.telegram_health.current_backoff_secs = update.current_backoff_secs;
            }
            _ => {}
        }
    }

    drop(gw_guard);

    if previous_status != Some(new_status) {
        let status = match update.status {
            GatewayConnectionStatus::Connected => "connected",
            GatewayConnectionStatus::Disconnected => "disconnected",
            GatewayConnectionStatus::Error => "error",
        }
        .to_string();
        let _ = agent
            .event_sender()
            .send(crate::agent::types::AgentEvent::GatewayStatus {
                platform: platform_label.clone(),
                status: status.clone(),
                last_error: update.last_error.clone(),
                consecutive_failures: Some(update.consecutive_failure_count),
            });

        let description = match (status.as_str(), update.last_error.as_deref()) {
            ("connected", _) => format!("{platform_label} reconnected"),
            ("error", Some(err)) => format!(
                "{platform_label} disconnected after {} failures: {err}",
                update.consecutive_failure_count
            ),
            ("error", None) => format!("{platform_label} disconnected"),
            ("disconnected", _) => format!("{platform_label} disconnected"),
            _ => format!("{platform_label} status: {status}"),
        };

        let _ = agent
            .event_sender()
            .send(crate::agent::types::AgentEvent::HeartbeatDigest {
                cycle_id: format!("gateway_health_{}", uuid::Uuid::new_v4()),
                actionable: status != "connected",
                digest: description.clone(),
                items: vec![crate::agent::types::HeartbeatDigestItem {
                    priority: if status == "connected" { 3 } else { 1 },
                    check_type: crate::agent::types::HeartbeatCheckType::UnrepliedGatewayMessages,
                    title: description.clone(),
                    suggestion: if status == "connected" {
                        format!("{platform_label} is back online")
                    } else {
                        format!("Check {platform_label} API credentials and connectivity")
                    },
                }],
                checked_at: updated_at_ms,
                explanation: None,
                confidence: None,
            });

        let audit_entry = crate::history::AuditEntryRow {
            id: format!("gw_health_{}", uuid::Uuid::new_v4()),
            timestamp: updated_at_ms as i64,
            action_type: "gateway_health_transition".to_string(),
            summary: format!("{platform_label} -> {status}"),
            explanation: update.last_error.clone(),
            confidence: None,
            confidence_band: None,
            causal_trace_id: None,
            thread_id: None,
            goal_run_id: None,
            task_id: None,
            raw_data_json: Some(
                serde_json::json!({
                    "platform": platform_label,
                    "new_status": status,
                    "consecutive_failures": update.consecutive_failure_count,
                })
                .to_string(),
            ),
        };
        agent.history.insert_action_audit(&audit_entry).await?;
    }

    Ok(())
}

fn gateway_response_channel_key(platform: &str, channel_id: &str) -> Option<String> {
    let label = match platform.to_ascii_lowercase().as_str() {
        "slack" => "Slack",
        "discord" => "Discord",
        "telegram" => "Telegram",
        "whatsapp" => "WhatsApp",
        _ => return None,
    };
    Some(format!("{label}:{channel_id}"))
}

fn is_expected_disconnect_error(error: &anyhow::Error) -> bool {
    let expected_io_error = |kind: std::io::ErrorKind| {
        matches!(
            kind,
            std::io::ErrorKind::UnexpectedEof
                | std::io::ErrorKind::BrokenPipe
                | std::io::ErrorKind::ConnectionReset
                | std::io::ErrorKind::ConnectionAborted
                | std::io::ErrorKind::NotConnected
        )
    };
    if error.chain().any(|cause| {
        cause
            .downcast_ref::<std::io::Error>()
            .is_some_and(|io| expected_io_error(io.kind()))
    }) {
        return true;
    }

    let message = error.to_string().to_ascii_lowercase();
    message.contains("unexpected end of file")
        || message.contains("connection reset by peer")
        || message.contains("broken pipe")
}

fn normalize_session_tag(value: &str) -> Option<String> {
    let mut normalized = String::with_capacity(value.len());
    let mut last_was_dash = false;
    for ch in value.chars() {
        let mapped = if ch.is_ascii_alphanumeric() {
            last_was_dash = false;
            Some(ch.to_ascii_lowercase())
        } else if matches!(ch, '/' | '\\' | ' ' | ':' | '_' | '.' | '-') {
            if last_was_dash {
                None
            } else {
                last_was_dash = true;
                Some('-')
            }
        } else {
            None
        };
        if let Some(ch) = mapped {
            normalized.push(ch);
        }
    }

    let trimmed = normalized.trim_matches('-');
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

fn summarize_session_output(recent_output: Option<&str>) -> Option<String> {
    let line = recent_output?
        .lines()
        .rev()
        .map(str::trim)
        .find(|line| !line.is_empty())?;
    let condensed = line.split_whitespace().collect::<Vec<_>>().join(" ");
    if condensed.is_empty() {
        return None;
    }
    const MAX_CHARS: usize = 120;
    if condensed.chars().count() <= MAX_CHARS {
        return Some(condensed);
    }
    Some(condensed.chars().take(MAX_CHARS - 3).collect::<String>() + "...")
}

fn build_session_end_episode_payload(
    session_id: &str,
    info: Option<&SessionInfo>,
    recent_output: Option<&str>,
) -> (String, Vec<String>) {
    let title = info
        .and_then(|value| value.title.as_deref())
        .filter(|value| !value.is_empty());
    let active_command = info
        .and_then(|value| value.active_command.as_deref())
        .filter(|value| !value.is_empty());
    let cwd = info
        .and_then(|value| value.cwd.as_deref())
        .filter(|value| !value.is_empty());
    let cwd_label = cwd
        .and_then(|value| Path::new(value).file_name())
        .and_then(|value| value.to_str());
    let recent_output_summary = summarize_session_output(recent_output);

    let focus = title
        .map(ToOwned::to_owned)
        .or_else(|| active_command.map(ToOwned::to_owned))
        .or_else(|| cwd_label.map(|value| format!("workspace {value}")))
        .unwrap_or_else(|| format!("session {session_id}"));

    let mut summary = format!("{focus} ended");
    if let Some(cwd_label) = cwd_label {
        summary.push_str(&format!(" in {cwd_label}"));
    }
    if let Some(output) = recent_output_summary {
        summary.push_str(&format!(". Last output: {output}"));
    }

    let mut entities = vec![format!("session:{session_id}")];
    if let Some(workspace_id) = info.and_then(|value| value.workspace_id.as_deref()) {
        entities.push(format!("workspace:{workspace_id}"));
    }
    if let Some(tag) = cwd_label.and_then(normalize_session_tag) {
        entities.push(format!("cwd:{tag}"));
    }
    if let Some(command) = active_command
        .and_then(|value| value.split_whitespace().next())
        .and_then(normalize_session_tag)
    {
        entities.push(format!("command:{command}"));
    }
    if let Some(tag) = title.and_then(normalize_session_tag) {
        entities.push(format!("title:{tag}"));
    }
    entities.sort();
    entities.dedup();

    (summary, entities)
}

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
        | AgentEvent::RetryStatus { thread_id, .. }
        | AgentEvent::WorkflowNotice { thread_id, .. }
        | AgentEvent::ModeShift { thread_id, .. }
        | AgentEvent::ConfidenceWarning { thread_id, .. }
        | AgentEvent::CounterWhoAlert { thread_id, .. } => Some(thread_id.as_str()),
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

#[cfg(not(test))]
async fn start_whatsapp_link_backend(agent: Arc<AgentEngine>) -> Result<()> {
    crate::agent::start_whatsapp_link_native(agent).await
}

#[cfg(not(test))]
async fn maybe_autostart_whatsapp_link(agent: Arc<AgentEngine>) {
    let gateway = agent.config.read().await.gateway.clone();
    if !gateway.enabled {
        return;
    }

    let persisted_path = crate::agent::whatsapp_native_store_path(&agent.data_dir);
    if !persisted_path.exists() {
        tracing::info!(
            persisted_path = %persisted_path.display(),
            "whatsapp link autostart skipped (no persisted daemon state)"
        );
        return;
    }

    tracing::info!(
        persisted_path = %persisted_path.display(),
        "whatsapp link autostarting from persisted daemon state"
    );
    match agent.whatsapp_link.start_if_idle().await {
        Ok(false) => return,
        Ok(true) => {}
        Err(error) => {
            tracing::warn!(
                error = %error,
                "whatsapp link autostart: failed to transition runtime to starting"
            );
            return;
        }
    }
    if let Err(error) = start_whatsapp_link_backend(agent.clone()).await {
        tracing::warn!(
            error = %error,
            "whatsapp link autostart: failed to start backend"
        );
        agent
            .whatsapp_link
            .broadcast_error(error.to_string(), false)
            .await;
    }
}

#[cfg(test)]
mod tests {
    use super::{
        build_gateway_bootstrap_payload, build_session_end_episode_payload,
        concierge_welcome_fingerprint, enqueue_gateway_incoming_event, handle_connection,
        is_expected_disconnect_error, persist_gateway_health_update,
    };
    use crate::agent::types::AgentConfig;
    use crate::agent::types::{
        AgentEvent, ConciergeAction, ConciergeActionType, ConciergeDetailLevel,
    };
    use crate::agent::AgentEngine;
    use crate::history::HistoryStore;
    use crate::plugin::PluginManager;
    use crate::session_manager::SessionManager;
    use amux_protocol::{
        AmuxCodec, ClientMessage, DaemonMessage, GatewayConnectionStatus, GatewayHealthState,
        GatewayIncomingEvent, GatewayRegistration, GatewaySendRequest, SessionInfo,
        GATEWAY_IPC_PROTOCOL_VERSION,
    };
    use futures::{SinkExt, StreamExt};
    use std::collections::HashSet;
    use std::path::PathBuf;
    use std::sync::Arc;
    use tokio::io::DuplexStream;
    use tokio::task::JoinHandle;
    use tokio::time::{timeout, Duration};
    use tokio_util::codec::Framed;

    #[test]
    fn concierge_welcome_fingerprint_matches_for_identical_events() {
        let event_a = AgentEvent::ConciergeWelcome {
            thread_id: "concierge".to_string(),
            content: "Welcome".to_string(),
            detail_level: ConciergeDetailLevel::ProactiveTriage,
            actions: vec![ConciergeAction {
                label: "Dismiss".to_string(),
                action_type: ConciergeActionType::DismissWelcome,
                thread_id: None,
            }],
        };
        let event_b = event_a.clone();
        assert_eq!(
            concierge_welcome_fingerprint(&event_a),
            concierge_welcome_fingerprint(&event_b)
        );
    }

    #[test]
    fn concierge_welcome_fingerprint_changes_with_action_payload() {
        let event_a = AgentEvent::ConciergeWelcome {
            thread_id: "concierge".to_string(),
            content: "Welcome".to_string(),
            detail_level: ConciergeDetailLevel::ProactiveTriage,
            actions: vec![ConciergeAction {
                label: "Dismiss".to_string(),
                action_type: ConciergeActionType::DismissWelcome,
                thread_id: None,
            }],
        };
        let event_b = AgentEvent::ConciergeWelcome {
            thread_id: "concierge".to_string(),
            content: "Welcome".to_string(),
            detail_level: ConciergeDetailLevel::ProactiveTriage,
            actions: vec![ConciergeAction {
                label: "Continue".to_string(),
                action_type: ConciergeActionType::ContinueSession,
                thread_id: Some("thread-1".to_string()),
            }],
        };
        assert_ne!(
            concierge_welcome_fingerprint(&event_a),
            concierge_welcome_fingerprint(&event_b)
        );
    }

    #[test]
    fn expected_disconnect_error_matches_unexpected_eof() {
        let error: anyhow::Error =
            std::io::Error::new(std::io::ErrorKind::UnexpectedEof, "peer closed").into();
        assert!(is_expected_disconnect_error(&error));
    }

    #[test]
    fn expected_disconnect_error_does_not_match_invalid_data() {
        let error: anyhow::Error =
            std::io::Error::new(std::io::ErrorKind::InvalidData, "bad frame").into();
        assert!(!is_expected_disconnect_error(&error));
    }

    #[test]
    fn gateway_incoming_events_forward_without_thread_subscription() {
        let event = AgentEvent::GatewayIncoming {
            platform: "WhatsApp".to_string(),
            sender: "alice".to_string(),
            content: "hello".to_string(),
            channel: "alice@s.whatsapp.net".to_string(),
        };
        let client_threads = HashSet::new();
        assert!(super::should_forward_agent_event(&event, &client_threads));
    }

    #[test]
    fn build_session_end_episode_payload_generates_summary_and_tags() {
        let session_id = uuid::Uuid::nil();
        let info = SessionInfo {
            id: session_id,
            title: Some("Cargo test runner".to_string()),
            cwd: Some("/workspace/cmux-next".to_string()),
            cols: 80,
            rows: 24,
            created_at: 0,
            workspace_id: Some("ws-main".to_string()),
            exit_code: None,
            is_alive: true,
            active_command: Some("cargo test -p tamux-daemon".to_string()),
        };

        let (summary, entities) = build_session_end_episode_payload(
            &session_id.to_string(),
            Some(&info),
            Some("running\n test result: ok. 3 passed"),
        );

        assert!(summary.contains("Cargo test runner ended in cmux-next"));
        assert!(summary.contains("Last output: test result: ok. 3 passed"));
        assert!(entities.contains(&format!("session:{session_id}")));
        assert!(entities.contains(&"workspace:ws-main".to_string()));
        assert!(entities.contains(&"cwd:cmux-next".to_string()));
        assert!(entities.contains(&"command:cargo".to_string()));
        assert!(entities.contains(&"title:cargo-test-runner".to_string()));
    }

    #[tokio::test]
    async fn gateway_bootstrap_uses_persisted_cursor_and_thread_state() {
        let root = std::env::current_dir()
            .expect("cwd")
            .join("tmp")
            .join(format!("server-gateway-bootstrap-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).expect("create test root");

        let history = Arc::new(
            HistoryStore::new_test_store(&root)
                .await
                .expect("create test history"),
        );

        history
            .save_gateway_replay_cursor("slack", "C123", "1712345678.000100", "message_ts")
            .await
            .expect("persist cursor");
        history
            .upsert_gateway_thread_binding("Slack:C123", "thread-123", 1234)
            .await
            .expect("persist binding");
        history
            .upsert_gateway_route_mode("Slack:C123", "swarog", 2345)
            .await
            .expect("persist route mode");

        let mut config = AgentConfig::default();
        config.gateway.enabled = true;
        config.gateway.slack_token = "slack-token".to_string();
        config.gateway.slack_channel_filter = "C123".to_string();
        config.gateway.command_prefix = "!tamux".to_string();
        config.gateway.gateway_electron_bridges_enabled = true;

        let manager =
            SessionManager::new_with_history(history.clone(), config.pty_channel_capacity);
        let agent = AgentEngine::new_with_shared_history(manager, config, history.clone());

        let payload = build_gateway_bootstrap_payload(&agent, "boot-test".to_string()).await;

        assert_eq!(payload.bootstrap_correlation_id, "boot-test");
        assert!(payload
            .feature_flags
            .contains(&"gateway_enabled".to_string()));
        assert!(payload
            .feature_flags
            .contains(&"gateway_electron_bridges_enabled".to_string()));
        assert!(payload
            .providers
            .iter()
            .any(|provider| provider.platform == "slack" && provider.enabled));
        assert!(payload
            .continuity
            .cursors
            .iter()
            .any(|cursor| cursor.platform == "slack" && cursor.channel_id == "C123"));
        assert!(payload
            .continuity
            .thread_bindings
            .iter()
            .any(|binding| binding.channel_key == "Slack:C123"
                && binding.thread_id.as_deref() == Some("thread-123")));
        assert!(payload
            .continuity
            .route_modes
            .iter()
            .any(|mode| mode.channel_key == "Slack:C123"
                && matches!(mode.route_mode, amux_protocol::GatewayRouteMode::Swarog)));
        assert!(payload
            .continuity
            .cursors
            .iter()
            .any(|cursor| cursor.platform == "slack" && cursor.channel_id == "C123"));

        let _ = std::fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn gateway_bootstrap_restores_health_snapshots() {
        let root = std::env::current_dir()
            .expect("cwd")
            .join("tmp")
            .join(format!("server-gateway-health-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).expect("create test root");

        let history = Arc::new(
            HistoryStore::new_test_store(&root)
                .await
                .expect("create test history"),
        );
        history
            .upsert_gateway_health_snapshot(
                &amux_protocol::GatewayHealthState {
                    platform: "slack".to_string(),
                    status: amux_protocol::GatewayConnectionStatus::Error,
                    last_success_at_ms: Some(111),
                    last_error_at_ms: Some(222),
                    consecutive_failure_count: 3,
                    last_error: Some("timeout".to_string()),
                    current_backoff_secs: 30,
                },
                333,
            )
            .await
            .expect("persist health snapshot");

        let mut config = AgentConfig::default();
        config.gateway.enabled = true;
        let manager =
            SessionManager::new_with_history(history.clone(), config.pty_channel_capacity);
        let agent = AgentEngine::new_with_shared_history(manager, config, history.clone());

        agent.init_gateway().await;

        let state_guard = agent.gateway_state.lock().await;
        let state = state_guard.as_ref().expect("gateway state should exist");
        assert_eq!(
            state.slack_health.status,
            crate::agent::RuntimeGatewayConnectionStatus::Error
        );
        assert_eq!(state.slack_health.last_success_at, Some(111));
        assert_eq!(state.slack_health.last_error_at, Some(222));
        assert_eq!(state.slack_health.consecutive_failure_count, 3);
        assert_eq!(state.slack_health.last_error.as_deref(), Some("timeout"));
        assert_eq!(state.slack_health.current_backoff_secs, 30);

        let payload = build_gateway_bootstrap_payload(&agent, "boot-health".to_string()).await;
        assert_eq!(payload.continuity.health_snapshots.len(), 1);
        assert_eq!(payload.continuity.health_snapshots[0].platform, "slack");
        assert_eq!(
            payload.continuity.health_snapshots[0].status,
            GatewayConnectionStatus::Error
        );

        drop(state_guard);
        let _ = std::fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn gateway_incoming_ipc_event_enqueues_agent_processing_without_poll_loop() {
        let root = std::env::current_dir()
            .expect("cwd")
            .join("tmp")
            .join(format!("server-gateway-ipc-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).expect("create test root");

        let history = Arc::new(
            HistoryStore::new_test_store(&root)
                .await
                .expect("create test history"),
        );
        let mut config = AgentConfig::default();
        config.gateway.enabled = true;
        let manager =
            SessionManager::new_with_history(history.clone(), config.pty_channel_capacity);
        let agent = AgentEngine::new_with_shared_history(manager, config, history.clone());
        agent.init_gateway().await;

        enqueue_gateway_incoming_event(
            &agent,
            GatewayIncomingEvent {
                platform: "Slack".to_string(),
                channel_id: "C123".to_string(),
                sender_id: "U123".to_string(),
                sender_display: Some("Alice".to_string()),
                content: "hello".to_string(),
                message_id: Some("msg-1".to_string()),
                thread_id: Some("thread-1".to_string()),
                received_at_ms: 1234,
                raw_event_json: None,
            },
        )
        .await
        .expect("enqueue gateway event");

        let queue = agent.gateway_injected_messages.lock().await;
        assert_eq!(queue.len(), 1);
        let queued = queue.front().expect("queued gateway message");
        let thread_context = queued
            .thread_context
            .as_ref()
            .expect("thread context should be preserved");
        assert_eq!(thread_context.slack_thread_ts.as_deref(), Some("thread-1"));
        assert_eq!(thread_context.discord_message_id, None);
        assert_eq!(thread_context.telegram_message_id, None);

        let _ = std::fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn gateway_health_updates_emit_status_and_digest_events() {
        let root = std::env::current_dir()
            .expect("cwd")
            .join("tmp")
            .join(format!(
                "server-gateway-health-events-{}",
                uuid::Uuid::new_v4()
            ));
        std::fs::create_dir_all(&root).expect("create test root");

        let history = Arc::new(
            HistoryStore::new_test_store(&root)
                .await
                .expect("create test history"),
        );
        let mut config = AgentConfig::default();
        config.gateway.enabled = true;
        let manager =
            SessionManager::new_with_history(history.clone(), config.pty_channel_capacity);
        let agent = AgentEngine::new_with_shared_history(manager, config, history.clone());
        agent.init_gateway().await;
        let mut events = agent.subscribe();

        persist_gateway_health_update(
            &agent,
            GatewayHealthState {
                platform: "slack".to_string(),
                status: GatewayConnectionStatus::Error,
                last_success_at_ms: Some(100),
                last_error_at_ms: Some(200),
                consecutive_failure_count: 2,
                last_error: Some("timeout".to_string()),
                current_backoff_secs: 30,
            },
        )
        .await
        .expect("persist health update");

        let status_event = timeout(Duration::from_secs(1), async {
            loop {
                match events.recv().await.expect("gateway status event") {
                    crate::agent::types::AgentEvent::GatewayStatus {
                        platform,
                        status,
                        last_error,
                        consecutive_failures,
                    } => {
                        break (platform, status, last_error, consecutive_failures);
                    }
                    _ => {}
                }
            }
        })
        .await
        .expect("timed out waiting for gateway status");
        assert_eq!(status_event.0, "Slack");
        assert_eq!(status_event.1, "error");
        assert_eq!(status_event.2.as_deref(), Some("timeout"));
        assert_eq!(status_event.3, Some(2));

        let digest_event = timeout(Duration::from_secs(1), async {
            loop {
                match events.recv().await.expect("heartbeat digest event") {
                    crate::agent::types::AgentEvent::HeartbeatDigest {
                        actionable, digest, ..
                    } => break (actionable, digest),
                    _ => {}
                }
            }
        })
        .await
        .expect("timed out waiting for heartbeat digest");
        assert!(digest_event.0);
        assert!(digest_event.1.contains("Slack disconnected"));

        let audits = agent
            .history
            .list_action_audit(None, None, 20)
            .await
            .expect("list action audits");
        assert!(audits.iter().any(|entry| {
            entry.action_type == "gateway_health_transition"
                && entry.summary.contains("Slack -> error")
        }));

        let _ = std::fs::remove_dir_all(root);
    }

    struct TestConnection {
        framed: Framed<DuplexStream, AmuxCodec>,
        task: JoinHandle<anyhow::Result<()>>,
        root: PathBuf,
        agent: Arc<AgentEngine>,
    }

    impl TestConnection {
        async fn recv(&mut self) -> DaemonMessage {
            timeout(Duration::from_millis(500), self.framed.next())
                .await
                .expect("timed out waiting for daemon message")
                .expect("connection closed")
                .expect("codec failure")
        }

        async fn shutdown(self) {
            let TestConnection {
                framed,
                task,
                root,
                agent: _,
            } = self;
            drop(framed);
            let join = timeout(Duration::from_secs(2), task)
                .await
                .expect("server task did not shut down in time")
                .expect("server task join failed");
            join.expect("server task returned error");
            let _ = std::fs::remove_dir_all(root);
        }
    }

    async fn spawn_test_connection_with_config(agent_config: AgentConfig) -> TestConnection {
        let root = std::env::current_dir()
            .expect("cwd")
            .join("tmp")
            .join(format!(
                "server-whatsapp-link-test-{}",
                uuid::Uuid::new_v4()
            ));
        std::fs::create_dir_all(&root).expect("create test root");

        let history = Arc::new(
            HistoryStore::new_test_store(&root)
                .await
                .expect("create test history"),
        );
        let manager =
            SessionManager::new_with_history(history.clone(), agent_config.pty_channel_capacity);
        let agent =
            AgentEngine::new_with_shared_history(manager.clone(), agent_config, history.clone());
        let plugin_manager = Arc::new(PluginManager::new(history, root.join("plugins")));

        let (client_stream, server_stream) = tokio::io::duplex(128 * 1024);
        let server_task = tokio::spawn(handle_connection(
            server_stream,
            manager,
            agent.clone(),
            plugin_manager,
        ));

        TestConnection {
            framed: Framed::new(client_stream, AmuxCodec),
            task: server_task,
            root,
            agent,
        }
    }

    async fn spawn_test_connection() -> TestConnection {
        spawn_test_connection_with_config(AgentConfig::default()).await
    }

    async fn register_gateway(conn: &mut TestConnection) -> String {
        conn.framed
            .send(ClientMessage::GatewayRegister {
                registration: GatewayRegistration {
                    gateway_id: "gateway-main".to_string(),
                    instance_id: "instance-01".to_string(),
                    protocol_version: GATEWAY_IPC_PROTOCOL_VERSION,
                    supported_platforms: vec!["slack".to_string(), "discord".to_string()],
                    process_id: Some(4242),
                },
            })
            .await
            .expect("send gateway register");
        match conn.recv().await {
            DaemonMessage::GatewayBootstrap { payload } => payload.bootstrap_correlation_id,
            other => panic!("expected GatewayBootstrap, got {other:?}"),
        }
    }

    async fn acknowledge_gateway_bootstrap(conn: &mut TestConnection, correlation_id: String) {
        conn.framed
            .send(ClientMessage::GatewayAck {
                ack: amux_protocol::GatewayAck {
                    correlation_id,
                    accepted: true,
                    detail: Some("bootstrap applied".to_string()),
                },
            })
            .await
            .expect("send gateway ack");
    }

    #[tokio::test]
    async fn gateway_register_rejects_incompatible_protocol_version() {
        let mut conn = spawn_test_connection().await;

        conn.framed
            .send(ClientMessage::GatewayRegister {
                registration: GatewayRegistration {
                    gateway_id: "gateway-main".to_string(),
                    instance_id: "instance-01".to_string(),
                    protocol_version: GATEWAY_IPC_PROTOCOL_VERSION + 1,
                    supported_platforms: vec!["slack".to_string()],
                    process_id: None,
                },
            })
            .await
            .expect("send gateway register");

        match conn.recv().await {
            DaemonMessage::Error { message } => {
                assert!(message.contains("unsupported gateway protocol version"))
            }
            other => panic!("expected Error, got {other:?}"),
        }

        let closed = timeout(Duration::from_millis(250), conn.framed.next()).await;
        assert!(
            matches!(closed, Ok(None)),
            "connection should close after version mismatch"
        );
        conn.shutdown().await;
    }

    #[tokio::test]
    async fn gateway_updates_require_registration_and_bootstrap_ack() {
        let mut conn = spawn_test_connection().await;

        conn.framed
            .send(ClientMessage::GatewayCursorUpdate {
                update: amux_protocol::GatewayCursorState {
                    platform: "slack".to_string(),
                    channel_id: "C123".to_string(),
                    cursor_value: "1712345678.000100".to_string(),
                    cursor_type: "message_ts".to_string(),
                    updated_at_ms: 123,
                },
            })
            .await
            .expect("send cursor update before register");
        match conn.recv().await {
            DaemonMessage::Error { message } => {
                assert!(message.contains("gateway cursor updates require"))
            }
            other => panic!("expected Error, got {other:?}"),
        }

        let correlation_id = register_gateway(&mut conn).await;
        conn.framed
            .send(ClientMessage::GatewayHealthUpdate {
                update: amux_protocol::GatewayHealthState {
                    platform: "slack".to_string(),
                    status: amux_protocol::GatewayConnectionStatus::Connected,
                    last_success_at_ms: Some(123),
                    last_error_at_ms: None,
                    consecutive_failure_count: 0,
                    last_error: None,
                    current_backoff_secs: 0,
                },
            })
            .await
            .expect("send health update before ack");
        match conn.recv().await {
            DaemonMessage::Error { message } => {
                assert!(message.contains("gateway health updates require"))
            }
            other => panic!("expected Error, got {other:?}"),
        }

        conn.framed
            .send(ClientMessage::GatewayAck {
                ack: amux_protocol::GatewayAck {
                    correlation_id: "wrong-token".to_string(),
                    accepted: true,
                    detail: None,
                },
            })
            .await
            .expect("send wrong ack");
        match conn.recv().await {
            DaemonMessage::Error { message } => {
                assert!(message.contains("invalid gateway bootstrap ack"))
            }
            other => panic!("expected Error, got {other:?}"),
        }

        acknowledge_gateway_bootstrap(&mut conn, correlation_id).await;
        conn.framed
            .send(ClientMessage::Ping)
            .await
            .expect("send ping after ack");
        assert!(matches!(conn.recv().await, DaemonMessage::Pong));

        conn.shutdown().await;
    }

    #[tokio::test]
    async fn gateway_updates_persist_cursor_thread_binding_and_route_mode_after_ack() {
        let mut conn = spawn_test_connection().await;
        let correlation_id = register_gateway(&mut conn).await;
        acknowledge_gateway_bootstrap(&mut conn, correlation_id).await;

        conn.framed
            .send(ClientMessage::GatewayCursorUpdate {
                update: amux_protocol::GatewayCursorState {
                    platform: "slack".to_string(),
                    channel_id: "C123".to_string(),
                    cursor_value: "1712345678.000100".to_string(),
                    cursor_type: "message_ts".to_string(),
                    updated_at_ms: 1111,
                },
            })
            .await
            .expect("send cursor update");
        conn.framed
            .send(ClientMessage::GatewayThreadBindingUpdate {
                update: amux_protocol::GatewayThreadBindingState {
                    channel_key: "Slack:C123".to_string(),
                    thread_id: Some("thread-123".to_string()),
                    updated_at_ms: 2222,
                },
            })
            .await
            .expect("send binding update");
        conn.framed
            .send(ClientMessage::GatewayRouteModeUpdate {
                update: amux_protocol::GatewayRouteModeState {
                    channel_key: "Slack:C123".to_string(),
                    route_mode: amux_protocol::GatewayRouteMode::Swarog,
                    updated_at_ms: 3333,
                },
            })
            .await
            .expect("send route mode update");

        conn.framed
            .send(ClientMessage::Ping)
            .await
            .expect("send ping barrier");
        assert!(matches!(conn.recv().await, DaemonMessage::Pong));

        let cursor = conn
            .agent
            .history
            .load_gateway_replay_cursor("slack", "C123")
            .await
            .expect("load cursor")
            .expect("cursor should exist");
        assert_eq!(cursor.cursor_value, "1712345678.000100");
        assert_eq!(cursor.cursor_type, "message_ts");

        let bindings = conn
            .agent
            .history
            .list_gateway_thread_bindings()
            .await
            .expect("list bindings");
        assert!(bindings.iter().any(
            |(channel_key, thread_id)| channel_key == "Slack:C123" && thread_id == "thread-123"
        ));

        let modes = conn
            .agent
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

        conn.shutdown().await;
    }

    #[tokio::test]
    async fn gateway_send_results_complete_waiters_and_update_last_response_state() {
        let mut config = AgentConfig::default();
        config.gateway.enabled = true;
        let mut conn = spawn_test_connection_with_config(config).await;
        let correlation_id = register_gateway(&mut conn).await;
        acknowledge_gateway_bootstrap(&mut conn, correlation_id).await;
        conn.agent.init_gateway().await;

        let agent = conn.agent.clone();
        let send_task = tokio::spawn(async move {
            agent
                .request_gateway_send(GatewaySendRequest {
                    correlation_id: "send-1".to_string(),
                    platform: "slack".to_string(),
                    channel_id: "C123".to_string(),
                    thread_id: Some("1712345678.000100".to_string()),
                    content: "hello".to_string(),
                })
                .await
        });

        let request = match conn.recv().await {
            DaemonMessage::GatewaySendRequest { request } => request,
            other => panic!("expected GatewaySendRequest, got {other:?}"),
        };
        assert_eq!(request.correlation_id, "send-1");
        assert_eq!(request.channel_id, "C123");

        conn.framed
            .send(ClientMessage::GatewaySendResult {
                result: amux_protocol::GatewaySendResult {
                    correlation_id: request.correlation_id.clone(),
                    platform: "slack".to_string(),
                    channel_id: "C123".to_string(),
                    requested_channel_id: Some("C123".to_string()),
                    delivery_id: Some("1712345678.000200".to_string()),
                    ok: true,
                    error: None,
                    completed_at_ms: 1234,
                },
            })
            .await
            .expect("send gateway result");

        let result = send_task
            .await
            .expect("join send task")
            .expect("gateway send should complete");
        assert!(result.ok);

        let gw_guard = conn.agent.gateway_state.lock().await;
        let gw = gw_guard.as_ref().expect("gateway state should exist");
        assert!(gw.last_response_at.contains_key("Slack:C123"));
        drop(gw_guard);

        conn.shutdown().await;
    }

    #[tokio::test]
    async fn gateway_send_results_use_canonical_discord_dm_channel_keys() {
        let mut config = AgentConfig::default();
        config.gateway.enabled = true;
        let mut conn = spawn_test_connection_with_config(config).await;
        let correlation_id = register_gateway(&mut conn).await;
        acknowledge_gateway_bootstrap(&mut conn, correlation_id).await;
        conn.agent.init_gateway().await;

        let agent = conn.agent.clone();
        let send_task = tokio::spawn(async move {
            agent
                .request_gateway_send(GatewaySendRequest {
                    correlation_id: "discord-dm-send".to_string(),
                    platform: "discord".to_string(),
                    channel_id: "user:123456789".to_string(),
                    thread_id: Some("987654321".to_string()),
                    content: "hello".to_string(),
                })
                .await
        });

        let request = match conn.recv().await {
            DaemonMessage::GatewaySendRequest { request } => request,
            other => panic!("expected GatewaySendRequest, got {other:?}"),
        };
        assert_eq!(request.channel_id, "user:123456789");

        conn.framed
            .send(ClientMessage::GatewaySendResult {
                result: amux_protocol::GatewaySendResult {
                    correlation_id: request.correlation_id.clone(),
                    platform: "discord".to_string(),
                    channel_id: "DM123".to_string(),
                    requested_channel_id: Some("user:123456789".to_string()),
                    delivery_id: Some("delivery-1".to_string()),
                    ok: true,
                    error: None,
                    completed_at_ms: 1234,
                },
            })
            .await
            .expect("send discord gateway result");

        let result = send_task
            .await
            .expect("join discord send task")
            .expect("gateway send should complete");
        assert!(result.ok);

        let gw_guard = conn.agent.gateway_state.lock().await;
        let gw = gw_guard.as_ref().expect("gateway state should exist");
        assert!(gw.last_response_at.contains_key("Discord:DM123"));
        assert!(!gw.last_response_at.contains_key("Discord:user:123456789"));
        assert_eq!(
            gw.discord_dm_channels_by_user
                .get("user:123456789")
                .map(String::as_str),
            Some("DM123")
        );
        assert_eq!(
            gw.reply_contexts
                .get("Discord:DM123")
                .and_then(|ctx| ctx.discord_message_id.as_deref()),
            Some("delivery-1")
        );
        drop(gw_guard);

        conn.shutdown().await;
    }

    #[tokio::test]
    async fn whatsapp_link_start_status_stop_send_status_responses() {
        let mut conn = spawn_test_connection().await;

        conn.framed
            .send(ClientMessage::AgentWhatsAppLinkStatus)
            .await
            .expect("send status request");
        match conn.recv().await {
            DaemonMessage::AgentWhatsAppLinkStatus { state, .. } => {
                assert_eq!(state, "disconnected")
            }
            other => panic!("expected AgentWhatsAppLinkStatus, got {other:?}"),
        }

        conn.framed
            .send(ClientMessage::AgentWhatsAppLinkStart)
            .await
            .expect("send start request");
        match conn.recv().await {
            DaemonMessage::AgentWhatsAppLinkStatus { state, .. } => assert_eq!(state, "starting"),
            other => panic!("expected AgentWhatsAppLinkStatus after start, got {other:?}"),
        }

        conn.framed
            .send(ClientMessage::AgentWhatsAppLinkStop)
            .await
            .expect("send stop request");
        match conn.recv().await {
            DaemonMessage::AgentWhatsAppLinkStatus { state, .. } => {
                assert_eq!(state, "disconnected")
            }
            other => panic!("expected AgentWhatsAppLinkStatus after stop, got {other:?}"),
        }

        conn.shutdown().await;
    }

    #[tokio::test]
    async fn whatsapp_link_reset_clears_link_state() {
        let mut conn = spawn_test_connection().await;

        conn.agent
            .whatsapp_link
            .broadcast_qr("QR-RESET".to_string(), Some(123))
            .await;
        conn.agent
            .whatsapp_link
            .broadcast_linked(Some("+15551112222".to_string()))
            .await;
        crate::agent::save_persisted_provider_state(
            &conn.agent.history,
            crate::agent::WHATSAPP_LINK_PROVIDER_ID,
            crate::agent::WhatsAppPersistedState {
                linked_phone: Some("+15551112222".to_string()),
                auth_json: Some("{\"session\":true}".to_string()),
                metadata_json: Some("{\"source\":\"server-test\"}".to_string()),
                last_reset_at: None,
                last_linked_at: Some(5),
                updated_at: 6,
            },
        )
        .await
        .expect("persist whatsapp provider state");

        conn.framed
            .send(ClientMessage::AgentWhatsAppLinkReset)
            .await
            .expect("send reset request");
        match conn.recv().await {
            DaemonMessage::AgentWhatsAppLinkReset { ok, .. } => assert!(ok),
            other => panic!("expected AgentWhatsAppLinkReset, got {other:?}"),
        }

        conn.framed
            .send(ClientMessage::AgentWhatsAppLinkStatus)
            .await
            .expect("send status request after reset");
        match conn.recv().await {
            DaemonMessage::AgentWhatsAppLinkStatus {
                state,
                phone,
                last_error,
            } => {
                assert_eq!(state, "disconnected");
                assert!(phone.is_none());
                assert!(last_error.is_none());
            }
            other => panic!("expected AgentWhatsAppLinkStatus after reset, got {other:?}"),
        }
        assert!(
            crate::agent::load_persisted_provider_state(
                &conn.agent.history,
                crate::agent::WHATSAPP_LINK_PROVIDER_ID,
            )
            .await
            .expect("load persisted provider state")
            .is_none(),
            "reset should remove persisted provider state"
        );

        conn.shutdown().await;
    }

    #[tokio::test]
    async fn whatsapp_link_subscribe_then_unsubscribe_stops_forwarding() {
        let mut conn = spawn_test_connection().await;

        conn.framed
            .send(ClientMessage::AgentWhatsAppLinkSubscribe)
            .await
            .expect("send subscribe request");
        assert!(
            matches!(
                conn.recv().await,
                DaemonMessage::AgentWhatsAppLinkStatus { .. }
            ),
            "subscribe should replay status snapshot"
        );

        conn.framed
            .send(ClientMessage::AgentWhatsAppLinkUnsubscribe)
            .await
            .expect("send unsubscribe request");
        conn.framed
            .send(ClientMessage::Ping)
            .await
            .expect("send ping barrier");
        assert!(
            matches!(conn.recv().await, DaemonMessage::Pong),
            "ping barrier should confirm unsubscribe was processed"
        );
        conn.agent
            .whatsapp_link
            .broadcast_qr("QR-UNSUB".to_string(), Some(123))
            .await;

        let maybe_msg = timeout(Duration::from_millis(150), conn.framed.next()).await;
        assert!(
            maybe_msg.is_err(),
            "no whatsapp link event should be forwarded after unsubscribe"
        );

        conn.shutdown().await;
    }

    #[tokio::test]
    async fn whatsapp_link_subscription_replay_status_then_incremental_events() {
        let mut conn = spawn_test_connection().await;

        conn.framed
            .send(ClientMessage::AgentWhatsAppLinkSubscribe)
            .await
            .expect("send subscribe request");
        assert!(
            matches!(
                conn.recv().await,
                DaemonMessage::AgentWhatsAppLinkStatus { .. }
            ),
            "first replayed event should be status snapshot"
        );

        conn.agent
            .whatsapp_link
            .broadcast_qr("QR-ORDER".to_string(), Some(111))
            .await;
        conn.agent
            .whatsapp_link
            .broadcast_linked(Some("+15550001111".to_string()))
            .await;
        conn.agent
            .whatsapp_link
            .broadcast_error("recoverable".to_string(), true)
            .await;
        conn.agent
            .whatsapp_link
            .broadcast_disconnected(Some("operator_cancelled".to_string()))
            .await;

        match conn.recv().await {
            DaemonMessage::AgentWhatsAppLinkQr { ascii_qr, .. } => assert_eq!(ascii_qr, "QR-ORDER"),
            other => panic!("expected AgentWhatsAppLinkQr, got {other:?}"),
        }
        match conn.recv().await {
            DaemonMessage::AgentWhatsAppLinked { phone } => {
                assert_eq!(phone.as_deref(), Some("+15550001111"))
            }
            other => panic!("expected AgentWhatsAppLinked, got {other:?}"),
        }
        match conn.recv().await {
            DaemonMessage::AgentWhatsAppLinkError {
                message,
                recoverable,
            } => {
                assert_eq!(message, "recoverable");
                assert!(recoverable);
            }
            other => panic!("expected AgentWhatsAppLinkError, got {other:?}"),
        }
        match conn.recv().await {
            DaemonMessage::AgentWhatsAppLinkDisconnected { reason } => {
                assert_eq!(reason.as_deref(), Some("operator_cancelled"))
            }
            other => panic!("expected AgentWhatsAppLinkDisconnected, got {other:?}"),
        }

        conn.shutdown().await;
    }

    #[tokio::test]
    async fn whatsapp_link_subscriber_is_cleaned_up_on_disconnect_without_unsubscribe() {
        let mut conn = spawn_test_connection().await;
        conn.framed
            .send(ClientMessage::AgentWhatsAppLinkSubscribe)
            .await
            .expect("send subscribe request");
        assert!(
            matches!(
                conn.recv().await,
                DaemonMessage::AgentWhatsAppLinkStatus { .. }
            ),
            "subscribe should replay status snapshot"
        );

        assert_eq!(
            conn.agent.whatsapp_link.subscriber_count().await,
            1,
            "subscriber should be registered after subscribe"
        );

        let agent = conn.agent.clone();
        conn.shutdown().await;

        assert_eq!(
            agent.whatsapp_link.subscriber_count().await,
            0,
            "subscriber should be removed when connection exits"
        );
    }

    #[tokio::test]
    async fn divergent_ipc_get_session_returns_completion_payload() {
        let mut conn = spawn_test_connection().await;
        let thread_id = "thread-divergent-server";
        let session_id = conn
            .agent
            .start_divergent_session("evaluate rollout strategy", None, thread_id, None)
            .await
            .expect("start divergent session");

        // Record contributions and complete session to synthesize retrieval payload.
        let framing_labels = vec!["analytical-lens".to_string(), "pragmatic-lens".to_string()];
        for (idx, label) in framing_labels.iter().enumerate() {
            conn.agent
                .record_divergent_contribution(
                    &session_id,
                    label,
                    if idx == 0 {
                        "Prefer conservative phased rollout"
                    } else {
                        "Prefer fast rollout with rollback hooks"
                    },
                )
                .await
                .expect("contribution recording should succeed");
        }
        conn.agent
            .complete_divergent_session(&session_id)
            .await
            .expect("session completion should succeed");

        conn.framed
            .send(ClientMessage::AgentGetDivergentSession {
                session_id: session_id.clone(),
            })
            .await
            .expect("send retrieval request");

        let payload = match conn.recv().await {
            DaemonMessage::AgentDivergentSession { session_json } => {
                serde_json::from_str::<serde_json::Value>(&session_json)
                    .expect("session payload should decode")
            }
            other => panic!("expected AgentDivergentSession, got {other:?}"),
        };
        assert_eq!(
            payload.get("session_id").and_then(|v| v.as_str()),
            Some(session_id.as_str())
        );
        assert_eq!(
            payload.get("status").and_then(|v| v.as_str()),
            Some("complete")
        );
        assert!(payload
            .get("tensions_markdown")
            .and_then(|v| v.as_str())
            .is_some_and(|v| !v.is_empty()));
        assert!(payload
            .get("mediator_prompt")
            .and_then(|v| v.as_str())
            .is_some_and(|v| !v.is_empty()));

        conn.shutdown().await;
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
                | crate::agent::types::AgentEvent::GatewayIncoming { .. }
                | crate::agent::types::AgentEvent::BudgetAlert { .. }
                | crate::agent::types::AgentEvent::TrajectoryUpdate { .. }
                | crate::agent::types::AgentEvent::EpisodeRecorded { .. }
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
async fn run_unix(
    manager: Arc<SessionManager>,
    agent: Arc<AgentEngine>,
    plugin_manager: Arc<crate::plugin::PluginManager>,
) -> Result<()> {
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
                    if let Err(e) = handle_connection(stream, manager, agent, plugin_manager).await
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
                    if let Err(e) = handle_connection(stream, manager, agent, plugin_manager).await
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
    let mut last_concierge_welcome_fingerprint: Option<String> = None;

    // Optional agent event subscription.
    let mut agent_event_rx: Option<broadcast::Receiver<crate::agent::types::AgentEvent>> = None;
    let mut whatsapp_link_rx: Option<
        broadcast::Receiver<crate::agent::types::WhatsAppLinkRuntimeEvent>,
    > = None;
    let mut gateway_ipc_rx: Option<mpsc::UnboundedReceiver<DaemonMessage>> = None;
    let mut whatsapp_link_subscriber_guard = WhatsAppLinkSubscriberGuard::new(agent.clone());
    let mut whatsapp_link_snapshot_replayed = false;
    let mut gateway_connection_state = GatewayConnectionState::Unregistered;

    loop {
        // Drain agent events if subscribed.
        if let Some(ref mut rx) = agent_event_rx {
            loop {
                match rx.try_recv() {
                    Ok(event) => {
                        if should_forward_agent_event(&event, &client_agent_threads) {
                            if let Some(fingerprint) = concierge_welcome_fingerprint(&event) {
                                if last_concierge_welcome_fingerprint.as_deref()
                                    == Some(fingerprint.as_str())
                                {
                                    continue;
                                }
                                last_concierge_welcome_fingerprint = Some(fingerprint);
                            }
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
        if let Some(ref mut rx) = whatsapp_link_rx {
            loop {
                match rx.try_recv() {
                    Ok(event) => match event {
                        crate::agent::types::WhatsAppLinkRuntimeEvent::Status(snapshot) => {
                            tracing::debug!(
                                state = %snapshot.state,
                                has_error = snapshot.last_error.is_some(),
                                "forwarding whatsapp runtime status to client"
                            );
                            if !whatsapp_link_snapshot_replayed {
                                whatsapp_link_snapshot_replayed = true;
                                framed
                                    .send(DaemonMessage::AgentWhatsAppLinkStatus {
                                        state: snapshot.state,
                                        phone: snapshot.phone,
                                        last_error: snapshot.last_error,
                                    })
                                    .await?;
                            }
                        }
                        crate::agent::types::WhatsAppLinkRuntimeEvent::Qr {
                            ascii_qr,
                            expires_at_ms,
                        } => {
                            tracing::debug!(
                                qr_len = ascii_qr.len(),
                                expires_at_ms,
                                "forwarding whatsapp runtime qr to client"
                            );
                            framed
                                .send(DaemonMessage::AgentWhatsAppLinkQr {
                                    ascii_qr,
                                    expires_at_ms,
                                })
                                .await?;
                        }
                        crate::agent::types::WhatsAppLinkRuntimeEvent::Linked { phone } => {
                            tracing::debug!(
                                phone = phone.as_deref().unwrap_or(""),
                                "forwarding whatsapp runtime linked to client"
                            );
                            framed
                                .send(DaemonMessage::AgentWhatsAppLinked { phone })
                                .await?;
                        }
                        crate::agent::types::WhatsAppLinkRuntimeEvent::Error {
                            message,
                            recoverable,
                        } => {
                            tracing::debug!(
                                recoverable,
                                message = %message,
                                "forwarding whatsapp runtime error to client"
                            );
                            framed
                                .send(DaemonMessage::AgentWhatsAppLinkError {
                                    message,
                                    recoverable,
                                })
                                .await?;
                        }
                        crate::agent::types::WhatsAppLinkRuntimeEvent::Disconnected { reason } => {
                            tracing::debug!(
                                reason = reason.as_deref().unwrap_or(""),
                                "forwarding whatsapp runtime disconnected to client"
                            );
                            framed
                                .send(DaemonMessage::AgentWhatsAppLinkDisconnected { reason })
                                .await?;
                        }
                    },
                    Err(broadcast::error::TryRecvError::Lagged(n)) => {
                        tracing::warn!(skipped = n, "whatsapp link broadcast lagged");
                        break;
                    }
                    _ => break,
                }
            }
        }
        if let Some(ref mut rx) = gateway_ipc_rx {
            loop {
                match rx.try_recv() {
                    Ok(daemon_msg) => {
                        framed.send(daemon_msg).await?;
                    }
                    Err(tokio::sync::mpsc::error::TryRecvError::Empty) => break,
                    Err(tokio::sync::mpsc::error::TryRecvError::Disconnected) => break,
                }
            }
        }

        // We need to select between: incoming client messages and output from attached sessions.
        let has_subscriptions = !attached_rxs.is_empty()
            || agent_event_rx.is_some()
            || whatsapp_link_rx.is_some()
            || gateway_ipc_rx.is_some();
        let msg = if !has_subscriptions {
            // No attached sessions or agent subscription — just wait for client input.
            match framed.next().await {
                Some(Ok(msg)) => Some(msg),
                Some(Err(e)) => {
                    if gateway_connection_is_tracked(&gateway_connection_state) {
                        agent
                            .record_gateway_ipc_loss("gateway connection error")
                            .await;
                    }
                    return Err(e.into());
                }
                None => {
                    if gateway_connection_is_tracked(&gateway_connection_state) {
                        agent
                            .record_gateway_ipc_loss("gateway connection closed")
                            .await;
                    }
                    return Ok(()); // client disconnected
                }
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
                Ok(Some(Err(e))) => {
                    if gateway_connection_is_tracked(&gateway_connection_state) {
                        agent
                            .record_gateway_ipc_loss("gateway connection error")
                            .await;
                    }
                    return Err(e.into());
                }
                Ok(None) => {
                    if gateway_connection_is_tracked(&gateway_connection_state) {
                        agent
                            .record_gateway_ipc_loss("gateway connection closed")
                            .await;
                    }
                    return Ok(());
                }
                Err(_) => None, // timeout — loop back to drain output
            }
        };

        if let Some(msg) = msg {
            match msg {
                ClientMessage::Ping => {
                    framed.send(DaemonMessage::Pong).await?;
                }

                ClientMessage::GatewayRegister { registration } => {
                    if !matches!(
                        gateway_connection_state,
                        GatewayConnectionState::Unregistered
                    ) {
                        framed
                            .send(DaemonMessage::Error {
                                message: "gateway runtime already registered on this connection"
                                    .to_string(),
                            })
                            .await?;
                        continue;
                    }
                    if registration.protocol_version != GATEWAY_IPC_PROTOCOL_VERSION {
                        framed
                            .send(DaemonMessage::Error {
                                message: format!(
                                    "unsupported gateway protocol version {} (expected {})",
                                    registration.protocol_version, GATEWAY_IPC_PROTOCOL_VERSION
                                ),
                            })
                            .await?;
                        return Ok(());
                    }
                    tracing::info!(
                        gateway_id = %registration.gateway_id,
                        instance_id = %registration.instance_id,
                        protocol_version = registration.protocol_version,
                        "gateway runtime registered on daemon socket"
                    );
                    let bootstrap_correlation_id =
                        format!("gateway-bootstrap:{}", uuid::Uuid::new_v4());
                    let payload =
                        build_gateway_bootstrap_payload(&agent, bootstrap_correlation_id.clone())
                            .await;
                    let (gateway_tx, gateway_rx) = mpsc::unbounded_channel();
                    agent.set_gateway_ipc_sender(Some(gateway_tx)).await;
                    gateway_ipc_rx = Some(gateway_rx);
                    framed
                        .send(DaemonMessage::GatewayBootstrap { payload })
                        .await?;
                    gateway_connection_state = GatewayConnectionState::AwaitingBootstrapAck {
                        registration,
                        bootstrap_correlation_id,
                    };
                }

                ClientMessage::GatewayAck { ack } => match &gateway_connection_state {
                    GatewayConnectionState::AwaitingBootstrapAck {
                        registration,
                        bootstrap_correlation_id,
                    } if ack.correlation_id == *bootstrap_correlation_id && ack.accepted => {
                        tracing::info!(
                            gateway_id = %registration.gateway_id,
                            instance_id = %registration.instance_id,
                            correlation_id = %ack.correlation_id,
                            "gateway runtime bootstrap acknowledged"
                        );
                        gateway_connection_state = GatewayConnectionState::Active {
                            registration: registration.clone(),
                        };
                    }
                    GatewayConnectionState::AwaitingBootstrapAck {
                        bootstrap_correlation_id,
                        ..
                    } => {
                        framed
                                .send(DaemonMessage::Error {
                                    message: format!(
                                        "invalid gateway bootstrap ack: expected correlation_id {} and accepted=true",
                                        bootstrap_correlation_id
                                    ),
                                })
                                .await?;
                    }
                    _ => {
                        framed
                            .send(DaemonMessage::Error {
                                message: "gateway ack received before gateway registration"
                                    .to_string(),
                            })
                            .await?;
                    }
                },

                ClientMessage::GatewayIncomingEvent { event } => {
                    if !gateway_connection_is_active(&gateway_connection_state) {
                        framed
                            .send(DaemonMessage::Error {
                                message:
                                    "gateway incoming events require a registered gateway connection"
                                        .to_string(),
                            })
                            .await?;
                        continue;
                    }
                    if let Err(error) = enqueue_gateway_incoming_event(&agent, event).await {
                        framed
                            .send(DaemonMessage::Error {
                                message: format!("failed to enqueue gateway event: {error}"),
                            })
                            .await?;
                    }
                }

                ClientMessage::GatewayCursorUpdate { update } => {
                    if !gateway_connection_is_active(&gateway_connection_state) {
                        framed
                            .send(DaemonMessage::Error {
                                message:
                                    "gateway cursor updates require a registered gateway connection"
                                        .to_string(),
                            })
                            .await?;
                        continue;
                    }
                    if let Err(error) = agent
                        .history
                        .save_gateway_replay_cursor(
                            &update.platform,
                            &update.channel_id,
                            &update.cursor_value,
                            &update.cursor_type,
                        )
                        .await
                    {
                        framed
                            .send(DaemonMessage::Error {
                                message: format!("failed to persist gateway cursor: {error}"),
                            })
                            .await?;
                    }
                }

                ClientMessage::GatewayThreadBindingUpdate { update } => {
                    if !gateway_connection_is_active(&gateway_connection_state) {
                        framed
                            .send(DaemonMessage::Error {
                                message: "gateway thread binding updates require a registered gateway connection".to_string(),
                            })
                            .await?;
                        continue;
                    }
                    let result = match update.thread_id.as_deref() {
                        Some(thread_id) => {
                            let updated_at = if update.updated_at_ms == 0 {
                                current_time_ms()
                            } else {
                                update.updated_at_ms
                            };
                            agent
                                .history
                                .upsert_gateway_thread_binding(
                                    &update.channel_key,
                                    thread_id,
                                    updated_at,
                                )
                                .await
                        }
                        None => {
                            agent
                                .history
                                .delete_gateway_thread_binding(&update.channel_key)
                                .await
                        }
                    };
                    if let Err(error) = result {
                        framed
                            .send(DaemonMessage::Error {
                                message: format!(
                                    "failed to persist gateway thread binding: {error}"
                                ),
                            })
                            .await?;
                    }
                }

                ClientMessage::GatewayRouteModeUpdate { update } => {
                    if !gateway_connection_is_active(&gateway_connection_state) {
                        framed
                            .send(DaemonMessage::Error {
                                message:
                                    "gateway route mode updates require a registered gateway connection"
                                        .to_string(),
                            })
                            .await?;
                        continue;
                    }
                    let result = agent
                        .history
                        .upsert_gateway_route_mode(
                            &update.channel_key,
                            match update.route_mode {
                                GatewayRouteMode::Rarog => "rarog",
                                GatewayRouteMode::Swarog => "swarog",
                            },
                            if update.updated_at_ms == 0 {
                                current_time_ms()
                            } else {
                                update.updated_at_ms
                            },
                        )
                        .await;
                    if let Err(error) = result {
                        framed
                            .send(DaemonMessage::Error {
                                message: format!("failed to persist gateway route mode: {error}"),
                            })
                            .await?;
                    }
                }

                ClientMessage::GatewaySendResult { result } => {
                    if !gateway_connection_is_active(&gateway_connection_state) {
                        framed
                            .send(DaemonMessage::Error {
                                message:
                                    "gateway send results require a registered gateway connection"
                                        .to_string(),
                            })
                            .await?;
                        continue;
                    }
                    tracing::debug!(
                        correlation_id = %result.correlation_id,
                        platform = %result.platform,
                        ok = result.ok,
                        "gateway send result received"
                    );
                    if result.ok {
                        if let Some(channel_key) =
                            gateway_response_channel_key(&result.platform, &result.channel_id)
                        {
                            let mut gw_guard = agent.gateway_state.lock().await;
                            if let Some(gateway_state) = gw_guard.as_mut() {
                                if result.platform.eq_ignore_ascii_case("discord") {
                                    if let Some(requested_channel_id) =
                                        result.requested_channel_id.as_deref()
                                    {
                                        if requested_channel_id.starts_with("user:")
                                            && requested_channel_id != result.channel_id
                                        {
                                            gateway_state.discord_dm_channels_by_user.insert(
                                                requested_channel_id.to_string(),
                                                result.channel_id.clone(),
                                            );
                                        }
                                    }
                                    if let Some(delivery_id) = result.delivery_id.as_ref() {
                                        gateway_state.reply_contexts.insert(
                                            channel_key.clone(),
                                            crate::agent::gateway::ThreadContext {
                                                discord_message_id: Some(delivery_id.clone()),
                                                ..Default::default()
                                            },
                                        );
                                    }
                                }
                                gateway_state
                                    .last_response_at
                                    .insert(channel_key, current_time_ms());
                            }
                        }
                    }
                    if !agent.complete_gateway_send_result(result.clone()).await {
                        tracing::debug!(
                            correlation_id = %result.correlation_id,
                            "gateway send result had no waiting caller"
                        );
                    }
                }

                ClientMessage::GatewayHealthUpdate { update } => {
                    if !gateway_connection_is_active(&gateway_connection_state) {
                        framed
                            .send(DaemonMessage::Error {
                                message:
                                    "gateway health updates require a registered gateway connection"
                                        .to_string(),
                            })
                            .await?;
                        continue;
                    }
                    let snapshot = update;
                    let status_label = match snapshot.status {
                        GatewayConnectionStatus::Connected => "connected",
                        GatewayConnectionStatus::Disconnected => "disconnected",
                        GatewayConnectionStatus::Error => "error",
                    };
                    tracing::debug!(
                        platform = %snapshot.platform,
                        status = status_label,
                        last_success_at_ms = snapshot.last_success_at_ms,
                        last_error_at_ms = snapshot.last_error_at_ms,
                        consecutive_failure_count = snapshot.consecutive_failure_count,
                        last_error = snapshot.last_error.as_deref().unwrap_or(""),
                        current_backoff_secs = snapshot.current_backoff_secs,
                        "gateway health update received"
                    );
                    if let Err(error) = persist_gateway_health_update(&agent, snapshot).await {
                        framed
                            .send(DaemonMessage::Error {
                                message: format!("failed to persist gateway health: {error}"),
                            })
                            .await?;
                        continue;
                    }
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
                    let session_info = manager
                        .list()
                        .await
                        .into_iter()
                        .find(|session| session.id == id);
                    let recent_output = manager.get_analysis_text(id, Some(40)).await.ok();
                    match manager.kill(id).await {
                        Ok(()) => {
                            // Record session-end episode (EPIS-08)
                            let session_id_str = id.to_string();
                            let (session_summary, entities) = build_session_end_episode_payload(
                                &session_id_str,
                                session_info.as_ref(),
                                recent_output.as_deref(),
                            );
                            if let Err(e) = agent
                                .record_session_end_episode(
                                    &session_id_str,
                                    &session_summary,
                                    entities,
                                )
                                .await
                            {
                                tracing::warn!(session_id = %session_id_str, error = %e, "failed to record session end episode");
                            }
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
                    match manager
                        .search_history(&query, limit.unwrap_or(8).max(1))
                        .await
                    {
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
                } => match manager
                    .complete_command_log(&id, exit_code, duration_ms)
                    .await
                {
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
                } => match manager
                    .query_command_log(workspace_id.as_deref(), pane_id.as_deref(), limit)
                    .await
                {
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
                } => match agent.delete_thread_messages(&thread_id, &message_ids).await {
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
                },

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
                    match manager
                        .list_agent_events(category.as_deref(), pane_id.as_deref(), limit)
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
                    match manager
                        .generate_skill(query.as_deref(), title.as_deref())
                        .await
                    {
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

                ClientMessage::AgentDirectMessage {
                    target,
                    thread_id,
                    content,
                    session_id,
                } => {
                    agent.mark_operator_present("direct_message").await;
                    match agent
                        .send_direct_message(
                            &target,
                            thread_id.as_deref(),
                            session_id.as_deref(),
                            &content,
                        )
                        .await
                    {
                        Ok((thread_id, response)) => {
                            client_agent_threads.insert(thread_id.clone());
                            framed
                                .send(DaemonMessage::AgentDirectMessageResponse {
                                    target,
                                    thread_id,
                                    response,
                                    session_id,
                                })
                                .await?;
                        }
                        Err(error) => {
                            framed
                                .send(DaemonMessage::Error {
                                    message: error.to_string(),
                                })
                                .await?;
                        }
                    }
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
                    autonomy_level,
                } => {
                    let goal_run = agent
                        .start_goal_run(
                            goal,
                            title,
                            thread_id,
                            session_id,
                            priority.as_deref(),
                            client_request_id,
                            autonomy_level,
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
                    let checkpoints_json = match agent
                        .history
                        .list_checkpoints_for_goal_run(&goal_run_id)
                        .await
                    {
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
                    let entries_json =
                        match agent.health_log_entries(limit.unwrap_or(50).max(1)).await {
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

                ClientMessage::AgentStartOperatorProfileSession { kind } => {
                    match agent.start_operator_profile_session(&kind).await {
                        Ok(started) => {
                            framed
                                .send(DaemonMessage::AgentOperatorProfileSessionStarted {
                                    session_id: started.session_id.clone(),
                                    kind: started.kind.clone(),
                                })
                                .await
                                .ok();
                            match agent
                                .next_operator_profile_question_for_session(&started.session_id)
                                .await
                            {
                                Ok((question, progress)) => {
                                    if let Some(question) = question {
                                        framed
                                            .send(DaemonMessage::AgentOperatorProfileQuestion {
                                                session_id: question.session_id,
                                                question_id: question.question_id,
                                                field_key: question.field_key,
                                                prompt: question.prompt,
                                                input_kind: question.input_kind,
                                                optional: question.optional,
                                            })
                                            .await
                                            .ok();
                                        framed
                                            .send(DaemonMessage::AgentOperatorProfileProgress {
                                                session_id: progress.session_id,
                                                answered: progress.answered,
                                                remaining: progress.remaining,
                                                completion_ratio: progress.completion_ratio,
                                            })
                                            .await
                                            .ok();
                                    } else {
                                        match agent
                                            .complete_operator_profile_session(&started.session_id)
                                            .await
                                        {
                                            Ok(done) => {
                                                framed
                                                    .send(DaemonMessage::AgentOperatorProfileSessionCompleted {
                                                        session_id: done.session_id,
                                                        updated_fields: done.updated_fields,
                                                    })
                                                    .await
                                                    .ok();
                                            }
                                            Err(error) => {
                                                framed
                                                    .send(DaemonMessage::AgentError {
                                                        message: format!(
                                                            "failed to complete operator profile session: {error}"
                                                        ),
                                                    })
                                                    .await
                                                    .ok();
                                            }
                                        }
                                    }
                                }
                                Err(error) => {
                                    framed
                                        .send(DaemonMessage::AgentError {
                                            message: format!(
                                                "failed to fetch operator profile question: {error}"
                                            ),
                                        })
                                        .await
                                        .ok();
                                }
                            }
                        }
                        Err(error) => {
                            framed
                                .send(DaemonMessage::AgentError {
                                    message: format!(
                                        "failed to start operator profile session: {error}"
                                    ),
                                })
                                .await
                                .ok();
                        }
                    }
                }

                ClientMessage::AgentNextOperatorProfileQuestion { session_id } => {
                    match agent
                        .next_operator_profile_question_for_session(&session_id)
                        .await
                    {
                        Ok((question, progress)) => {
                            if let Some(question) = question {
                                framed
                                    .send(DaemonMessage::AgentOperatorProfileQuestion {
                                        session_id: question.session_id,
                                        question_id: question.question_id,
                                        field_key: question.field_key,
                                        prompt: question.prompt,
                                        input_kind: question.input_kind,
                                        optional: question.optional,
                                    })
                                    .await
                                    .ok();
                                framed
                                    .send(DaemonMessage::AgentOperatorProfileProgress {
                                        session_id: progress.session_id,
                                        answered: progress.answered,
                                        remaining: progress.remaining,
                                        completion_ratio: progress.completion_ratio,
                                    })
                                    .await
                                    .ok();
                            } else {
                                match agent.complete_operator_profile_session(&session_id).await {
                                    Ok(done) => {
                                        framed
                                            .send(DaemonMessage::AgentOperatorProfileSessionCompleted {
                                                session_id: done.session_id,
                                                updated_fields: done.updated_fields,
                                            })
                                            .await
                                            .ok();
                                    }
                                    Err(error) => {
                                        framed
                                            .send(DaemonMessage::AgentError {
                                                message: format!(
                                                    "failed to complete operator profile session: {error}"
                                                ),
                                            })
                                            .await
                                            .ok();
                                    }
                                }
                            }
                        }
                        Err(error) => {
                            framed
                                .send(DaemonMessage::AgentError {
                                    message: format!(
                                        "failed to fetch operator profile question: {error}"
                                    ),
                                })
                                .await
                                .ok();
                        }
                    }
                }

                ClientMessage::AgentSubmitOperatorProfileAnswer {
                    session_id,
                    question_id,
                    answer_json,
                } => {
                    match agent
                        .submit_operator_profile_answer(&session_id, &question_id, &answer_json)
                        .await
                    {
                        Ok((question, progress)) => {
                            if let Some(question) = question {
                                framed
                                    .send(DaemonMessage::AgentOperatorProfileQuestion {
                                        session_id: question.session_id,
                                        question_id: question.question_id,
                                        field_key: question.field_key,
                                        prompt: question.prompt,
                                        input_kind: question.input_kind,
                                        optional: question.optional,
                                    })
                                    .await
                                    .ok();
                                framed
                                    .send(DaemonMessage::AgentOperatorProfileProgress {
                                        session_id: progress.session_id,
                                        answered: progress.answered,
                                        remaining: progress.remaining,
                                        completion_ratio: progress.completion_ratio,
                                    })
                                    .await
                                    .ok();
                            } else {
                                match agent.complete_operator_profile_session(&session_id).await {
                                    Ok(done) => {
                                        framed
                                            .send(DaemonMessage::AgentOperatorProfileSessionCompleted {
                                                session_id: done.session_id,
                                                updated_fields: done.updated_fields,
                                            })
                                            .await
                                            .ok();
                                    }
                                    Err(error) => {
                                        framed
                                            .send(DaemonMessage::AgentError {
                                                message: format!(
                                                    "failed to complete operator profile session: {error}"
                                                ),
                                            })
                                            .await
                                            .ok();
                                    }
                                }
                            }
                        }
                        Err(error) => {
                            framed
                                .send(DaemonMessage::AgentError {
                                    message: format!(
                                        "failed to submit operator profile answer: {error}"
                                    ),
                                })
                                .await
                                .ok();
                        }
                    }
                }

                ClientMessage::AgentSkipOperatorProfileQuestion {
                    session_id,
                    question_id,
                    reason,
                } => {
                    match agent
                        .skip_operator_profile_question(
                            &session_id,
                            &question_id,
                            reason.as_deref(),
                        )
                        .await
                    {
                        Ok((question, progress)) => {
                            if let Some(question) = question {
                                framed
                                    .send(DaemonMessage::AgentOperatorProfileQuestion {
                                        session_id: question.session_id,
                                        question_id: question.question_id,
                                        field_key: question.field_key,
                                        prompt: question.prompt,
                                        input_kind: question.input_kind,
                                        optional: question.optional,
                                    })
                                    .await
                                    .ok();
                                framed
                                    .send(DaemonMessage::AgentOperatorProfileProgress {
                                        session_id: progress.session_id,
                                        answered: progress.answered,
                                        remaining: progress.remaining,
                                        completion_ratio: progress.completion_ratio,
                                    })
                                    .await
                                    .ok();
                            } else {
                                match agent.complete_operator_profile_session(&session_id).await {
                                    Ok(done) => {
                                        framed
                                            .send(DaemonMessage::AgentOperatorProfileSessionCompleted {
                                                session_id: done.session_id,
                                                updated_fields: done.updated_fields,
                                            })
                                            .await
                                            .ok();
                                    }
                                    Err(error) => {
                                        framed
                                            .send(DaemonMessage::AgentError {
                                                message: format!(
                                                    "failed to complete operator profile session: {error}"
                                                ),
                                            })
                                            .await
                                            .ok();
                                    }
                                }
                            }
                        }
                        Err(error) => {
                            framed
                                .send(DaemonMessage::AgentError {
                                    message: format!(
                                        "failed to skip operator profile question: {error}"
                                    ),
                                })
                                .await
                                .ok();
                        }
                    }
                }

                ClientMessage::AgentDeferOperatorProfileQuestion {
                    session_id,
                    question_id,
                    defer_until_unix_ms,
                } => {
                    match agent
                        .defer_operator_profile_question(
                            &session_id,
                            &question_id,
                            defer_until_unix_ms,
                        )
                        .await
                    {
                        Ok((question, progress)) => {
                            if let Some(question) = question {
                                framed
                                    .send(DaemonMessage::AgentOperatorProfileQuestion {
                                        session_id: question.session_id,
                                        question_id: question.question_id,
                                        field_key: question.field_key,
                                        prompt: question.prompt,
                                        input_kind: question.input_kind,
                                        optional: question.optional,
                                    })
                                    .await
                                    .ok();
                                framed
                                    .send(DaemonMessage::AgentOperatorProfileProgress {
                                        session_id: progress.session_id,
                                        answered: progress.answered,
                                        remaining: progress.remaining,
                                        completion_ratio: progress.completion_ratio,
                                    })
                                    .await
                                    .ok();
                            } else {
                                match agent.complete_operator_profile_session(&session_id).await {
                                    Ok(done) => {
                                        framed
                                            .send(DaemonMessage::AgentOperatorProfileSessionCompleted {
                                                session_id: done.session_id,
                                                updated_fields: done.updated_fields,
                                            })
                                            .await
                                            .ok();
                                    }
                                    Err(error) => {
                                        framed
                                            .send(DaemonMessage::AgentError {
                                                message: format!(
                                                    "failed to complete operator profile session: {error}"
                                                ),
                                            })
                                            .await
                                            .ok();
                                    }
                                }
                            }
                        }
                        Err(error) => {
                            framed
                                .send(DaemonMessage::AgentError {
                                    message: format!(
                                        "failed to defer operator profile question: {error}"
                                    ),
                                })
                                .await
                                .ok();
                        }
                    }
                }

                ClientMessage::AgentGetOperatorProfileSummary => {
                    match agent.get_operator_profile_summary_json().await {
                        Ok(summary_json) => {
                            framed
                                .send(DaemonMessage::AgentOperatorProfileSummary { summary_json })
                                .await
                                .ok();
                        }
                        Err(error) => {
                            framed
                                .send(DaemonMessage::AgentError {
                                    message: format!(
                                        "failed to build operator profile summary: {error}"
                                    ),
                                })
                                .await
                                .ok();
                        }
                    }
                }

                ClientMessage::AgentSetOperatorProfileConsent {
                    consent_key,
                    granted,
                } => {
                    match agent
                        .set_operator_profile_consent(&consent_key, granted)
                        .await
                    {
                        Ok(updated_fields) => {
                            framed
                                .send(DaemonMessage::AgentOperatorProfileSessionCompleted {
                                    session_id: "consent-update".to_string(),
                                    updated_fields,
                                })
                                .await
                                .ok();
                        }
                        Err(error) => {
                            framed
                                .send(DaemonMessage::AgentError {
                                    message: format!(
                                        "failed to set operator profile consent: {error}"
                                    ),
                                })
                                .await
                                .ok();
                        }
                    }
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
                } => match agent
                    .counterfactual_report(&option_type, &command_family, limit.unwrap_or(20))
                    .await
                {
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

                ClientMessage::AgentGetDivergentSession { session_id } => {
                    match agent.get_divergent_session(&session_id).await {
                        Ok(session_payload) => {
                            framed
                                .send(DaemonMessage::AgentDivergentSession {
                                    session_json: session_payload.to_string(),
                                })
                                .await
                                .ok();
                        }
                        Err(error) => {
                            framed
                                .send(DaemonMessage::AgentError {
                                    message: format!(
                                        "failed to read divergent session {session_id}: {error}"
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
                        let t = cfg
                            .tier
                            .user_self_assessment
                            .unwrap_or(crate::agent::capability_tier::CapabilityTier::Newcomer);
                        (done, t)
                    };
                    let mut onboarding_just_delivered = false;
                    if !onboarding_done {
                        if let Err(e) = agent
                            .concierge
                            .deliver_onboarding(tier, &agent.threads)
                            .await
                        {
                            tracing::warn!(
                                "onboarding delivery failed, falling back to generic welcome: {e}"
                            );
                        } else {
                            onboarding_just_delivered = true;
                            agent
                                .persist_thread_by_id(crate::agent::concierge::CONCIERGE_THREAD_ID)
                                .await;
                        }
                        // Mark onboarding as completed so it doesn't re-trigger on reconnect
                        {
                            let mut cfg = agent.config.write().await;
                            cfg.tier.onboarding_completed = true;
                        }
                    }

                    // Skip welcome generation if onboarding was just delivered —
                    // otherwise we'd emit two concierge messages back to back.
                    if onboarding_just_delivered {
                        continue;
                    }

                    // Generate welcome inline (awaits LLM call for non-Minimal levels).
                    let welcome = agent
                        .concierge
                        .generate_welcome(&agent.threads, &agent.tasks)
                        .await;
                    if let Some((content, detail_level, actions)) = welcome {
                        agent
                            .persist_thread_by_id(crate::agent::concierge::CONCIERGE_THREAD_ID)
                            .await;
                        let event = crate::agent::types::AgentEvent::ConciergeWelcome {
                            thread_id: crate::agent::concierge::CONCIERGE_THREAD_ID.to_string(),
                            content,
                            detail_level,
                            actions,
                        };
                        if let Some(fingerprint) = concierge_welcome_fingerprint(&event) {
                            if last_concierge_welcome_fingerprint.as_deref()
                                == Some(fingerprint.as_str())
                            {
                                tracing::info!(
                                    "server: suppressed duplicate concierge welcome for client"
                                );
                                continue;
                            }
                            last_concierge_welcome_fingerprint = Some(fingerprint);
                        }
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
                    agent
                        .persist_thread_by_id(crate::agent::concierge::CONCIERGE_THREAD_ID)
                        .await;
                    last_concierge_welcome_fingerprint = None;
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
                        raw_data_json: Some(
                            serde_json::json!({
                                "action": "cancel",
                                "thread_id": thread_id,
                                "outcome": "cancelled_by_user",
                            })
                            .to_string(),
                        ),
                    };

                    if let Err(e) = agent.history.insert_action_audit(&audit_entry).await {
                        tracing::warn!("failed to record escalation cancel audit: {e}");
                    }

                    // Broadcast EscalationUpdate event so all clients see the cancel.
                    let _ =
                        agent
                            .event_tx
                            .send(crate::agent::types::AgentEvent::EscalationUpdate {
                                thread_id: thread_id.clone(),
                                from_level: "unknown".to_string(),
                                to_level: "L0".to_string(),
                                reason: "User took over (I'll handle this)".to_string(),
                                attempts: 0,
                                audit_id: Some(audit_id.clone()),
                            });

                    // Broadcast AuditAction event.
                    let _ = agent
                        .event_tx
                        .send(crate::agent::types::AgentEvent::AuditAction {
                            id: audit_id,
                            timestamp: now_ms as u64,
                            action_type: "escalation".to_string(),
                            summary: summary.clone(),
                            explanation: Some(summary.clone()),
                            confidence: None,
                            confidence_band: None,
                            causal_trace_id: None,
                            thread_id: Some(thread_id.clone()),
                        });

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
                            match agent
                                .history
                                .list_skill_variants(Some(&identifier), 1)
                                .await
                            {
                                Ok(variants) => variants.into_iter().next(),
                                Err(_) => None,
                            }
                        }
                    };

                    let (public, content) = if let Some(ref v) = variant {
                        // Read SKILL.md content from disk
                        let skill_path = agent
                            .data_dir
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
                            match agent
                                .history
                                .list_skill_variants(Some(&identifier), 1)
                                .await
                            {
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
                            let skill_path = agent
                                .data_dir
                                .parent()
                                .unwrap_or(std::path::Path::new("."))
                                .join("skills")
                                .join(&v.relative_path);
                            let _ = tokio::fs::remove_file(&skill_path).await;

                            // Update status to archived
                            match agent
                                .history
                                .update_skill_variant_status(&v.variant_id, "archived")
                                .await
                            {
                                Ok(()) => DaemonMessage::SkillActionResult {
                                    success: true,
                                    message: format!(
                                        "Rejected and archived skill '{}'.",
                                        v.skill_name
                                    ),
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

                ClientMessage::SkillPromote {
                    identifier,
                    target_status,
                } => {
                    // Validate target status
                    let valid_statuses = [
                        "draft",
                        "testing",
                        "active",
                        "proven",
                        "promoted_to_canonical",
                    ];
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
                                match agent
                                    .history
                                    .list_skill_variants(Some(&identifier), 1)
                                    .await
                                {
                                    Ok(variants) => variants.into_iter().next(),
                                    Err(_) => None,
                                }
                            }
                        };

                        let msg = if let Some(v) = variant {
                            match agent
                                .history
                                .update_skill_variant_status(&v.variant_id, &target_status)
                                .await
                            {
                                Ok(()) => {
                                    // Record provenance
                                    agent
                                        .record_provenance_event(
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
                                            None,
                                            None,
                                            None,
                                            None,
                                            None,
                                        )
                                        .await;

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
                    let entries: Vec<amux_protocol::CommunitySkillEntry> =
                        match client.search(&query).await {
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
                                message: format!(
                                    "Imported community skill '{skill_name}' as draft."
                                ),
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
                        _ => match agent
                            .history
                            .list_skill_variants(Some(&identifier), 1)
                            .await
                        {
                            Ok(variants) => variants.into_iter().next(),
                            Err(_) => None,
                        },
                    };

                    let msg = if let Some(v) = variant {
                        let skill_path = agent
                            .history
                            .data_dir()
                            .join("skills")
                            .join(&v.relative_path);
                        match tokio::fs::read_to_string(&skill_path).await {
                            Ok(content) => match export_skill(
                                &content,
                                &format,
                                Path::new(&output_dir),
                                &v.skill_name,
                            ) {
                                Ok(path) => DaemonMessage::SkillExportResult {
                                    success: true,
                                    message: format!(
                                        "Exported skill '{}' to {}.",
                                        v.skill_name, path
                                    ),
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
                        _ => match agent
                            .history
                            .list_skill_variants(Some(&identifier), 1)
                            .await
                        {
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

                            let skill_dir = agent.history.data_dir().join("skills").join(
                                Path::new(&v.relative_path)
                                    .parent()
                                    .unwrap_or(Path::new(".")),
                            );
                            let machine_id = agent.history.data_dir().to_string_lossy().to_string();
                            match prepare_publish(&skill_dir, &v, &machine_id) {
                                Ok((tarball, metadata)) => {
                                    let client =
                                        RegistryClient::new(registry_url, agent.history.data_dir());
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
                ClientMessage::PluginGet { name } => match plugin_manager.get_plugin(&name).await {
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
                },
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
                ClientMessage::PluginInstall {
                    dir_name,
                    install_source,
                } => {
                    let result = plugin_manager
                        .register_plugin(&dir_name, &install_source)
                        .await;
                    let (success, message) = match result {
                        Ok(info) => (
                            true,
                            format!(
                                "Plugin '{}' v{} registered successfully",
                                info.name, info.version
                            ),
                        ),
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
                        Err(e) => (
                            false,
                            format!("Failed to unregister plugin '{}': {}", name, e),
                        ),
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
                            format!("Setting '{}' updated for plugin '{}'", key, plugin_name),
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

                // OAuth2 flow: start listener, return URL, await callback, exchange, store.
                ClientMessage::PluginOAuthStart { name } => {
                    tracing::info!(plugin = %name, "OAuth2 flow start requested");
                    match plugin_manager.start_oauth_flow_for_plugin(&name).await {
                        Ok(mut flow_state) => {
                            // Send the auth URL to the requesting client immediately
                            let auth_url = flow_state.auth_url.clone();
                            framed
                                .send(DaemonMessage::PluginOAuthUrl {
                                    name: name.clone(),
                                    url: auth_url,
                                })
                                .await?;

                            // Await callback and complete the flow (up to 5 min timeout)
                            match plugin_manager
                                .complete_oauth_flow(&name, &mut flow_state)
                                .await
                            {
                                Ok(()) => {
                                    framed
                                        .send(DaemonMessage::PluginOAuthComplete {
                                            name,
                                            success: true,
                                            error: None,
                                        })
                                        .await?;
                                }
                                Err(e) => {
                                    tracing::warn!(plugin = %name, error = %e, "OAuth2 flow failed");
                                    framed
                                        .send(DaemonMessage::PluginOAuthComplete {
                                            name,
                                            success: false,
                                            error: Some(e.to_string()),
                                        })
                                        .await?;
                                }
                            }
                        }
                        Err(e) => {
                            tracing::warn!(plugin = %name, error = %e, "OAuth2 flow start failed");
                            framed
                                .send(DaemonMessage::PluginOAuthComplete {
                                    name,
                                    success: false,
                                    error: Some(e.to_string()),
                                })
                                .await?;
                        }
                    }
                }

                // Plugin API proxy call: orchestrates full proxy flow through PluginManager.
                ClientMessage::PluginApiCall {
                    plugin_name,
                    endpoint_name,
                    params,
                } => {
                    let params_json: serde_json::Value = serde_json::from_str(&params)
                        .unwrap_or(serde_json::Value::Object(Default::default()));
                    match plugin_manager
                        .api_call(&plugin_name, &endpoint_name, params_json)
                        .await
                    {
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
                                crate::plugin::PluginApiError::TemplateError { .. } => {
                                    "template_error"
                                }
                                crate::plugin::PluginApiError::EndpointNotFound { .. } => {
                                    "endpoint_not_found"
                                }
                                crate::plugin::PluginApiError::PluginNotFound { .. } => {
                                    "plugin_not_found"
                                }
                                crate::plugin::PluginApiError::PluginDisabled { .. } => {
                                    "plugin_disabled"
                                }
                                crate::plugin::PluginApiError::AuthExpired { .. } => "auth_expired",
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
                ClientMessage::AgentExplainAction {
                    action_id,
                    step_index,
                } => {
                    let explanation = agent.handle_explain_action(&action_id, step_index).await;
                    let json = serde_json::to_string(&explanation).unwrap_or_default();
                    framed
                        .send(DaemonMessage::AgentExplanation {
                            explanation_json: json,
                        })
                        .await?;
                }
                ClientMessage::AgentStartDivergentSession {
                    problem_statement,
                    thread_id,
                    goal_run_id,
                    custom_framings_json,
                } => {
                    // Parse optional custom framings from JSON
                    let custom_framings = custom_framings_json
                        .as_deref()
                        .and_then(|json| serde_json::from_str::<Vec<serde_json::Value>>(json).ok())
                        .map(|items| {
                            items
                                .iter()
                                .filter_map(|item| {
                                    let label = item.get("label")?.as_str()?.to_string();
                                    let prompt =
                                        item.get("system_prompt_override")?.as_str()?.to_string();
                                    Some(crate::agent::handoff::divergent::Framing {
                                        label,
                                        system_prompt_override: prompt,
                                        task_id: None,
                                        contribution_id: None,
                                    })
                                })
                                .collect::<Vec<_>>()
                        })
                        .filter(|v| v.len() >= 2);

                    match agent
                        .start_divergent_session(
                            &problem_statement,
                            custom_framings,
                            &thread_id,
                            goal_run_id.as_deref(),
                        )
                        .await
                    {
                        Ok(session_id) => {
                            let result = serde_json::json!({
                                "session_id": session_id,
                                "status": "started",
                            });
                            framed
                                .send(DaemonMessage::AgentDivergentSessionStarted {
                                    session_json: serde_json::to_string(&result)
                                        .unwrap_or_default(),
                                })
                                .await?;
                        }
                        Err(e) => {
                            framed
                                .send(DaemonMessage::Error {
                                    message: format!("Failed to start divergent session: {e}"),
                                })
                                .await?;
                        }
                    }
                }
                ClientMessage::AgentWhatsAppLinkStart => {
                    tracing::info!("whatsapp link start requested by client");
                    match agent.whatsapp_link.start_if_idle().await {
                        Ok(_started) => {
                            #[cfg(not(test))]
                            {
                                if _started {
                                    if let Err(e) = start_whatsapp_link_backend(agent.clone()).await
                                    {
                                        agent
                                            .whatsapp_link
                                            .broadcast_error(e.to_string(), false)
                                            .await;
                                    }
                                }
                            }
                            let snapshot = agent.whatsapp_link.status_snapshot().await;
                            framed
                                .send(DaemonMessage::AgentWhatsAppLinkStatus {
                                    state: snapshot.state,
                                    phone: snapshot.phone,
                                    last_error: snapshot.last_error,
                                })
                                .await?;
                        }
                        Err(e) => {
                            framed
                                .send(DaemonMessage::AgentWhatsAppLinkError {
                                    message: e.to_string(),
                                    recoverable: false,
                                })
                                .await?;
                        }
                    }
                }
                ClientMessage::AgentWhatsAppLinkStop => {
                    match agent
                        .whatsapp_link
                        .stop(Some("operator_cancelled".to_string()))
                        .await
                    {
                        Ok(()) => {
                            let snapshot = agent.whatsapp_link.status_snapshot().await;
                            framed
                                .send(DaemonMessage::AgentWhatsAppLinkStatus {
                                    state: snapshot.state,
                                    phone: snapshot.phone,
                                    last_error: snapshot.last_error,
                                })
                                .await?;
                        }
                        Err(e) => {
                            framed
                                .send(DaemonMessage::AgentWhatsAppLinkError {
                                    message: e.to_string(),
                                    recoverable: false,
                                })
                                .await?;
                        }
                    }
                }
                ClientMessage::AgentWhatsAppLinkReset => {
                    tracing::info!("whatsapp link reset requested by client");
                    match agent.whatsapp_link.reset().await {
                        Ok(()) => {
                            if let Err(e) = crate::agent::clear_persisted_provider_state(
                                &agent.history,
                                crate::agent::WHATSAPP_LINK_PROVIDER_ID,
                            )
                            .await
                            {
                                framed
                                    .send(DaemonMessage::AgentWhatsAppLinkError {
                                        message: format!(
                                            "failed to clear whatsapp provider state: {e}"
                                        ),
                                        recoverable: false,
                                    })
                                    .await?;
                                continue;
                            }
                            let native_store_path =
                                crate::agent::whatsapp_native_store_path(&agent.data_dir);
                            if native_store_path.exists() {
                                tracing::info!(
                                    path = %native_store_path.display(),
                                    "whatsapp link reset removing native store"
                                );
                                if let Err(e) = tokio::fs::remove_file(&native_store_path).await {
                                    framed
                                        .send(DaemonMessage::AgentWhatsAppLinkError {
                                            message: format!(
                                                "failed to remove native whatsapp store {}: {e}",
                                                native_store_path.display()
                                            ),
                                            recoverable: false,
                                        })
                                        .await?;
                                    continue;
                                }
                            }
                            framed
                                .send(DaemonMessage::AgentWhatsAppLinkReset {
                                    ok: true,
                                    message: Some("reset".to_string()),
                                })
                                .await?;
                        }
                        Err(e) => {
                            framed
                                .send(DaemonMessage::AgentWhatsAppLinkError {
                                    message: e.to_string(),
                                    recoverable: false,
                                })
                                .await?;
                        }
                    }
                }
                ClientMessage::AgentWhatsAppLinkStatus => {
                    let snapshot = agent.whatsapp_link.status_snapshot().await;
                    framed
                        .send(DaemonMessage::AgentWhatsAppLinkStatus {
                            state: snapshot.state,
                            phone: snapshot.phone,
                            last_error: snapshot.last_error,
                        })
                        .await?;
                }
                ClientMessage::AgentWhatsAppLinkSubscribe => {
                    let (subscriber_id, rx) = agent.whatsapp_link.subscribe_with_id().await;
                    whatsapp_link_subscriber_guard.set(subscriber_id).await;
                    whatsapp_link_rx = Some(rx);
                    whatsapp_link_snapshot_replayed = false;
                    let snapshot = agent.whatsapp_link.status_snapshot().await;
                    framed
                        .send(DaemonMessage::AgentWhatsAppLinkStatus {
                            state: snapshot.state,
                            phone: snapshot.phone,
                            last_error: snapshot.last_error,
                        })
                        .await?;
                    whatsapp_link_snapshot_replayed = true;
                }
                ClientMessage::AgentWhatsAppLinkUnsubscribe => {
                    whatsapp_link_subscriber_guard.clear().await;
                    whatsapp_link_rx = None;
                    whatsapp_link_snapshot_replayed = false;
                }
            }
        }
    }
}
