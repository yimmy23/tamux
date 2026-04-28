pub(crate) mod domain_learning;
pub(crate) mod domain_memory;
pub(crate) mod domain_routing;
pub(crate) mod domain_safety;
pub(crate) mod protocol;
mod worker_main;

use anyhow::Result;
use protocol::{BackgroundWorkerCommand, BackgroundWorkerKind, BackgroundWorkerResult};

#[cfg(not(test))]
use super::skill_preflight::resolve_daemon_worker_executable;
#[cfg(not(test))]
use anyhow::Context;
#[cfg(not(test))]
use std::process::Stdio;
#[cfg(not(test))]
use tokio::io::AsyncWriteExt;

pub(crate) fn resolve_background_worker_kind_arg(
    arg: Option<&str>,
) -> Option<BackgroundWorkerKind> {
    arg.and_then(BackgroundWorkerKind::from_cli_arg)
}

#[cfg(test)]
pub(crate) async fn run_background_worker_command(
    kind: BackgroundWorkerKind,
    command: BackgroundWorkerCommand,
) -> Result<BackgroundWorkerResult> {
    Ok(worker_main::handle_background_worker_command(kind, command))
}

#[cfg(not(test))]
pub(crate) async fn run_background_worker_command(
    kind: BackgroundWorkerKind,
    command: BackgroundWorkerCommand,
) -> Result<BackgroundWorkerResult> {
    let executable = resolve_daemon_worker_executable()?;
    let mut child = tokio::process::Command::new(executable)
        .arg(kind.cli_arg())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("spawn background worker subprocess for {kind:?}"))?;

    let request_json =
        serde_json::to_vec(&command).context("serialize background worker request")?;
    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| anyhow::anyhow!("background worker stdin unavailable"))?;
    stdin
        .write_all(&request_json)
        .await
        .context("write background worker request")?;
    drop(stdin);

    let output = child
        .wait_with_output()
        .await
        .context("wait for background worker subprocess")?;
    if !output.status.success() {
        anyhow::bail!(
            "background worker subprocess failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }

    serde_json::from_slice::<BackgroundWorkerResult>(&output.stdout)
        .context("parse background worker response")
}

pub(crate) use worker_main::run_background_worker_from_stdio;
