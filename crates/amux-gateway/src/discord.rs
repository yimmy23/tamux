use std::collections::{HashMap, HashSet, VecDeque};
use std::time::Duration;

use amux_protocol::{GatewayCursorState, GatewayProviderBootstrap, GatewaySendRequest};
use anyhow::{bail, Context, Result};
use serde_json::{json, Value};

use crate::format::{chunk_message, markdown_to_discord, DISCORD_MAX_CHARS};
use crate::health::{PlatformHealthState, TokenBucket};
use crate::router::{normalize_message, RawGatewayMessage};
use crate::runtime::{GatewayProvider, GatewayProviderEvent, GatewaySendOutcome};

pub struct DiscordProvider {
    token: String,
    api_base: String,
    channels: Vec<String>,
    allowed_users: HashSet<String>,
    connected: bool,
    cursors: HashMap<String, String>,
    pending_events: VecDeque<GatewayProviderEvent>,
    health: PlatformHealthState,
    self_user_id: Option<String>,
    rate_limiter: TokenBucket,
    last_poll_ms: Option<u64>,
    poll_interval_ms: u64,
}

impl DiscordProvider {
    #[cfg(test)]
    pub fn from_bootstrap(bootstrap: &GatewayProviderBootstrap) -> Result<Option<Self>> {
        Self::from_bootstrap_with_cursors(bootstrap, &[])
    }

    pub fn from_bootstrap_with_cursors(
        bootstrap: &GatewayProviderBootstrap,
        cursors: &[GatewayCursorState],
    ) -> Result<Option<Self>> {
        if bootstrap.platform != "discord" || !bootstrap.enabled {
            return Ok(None);
        }

        let credentials: Value = serde_json::from_str(&bootstrap.credentials_json)
            .context("parse discord credentials")?;
        let config: Value =
            serde_json::from_str(&bootstrap.config_json).context("parse discord config")?;

        let token = credentials
            .get("token")
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim();
        if token.is_empty() {
            return Ok(None);
        }

        let mut replay_cursors = HashMap::new();
        for cursor in cursors {
            if cursor.platform.eq_ignore_ascii_case("discord") {
                replay_cursors.insert(cursor.channel_id.clone(), cursor.cursor_value.clone());
            }
        }
        let poll_interval_ms = config
            .get("poll_interval_ms")
            .and_then(Value::as_u64)
            .unwrap_or(5_000);

        Ok(Some(Self {
            token: token.to_string(),
            api_base: config
                .get("api_base")
                .and_then(Value::as_str)
                .unwrap_or("https://discord.com/api/v10")
                .trim_end_matches('/')
                .to_string(),
            channels: csv_values(
                config
                    .get("channel_filter")
                    .and_then(Value::as_str)
                    .unwrap_or(""),
            ),
            allowed_users: csv_values(
                config
                    .get("allowed_users")
                    .and_then(Value::as_str)
                    .unwrap_or(""),
            )
            .into_iter()
            .collect(),
            connected: false,
            cursors: replay_cursors,
            pending_events: VecDeque::new(),
            health: PlatformHealthState::new(),
            self_user_id: None,
            rate_limiter: TokenBucket::discord(),
            last_poll_ms: None,
            poll_interval_ms,
        }))
    }

    fn authorization_header(&self) -> String {
        let trimmed = self.token.trim();
        let normalized = trimmed
            .strip_prefix("Bot ")
            .or_else(|| trimmed.strip_prefix("bot "))
            .or_else(|| trimmed.strip_prefix("Bearer "))
            .or_else(|| trimmed.strip_prefix("bearer "))
            .unwrap_or(trimmed)
            .trim();
        format!("Bot {normalized}")
    }

    async fn get_messages(&self, url: &str) -> Result<Value> {
        let response = reqwest::Client::new()
            .get(url)
            .header("Authorization", self.authorization_header())
            .timeout(Duration::from_secs(5))
            .send()
            .await
            .with_context(|| format!("discord request failed for {url}"))?;
        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            bail!("discord request rejected: status={status}, body={body}");
        }
        response
            .json::<Value>()
            .await
            .with_context(|| format!("discord response parse failed for {url}"))
    }

    async fn resolve_target_channel(&self, channel_or_user: &str) -> Result<String> {
        let explicit_user = channel_or_user
            .strip_prefix("user:")
            .or_else(|| channel_or_user.strip_prefix("dm:"))
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);
        let treat_as_user = explicit_user.clone().or_else(|| {
            self.allowed_users
                .contains(channel_or_user)
                .then(|| channel_or_user.to_string())
        });

        let Some(user_id) = treat_as_user else {
            return Ok(channel_or_user.to_string());
        };

        let response = reqwest::Client::new()
            .post(format!("{}/users/@me/channels", self.api_base))
            .header("Authorization", self.authorization_header())
            .json(&json!({ "recipient_id": user_id }))
            .timeout(Duration::from_secs(5))
            .send()
            .await
            .context("discord DM channel creation failed")?;
        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            bail!("discord DM creation rejected: status={status}, body={body}");
        }
        let body = response
            .json::<Value>()
            .await
            .context("discord DM channel parse failed")?;
        body.get("id")
            .and_then(Value::as_str)
            .map(str::to_string)
            .ok_or_else(|| anyhow::anyhow!("discord DM channel response missing id"))
    }

    async fn poll_channel(&mut self, channel_id: &str) -> Result<()> {
        let had_cursor = self.cursors.contains_key(channel_id);
        let mut url = format!("{}/channels/{channel_id}/messages?limit=5", self.api_base);
        if let Some(after) = self.cursors.get(channel_id) {
            url.push_str("&after=");
            url.push_str(after);
        }

        let body = self.get_messages(&url).await?;
        let Some(messages) = body.as_array() else {
            return Ok(());
        };

        let newest_id = messages
            .first()
            .and_then(|message| message.get("id").and_then(Value::as_str))
            .unwrap_or("");

        if !had_cursor {
            if !newest_id.is_empty() {
                self.cursors
                    .insert(channel_id.to_string(), newest_id.to_string());
                self.pending_events
                    .push_back(GatewayProviderEvent::CursorUpdate(GatewayCursorState {
                        platform: "discord".to_string(),
                        channel_id: channel_id.to_string(),
                        cursor_value: newest_id.to_string(),
                        cursor_type: "message_id".to_string(),
                        updated_at_ms: now_ms(),
                    }));
            }
            return Ok(());
        }

        let mut newest_cursor = None::<String>;
        for message in messages.iter().rev() {
            if let Some(normalized) =
                parse_discord_message(channel_id, message, self.self_user_id.as_deref())
            {
                newest_cursor = message
                    .get("id")
                    .and_then(Value::as_str)
                    .map(str::to_string)
                    .or(newest_cursor);
                self.pending_events
                    .push_back(GatewayProviderEvent::Incoming(normalized));
            }
        }

        if newest_cursor.is_none() && !newest_id.is_empty() {
            newest_cursor = Some(newest_id.to_string());
        }

        if let Some(cursor_value) = newest_cursor {
            self.cursors
                .insert(channel_id.to_string(), cursor_value.clone());
            self.pending_events
                .push_back(GatewayProviderEvent::CursorUpdate(GatewayCursorState {
                    platform: "discord".to_string(),
                    channel_id: channel_id.to_string(),
                    cursor_value,
                    cursor_type: "message_id".to_string(),
                    updated_at_ms: now_ms(),
                }));
        }

        Ok(())
    }

    fn health_state(&self) -> amux_protocol::GatewayHealthState {
        amux_protocol::GatewayHealthState {
            platform: "discord".to_string(),
            status: self.health.status,
            last_success_at_ms: self.health.last_success_at,
            last_error_at_ms: self.health.last_error_at,
            consecutive_failure_count: self.health.consecutive_failure_count,
            last_error: self.health.last_error.clone(),
            current_backoff_secs: self.health.current_backoff_secs,
        }
    }
}

impl GatewayProvider for DiscordProvider {
    fn platform(&self) -> &str {
        "discord"
    }

    fn connect(
        &mut self,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send + '_>> {
        Box::pin(async move {
            let url = format!("{}/users/@me", self.api_base);
            let body = self.get_messages(&url).await?;
            self.self_user_id = body.get("id").and_then(Value::as_str).map(str::to_string);
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
                bail!("Discord provider not connected");
            }

            let now = now_ms();
            if !self.health.should_retry(now) {
                return Ok(None);
            }
            if let Some(last_poll_ms) = self.last_poll_ms {
                if now.saturating_sub(last_poll_ms) < self.poll_interval_ms {
                    return Ok(None);
                }
            }
            self.last_poll_ms = Some(now);

            let old_status = self.health.status;
            let channels = self.channels.clone();
            let outcome = async {
                for channel_id in channels {
                    self.poll_channel(&channel_id).await?;
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
                        amux_protocol::GatewayConnectionStatus::Disconnected,
                        amux_protocol::GatewayConnectionStatus::Connected
                    )
                ))
                || (self.health.status == amux_protocol::GatewayConnectionStatus::Error
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
                bail!("Discord provider not connected");
            }
            if let Some(wait) = self.rate_limiter.try_acquire() {
                tokio::time::sleep(wait).await;
            }

            let formatted = markdown_to_discord(&request.content);
            let chunks = chunk_message(&formatted, DISCORD_MAX_CHARS);
            let mut delivery_id = None::<String>;
            let target_channel = self.resolve_target_channel(&request.channel_id).await?;

            for (index, chunk) in chunks.iter().enumerate() {
                let mut payload = json!({ "content": chunk });
                if index == 0 {
                    if let Some(message_id) = request
                        .thread_id
                        .as_deref()
                        .filter(|value| !value.is_empty())
                    {
                        payload["message_reference"] = json!({
                            "message_id": message_id,
                            "fail_if_not_exists": false
                        });
                    }
                }

                let body = reqwest::Client::new()
                    .post(format!(
                        "{}/channels/{}/messages",
                        self.api_base, target_channel
                    ))
                    .header("Authorization", self.authorization_header())
                    .json(&payload)
                    .timeout(Duration::from_secs(5))
                    .send()
                    .await
                    .context("discord send request failed")?;
                if !body.status().is_success() {
                    let status = body.status();
                    let error_body = body.text().await.unwrap_or_default();
                    bail!("Discord API error (status {status}): {error_body}");
                }
                let body = body
                    .json::<Value>()
                    .await
                    .context("discord send response parse failed")?;
                delivery_id = body.get("id").and_then(Value::as_str).map(str::to_string);
            }

            Ok(GatewaySendOutcome {
                channel_id: target_channel,
                delivery_id,
            })
        })
    }
}

fn parse_discord_message(
    channel_id: &str,
    message: &Value,
    self_user_id: Option<&str>,
) -> Option<crate::router::GatewayMessage> {
    let id = message.get("id").and_then(Value::as_str).unwrap_or("");
    let content = message.get("content").and_then(Value::as_str).unwrap_or("");
    let author = message.get("author");
    let author_id = author
        .and_then(|value| value.get("id"))
        .and_then(Value::as_str)
        .unwrap_or("");
    let is_bot = author
        .and_then(|value| value.get("bot"))
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let is_self_authored = self_user_id
        .filter(|value| !value.is_empty())
        .is_some_and(|self_user_id| self_user_id == author_id);
    let is_application_message =
        message.get("webhook_id").is_some() || message.get("application_id").is_some();
    if id.is_empty() || content.is_empty() || is_bot || is_self_authored || is_application_message {
        return None;
    }

    normalize_message(RawGatewayMessage {
        platform: "discord",
        channel_id,
        user_id: if author_id.is_empty() {
            "unknown"
        } else {
            author_id
        },
        sender_display: author
            .and_then(|value| value.get("username"))
            .and_then(Value::as_str)
            .map(str::to_string),
        text: content,
        message_id: Some(format!("discord:{id}")),
        thread_id: Some(id.to_string()),
        timestamp: discord_timestamp_secs(id),
        raw_event_json: serde_json::to_string(message).ok(),
    })
}

fn discord_timestamp_secs(id: &str) -> u64 {
    const DISCORD_EPOCH_MS: u64 = 1_420_070_400_000;
    id.parse::<u64>()
        .ok()
        .map(|snowflake| ((snowflake >> 22) + DISCORD_EPOCH_MS) / 1000)
        .unwrap_or(0)
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
#[path = "discord/tests.rs"]
mod tests;
