pub mod agent;
mod criu;
mod git;
mod governance;
mod history;
mod lsp_client;
mod network;
mod notifications;
mod orchestration;
mod osc;
pub mod plugin;
mod policy;
mod policy_external;
mod pty_session;
mod sandbox;
mod scrub;
mod server;
mod session_manager;
mod snapshot;
mod state;
#[cfg(test)]
mod test_support;
mod validation;

use anyhow::Result;
use tracing_subscriber::EnvFilter;

fn init_logging() -> Result<tracing_appender::non_blocking::WorkerGuard> {
    let file_appender = amux_protocol::DailyLogWriter::new("tamux-daemon.log")?;
    let log_path = file_appender.current_path()?;
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
        tracing::error!(panic = %panic_info, "tamux-daemon panicked");
    }));

    tracing::info!(path = %log_path.display(), "daemon log file initialized");
    Ok(guard)
}

const DAEMON_TOKIO_WORKER_STACK_BYTES: usize = 8 * 1024 * 1024;

async fn daemon_main() -> Result<()> {
    if std::env::args().nth(1).as_deref()
        == Some(agent::skill_preflight::SKILL_DISCOVERY_WORKER_ARG)
    {
        return agent::skill_preflight::run_skill_discovery_worker_from_stdio().await;
    }
    if std::env::args().nth(1).as_deref() == Some(agent::ALINE_STARTUP_WORKER_ARG) {
        return agent::run_aline_startup_worker_from_stdio().await;
    }
    if let Some(kind) = agent::background_workers::resolve_background_worker_kind_arg(
        std::env::args().nth(1).as_deref(),
    ) {
        return agent::background_workers::run_background_worker_from_stdio(kind).await;
    }

    let _log_guard = init_logging()?;

    tracing::info!("tamux-daemon starting");

    // Restore any persisted state.
    let state_path = state::default_state_path();
    tracing::info!(?state_path, "state file location");
    match state::load_state(&state_path) {
        Ok(state) => {
            tracing::info!(
                previous_sessions = state.previous_sessions.len(),
                "loaded persisted daemon state"
            );
        }
        Err(error) => {
            tracing::warn!(error = %error, path = %state_path.display(), "failed to load persisted daemon state");
        }
    }

    // Start the IPC server (blocks until shutdown signal).
    server::run().await?;

    Ok(())
}

fn main() -> Result<()> {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_stack_size(DAEMON_TOKIO_WORKER_STACK_BYTES)
        .build()?
        .block_on(daemon_main())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn background_worker_protocol_round_trips() {
        let command = agent::background_workers::protocol::BackgroundWorkerCommand::Ping;
        let encoded =
            serde_json::to_string(&command).expect("background worker command should serialize");
        let decoded = serde_json::from_str::<
            agent::background_workers::protocol::BackgroundWorkerCommand,
        >(&encoded)
        .expect("background worker command should deserialize");

        assert!(matches!(
            decoded,
            agent::background_workers::protocol::BackgroundWorkerCommand::Ping
        ));
    }

    #[test]
    fn daemon_main_dispatches_background_worker_args() {
        let kind = agent::background_workers::resolve_background_worker_kind_arg(Some(
            "__tamux-background-worker-safety",
        ))
        .expect("expected safety worker arg to resolve");
        assert_eq!(
            kind,
            agent::background_workers::protocol::BackgroundWorkerKind::Safety
        );
        let routing = agent::background_workers::resolve_background_worker_kind_arg(Some(
            "__tamux-background-worker-routing",
        ))
        .expect("expected routing worker arg to resolve");
        assert_eq!(
            routing,
            agent::background_workers::protocol::BackgroundWorkerKind::Routing
        );
        let memory = agent::background_workers::resolve_background_worker_kind_arg(Some(
            "__tamux-background-worker-memory",
        ))
        .expect("expected memory worker arg to resolve");
        assert_eq!(
            memory,
            agent::background_workers::protocol::BackgroundWorkerKind::Memory
        );
        assert!(
            agent::background_workers::resolve_background_worker_kind_arg(Some(
                agent::skill_preflight::SKILL_DISCOVERY_WORKER_ARG,
            ))
            .is_none(),
            "background worker resolver must ignore other daemon helper args"
        );
    }
}
