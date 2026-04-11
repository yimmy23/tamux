use super::schema_helpers::{ensure_column, table_has_column};
use rusqlite::{Connection, OptionalExtension};
use std::path::Path;

const OFFLOADED_PAYLOADS_TABLE_SQL: &str = "CREATE TABLE offloaded_payloads (
    payload_id TEXT PRIMARY KEY,
    thread_id TEXT NOT NULL,
    tool_name TEXT NOT NULL,
    tool_call_id TEXT,
    storage_path TEXT NOT NULL,
    content_type TEXT NOT NULL,
    byte_size INTEGER NOT NULL,
    summary TEXT NOT NULL,
    created_at INTEGER NOT NULL
)";

const OFFLOADED_PAYLOADS_TABLE_IF_MISSING_SQL: &str =
    "CREATE TABLE IF NOT EXISTS offloaded_payloads (
    payload_id TEXT PRIMARY KEY,
    thread_id TEXT NOT NULL,
    tool_name TEXT NOT NULL,
    tool_call_id TEXT,
    storage_path TEXT NOT NULL,
    content_type TEXT NOT NULL,
    byte_size INTEGER NOT NULL,
    summary TEXT NOT NULL,
    created_at INTEGER NOT NULL
)";

const OFFLOADED_PAYLOADS_INDEX_SQL: &str = "CREATE INDEX IF NOT EXISTS idx_offloaded_payloads_thread_created ON offloaded_payloads(thread_id, created_at DESC)";

fn offloaded_payloads_summary_is_required(connection: &Connection) -> rusqlite::Result<bool> {
    let summary_notnull = connection
        .query_row(
            "SELECT \"notnull\" FROM pragma_table_info('offloaded_payloads') WHERE name = 'summary'",
            [],
            |row| row.get::<_, i64>(0),
        )
        .optional()?
        .unwrap_or(0);
    Ok(summary_notnull == 1)
}

fn canonical_offloaded_payload_storage_path(
    offloaded_payloads_dir: &Path,
    thread_id: &str,
    payload_id: &str,
) -> String {
    offloaded_payloads_dir
        .join(thread_id)
        .join(format!("{payload_id}.txt"))
        .to_string_lossy()
        .into_owned()
}

fn rebuild_offloaded_payloads_table(
    connection: &Connection,
    offloaded_payloads_dir: &Path,
) -> rusqlite::Result<()> {
    let transaction = connection.unchecked_transaction()?;

    transaction.execute_batch(&format!(
        "ALTER TABLE offloaded_payloads RENAME TO offloaded_payloads_legacy;
         {OFFLOADED_PAYLOADS_TABLE_SQL};"
    ))?;

    let legacy_rows = {
        let mut stmt = transaction.prepare(
            "SELECT payload_id, thread_id, tool_name, tool_call_id, content_type, byte_size, summary, created_at
             FROM offloaded_payloads_legacy",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, i64>(5)?,
                row.get::<_, Option<String>>(6)?,
                row.get::<_, i64>(7)?,
            ))
        })?;
        rows.collect::<std::result::Result<Vec<_>, _>>()?
    };

    let mut insert_stmt = transaction.prepare(
        "INSERT INTO offloaded_payloads (
             payload_id,
             thread_id,
             tool_name,
             tool_call_id,
             storage_path,
             content_type,
             byte_size,
             summary,
             created_at
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
    )?;

    for (
        payload_id,
        thread_id,
        tool_name,
        tool_call_id,
        content_type,
        byte_size,
        summary,
        created_at,
    ) in legacy_rows
    {
        let storage_path = canonical_offloaded_payload_storage_path(
            offloaded_payloads_dir,
            &thread_id,
            &payload_id,
        );
        insert_stmt.execute(rusqlite::params![
            payload_id,
            thread_id,
            tool_name,
            tool_call_id,
            storage_path,
            content_type,
            byte_size,
            summary.unwrap_or_default(),
            created_at,
        ])?;
    }

    drop(insert_stmt);
    transaction.execute_batch(&format!(
        "DROP TABLE offloaded_payloads_legacy;
         {OFFLOADED_PAYLOADS_INDEX_SQL};"
    ))?;
    transaction.commit()
}

fn ensure_offloaded_payloads_schema(
    connection: &Connection,
    offloaded_payloads_dir: &Path,
) -> rusqlite::Result<()> {
    connection.execute_batch(&format!("{OFFLOADED_PAYLOADS_TABLE_IF_MISSING_SQL};"))?;
    if table_has_column(connection, "offloaded_payloads", "summary")?
        && !offloaded_payloads_summary_is_required(connection)?
    {
        rebuild_offloaded_payloads_table(connection, offloaded_payloads_dir)?;
    }
    connection.execute_batch(&format!("{OFFLOADED_PAYLOADS_INDEX_SQL};"))?;
    Ok(())
}

pub(super) fn ensure_context_archive_fts(connection: &Connection) {
    connection
        .execute_batch(
            "CREATE VIRTUAL TABLE IF NOT EXISTS context_archive_fts USING fts5(summary, compressed_content, content=context_archive, content_rowid=rowid);",
        )
        .ok();
}

pub(super) fn apply_schema_migrations(
    connection: &Connection,
    offloaded_payloads_dir: &Path,
) -> rusqlite::Result<()> {
    ensure_offloaded_payloads_schema(connection, offloaded_payloads_dir)?;
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS thread_structural_memory (
            thread_id TEXT PRIMARY KEY,
            state_json TEXT NOT NULL,
            updated_at INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_thread_structural_memory_updated ON thread_structural_memory(updated_at DESC);",
    )?;
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
    ensure_column(connection, "agent_tasks", "tool_whitelist_json", "TEXT")?;
    ensure_column(connection, "agent_tasks", "tool_blacklist_json", "TEXT")?;
    ensure_column(
        connection,
        "agent_tasks",
        "context_budget_tokens",
        "INTEGER",
    )?;
    ensure_column(connection, "agent_tasks", "context_overflow_action", "TEXT")?;
    ensure_column(connection, "agent_tasks", "termination_conditions", "TEXT")?;
    ensure_column(connection, "agent_tasks", "success_criteria", "TEXT")?;
    ensure_column(connection, "agent_tasks", "max_duration_secs", "INTEGER")?;
    ensure_column(connection, "agent_tasks", "supervisor_config_json", "TEXT")?;
    ensure_column(connection, "agent_tasks", "policy_fingerprint", "TEXT")?;
    ensure_column(connection, "agent_tasks", "approval_expires_at", "INTEGER")?;
    ensure_column(connection, "agent_tasks", "containment_scope", "TEXT")?;
    ensure_column(connection, "agent_tasks", "compensation_status", "TEXT")?;
    ensure_column(connection, "agent_tasks", "compensation_summary", "TEXT")?;
    ensure_column(connection, "goal_runs", "client_request_id", "TEXT")?;
    ensure_column(connection, "goal_runs", "failure_cause", "TEXT")?;
    ensure_column(
        connection,
        "goal_runs",
        "child_task_count",
        "INTEGER NOT NULL DEFAULT 0",
    )?;
    ensure_column(
        connection,
        "goal_runs",
        "approval_count",
        "INTEGER NOT NULL DEFAULT 0",
    )?;
    ensure_column(connection, "goal_runs", "awaiting_approval_id", "TEXT")?;
    ensure_column(connection, "goal_runs", "policy_fingerprint", "TEXT")?;
    ensure_column(connection, "goal_runs", "approval_expires_at", "INTEGER")?;
    ensure_column(connection, "goal_runs", "containment_scope", "TEXT")?;
    ensure_column(connection, "goal_runs", "compensation_status", "TEXT")?;
    ensure_column(connection, "goal_runs", "compensation_summary", "TEXT")?;
    ensure_column(connection, "goal_runs", "active_task_id", "TEXT")?;
    ensure_column(connection, "goal_runs", "duration_ms", "INTEGER")?;
    ensure_column(
        connection,
        "goal_runs",
        "total_prompt_tokens",
        "INTEGER NOT NULL DEFAULT 0",
    )?;
    ensure_column(
        connection,
        "goal_runs",
        "total_completion_tokens",
        "INTEGER NOT NULL DEFAULT 0",
    )?;
    ensure_column(connection, "goal_runs", "estimated_cost_usd", "REAL")?;
    ensure_column(
        connection,
        "goal_runs",
        "autonomy_level",
        "TEXT NOT NULL DEFAULT 'aware'",
    )?;
    ensure_column(connection, "goal_runs", "authorship_tag", "TEXT")?;
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
