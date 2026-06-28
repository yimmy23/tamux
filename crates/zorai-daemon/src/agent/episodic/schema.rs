//! SQLite schema for episodic memory tables.

use crate::history::db::{self, DbExecutor};
use anyhow::Result;

/// Base table schema for the episodic memory subsystem.
///
/// Tables:
/// - `episodes` — structured episode records
/// - `episode_links` — directed relationships between episodes
/// - `negative_knowledge` — constraint graph of ruled-out approaches
/// - `counter_who_state` — persistent self-model snapshots
const EPISODIC_TABLES: &str = "
    CREATE TABLE IF NOT EXISTS episodes (
        id             TEXT PRIMARY KEY,
        agent_id       TEXT,
        goal_run_id    TEXT,
        thread_id      TEXT,
        session_id     TEXT,
        goal_text      TEXT,
        goal_type      TEXT,
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
        confidence_before REAL,
        confidence_after REAL,
        created_at     INTEGER NOT NULL,
        expires_at     INTEGER,
        deleted_at     INTEGER
    );

    CREATE TABLE IF NOT EXISTS episode_links (
        id                 TEXT PRIMARY KEY,
        agent_id           TEXT,
        source_episode_id  TEXT NOT NULL,
        target_episode_id  TEXT NOT NULL,
        link_type          TEXT NOT NULL,
        evidence           TEXT,
        created_at         INTEGER NOT NULL
    );

    CREATE TABLE IF NOT EXISTS negative_knowledge (
        id                          TEXT PRIMARY KEY,
        agent_id                    TEXT,
        episode_id                  TEXT,
        constraint_type             TEXT NOT NULL,
        subject                     TEXT NOT NULL,
        solution_class              TEXT,
        description                 TEXT NOT NULL,
        confidence                  REAL NOT NULL,
        state                       TEXT NOT NULL DEFAULT 'dying',
        evidence_count              INTEGER NOT NULL DEFAULT 1,
        direct_observation          INTEGER NOT NULL DEFAULT 1,
        derived_from_constraint_ids TEXT NOT NULL DEFAULT '[]',
        related_subject_tokens      TEXT NOT NULL DEFAULT '[]',
        valid_until                 INTEGER,
        created_at                  INTEGER NOT NULL,
        deleted_at                  INTEGER
    );

    CREATE TABLE IF NOT EXISTS counter_who_state (
        id           TEXT PRIMARY KEY,
        agent_id     TEXT,
        goal_run_id  TEXT,
        thread_id    TEXT,
        state_json   TEXT NOT NULL,
        updated_at   INTEGER NOT NULL
    );
";

/// Indexes created after column-migration helpers run.
const EPISODIC_INDEXES: &str = "
    CREATE INDEX IF NOT EXISTS idx_episodes_agent ON episodes(agent_id, created_at DESC);
    CREATE INDEX IF NOT EXISTS idx_episodes_goal ON episodes(goal_run_id);
    CREATE INDEX IF NOT EXISTS idx_episodes_thread ON episodes(thread_id);
    CREATE INDEX IF NOT EXISTS idx_episodes_type_ts ON episodes(episode_type, created_at DESC);
    CREATE INDEX IF NOT EXISTS idx_episodes_outcome ON episodes(outcome, created_at DESC);

    CREATE INDEX IF NOT EXISTS idx_episode_links_agent ON episode_links(agent_id, created_at DESC);
    CREATE INDEX IF NOT EXISTS idx_episode_links_source ON episode_links(source_episode_id);
    CREATE INDEX IF NOT EXISTS idx_episode_links_target ON episode_links(target_episode_id);
    CREATE INDEX IF NOT EXISTS idx_episode_links_type ON episode_links(link_type);

    CREATE INDEX IF NOT EXISTS idx_negative_knowledge_agent ON negative_knowledge(agent_id, created_at DESC);
    CREATE INDEX IF NOT EXISTS idx_negative_knowledge_subject ON negative_knowledge(subject);
    CREATE INDEX IF NOT EXISTS idx_negative_knowledge_type ON negative_knowledge(constraint_type);
    CREATE INDEX IF NOT EXISTS idx_negative_knowledge_valid ON negative_knowledge(valid_until);

    CREATE INDEX IF NOT EXISTS idx_counter_who_state_updated ON counter_who_state(updated_at DESC);
";

/// Initialize the episodic memory schema in the given SQLite connection.
///
/// This creates all episodic tables, indexes, and FTS5 virtual tables.
/// Safe to call multiple times (all statements use IF NOT EXISTS).
pub(crate) async fn init_episodic_schema<E: DbExecutor + ?Sized>(exec: &mut E) -> Result<()> {
    exec.execute_batch(EPISODIC_TABLES).await?;
    ensure_episode_columns(&mut *exec).await?;
    exec.execute_batch(EPISODIC_INDEXES).await?;

    exec.execute_batch(
        "CREATE VIRTUAL TABLE IF NOT EXISTS episodes_fts USING fts5(
            summary,
            entities,
            root_cause,
            content=episodes,
            content_rowid=rowid,
            detail=column
        );",
    )
    .await
    .ok();

    exec.execute_batch(
        "CREATE TRIGGER IF NOT EXISTS episodes_ai AFTER INSERT ON episodes BEGIN
            INSERT INTO episodes_fts(rowid, summary, entities, root_cause)
            VALUES (new.rowid, new.summary, new.entities, new.root_cause);
        END;",
    )
    .await
    .ok();

    exec.execute_batch(
        "CREATE TRIGGER IF NOT EXISTS episodes_ad AFTER DELETE ON episodes BEGIN
            INSERT INTO episodes_fts(episodes_fts, rowid, summary, entities, root_cause)
            VALUES ('delete', old.rowid, old.summary, old.entities, old.root_cause);
        END;",
    )
    .await
    .ok();

    Ok(())
}

async fn ensure_episode_columns<E: DbExecutor + ?Sized>(exec: &mut E) -> Result<()> {
    ensure_column(&mut *exec, "episodes", "agent_id", "TEXT").await?;
    ensure_column(&mut *exec, "episodes", "goal_text", "TEXT").await?;
    ensure_column(&mut *exec, "episodes", "goal_type", "TEXT").await?;
    ensure_column(&mut *exec, "episodes", "confidence_before", "REAL").await?;
    ensure_column(&mut *exec, "episodes", "confidence_after", "REAL").await?;
    ensure_column(&mut *exec, "episodes", "deleted_at", "INTEGER").await?;
    ensure_column(&mut *exec, "episode_links", "agent_id", "TEXT").await?;
    ensure_column(&mut *exec, "negative_knowledge", "agent_id", "TEXT").await?;
    ensure_column(&mut *exec, "negative_knowledge", "deleted_at", "INTEGER").await?;
    ensure_column(
        &mut *exec,
        "negative_knowledge",
        "state",
        "TEXT NOT NULL DEFAULT 'dying'",
    )
    .await?;
    ensure_column(
        &mut *exec,
        "negative_knowledge",
        "evidence_count",
        "INTEGER NOT NULL DEFAULT 1",
    )
    .await?;
    ensure_column(
        &mut *exec,
        "negative_knowledge",
        "direct_observation",
        "INTEGER NOT NULL DEFAULT 1",
    )
    .await?;
    ensure_column(
        &mut *exec,
        "negative_knowledge",
        "derived_from_constraint_ids",
        "TEXT NOT NULL DEFAULT '[]'",
    )
    .await?;
    ensure_column(
        &mut *exec,
        "negative_knowledge",
        "related_subject_tokens",
        "TEXT NOT NULL DEFAULT '[]'",
    )
    .await?;
    ensure_column(&mut *exec, "counter_who_state", "agent_id", "TEXT").await?;
    Ok(())
}

async fn ensure_column<E: DbExecutor + ?Sized>(
    exec: &mut E,
    table: &str,
    column: &str,
    column_def: &str,
) -> Result<()> {
    let rows = exec
        .query(&format!("PRAGMA table_info({table})"), db::Params::None)
        .await?;
    let mut exists = false;
    for row in &rows {
        if row.get::<String>(1)? == column {
            exists = true;
            break;
        }
    }
    if !exists {
        match exec
            .execute(
                &format!("ALTER TABLE {table} ADD COLUMN {column} {column_def}"),
                db::Params::None,
            )
            .await
        {
            Ok(_) => {}
            Err(err) if is_duplicate_column_error(&err) => {}
            Err(err) => return Err(err),
        }
    }
    Ok(())
}

fn is_duplicate_column_error(err: &anyhow::Error) -> bool {
    format!("{err:?}")
        .to_ascii_lowercase()
        .contains("duplicate column name")
}

#[cfg(test)]
mod tests {
    use super::{init_episodic_schema, is_duplicate_column_error};
    use crate::history::db::{self, sqlite::SqliteWriteConn, DbConn};
    use anyhow::Result;
    use std::path::{Path, PathBuf};
    use std::sync::Arc;
    use uuid::Uuid;

    async fn mem_conn() -> SqliteWriteConn {
        let raw = tokio_rusqlite::Connection::open_in_memory()
            .await
            .expect("open in-memory connection");
        SqliteWriteConn::new(raw, PathBuf::from(":memory:"))
    }

    async fn file_conn(path: &Path) -> SqliteWriteConn {
        let raw = tokio_rusqlite::Connection::open(path)
            .await
            .expect("open file connection");
        SqliteWriteConn::new(raw, path.to_path_buf())
    }

    async fn assert_constraint_state_columns_exist(conn: &dyn DbConn) -> Result<()> {
        let rows = conn
            .query("PRAGMA table_info(negative_knowledge)", db::Params::None)
            .await?;
        let columns = rows
            .iter()
            .map(|row| row.get::<String>(1))
            .collect::<Result<Vec<_>>>()?;

        for expected in [
            "state",
            "evidence_count",
            "direct_observation",
            "derived_from_constraint_ids",
            "related_subject_tokens",
        ] {
            assert!(
                columns.iter().any(|column| column == expected),
                "missing column {expected}; found columns: {columns:?}"
            );
        }

        Ok(())
    }

    #[tokio::test]
    async fn init_episodic_schema_adds_constraint_state_columns() -> Result<()> {
        let conn = mem_conn().await;

        init_episodic_schema(&mut db::ConnExecutor(&conn)).await?;

        assert_constraint_state_columns_exist(&conn).await?;

        Ok(())
    }

    #[tokio::test]
    async fn init_episodic_schema_migrates_legacy_negative_knowledge_table() -> Result<()> {
        let conn = mem_conn().await;

        conn.execute_batch(
            "CREATE TABLE negative_knowledge (
                id              TEXT PRIMARY KEY,
                agent_id        TEXT,
                episode_id      TEXT,
                constraint_type TEXT NOT NULL,
                subject         TEXT NOT NULL,
                solution_class  TEXT,
                description     TEXT NOT NULL,
                confidence      REAL NOT NULL,
                valid_until     INTEGER,
                created_at      INTEGER NOT NULL
            );",
        )
        .await?;

        init_episodic_schema(&mut db::ConnExecutor(&conn)).await?;

        assert_constraint_state_columns_exist(&conn).await?;

        Ok(())
    }

    #[test]
    fn duplicate_column_error_detection_matches_sqlite_shape() {
        let duplicate_column = anyhow::anyhow!("ALTER TABLE failed: duplicate column name: state");
        let other_sqlite_error = anyhow::anyhow!("some other sqlite failure");

        assert!(is_duplicate_column_error(&duplicate_column));
        assert!(!is_duplicate_column_error(&other_sqlite_error));
    }

    #[tokio::test]
    async fn init_episodic_schema_tolerates_concurrent_legacy_migration() -> Result<()> {
        let db_path = std::env::temp_dir().join(format!(
            "zorai-episodic-schema-concurrency-{}.db",
            Uuid::new_v4()
        ));

        file_conn(&db_path)
            .await
            .execute_batch(
                "CREATE TABLE negative_knowledge (
                id              TEXT PRIMARY KEY,
                agent_id        TEXT,
                episode_id      TEXT,
                constraint_type TEXT NOT NULL,
                subject         TEXT NOT NULL,
                solution_class  TEXT,
                description     TEXT NOT NULL,
                confidence      REAL NOT NULL,
                valid_until     INTEGER,
                created_at      INTEGER NOT NULL
            );",
            )
            .await?;

        let workers = 8;
        let barrier = Arc::new(tokio::sync::Barrier::new(workers));
        let mut handles = Vec::with_capacity(workers);

        for _ in 0..workers {
            let barrier = Arc::clone(&barrier);
            let db_path = db_path.clone();
            handles.push(tokio::spawn(async move {
                let conn = file_conn(&db_path).await;
                barrier.wait().await;
                init_episodic_schema(&mut db::ConnExecutor(&conn)).await
            }));
        }

        for handle in handles {
            handle.await.expect("schema worker panicked")?;
        }

        let conn = file_conn(&db_path).await;
        assert_constraint_state_columns_exist(&conn).await?;
        let _ = std::fs::remove_file(db_path);

        Ok(())
    }
}
