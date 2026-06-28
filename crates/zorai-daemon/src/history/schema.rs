use super::*;

use super::schema_migrations::{
    apply_schema_migrations, ensure_context_archive_fts, prepare_extended_schema_migrations,
};
use super::schema_sql::base_schema_sql;
use super::schema_sql_extra::extended_schema_sql;

async fn ensure_execution_traces_extended_schema<E: super::db::DbExecutor + ?Sized>(
    exec: &mut E,
) -> anyhow::Result<()> {
    let rows = exec
        .query(
            "PRAGMA table_info(execution_traces)",
            super::db::Params::None,
        )
        .await?;
    let mut columns = std::collections::HashSet::new();
    for row in &rows {
        columns.insert(row.get::<String>(1)?);
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
            exec.execute(
                &format!("ALTER TABLE execution_traces ADD COLUMN {name} {definition}"),
                super::db::Params::None,
            )
            .await?;
        }
    }

    if columns.contains("created_at") {
        exec.execute(
            "UPDATE execution_traces SET started_at_ms = CASE WHEN started_at_ms = 0 THEN created_at ELSE started_at_ms END",
            super::db::Params::None,
        )
        .await?;
        exec.execute(
            "UPDATE execution_traces SET completed_at_ms = CASE WHEN completed_at_ms IS NULL THEN created_at ELSE completed_at_ms END",
            super::db::Params::None,
        )
        .await?;
    }

    exec.execute(
        "CREATE INDEX IF NOT EXISTS idx_execution_traces_agent ON execution_traces(agent_id, started_at_ms DESC)",
        super::db::Params::None,
    )
    .await?;
    exec.execute(
        "CREATE INDEX IF NOT EXISTS idx_execution_traces_outcome ON execution_traces(outcome, started_at_ms DESC)",
        super::db::Params::None,
    )
    .await?;

    Ok(())
}

impl HistoryStore {
    pub(super) async fn init_schema(&self) -> Result<()> {
        let offloaded_payloads_dir = self.offloaded_payloads_dir();
        init_schema_on_connection(
            &mut super::db::ConnExecutor(&*self.conn_db),
            &offloaded_payloads_dir,
        )
        .await?;
        Ok(())
    }
}

pub(crate) async fn init_schema_on_connection<E: super::db::DbExecutor + ?Sized>(
    exec: &mut E,
    offloaded_payloads_dir: &std::path::Path,
) -> anyhow::Result<()> {
    exec.execute_batch(base_schema_sql()).await?;
    prepare_extended_schema_migrations(&mut *exec).await?;
    exec.execute_batch(extended_schema_sql()).await?;
    ensure_execution_traces_extended_schema(&mut *exec).await?;
    ensure_context_archive_fts(&mut *exec).await;
    apply_schema_migrations(&mut *exec, offloaded_payloads_dir).await?;
    Ok(())
}
