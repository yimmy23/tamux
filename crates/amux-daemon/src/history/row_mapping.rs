use super::*;

pub(super) fn refresh_thread_stats(
    connection: &Connection,
    thread_id: &str,
) -> std::result::Result<(), rusqlite::Error> {
    let (message_count, total_tokens, last_preview, latest_message_at): (i64, i64, String, i64) = connection.query_row(
        "SELECT
            COUNT(*),
            COALESCE(SUM(total_tokens), 0),
            COALESCE((SELECT substr(content, 1, 100) FROM agent_messages WHERE thread_id = ?1 ORDER BY created_at DESC LIMIT 1), ''),
            COALESCE((SELECT created_at FROM agent_messages WHERE thread_id = ?1 ORDER BY created_at DESC LIMIT 1), strftime('%s','now') * 1000)
         FROM agent_messages WHERE thread_id = ?1",
        params![thread_id],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
    )?;

    connection.execute(
        "UPDATE agent_threads
         SET
            message_count = ?2,
            total_tokens = ?3,
            last_preview = ?4,
            updated_at = MAX(updated_at, ?5)
         WHERE id = ?1",
        params![
            thread_id,
            message_count,
            total_tokens,
            last_preview,
            latest_message_at
        ],
    )?;
    Ok(())
}

#[derive(Debug, Clone)]
pub struct CausalTraceRecord {
    pub selected_json: String,
    pub causal_factors_json: String,
    pub outcome_json: String,
    pub created_at: u64,
}

/// Full causal trace record for explainability queries (EXPL-01, EXPL-02).
#[derive(Debug, Clone)]
pub struct CausalTraceFullRecord {
    pub id: String,
    pub thread_id: Option<String>,
    pub goal_run_id: Option<String>,
    pub task_id: Option<String>,
    pub decision_type: String,
    pub selected_json: String,
    pub rejected_options_json: String,
    pub context_hash: String,
    pub causal_factors_json: String,
    pub outcome_json: String,
    pub model_used: Option<String>,
    pub created_at: u64,
}

pub(super) fn map_command_log_entry(row: &rusqlite::Row<'_>) -> rusqlite::Result<CommandLogEntry> {
    Ok(CommandLogEntry {
        id: row.get(0)?,
        command: row.get(1)?,
        timestamp: row.get(2)?,
        path: row.get(3)?,
        cwd: row.get(4)?,
        workspace_id: row.get(5)?,
        surface_id: row.get(6)?,
        pane_id: row.get(7)?,
        exit_code: row.get(8)?,
        duration_ms: row.get(9)?,
    })
}

pub(super) fn map_agent_thread(row: &rusqlite::Row<'_>) -> rusqlite::Result<AgentDbThread> {
    Ok(AgentDbThread {
        id: row.get(0)?,
        workspace_id: row.get(1)?,
        surface_id: row.get(2)?,
        pane_id: row.get(3)?,
        agent_name: row.get(4)?,
        title: row.get(5)?,
        created_at: row.get(6)?,
        updated_at: row.get(7)?,
        message_count: row.get(8)?,
        total_tokens: row.get(9)?,
        last_preview: row.get(10)?,
        metadata_json: row.get(11)?,
    })
}

pub(super) fn map_agent_message(row: &rusqlite::Row<'_>) -> rusqlite::Result<AgentDbMessage> {
    Ok(AgentDbMessage {
        id: row.get(0)?,
        thread_id: row.get(1)?,
        created_at: row.get(2)?,
        role: row.get(3)?,
        content: row.get(4)?,
        provider: row.get(5)?,
        model: row.get(6)?,
        input_tokens: row.get(7)?,
        output_tokens: row.get(8)?,
        total_tokens: row.get(9)?,
        reasoning: row.get(10)?,
        tool_calls_json: row.get(11)?,
        metadata_json: row.get(12)?,
    })
}

pub(super) fn map_transcript_index_entry(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<TranscriptIndexEntry> {
    Ok(TranscriptIndexEntry {
        id: row.get(0)?,
        pane_id: row.get(1)?,
        workspace_id: row.get(2)?,
        surface_id: row.get(3)?,
        filename: row.get(4)?,
        reason: row.get(5)?,
        captured_at: row.get(6)?,
        size_bytes: row.get(7)?,
        preview: row.get(8)?,
    })
}

pub(super) fn map_snapshot_index_entry(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<SnapshotIndexEntry> {
    Ok(SnapshotIndexEntry {
        snapshot_id: row.get(0)?,
        workspace_id: row.get(1)?,
        session_id: row.get(2)?,
        kind: row.get(3)?,
        label: row.get(4)?,
        path: row.get(5)?,
        created_at: row.get(6)?,
        details_json: row.get(7)?,
    })
}

pub(super) fn map_agent_event_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<AgentEventRow> {
    Ok(AgentEventRow {
        id: row.get(0)?,
        category: row.get(1)?,
        kind: row.get(2)?,
        pane_id: row.get(3)?,
        workspace_id: row.get(4)?,
        surface_id: row.get(5)?,
        session_id: row.get(6)?,
        payload_json: row.get(7)?,
        timestamp: row.get(8)?,
    })
}

pub(super) fn flatten_option_str(value: &Option<Option<String>>) -> Option<&str> {
    value.as_ref().and_then(|inner| inner.as_deref())
}

pub(super) fn flatten_option_i64(value: &Option<Option<i64>>) -> Option<i64> {
    value.as_ref().copied().flatten()
}
