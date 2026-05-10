use super::*;
use rusqlite::params;

#[derive(Debug, Clone)]
pub(crate) struct ThreadSkillRead {
    pub name: String,
    pub content: String,
    pub read_count: i64,
    pub last_read_at: i64,
}

impl HistoryStore {
    pub(crate) async fn record_thread_skill_read(
        &self,
        thread_id: &str,
        kind: &str,
        name: &str,
        content: &str,
        now_ms: i64,
    ) -> Result<()> {
        let thread_id = thread_id.to_string();
        let kind = kind.to_string();
        let name = name.to_string();
        let content = content.to_string();
        self.conn
            .call(move |conn| {
                conn.execute(
                    "INSERT INTO thread_skill_reads
                        (thread_id, kind, name, read_count, last_read_at, last_content)
                     VALUES (?1, ?2, ?3, 1, ?4, ?5)
                     ON CONFLICT(thread_id, kind, name) DO UPDATE SET
                        read_count = read_count + 1,
                        last_read_at = excluded.last_read_at,
                        last_content = excluded.last_content",
                    params![thread_id, kind, name, now_ms, content],
                )?;
                Ok(())
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub(crate) async fn top_thread_skill_reads(
        &self,
        thread_id: &str,
        kind: &str,
        limit: usize,
    ) -> Result<Vec<ThreadSkillRead>> {
        let thread_id = thread_id.to_string();
        let kind = kind.to_string();
        let limit = limit as i64;
        self.interactive_read_conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT name, last_content, read_count, last_read_at
                     FROM thread_skill_reads
                     WHERE thread_id = ?1 AND kind = ?2
                     ORDER BY read_count DESC, last_read_at DESC
                     LIMIT ?3",
                )?;
                let rows = stmt.query_map(params![thread_id, kind, limit], |row| {
                    Ok(ThreadSkillRead {
                        name: row.get(0)?,
                        content: row.get(1)?,
                        read_count: row.get(2)?,
                        last_read_at: row.get(3)?,
                    })
                })?;
                Ok(rows.filter_map(|row| row.ok()).collect())
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }
}
