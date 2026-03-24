//! Slack Bot API integration stub.
//!
//! Requires `AMUX_SLACK_TOKEN` environment variable to be set to a valid
//! Slack Bot token (xoxb-...). When the token is present the provider is
//! registered with the gateway; actual Slack API calls require a Slack client
//! crate (e.g. `slack-morphism` or a raw HTTP client) which is not included
//! in this scaffold.
//!
//! Extension points:
//! - Replace `connect()` with Slack WebSocket RTM or Events API setup.
//! - Replace `recv()` with real event deserialization from the Slack stream.
//! - Replace `send()` with `chat.postMessage` API calls.

use anyhow::{bail, Result};

use crate::router::{GatewayMessage, GatewayResponse};
use crate::GatewayProvider;

/// Slack gateway provider.
pub struct SlackProvider {
    token: String,
    connected: bool,
}

impl SlackProvider {
    /// Create a `SlackProvider` if `AMUX_SLACK_TOKEN` is set and non-empty.
    pub fn from_env() -> Option<Self> {
        let token = std::env::var("AMUX_SLACK_TOKEN").ok()?;
        if token.is_empty() {
            return None;
        }
        Some(Self {
            token,
            connected: false,
        })
    }
}

impl GatewayProvider for SlackProvider {
    fn name(&self) -> &str {
        "Slack"
    }

    #[allow(unused)]
    fn connect<'a>(
        &'a mut self,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send + 'a>> {
        Box::pin(async move {
            tracing::info!(
                "Slack provider: would connect using token {}…{}",
                &self.token[..6.min(self.token.len())],
                &self.token[self.token.len().saturating_sub(4)..],
            );
            // TODO: Establish a real Slack RTM/Events API WebSocket here.
            // For now, mark as connected but note the stub status.
            tracing::warn!(
                "Slack API client is not configured — \
                 install a Slack client crate and implement the WebSocket connection"
            );
            self.connected = true;
            Ok(())
        })
    }

    fn recv<'a>(
        &'a mut self,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<Option<GatewayMessage>>> + Send + 'a>,
    > {
        Box::pin(async move {
            if !self.connected {
                bail!("Slack provider not connected");
            }
            // TODO: Poll the Slack WebSocket for incoming message events and
            // deserialize them into GatewayMessage structs.
            //
            // Example message mapping:
            //   SlackEvent::Message { channel, user, text, ts } -> GatewayMessage {
            //       platform: "slack",
            //       channel_id: channel,
            //       user_id: user,
            //       text,
            //       timestamp: parse_slack_ts(ts),
            //   }
            Ok(None)
        })
    }

    fn send<'a>(
        &'a mut self,
        response: GatewayResponse,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send + 'a>> {
        Box::pin(async move {
            if !self.connected {
                bail!("Slack provider not connected");
            }
            tracing::info!(
                channel = %response.channel_id,
                text = %response.text,
                "Slack: would send message via chat.postMessage API"
            );
            // TODO: POST to https://slack.com/api/chat.postMessage with:
            //   { "channel": response.channel_id, "text": response.text }
            //   Authorization: Bearer {self.token}
            tracing::warn!("Slack send is a stub — message not actually delivered");
            Ok(())
        })
    }
}
