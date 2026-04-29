use std::collections::{HashSet, VecDeque};
use std::time::Duration;

use anyhow::{bail, Context, Result};
use serde_json::{json, Value};
use zorai_protocol::{GatewayCursorState, GatewayProviderBootstrap, GatewaySendRequest};

use crate::format::{
    chunk_message, markdown_to_plain, markdown_to_telegram_v2, TELEGRAM_MAX_CHARS,
};
use crate::health::{PlatformHealthState, TokenBucket};
use crate::router::{normalize_message, RawGatewayMessage};
use crate::runtime::{GatewayProvider, GatewayProviderEvent, GatewaySendOutcome};

pub struct TelegramProvider {
    token: String,
    api_base: String,
    allowed_chats: HashSet<String>,
    connected: bool,
    update_offset: i64,
    pending_events: VecDeque<GatewayProviderEvent>,
    health: PlatformHealthState,
    rate_limiter: TokenBucket,
}

impl TelegramProvider {
    #[cfg(test)]
    pub fn from_bootstrap(bootstrap: &GatewayProviderBootstrap) -> Result<Option<Self>> {
        Self::from_bootstrap_with_cursors(bootstrap, &[])
    }

    pub fn from_bootstrap_with_cursors(
        bootstrap: &GatewayProviderBootstrap,
        cursors: &[GatewayCursorState],
    ) -> Result<Option<Self>> {
        if bootstrap.platform != "telegram" || !bootstrap.enabled {
            return Ok(None);
        }

        let credentials: Value = serde_json::from_str(&bootstrap.credentials_json)
            .context("parse telegram credentials")?;
        let config: Value =
            serde_json::from_str(&bootstrap.config_json).context("parse telegram config")?;
        let token = credentials
            .get("token")
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim();
        if token.is_empty() {
            return Ok(None);
        }

        let update_offset = cursors
            .iter()
            .find(|cursor| cursor.platform.eq_ignore_ascii_case("telegram"))
            .and_then(|cursor| cursor.cursor_value.parse::<i64>().ok())
            .map(|value| value + 1)
            .unwrap_or(0);

        Ok(Some(Self {
            token: token.to_string(),
            api_base: config
                .get("api_base")
                .and_then(Value::as_str)
                .unwrap_or("https://api.telegram.org")
                .trim_end_matches('/')
                .to_string(),
            allowed_chats: csv_values(
                config
                    .get("allowed_chats")
                    .and_then(Value::as_str)
                    .unwrap_or(""),
            )
            .into_iter()
            .collect(),
            connected: false,
            update_offset,
            pending_events: VecDeque::new(),
            health: PlatformHealthState::new(),
            rate_limiter: TokenBucket::telegram(),
        }))
    }

    fn endpoint(&self, method: &str) -> String {
        format!(
            "{}/bot{}/{}",
            self.api_base,
            self.token,
            method.trim_start_matches('/')
        )
    }

    async fn request_json(&self, method: &str) -> Result<Value> {
        reqwest::Client::new()
            .get(self.endpoint(method))
            .timeout(Duration::from_secs(5))
            .send()
            .await
            .with_context(|| format!("telegram request failed for {method}"))?
            .json::<Value>()
            .await
            .with_context(|| format!("telegram response parse failed for {method}"))
    }

    async fn send_payload(&self, payload: &Value) -> Result<Value> {
        reqwest::Client::new()
            .post(self.endpoint("sendMessage"))
            .json(payload)
            .timeout(Duration::from_secs(5))
            .send()
            .await
            .context("telegram sendMessage request failed")?
            .json::<Value>()
            .await
            .context("telegram sendMessage response parse failed")
    }

    fn health_state(&self) -> zorai_protocol::GatewayHealthState {
        zorai_protocol::GatewayHealthState {
            platform: "telegram".to_string(),
            status: self.health.status,
            last_success_at_ms: self.health.last_success_at,
            last_error_at_ms: self.health.last_error_at,
            consecutive_failure_count: self.health.consecutive_failure_count,
            last_error: self.health.last_error.clone(),
            current_backoff_secs: self.health.current_backoff_secs,
        }
    }

    async fn send_chunks(
        &self,
        chunks: &[String],
        chat_id: &str,
        reply_to_message_id: Option<i64>,
        parse_mode: Option<&str>,
    ) -> Result<Option<String>> {
        let mut delivery_id = None::<String>;
        for (index, chunk) in chunks.iter().enumerate() {
            let mut payload = json!({
                "chat_id": chat_id,
                "text": chunk,
            });
            if let Some(parse_mode) = parse_mode {
                payload["parse_mode"] = json!(parse_mode);
            }
            if index == 0 {
                if let Some(reply_to_message_id) = reply_to_message_id {
                    payload["reply_to_message_id"] = json!(reply_to_message_id);
                }
            }

            let body = self.send_payload(&payload).await?;
            if body.get("ok").and_then(Value::as_bool) != Some(true) {
                let error = body
                    .get("description")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown error");
                bail!("Telegram API error: {error}");
            }
            delivery_id = body
                .get("result")
                .and_then(|value| value.get("message_id"))
                .and_then(Value::as_i64)
                .map(|value| value.to_string());
        }
        Ok(delivery_id)
    }
}

impl GatewayProvider for TelegramProvider {
    fn platform(&self) -> &str {
        "telegram"
    }

    fn connect(
        &mut self,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send + '_>> {
        Box::pin(async move {
            let body = self.request_json("getMe").await?;
            if body.get("ok").and_then(Value::as_bool) != Some(true) {
                let error = body
                    .get("description")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown error");
                bail!("Telegram API error: {error}");
            }
            self.connected = true;
            Ok(())
        })
    }

    fn recv(
        &mut self,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<Option<GatewayProviderEvent>>> + Send + '_>,
    > {
        Box::pin(async move {
            if let Some(event) = self.pending_events.pop_front() {
                return Ok(Some(event));
            }
            if !self.connected {
                bail!("Telegram provider not connected");
            }

            let now = now_ms();
            if !self.health.should_retry(now) {
                return Ok(None);
            }

            let old_status = self.health.status;
            let url = format!(
                "{}?offset={}&timeout=1&limit=10",
                self.endpoint("getUpdates"),
                self.update_offset
            );
            let outcome = async {
                let body = reqwest::Client::new()
                    .get(&url)
                    .timeout(Duration::from_secs(5))
                    .send()
                    .await
                    .context("telegram getUpdates request failed")?
                    .json::<Value>()
                    .await
                    .context("telegram getUpdates response parse failed")?;

                if body.get("ok").and_then(Value::as_bool) != Some(true) {
                    let error = body
                        .get("description")
                        .and_then(Value::as_str)
                        .unwrap_or("unknown error");
                    bail!("Telegram API error: {error}");
                }

                if let Some(results) = body.get("result").and_then(Value::as_array) {
                    let mut latest_cursor = None::<String>;
                    for update in results {
                        let update_id = update
                            .get("update_id")
                            .and_then(Value::as_i64)
                            .unwrap_or_default();
                        if update_id >= self.update_offset {
                            self.update_offset = update_id + 1;
                            latest_cursor = Some(update_id.to_string());
                        }

                        let message = update.get("message").or_else(|| update.get("channel_post"));
                        let Some(message) = message else {
                            continue;
                        };

                        let text = message.get("text").and_then(Value::as_str).unwrap_or("");
                        if text.is_empty() {
                            continue;
                        }

                        let chat_id = message
                            .get("chat")
                            .and_then(|value| value.get("id"))
                            .and_then(Value::as_i64)
                            .map(|value| value.to_string())
                            .unwrap_or_default();
                        if !self.allowed_chats.is_empty() && !self.allowed_chats.contains(&chat_id)
                        {
                            continue;
                        }

                        let sender = message
                            .get("from")
                            .and_then(|value| value.get("username"))
                            .and_then(Value::as_str)
                            .or_else(|| {
                                message
                                    .get("from")
                                    .and_then(|value| value.get("first_name"))
                                    .and_then(Value::as_str)
                            })
                            .unwrap_or("unknown");

                        if let Some(normalized) = normalize_message(RawGatewayMessage {
                            platform: "telegram",
                            channel_id: &chat_id,
                            user_id: sender,
                            sender_display: Some(sender.to_string()),
                            text,
                            message_id: message
                                .get("message_id")
                                .and_then(Value::as_i64)
                                .map(|value| format!("tg:{value}")),
                            thread_id: message
                                .get("message_id")
                                .and_then(Value::as_i64)
                                .map(|value| value.to_string()),
                            timestamp: message.get("date").and_then(Value::as_u64).unwrap_or(0),
                            raw_event_json: serde_json::to_string(message).ok(),
                        }) {
                            self.pending_events
                                .push_back(GatewayProviderEvent::Incoming(normalized));
                        }
                    }

                    if let Some(cursor_value) = latest_cursor {
                        self.pending_events
                            .push_back(GatewayProviderEvent::CursorUpdate(GatewayCursorState {
                                platform: "telegram".to_string(),
                                channel_id: "global".to_string(),
                                cursor_value,
                                cursor_type: "update_id".to_string(),
                                updated_at_ms: now_ms(),
                            }));
                    }
                }

                Result::<()>::Ok(())
            }
            .await;

            match outcome {
                Ok(()) => self.health.on_success(now),
                Err(error) => self.health.on_failure(now, error.to_string()),
            }

            let emit_health = (self.health.status != old_status
                && !matches!(
                    (old_status, self.health.status),
                    (
                        zorai_protocol::GatewayConnectionStatus::Disconnected,
                        zorai_protocol::GatewayConnectionStatus::Connected
                    )
                ))
                || (self.health.status == zorai_protocol::GatewayConnectionStatus::Error
                    && self.health.last_error.is_some());
            if emit_health {
                self.pending_events
                    .push_front(GatewayProviderEvent::HealthUpdate(self.health_state()));
            }

            Ok(self.pending_events.pop_front())
        })
    }

    fn send(
        &mut self,
        request: GatewaySendRequest,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<GatewaySendOutcome>> + Send + '_>>
    {
        Box::pin(async move {
            if !self.connected {
                bail!("Telegram provider not connected");
            }
            if let Some(wait) = self.rate_limiter.try_acquire() {
                tokio::time::sleep(wait).await;
            }

            let reply_to_message_id = request
                .thread_id
                .as_deref()
                .and_then(|value| value.parse::<i64>().ok());

            let formatted = markdown_to_telegram_v2(&request.content);
            let chunks = chunk_message(&formatted, TELEGRAM_MAX_CHARS);
            let delivery_id = match self
                .send_chunks(
                    &chunks,
                    &request.channel_id,
                    reply_to_message_id,
                    Some("MarkdownV2"),
                )
                .await
            {
                Ok(delivery_id) => Ok(delivery_id),
                Err(error) if error.to_string().contains("can't parse entities") => {
                    let plain = markdown_to_plain(&request.content);
                    let plain_chunks = chunk_message(&plain, TELEGRAM_MAX_CHARS);
                    self.send_chunks(
                        &plain_chunks,
                        &request.channel_id,
                        reply_to_message_id,
                        None,
                    )
                    .await
                }
                Err(error) => Err(error),
            }?;

            Ok(GatewaySendOutcome {
                channel_id: request.channel_id,
                delivery_id,
            })
        })
    }
}

fn csv_values(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .collect()
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
#[path = "telegram/tests.rs"]
mod tests;
