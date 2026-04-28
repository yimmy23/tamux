#![allow(dead_code)]

use std::path::Path;
use std::sync::{Mutex, OnceLock};

use anyhow::Result;

use super::super::memory::MemoryTarget;
use crate::history::HistoryStore;

const USER_SYNC_STATE_CLEAN: &str = "clean";
const USER_SYNC_STATE_DIRTY: &str = "dirty";
const USER_SYNC_STATE_RECONCILING: &str = "reconciling";
const USER_PROFILE_IMPORT_SENTINEL: &str = "__legacy_user_import_done";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::agent) enum UserProfileSyncState {
    Clean,
    Dirty,
    Reconciling,
}

impl UserProfileSyncState {
    pub(in crate::agent) fn as_str(self) -> &'static str {
        match self {
            Self::Clean => USER_SYNC_STATE_CLEAN,
            Self::Dirty => USER_SYNC_STATE_DIRTY,
            Self::Reconciling => USER_SYNC_STATE_RECONCILING,
        }
    }

    pub(in crate::agent) fn from_str(value: &str) -> Self {
        match value {
            USER_SYNC_STATE_DIRTY => Self::Dirty,
            USER_SYNC_STATE_RECONCILING => Self::Reconciling,
            _ => Self::Clean,
        }
    }
}

fn sync_state_guard() -> &'static Mutex<UserProfileSyncState> {
    static STATE: OnceLock<Mutex<UserProfileSyncState>> = OnceLock::new();
    STATE.get_or_init(|| Mutex::new(UserProfileSyncState::Clean))
}

pub(in crate::agent) fn current_user_sync_state() -> UserProfileSyncState {
    *sync_state_guard()
        .lock()
        .expect("user profile sync state mutex poisoned")
}

fn set_user_sync_state(state: UserProfileSyncState) {
    *sync_state_guard()
        .lock()
        .expect("user profile sync state mutex poisoned") = state;
}

/// Atomically transition from any non-`Reconciling` state to `Reconciling`.
///
/// Returns `true` if the caller acquired the reconcile slot (and is now responsible
/// for driving the reconcile to completion), or `false` if a reconcile was already
/// in progress.  Using this instead of a separate check + set eliminates the TOCTOU
/// window where two concurrent callers could both believe they own the reconcile.
fn try_acquire_reconcile() -> bool {
    let mut guard = sync_state_guard()
        .lock()
        .expect("user profile sync state mutex poisoned");
    if *guard == UserProfileSyncState::Reconciling {
        return false;
    }
    *guard = UserProfileSyncState::Reconciling;
    true
}

fn active_memory_dir(agent_data_dir: &Path) -> std::path::PathBuf {
    super::super::active_memory_dir(agent_data_dir)
}

fn user_memory_path(agent_data_dir: &Path) -> std::path::PathBuf {
    active_memory_dir(agent_data_dir).join(MemoryTarget::User.file_name())
}

async fn bootstrap_legacy_user_import(agent_data_dir: &Path, history: &HistoryStore) -> Result<()> {
    if history
        .get_profile_field(USER_PROFILE_IMPORT_SENTINEL)
        .await?
        .is_some()
    {
        return Ok(());
    }

    let path = user_memory_path(agent_data_dir);
    let existing = tokio::fs::read_to_string(&path).await.unwrap_or_default();
    let trimmed = existing.trim();
    if !trimmed.is_empty() {
        history
            .upsert_profile_field(
                "legacy_user_md",
                &serde_json::to_string(trimmed)?,
                0.30,
                "legacy_import",
            )
            .await?;
        history
            .append_profile_event(
                &format!("op_evt_{}", uuid::Uuid::new_v4()),
                "legacy_user_import",
                Some("legacy_user_md"),
                Some(&serde_json::to_string(trimmed)?),
                "legacy_import",
                None,
            )
            .await?;
    }

    history
        .upsert_profile_field(
            USER_PROFILE_IMPORT_SENTINEL,
            "\"true\"",
            1.0,
            "legacy_import",
        )
        .await?;
    Ok(())
}

fn render_user_profile_markdown(fields: &[crate::history::OperatorProfileFieldRow]) -> String {
    let mut lines = vec![
        "# User".to_string(),
        "Profile summary is generated from SQLite-backed operator profile.".to_string(),
        "".to_string(),
    ];

    let mut ordered = fields
        .iter()
        .filter(|row| row.field_key != USER_PROFILE_IMPORT_SENTINEL)
        .cloned()
        .collect::<Vec<_>>();
    ordered.sort_by(|a, b| a.field_key.cmp(&b.field_key));
    for row in ordered {
        lines.push(format!("- {}: {}", row.field_key, row.field_value_json));
    }
    lines.push(String::new());
    lines.join("\n")
}

pub(in crate::agent) async fn reconcile_user_profile_from_db(
    agent_data_dir: &Path,
    history: &HistoryStore,
) -> Result<()> {
    if !try_acquire_reconcile() {
        // A reconcile is already in progress; return no-op success so the caller
        // does not need to distinguish "skipped" from "done".  The in-flight
        // reconcile will complete and leave state as Clean (or Dirty on error).
        return Ok(());
    }
    reconcile_inner(agent_data_dir, history).await
}

/// Drive the reconcile body.  **Caller must have already set state to `Reconciling`**
/// (either via `set_user_sync_state` or `try_acquire_reconcile`).
/// All error paths reset state to `Dirty` so the slot is never left stuck.
async fn reconcile_inner(agent_data_dir: &Path, history: &HistoryStore) -> Result<()> {
    if let Err(error) = bootstrap_legacy_user_import(agent_data_dir, history).await {
        set_user_sync_state(UserProfileSyncState::Dirty);
        return Err(error);
    }

    let rows = match history.list_profile_fields().await {
        Ok(r) => r,
        Err(error) => {
            set_user_sync_state(UserProfileSyncState::Dirty);
            return Err(error);
        }
    };

    let content = render_user_profile_markdown(&rows);
    let path = user_memory_path(agent_data_dir);
    match tokio::fs::write(&path, content).await {
        Ok(()) => {
            set_user_sync_state(UserProfileSyncState::Clean);
            Ok(())
        }
        Err(error) => {
            set_user_sync_state(UserProfileSyncState::Dirty);
            Err(error.into())
        }
    }
}

pub(in crate::agent) async fn stage_legacy_user_memory_write(
    history: &HistoryStore,
    content: &str,
) -> Result<()> {
    stage_legacy_user_memory_write_events(history, content).await?;
    set_user_sync_state(UserProfileSyncState::Dirty);
    Ok(())
}

/// Write the legacy-append events to the DB **without** touching sync state.
/// Used by `handle_user_memory_append_with_reconcile` so that the caller can
/// manage the state transition atomically around the reconcile slot acquisition.
async fn stage_legacy_user_memory_write_events(
    history: &HistoryStore,
    content: &str,
) -> Result<()> {
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return Ok(());
    }
    let value_json = serde_json::to_string(trimmed)?;
    history
        .upsert_profile_field("legacy_user_signal", &value_json, 0.55, "legacy_append")
        .await?;
    history
        .append_profile_event(
            &format!("op_evt_{}", uuid::Uuid::new_v4()),
            "legacy_user_memory_append",
            Some("legacy_user_signal"),
            Some(&value_json),
            "legacy_append",
            None,
        )
        .await?;
    Ok(())
}

pub(in crate::agent) async fn handle_user_memory_append_with_reconcile(
    agent_data_dir: &Path,
    history: &HistoryStore,
    content: &str,
) -> Result<()> {
    // Atomically claim the reconcile slot BEFORE any staging work so the state
    // never escapes the Reconciling→Dirty→re-acquire TOCTOU window.
    let acquired = try_acquire_reconcile();

    // Stage the event to DB only (no state change here); the state transition is
    // owned by the reconcile-slot logic above and by reconcile_inner below.
    // If staging fails and we own the reconcile slot, release it to Dirty so the
    // state machine is never left stuck in Reconciling.
    if let Err(error) = stage_legacy_user_memory_write_events(history, content).await {
        if acquired {
            set_user_sync_state(UserProfileSyncState::Dirty);
        }
        return Err(error);
    }

    if !acquired {
        // A reconcile is already in progress.  The staged event will be picked up
        // on the next reconcile cycle.  Leave state as Reconciling (owned by the
        // in-flight reconcile).
        return Ok(());
    }

    // We own the Reconciling state; drive the reconcile directly.
    reconcile_inner(agent_data_dir, history).await
}

#[cfg(test)]
#[path = "user_sync/test_support.rs"]
mod test_support;
#[cfg(test)]
pub(in crate::agent) use test_support::{
    acquire_user_sync_test_guard, set_user_sync_state_for_test,
};
#[cfg(test)]
#[path = "user_sync/tests.rs"]
mod tests;
