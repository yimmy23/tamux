use serde::Deserialize;

/// Commands for the database bridge (JSON over stdin).
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub(super) enum DbBridgeCommand {
    AppendCommandLog {
        entry_json: String,
    },
    CompleteCommandLog {
        id: String,
        exit_code: Option<i32>,
        duration_ms: Option<i64>,
    },
    QueryCommandLog {
        workspace_id: Option<String>,
        pane_id: Option<String>,
        limit: Option<usize>,
    },
    ClearCommandLog,
    CreateAgentThread {
        thread_json: String,
    },
    DeleteAgentThread {
        thread_id: String,
    },
    ListAgentThreads,
    GetAgentThread {
        thread_id: String,
    },
    AddAgentMessage {
        message_json: String,
    },
    DeleteAgentMessages {
        thread_id: String,
        message_ids: Vec<String>,
    },
    ListAgentMessages {
        thread_id: String,
        limit: Option<usize>,
    },
    UpsertTranscriptIndex {
        entry_json: String,
    },
    ListTranscriptIndex {
        workspace_id: Option<String>,
    },
    UpsertSnapshotIndex {
        entry_json: String,
    },
    ListSnapshotIndex {
        workspace_id: Option<String>,
    },
    UpsertAgentEvent {
        event_json: String,
    },
    ListAgentEvents {
        category: Option<String>,
        pane_id: Option<String>,
        limit: Option<usize>,
    },
    Shutdown,
}
