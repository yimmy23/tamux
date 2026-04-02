//! Gateway message normalization.
//!
//! Platform adapters should normalize inbound provider payloads here and pass
//! the resulting messages upstream. Command interpretation belongs in the
//! daemon, not in `tamux-gateway`.

/// An inbound message received from a chat platform.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GatewayMessage {
    /// Platform name (for example `slack`, `discord`, `telegram`).
    pub platform: String,
    /// Platform-specific channel identifier.
    pub channel_id: String,
    /// Platform-specific user identifier.
    pub user_id: String,
    /// Optional provider-specific display name for the sender.
    pub sender_display: Option<String>,
    /// Normalized message text.
    pub text: String,
    /// Provider-specific message identifier when available.
    pub message_id: Option<String>,
    /// Provider-specific reply/thread reference when available.
    pub thread_id: Option<String>,
    /// Unix timestamp (seconds since epoch).
    pub timestamp: u64,
    /// Raw provider payload when the adapter wants to retain it.
    pub raw_event_json: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawGatewayMessage<'a> {
    pub platform: &'a str,
    pub channel_id: &'a str,
    pub user_id: &'a str,
    pub sender_display: Option<String>,
    pub text: &'a str,
    pub message_id: Option<String>,
    pub thread_id: Option<String>,
    pub timestamp: u64,
    pub raw_event_json: Option<String>,
}

/// Normalize a provider payload into a daemon-facing gateway message.
pub fn normalize_message(raw: RawGatewayMessage<'_>) -> Option<GatewayMessage> {
    let platform = raw.platform.trim().to_ascii_lowercase();
    let channel_id = raw.channel_id.trim().to_string();
    let user_id = raw.user_id.trim().to_string();
    let text = raw.text.trim().to_string();

    if platform.is_empty() || channel_id.is_empty() || user_id.is_empty() || text.is_empty() {
        return None;
    }

    Some(GatewayMessage {
        platform,
        channel_id,
        user_id,
        sender_display: raw.sender_display.map(|value| value.trim().to_string()),
        text,
        message_id: raw.message_id,
        thread_id: raw.thread_id,
        timestamp: raw.timestamp,
        raw_event_json: raw.raw_event_json,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gateway_router_normalizes_messages_without_daemon_command_routing() {
        let normalized = normalize_message(RawGatewayMessage {
            platform: "slack",
            channel_id: "C123",
            user_id: "U123",
            sender_display: Some("Alice".to_string()),
            text: "  !deploy now  ",
            message_id: Some("m-1".to_string()),
            thread_id: Some("t-1".to_string()),
            timestamp: 1_700_000_000,
            raw_event_json: Some("{\"ok\":true}".to_string()),
        })
        .expect("message should normalize");

        assert_eq!(normalized.platform, "slack");
        assert_eq!(normalized.channel_id, "C123");
        assert_eq!(normalized.user_id, "U123");
        assert_eq!(normalized.sender_display.as_deref(), Some("Alice"));
        assert_eq!(normalized.text, "!deploy now");
        assert_eq!(normalized.message_id.as_deref(), Some("m-1"));
        assert_eq!(normalized.thread_id.as_deref(), Some("t-1"));
        assert_eq!(normalized.timestamp, 1_700_000_000);
        assert_eq!(normalized.raw_event_json.as_deref(), Some("{\"ok\":true}"));
    }

    #[test]
    fn gateway_router_rejects_empty_content() {
        assert!(normalize_message(RawGatewayMessage {
            platform: "telegram",
            channel_id: "777",
            user_id: "alice",
            sender_display: None,
            text: "   ",
            message_id: None,
            thread_id: None,
            timestamp: 1,
            raw_event_json: None,
        })
        .is_none());
    }
}
