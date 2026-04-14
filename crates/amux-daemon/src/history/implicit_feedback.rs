use super::*;

impl HistoryStore {
    pub async fn insert_implicit_signal(&self, row: &ImplicitSignalRow) -> Result<()> {
        let row = row.clone();
        self.conn
            .call(move |conn| {
                conn.execute(
                    "INSERT INTO implicit_signals (id, session_id, signal_type, weight, timestamp_ms, context_snapshot_json) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    params![
                        row.id,
                        row.session_id,
                        row.signal_type,
                        row.weight,
                        row.timestamp_ms as i64,
                        row.context_snapshot_json,
                    ],
                )?;
                Ok(())
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn list_implicit_signals(
        &self,
        session_id: &str,
        limit: usize,
    ) -> Result<Vec<ImplicitSignalRow>> {
        let session_id = session_id.to_string();
        let limit = limit.max(1) as i64;
        self.conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT id, session_id, signal_type, weight, timestamp_ms, context_snapshot_json
                     FROM implicit_signals
                     WHERE session_id = ?1
                     ORDER BY timestamp_ms DESC, id DESC
                     LIMIT ?2",
                )?;
                let rows = stmt.query_map(params![session_id, limit], |row| {
                    Ok(ImplicitSignalRow {
                        id: row.get(0)?,
                        session_id: row.get(1)?,
                        signal_type: row.get(2)?,
                        weight: row.get(3)?,
                        timestamp_ms: row.get::<_, i64>(4)?.max(0) as u64,
                        context_snapshot_json: row.get(5)?,
                    })
                })?;
                rows.collect::<std::result::Result<Vec<_>, _>>()
                    .map_err(Into::into)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn insert_satisfaction_score(&self, row: &SatisfactionScoreRow) -> Result<()> {
        let row = row.clone();
        self.conn
            .call(move |conn| {
                conn.execute(
                    "INSERT INTO satisfaction_scores (id, session_id, score, computed_at_ms, label, signal_count) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    params![
                        row.id,
                        row.session_id,
                        row.score,
                        row.computed_at_ms as i64,
                        row.label,
                        row.signal_count as i64,
                    ],
                )?;
                Ok(())
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn list_satisfaction_scores(
        &self,
        session_id: &str,
        limit: usize,
    ) -> Result<Vec<SatisfactionScoreRow>> {
        let session_id = session_id.to_string();
        let limit = limit.max(1) as i64;
        self.conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT id, session_id, score, computed_at_ms, label, signal_count
                     FROM satisfaction_scores
                     WHERE session_id = ?1
                     ORDER BY computed_at_ms DESC, id DESC
                     LIMIT ?2",
                )?;
                let rows = stmt.query_map(params![session_id, limit], |row| {
                    Ok(SatisfactionScoreRow {
                        id: row.get(0)?,
                        session_id: row.get(1)?,
                        score: row.get(2)?,
                        computed_at_ms: row.get::<_, i64>(3)?.max(0) as u64,
                        label: row.get(4)?,
                        signal_count: row.get::<_, i64>(5)?.max(0) as u64,
                    })
                })?;
                rows.collect::<std::result::Result<Vec<_>, _>>()
                    .map_err(Into::into)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn list_recent_implicit_signal_samples(
        &self,
        limit: usize,
    ) -> Result<Vec<(f64, u64)>> {
        let limit = limit.max(1) as i64;
        self.conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT weight, timestamp_ms
                     FROM implicit_signals
                     ORDER BY timestamp_ms DESC, id DESC
                     LIMIT ?1",
                )?;
                let rows = stmt.query_map(params![limit], |row| {
                    Ok((row.get::<_, f64>(0)?, row.get::<_, i64>(1)?.max(0) as u64))
                })?;
                rows.collect::<std::result::Result<Vec<_>, _>>()
                    .map_err(Into::into)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }
}
