//! SQLite schema for handoff persistence tables.
//!
//! Tables:
//! - `specialist_profiles` — registered specialist agent profiles
//! - `handoff_log` — audit trail for every handoff dispatch

use crate::history::db::{self, DbExecutor};
use anyhow::Result;

async fn table_has_column<E: DbExecutor + ?Sized>(
    exec: &mut E,
    table: &str,
    column: &str,
) -> Result<bool> {
    let rows = exec
        .query(&format!("PRAGMA table_info({table})"), db::Params::None)
        .await?;
    for row in &rows {
        if row.get::<String>(1)? == column {
            return Ok(true);
        }
    }
    Ok(false)
}

async fn ensure_column<E: DbExecutor + ?Sized>(
    exec: &mut E,
    table: &str,
    column: &str,
    definition: &str,
) -> Result<()> {
    if table_has_column(&mut *exec, table, column).await? {
        return Ok(());
    }
    exec.execute(
        &format!("ALTER TABLE {table} ADD COLUMN {column} {definition}"),
        db::Params::None,
    )
    .await?;
    Ok(())
}

/// Full SQL schema for the handoff subsystem.
pub const HANDOFF_SCHEMA: &str = "
    CREATE TABLE IF NOT EXISTS specialist_profiles (
        id                    TEXT PRIMARY KEY,
        name                  TEXT NOT NULL,
        role                  TEXT NOT NULL,
        capabilities_json     TEXT NOT NULL,
        tool_filter_json      TEXT,
        system_prompt_snippet TEXT,
        escalation_chain_json TEXT,
        is_builtin            INTEGER NOT NULL DEFAULT 0,
        created_at            INTEGER NOT NULL
    );

    CREATE TABLE IF NOT EXISTS handoff_log (
        id                      TEXT PRIMARY KEY,
        from_task_id            TEXT NOT NULL,
        to_specialist_id        TEXT NOT NULL,
        to_task_id              TEXT,
        task_description        TEXT NOT NULL,
        acceptance_criteria_json TEXT,
        context_bundle_json     TEXT,
        capability_tags_json    TEXT,
        handoff_depth           INTEGER NOT NULL DEFAULT 0,
        outcome                 TEXT NOT NULL DEFAULT 'dispatched',
        confidence_band         TEXT,
        routing_method          TEXT NOT NULL DEFAULT 'deterministic',
        routing_score           REAL NOT NULL DEFAULT 0.0,
        fallback_used           INTEGER NOT NULL DEFAULT 0,
        duration_ms             INTEGER,
        error_message           TEXT,
        created_at              INTEGER NOT NULL,
        completed_at            INTEGER
    );
    CREATE INDEX IF NOT EXISTS idx_handoff_log_from_task ON handoff_log(from_task_id);
    CREATE INDEX IF NOT EXISTS idx_handoff_log_specialist ON handoff_log(to_specialist_id);
    CREATE INDEX IF NOT EXISTS idx_handoff_log_outcome ON handoff_log(outcome);
    CREATE INDEX IF NOT EXISTS idx_handoff_log_created ON handoff_log(created_at);

    CREATE TABLE IF NOT EXISTS agent_capability_scores (
        id                    INTEGER PRIMARY KEY AUTOINCREMENT,
        agent_id              TEXT NOT NULL,
        capability_tag        TEXT NOT NULL,
        attempts              INTEGER NOT NULL DEFAULT 0,
        successes             INTEGER NOT NULL DEFAULT 0,
        failures              INTEGER NOT NULL DEFAULT 0,
        partials              INTEGER NOT NULL DEFAULT 0,
        last_attempt_ms       INTEGER,
        avg_confidence_score  REAL NOT NULL DEFAULT 0.5,
        total_tokens_used     INTEGER NOT NULL DEFAULT 0,
        UNIQUE(agent_id, capability_tag)
    );
    CREATE INDEX IF NOT EXISTS idx_agent_capability_scores_agent_tag
        ON agent_capability_scores(agent_id, capability_tag);
    CREATE INDEX IF NOT EXISTS idx_agent_capability_scores_tag
        ON agent_capability_scores(capability_tag);
";

/// Initialize the handoff schema in the given SQLite connection.
///
/// Creates specialist_profiles, handoff_log, and agent_capability_scores tables with indexes.
/// Safe to call multiple times (all statements use IF NOT EXISTS).
pub(crate) async fn init_handoff_schema<E: DbExecutor + ?Sized>(exec: &mut E) -> Result<()> {
    exec.execute_batch(HANDOFF_SCHEMA).await?;
    ensure_column(&mut *exec, "handoff_log", "capability_tags_json", "TEXT").await?;
    ensure_column(
        &mut *exec,
        "handoff_log",
        "routing_method",
        "TEXT NOT NULL DEFAULT 'deterministic'",
    )
    .await?;
    ensure_column(
        &mut *exec,
        "handoff_log",
        "routing_score",
        "REAL NOT NULL DEFAULT 0.0",
    )
    .await?;
    ensure_column(
        &mut *exec,
        "handoff_log",
        "fallback_used",
        "INTEGER NOT NULL DEFAULT 0",
    )
    .await?;
    ensure_column(
        &mut *exec,
        "agent_capability_scores",
        "avg_confidence_score",
        "REAL NOT NULL DEFAULT 0.5",
    )
    .await?;
    ensure_column(
        &mut *exec,
        "agent_capability_scores",
        "total_tokens_used",
        "INTEGER NOT NULL DEFAULT 0",
    )
    .await?;
    Ok(())
}
