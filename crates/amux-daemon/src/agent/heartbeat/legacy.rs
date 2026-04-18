#![allow(dead_code)]

use super::*;

impl AgentEngine {
    pub(super) async fn run_heartbeat(&self) -> Result<()> {
        let items = self.heartbeat_items.read().await.clone();
        let now = now_millis();

        for item in &items {
            if !item.enabled {
                continue;
            }

            let interval_ms = if item.interval_minutes > 0 {
                item.interval_minutes * 60 * 1000
            } else {
                self.config.read().await.heartbeat_interval_mins * 60 * 1000
            };

            let due = match item.last_run_at {
                Some(last) => now - last >= interval_ms,
                None => true,
            };

            if !due {
                continue;
            }

            let prompt = format!(
                "Heartbeat check: {}\n\n\
                 Respond with HEARTBEAT_OK if everything is normal, \
                 or HEARTBEAT_ALERT: <explanation> if something needs attention.",
                item.prompt
            );

            let result = match self
                .send_internal_message_as(
                    None,
                    crate::agent::agent_identity::WELES_AGENT_ID,
                    &prompt,
                )
                .await
            {
                Ok(thread_id) => {
                    let threads = self.threads.read().await;
                    let response = threads
                        .get(&thread_id)
                        .and_then(|t| {
                            t.messages
                                .iter()
                                .rev()
                                .find(|m| m.role == MessageRole::Assistant)
                                .map(|m| m.content.clone())
                        })
                        .unwrap_or_default();

                    if response.contains("HEARTBEAT_OK") {
                        (HeartbeatOutcome::Ok, "OK".into())
                    } else if response.contains("HEARTBEAT_ALERT") {
                        (HeartbeatOutcome::Alert, response)
                    } else {
                        (HeartbeatOutcome::Ok, response)
                    }
                }
                Err(e) => (HeartbeatOutcome::Error, format!("Error: {e}")),
            };

            let _ = self.event_tx.send(AgentEvent::HeartbeatResult {
                item_id: item.id.clone(),
                result: result.0,
                message: result.1.clone(),
            });

            {
                let mut items = self.heartbeat_items.write().await;
                if let Some(i) = items.iter_mut().find(|i| i.id == item.id) {
                    i.last_run_at = Some(now);
                    i.last_result = Some(result.0);
                    i.last_message = Some(result.1);
                }
            }

            if result.0 == HeartbeatOutcome::Alert && item.notify_on_alert {
                let _ = self.event_tx.send(AgentEvent::Notification {
                    title: format!("Heartbeat Alert: {}", item.label),
                    body: item.last_message.clone().unwrap_or_default(),
                    severity: NotificationSeverity::Alert,
                    channels: item.notify_channels.clone(),
                });
                let now_ts = now as i64;
                let _ = self
                    .upsert_inbox_notification(amux_protocol::InboxNotification {
                        id: format!("heartbeat-alert:{}", item.id),
                        source: "heartbeat".to_string(),
                        kind: "heartbeat_alert".to_string(),
                        title: format!("Heartbeat Alert: {}", item.label),
                        body: item.last_message.clone().unwrap_or_default(),
                        subtitle: Some("heartbeat".to_string()),
                        severity: "alert".to_string(),
                        created_at: now_ts,
                        updated_at: now_ts,
                        read_at: None,
                        archived_at: None,
                        deleted_at: None,
                        actions: Vec::new(),
                        metadata_json: None,
                    })
                    .await;
            }
        }

        self.persist_heartbeat().await;
        Ok(())
    }
}
