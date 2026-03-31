use amux_protocol::{ClientMessage, DaemonMessage};
use anyhow::Result;

use super::connection::roundtrip;

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

    match roundtrip(ClientMessage::SkillImport {
        source: source.to_string(),
        force,
        publisher_verified,
    })
    .await?
    {
        DaemonMessage::SkillImportResult {
            success,
            message,
            variant_id,
            scan_verdict,
            findings_count,
        } => Ok((success, message, variant_id, scan_verdict, findings_count)),
        DaemonMessage::Error { message } => anyhow::bail!("daemon error: {message}"),
        other => anyhow::bail!("unexpected response: {other:?}"),
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
    match roundtrip(ClientMessage::SkillPublish {
        identifier: identifier.to_string(),
    })
    .await?
    {
        DaemonMessage::SkillPublishResult { success, message } => Ok((success, message)),
        DaemonMessage::Error { message } => anyhow::bail!("daemon error: {message}"),
        other => anyhow::bail!("unexpected response: {other:?}"),
    }
}
