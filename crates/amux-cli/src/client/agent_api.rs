use amux_protocol::{ClientMessage, DaemonMessage, OperationStatusSnapshot};
use anyhow::Result;
use futures::StreamExt;
use serde::{Deserialize, Serialize};

use super::connection::{connect, roundtrip};
use futures::SinkExt;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AgentThreadMessageRecord {
    pub id: String,
    pub role: String,
    pub content: String,
    pub timestamp: u64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AgentThreadRecord {
    pub id: String,
    pub agent_name: Option<String>,
    pub title: String,
    pub messages: Vec<AgentThreadMessageRecord>,
    pub pinned: bool,
    pub created_at: u64,
    pub updated_at: u64,
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
}

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

pub struct DeleteThreadResponse {
    pub thread_id: String,
    pub deleted: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AgentPromptInspectionSection {
    pub id: String,
    pub title: String,
    pub content: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AgentPromptInspection {
    pub agent_id: String,
    pub agent_name: String,
    pub provider_id: String,
    pub model: String,
    pub sections: Vec<AgentPromptInspectionSection>,
    pub final_prompt: String,
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

pub async fn send_thread_list_query() -> Result<Vec<AgentThreadRecord>> {
    match roundtrip(ClientMessage::AgentListThreads).await? {
        DaemonMessage::AgentThreadList { threads_json } => Ok(serde_json::from_str(&threads_json)?),
        DaemonMessage::Error { message } => anyhow::bail!("daemon error: {message}"),
        other => anyhow::bail!("unexpected response: {other:?}"),
    }
}

pub async fn send_thread_get_query(thread_id: String) -> Result<Option<AgentThreadRecord>> {
    let mut framed = connect().await?;
    let requested_thread_id = thread_id.clone();
    framed
        .send(ClientMessage::AgentGetThread {
            thread_id,
            message_limit: None,
            message_offset: None,
        })
        .await?;

    let mut thread_detail_bytes = Vec::new();
    loop {
        let response = framed
            .next()
            .await
            .ok_or_else(super::connection::closed_connection_error)??;
        match response {
            DaemonMessage::AgentThreadDetail { thread_json } => {
                return Ok(serde_json::from_str(&thread_json)?);
            }
            DaemonMessage::AgentThreadDetailChunk {
                thread_id,
                thread_json_chunk,
                done,
            } => {
                if thread_id != requested_thread_id {
                    thread_detail_bytes.clear();
                    anyhow::bail!(
                        "received chunk for unexpected thread: expected {}, got {}",
                        requested_thread_id,
                        thread_id
                    );
                }
                thread_detail_bytes.extend(thread_json_chunk);
                if done {
                    let thread_json = String::from_utf8(thread_detail_bytes)?;
                    return Ok(serde_json::from_str(&thread_json)?);
                }
            }
            DaemonMessage::Error { message } => anyhow::bail!("daemon error: {message}"),
            other => anyhow::bail!("unexpected response: {other:?}"),
        }
    }
}

pub async fn send_thread_delete(thread_id: String) -> Result<DeleteThreadResponse> {
    let mut framed = connect().await?;
    framed
        .send(ClientMessage::AgentDeleteThread {
            thread_id: thread_id.clone(),
        })
        .await?;
    let resp = futures::StreamExt::next(&mut framed)
        .await
        .ok_or_else(super::connection::closed_connection_error)??;
    match resp {
        DaemonMessage::AgentThreadDeleted { thread_id, deleted } => {
            Ok(DeleteThreadResponse { thread_id, deleted })
        }
        DaemonMessage::Error { message } => anyhow::bail!("daemon error: {message}"),
        other => anyhow::bail!("unexpected response: {other:?}"),
    }
}

pub async fn send_prompt_query(agent_id: Option<String>) -> Result<AgentPromptInspection> {
    match roundtrip(ClientMessage::AgentInspectPrompt { agent_id }).await? {
        DaemonMessage::AgentPromptInspection { prompt_json } => {
            Ok(serde_json::from_str(&prompt_json)?)
        }
        DaemonMessage::Error { message } => anyhow::bail!("daemon error: {message}"),
        other => anyhow::bail!("unexpected response: {other:?}"),
    }
}

pub async fn send_operation_status_query(operation_id: String) -> Result<OperationStatusSnapshot> {
    parse_operation_status_response(
        roundtrip(ClientMessage::AgentGetOperationStatus { operation_id }).await?,
    )
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

pub(crate) fn parse_operation_status_response(
    msg: DaemonMessage,
) -> Result<OperationStatusSnapshot> {
    match msg {
        DaemonMessage::OperationStatus { snapshot } => Ok(snapshot),
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
