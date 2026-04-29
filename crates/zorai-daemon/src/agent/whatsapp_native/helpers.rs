use std::collections::HashSet;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use wa_rs::wa_rs_proto::whatsapp as wa;
use zorai_protocol::parse_whatsapp_allowed_contacts;

use super::{
    clear_persisted_provider_state, AgentEngine, CONCIERGE_AGENT_NAME, MAIN_AGENT_NAME,
    WHATSAPP_LINK_PROVIDER_ID,
};

pub(crate) fn zorai_self_chat_prefix() -> String {
    format!(
        "🦅 {} - {}'s concierge ⚒️\n",
        CONCIERGE_AGENT_NAME, MAIN_AGENT_NAME
    )
}

pub(crate) fn whatsapp_native_store_path(base_dir: &Path) -> PathBuf {
    base_dir.join("whatsapp-link.sqlite")
}

pub(super) fn parse_zorai_device_version() -> wa::device_props::AppVersion {
    let mut parts = env!("CARGO_PKG_VERSION")
        .split('.')
        .map(|part| part.parse::<u32>().ok());
    wa::device_props::AppVersion {
        primary: parts.next().flatten(),
        secondary: parts.next().flatten(),
        tertiary: parts.next().flatten(),
        quaternary: parts.next().flatten(),
        quinary: parts.next().flatten(),
    }
}

pub(super) fn zorai_device_props() -> (
    Option<String>,
    Option<wa::device_props::AppVersion>,
    Option<wa::device_props::PlatformType>,
) {
    (
        Some("Zorai".to_string()),
        Some(parse_zorai_device_version()),
        Some(wa::device_props::PlatformType::Desktop),
    )
}

pub(super) fn format_outbound_whatsapp_text(text: &str, prefix_self_chat: bool) -> String {
    let prefix = zorai_self_chat_prefix();
    if !prefix_self_chat || text.starts_with(&prefix) {
        return text.to_string();
    }
    format!("{prefix}{text}")
}

pub(crate) fn build_whatsapp_cursor(ts_secs: u64, msg_id: &str) -> String {
    format!("{ts_secs}:{msg_id}")
}

pub(crate) fn parse_whatsapp_cursor(cursor: &str) -> Option<(u64, &str)> {
    let (ts_secs, msg_id) = cursor.split_once(':')?;
    let ts_secs = ts_secs.parse::<u64>().ok()?;
    let msg_id = msg_id.trim();
    if ts_secs == 0 || msg_id.is_empty() {
        return None;
    }
    Some((ts_secs, msg_id))
}

pub(super) fn mark_whatsapp_replay_active_if_cursors(gw: &mut super::gateway::GatewayState) {
    if !gw.whatsapp_replay_cursors.is_empty() {
        gw.replay_cycle_active.insert("whatsapp".to_string());
    }
}

#[cfg_attr(test, allow(dead_code))]
pub(super) fn is_whatsapp_cursor_newer(
    stored_cursor: &str,
    ts_secs: u64,
    msg_id: &str,
) -> Option<bool> {
    let msg_id = msg_id.trim();
    if ts_secs == 0 || msg_id.is_empty() {
        return None;
    }
    let (stored_ts_secs, stored_msg_id) = parse_whatsapp_cursor(stored_cursor)?;
    Some((ts_secs, msg_id) > (stored_ts_secs, stored_msg_id))
}

fn is_whatsapp_self_chat(
    chat: &str,
    sender: &str,
    own_identifiers: &HashSet<String>,
    exact_self_jids: &[String],
) -> bool {
    let chat_norm = super::normalize_identifier(chat);
    let sender_norm = super::normalize_identifier(sender);
    let chat_exact = chat.trim();
    let sender_exact = sender.trim();
    !chat_norm.is_empty()
        && own_identifiers.contains(&chat_norm)
        && !sender_norm.is_empty()
        && own_identifiers.contains(&sender_norm)
        && exact_self_jids
            .iter()
            .any(|jid| jid == chat_exact || jid == sender_exact)
}

pub(crate) fn should_enqueue_from_me_whatsapp_message(
    text: &str,
    chat: &str,
    sender: &str,
    own_identifiers: &HashSet<String>,
    exact_self_jids: &[String],
    known_outbound_echo: bool,
) -> bool {
    !known_outbound_echo
        && is_whatsapp_self_chat(chat, sender, own_identifiers, exact_self_jids)
        && !text.starts_with(&zorai_self_chat_prefix())
}

fn collect_whatsapp_allowlist_candidates(sender: &str, chat: &str) -> Vec<String> {
    let mut candidates = Vec::new();
    for value in [sender, chat] {
        let normalized = super::normalize_identifier(value);
        if !normalized.is_empty() && !candidates.iter().any(|candidate| candidate == &normalized) {
            candidates.push(normalized);
        }
    }
    candidates
}

fn whatsapp_sender_matches_allowlist(raw_allowlist: &str, sender: &str, chat: &str) -> bool {
    let allowlist = parse_whatsapp_allowed_contacts(raw_allowlist);
    if allowlist.is_empty() {
        return false;
    }

    let candidates = collect_whatsapp_allowlist_candidates(sender, chat);
    candidates
        .iter()
        .any(|candidate| allowlist.iter().any(|allowed| allowed == candidate))
}

#[cfg_attr(test, allow(dead_code))]
pub(super) fn log_whatsapp_allowlist_suppression(sender: &str, chat: &str) {
    tracing::info!(
        sender = %sender,
        chat = %chat,
        candidates = ?collect_whatsapp_allowlist_candidates(sender, chat),
        "whatsapp: suppressed sender outside allowlist"
    );
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WhatsAppEnqueueDecision {
    Enqueue,
    SuppressAllowlist,
    SuppressSelfEcho,
}

pub(crate) fn classify_whatsapp_enqueue_decision(
    text: &str,
    chat: &str,
    sender: &str,
    own_identifiers: &HashSet<String>,
    exact_self_jids: &[String],
    known_outbound_echo: bool,
    is_from_me: bool,
    raw_allowlist: &str,
) -> WhatsAppEnqueueDecision {
    if is_from_me
        && !should_enqueue_from_me_whatsapp_message(
            text,
            chat,
            sender,
            own_identifiers,
            exact_self_jids,
            known_outbound_echo,
        )
    {
        return WhatsAppEnqueueDecision::SuppressSelfEcho;
    }

    if !whatsapp_sender_matches_allowlist(raw_allowlist, sender, chat) {
        return WhatsAppEnqueueDecision::SuppressAllowlist;
    }

    WhatsAppEnqueueDecision::Enqueue
}

#[cfg_attr(test, allow(dead_code))]
pub(in crate::agent) fn now_millis_local() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

pub(in crate::agent) async fn clear_logged_out_whatsapp_session(agent: &AgentEngine) -> Result<()> {
    tracing::info!("whatsapp: clearing logged out native session");
    agent
        .whatsapp_link
        .stop(Some("logged_out".to_string()))
        .await
        .context("failed to stop logged out whatsapp runtime")?;
    clear_persisted_provider_state(&agent.history, WHATSAPP_LINK_PROVIDER_ID).await?;
    let store_path = whatsapp_native_store_path(&agent.data_dir);
    if store_path.exists() {
        tokio::fs::remove_file(&store_path)
            .await
            .with_context(|| format!("failed to remove {}", store_path.display()))?;
    }
    tracing::info!(path=%store_path.display(), "whatsapp: cleared logged out native session store");
    Ok(())
}
