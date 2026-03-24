//! Telegram Bot API integration stub.
//!
//! Requires `AMUX_TELEGRAM_TOKEN` environment variable to be set to a valid
//! Telegram Bot token (obtained from @BotFather). When the token is present
//! the provider is registered with the gateway; actual Telegram API calls
//! require an HTTP client (e.g. `teloxide` or `reqwest`) which is not included
//! in this scaffold.
//!
//! Extension points:
//! - Replace `connect()` with `getMe` validation and webhook/long-poll setup.
//! - Replace `recv()` with `getUpdates` long-polling or webhook handler.
//! - Replace `send()` with `sendMessage` API call.

use anyhow::{bail, Result};

use crate::router::{GatewayMessage, GatewayResponse};
use crate::GatewayProvider;

/// Telegram gateway provider.
pub struct TelegramProvider {
    token: String,
    connected: bool,
    /// Offset for long-polling `getUpdates`.
    #[allow(dead_code)]
    update_offset: i64,
}

impl TelegramProvider {
    /// Create a `TelegramProvider` if `AMUX_TELEGRAM_TOKEN` is set and non-empty.
    pub fn from_env() -> Option<Self> {
        let token = std::env::var("AMUX_TELEGRAM_TOKEN").ok()?;
        if token.is_empty() {
            return None;
        }
        Some(Self {
            token,
            connected: false,
            update_offset: 0,
        })
    }

    /// Base URL for the Telegram Bot API.
    #[allow(dead_code)]
    fn api_base(&self) -> String {
        format!("https://api.telegram.org/bot{}", self.token)
    }
}

impl GatewayProvider for TelegramProvider {
    fn name(&self) -> &str {
        "Telegram"
    }

    fn connect<'a>(
        &'a mut self,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send + 'a>> {
        Box::pin(async move {
            tracing::info!(
                "Telegram provider: would validate token via getMe (token: {}…)",
                &self.token[..8.min(self.token.len())],
            );
            // TODO: Call GET {api_base}/getMe to validate the token and log
            // the bot's username.
            tracing::warn!(
                "Telegram API client is not configured — \
                 install an HTTP client crate and implement long-polling"
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
                bail!("Telegram provider not connected");
            }
            // TODO: Long-poll via GET {api_base}/getUpdates?offset={update_offset}&timeout=30
            //
            // For each Update containing a Message:
            //   GatewayMessage {
            //       platform: "telegram",
            //       channel_id: message.chat.id.to_string(),
            //       user_id: message.from.id.to_string(),
            //       text: message.text.unwrap_or_default(),
            //       timestamp: message.date as u64,
            //   }
            //
            // After processing, set update_offset = last_update_id + 1.
            Ok(None)
        })
    }

    fn send<'a>(
        &'a mut self,
        response: GatewayResponse,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send + 'a>> {
        Box::pin(async move {
            if !self.connected {
                bail!("Telegram provider not connected");
            }
            tracing::info!(
                chat_id = %response.channel_id,
                text = %response.text,
                "Telegram: would send message via sendMessage API"
            );
            // TODO: POST {api_base}/sendMessage with:
            //   { "chat_id": response.channel_id, "text": response.text }
            tracing::warn!("Telegram send is a stub — message not actually delivered");
            Ok(())
        })
    }
}
