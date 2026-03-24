//! Discord Bot integration stub.
//!
//! Requires `AMUX_DISCORD_TOKEN` environment variable to be set to a valid
//! Discord Bot token. When the token is present the provider is registered
//! with the gateway; actual Discord API calls require a Discord client crate
//! (e.g. `serenity` or `twilight`) which is not included in this scaffold.
//!
//! Extension points:
//! - Replace `connect()` with Discord Gateway WebSocket handshake + IDENTIFY.
//! - Replace `recv()` with MESSAGE_CREATE event deserialization.
//! - Replace `send()` with `POST /channels/{id}/messages` REST call.

use anyhow::{bail, Result};

use crate::router::{GatewayMessage, GatewayResponse};
use crate::GatewayProvider;

/// Discord gateway provider.
pub struct DiscordProvider {
    token: String,
    connected: bool,
}

impl DiscordProvider {
    /// Create a `DiscordProvider` if `AMUX_DISCORD_TOKEN` is set and non-empty.
    pub fn from_env() -> Option<Self> {
        let token = std::env::var("AMUX_DISCORD_TOKEN").ok()?;
        if token.is_empty() {
            return None;
        }
        Some(Self {
            token,
            connected: false,
        })
    }
}

impl GatewayProvider for DiscordProvider {
    fn name(&self) -> &str {
        "Discord"
    }

    fn connect<'a>(
        &'a mut self,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send + 'a>> {
        Box::pin(async move {
            tracing::info!(
                "Discord provider: would connect to Gateway WebSocket (token: {}…)",
                &self.token[..8.min(self.token.len())],
            );
            // TODO: Implement Discord Gateway connection:
            // 1. GET /gateway/bot to obtain WebSocket URL.
            // 2. Connect via WebSocket, receive Hello (opcode 10).
            // 3. Send Identify (opcode 2) with token and intents.
            // 4. Begin heartbeat loop.
            tracing::warn!(
                "Discord API client is not configured — \
                 install a Discord client crate and implement the Gateway connection"
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
                bail!("Discord provider not connected");
            }
            // TODO: Read from the Discord Gateway WebSocket and match on
            // MESSAGE_CREATE dispatch events:
            //
            //   GatewayMessage {
            //       platform: "discord",
            //       channel_id: message.channel_id,
            //       user_id: message.author.id,
            //       text: message.content,
            //       timestamp: parse_discord_timestamp(message.timestamp),
            //   }
            //
            // Filter out messages from bots (message.author.bot == true).
            Ok(None)
        })
    }

    fn send<'a>(
        &'a mut self,
        response: GatewayResponse,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send + 'a>> {
        Box::pin(async move {
            if !self.connected {
                bail!("Discord provider not connected");
            }
            tracing::info!(
                channel = %response.channel_id,
                text = %response.text,
                "Discord: would send message via Create Message API"
            );
            // TODO: POST https://discord.com/api/v10/channels/{channel_id}/messages
            //   { "content": response.text }
            //   Authorization: Bot {self.token}
            tracing::warn!("Discord send is a stub — message not actually delivered");
            Ok(())
        })
    }
}
