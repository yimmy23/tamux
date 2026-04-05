use super::schema_helpers::ensure_column;
use rusqlite::Connection;

pub(super) fn ensure_context_archive_fts(connection: &Connection) {
    connection
        .execute_batch(
            "CREATE VIRTUAL TABLE IF NOT EXISTS context_archive_fts USING fts5(summary, compressed_content, content=context_archive, content_rowid=rowid);",
        )
        .ok();
}

pub(super) fn apply_schema_migrations(connection: &Connection) -> rusqlite::Result<()> {
    ensure_column(connection, "agent_tasks", "session_id", "TEXT")?;
    ensure_column(connection, "agent_threads", "metadata_json", "TEXT")?;
    ensure_column(connection, "agent_tasks", "scheduled_at", "INTEGER")?;
    ensure_column(connection, "agent_tasks", "goal_run_id", "TEXT")?;
    ensure_column(connection, "agent_tasks", "goal_run_title", "TEXT")?;
    ensure_column(connection, "agent_tasks", "goal_step_id", "TEXT")?;
    ensure_column(connection, "agent_tasks", "goal_step_title", "TEXT")?;
    ensure_column(connection, "agent_tasks", "parent_task_id", "TEXT")?;
    ensure_column(connection, "agent_tasks", "parent_thread_id", "TEXT")?;
    ensure_column(
        connection,
        "agent_tasks",
        "runtime",
        "TEXT NOT NULL DEFAULT 'daemon'",
    )?;
    ensure_column(connection, "agent_tasks", "override_provider", "TEXT")?;
    ensure_column(connection, "agent_tasks", "override_model", "TEXT")?;
    ensure_column(connection, "agent_tasks", "override_system_prompt", "TEXT")?;
    ensure_column(connection, "agent_tasks", "sub_agent_def_id", "TEXT")?;
    ensure_column(connection, "goal_runs", "client_request_id", "TEXT")?;
    ensure_column(connection, "goal_run_events", "step_index", "INTEGER")?;
    ensure_column(connection, "goal_run_events", "todo_snapshot_json", "TEXT")?;
    // BEAT-09: user_action column for dismissal tracking in action_audit.
    ensure_column(connection, "action_audit", "user_action", "TEXT")?;
    ensure_column(connection, "memory_provenance", "confirmed_at", "INTEGER")?;
    ensure_column(connection, "memory_provenance", "retracted_at", "INTEGER")?;
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS memory_provenance_relationships (
            id TEXT PRIMARY KEY,
            source_entry_id TEXT NOT NULL,
            target_entry_id TEXT NOT NULL,
            relation_type TEXT NOT NULL,
            fact_key TEXT,
            created_at INTEGER NOT NULL
        );
        CREATE UNIQUE INDEX IF NOT EXISTS idx_memory_provenance_rel_unique ON memory_provenance_relationships(source_entry_id, target_entry_id, relation_type, fact_key);
        CREATE INDEX IF NOT EXISTS idx_memory_provenance_rel_source ON memory_provenance_relationships(source_entry_id, created_at DESC);",
    )?;
    connection.execute(
            "CREATE INDEX IF NOT EXISTS idx_agent_tasks_goal_run ON agent_tasks(goal_run_id, created_at DESC)",
            [],
        )?;
    // Episodic memory schema (Phase v3.0).
    crate::agent::episodic::schema::init_episodic_schema(connection)
        .map_err(|e| rusqlite::Error::ToSqlConversionFailure(e.into()))?;
    // Handoff broker schema (Phase v3.0: HAND-09).
    crate::agent::handoff::schema::init_handoff_schema(connection)
        .map_err(|e| rusqlite::Error::ToSqlConversionFailure(e.into()))?;
    Ok(())
}
