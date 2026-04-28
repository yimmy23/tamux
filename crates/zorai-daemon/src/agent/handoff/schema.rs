//! SQLite schema for handoff persistence tables.
//!
//! Tables:
//! - `specialist_profiles` — registered specialist agent profiles
//! - `handoff_log` — audit trail for every handoff dispatch

use anyhow::Result;

fn table_has_column(
    conn: &rusqlite::Connection,
    table: &str,
    column: &str,
) -> std::result::Result<bool, rusqlite::Error> {
    let pragma = format!("PRAGMA table_info({table})");
    let mut stmt = conn.prepare(&pragma)?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(1))?;
    for row in rows {
        if row? == column {
            return Ok(true);
        }
    }
    Ok(false)
}

fn ensure_column(
    conn: &rusqlite::Connection,
    table: &str,
    column: &str,
    definition: &str,
) -> std::result::Result<(), rusqlite::Error> {
    if table_has_column(conn, table, column)? {
        return Ok(());
    }
    conn.execute(
        &format!("ALTER TABLE {table} ADD COLUMN {column} {definition}"),
        [],
    )?;
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
pub fn init_handoff_schema(conn: &rusqlite::Connection) -> Result<()> {
    conn.execute_batch(HANDOFF_SCHEMA)?;
    ensure_column(conn, "handoff_log", "capability_tags_json", "TEXT")?;
    ensure_column(
        conn,
        "handoff_log",
        "routing_method",
        "TEXT NOT NULL DEFAULT 'deterministic'",
    )?;
    ensure_column(
        conn,
        "handoff_log",
        "routing_score",
        "REAL NOT NULL DEFAULT 0.0",
    )?;
    ensure_column(
        conn,
        "handoff_log",
        "fallback_used",
        "INTEGER NOT NULL DEFAULT 0",
    )?;
    ensure_column(
        conn,
        "agent_capability_scores",
        "avg_confidence_score",
        "REAL NOT NULL DEFAULT 0.5",
    )?;
    ensure_column(
        conn,
        "agent_capability_scores",
        "total_tokens_used",
        "INTEGER NOT NULL DEFAULT 0",
    )?;
    Ok(())
}
