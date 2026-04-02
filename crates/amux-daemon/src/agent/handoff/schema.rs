//! SQLite schema for handoff persistence tables.
//!
//! Tables:
//! - `specialist_profiles` — registered specialist agent profiles
//! - `handoff_log` — audit trail for every handoff dispatch

use anyhow::Result;

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
        handoff_depth           INTEGER NOT NULL DEFAULT 0,
        outcome                 TEXT NOT NULL DEFAULT 'dispatched',
        confidence_band         TEXT,
        duration_ms             INTEGER,
        error_message           TEXT,
        created_at              INTEGER NOT NULL,
        completed_at            INTEGER
    );
    CREATE INDEX IF NOT EXISTS idx_handoff_log_from_task ON handoff_log(from_task_id);
    CREATE INDEX IF NOT EXISTS idx_handoff_log_specialist ON handoff_log(to_specialist_id);
    CREATE INDEX IF NOT EXISTS idx_handoff_log_outcome ON handoff_log(outcome);
    CREATE INDEX IF NOT EXISTS idx_handoff_log_created ON handoff_log(created_at);
";

/// Initialize the handoff schema in the given SQLite connection.
///
/// Creates specialist_profiles and handoff_log tables with indexes.
/// Safe to call multiple times (all statements use IF NOT EXISTS).
pub fn init_handoff_schema(conn: &rusqlite::Connection) -> Result<()> {
    conn.execute_batch(HANDOFF_SCHEMA)?;
    Ok(())
}
