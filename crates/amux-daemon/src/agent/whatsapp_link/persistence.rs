use super::*;

pub fn persisted_state_from_history_row(
    row: WhatsAppProviderStateRow,
) -> transport::PersistedState {
    transport::PersistedState {
        linked_phone: row.linked_phone,
        auth_json: row.auth_json,
        metadata_json: row.metadata_json,
        last_reset_at: row.last_reset_at,
        last_linked_at: row.last_linked_at,
        updated_at: row.updated_at,
    }
}

pub fn persisted_state_into_history_row(
    provider_id: &str,
    state: transport::PersistedState,
) -> WhatsAppProviderStateRow {
    WhatsAppProviderStateRow {
        provider_id: provider_id.to_string(),
        linked_phone: state.linked_phone,
        auth_json: state.auth_json,
        metadata_json: state.metadata_json,
        last_reset_at: state.last_reset_at,
        last_linked_at: state.last_linked_at,
        updated_at: state.updated_at,
    }
}

pub async fn load_persisted_provider_state(
    history: &HistoryStore,
    provider_id: &str,
) -> Result<Option<transport::PersistedState>> {
    Ok(history
        .get_whatsapp_provider_state(provider_id)
        .await?
        .map(persisted_state_from_history_row))
}

pub async fn save_persisted_provider_state(
    history: &HistoryStore,
    provider_id: &str,
    state: transport::PersistedState,
) -> Result<()> {
    history
        .upsert_whatsapp_provider_state(persisted_state_into_history_row(provider_id, state))
        .await
}

pub async fn clear_persisted_provider_state(
    history: &HistoryStore,
    provider_id: &str,
) -> Result<()> {
    history.delete_whatsapp_provider_state(provider_id).await
}

pub fn merge_persisted_state_update(
    existing: Option<transport::PersistedState>,
    update: transport::SessionUpdate,
) -> transport::PersistedState {
    let existing = existing.unwrap_or(transport::PersistedState {
        linked_phone: None,
        auth_json: None,
        metadata_json: None,
        last_reset_at: None,
        last_linked_at: None,
        updated_at: update.updated_at,
    });

    transport::PersistedState {
        linked_phone: update.linked_phone.or(existing.linked_phone),
        auth_json: update.auth_json.or(existing.auth_json),
        metadata_json: update.metadata_json.or(existing.metadata_json),
        last_reset_at: existing.last_reset_at,
        last_linked_at: update.linked_at.or(existing.last_linked_at),
        updated_at: update.updated_at,
    }
}

pub async fn persist_transport_session_update(
    history: &HistoryStore,
    provider_id: &str,
    update: transport::SessionUpdate,
) -> Result<transport::PersistedState> {
    let merged = merge_persisted_state_update(
        load_persisted_provider_state(history, provider_id).await?,
        update,
    );
    save_persisted_provider_state(history, provider_id, merged.clone()).await?;
    Ok(merged)
}

#[allow(dead_code)]
pub async fn apply_transport_event(
    runtime: &WhatsAppLinkRuntime,
    history: &HistoryStore,
    provider_id: &str,
    event: transport::WhatsAppTransportEvent,
) -> Result<()> {
    match event {
        transport::WhatsAppTransportEvent::Starting => runtime.start().await,
        transport::WhatsAppTransportEvent::Qr {
            ascii_qr,
            expires_at_ms,
        } => {
            runtime.broadcast_qr(ascii_qr, expires_at_ms).await;
            Ok(())
        }
        transport::WhatsAppTransportEvent::SessionUpdated(update) => {
            persist_transport_session_update(history, provider_id, update).await?;
            Ok(())
        }
        transport::WhatsAppTransportEvent::Linked { phone } => {
            runtime.broadcast_linked(phone).await;
            Ok(())
        }
        transport::WhatsAppTransportEvent::Disconnected { reason } => {
            runtime.broadcast_disconnected(reason).await;
            Ok(())
        }
        transport::WhatsAppTransportEvent::Error {
            message,
            recoverable,
        } => {
            runtime.broadcast_error(message, recoverable).await;
            Ok(())
        }
    }
}

#[allow(dead_code)]
pub fn spawn_transport_event_bridge(
    runtime: Arc<WhatsAppLinkRuntime>,
    history: HistoryStore,
    provider_id: String,
    mut rx: broadcast::Receiver<transport::WhatsAppTransportEvent>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            match rx.recv().await {
                Ok(event) => {
                    if let Err(error) =
                        apply_transport_event(&runtime, &history, &provider_id, event).await
                    {
                        runtime
                            .broadcast_error(
                                format!("failed to handle whatsapp transport event: {error}"),
                                false,
                            )
                            .await;
                    }
                }
                Err(broadcast::error::RecvError::Lagged(skipped)) => {
                    runtime
                        .broadcast_error(
                            format!("whatsapp transport event bridge lagged by {skipped} event(s)"),
                            true,
                        )
                        .await;
                }
                Err(broadcast::error::RecvError::Closed) => break,
            }
        }
    })
}

#[allow(dead_code)]
pub async fn start_transport_bridge<T>(
    runtime: Arc<WhatsAppLinkRuntime>,
    history: HistoryStore,
    transport: Arc<T>,
) -> Result<JoinHandle<()>>
where
    T: transport::WhatsAppTransport + 'static,
{
    let provider_id = transport.provider_id();
    let restored_state = load_persisted_provider_state(&history, provider_id).await?;
    let rx = transport.subscribe();
    transport.start(restored_state).await?;
    Ok(spawn_transport_event_bridge(
        runtime,
        history,
        provider_id.to_string(),
        rx,
    ))
}
