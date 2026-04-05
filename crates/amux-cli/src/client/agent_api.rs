use amux_protocol::{ClientMessage, DaemonMessage};
use anyhow::Result;

use super::connection::roundtrip;

/// Status response fields from the daemon.
pub struct AgentStatusSnapshot {
    pub tier: String,
    pub activity: String,
    pub active_thread_id: Option<String>,
    pub active_goal_run_id: Option<String>,
    pub active_goal_run_title: Option<String>,
    pub provider_health_json: String,
    pub gateway_statuses_json: String,
    pub recent_actions_json: String,
}

pub struct DirectMessageResponse {
    pub target: String,
    pub thread_id: String,
    pub response: String,
    pub session_id: Option<String>,
    pub provider_final_result_json: Option<String>,
}

pub async fn send_direct_message(
    target: &str,
    thread_id: Option<String>,
    content: String,
    session_id: Option<String>,
) -> Result<DirectMessageResponse> {
    match roundtrip(ClientMessage::AgentDirectMessage {
        target: target.to_string(),
        thread_id,
        content,
        session_id,
    })
    .await?
    {
        DaemonMessage::AgentDirectMessageResponse {
            target,
            thread_id,
            response,
            session_id,
            provider_final_result_json,
        } => Ok(DirectMessageResponse {
            target,
            thread_id,
            response,
            session_id,
            provider_final_result_json,
        }),
        DaemonMessage::Error { message } => anyhow::bail!("daemon error: {message}"),
        other => anyhow::bail!("unexpected response: {other:?}"),
    }
}

pub async fn send_status_query() -> Result<AgentStatusSnapshot> {
    match roundtrip(ClientMessage::AgentStatusQuery).await? {
        DaemonMessage::AgentStatusResponse {
            tier,
            activity,
            active_thread_id,
            active_goal_run_id,
            active_goal_run_title,
            provider_health_json,
            gateway_statuses_json,
            recent_actions_json,
            ..
        } => Ok(AgentStatusSnapshot {
            tier,
            activity,
            active_thread_id,
            active_goal_run_id,
            active_goal_run_title,
            provider_health_json,
            gateway_statuses_json,
            recent_actions_json,
        }),
        DaemonMessage::Error { message } => anyhow::bail!("daemon error: {message}"),
        other => anyhow::bail!("unexpected response: {other:?}"),
    }
}

pub async fn send_config_get() -> Result<serde_json::Value> {
    match roundtrip(ClientMessage::AgentGetConfig).await? {
        DaemonMessage::AgentConfigResponse { config_json } => {
            let config: serde_json::Value = serde_json::from_str(&config_json)?;
            Ok(config)
        }
        DaemonMessage::Error { message } => anyhow::bail!("daemon error: {message}"),
        other => anyhow::bail!("unexpected response: {other:?}"),
    }
}

pub async fn send_config_set(key_path: String, value_json: String) -> Result<()> {
    parse_config_set_response(
        roundtrip(ClientMessage::AgentSetConfigItem {
            key_path,
            value_json,
        })
        .await?,
    )
}

pub(crate) fn parse_config_set_response(msg: DaemonMessage) -> Result<()> {
    match msg {
        DaemonMessage::OperationAccepted { .. } | DaemonMessage::AgentConfigResponse { .. } => {
            Ok(())
        }
        DaemonMessage::Error { message } | DaemonMessage::AgentError { message } => {
            anyhow::bail!("daemon error: {message}")
        }
        other => anyhow::bail!("unexpected response: {other:?}"),
    }
}

pub async fn send_audit_query(
    action_types: Option<Vec<String>>,
    since: Option<u64>,
    limit: Option<usize>,
) -> Result<Vec<amux_protocol::AuditEntryPublic>> {
    match roundtrip(ClientMessage::AuditQuery {
        action_types,
        since,
        limit,
    })
    .await?
    {
        DaemonMessage::AuditList { entries_json } => {
            let entries: Vec<amux_protocol::AuditEntryPublic> =
                serde_json::from_str(&entries_json)?;
            Ok(entries)
        }
        DaemonMessage::Error { message } => anyhow::bail!("daemon error: {message}"),
        other => anyhow::bail!("unexpected response: {other:?}"),
    }
}
