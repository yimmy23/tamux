use amux_protocol::{
    AgentDbMessage, AgentDbThread, AgentEventRow, ApprovalDecision, CommandLogEntry, DaemonMessage,
    HistorySearchHit, ManagedCommandRequest, SessionId, SessionInfo, SnapshotIndexEntry,
    SnapshotInfo, SymbolMatch, TelemetryLedgerStatus, TranscriptIndexEntry, WorkspaceTopology,
};
use anyhow::Result;
use serde::Serialize;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{broadcast, Mutex, RwLock};
use uuid::Uuid;

use crate::history::HistoryStore;
use crate::policy::{evaluate_command, PolicyDecision};
use crate::pty_session::PtySession;
use crate::snapshot::SnapshotStore;
use crate::state::{save_state, DaemonState, SavedSession};
use crate::validation::{find_symbol, validate_command};

mod history_api;
mod session_ops;

/// Central session registry — the source of truth for all running terminals.
pub struct SessionManager {
    sessions: RwLock<HashMap<SessionId, Arc<Mutex<PtySession>>>>,
    state_path: std::path::PathBuf,
    history: Arc<HistoryStore>,
    snapshots: SnapshotStore,
    pending_approvals: RwLock<HashMap<String, PendingApproval>>,
    pty_channel_capacity: usize,
}

struct PendingApproval {
    session_id: SessionId,
    workspace_id: Option<String>,
    execution_id: String,
    request: ManagedCommandRequest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BackgroundTaskState {
    Queued,
    Running,
    Completed,
    Failed,
}

#[derive(Debug, Clone, Serialize)]
pub struct BackgroundTaskStatus {
    pub background_task_id: String,
    pub kind: String,
    pub state: BackgroundTaskState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub position: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snapshot_path: Option<String>,
}

impl SessionManager {
    #[cfg(test)]
    pub async fn new_test(root: &std::path::Path) -> Arc<Self> {
        let history = Arc::new(
            HistoryStore::new_test_store(root)
                .await
                .expect("test history store initialization failed"),
        );
        Self::new_with_history(history, 256)
    }

    pub fn new_with_history(history: Arc<HistoryStore>, pty_channel_capacity: usize) -> Arc<Self> {
        let snapshots = SnapshotStore::new_with_history((*history).clone());
        Arc::new(Self {
            sessions: RwLock::new(HashMap::new()),
            state_path: crate::state::default_state_path(),
            history,
            snapshots,
            pending_approvals: RwLock::new(HashMap::new()),
            pty_channel_capacity,
        })
    }
}

#[cfg(test)]
#[path = "session_manager/tests.rs"]
mod tests;
