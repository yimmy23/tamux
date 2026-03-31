use super::*;

impl WhatsAppLinkRuntime {
    pub async fn send_message(&self, jid: &str, text: &str) -> Result<()> {
        let jid = jid.trim();
        if text.trim().is_empty() {
            bail!("whatsapp message body is empty");
        }
        let native_client = {
            let inner = self.inner.lock().await;
            inner.native_client.clone()
        };
        if let Some(client) = native_client {
            let own_pn = client
                .get_pn()
                .await
                .map(|jid| jid.to_string())
                .unwrap_or_default();
            let own_lid = client
                .get_lid()
                .await
                .map(|jid| jid.to_string())
                .unwrap_or_default();
            let (targets, prefix_self_chat) = resolve_native_send_plan(jid, &own_pn, &own_lid);
            let primary_target = targets.first().map(String::as_str).unwrap_or(jid);
            if primary_target.is_empty() {
                bail!("whatsapp send target is empty");
            }
            let sent_message_id = crate::agent::send_native_whatsapp_message(
                &client,
                &targets,
                text,
                prefix_self_chat,
            )
            .await;
            if let Ok(message_id) = sent_message_id.as_ref() {
                self.remember_outbound_message_id(message_id).await;
            }
            return sent_message_id.map(|_| ());
        }
        let targets = resolve_send_target_candidates(jid, &HashSet::new(), None, &[]);
        let primary_target = targets.first().map(String::as_str).unwrap_or(jid);
        if primary_target.is_empty() {
            bail!("whatsapp send target is empty");
        }
        self.send_sidecar_command(
            "send",
            serde_json::json!({
                "jid": primary_target,
                "text": text,
                "targets": targets,
            }),
        )
        .await
    }

    pub(super) async fn send_sidecar_command(&self, method: &str, params: serde_json::Value) -> Result<()> {
        let mut inner = self.inner.lock().await;
        if inner.stopping {
            bail!("whatsapp link sidecar is stopping");
        }
        if method == "send" && inner.state != WhatsAppLinkState::Connected {
            bail!("whatsapp link is not connected");
        }
        let rpc_id = inner.next_rpc_id;
        inner.next_rpc_id = inner.next_rpc_id.saturating_add(1);
        let payload = serde_json::json!({
            "id": rpc_id,
            "method": method,
            "params": params,
        });
        let line = format!("{payload}\n");
        let process = inner
            .process
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("whatsapp link sidecar is not running"))?;
        let stdin = process
            .stdin
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("whatsapp link sidecar stdin unavailable"))?;
        stdin
            .write_all(line.as_bytes())
            .await
            .context("failed to write command to whatsapp link sidecar stdin")?;
        Ok(())
    }

    pub async fn broadcast_qr(&self, ascii_qr: String, expires_at_ms: Option<u64>) {
        let ascii_qr = if looks_like_raw_pairing_qr_payload(&ascii_qr) {
            match render_pairing_qr_payload(&ascii_qr) {
                Ok(rendered) => rendered,
                Err(error) => {
                    tracing::warn!(error = %error, "failed to render whatsapp pairing qr payload");
                    ascii_qr
                }
            }
        } else {
            ascii_qr
        };
        let should_emit = {
            let mut inner = self.inner.lock().await;
            if inner.active_qr.as_deref() == Some(ascii_qr.as_str()) {
                false
            } else {
                inner.active_qr = Some(ascii_qr.clone());
                inner.active_qr_expires_at_ms = expires_at_ms;
                inner.state = WhatsAppLinkState::QrReady;
                inner.last_error = None;
                true
            }
        };
        if should_emit {
            self.broadcast_event(WhatsAppLinkEvent::Qr {
                ascii_qr,
                expires_at_ms,
            })
            .await;
            self.broadcast_status().await;
        }
    }

    pub async fn broadcast_linked(&self, phone: Option<String>) {
        {
            let mut inner = self.inner.lock().await;
            inner.state = WhatsAppLinkState::Connected;
            inner.last_error = None;
            inner.phone = phone.clone();
            inner.active_qr = None;
            inner.active_qr_expires_at_ms = None;
        }
        self.broadcast_event(WhatsAppLinkEvent::Linked { phone })
            .await;
        self.broadcast_status().await;
    }

    pub async fn broadcast_error(&self, message: String, recoverable: bool) {
        {
            let mut inner = self.inner.lock().await;
            inner.last_error = Some(message.clone());
            if recoverable {
                if inner.state != WhatsAppLinkState::Connected {
                    inner.state = WhatsAppLinkState::Error;
                }
                inner.retry_count = inner.retry_count.saturating_add(1);
                inner.last_retry_at_ms = Some(now_millis());
            } else {
                inner.state = WhatsAppLinkState::Error;
            }
        }
        self.broadcast_event(WhatsAppLinkEvent::Error {
            message,
            recoverable,
        })
        .await;
        self.broadcast_status().await;
    }

    pub async fn broadcast_disconnected(&self, reason: Option<String>) {
        {
            let mut inner = self.inner.lock().await;
            inner.state = WhatsAppLinkState::Disconnected;
            inner.phone = None;
            inner.active_qr = None;
            inner.active_qr_expires_at_ms = None;
        }
        self.broadcast_event(WhatsAppLinkEvent::Disconnected { reason })
            .await;
    }

    pub(super) async fn broadcast_status(&self) {
        let snapshot = self.status_snapshot().await;
        self.broadcast_event(WhatsAppLinkEvent::Status(snapshot))
            .await;
    }

    pub(super) async fn broadcast_event(&self, event: WhatsAppLinkEvent) {
        let mut subscribers = self.subscribers.lock().await;
        subscribers.retain(|_, tx| tx.send(event.clone()).is_ok());
    }
}
