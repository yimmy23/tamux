use std::path::PathBuf;

use anyhow::Result;
use serde::{Deserialize, Serialize};

/// Persisted daemon state (saved between restarts).
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct DaemonState {
    /// Sessions that were running when the daemon last shut down.
    /// We store metadata only — the actual PTY processes are gone after a
    /// daemon restart, but we record them so the UI can show "stale" sessions
    /// and offer to re-create them.
    pub previous_sessions: Vec<SavedSession>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SavedSession {
    pub id: String,
    pub shell: Option<String>,
    pub cwd: Option<String>,
    pub workspace_id: Option<String>,
    pub cols: u16,
    pub rows: u16,
}

/// Default path for the state file.
pub fn default_state_path() -> PathBuf {
    let base = dirs::data_local_dir().unwrap_or_else(|| PathBuf::from("."));
    let dir = base.join("zorai");
    let _ = std::fs::create_dir_all(&dir);
    dir.join("daemon-state.json")
}

/// Load state from disk.
#[allow(dead_code)]
pub fn load_state(path: &std::path::Path) -> Result<DaemonState> {
    if path.exists() {
        let data = std::fs::read_to_string(path)?;
        let state: DaemonState = serde_json::from_str(&data)?;
        Ok(state)
    } else {
        Ok(DaemonState::default())
    }
}

/// Save state to disk.
#[allow(dead_code)]
pub fn save_state(path: &std::path::Path, state: &DaemonState) -> Result<()> {
    let data = serde_json::to_string_pretty(state)?;
    std::fs::write(path, data)?;
    Ok(())
}
