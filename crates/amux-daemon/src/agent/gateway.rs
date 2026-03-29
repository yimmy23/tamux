//! Gateway: direct platform connections for receiving messages.
//!
//! Runs inside the daemon. No separate process, no Electron dependency.
//! - Telegram: long-poll getUpdates API
//! - Slack: poll conversations.history API
//! - Discord: poll channel messages REST API
//!
//! Incoming messages are routed to the agent engine for processing.

use std::collections::{HashMap, HashSet};

use anyhow::{Context, Result};

use super::gateway_health::{PlatformHealthState, TokenBucket};
use super::types::GatewayConfig;

fn discord_authorization_header(token: &str) -> String {
    let trimmed = token.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    let normalized = trimmed
        .strip_prefix("Bot ")
        .or_else(|| trimmed.strip_prefix("bot "))
        .or_else(|| trimmed.strip_prefix("Bearer "))
        .or_else(|| trimmed.strip_prefix("bearer "))
        .unwrap_or(trimmed)
        .trim();
    format!("Bot {normalized}")
}

/// Thread context for reply routing — captures platform-specific message
/// references needed to reply in the correct thread/conversation.
#[derive(Debug, Clone, Default)]
pub struct ThreadContext {
    /// Slack: thread_ts of the parent message (for in-thread replies).
    pub slack_thread_ts: Option<String>,
    /// Discord: message ID to reference in reply.
    pub discord_message_id: Option<String>,
    /// Telegram: message_id to reply to.
    pub telegram_message_id: Option<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GatewayRouteMode {
    #[default]
    Rarog,
    Swarog,
}

impl GatewayRouteMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Rarog => "rarog",
            Self::Swarog => "swarog",
        }
    }

    pub fn parse(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "swarog" | "main" => Self::Swarog,
            _ => Self::Rarog,
        }
    }
}

/// State for tracking already-seen messages per platform.
pub struct GatewayState {
    pub config: GatewayConfig,
    pub telegram_offset: i64,
    pub slack_last_ts: HashMap<String, String>,
    pub discord_last_id: HashMap<String, String>,
    pub telegram_replay_cursor: Option<i64>,
    pub slack_replay_cursors: HashMap<String, String>,
    pub discord_replay_cursors: HashMap<String, String>,
    pub whatsapp_replay_cursors: HashMap<String, String>,
    pub replay_cycle_active: HashSet<String>,
    pub http_client: reqwest::Client,
    // Per-platform health tracking (Plan 02)
    pub slack_health: PlatformHealthState,
    pub discord_health: PlatformHealthState,
    pub telegram_health: PlatformHealthState,
    /// Per-channel thread context for auto-injecting reply metadata.
    /// Key: "Platform:channel_id", Value: most recent ThreadContext.
    pub reply_contexts: HashMap<String, ThreadContext>,
    /// Per-channel timestamp of last outgoing agent response (epoch millis).
    /// Key: "Platform:channel_id". Populated by send tools after successful sends.
    pub last_response_at: HashMap<String, u64>,
    /// Per-channel timestamp of last incoming message (epoch millis).
    /// Key: "Platform:channel_id". Populated by poll_gateway_messages.
    pub last_incoming_at: HashMap<String, u64>,
    /// Per-platform rate limiters. Stored here so they persist across tool calls.
    pub slack_rate_limiter: TokenBucket,
    pub discord_rate_limiter: TokenBucket,
    pub telegram_rate_limiter: TokenBucket,
    /// Slack poll interval in seconds (default 60, conservative for rate limits).
    pub slack_poll_interval_secs: u64,
    /// Last time Slack was polled (epoch millis).
    pub last_slack_poll_ms: Option<u64>,
}

impl GatewayState {
    pub fn new(config: GatewayConfig, http_client: reqwest::Client) -> Self {
        Self {
            config,
            telegram_offset: 0,
            slack_last_ts: HashMap::new(),
            discord_last_id: HashMap::new(),
            telegram_replay_cursor: None,
            slack_replay_cursors: HashMap::new(),
            discord_replay_cursors: HashMap::new(),
            whatsapp_replay_cursors: HashMap::new(),
            replay_cycle_active: HashSet::new(),
            http_client,
            slack_health: PlatformHealthState::new(),
            discord_health: PlatformHealthState::new(),
            telegram_health: PlatformHealthState::new(),
            reply_contexts: HashMap::new(),
            last_response_at: HashMap::new(),
            last_incoming_at: HashMap::new(),
            slack_rate_limiter: TokenBucket::slack(),
            discord_rate_limiter: TokenBucket::discord(),
            telegram_rate_limiter: TokenBucket::telegram(),
            slack_poll_interval_secs: 60,
            last_slack_poll_ms: None,
        }
    }
}

/// Incoming message from any platform.
#[derive(Debug, Clone)]
pub struct IncomingMessage {
    pub platform: String,
    pub sender: String,
    pub content: String,
    pub channel: String,
    /// Platform-specific unique message ID for deduplication.
    pub message_id: Option<String>,
    /// Thread context for reply routing — captures the message reference
    /// needed to reply in the correct thread/conversation on the platform.
    pub thread_context: Option<ThreadContext>,
}

#[derive(Debug, Clone)]
pub enum ReplayFetchResult {
    Replay(Vec<ReplayEnvelope>),
    InitializeBoundary {
        channel_id: String,
        cursor_value: String,
        cursor_type: &'static str,
    },
}

#[derive(Debug, Clone)]
pub struct ReplayEnvelope {
    pub message: IncomingMessage,
    pub channel_id: String,
    pub cursor_value: String,
    pub cursor_type: &'static str,
}

fn parse_telegram_replay_envelope(update: &serde_json::Value) -> Option<ReplayEnvelope> {
    let update_id = update.get("update_id").and_then(|v| v.as_i64())?;
    let msg = update
        .get("message")
        .or_else(|| update.get("channel_post"))?;
    let text = msg.get("text").and_then(|v| v.as_str()).unwrap_or("");
    if text.is_empty() {
        return None;
    }

    let from = msg.get("from");
    let sender = from
        .and_then(|f| f.get("username").and_then(|v| v.as_str()))
        .or_else(|| from.and_then(|f| f.get("first_name").and_then(|v| v.as_str())))
        .unwrap_or("unknown")
        .to_string();

    let chat_id = msg
        .get("chat")
        .and_then(|c| c.get("id"))
        .and_then(|v| v.as_i64())
        .map(|id| id.to_string())
        .unwrap_or_default();

    let telegram_message_id = msg.get("message_id").and_then(|v| v.as_i64());
    let reply_msg_id = msg
        .get("reply_to_message")
        .and_then(|r| r.get("message_id"))
        .and_then(|v| v.as_i64());

    Some(ReplayEnvelope {
        message: IncomingMessage {
            platform: "Telegram".into(),
            sender,
            content: text.into(),
            channel: chat_id,
            message_id: msg
                .get("message_id")
                .and_then(|v| v.as_i64())
                .map(|id| format!("tg:{id}")),
            thread_context: Some(ThreadContext {
                telegram_message_id: telegram_message_id.or(reply_msg_id),
                ..Default::default()
            }),
        },
        channel_id: "global".into(),
        cursor_value: update_id.to_string(),
        cursor_type: "update_id",
    })
}

fn parse_slack_replay_envelope(channel: &str, msg: &serde_json::Value) -> Option<ReplayEnvelope> {
    let ts = msg.get("ts").and_then(|v| v.as_str()).unwrap_or("");
    let text = msg.get("text").and_then(|v| v.as_str()).unwrap_or("");
    let user = msg
        .get("user")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");
    let subtype = msg.get("subtype").and_then(|v| v.as_str());

    if ts.is_empty() || text.is_empty() || subtype == Some("bot_message") {
        return None;
    }

    let thread_ts = msg
        .get("thread_ts")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .or_else(|| Some(ts.to_string()));

    Some(ReplayEnvelope {
        message: IncomingMessage {
            platform: "Slack".into(),
            sender: user.into(),
            content: text.into(),
            channel: channel.to_string(),
            message_id: Some(format!("slack:{channel}:{ts}")),
            thread_context: Some(ThreadContext {
                slack_thread_ts: thread_ts,
                ..Default::default()
            }),
        },
        channel_id: channel.to_string(),
        cursor_value: ts.to_string(),
        cursor_type: "message_ts",
    })
}

fn discord_id_cmp(left: &str, right: &str) -> std::cmp::Ordering {
    match (left.parse::<u64>(), right.parse::<u64>()) {
        (Ok(left), Ok(right)) => left.cmp(&right),
        _ => left.cmp(right),
    }
}

fn parse_discord_replay_envelope(
    channel_id: &str,
    msg: &serde_json::Value,
) -> Option<ReplayEnvelope> {
    let id = msg.get("id").and_then(|v| v.as_str()).unwrap_or("");
    let content = msg.get("content").and_then(|v| v.as_str()).unwrap_or("");
    let author = msg.get("author");
    let is_bot = author
        .and_then(|a| a.get("bot").and_then(|v| v.as_bool()))
        .unwrap_or(false);
    let username = author
        .and_then(|a| a.get("username").and_then(|v| v.as_str()))
        .unwrap_or("unknown");

    if id.is_empty() || content.is_empty() || is_bot {
        return None;
    }

    Some(ReplayEnvelope {
        message: IncomingMessage {
            platform: "Discord".into(),
            sender: username.into(),
            content: content.into(),
            channel: channel_id.to_string(),
            message_id: Some(format!("discord:{id}")),
            thread_context: Some(ThreadContext {
                discord_message_id: Some(id.to_string()),
                ..Default::default()
            }),
        },
        channel_id: channel_id.to_string(),
        cursor_value: id.to_string(),
        cursor_type: "message_id",
    })
}

pub async fn fetch_telegram_replay(state: &GatewayState) -> Result<ReplayFetchResult> {
    fetch_telegram_replay_from_base(state, "https://api.telegram.org").await
}

async fn fetch_telegram_replay_from_base(
    state: &GatewayState,
    base_url: &str,
) -> Result<ReplayFetchResult> {
    let token = &state.config.telegram_token;
    if token.is_empty() {
        return Ok(ReplayFetchResult::Replay(Vec::new()));
    }

    let Some(cursor) = state.telegram_replay_cursor else {
        let url = format!("{base_url}/bot{token}/getUpdates?offset=0&timeout=1&limit=100");
        let resp = state
            .http_client
            .get(&url)
            .timeout(std::time::Duration::from_secs(5))
            .send()
            .await
            .context("telegram replay: HTTP request failed")?;
        let body: serde_json::Value = resp
            .json()
            .await
            .context("telegram replay: failed to parse JSON response")?;
        let newest = body
            .get("result")
            .and_then(|v| v.as_array())
            .and_then(|results| {
                results
                    .iter()
                    .filter_map(|update| update.get("update_id").and_then(|v| v.as_i64()))
                    .max()
            });
        return Ok(match newest {
            Some(update_id) => ReplayFetchResult::InitializeBoundary {
                channel_id: "global".into(),
                cursor_value: update_id.to_string(),
                cursor_type: "update_id",
            },
            None => ReplayFetchResult::Replay(Vec::new()),
        });
    };

    let mut offset = cursor + 1;
    let mut replay = Vec::new();

    loop {
        let url = format!("{base_url}/bot{token}/getUpdates?offset={offset}&timeout=1&limit=100");
        let resp = state
            .http_client
            .get(&url)
            .timeout(std::time::Duration::from_secs(5))
            .send()
            .await
            .context("telegram replay: HTTP request failed")?;
        let body: serde_json::Value = resp
            .json()
            .await
            .context("telegram replay: failed to parse JSON response")?;
        let Some(results) = body.get("result").and_then(|v| v.as_array()) else {
            break;
        };
        if results.is_empty() {
            break;
        }

        for update in results {
            let update_id = update
                .get("update_id")
                .and_then(|v| v.as_i64())
                .unwrap_or_default();
            if update_id >= offset {
                offset = update_id + 1;
            }
            if update_id <= cursor {
                continue;
            }
            if let Some(envelope) = parse_telegram_replay_envelope(update) {
                replay.push(envelope);
            }
        }
    }

    Ok(ReplayFetchResult::Replay(replay))
}

pub async fn fetch_slack_replay(
    state: &GatewayState,
    channel_id: &str,
) -> Result<ReplayFetchResult> {
    fetch_slack_replay_from_base(state, channel_id, "https://slack.com/api").await
}

async fn fetch_slack_replay_from_base(
    state: &GatewayState,
    channel_id: &str,
    base_url: &str,
) -> Result<ReplayFetchResult> {
    let token = &state.config.slack_token;
    if token.is_empty() {
        return Ok(ReplayFetchResult::Replay(Vec::new()));
    }

    let Some(cursor) = state.slack_replay_cursors.get(channel_id).cloned() else {
        let url = format!("{base_url}/conversations.history?channel={channel_id}&limit=100");
        let resp = state
            .http_client
            .get(&url)
            .bearer_auth(token)
            .timeout(std::time::Duration::from_secs(5))
            .send()
            .await
            .with_context(|| {
                format!("slack replay: HTTP request failed for channel {channel_id}")
            })?;
        let body: serde_json::Value = resp.json().await.with_context(|| {
            format!("slack replay: failed to parse JSON for channel {channel_id}")
        })?;
        if body.get("ok").and_then(|v| v.as_bool()) != Some(true) {
            let error_msg = body
                .get("error")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown error");
            anyhow::bail!("slack replay API error for channel {channel_id}: {error_msg}");
        }
        let newest = body
            .get("messages")
            .and_then(|v| v.as_array())
            .and_then(|msgs| {
                msgs.iter()
                    .find_map(|msg| msg.get("ts").and_then(|v| v.as_str()))
            });
        return Ok(match newest {
            Some(ts) if !ts.is_empty() => ReplayFetchResult::InitializeBoundary {
                channel_id: channel_id.to_string(),
                cursor_value: ts.to_string(),
                cursor_type: "message_ts",
            },
            _ => ReplayFetchResult::Replay(Vec::new()),
        });
    };

    let mut latest = None::<String>;
    let mut replay = Vec::new();

    loop {
        let mut url = format!(
            "{base_url}/conversations.history?channel={channel_id}&limit=100&oldest={cursor}&inclusive=false"
        );
        if let Some(latest) = &latest {
            url.push_str(&format!("&latest={latest}"));
        }

        let resp = state
            .http_client
            .get(&url)
            .bearer_auth(token)
            .timeout(std::time::Duration::from_secs(5))
            .send()
            .await
            .with_context(|| {
                format!("slack replay: HTTP request failed for channel {channel_id}")
            })?;
        let body: serde_json::Value = resp.json().await.with_context(|| {
            format!("slack replay: failed to parse JSON for channel {channel_id}")
        })?;
        if body.get("ok").and_then(|v| v.as_bool()) != Some(true) {
            let error_msg = body
                .get("error")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown error");
            anyhow::bail!("slack replay API error for channel {channel_id}: {error_msg}");
        }

        let has_more = body
            .get("has_more")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let Some(msgs) = body.get("messages").and_then(|v| v.as_array()) else {
            break;
        };
        if msgs.is_empty() {
            break;
        }

        let mut oldest_visible = None::<String>;
        let mut saw_newer = false;
        for msg in msgs {
            let ts = msg.get("ts").and_then(|v| v.as_str()).unwrap_or("");
            if ts.is_empty() {
                continue;
            }
            oldest_visible = Some(ts.to_string());
            if ts <= cursor.as_str() {
                continue;
            }
            saw_newer = true;
            if let Some(envelope) = parse_slack_replay_envelope(channel_id, msg) {
                replay.push(envelope);
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

    replay.sort_by(|left, right| left.cursor_value.cmp(&right.cursor_value));
    Ok(ReplayFetchResult::Replay(replay))
}

pub async fn fetch_discord_replay(
    state: &GatewayState,
    channel_id: &str,
) -> Result<ReplayFetchResult> {
    fetch_discord_replay_from_base(state, channel_id, "https://discord.com/api/v10").await
}

async fn fetch_discord_replay_from_base(
    state: &GatewayState,
    channel_id: &str,
    base_url: &str,
) -> Result<ReplayFetchResult> {
    let token = &state.config.discord_token;
    if token.is_empty() {
        return Ok(ReplayFetchResult::Replay(Vec::new()));
    }

    let auth_header = discord_authorization_header(token);
    if auth_header.is_empty() {
        return Ok(ReplayFetchResult::Replay(Vec::new()));
    }

    let Some(cursor) = state.discord_replay_cursors.get(channel_id).cloned() else {
        let url = format!("{base_url}/channels/{channel_id}/messages?limit=100");
        let resp = state
            .http_client
            .get(&url)
            .header("Authorization", &auth_header)
            .timeout(std::time::Duration::from_secs(5))
            .send()
            .await
            .with_context(|| {
                format!("discord replay: HTTP request failed for channel {channel_id}")
            })?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!(
                "discord replay rejected for channel {channel_id}: status={status}, body={body}"
            );
        }
        let body: serde_json::Value = resp.json().await.with_context(|| {
            format!("discord replay: failed to parse JSON for channel {channel_id}")
        })?;
        let newest = body.as_array().and_then(|msgs| {
            msgs.iter()
                .find_map(|msg| msg.get("id").and_then(|v| v.as_str()))
        });
        return Ok(match newest {
            Some(id) if !id.is_empty() => ReplayFetchResult::InitializeBoundary {
                channel_id: channel_id.to_string(),
                cursor_value: id.to_string(),
                cursor_type: "message_id",
            },
            _ => ReplayFetchResult::Replay(Vec::new()),
        });
    };

    let mut before = None::<String>;
    let mut replay = Vec::new();

    loop {
        let mut url = format!("{base_url}/channels/{channel_id}/messages?limit=100");
        if let Some(before) = &before {
            url.push_str(&format!("&before={before}"));
        }

        let resp = state
            .http_client
            .get(&url)
            .header("Authorization", &auth_header)
            .timeout(std::time::Duration::from_secs(5))
            .send()
            .await
            .with_context(|| {
                format!("discord replay: HTTP request failed for channel {channel_id}")
            })?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!(
                "discord replay rejected for channel {channel_id}: status={status}, body={body}"
            );
        }
        let body: serde_json::Value = resp.json().await.with_context(|| {
            format!("discord replay: failed to parse JSON for channel {channel_id}")
        })?;
        let Some(msgs) = body.as_array() else {
            break;
        };
        if msgs.is_empty() {
            break;
        }

        let mut oldest_visible = None::<String>;
        let mut reached_cursor = false;
        let mut saw_newer = false;
        for msg in msgs {
            let id = msg.get("id").and_then(|v| v.as_str()).unwrap_or("");
            if id.is_empty() {
                continue;
            }
            oldest_visible = Some(id.to_string());
            if discord_id_cmp(id, &cursor).is_le() {
                reached_cursor = true;
                continue;
            }
            saw_newer = true;
            if let Some(envelope) = parse_discord_replay_envelope(channel_id, msg) {
                replay.push(envelope);
            }
        }

        if reached_cursor || !saw_newer {
            break;
        }

        let Some(next_before) = oldest_visible else {
            break;
        };
        before = Some(next_before);
    }

    replay.sort_by(|left, right| discord_id_cmp(&left.cursor_value, &right.cursor_value));
    Ok(ReplayFetchResult::Replay(replay))
}

// ---------------------------------------------------------------------------
// Telegram: long-poll getUpdates
// ---------------------------------------------------------------------------

pub async fn poll_telegram(state: &mut GatewayState) -> Result<Vec<IncomingMessage>> {
    let token = &state.config.telegram_token;
    if token.is_empty() {
        return Ok(vec![]);
    }

    let url = format!(
        "https://api.telegram.org/bot{token}/getUpdates?offset={}&timeout=1&limit=10",
        state.telegram_offset
    );

    let resp = state
        .http_client
        .get(&url)
        .timeout(std::time::Duration::from_secs(5))
        .send()
        .await
        .context("telegram poll: HTTP request failed")?;

    let body: serde_json::Value = resp
        .json()
        .await
        .context("telegram poll: failed to parse JSON response")?;

    let mut messages = Vec::new();
    let is_first_poll = state.telegram_offset == 0;

    let mut next_offset = state.telegram_offset;

    if let Some(results) = body.get("result").and_then(|v| v.as_array()) {
        for update in results {
            let update_id = update
                .get("update_id")
                .and_then(|v| v.as_i64())
                .unwrap_or(0);
            if update_id >= next_offset {
                next_offset = update_id + 1;
            }

            // Skip historical messages on first poll
            if is_first_poll {
                continue;
            }

            let msg = update.get("message").or_else(|| update.get("channel_post"));
            if let Some(msg) = msg {
                let text = msg.get("text").and_then(|v| v.as_str()).unwrap_or("");
                if text.is_empty() {
                    continue;
                }

                let from = msg.get("from");
                let sender = from
                    .and_then(|f| f.get("username").and_then(|v| v.as_str()))
                    .or_else(|| from.and_then(|f| f.get("first_name").and_then(|v| v.as_str())))
                    .unwrap_or("unknown")
                    .to_string();

                let chat_id = msg
                    .get("chat")
                    .and_then(|c| c.get("id"))
                    .and_then(|v| v.as_i64())
                    .map(|id| id.to_string())
                    .unwrap_or_default();

                let tg_msg_id = msg
                    .get("message_id")
                    .and_then(|v| v.as_i64())
                    .map(|id| format!("tg:{id}"));

                // Extract thread context: message_id for reply-to, and check
                // reply_to_message for existing thread context
                let telegram_message_id = msg.get("message_id").and_then(|v| v.as_i64());
                let reply_msg_id = msg
                    .get("reply_to_message")
                    .and_then(|r| r.get("message_id"))
                    .and_then(|v| v.as_i64());

                let thread_context = Some(ThreadContext {
                    telegram_message_id: telegram_message_id.or(reply_msg_id),
                    ..Default::default()
                });

                messages.push(IncomingMessage {
                    platform: "Telegram".into(),
                    sender,
                    content: text.into(),
                    channel: chat_id,
                    message_id: tg_msg_id,
                    thread_context,
                });
            }
        }
    }

    state.telegram_offset = next_offset;

    Ok(messages)
}

// ---------------------------------------------------------------------------
// Slack: poll conversations.history
// ---------------------------------------------------------------------------

pub async fn poll_slack(
    state: &mut GatewayState,
    channels: &[String],
) -> Result<Vec<IncomingMessage>> {
    let token = &state.config.slack_token;
    if token.is_empty() {
        return Ok(vec![]);
    }

    let mut messages = Vec::new();

    for channel in channels {
        let oldest = state
            .slack_last_ts
            .get(channel)
            .cloned()
            .unwrap_or_default();

        let mut url =
            format!("https://slack.com/api/conversations.history?channel={channel}&limit=5");
        if !oldest.is_empty() {
            url.push_str(&format!("&oldest={oldest}"));
        }

        let resp = state
            .http_client
            .get(&url)
            .bearer_auth(token)
            .timeout(std::time::Duration::from_secs(5))
            .send()
            .await
            .with_context(|| format!("slack poll: HTTP request failed for channel {channel}"))?;

        let body: serde_json::Value = resp
            .json()
            .await
            .with_context(|| format!("slack poll: failed to parse JSON for channel {channel}"))?;

        if body.get("ok").and_then(|v| v.as_bool()) != Some(true) {
            let error_msg = body
                .get("error")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown error");
            anyhow::bail!("slack API error for channel {channel}: {error_msg}");
        }

        let mut next_oldest = oldest.clone();

        if let Some(msgs) = body.get("messages").and_then(|v| v.as_array()) {
            for msg in msgs {
                let ts = msg.get("ts").and_then(|v| v.as_str()).unwrap_or("");
                let text = msg.get("text").and_then(|v| v.as_str()).unwrap_or("");
                let user = msg
                    .get("user")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");
                let subtype = msg.get("subtype").and_then(|v| v.as_str());

                // Skip bot messages and empty
                if text.is_empty() || subtype == Some("bot_message") {
                    continue;
                }

                // Skip if we've already seen this timestamp
                if !oldest.is_empty() && ts <= oldest.as_str() {
                    continue;
                }

                if !ts.is_empty() {
                    if next_oldest.is_empty() || ts > next_oldest.as_str() {
                        next_oldest = ts.to_string();
                    }
                }

                // Extract thread context: thread_ts if in a thread, otherwise ts
                // for potential thread start reference.
                let thread_ts = msg
                    .get("thread_ts")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
                    .or_else(|| {
                        if !ts.is_empty() {
                            Some(ts.to_string())
                        } else {
                            None
                        }
                    });

                let thread_context = Some(ThreadContext {
                    slack_thread_ts: thread_ts,
                    ..Default::default()
                });

                messages.push(IncomingMessage {
                    platform: "Slack".into(),
                    sender: user.into(),
                    content: text.into(),
                    channel: channel.clone(),
                    message_id: if ts.is_empty() {
                        None
                    } else {
                        Some(format!("slack:{channel}:{ts}"))
                    },
                    thread_context,
                });
            }
        }

        if !next_oldest.is_empty() {
            state.slack_last_ts.insert(channel.clone(), next_oldest);
        }
    }

    Ok(messages)
}

// ---------------------------------------------------------------------------
// Discord: poll channel messages REST API
// ---------------------------------------------------------------------------

pub async fn poll_discord(
    state: &mut GatewayState,
    channel_ids: &[String],
) -> Result<Vec<IncomingMessage>> {
    let token = &state.config.discord_token;
    if token.is_empty() {
        return Ok(vec![]);
    }

    let auth_header = discord_authorization_header(token);
    if auth_header.is_empty() {
        return Ok(vec![]);
    }

    let mut messages = Vec::new();

    for channel_id in channel_ids {
        let had_last_id = state.discord_last_id.contains_key(channel_id);

        let mut url = format!("https://discord.com/api/v10/channels/{channel_id}/messages?limit=5");

        if let Some(after) = state.discord_last_id.get(channel_id) {
            url.push_str(&format!("&after={after}"));
        }

        let resp = state
            .http_client
            .get(&url)
            .header("Authorization", &auth_header)
            .timeout(std::time::Duration::from_secs(5))
            .send()
            .await
            .with_context(|| {
                format!("discord poll: HTTP request failed for channel {channel_id}")
            })?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!(
                "discord poll rejected for channel {channel_id}: status={status}, body={body}"
            );
        }

        let body: serde_json::Value = resp.json().await.with_context(|| {
            format!("discord poll: failed to parse JSON for channel {channel_id}")
        })?;

        let msgs = match body.as_array() {
            Some(a) => a,
            None => continue,
        };

        // Track the newest message ID (for ALL messages including bot) so we
        // never re-fetch. Discord returns newest first.
        let newest_id = msgs
            .first()
            .and_then(|m| m.get("id").and_then(|v| v.as_str()))
            .unwrap_or("");

        // On first poll (no prior last_id), skip historical messages —
        // only process messages from subsequent polls
        if !had_last_id {
            if !newest_id.is_empty() {
                state
                    .discord_last_id
                    .insert(channel_id.clone(), newest_id.to_string());
            }
            tracing::info!(
                channel = %channel_id,
                skipped = msgs.len(),
                newest_id = %newest_id,
                "discord: initialized, skipping historical messages"
            );
            continue;
        }

        // Process messages oldest first (reverse since Discord returns newest first)
        for msg in msgs.iter().rev() {
            let content = msg.get("content").and_then(|v| v.as_str()).unwrap_or("");
            let author = msg.get("author");
            let is_bot = author
                .and_then(|a| a.get("bot").and_then(|v| v.as_bool()))
                .unwrap_or(false);
            let username = author
                .and_then(|a| a.get("username").and_then(|v| v.as_str()))
                .unwrap_or("unknown");

            // Skip bot messages (our own replies) and empty
            if content.is_empty() || is_bot {
                continue;
            }

            let discord_msg_id = msg
                .get("id")
                .and_then(|v| v.as_str())
                .map(|id| format!("discord:{id}"));

            // Extract thread context: Discord message ID for reply reference
            let discord_reply_id = msg
                .get("id")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());

            let thread_context = Some(ThreadContext {
                discord_message_id: discord_reply_id,
                ..Default::default()
            });

            messages.push(IncomingMessage {
                platform: "Discord".into(),
                sender: username.into(),
                content: content.into(),
                channel: channel_id.clone(),
                message_id: discord_msg_id,
                thread_context,
            });
        }

        if !newest_id.is_empty() {
            state
                .discord_last_id
                .insert(channel_id.clone(), newest_id.to_string());
        }
    }

    Ok(messages)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;
    use std::sync::{Arc, Mutex};

    use anyhow::Result;
    use serde_json::json;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    struct TestHttpServer {
        base_url: String,
        requests: Arc<Mutex<Vec<String>>>,
    }

    impl TestHttpServer {
        async fn spawn(responses: Vec<String>) -> Result<Self> {
            let listener = TcpListener::bind("127.0.0.1:0").await?;
            let addr = listener.local_addr()?;
            let responses = Arc::new(Mutex::new(VecDeque::from(responses)));
            let requests = Arc::new(Mutex::new(Vec::new()));
            let requests_task = Arc::clone(&requests);
            let responses_task = Arc::clone(&responses);

            tokio::spawn(async move {
                while let Ok((mut socket, _)) = listener.accept().await {
                    let mut buffer = vec![0_u8; 8192];
                    let read = match socket.read(&mut buffer).await {
                        Ok(read) => read,
                        Err(_) => continue,
                    };
                    if read == 0 {
                        continue;
                    }

                    let request = String::from_utf8_lossy(&buffer[..read]);
                    if let Some(path) = request
                        .lines()
                        .next()
                        .and_then(|line| line.split_whitespace().nth(1))
                    {
                        requests_task.lock().unwrap().push(path.to_string());
                    }

                    let body = responses_task
                        .lock()
                        .unwrap()
                        .pop_front()
                        .unwrap_or_else(|| "{}".to_string());
                    let response = format!(
                        "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                        body.len(),
                        body
                    );
                    let _ = socket.write_all(response.as_bytes()).await;
                }
            });

            Ok(Self {
                base_url: format!("http://{addr}"),
                requests,
            })
        }

        fn requests(&self) -> Vec<String> {
            self.requests.lock().unwrap().clone()
        }
    }

    fn test_gateway_config() -> GatewayConfig {
        GatewayConfig {
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

    fn test_client() -> reqwest::Client {
        reqwest::Client::builder()
            .pool_max_idle_per_host(0)
            .build()
            .unwrap()
    }

    #[tokio::test]
    async fn telegram_replay_fetches_only_updates_after_cursor() -> Result<()> {
        let server = TestHttpServer::spawn(vec![
            json!({
                "ok": true,
                "result": [
                    {
                        "update_id": 101,
                        "message": {
                            "message_id": 11,
                            "text": "first",
                            "chat": { "id": 777 },
                            "from": { "username": "alice" }
                        }
                    },
                    {
                        "update_id": 103,
                        "message": {
                            "message_id": 13,
                            "text": "second",
                            "chat": { "id": 777 },
                            "from": { "username": "bob" }
                        }
                    }
                ]
            })
            .to_string(),
            json!({
                "ok": true,
                "result": [
                    {
                        "update_id": 104,
                        "message": {
                            "message_id": 14,
                            "text": "third",
                            "chat": { "id": 777 },
                            "from": { "username": "carol" }
                        }
                    }
                ]
            })
            .to_string(),
            json!({ "ok": true, "result": [] }).to_string(),
        ])
        .await?;

        let mut state = GatewayState::new(test_gateway_config(), test_client());
        state.config.telegram_token = "telegram-token".into();
        state.telegram_replay_cursor = Some(100);

        let result = fetch_telegram_replay_from_base(&state, &server.base_url).await?;

        let replay = match result {
            ReplayFetchResult::Replay(replay) => replay,
            ReplayFetchResult::InitializeBoundary { .. } => {
                panic!("expected replay result");
            }
        };

        assert_eq!(replay.len(), 3);
        assert_eq!(
            replay
                .iter()
                .map(|item| item.cursor_value.as_str())
                .collect::<Vec<_>>(),
            vec!["101", "103", "104"]
        );
        assert!(replay
            .iter()
            .all(|item| item.channel_id == "global" && item.cursor_type == "update_id"));
        assert_eq!(
            replay
                .iter()
                .map(|item| item.message.content.as_str())
                .collect::<Vec<_>>(),
            vec!["first", "second", "third"]
        );
        assert_eq!(
            server.requests(),
            vec![
                "/bottelegram-token/getUpdates?offset=101&timeout=1&limit=100",
                "/bottelegram-token/getUpdates?offset=104&timeout=1&limit=100",
                "/bottelegram-token/getUpdates?offset=105&timeout=1&limit=100",
            ]
        );

        Ok(())
    }

    #[tokio::test]
    async fn slack_replay_fetches_only_messages_newer_than_channel_ts() -> Result<()> {
        let server = TestHttpServer::spawn(vec![
            json!({
                "ok": true,
                "has_more": true,
                "messages": [
                    { "ts": "1712345678.000300", "text": "latest", "user": "U300" },
                    { "ts": "1712345678.000200", "text": "middle", "user": "U200" }
                ]
            })
            .to_string(),
            json!({
                "ok": true,
                "has_more": false,
                "messages": [
                    { "ts": "1712345678.000150", "text": "earliest", "user": "U150" },
                    { "ts": "1712345678.000100", "text": "cursor", "user": "U100" }
                ]
            })
            .to_string(),
        ])
        .await?;

        let mut state = GatewayState::new(test_gateway_config(), test_client());
        state.config.slack_token = "slack-token".into();
        state
            .slack_replay_cursors
            .insert("C123".into(), "1712345678.000100".into());
        let slack_base_url = format!("{}/api", server.base_url);

        let result = fetch_slack_replay_from_base(&state, "C123", &slack_base_url).await?;

        let replay = match result {
            ReplayFetchResult::Replay(replay) => replay,
            ReplayFetchResult::InitializeBoundary { .. } => {
                panic!("expected replay result");
            }
        };

        assert_eq!(replay.len(), 3);
        assert_eq!(
            replay
                .iter()
                .map(|item| item.cursor_value.as_str())
                .collect::<Vec<_>>(),
            vec![
                "1712345678.000150",
                "1712345678.000200",
                "1712345678.000300"
            ]
        );
        assert!(replay
            .iter()
            .all(|item| item.channel_id == "C123" && item.cursor_type == "message_ts"));
        assert_eq!(
            replay
                .iter()
                .map(|item| item.message.content.as_str())
                .collect::<Vec<_>>(),
            vec!["earliest", "middle", "latest"]
        );
        assert_eq!(
            server.requests(),
            vec![
                "/api/conversations.history?channel=C123&limit=100&oldest=1712345678.000100&inclusive=false",
                "/api/conversations.history?channel=C123&limit=100&oldest=1712345678.000100&inclusive=false&latest=1712345678.000200",
            ]
        );

        Ok(())
    }

    #[tokio::test]
    async fn discord_replay_reverses_api_order_to_oldest_first() -> Result<()> {
        let server = TestHttpServer::spawn(vec![
            json!([
                {
                    "id": "130",
                    "content": "latest",
                    "author": { "username": "latest-user", "bot": false }
                },
                {
                    "id": "120",
                    "content": "middle",
                    "author": { "username": "middle-user", "bot": false }
                }
            ])
            .to_string(),
            json!([
                {
                    "id": "110",
                    "content": "earliest",
                    "author": { "username": "earliest-user", "bot": false }
                },
                {
                    "id": "100",
                    "content": "cursor",
                    "author": { "username": "cursor-user", "bot": false }
                }
            ])
            .to_string(),
        ])
        .await?;

        let mut state = GatewayState::new(test_gateway_config(), test_client());
        state.config.discord_token = "discord-token".into();
        state
            .discord_replay_cursors
            .insert("D123".into(), "100".into());
        let discord_base_url = format!("{}/api/v10", server.base_url);

        let result = fetch_discord_replay_from_base(&state, "D123", &discord_base_url).await?;

        let replay = match result {
            ReplayFetchResult::Replay(replay) => replay,
            ReplayFetchResult::InitializeBoundary { .. } => {
                panic!("expected replay result");
            }
        };

        assert_eq!(replay.len(), 3);
        assert_eq!(
            replay
                .iter()
                .map(|item| item.cursor_value.as_str())
                .collect::<Vec<_>>(),
            vec!["110", "120", "130"]
        );
        assert!(replay
            .iter()
            .all(|item| item.channel_id == "D123" && item.cursor_type == "message_id"));
        assert_eq!(
            replay
                .iter()
                .map(|item| item.message.content.as_str())
                .collect::<Vec<_>>(),
            vec!["earliest", "middle", "latest"]
        );
        assert_eq!(
            server.requests(),
            vec![
                "/api/v10/channels/D123/messages?limit=100",
                "/api/v10/channels/D123/messages?limit=100&before=120",
            ]
        );

        Ok(())
    }
}
