use amux_protocol::{ClientMessage, DaemonMessage, SessionId, SkillDiscoveryResultPublic};
use anyhow::{Context, Result};

use super::connection::{roundtrip, roundtrip_async_until, roundtrip_until};

pub async fn send_skill_list(
    status: Option<String>,
    limit: usize,
) -> Result<Vec<amux_protocol::SkillVariantPublic>> {
    match roundtrip(ClientMessage::SkillList { status, limit }).await? {
        DaemonMessage::SkillListResult { variants } => Ok(variants),
        DaemonMessage::Error { message } => anyhow::bail!("daemon error: {message}"),
        other => anyhow::bail!("unexpected response: {other:?}"),
    }
}

pub async fn send_skill_inspect(
    identifier: &str,
) -> Result<(Option<amux_protocol::SkillVariantPublic>, Option<String>)> {
    match roundtrip(ClientMessage::SkillInspect {
        identifier: identifier.to_string(),
    })
    .await?
    {
        DaemonMessage::SkillInspectResult { variant, content } => Ok((variant, content)),
        DaemonMessage::Error { message } => anyhow::bail!("daemon error: {message}"),
        other => anyhow::bail!("unexpected response: {other:?}"),
    }
}

pub async fn send_skill_discover(
    query: &str,
    session_id: Option<SessionId>,
    limit: usize,
) -> Result<SkillDiscoveryResultPublic> {
    roundtrip_until(
        ClientMessage::SkillDiscover {
            query: query.to_string(),
            session_id,
            limit,
        },
        parse_skill_discover_terminal_response,
    )
    .await
}

pub(super) fn parse_skill_discover_terminal_response(
    msg: DaemonMessage,
) -> Option<Result<SkillDiscoveryResultPublic>> {
    match msg {
        DaemonMessage::CwdChanged { .. }
        | DaemonMessage::Output { .. }
        | DaemonMessage::CommandStarted { .. }
        | DaemonMessage::CommandFinished { .. } => None,
        DaemonMessage::SkillDiscoverResult { result_json } => Some(
            serde_json::from_str(&result_json)
                .context("invalid skill discovery payload from daemon"),
        ),
        DaemonMessage::AgentError { message } | DaemonMessage::Error { message } => {
            Some(Err(anyhow::anyhow!("daemon error: {message}")))
        }
        other => Some(Err(anyhow::anyhow!("unexpected response: {other:?}"))),
    }
}

pub async fn send_skill_reject(identifier: &str) -> Result<(bool, String)> {
    match roundtrip(ClientMessage::SkillReject {
        identifier: identifier.to_string(),
    })
    .await?
    {
        DaemonMessage::SkillActionResult { success, message } => Ok((success, message)),
        DaemonMessage::Error { message } => anyhow::bail!("daemon error: {message}"),
        other => anyhow::bail!("unexpected response: {other:?}"),
    }
}

pub async fn send_skill_promote(identifier: &str, target_status: &str) -> Result<(bool, String)> {
    match roundtrip(ClientMessage::SkillPromote {
        identifier: identifier.to_string(),
        target_status: target_status.to_string(),
    })
    .await?
    {
        DaemonMessage::SkillActionResult { success, message } => Ok((success, message)),
        DaemonMessage::Error { message } => anyhow::bail!("daemon error: {message}"),
        other => anyhow::bail!("unexpected response: {other:?}"),
    }
}

pub async fn send_skill_search(query: &str) -> Result<Vec<amux_protocol::CommunitySkillEntry>> {
    match roundtrip(ClientMessage::SkillSearch {
        query: query.to_string(),
    })
    .await?
    {
        DaemonMessage::SkillSearchResult { entries } => Ok(entries),
        DaemonMessage::Error { message } => anyhow::bail!("daemon error: {message}"),
        other => anyhow::bail!("unexpected response: {other:?}"),
    }
}

pub async fn send_skill_import(
    source: &str,
    force: bool,
) -> Result<(bool, String, Option<String>, Option<String>, u32)> {
    let publisher_verified = if source.starts_with("http://") || source.starts_with("https://") {
        false
    } else {
        send_skill_search(source)
            .await?
            .into_iter()
            .find(|entry| entry.name.eq_ignore_ascii_case(source))
            .map(|entry| entry.publisher_verified)
            .unwrap_or(false)
    };

    roundtrip_async_until(
        ClientMessage::SkillImport {
            source: source.to_string(),
            force,
            publisher_verified,
        },
        parse_skill_import_terminal_response,
    )
    .await
}

pub(super) fn parse_skill_import_terminal_response(
    msg: DaemonMessage,
) -> Option<Result<(bool, String, Option<String>, Option<String>, u32)>> {
    match msg {
        DaemonMessage::OperationAccepted { .. } => None,
        DaemonMessage::SkillImportResult {
            operation_id: _,
            success,
            message,
            variant_id,
            scan_verdict,
            findings_count,
        } => Some(Ok((
            success,
            message,
            variant_id,
            scan_verdict,
            findings_count,
        ))),
        DaemonMessage::Error { message } => Some(Err(anyhow::anyhow!("daemon error: {message}"))),
        other => Some(Err(anyhow::anyhow!("unexpected response: {other:?}"))),
    }
}

pub async fn send_skill_export(
    identifier: &str,
    format: &str,
    output_dir: &str,
) -> Result<(bool, String, Option<String>)> {
    match roundtrip(ClientMessage::SkillExport {
        identifier: identifier.to_string(),
        format: format.to_string(),
        output_dir: output_dir.to_string(),
    })
    .await?
    {
        DaemonMessage::SkillExportResult {
            success,
            message,
            output_path,
        } => Ok((success, message, output_path)),
        DaemonMessage::Error { message } => anyhow::bail!("daemon error: {message}"),
        other => anyhow::bail!("unexpected response: {other:?}"),
    }
}

pub async fn send_skill_publish(identifier: &str) -> Result<(bool, String)> {
    roundtrip_async_until(
        ClientMessage::SkillPublish {
            identifier: identifier.to_string(),
        },
        parse_skill_publish_terminal_response,
    )
    .await
}

pub(super) fn parse_skill_publish_terminal_response(
    msg: DaemonMessage,
) -> Option<Result<(bool, String)>> {
    match msg {
        DaemonMessage::OperationAccepted { .. } => None,
        DaemonMessage::SkillPublishResult {
            operation_id: _,
            success,
            message,
        } => Some(Ok((success, message))),
        DaemonMessage::Error { message } => Some(Err(anyhow::anyhow!("daemon error: {message}"))),
        other => Some(Err(anyhow::anyhow!("unexpected response: {other:?}"))),
    }
}
