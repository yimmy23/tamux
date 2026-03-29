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
use super::{persist_transport_session_update, AgentEngine, WHATSAPP_LINK_PROVIDER_ID};
use super::whatsapp_link::{
    collect_exact_jid_candidates, collect_normalized_identifiers, normalize_identifier,
    normalize_jid_user, transport,
};

pub(crate) fn whatsapp_native_store_path(base_dir: &Path) -> PathBuf {
    base_dir.join("whatsapp-link.sqlite")
}

pub(crate) async fn start_whatsapp_link_native(agent: Arc<AgentEngine>) -> Result<()> {
    let store_path = whatsapp_native_store_path(&agent.data_dir);
    if let Some(parent) = store_path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .with_context(|| format!("failed to create whatsapp store directory {}", parent.display()))?;
    }

    let store = Arc::new(
        SqliteStore::new(&store_path.to_string_lossy())
            .await
            .with_context(|| format!("failed to open wa-rs sqlite store {}", store_path.display()))?,
    );
    let store_path_string = store_path.to_string_lossy().to_string();
    let agent_for_events = agent.clone();
    let mut bot = Bot::builder()
        .with_backend(store)
        .with_transport_factory(TokioWebSocketTransportFactory::new())
        .with_http_client(UreqHttpClient::new())
        .skip_history_sync()
        .on_event(move |event, client| {
            let agent = agent_for_events.clone();
            let store_path = store_path_string.clone();
            async move {
                handle_native_event(agent, client, event, &store_path).await;
            }
        })
        .build()
        .await
        .context("failed to build wa-rs bot")?;

    let client = bot.client();
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

async fn handle_native_event(
    agent: Arc<AgentEngine>,
    client: Arc<wa_rs::Client>,
    event: Event,
    store_path: &str,
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
                    })
                    .to_string(),
                ),
                linked_at: Some(now_millis_local()),
                updated_at: now_millis_local(),
            };
            if let Err(error) = persist_transport_session_update(
                &agent.history,
                WHATSAPP_LINK_PROVIDER_ID,
                update,
            )
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
        }
        Event::Disconnected(_) => {
            agent
                .whatsapp_link
                .broadcast_disconnected(Some("native_disconnected".to_string()))
                .await;
        }
        Event::LoggedOut(_) => {
            agent
                .whatsapp_link
                .broadcast_disconnected(Some("logged_out".to_string()))
                .await;
        }
        Event::PairError(error) => {
            agent
                .whatsapp_link
                .broadcast_error(format!("pairing failed: {}", error.error), false)
                .await;
        }
        Event::Message(message, info) => {
            if let Some(text) = message.text_content() {
                let text = text.trim().to_string();
                if text.is_empty() {
                    return;
                }

                let chat = info.source.chat.to_string();
                let sender = info.source.sender.to_string();
                if info.source.is_from_me {
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
                    let chat_norm = normalize_identifier(&chat);
                    let sender_norm = normalize_identifier(&sender);
                    let self_chat = !chat_norm.is_empty()
                        && own_identifiers.contains(&chat_norm)
                        && !sender_norm.is_empty()
                        && own_identifiers.contains(&sender_norm)
                        && !exact_self_jids.is_empty();
                    if !self_chat {
                        return;
                    }
                }

                let sender_display = sender.clone();
                if let Err(error) = agent
                    .enqueue_gateway_message(IncomingMessage {
                        platform: "WhatsApp".to_string(),
                        sender: sender_display,
                        content: text,
                        channel: chat,
                        message_id: Some(format!("wa:{}", info.id)),
                        thread_context: None,
                    })
                    .await
                {
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
        _ => {}
    }
}

pub(crate) async fn send_native_whatsapp_message(
    client: &Arc<wa_rs::Client>,
    targets: &[String],
    text: &str,
) -> Result<()> {
    let mut last_error = None;
    for target in targets {
        let jid: Jid = target
            .parse()
            .with_context(|| format!("invalid whatsapp jid target: {target}"))?;
        match client
            .send_message(
                jid,
                wa::Message {
                    conversation: Some(text.to_string()),
                    ..Default::default()
                },
            )
            .await
        {
            Ok(_) => return Ok(()),
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
