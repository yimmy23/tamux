use std::collections::{HashMap, VecDeque};

use amux_protocol::{GatewayCursorState, GatewayProviderBootstrap, GatewaySendRequest};
use anyhow::{bail, Context, Result};
use serde_json::{json, Value};

use crate::format::{chunk_message, markdown_to_slack_mrkdwn, SLACK_MAX_CHARS};
use crate::health::{PlatformHealthState, TokenBucket};
use crate::router::{normalize_message, RawGatewayMessage};
use crate::runtime::{GatewayProvider, GatewayProviderEvent, GatewaySendOutcome};

pub struct SlackProvider {
    token: String,
    api_base: String,
    channels: Vec<String>,
    connected: bool,
    cursors: HashMap<String, String>,
    pending_events: VecDeque<GatewayProviderEvent>,
    health: PlatformHealthState,
    rate_limiter: TokenBucket,
    last_poll_ms: Option<u64>,
    poll_interval_ms: u64,
}

impl SlackProvider {
    pub fn from_bootstrap(bootstrap: &GatewayProviderBootstrap) -> Result<Option<Self>> {
        Self::from_bootstrap_with_cursors(bootstrap, &[])
    }

    pub fn from_bootstrap_with_cursors(
        bootstrap: &GatewayProviderBootstrap,
        cursors: &[GatewayCursorState],
    ) -> Result<Option<Self>> {
        if bootstrap.platform != "slack" || !bootstrap.enabled {
            return Ok(None);
        }

        let credentials: Value =
            serde_json::from_str(&bootstrap.credentials_json).context("parse slack credentials")?;
        let config: Value =
            serde_json::from_str(&bootstrap.config_json).context("parse slack config")?;

        let token = credentials
            .get("token")
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim();
        if token.is_empty() {
            return Ok(None);
        }

        let channels = csv_values(config.get("channel_filter").and_then(Value::as_str).unwrap_or(""));
        let api_base = config
            .get("api_base")
            .and_then(Value::as_str)
            .unwrap_or("https://slack.com/api")
            .trim_end_matches('/')
            .to_string();
        let poll_interval_ms = config
            .get("poll_interval_ms")
            .and_then(Value::as_u64)
            .unwrap_or(60_000);

        let mut replay_cursors = HashMap::new();
        for cursor in cursors {
            if cursor.platform.eq_ignore_ascii_case("slack") {
                replay_cursors.insert(cursor.channel_id.clone(), cursor.cursor_value.clone());
            }
        }

        Ok(Some(Self {
            token: token.to_string(),
            api_base,
            channels,
            connected: false,
            cursors: replay_cursors,
            pending_events: VecDeque::new(),
            health: PlatformHealthState::new(),
            rate_limiter: TokenBucket::slack(),
            last_poll_ms: None,
            poll_interval_ms,
        }))
    }

    async fn poll_channel(&mut self, channel_id: &str) -> Result<()> {
        let Some(cursor) = self.cursors.get(channel_id).cloned() else {
            let url = format!(
                "{}/conversations.history?channel={channel_id}&limit=100",
                self.api_base
            );
            let body = self.get_json(&url).await?;
            let newest = body
                .get("messages")
                .and_then(Value::as_array)
                .and_then(|messages| {
                    messages
                        .iter()
                        .find_map(|msg| msg.get("ts").and_then(Value::as_str))
                });
            if let Some(ts) = newest.filter(|ts| !ts.is_empty()) {
                self.cursors.insert(channel_id.to_string(), ts.to_string());
                self.pending_events
                    .push_back(GatewayProviderEvent::CursorUpdate(GatewayCursorState {
                        platform: "slack".to_string(),
                        channel_id: channel_id.to_string(),
                        cursor_value: ts.to_string(),
                        cursor_type: "message_ts".to_string(),
                        updated_at_ms: now_ms(),
                    }));
            }
            return Ok(());
        };

        let mut latest = None::<String>;
        let mut replay = Vec::new();

        loop {
            let mut url = format!(
                "{}/conversations.history?channel={channel_id}&limit=100&oldest={cursor}&inclusive=false",
                self.api_base
            );
            if let Some(value) = &latest {
                url.push_str("&latest=");
                url.push_str(value);
            }

            let body = self.get_json(&url).await?;
            let has_more = body
                .get("has_more")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let Some(messages) = body.get("messages").and_then(Value::as_array) else {
                break;
            };
            if messages.is_empty() {
                break;
            }

            let mut oldest_visible = None::<String>;
            let mut saw_newer = false;
            for message in messages {
                let ts = message.get("ts").and_then(Value::as_str).unwrap_or("");
                if ts.is_empty() {
                    continue;
                }
                oldest_visible = Some(ts.to_string());
                if ts <= cursor.as_str() {
                    continue;
                }
                saw_newer = true;
                if let Some(normalized) = parse_slack_message(channel_id, message) {
                    replay.push((ts.to_string(), normalized));
                }
            }

            if !has_more || !saw_newer {
                break;
            }

            let Some(next_latest) = oldest_visible else {
                break;
            };
            latest = Some(next_latest);
        }

        replay.sort_by(|left, right| left.0.cmp(&right.0));
        let mut newest_cursor = None::<String>;
        for (ts, message) in replay {
            newest_cursor = Some(ts);
            self.pending_events
                .push_back(GatewayProviderEvent::Incoming(message));
        }

        if let Some(cursor_value) = newest_cursor {
            self.cursors
                .insert(channel_id.to_string(), cursor_value.clone());
            self.pending_events
                .push_back(GatewayProviderEvent::CursorUpdate(GatewayCursorState {
                    platform: "slack".to_string(),
                    channel_id: channel_id.to_string(),
                    cursor_value,
                    cursor_type: "message_ts".to_string(),
                    updated_at_ms: now_ms(),
                }));
        }

        Ok(())
    }

    async fn get_json(&self, url: &str) -> Result<Value> {
        let response = reqwest::Client::new()
            .get(url)
            .bearer_auth(&self.token)
            .timeout(std::time::Duration::from_secs(5))
            .send()
            .await
            .with_context(|| format!("slack request failed for {url}"))?;
        let body: Value = response
            .json()
            .await
            .with_context(|| format!("slack response parse failed for {url}"))?;
        if body.get("ok").and_then(Value::as_bool) != Some(true) {
            let error = body
                .get("error")
                .and_then(Value::as_str)
                .unwrap_or("unknown error");
            bail!("slack API error: {error}");
        }
        Ok(body)
    }
}

impl GatewayProvider for SlackProvider {
    fn platform(&self) -> &str {
        "slack"
    }

    fn connect(
        &mut self,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send + '_>> {
        Box::pin(async move {
            let url = format!("{}/auth.test", self.api_base);
            self.get_json(&url).await?;
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
                bail!("Slack provider not connected");
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
                Ok(()) => {
                    self.health.on_success(now);
                }
                Err(error) => {
                    self.health.on_failure(now, error.to_string());
                }
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
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<GatewaySendOutcome>> + Send + '_>> {
        Box::pin(async move {
            if !self.connected {
                bail!("Slack provider not connected");
            }
            if let Some(wait) = self.rate_limiter.try_acquire() {
                tokio::time::sleep(wait).await;
            }

            let formatted = markdown_to_slack_mrkdwn(&request.content);
            let chunks = chunk_message(&formatted, SLACK_MAX_CHARS);
            let url = format!("{}/chat.postMessage", self.api_base);
            let mut delivery_id = None::<String>;

            for chunk in chunks.iter() {
                let mut payload = json!({
                    "channel": request.channel_id,
                    "text": chunk,
                });
                if let Some(thread_ts) = request.thread_id.as_deref().filter(|value| !value.is_empty()) {
                    payload["thread_ts"] = json!(thread_ts);
                }
                let body = reqwest::Client::new()
                    .post(&url)
                    .bearer_auth(&self.token)
                    .json(&payload)
                    .timeout(std::time::Duration::from_secs(5))
                    .send()
                    .await
                    .context("slack send request failed")?
                    .json::<Value>()
                    .await
                    .context("slack send response parse failed")?;
                if body.get("ok").and_then(Value::as_bool) != Some(true) {
                    let error = body
                        .get("error")
                        .and_then(Value::as_str)
                        .unwrap_or("unknown error");
                    bail!("Slack API error: {error}");
                }
                delivery_id = body.get("ts").and_then(Value::as_str).map(str::to_string);
            }

            Ok(GatewaySendOutcome {
                channel_id: request.channel_id,
                delivery_id,
            })
        })
    }
}

impl SlackProvider {
    fn health_state(&self) -> amux_protocol::GatewayHealthState {
        amux_protocol::GatewayHealthState {
            platform: "slack".to_string(),
            status: self.health.status,
            last_success_at_ms: self.health.last_success_at,
            last_error_at_ms: self.health.last_error_at,
            consecutive_failure_count: self.health.consecutive_failure_count,
            last_error: self.health.last_error.clone(),
            current_backoff_secs: self.health.current_backoff_secs,
        }
    }
}

fn parse_slack_message(channel_id: &str, message: &Value) -> Option<crate::router::GatewayMessage> {
    let ts = message.get("ts").and_then(Value::as_str).unwrap_or("");
    let text = message.get("text").and_then(Value::as_str).unwrap_or("");
    let subtype = message.get("subtype").and_then(Value::as_str);
    if ts.is_empty() || text.is_empty() || subtype == Some("bot_message") {
        return None;
    }
    normalize_message(RawGatewayMessage {
        platform: "slack",
        channel_id,
        user_id: message
            .get("user")
            .and_then(Value::as_str)
            .unwrap_or("unknown"),
        sender_display: None,
        text,
        message_id: Some(format!("slack:{channel_id}:{ts}")),
        thread_id: message
            .get("thread_ts")
            .and_then(Value::as_str)
            .or(Some(ts))
            .map(str::to_string),
        timestamp: parse_slack_timestamp(ts),
        raw_event_json: serde_json::to_string(message).ok(),
    })
}

fn parse_slack_timestamp(value: &str) -> u64 {
    value
        .split('.')
        .next()
        .and_then(|seconds| seconds.parse::<u64>().ok())
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
mod tests {
    use amux_protocol::GatewaySendRequest;
    use serde_json::json;

    use super::*;
    use crate::test_support::{HttpResponse, TestHttpServer};

    #[tokio::test]
    async fn slack_provider_connects_and_posts_messages_via_http_client() {
        let server = TestHttpServer::spawn(vec![
            HttpResponse::ok(json!({ "ok": true, "user_id": "U123" }).to_string()),
            HttpResponse::ok(json!({ "ok": true, "ts": "1712345678.000100" }).to_string()),
        ])
        .await
        .expect("spawn server");

        let bootstrap = GatewayProviderBootstrap {
            platform: "slack".to_string(),
            enabled: true,
            credentials_json: json!({ "token": "xoxb-test" }).to_string(),
            config_json: json!({
                "api_base": server.base_url,
                "channel_filter": "C123"
            })
            .to_string(),
        };

        let mut provider = SlackProvider::from_bootstrap(&bootstrap)
            .expect("provider bootstrap")
            .expect("provider enabled");
        provider.connect().await.expect("connect succeeds");

        let outcome = provider
            .send(GatewaySendRequest {
                correlation_id: "send-1".to_string(),
                platform: "slack".to_string(),
                channel_id: "C123".to_string(),
                thread_id: Some("1712345600.000100".to_string()),
                content: "**deploy** complete".to_string(),
            })
            .await
            .expect("send succeeds");

        let requests = server.requests();
        assert_eq!(requests.len(), 2, "expected auth + post requests");
        assert_eq!(requests[0].method, "GET");
        assert!(requests[0].path.contains("/auth.test"));
        assert_eq!(requests[1].method, "POST");
        assert!(requests[1].path.contains("/chat.postMessage"));
        assert!(requests[1].body.contains("\"channel\":\"C123\""));
        assert!(requests[1].body.contains("\"thread_ts\":\"1712345600.000100\""));
        assert!(requests[1].body.contains("\"text\":\"*deploy* complete\""));
        assert_eq!(outcome.channel_id, "C123");
        assert_eq!(outcome.delivery_id.as_deref(), Some("1712345678.000100"));
    }
}
