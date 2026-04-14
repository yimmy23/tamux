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

mod helpers;

pub(crate) use helpers::{
    build_whatsapp_cursor, classify_whatsapp_enqueue_decision, whatsapp_native_store_path,
    WhatsAppEnqueueDecision,
};
pub(super) use helpers::{clear_logged_out_whatsapp_session, now_millis_local};
use helpers::{
    format_outbound_whatsapp_text, is_whatsapp_cursor_newer, log_whatsapp_allowlist_suppression,
    mark_whatsapp_replay_active_if_cursors, tamux_device_props,
};
#[cfg(test)]
pub(crate) use helpers::{
    parse_whatsapp_cursor, should_enqueue_from_me_whatsapp_message, tamux_self_chat_prefix,
};

#[cfg_attr(test, allow(dead_code))]
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

#[cfg_attr(test, allow(dead_code))]
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

            let whatsapp_allowed_contacts = agent
                .config
                .read()
                .await
                .gateway
                .whatsapp_allowed_contacts
                .clone();

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
                let decision = classify_whatsapp_enqueue_decision(
                    &text,
                    &chat_jid,
                    &sender,
                    &own_identifiers,
                    &exact_self_jids,
                    known_outbound_echo,
                    is_from_me,
                    &whatsapp_allowed_contacts,
                );
                let should_enqueue = matches!(decision, WhatsAppEnqueueDecision::Enqueue);
                if matches!(decision, WhatsAppEnqueueDecision::SuppressAllowlist) {
                    log_whatsapp_allowlist_suppression(&sender, &chat_jid);
                }
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
                let whatsapp_allowed_contacts = agent
                    .config
                    .read()
                    .await
                    .gateway
                    .whatsapp_allowed_contacts
                    .clone();

                // Determine whether this message should be enqueued (WhatsApp-specific
                // self-echo suppression and self-chat acceptance logic).
                let known_outbound_echo = if info.source.is_from_me {
                    agent
                        .whatsapp_link
                        .is_recent_outbound_message_id(&msg_id)
                        .await
                } else {
                    false
                };
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
                let decision = classify_whatsapp_enqueue_decision(
                    &text,
                    &chat,
                    &sender,
                    &own_identifiers,
                    &exact_self_jids,
                    known_outbound_echo,
                    info.source.is_from_me,
                    &whatsapp_allowed_contacts,
                );
                let should_enqueue = matches!(decision, WhatsAppEnqueueDecision::Enqueue);
                if matches!(decision, WhatsAppEnqueueDecision::SuppressAllowlist) {
                    log_whatsapp_allowlist_suppression(&sender, &chat);
                }

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
#[path = "tests/whatsapp_native/mod.rs"]
mod tests;
