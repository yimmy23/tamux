use super::*;

impl Default for WhatsAppLinkRuntime {
    fn default() -> Self {
        Self::new()
    }
}

impl WhatsAppLinkRuntime {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(RuntimeInner {
                state: WhatsAppLinkState::Disconnected,
                phone: None,
                last_error: None,
                active_qr: None,
                active_qr_expires_at_ms: None,
                process: None,
                native_client: None,
                native_task: None,
                stopping: false,
                retry_count: 0,
                last_retry_at_ms: None,
                recent_outbound_message_ids: VecDeque::new(),
                next_rpc_id: 1,
                #[cfg(test)]
                forced_stop_kill_error: None,
            }),
            subscribers: Mutex::new(HashMap::new()),
            next_subscriber_id: AtomicU64::new(1),
        }
    }

    pub async fn start(&self) -> Result<()> {
        let _ = self.start_if_idle().await?;
        Ok(())
    }

    pub async fn stop(&self, reason: Option<String>) -> Result<()> {
        #[cfg(test)]
        let forced_stop_kill_error;
        let (mut child, native_client, native_task) = {
            let mut inner = self.inner.lock().await;
            inner.stopping = true;
            inner.retry_count = 0;
            inner.last_retry_at_ms = None;
            #[cfg(test)]
            {
                forced_stop_kill_error = inner.forced_stop_kill_error.take();
            }
            (
                inner.process.take(),
                inner.native_client.take(),
                inner.native_task.take(),
            )
        };

        let kill_result = if let Some(ref mut proc) = child {
            #[cfg(test)]
            let forced_stop_kill_error = forced_stop_kill_error;
            #[cfg(not(test))]
            let forced_stop_kill_error: Option<String> = None;
            if let Some(message) = forced_stop_kill_error {
                Err(anyhow::Error::msg(message))
            } else {
                proc.kill()
                    .await
                    .context("failed to stop whatsapp link sidecar process")
            }
        } else {
            Ok(())
        };

        if let Some(client) = native_client.as_ref() {
            crate::agent::disconnect_native_whatsapp_client(client, native_task).await;
        }

        match kill_result {
            Ok(()) => {
                {
                    let mut inner = self.inner.lock().await;
                    inner.process = None;
                    inner.native_client = None;
                    inner.native_task = None;
                    inner.stopping = false;
                    inner.last_error = None;
                }
                self.broadcast_disconnected(reason).await;
                self.broadcast_status().await;
                Ok(())
            }
            Err(err) => {
                let message = err.to_string();
                {
                    let mut inner = self.inner.lock().await;
                    inner.process = child;
                    inner.native_client = native_client;
                    inner.stopping = false;
                    inner.state = WhatsAppLinkState::Error;
                    inner.last_error = Some(message.clone());
                }
                self.broadcast_event(WhatsAppLinkEvent::Error {
                    message,
                    recoverable: false,
                })
                .await;
                self.broadcast_status().await;
                Err(err)
            }
        }
    }

    pub async fn reset(&self) -> Result<()> {
        self.stop(Some("operator_reset".to_string())).await?;
        {
            let mut inner = self.inner.lock().await;
            inner.phone = None;
            inner.last_error = None;
            inner.active_qr = None;
            inner.active_qr_expires_at_ms = None;
            inner.retry_count = 0;
            inner.last_retry_at_ms = None;
            inner.recent_outbound_message_ids.clear();
            inner.state = WhatsAppLinkState::Disconnected;
        }
        self.broadcast_status().await;
        Ok(())
    }

    pub async fn status_snapshot(&self) -> WhatsAppLinkStatusSnapshot {
        let inner = self.inner.lock().await;
        WhatsAppLinkStatusSnapshot {
            state: inner.state.as_str().to_string(),
            phone: inner.phone.clone(),
            last_error: inner.last_error.clone(),
        }
    }

    pub async fn subscribe_with_id(&self) -> (u64, broadcast::Receiver<WhatsAppLinkEvent>) {
        let (tx, rx) = broadcast::channel(64);
        let id = self.next_subscriber_id.fetch_add(1, Ordering::Relaxed);
        {
            let inner = self.inner.lock().await;
            let snapshot = WhatsAppLinkStatusSnapshot {
                state: inner.state.as_str().to_string(),
                phone: inner.phone.clone(),
                last_error: inner.last_error.clone(),
            };
            let replay_qr = if inner.state == WhatsAppLinkState::QrReady {
                inner.active_qr.clone()
            } else {
                None
            };
            let replay_qr_expires_at_ms = if replay_qr.is_some() {
                inner.active_qr_expires_at_ms
            } else {
                None
            };
            let mut subscribers = self.subscribers.lock().await;
            let _ = tx.send(WhatsAppLinkEvent::Status(snapshot));
            if let Some(ascii_qr) = replay_qr {
                let _ = tx.send(WhatsAppLinkEvent::Qr {
                    ascii_qr,
                    expires_at_ms: replay_qr_expires_at_ms,
                });
            }
            subscribers.insert(id, tx.clone());
        }
        (id, rx)
    }

    pub async fn subscribe(&self) -> broadcast::Receiver<WhatsAppLinkEvent> {
        let (_, rx) = self.subscribe_with_id().await;
        rx
    }

    pub async fn unsubscribe(&self, subscriber_id: u64) {
        let mut subscribers = self.subscribers.lock().await;
        subscribers.remove(&subscriber_id);
    }

    #[cfg(test)]
    pub async fn subscriber_count(&self) -> usize {
        self.subscribers.lock().await.len()
    }

    pub async fn start_if_idle(&self) -> Result<bool> {
        let should_broadcast = {
            let mut inner = self.inner.lock().await;
            if inner.stopping {
                bail!("whatsapp link transport is stopping");
            }
            if matches!(
                inner.state,
                WhatsAppLinkState::Starting
                    | WhatsAppLinkState::QrReady
                    | WhatsAppLinkState::Connected
            ) {
                false
            } else {
                inner.state = WhatsAppLinkState::Starting;
                inner.last_error = None;
                true
            }
        };
        if should_broadcast {
            self.broadcast_status().await;
        }
        Ok(should_broadcast)
    }

    pub async fn attach_sidecar_process(&self, child: Child) -> Result<()> {
        let (mut incoming, mut previous) = {
            let mut inner = self.inner.lock().await;
            if inner.stopping {
                (Some(child), None)
            } else {
                let previous = inner.process.take();
                inner.process = Some(child);
                (None, previous)
            }
        };

        if let Some(ref mut process) = incoming {
            process
                .kill()
                .await
                .context("failed to discard whatsapp link sidecar while stop is in progress")?;
        }

        if let Some(ref mut process) = previous {
            process
                .kill()
                .await
                .context("failed to replace existing whatsapp link sidecar process")?;
        }

        Ok(())
    }

    pub async fn attach_native_client(
        &self,
        client: Arc<wa_rs::Client>,
        run_task: JoinHandle<()>,
    ) -> Result<()> {
        let (incoming, previous_process, previous_client, previous_task) = {
            let mut inner = self.inner.lock().await;
            if inner.stopping {
                (Some(client), None, None, Some(run_task))
            } else {
                (
                    None,
                    inner.process.take(),
                    inner.native_client.replace(client),
                    inner.native_task.replace(run_task),
                )
            }
        };

        if let Some(client) = incoming.as_ref() {
            crate::agent::disconnect_native_whatsapp_client(client, previous_task).await;
        } else if let Some(client) = previous_client.as_ref() {
            crate::agent::disconnect_native_whatsapp_client(client, previous_task).await;
        }

        if let Some(mut process) = previous_process {
            process
                .kill()
                .await
                .context("failed to replace existing whatsapp sidecar transport")?;
        }

        Ok(())
    }

    pub async fn remember_outbound_message_id(&self, message_id: &str) {
        let trimmed = message_id.trim();
        if trimmed.is_empty() {
            return;
        }
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_millis() as u64)
            .unwrap_or(0);
        let mut inner = self.inner.lock().await;
        while let Some((_, recorded_at_ms)) = inner.recent_outbound_message_ids.front() {
            if now_ms.saturating_sub(*recorded_at_ms) <= 120_000 {
                break;
            }
            inner.recent_outbound_message_ids.pop_front();
        }
        if inner
            .recent_outbound_message_ids
            .iter()
            .any(|(existing_id, _)| existing_id == trimmed)
        {
            return;
        }
        inner
            .recent_outbound_message_ids
            .push_back((trimmed.to_string(), now_ms));
        while inner.recent_outbound_message_ids.len() > 512 {
            inner.recent_outbound_message_ids.pop_front();
        }
    }

    pub async fn is_recent_outbound_message_id(&self, message_id: &str) -> bool {
        let trimmed = message_id.trim();
        if trimmed.is_empty() {
            return false;
        }
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_millis() as u64)
            .unwrap_or(0);
        let mut inner = self.inner.lock().await;
        while let Some((_, recorded_at_ms)) = inner.recent_outbound_message_ids.front() {
            if now_ms.saturating_sub(*recorded_at_ms) <= 120_000 {
                break;
            }
            inner.recent_outbound_message_ids.pop_front();
        }
        inner
            .recent_outbound_message_ids
            .iter()
            .any(|(existing_id, _)| existing_id == trimmed)
    }

    pub async fn has_sidecar_process(&self) -> bool {
        let inner = self.inner.lock().await;
        inner.process.is_some()
    }

    pub async fn has_native_client(&self) -> bool {
        let inner = self.inner.lock().await;
        inner.native_client.is_some()
    }

    pub async fn connect_sidecar(&self) -> Result<()> {
        self.send_sidecar_command("connect", serde_json::json!({}))
            .await
    }
}
