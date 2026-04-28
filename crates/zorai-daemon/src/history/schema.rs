use super::*;

use super::schema_migrations::{apply_schema_migrations, ensure_context_archive_fts};
use super::schema_sql::base_schema_sql;
use super::schema_sql_extra::extended_schema_sql;

fn ensure_execution_traces_extended_schema(
    connection: &rusqlite::Connection,
) -> rusqlite::Result<()> {
    let mut stmt = connection.prepare("PRAGMA table_info(execution_traces)")?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(1))?;
    let mut columns = std::collections::HashSet::new();
    for row in rows {
        columns.insert(row?);
    }
    if columns.is_empty() {
        return Ok(());
    }

    for (name, definition) in [
        ("thread_id", "TEXT"),
        ("agent_id", "TEXT NOT NULL DEFAULT 'legacy'"),
        ("tool_calls_json", "TEXT"),
        ("tool_fallbacks", "INTEGER DEFAULT 0"),
        ("operator_revisions", "INTEGER DEFAULT 0"),
        ("fast_denials", "INTEGER DEFAULT 0"),
        ("exit_code", "INTEGER"),
        ("started_at_ms", "INTEGER NOT NULL DEFAULT 0"),
        ("completed_at_ms", "INTEGER"),
        ("strategy_hint", "TEXT"),
    ] {
        if !columns.contains(name) {
            connection.execute(
                &format!("ALTER TABLE execution_traces ADD COLUMN {name} {definition}"),
                [],
            )?;
        }
    }

    if columns.contains("created_at") {
        connection.execute(
            "UPDATE execution_traces SET started_at_ms = CASE WHEN started_at_ms = 0 THEN created_at ELSE started_at_ms END",
            [],
        )?;
        connection.execute(
            "UPDATE execution_traces SET completed_at_ms = CASE WHEN completed_at_ms IS NULL THEN created_at ELSE completed_at_ms END",
            [],
        )?;
    }

    connection.execute(
        "CREATE INDEX IF NOT EXISTS idx_execution_traces_agent ON execution_traces(agent_id, started_at_ms DESC)",
        [],
    )?;
    connection.execute(
        "CREATE INDEX IF NOT EXISTS idx_execution_traces_outcome ON execution_traces(outcome, started_at_ms DESC)",
        [],
    )?;

    Ok(())
}

impl HistoryStore {
    pub(super) async fn init_schema(&self) -> Result<()> {
        let offloaded_payloads_dir = self.offloaded_payloads_dir();
        self.conn
            .call(move |connection| {
                Ok(init_schema_on_connection(
                    connection,
                    &offloaded_payloads_dir,
                )?)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }
}

pub(super) fn init_schema_on_connection(
    connection: &rusqlite::Connection,
    offloaded_payloads_dir: &std::path::Path,
) -> rusqlite::Result<()> {
    let schema_sql = format!("{}{}", base_schema_sql(), extended_schema_sql());
    connection.execute_batch(&schema_sql)?;
    ensure_execution_traces_extended_schema(connection)?;
    ensure_context_archive_fts(connection);
    apply_schema_migrations(connection, offloaded_payloads_dir)?;
    Ok(())
}
