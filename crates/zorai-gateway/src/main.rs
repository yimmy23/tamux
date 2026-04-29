//! zorai-gateway runtime bootstrap.
//!
//! This binary now runs as a dedicated gateway runtime process managed by
//! zorai-daemon. Startup handshake:
//! 1. Connect to daemon IPC.
//! 2. Register gateway runtime.
//! 3. Receive bootstrap payload.
//! 4. Build providers from daemon bootstrap config.
//! 5. Start `GatewayRuntime` (which sends ready ACK and enters the main loop).

mod discord;
mod format;
mod health;
mod ipc;
mod router;
mod runtime;
mod slack;
mod state;
mod telegram;

#[cfg(test)]
mod test_support;

use anyhow::Result;
use runtime::GatewayRuntime;
use zorai_protocol::GatewayBootstrapPayload;

pub use runtime::GatewayProvider;

fn init_logging() -> Result<tracing_appender::non_blocking::WorkerGuard> {
    let file_appender = zorai_protocol::DailyLogWriter::new("zorai-gateway.log")?;
    let log_path = file_appender.current_path()?;
    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_env("ZORAI_GATEWAY_LOG")
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_writer(non_blocking)
        .with_ansi(false)
        .init();

    tracing::info!(path = %log_path.display(), "gateway log file initialized");
    Ok(guard)
}

fn providers_from_bootstrap(
    bootstrap: &GatewayBootstrapPayload,
) -> Result<Vec<Box<dyn GatewayProvider>>> {
    let mut active: Vec<Box<dyn GatewayProvider>> = Vec::new();

    for provider in &bootstrap.providers {
        match provider.platform.to_ascii_lowercase().as_str() {
            "slack" => {
                if let Some(slack) = slack::SlackProvider::from_bootstrap_with_cursors(
                    provider,
                    &bootstrap.continuity.cursors,
                )? {
                    active.push(Box::new(slack));
                }
            }
            "discord" => {
                if let Some(discord) = discord::DiscordProvider::from_bootstrap_with_cursors(
                    provider,
                    &bootstrap.continuity.cursors,
                )? {
                    active.push(Box::new(discord));
                }
            }
            "telegram" => {
                if let Some(telegram) = telegram::TelegramProvider::from_bootstrap_with_cursors(
                    provider,
                    &bootstrap.continuity.cursors,
                )? {
                    active.push(Box::new(telegram));
                }
            }
            other => {
                tracing::debug!(platform = other, "ignoring unsupported gateway platform");
            }
        }
    }

    Ok(active)
}

#[tokio::main]
async fn main() -> Result<()> {
    let _log_guard = init_logging()?;

    tracing::info!("zorai-gateway starting");
    let (daemon, bootstrap) = ipc::connect_and_bootstrap(
        "gateway-main",
        vec![
            "slack".to_string(),
            "discord".to_string(),
            "telegram".to_string(),
        ],
    )
    .await?;

    let state = state::GatewayRuntimeState::from_bootstrap(&bootstrap);
    let providers = providers_from_bootstrap(&bootstrap)?;
    tracing::info!(
        providers = providers.len(),
        "gateway providers built from bootstrap"
    );

    GatewayRuntime::new(daemon, state, providers).run().await
}
