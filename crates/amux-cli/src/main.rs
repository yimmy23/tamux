mod cli;
mod client;
mod commands;
mod output;
mod plugins;
mod setup_wizard;

use anyhow::Result;
use clap::Parser;
use tracing_subscriber::EnvFilter;

use crate::cli::{Cli, Commands};

fn init_logging(log_file_name: &str) -> Result<tracing_appender::non_blocking::WorkerGuard> {
    let log_dir = amux_protocol::ensure_amux_data_dir()?;
    let log_path = amux_protocol::log_file_path(log_file_name);
    let file_appender = tracing_appender::rolling::never(log_dir, log_file_name);
    let (writer, guard) = tracing_appender::non_blocking(file_appender);

    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_env("TAMUX_LOG")
                .or_else(|_| EnvFilter::try_from_env("AMUX_LOG"))
                .unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with_writer(writer)
        .with_ansi(false)
        .init();

    std::panic::set_hook(Box::new(|panic_info| {
        tracing::error!(panic = %panic_info, "tamux-cli panicked");
    }));

    tracing::info!(path = %log_path.display(), "cli log file initialized");
    Ok(guard)
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let log_file_name = match &cli.command {
        Some(Commands::Bridge { .. }) => "tamux-bridge.log",
        _ => "tamux-cli.log",
    };
    let _log_guard = init_logging(log_file_name)?;
    tracing::info!(command = ?cli.command, "tamux-cli starting");

    match cli.command {
        Some(command) => commands::run(command).await?,
        None => commands::run_default().await?,
    }

    Ok(())
}
