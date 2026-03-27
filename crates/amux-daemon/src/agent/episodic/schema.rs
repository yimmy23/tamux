//! SQLite schema for episodic memory tables.

use anyhow::Result;

/// Full SQL schema for the episodic memory subsystem.
///
/// Tables:
/// - `episodes` — structured episode records
/// - `episodes_fts` — FTS5 full-text search index on episodes
/// - `episode_links` — directed relationships between episodes
/// - `negative_knowledge` — constraint graph of ruled-out approaches
/// - `counter_who_state` — persistent self-model snapshots
pub const EPISODIC_SCHEMA: &str = "
    CREATE TABLE IF NOT EXISTS episodes (
        id             TEXT PRIMARY KEY,
        goal_run_id    TEXT,
        thread_id      TEXT,
        session_id     TEXT,
        episode_type   TEXT NOT NULL,
        summary        TEXT NOT NULL,
        outcome        TEXT NOT NULL,
        root_cause     TEXT,
        entities       TEXT NOT NULL DEFAULT '[]',
        causal_chain   TEXT NOT NULL DEFAULT '[]',
        solution_class TEXT,
        duration_ms    INTEGER,
        tokens_used    INTEGER,
        confidence     REAL,
        created_at     INTEGER NOT NULL,
        expires_at     INTEGER
    );
    CREATE INDEX IF NOT EXISTS idx_episodes_goal ON episodes(goal_run_id);
    CREATE INDEX IF NOT EXISTS idx_episodes_thread ON episodes(thread_id);
    CREATE INDEX IF NOT EXISTS idx_episodes_type_ts ON episodes(episode_type, created_at DESC);
    CREATE INDEX IF NOT EXISTS idx_episodes_outcome ON episodes(outcome, created_at DESC);

    CREATE TABLE IF NOT EXISTS episode_links (
        id                 TEXT PRIMARY KEY,
        source_episode_id  TEXT NOT NULL,
        target_episode_id  TEXT NOT NULL,
        link_type          TEXT NOT NULL,
        evidence           TEXT,
        created_at         INTEGER NOT NULL
    );
    CREATE INDEX IF NOT EXISTS idx_episode_links_source ON episode_links(source_episode_id);
    CREATE INDEX IF NOT EXISTS idx_episode_links_target ON episode_links(target_episode_id);
    CREATE INDEX IF NOT EXISTS idx_episode_links_type ON episode_links(link_type);

    CREATE TABLE IF NOT EXISTS negative_knowledge (
        id              TEXT PRIMARY KEY,
        episode_id      TEXT,
        constraint_type TEXT NOT NULL,
        subject         TEXT NOT NULL,
        solution_class  TEXT,
        description     TEXT NOT NULL,
        confidence      REAL NOT NULL,
        valid_until     INTEGER,
        created_at      INTEGER NOT NULL
    );
    CREATE INDEX IF NOT EXISTS idx_negative_knowledge_subject ON negative_knowledge(subject);
    CREATE INDEX IF NOT EXISTS idx_negative_knowledge_type ON negative_knowledge(constraint_type);
    CREATE INDEX IF NOT EXISTS idx_negative_knowledge_valid ON negative_knowledge(valid_until);

    CREATE TABLE IF NOT EXISTS counter_who_state (
        id           TEXT PRIMARY KEY,
        goal_run_id  TEXT,
        thread_id    TEXT,
        state_json   TEXT NOT NULL,
        updated_at   INTEGER NOT NULL
    );
    CREATE INDEX IF NOT EXISTS idx_counter_who_state_updated ON counter_who_state(updated_at DESC);
";

/// Initialize the episodic memory schema in the given SQLite connection.
///
/// This creates all episodic tables, indexes, and FTS5 virtual tables.
/// Safe to call multiple times (all statements use IF NOT EXISTS).
pub fn init_episodic_schema(conn: &rusqlite::Connection) -> Result<()> {
    conn.execute_batch(EPISODIC_SCHEMA)?;

    // FTS5 virtual table created separately — virtual tables need individual statements.
    // Use .ok() to tolerate SQLite builds without FTS5 support.
    conn.execute_batch(
        "CREATE VIRTUAL TABLE IF NOT EXISTS episodes_fts USING fts5(
            summary,
            entities,
            root_cause,
            content=episodes,
            content_rowid=rowid,
            detail=column
        );",
    )
    .ok();

    // FTS5 sync triggers — keep the FTS index in sync with the episodes table.
    conn.execute_batch(
        "CREATE TRIGGER IF NOT EXISTS episodes_ai AFTER INSERT ON episodes BEGIN
            INSERT INTO episodes_fts(rowid, summary, entities, root_cause)
            VALUES (new.rowid, new.summary, new.entities, new.root_cause);
        END;",
    )
    .ok();

    conn.execute_batch(
        "CREATE TRIGGER IF NOT EXISTS episodes_ad AFTER DELETE ON episodes BEGIN
            INSERT INTO episodes_fts(episodes_fts, rowid, summary, entities, root_cause)
            VALUES ('delete', old.rowid, old.summary, old.entities, old.root_cause);
        END;",
    )
    .ok();

    Ok(())
}
