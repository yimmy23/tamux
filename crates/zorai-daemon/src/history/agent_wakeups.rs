use super::*;

fn map_agent_wakeup_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<AgentWakeupRow> {
    Ok(AgentWakeupRow {
        id: row.get(0)?,
        thread_id: row.get(1)?,
        message: row.get(2)?,
        interval_ms: row.get::<_, i64>(3)?.max(0) as u64,
        next_fire_at: row.get::<_, i64>(4)?.max(0) as u64,
        repetitions_remaining: row
            .get::<_, Option<i64>>(5)?
            .map(|value| value.max(0) as u64),
        created_at: row.get::<_, i64>(6)?.max(0) as u64,
    })
}

impl HistoryStore {
    pub async fn upsert_agent_wakeup(&self, row: &AgentWakeupRow) -> Result<()> {
        let row = row.clone();
        self.conn
            .call(move |conn| {
                conn.execute(
                    "INSERT OR REPLACE INTO agent_wakeups (id, thread_id, message, interval_ms, next_fire_at, repetitions_remaining, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                    params![
                        row.id,
                        row.thread_id,
                        row.message,
                        row.interval_ms as i64,
                        row.next_fire_at as i64,
                        row.repetitions_remaining.map(|value| value as i64),
                        row.created_at as i64,
                    ],
                )?;
                Ok(())
            })
            .await
            .map_err(|error| anyhow::anyhow!("{error}"))
    }

    pub async fn delete_agent_wakeup(&self, id: &str) -> Result<bool> {
        let id = id.to_string();
        self.conn
            .call(move |conn| {
                let deleted =
                    conn.execute("DELETE FROM agent_wakeups WHERE id = ?1", params![id])?;
                Ok(deleted > 0)
            })
            .await
            .map_err(|error| anyhow::anyhow!("{error}"))
    }

    pub async fn list_agent_wakeups(&self) -> Result<Vec<AgentWakeupRow>> {
        self.read_conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT id, thread_id, message, interval_ms, next_fire_at, repetitions_remaining, created_at FROM agent_wakeups ORDER BY next_fire_at ASC, id ASC",
                )?;
                let rows = stmt
                    .query_map([], map_agent_wakeup_row)?
                    .collect::<std::result::Result<Vec<_>, _>>()?;
                Ok(rows)
            })
            .await
            .map_err(|error| anyhow::anyhow!("{error}"))
    }
}
