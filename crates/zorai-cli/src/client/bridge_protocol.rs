use anyhow::Result;
use serde::{Deserialize, Serialize};
use zorai_protocol::{
    ApprovalDecision, ApprovalPayload, HistorySearchHit, ManagedCommandSource,
    OscNotificationPayload, SnapshotInfo, SymbolMatch,
};

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub(super) enum BridgeCommand {
    Input {
        data: String,
    },
    Resize {
        cols: u16,
        rows: u16,
    },
    ExecuteManaged {
        command: String,
        rationale: String,
        allow_network: bool,
        sandbox_enabled: Option<bool>,
        security_level: Option<String>,
        cwd: Option<String>,
        language_hint: Option<String>,
        source: Option<String>,
    },
    ApprovalDecision {
        approval_id: String,
        decision: String,
    },
    SearchHistory {
        query: String,
        limit: Option<usize>,
    },
    GenerateSkill {
        query: Option<String>,
        title: Option<String>,
    },
    FindSymbol {
        workspace_root: String,
        symbol: String,
        limit: Option<usize>,
    },
    ListSnapshots {
        workspace_id: Option<String>,
    },
    RestoreSnapshot {
        snapshot_id: String,
    },
    Shutdown,
    KillSession,
}

#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub(super) enum BridgeEvent {
    Ready {
        session_id: String,
    },
    Output {
        session_id: String,
        data: String,
    },
    CommandStarted {
        session_id: String,
        command_b64: String,
    },
    CommandFinished {
        session_id: String,
        exit_code: Option<i32>,
    },
    CwdChanged {
        session_id: String,
        cwd: String,
    },
    ManagedQueued {
        session_id: String,
        execution_id: String,
        position: usize,
        snapshot: Option<SnapshotInfo>,
    },
    ApprovalRequired {
        session_id: String,
        approval: ApprovalPayload,
    },
    ApprovalResolved {
        session_id: String,
        approval_id: String,
        decision: ApprovalDecision,
    },
    ManagedStarted {
        session_id: String,
        execution_id: String,
        command: String,
        source: ManagedCommandSource,
    },
    ManagedFinished {
        session_id: String,
        execution_id: String,
        command: String,
        exit_code: Option<i32>,
        duration_ms: Option<u64>,
        snapshot: Option<SnapshotInfo>,
    },
    ManagedRejected {
        session_id: String,
        execution_id: Option<String>,
        message: String,
    },
    HistorySearchResult {
        query: String,
        summary: String,
        hits: Vec<HistorySearchHit>,
    },
    SkillGenerated {
        title: String,
        path: String,
    },
    SymbolSearchResult {
        symbol: String,
        matches: Vec<SymbolMatch>,
    },
    SnapshotList {
        snapshots: Vec<SnapshotInfo>,
    },
    SnapshotRestored {
        snapshot_id: String,
        ok: bool,
        message: String,
    },
    OscNotification {
        session_id: String,
        notification: OscNotificationPayload,
    },
    SessionExited {
        session_id: String,
        exit_code: Option<i32>,
    },
    Error {
        message: String,
    },
}

pub(super) fn emit_bridge_event(event: BridgeEvent) -> Result<()> {
    println!("{}", serde_json::to_string(&event)?);
    Ok(())
}
