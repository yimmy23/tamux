use anyhow::{Context, Result};
use futures::{SinkExt, StreamExt};
use zorai_protocol::{ClientMessage, DaemonMessage, SessionInfo};

use super::connection::{connect, roundtrip};

pub async fn ping() -> Result<()> {
    match roundtrip(ClientMessage::Ping).await? {
        DaemonMessage::Pong => Ok(()),
        other => anyhow::bail!("unexpected response: {other:?}"),
    }
}

pub async fn list_sessions() -> Result<Vec<SessionInfo>> {
    match roundtrip(ClientMessage::ListSessions).await? {
        DaemonMessage::SessionList { sessions } => Ok(sessions),
        DaemonMessage::Error { message } => anyhow::bail!("daemon error: {message}"),
        other => anyhow::bail!("unexpected response: {other:?}"),
    }
}

pub async fn spawn_session(
    shell: Option<String>,
    cwd: Option<String>,
    workspace_id: Option<String>,
) -> Result<String> {
    match roundtrip(ClientMessage::SpawnSession {
        shell,
        cwd,
        env: None,
        workspace_id,
        cols: 80,
        rows: 24,
    })
    .await?
    {
        DaemonMessage::SessionSpawned { id } => Ok(id.to_string()),
        DaemonMessage::Error { message } => anyhow::bail!("daemon error: {message}"),
        other => anyhow::bail!("unexpected response: {other:?}"),
    }
}

pub async fn clone_session(
    source_id: &str,
    workspace_id: Option<String>,
    cols: Option<u16>,
    rows: Option<u16>,
    cwd: Option<String>,
) -> Result<(String, Option<String>)> {
    let source_uuid = source_id.parse().context("invalid source session ID")?;
    match roundtrip(ClientMessage::CloneSession {
        source_id: source_uuid,
        workspace_id,
        cols,
        rows,
        replay_scrollback: false,
        cwd,
    })
    .await?
    {
        DaemonMessage::SessionCloned {
            id, active_command, ..
        } => Ok((id.to_string(), active_command)),
        DaemonMessage::Error { message } => anyhow::bail!("daemon error: {message}"),
        other => anyhow::bail!("unexpected response: {other:?}"),
    }
}

pub async fn kill_session(id: &str) -> Result<()> {
    let uuid = id.parse().context("invalid session ID")?;
    match roundtrip(ClientMessage::KillSession { id: uuid }).await? {
        DaemonMessage::SessionKilled { .. } => Ok(()),
        DaemonMessage::Error { message } => anyhow::bail!("daemon error: {message}"),
        other => anyhow::bail!("unexpected response: {other:?}"),
    }
}

pub async fn attach_session(id: &str) -> Result<()> {
    let uuid: uuid::Uuid = id.parse().context("invalid session ID")?;
    let mut framed = connect().await?;

    // Attach.
    framed
        .send(ClientMessage::AttachSession { id: uuid })
        .await?;

    // Stream output to stdout.
    while let Some(msg) = framed.next().await {
        match msg? {
            DaemonMessage::Output { data, .. } => {
                use std::io::Write;
                std::io::stdout().write_all(&data)?;
                std::io::stdout().flush()?;
            }
            DaemonMessage::SessionExited { exit_code, .. } => {
                println!(
                    "\r\nSession exited (code: {})",
                    exit_code.map_or("unknown".to_string(), |c| c.to_string())
                );
                break;
            }
            DaemonMessage::Error { message } => {
                eprintln!("Error: {message}");
                break;
            }
            DaemonMessage::GatewayBootstrap { .. }
            | DaemonMessage::GatewaySendRequest { .. }
            | DaemonMessage::GatewayReloadCommand { .. }
            | DaemonMessage::GatewayShutdownCommand { .. } => {}
            _ => {}
        }
    }

    Ok(())
}

pub async fn get_git_status(path: String) -> Result<zorai_protocol::GitInfo> {
    match roundtrip(ClientMessage::GetGitStatus { path }).await? {
        DaemonMessage::GitStatus { info, .. } => Ok(info),
        DaemonMessage::Error { message } => anyhow::bail!("daemon error: {message}"),
        other => anyhow::bail!("unexpected response: {other:?}"),
    }
}

pub async fn scrub_text(text: String) -> Result<String> {
    match roundtrip(ClientMessage::ScrubSensitive { text }).await? {
        DaemonMessage::ScrubResult { text } => Ok(text),
        DaemonMessage::Error { message } => anyhow::bail!("daemon error: {message}"),
        other => anyhow::bail!("unexpected response: {other:?}"),
    }
}
