//! Gateway: direct platform connections for receiving messages.
//!
//! Runs inside the daemon. No separate process, no Electron dependency.
//! - Telegram: long-poll getUpdates API
//! - Slack: poll conversations.history API
//! - Discord: poll channel messages REST API
//!
//! Incoming messages are routed to the agent engine for processing.

use std::collections::HashMap;

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

/// State for tracking already-seen messages per platform.
pub struct GatewayState {
    pub config: GatewayConfig,
    pub telegram_offset: i64,
    pub slack_last_ts: HashMap<String, String>,
    pub discord_last_id: HashMap<String, String>,
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

    if let Some(results) = body.get("result").and_then(|v| v.as_array()) {
        for update in results {
            let update_id = update
                .get("update_id")
                .and_then(|v| v.as_i64())
                .unwrap_or(0);
            if update_id >= state.telegram_offset {
                state.telegram_offset = update_id + 1;
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
                let telegram_message_id = msg
                    .get("message_id")
                    .and_then(|v| v.as_i64());
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

    Ok(messages)
}

// ---------------------------------------------------------------------------
// Slack: poll conversations.history
// ---------------------------------------------------------------------------

pub async fn poll_slack(state: &mut GatewayState, channels: &[String]) -> Result<Vec<IncomingMessage>> {
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
                    state
                        .slack_last_ts
                        .entry(channel.clone())
                        .and_modify(|v| {
                            if ts > v.as_str() {
                                *v = ts.to_string();
                            }
                        })
                        .or_insert_with(|| ts.to_string());
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
            .with_context(|| format!("discord poll: HTTP request failed for channel {channel_id}"))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!(
                "discord poll rejected for channel {channel_id}: status={status}, body={body}"
            );
        }

        let body: serde_json::Value = resp
            .json()
            .await
            .with_context(|| format!("discord poll: failed to parse JSON for channel {channel_id}"))?;

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
        if !newest_id.is_empty() {
            state
                .discord_last_id
                .insert(channel_id.clone(), newest_id.to_string());
        }

        // On first poll (no prior last_id), skip historical messages —
        // only process messages from subsequent polls
        if !had_last_id {
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
    }

    Ok(messages)
}
