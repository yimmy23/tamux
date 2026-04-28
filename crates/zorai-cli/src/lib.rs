mod cli;
mod client;
mod commands;
mod output;
mod plugins;
mod setup_wizard;
mod update;

use anyhow::Result;
use clap::Parser;
use tracing_subscriber::EnvFilter;

use crate::cli::{Cli, Commands};

fn init_logging(log_file_name: &str) -> Result<tracing_appender::non_blocking::WorkerGuard> {
    let file_appender = zorai_protocol::DailyLogWriter::new(log_file_name)?;
    let log_path = file_appender.current_path()?;
    let (writer, guard) = tracing_appender::non_blocking(file_appender);

    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_env("ZORAI_LOG")
                .or_else(|_| EnvFilter::try_from_env("ZORAI_LOG"))
                .unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with_writer(writer)
        .with_ansi(false)
        .init();

    std::panic::set_hook(Box::new(|panic_info| {
        tracing::error!(panic = %panic_info, "zorai-cli panicked");
    }));

    tracing::info!(path = %log_path.display(), "cli log file initialized");
    Ok(guard)
}

pub async fn run() -> Result<()> {
    let cli = Cli::parse();
    let log_file_name = match &cli.command {
        Some(Commands::Bridge { .. }) => "zorai-bridge.log",
        _ => "zorai-cli.log",
    };
    let _log_guard = init_logging(log_file_name)?;
    tracing::info!(command = ?cli.command, "zorai-cli starting");

    match cli.command {
        Some(command) => commands::run(command).await?,
        None => commands::run_default().await?,
    }

    Ok(())
}
