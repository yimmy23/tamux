use super::*;

#[derive(Debug, Clone)]
pub(crate) struct ThreadSkillRead {
    pub name: String,
    pub content: String,
    pub read_count: i64,
    pub last_read_at: i64,
}

fn map_thread_skill_read(row: &db::Row) -> anyhow::Result<ThreadSkillRead> {
    Ok(ThreadSkillRead {
        name: row.get(0)?,
        content: row.get(1)?,
        read_count: row.get(2)?,
        last_read_at: row.get(3)?,
    })
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
        self.conn_db
            .execute(
                "INSERT INTO thread_skill_reads
                        (thread_id, kind, name, read_count, last_read_at, last_content)
                     VALUES (?1, ?2, ?3, 1, ?4, ?5)
                     ON CONFLICT(thread_id, kind, name) DO UPDATE SET
                        read_count = read_count + 1,
                        last_read_at = excluded.last_read_at,
                        last_content = excluded.last_content",
                db::db_params![thread_id, kind, name, now_ms, content],
            )
            .await?;
        Ok(())
    }

    pub(crate) async fn top_thread_skill_reads(
        &self,
        thread_id: &str,
        kind: &str,
        limit: usize,
    ) -> Result<Vec<ThreadSkillRead>> {
        let limit = limit as i64;
        let rows = self
            .interactive_read_db
            .query(
                "SELECT name, last_content, read_count, last_read_at
                     FROM thread_skill_reads
                     WHERE thread_id = ?1 AND kind = ?2
                     ORDER BY read_count DESC, last_read_at DESC
                     LIMIT ?3",
                db::db_params![thread_id, kind, limit],
            )
            .await?;
        Ok(rows
            .iter()
            .filter_map(|row| map_thread_skill_read(row).ok())
            .collect())
    }
}
