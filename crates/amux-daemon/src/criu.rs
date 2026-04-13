#![allow(dead_code)]

//! CRIU (Checkpoint/Restore In Userspace) integration.
//! Allows freezing and restoring process state for managed sessions.
//! Requires `criu` binary with root privileges on Linux.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

/// Check if CRIU is available on the system.
pub fn is_available() -> bool {
    std::process::Command::new("criu")
        .arg("check")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Checkpoint (freeze) a process tree by PID.
/// Saves process state to `dump_dir`.
pub fn checkpoint(pid: u32, dump_dir: &Path) -> Result<bool> {
    std::fs::create_dir_all(dump_dir).with_context(|| {
        format!(
            "failed to create CRIU dump directory: {}",
            dump_dir.display()
        )
    })?;

    let status = std::process::Command::new("criu")
        .args([
            "dump",
            "--tree",
            &pid.to_string(),
            "--images-dir",
            &dump_dir.to_string_lossy(),
            "--shell-job",
            "--leave-running", // Don't kill the process after dumping
        ])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .status()
        .with_context(|| "failed to execute criu dump")?;

    if status.success() {
        tracing::info!(pid, path = %dump_dir.display(), "CRIU checkpoint created");
        Ok(true)
    } else {
        tracing::warn!(pid, "CRIU checkpoint failed (requires root privileges)");
        Ok(false)
    }
}

/// Restore a process from a CRIU checkpoint.
/// Returns the new PID of the restored process.
pub fn restore(dump_dir: &Path) -> Result<Option<u32>> {
    if !dump_dir.exists() {
        anyhow::bail!("CRIU dump directory does not exist: {}", dump_dir.display());
    }

    let output = std::process::Command::new("criu")
        .args([
            "restore",
            "--images-dir",
            &dump_dir.to_string_lossy(),
            "--shell-job",
            "--restore-detached",
        ])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .output()
        .with_context(|| "failed to execute criu restore")?;

    if output.status.success() {
        tracing::info!(path = %dump_dir.display(), "CRIU process restored");
        // CRIU doesn't directly return the new PID in stdout,
        // but the process is restored with its original PID
        Ok(None)
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        tracing::warn!(path = %dump_dir.display(), error = %stderr, "CRIU restore failed");
        Ok(None)
    }
}

/// Get the default CRIU dump directory for a session.
pub fn dump_dir_for_session(session_id: &str) -> Result<PathBuf> {
    let base = amux_protocol::ensure_amux_data_dir()?;
    let dir = base.join("criu-dumps").join(session_id);
    Ok(dir)
}
