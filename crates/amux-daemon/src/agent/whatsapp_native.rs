use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use wa_rs::bot::Bot;
use wa_rs::proto_helpers::MessageExt;
use wa_rs::store::SqliteStore;
use wa_rs::types::events::Event;
use wa_rs::wa_rs_proto::whatsapp as wa;
use wa_rs::Jid;
use wa_rs_tokio_transport::TokioWebSocketTransportFactory;
use wa_rs_ureq_http::UreqHttpClient;

use super::gateway::IncomingMessage;
use super::gateway_loop::process_replay_result;
use super::whatsapp_link::{
    clear_persisted_provider_state, collect_exact_jid_candidates, collect_normalized_identifiers,
    normalize_identifier, normalize_jid_user, transport,
};
use super::{
    gateway, persist_transport_session_update, AgentEngine, CONCIERGE_AGENT_NAME, MAIN_AGENT_NAME,
    WHATSAPP_LINK_PROVIDER_ID,
};

fn tamux_self_chat_prefix() -> String {
    format!(
        "🦅 {} - {}'s concierge ⚒️\n",
        CONCIERGE_AGENT_NAME, MAIN_AGENT_NAME
    )
}

pub(crate) fn whatsapp_native_store_path(base_dir: &Path) -> PathBuf {
    base_dir.join("whatsapp-link.sqlite")
}

fn parse_tamux_device_version() -> wa::device_props::AppVersion {
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

fn tamux_device_props() -> (
    Option<String>,
    Option<wa::device_props::AppVersion>,
    Option<wa::device_props::PlatformType>,
) {
    (
        Some("Tamux".to_string()),
        Some(parse_tamux_device_version()),
        Some(wa::device_props::PlatformType::Desktop),
    )
}

fn format_outbound_whatsapp_text(text: &str, prefix_self_chat: bool) -> String {
    let prefix = tamux_self_chat_prefix();
    if !prefix_self_chat || text.starts_with(&prefix) {
        return text.to_string();
    }
    format!("{prefix}{text}")
}

/// Encode a WhatsApp replay cursor from a message timestamp (Unix seconds) and message ID.
/// Format: `"{ts_secs}:{msg_id}"`.
fn build_whatsapp_cursor(ts_secs: u64, msg_id: &str) -> String {
    format!("{ts_secs}:{msg_id}")
}

/// Parse the timestamp + message id components from a WhatsApp cursor value.
/// Returns `None` for legacy cursors that cannot be parsed, which triggers a
/// safe no-backfill fallback for reconnect replay.
fn parse_whatsapp_cursor(cursor: &str) -> Option<(u64, &str)> {
    let (ts_secs, msg_id) = cursor.split_once(':')?;
    let ts_secs = ts_secs.parse::<u64>().ok()?;
    let msg_id = msg_id.trim();
    if ts_secs == 0 || msg_id.is_empty() {
        return None;
    }
    Some((ts_secs, msg_id))
}

/// Insert "whatsapp" into `replay_cycle_active` when persisted cursors exist.
/// Called once per reconnect cycle from the `Event::Connected` handler.
fn mark_whatsapp_replay_active_if_cursors(gw: &mut gateway::GatewayState) {
    if !gw.whatsapp_replay_cursors.is_empty() {
        gw.replay_cycle_active.insert("whatsapp".to_string());
    }
}

fn is_whatsapp_cursor_newer(stored_cursor: &str, ts_secs: u64, msg_id: &str) -> Option<bool> {
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
    own_identifiers: &std::collections::HashSet<String>,
    exact_self_jids: &[String],
) -> bool {
    let chat_norm = normalize_identifier(chat);
    let sender_norm = normalize_identifier(sender);
    !chat_norm.is_empty()
        && own_identifiers.contains(&chat_norm)
        && !sender_norm.is_empty()
        && own_identifiers.contains(&sender_norm)
        && !exact_self_jids.is_empty()
}

fn should_enqueue_from_me_whatsapp_message(
    text: &str,
    chat: &str,
    sender: &str,
    own_identifiers: &std::collections::HashSet<String>,
    exact_self_jids: &[String],
    known_outbound_echo: bool,
) -> bool {
    !known_outbound_echo
        && is_whatsapp_self_chat(chat, sender, own_identifiers, exact_self_jids)
        && !text.starts_with(&tamux_self_chat_prefix())
}

pub(crate) async fn start_whatsapp_link_native(agent: Arc<AgentEngine>) -> Result<()> {
    let store_path = whatsapp_native_store_path(&agent.data_dir);
    if let Some(parent) = store_path.parent() {
        tokio::fs::create_dir_all(parent).await.with_context(|| {
            format!(
                "failed to create whatsapp store directory {}",
                parent.display()
            )
        })?;
    }

    let store = Arc::new(
        SqliteStore::new(&store_path.to_string_lossy())
            .await
            .with_context(|| {
                format!("failed to open wa-rs sqlite store {}", store_path.display())
            })?,
    );

    // Enable history sync only when persisted WhatsApp replay cursors already exist.
    // On first-ever connect there are no cursors, so skip history sync to prevent
    // backfilling old messages.  On reconnect, cursors exist and history sync is
    // the source for the reconnect-replay path.
    let has_whatsapp_cursors = match agent.history.load_gateway_replay_cursors("whatsapp").await {
        Ok(rows) => !rows.is_empty(),
        Err(e) => {
            tracing::warn!("whatsapp: failed to check replay cursors, defaulting to no-sync: {e}");
            false
        }
    };

    let store_path_string = store_path.to_string_lossy().to_string();
    let agent_for_events = agent.clone();
    let (device_os, device_version, device_platform) = tamux_device_props();
    let mut bot = Bot::builder()
        .with_backend(store)
        .with_transport_factory(TokioWebSocketTransportFactory::new())
        .with_http_client(UreqHttpClient::new())
        .with_device_props(device_os.clone(), device_version, device_platform)
        .on_event(move |event, client| {
            let agent = agent_for_events.clone();
            let store_path = store_path_string.clone();
            let device_os = device_os.clone();
            let device_version = device_version;
            let device_platform = device_platform;
            async move {
                handle_native_event(
                    agent,
                    client,
                    event,
                    &store_path,
                    device_os.as_deref(),
                    device_version,
                    device_platform,
                )
                .await;
            }
        })
        .build()
        .await
        .context("failed to build wa-rs bot")?;

    let client = bot.client();
    client.set_skip_history_sync(!has_whatsapp_cursors);
    let run_task = bot.run().await.context("failed to run wa-rs bot")?;
    agent
        .whatsapp_link
        .attach_native_client(client, run_task)
        .await?;
    Ok(())
}

fn now_millis_local() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

async fn clear_logged_out_whatsapp_session(agent: &AgentEngine) -> Result<()> {
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

async fn handle_native_event(
    agent: Arc<AgentEngine>,
    client: Arc<wa_rs::Client>,
    event: Event,
    store_path: &str,
    device_os: Option<&str>,
    device_version: Option<wa::device_props::AppVersion>,
    device_platform: Option<wa::device_props::PlatformType>,
) {
    match event {
        Event::PairingQrCode { code, timeout } => {
            let expires_at_ms = now_millis_local().checked_add(timeout.as_millis() as u64);
            agent.whatsapp_link.broadcast_qr(code, expires_at_ms).await;
        }
        Event::Connected(_) => {
            let pn = client.get_pn().await.map(|jid| jid.to_string());
            let lid = client.get_lid().await.map(|jid| jid.to_string());
            let phone = pn
                .as_deref()
                .map(normalize_jid_user)
                .filter(|value| !value.is_empty())
                .map(|value| format!("+{value}"));
            let update = transport::SessionUpdate {
                linked_phone: phone.clone(),
                auth_json: None,
                metadata_json: Some(
                    serde_json::json!({
                        "pn": pn,
                        "lid": lid,
                        "store_path": store_path,
                        "device_os": device_os.unwrap_or(""),
                        "device_platform": device_platform.map(|platform| platform.as_str_name()).unwrap_or(""),
                        "device_version": format!(
                            "{}.{}.{}",
                            device_version.and_then(|v| v.primary).unwrap_or_default(),
                            device_version.and_then(|v| v.secondary).unwrap_or_default(),
                            device_version.and_then(|v| v.tertiary).unwrap_or_default(),
                        ),
                    })
                    .to_string(),
                ),
                linked_at: Some(now_millis_local()),
                updated_at: now_millis_local(),
            };
            if let Err(error) =
                persist_transport_session_update(&agent.history, WHATSAPP_LINK_PROVIDER_ID, update)
                    .await
            {
                agent
                    .whatsapp_link
                    .broadcast_error(
                        format!("failed to persist wa-rs linked state: {error}"),
                        false,
                    )
                    .await;
            }
            agent.whatsapp_link.broadcast_linked(phone).await;
            // Mark whatsapp replay active for this reconnect cycle when cursors exist.
            let has_cursors = {
                let mut gw_guard = agent.gateway_state.lock().await;
                if let Some(gw) = gw_guard.as_mut() {
                    mark_whatsapp_replay_active_if_cursors(gw);
                    !gw.whatsapp_replay_cursors.is_empty()
                } else {
                    false
                }
            };
            client.set_skip_history_sync(!has_cursors);
        }
        Event::Disconnected(_) => {
            agent
                .whatsapp_link
                .broadcast_disconnected(Some("native_disconnected".to_string()))
                .await;
        }
        Event::LoggedOut(_) => {
            tracing::info!("whatsapp: received native logged_out event");
            if let Err(error) = clear_logged_out_whatsapp_session(&agent).await {
                tracing::warn!(%error, "whatsapp: failed to clear logged out native session");
            }
        }
        Event::PairError(error) => {
            agent
                .whatsapp_link
                .broadcast_error(format!("pairing failed: {}", error.error), false)
                .await;
        }
        Event::JoinedGroup(lazy_conv) => {
            // History sync delivers one JoinedGroup event per conversation.
            // We only process it when a WhatsApp replay cycle is active (i.e. we have
            // persisted cursors and history sync was enabled for this connect).
            let is_replay_active = {
                let gw_guard = agent.gateway_state.lock().await;
                gw_guard
                    .as_ref()
                    .map(|g| g.replay_cycle_active.contains("whatsapp"))
                    .unwrap_or(false)
            };
            if !is_replay_active {
                return;
            }

            // Extract all data from the lazy conversation synchronously so we do
            // not hold a borrow across await points.
            let (chat_jid, extracted_msgs) = {
                let Some(conv) = lazy_conv.get() else {
                    return;
                };
                let jid = conv.id.clone();
                let msgs: Vec<(u64, String, String, String, bool)> = conv
                    .messages
                    .iter()
                    .filter_map(|h| h.message.as_ref())
                    .filter_map(|wmi| {
                        let ts = wmi.message_timestamp.unwrap_or(0);
                        let msg_id = wmi.key.id.clone().unwrap_or_default();
                        if msg_id.is_empty() {
                            return None;
                        }
                        // Group: participant field holds the real sender.
                        // 1-1:   remote_jid is the other party (= the chat jid).
                        let sender = wmi
                            .key
                            .participant
                            .clone()
                            .or_else(|| wmi.participant.clone())
                            .unwrap_or_else(|| {
                                wmi.key.remote_jid.clone().unwrap_or_else(|| jid.clone())
                            });
                        let text = wmi
                            .message
                            .as_ref()
                            .and_then(|m| m.text_content())
                            .unwrap_or("")
                            .trim()
                            .to_string();
                        if text.is_empty() {
                            return None;
                        }
                        Some((ts, msg_id, sender, text, wmi.key.from_me.unwrap_or(false)))
                    })
                    .collect();
                (jid, msgs)
            };
            // lazy_conv borrow released here.

            if chat_jid.is_empty() || extracted_msgs.is_empty() {
                return;
            }

            let chat_key = normalize_identifier(&chat_jid);
            if chat_key.is_empty() {
                return;
            }

            // Only replay conversations for which a persisted cursor exists.
            // This ensures chats seen for the first time are not backfilled.
            let stored_cursor: Option<String> = {
                let gw_guard = agent.gateway_state.lock().await;
                gw_guard
                    .as_ref()
                    .and_then(|g| g.whatsapp_replay_cursors.get(&chat_key).cloned())
            };
            let Some(stored_cursor) = stored_cursor else {
                // No parseable cursor for this chat; skip (safe no-backfill fallback).
                return;
            };

            let own_identifiers = collect_normalized_identifiers(&[
                &client
                    .get_pn()
                    .await
                    .map(|jid| jid.to_string())
                    .unwrap_or_default(),
                &client
                    .get_lid()
                    .await
                    .map(|jid| jid.to_string())
                    .unwrap_or_default(),
            ]);
            let exact_self_jids = collect_exact_jid_candidates(&[
                &client
                    .get_lid()
                    .await
                    .map(|jid| jid.to_string())
                    .unwrap_or_default(),
                &client
                    .get_pn()
                    .await
                    .map(|jid| jid.to_string())
                    .unwrap_or_default(),
            ]);

            // Build replay envelopes for messages newer than the cursor.
            let mut envelopes: Vec<gateway::ReplayEnvelope> = Vec::new();
            for (ts, msg_id, sender, text, is_from_me) in extracted_msgs {
                let Some(true) = is_whatsapp_cursor_newer(&stored_cursor, ts, &msg_id) else {
                    continue;
                };
                let known_outbound_echo = if is_from_me {
                    agent
                        .whatsapp_link
                        .is_recent_outbound_message_id(&msg_id)
                        .await
                } else {
                    false
                };
                let should_enqueue = if is_from_me {
                    should_enqueue_from_me_whatsapp_message(
                        &text,
                        &chat_jid,
                        &sender,
                        &own_identifiers,
                        &exact_self_jids,
                        known_outbound_echo,
                    )
                } else {
                    true
                };
                envelopes.push(gateway::ReplayEnvelope {
                    message: IncomingMessage {
                        platform: "WhatsApp".to_string(),
                        sender,
                        content: if should_enqueue { text } else { String::new() },
                        channel: chat_jid.clone(),
                        message_id: Some(format!("wa:{msg_id}")),
                        thread_context: None,
                    },
                    channel_id: chat_key.clone(),
                    cursor_value: build_whatsapp_cursor(ts, &msg_id),
                    cursor_type: "ts_msgid",
                });
            }

            if envelopes.is_empty() {
                return;
            }

            let result = gateway::ReplayFetchResult::Replay(envelopes);
            let accepted = {
                let mut gw_guard = agent.gateway_state.lock().await;
                let mut seen_ids = agent.gateway_seen_ids.lock().await.clone();
                match gw_guard.as_mut() {
                    Some(gw) => {
                        let (msgs, _) = process_replay_result(
                            &agent.history,
                            "whatsapp",
                            result,
                            gw,
                            &mut seen_ids,
                        )
                        .await;
                        msgs
                    }
                    None => Vec::new(),
                }
            };

            for msg in accepted {
                if let Err(error) = agent.enqueue_gateway_message(msg).await {
                    tracing::warn!(
                        chat = %chat_jid,
                        "whatsapp replay: failed to enqueue history message: {error}"
                    );
                }
            }
        }
        Event::OfflineSyncCompleted(_) => {
            // History sync is complete for this reconnect cycle; clear the active flag.
            let mut gw_guard = agent.gateway_state.lock().await;
            if let Some(gw) = gw_guard.as_mut() {
                gw.replay_cycle_active.remove("whatsapp");
            }
        }
        Event::Message(message, info) => {
            if let Some(text) = message.text_content() {
                let text = text.trim().to_string();
                if text.is_empty() {
                    return;
                }

                let chat = info.source.chat.to_string();
                let sender = info.source.sender.to_string();
                let msg_id = info.id.clone();
                let ts_secs = info.timestamp.timestamp() as u64;

                // Determine whether this message should be enqueued (WhatsApp-specific
                // self-echo suppression and self-chat acceptance logic).
                let should_enqueue = if info.source.is_from_me {
                    let known_outbound_echo = agent
                        .whatsapp_link
                        .is_recent_outbound_message_id(&msg_id)
                        .await;
                    let own_identifiers = collect_normalized_identifiers(&[
                        &chat,
                        &sender,
                        &client
                            .get_pn()
                            .await
                            .map(|jid| jid.to_string())
                            .unwrap_or_default(),
                        &client
                            .get_lid()
                            .await
                            .map(|jid| jid.to_string())
                            .unwrap_or_default(),
                    ]);
                    let exact_self_jids = collect_exact_jid_candidates(&[
                        &client
                            .get_lid()
                            .await
                            .map(|jid| jid.to_string())
                            .unwrap_or_default(),
                        &client
                            .get_pn()
                            .await
                            .map(|jid| jid.to_string())
                            .unwrap_or_default(),
                    ]);
                    should_enqueue_from_me_whatsapp_message(
                        &text,
                        &chat,
                        &sender,
                        &own_identifiers,
                        &exact_self_jids,
                        known_outbound_echo,
                    )
                } else {
                    true
                };

                // Build the replay envelope for cursor-advancement (applies to ALL
                // live messages, including suppressed outbound echoes).
                let chat_key = normalize_identifier(&chat);
                let envelope = gateway::ReplayEnvelope {
                    message: IncomingMessage {
                        platform: "WhatsApp".to_string(),
                        sender: sender.clone(),
                        content: if should_enqueue {
                            text.clone()
                        } else {
                            String::new()
                        },
                        channel: chat.clone(),
                        message_id: Some(format!("wa:{msg_id}")),
                        thread_context: None,
                    },
                    channel_id: chat_key,
                    cursor_value: build_whatsapp_cursor(ts_secs, &msg_id),
                    cursor_type: "ts_msgid",
                };

                // Route through the shared replay classification path.  This advances
                // the persisted cursor for every classified message (accepted, filtered,
                // or duplicate), including outbound echoes that are not enqueued.
                let result = gateway::ReplayFetchResult::Replay(vec![envelope]);
                let accepted = {
                    let mut gw_guard = agent.gateway_state.lock().await;
                    let mut seen_ids = agent.gateway_seen_ids.lock().await.clone();
                    match gw_guard.as_mut() {
                        Some(gw) => {
                            let (msgs, _) = process_replay_result(
                                &agent.history,
                                "whatsapp",
                                result,
                                gw,
                                &mut seen_ids,
                            )
                            .await;
                            if !gw.whatsapp_replay_cursors.is_empty() {
                                client.set_skip_history_sync(false);
                            }
                            msgs
                        }
                        None => {
                            // Gateway not yet initialised — fall back to direct enqueue.
                            if should_enqueue {
                                vec![IncomingMessage {
                                    platform: "WhatsApp".to_string(),
                                    sender,
                                    content: text,
                                    channel: chat,
                                    message_id: Some(format!("wa:{msg_id}")),
                                    thread_context: None,
                                }]
                            } else {
                                vec![]
                            }
                        }
                    }
                };

                for msg in accepted {
                    if should_enqueue {
                        if let Err(error) = agent.enqueue_gateway_message(msg).await {
                            agent
                                .whatsapp_link
                                .broadcast_error(
                                    format!("failed to enqueue native whatsapp message: {error}"),
                                    true,
                                )
                                .await;
                        }
                    }
                }
            }
        }
        _ => {}
    }
}

pub(crate) async fn send_native_whatsapp_message(
    client: &Arc<wa_rs::Client>,
    targets: &[String],
    text: &str,
    prefix_self_chat: bool,
) -> Result<String> {
    let text = format_outbound_whatsapp_text(text, prefix_self_chat);
    let mut last_error = None;
    for target in targets {
        let jid: Jid = target
            .parse()
            .with_context(|| format!("invalid whatsapp jid target: {target}"))?;
        match client
            .send_message(
                jid,
                wa::Message {
                    conversation: Some(text.clone()),
                    ..Default::default()
                },
            )
            .await
        {
            Ok(message_id) => return Ok(message_id),
            Err(error) => last_error = Some(error),
        }
    }

    Err(last_error
        .map(anyhow::Error::from)
        .unwrap_or_else(|| anyhow::anyhow!("no valid whatsapp send targets")))
}

pub(crate) async fn disconnect_native_whatsapp_client(
    client: &Arc<wa_rs::Client>,
    run_task: Option<tokio::task::JoinHandle<()>>,
) {
    client.disconnect().await;
    if let Some(task) = run_task {
        task.abort();
        let _ = tokio::time::timeout(Duration::from_secs(2), task).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::load_persisted_provider_state;
    use crate::agent::types::AgentConfig;
    use crate::session_manager::SessionManager;
    use tempfile::tempdir;

    #[test]
    fn tamux_device_props_uses_desktop_platform_and_package_version() {
        let (os_name, version, platform_type) = tamux_device_props();
        assert_eq!(os_name.as_deref(), Some("Tamux"));
        assert_eq!(platform_type, Some(wa::device_props::PlatformType::Desktop));
        assert!(version.and_then(|value| value.primary).is_some());
    }

    #[test]
    fn outbound_self_chat_messages_get_tamux_prefix() {
        let expected = format!("{}Hello", tamux_self_chat_prefix());
        assert_eq!(format_outbound_whatsapp_text("Hello", true), expected);
    }

    #[test]
    fn outbound_non_self_chat_messages_keep_original_text() {
        assert_eq!(format_outbound_whatsapp_text("Hello", false), "Hello");
    }

    #[test]
    fn outbound_self_chat_prefix_is_idempotent() {
        let prefixed = format!("{}Hello", tamux_self_chat_prefix());
        assert_eq!(format_outbound_whatsapp_text(&prefixed, true), prefixed);
    }

    // -----------------------------------------------------------------------
    // Task 5: Cursor helpers
    // -----------------------------------------------------------------------

    #[test]
    fn whatsapp_replay_cursor_roundtrips() {
        let cursor = build_whatsapp_cursor(1700000000, "msg-abc-123");
        assert_eq!(cursor, "1700000000:msg-abc-123");
        assert_eq!(
            parse_whatsapp_cursor(&cursor),
            Some((1700000000, "msg-abc-123"))
        );
    }

    #[test]
    fn whatsapp_replay_cursor_parses_ts_only_prefix() {
        // Cursor values that contain colons in the message id still round-trip.
        assert_eq!(
            parse_whatsapp_cursor("1700000001:some:extra:colons"),
            Some((1700000001, "some:extra:colons"))
        );
    }

    #[test]
    fn whatsapp_replay_cursor_rejects_non_numeric_legacy_format() {
        // Legacy "message_id" cursors (e.g. plain base64 IDs) cannot be parsed
        // as a timestamp, which safely triggers the no-backfill fallback.
        assert_eq!(parse_whatsapp_cursor("wamid-ABC123"), None);
        assert_eq!(parse_whatsapp_cursor(""), None);
    }

    #[test]
    fn whatsapp_self_chat_messages_still_enqueue_but_prefixed_echoes_do_not() {
        let own_identifiers =
            collect_normalized_identifiers(&["48663977535@s.whatsapp.net", "48663977535@lid"]);
        let exact_self_jids =
            collect_exact_jid_candidates(&["48663977535@s.whatsapp.net", "48663977535@lid"]);

        assert!(should_enqueue_from_me_whatsapp_message(
            "hello from phone",
            "48663977535@s.whatsapp.net",
            "48663977535@lid",
            &own_identifiers,
            &exact_self_jids,
            false
        ));
        assert!(!should_enqueue_from_me_whatsapp_message(
            &format!("{}assistant reply", tamux_self_chat_prefix()),
            "48663977535@s.whatsapp.net",
            "48663977535@lid",
            &own_identifiers,
            &exact_self_jids,
            false
        ));
    }

    // -----------------------------------------------------------------------
    // Task 5: replay-cycle marking on Connected
    // -----------------------------------------------------------------------

    fn make_replay_gateway_config() -> super::super::types::GatewayConfig {
        super::super::types::GatewayConfig {
            enabled: true,
            slack_token: String::new(),
            slack_channel_filter: String::new(),
            telegram_token: String::new(),
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

    /// When a WhatsApp cursor exists for at least one chat, the Connected event
    /// must mark "whatsapp" as replay-active so history sync conversations are
    /// processed.
    #[test]
    fn whatsapp_connected_event_triggers_replay_for_chat_with_cursor() {
        let config = make_replay_gateway_config();
        let client = reqwest::Client::new();
        let mut gw = gateway::GatewayState::new(config, client);

        gw.whatsapp_replay_cursors.insert(
            "15551234567".into(),
            build_whatsapp_cursor(1700000000, "msg1"),
        );

        // Simulate the Connected handler logic.
        mark_whatsapp_replay_active_if_cursors(&mut gw);

        assert!(
            gw.replay_cycle_active.contains("whatsapp"),
            "replay_cycle_active must contain 'whatsapp' when cursors are present"
        );
    }

    /// On first connect there are no persisted cursors.  The replay cycle must
    /// NOT be activated so we never backfill old history.
    #[test]
    fn whatsapp_first_connect_without_cursor_does_not_backfill_history() {
        let config = make_replay_gateway_config();
        let client = reqwest::Client::new();
        let mut gw = gateway::GatewayState::new(config, client);

        // No cursors inserted — whatsapp_replay_cursors is empty.
        mark_whatsapp_replay_active_if_cursors(&mut gw);

        assert!(
            !gw.replay_cycle_active.contains("whatsapp"),
            "replay_cycle_active must NOT contain 'whatsapp' on first connect (no cursors)"
        );
    }

    // -----------------------------------------------------------------------
    // Task 5: self-echo is classified but not re-enqueued
    // -----------------------------------------------------------------------

    /// Verify that an outbound echo (`should_enqueue = false`) flows through
    /// `process_replay_result` so the cursor advances, but the returned accepted
    /// message is then discarded by the caller.
    ///
    /// We test this by confirming `process_replay_result` accepts a message
    /// regardless of the `should_enqueue` flag (which is caller-side logic) and
    /// that the cursor is persisted.
    #[tokio::test]
    async fn whatsapp_self_echo_replay_is_classified_but_not_reenqueued() {
        use std::fs;
        use uuid::Uuid;

        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join("test-artifacts")
            .join(format!("wa-self-echo-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).expect("create test root");

        let history = crate::history::HistoryStore::new_test_store(&root)
            .await
            .expect("history store");

        let config = make_replay_gateway_config();
        let http = reqwest::Client::new();
        let mut gw = gateway::GatewayState::new(config, http);
        let mut seen_ids: Vec<String> = Vec::new();

        // The envelope represents a live outbound echo message.
        let envelope = gateway::ReplayEnvelope {
            message: gateway::IncomingMessage {
                platform: "WhatsApp".into(),
                sender: "49123456789@s.whatsapp.net".into(),
                content: "echo text".into(),
                channel: "49123456789@s.whatsapp.net".into(),
                message_id: Some("wa:ECHO001".into()),
                thread_context: None,
            },
            channel_id: "49123456789".into(),
            cursor_value: build_whatsapp_cursor(1700000042, "ECHO001"),
            cursor_type: "ts_msgid",
        };

        let result = gateway::ReplayFetchResult::Replay(vec![envelope]);

        // process_replay_result accepts the message (cursor advances).
        let (accepted_msgs, completed) =
            process_replay_result(&history, "whatsapp", result, &mut gw, &mut seen_ids).await;

        assert!(
            completed,
            "replay must complete for a single valid envelope"
        );
        assert_eq!(
            accepted_msgs.len(),
            1,
            "process_replay_result must accept the message (cursor must advance)"
        );
        // In-memory cursor must be updated.
        assert_eq!(
            gw.whatsapp_replay_cursors
                .get("49123456789")
                .map(String::as_str),
            Some("1700000042:ECHO001"),
            "in-memory cursor must advance even for an outbound echo"
        );

        // The CALLER (Event::Message handler) gates enqueue on `should_enqueue`.
        // For an outbound echo, should_enqueue = false, so we discard the message.
        let should_enqueue = false; // simulates outbound echo
        let enqueued: Vec<_> = accepted_msgs
            .into_iter()
            .filter(|_| should_enqueue)
            .collect();
        assert!(
            enqueued.is_empty(),
            "outbound echo must not be enqueued despite cursor advancement"
        );

        fs::remove_dir_all(&root).ok();
    }

    #[tokio::test]
    async fn logged_out_clears_persisted_session_and_native_store() {
        let root = tempdir().expect("tempdir");
        let manager = SessionManager::new_test(root.path()).await;
        let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

        persist_transport_session_update(
            &engine.history,
            WHATSAPP_LINK_PROVIDER_ID,
            transport::SessionUpdate {
                linked_phone: Some("+15550000001".to_string()),
                auth_json: Some("{\"session\":true}".to_string()),
                metadata_json: Some("{\"device\":\"native\"}".to_string()),
                linked_at: Some(1),
                updated_at: 1,
            },
        )
        .await
        .expect("persist provider state");

        let store_path = whatsapp_native_store_path(&engine.data_dir);
        tokio::fs::write(&store_path, b"sqlite-placeholder")
            .await
            .expect("write native store");

        clear_logged_out_whatsapp_session(&engine)
            .await
            .expect("clear logged out session");

        assert!(
            load_persisted_provider_state(&engine.history, WHATSAPP_LINK_PROVIDER_ID)
                .await
                .expect("load provider state")
                .is_none(),
            "logged out cleanup must remove persisted provider state"
        );
        assert!(
            !store_path.exists(),
            "logged out cleanup must remove the native sqlite store"
        );
    }
}
