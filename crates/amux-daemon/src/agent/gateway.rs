//! Gateway runtime state shared by daemon supervision and event handling.
//!
//! `tamux-gateway` is the sole owner of Slack/Discord/Telegram transport I/O.
//! The daemon keeps only continuity state, reply-routing metadata, and the
//! normalized message types needed to process inbound IPC events.

use std::collections::{HashMap, HashSet};

use amux_protocol::{
    AGENT_HANDLE_CONCIERGE, AGENT_HANDLE_MAIN, AGENT_HANDLE_RAROG, AGENT_HANDLE_SVAROG,
    AGENT_HANDLE_SWAROG_LEGACY, AGENT_ID_RAROG, AGENT_ID_SWAROG,
};

use super::gateway_health::PlatformHealthState;
use super::types::GatewayConfig;

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
            Self::Rarog => AGENT_ID_RAROG,
            Self::Swarog => AGENT_ID_SWAROG,
        }
    }

    pub fn parse(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            AGENT_HANDLE_SVAROG | AGENT_HANDLE_SWAROG_LEGACY | AGENT_HANDLE_MAIN => Self::Swarog,
            AGENT_HANDLE_RAROG | AGENT_HANDLE_CONCIERGE => Self::Rarog,
            _ => Self::Rarog,
        }
    }
}

/// State retained by the daemon for gateway continuity and reply routing.
pub struct GatewayState {
    pub config: GatewayConfig,
    pub telegram_replay_cursor: Option<i64>,
    pub slack_replay_cursors: HashMap<String, String>,
    pub discord_replay_cursors: HashMap<String, String>,
    pub whatsapp_replay_cursors: HashMap<String, String>,
    pub replay_cycle_active: HashSet<String>,
    pub slack_health: PlatformHealthState,
    pub discord_health: PlatformHealthState,
    pub telegram_health: PlatformHealthState,
    /// Per-channel thread context for auto-injecting reply metadata.
    /// Key: "Platform:channel_id", Value: most recent ThreadContext.
    pub reply_contexts: HashMap<String, ThreadContext>,
    /// Discord DM alias resolution for user-targeted sends.
    /// Key: "user:<discord_user_id>", Value: canonical DM channel id.
    pub discord_dm_channels_by_user: HashMap<String, String>,
    /// Per-channel timestamp of last outgoing agent response (epoch millis).
    /// Key: "Platform:channel_id". Populated by send tools after successful sends.
    pub last_response_at: HashMap<String, u64>,
    /// Per-channel content of the last successfully delivered outgoing response.
    /// Key: "Platform:channel_id". Used to suppress duplicate sequential sends.
    pub last_response_content: HashMap<String, String>,
    /// Per-channel timestamp of last incoming message (epoch millis).
    /// Key: "Platform:channel_id". Populated by inbound IPC event handling.
    pub last_incoming_at: HashMap<String, u64>,
}

impl GatewayState {
    pub fn new(config: GatewayConfig, _http_client: reqwest::Client) -> Self {
        Self {
            config,
            telegram_replay_cursor: None,
            slack_replay_cursors: HashMap::new(),
            discord_replay_cursors: HashMap::new(),
            whatsapp_replay_cursors: HashMap::new(),
            replay_cycle_active: HashSet::new(),
            slack_health: PlatformHealthState::new(),
            discord_health: PlatformHealthState::new(),
            telegram_health: PlatformHealthState::new(),
            reply_contexts: HashMap::new(),
            discord_dm_channels_by_user: HashMap::new(),
            last_response_at: HashMap::new(),
            last_response_content: HashMap::new(),
            last_incoming_at: HashMap::new(),
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
