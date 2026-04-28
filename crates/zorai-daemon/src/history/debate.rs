use super::*;

impl HistoryStore {
    pub async fn upsert_debate_session(
        &self,
        session_id: &str,
        session_json: &str,
        updated_at: u64,
    ) -> Result<()> {
        let session_id = session_id.to_string();
        let session_json = session_json.to_string();
        self.conn.call(move |conn| {
            conn.execute(
                "INSERT OR REPLACE INTO debate_sessions (session_id, session_json, updated_at) VALUES (?1, ?2, ?3)",
                params![session_id, session_json, updated_at as i64],
            )?;
            Ok(())
        }).await.map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn get_debate_session(&self, session_id: &str) -> Result<Option<DebateSessionRow>> {
        let session_id = session_id.to_string();
        self.conn.call(move |conn| {
            conn.query_row(
                "SELECT session_id, session_json, updated_at FROM debate_sessions WHERE session_id = ?1",
                params![session_id],
                |row| {
                    Ok(DebateSessionRow {
                        session_id: row.get(0)?,
                        session_json: row.get(1)?,
                        updated_at: row.get::<_, i64>(2)? as u64,
                    })
                },
            )
            .optional()
            .map_err(Into::into)
        }).await.map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn list_debate_sessions(&self, limit: usize) -> Result<Vec<DebateSessionRow>> {
        let limit = limit.max(1) as i64;
        self.conn.call(move |conn| {
            let mut stmt = conn.prepare(
                "SELECT session_id, session_json, updated_at FROM debate_sessions ORDER BY updated_at DESC, session_id DESC LIMIT ?1",
            )?;
            let rows = stmt.query_map(params![limit], |row| {
                Ok(DebateSessionRow {
                    session_id: row.get(0)?,
                    session_json: row.get(1)?,
                    updated_at: row.get::<_, i64>(2)? as u64,
                })
            })?;
            rows.collect::<std::result::Result<Vec<_>, _>>().map_err(Into::into)
        }).await.map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn insert_debate_argument(
        &self,
        session_id: &str,
        argument_json: &str,
        created_at: u64,
    ) -> Result<()> {
        let session_id = session_id.to_string();
        let argument_json = argument_json.to_string();
        self.conn.call(move |conn| {
            conn.execute(
                "INSERT INTO debate_arguments (session_id, argument_json, created_at) VALUES (?1, ?2, ?3)",
                params![session_id, argument_json, created_at as i64],
            )?;
            Ok(())
        }).await.map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn list_debate_arguments(&self, session_id: &str) -> Result<Vec<DebateArgumentRow>> {
        let session_id = session_id.to_string();
        self.conn.call(move |conn| {
            let mut stmt = conn.prepare(
                "SELECT session_id, argument_json, created_at FROM debate_arguments WHERE session_id = ?1 ORDER BY created_at ASC",
            )?;
            let rows = stmt.query_map(params![session_id], |row| {
                Ok(DebateArgumentRow {
                    session_id: row.get(0)?,
                    argument_json: row.get(1)?,
                    created_at: row.get::<_, i64>(2)? as u64,
                })
            })?;
            rows.collect::<std::result::Result<Vec<_>, _>>().map_err(Into::into)
        }).await.map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn upsert_debate_verdict(
        &self,
        session_id: &str,
        verdict_json: &str,
        updated_at: u64,
    ) -> Result<()> {
        let session_id = session_id.to_string();
        let verdict_json = verdict_json.to_string();
        self.conn.call(move |conn| {
            conn.execute(
                "INSERT OR REPLACE INTO debate_verdicts (session_id, verdict_json, updated_at) VALUES (?1, ?2, ?3)",
                params![session_id, verdict_json, updated_at as i64],
            )?;
            Ok(())
        }).await.map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn get_debate_verdict(&self, session_id: &str) -> Result<Option<DebateVerdictRow>> {
        let session_id = session_id.to_string();
        self.conn.call(move |conn| {
            conn.query_row(
                "SELECT session_id, verdict_json, updated_at FROM debate_verdicts WHERE session_id = ?1",
                params![session_id],
                |row| {
                    Ok(DebateVerdictRow {
                        session_id: row.get(0)?,
                        verdict_json: row.get(1)?,
                        updated_at: row.get::<_, i64>(2)? as u64,
                    })
                },
            )
            .optional()
            .map_err(Into::into)
        }).await.map_err(|e| anyhow::anyhow!("{e}"))
    }
}
