use super::*;

fn map_debate_session_row(row: &db::Row) -> anyhow::Result<DebateSessionRow> {
    Ok(DebateSessionRow {
        session_id: row.get(0)?,
        session_json: row.get(1)?,
        updated_at: row.get::<i64>(2)? as u64,
    })
}

fn map_debate_argument_row(row: &db::Row) -> anyhow::Result<DebateArgumentRow> {
    Ok(DebateArgumentRow {
        session_id: row.get(0)?,
        argument_json: row.get(1)?,
        created_at: row.get::<i64>(2)? as u64,
    })
}

fn map_debate_verdict_row(row: &db::Row) -> anyhow::Result<DebateVerdictRow> {
    Ok(DebateVerdictRow {
        session_id: row.get(0)?,
        verdict_json: row.get(1)?,
        updated_at: row.get::<i64>(2)? as u64,
    })
}

impl HistoryStore {
    pub async fn upsert_debate_session(
        &self,
        session_id: &str,
        session_json: &str,
        updated_at: u64,
    ) -> Result<()> {
        self.conn_db
            .execute(
                "INSERT OR REPLACE INTO debate_sessions (session_id, session_json, updated_at) VALUES (?1, ?2, ?3)",
                db::db_params![session_id, session_json, updated_at as i64],
            )
            .await?;
        Ok(())
    }

    pub async fn get_debate_session(&self, session_id: &str) -> Result<Option<DebateSessionRow>> {
        let row = self
            .conn_db
            .query_opt(
                "SELECT session_id, session_json, updated_at FROM debate_sessions WHERE session_id = ?1",
                db::db_params![session_id],
            )
            .await?;
        row.map(|row| map_debate_session_row(&row)).transpose()
    }

    pub async fn list_debate_sessions(&self, limit: usize) -> Result<Vec<DebateSessionRow>> {
        let limit = limit.max(1) as i64;
        let rows = self
            .conn_db
            .query(
                "SELECT session_id, session_json, updated_at FROM debate_sessions ORDER BY updated_at DESC, session_id DESC LIMIT ?1",
                db::db_params![limit],
            )
            .await?;
        rows.iter().map(map_debate_session_row).collect()
    }

    pub async fn insert_debate_argument(
        &self,
        session_id: &str,
        argument_json: &str,
        created_at: u64,
    ) -> Result<()> {
        self.conn_db
            .execute(
                "INSERT INTO debate_arguments (session_id, argument_json, created_at) VALUES (?1, ?2, ?3)",
                db::db_params![session_id, argument_json, created_at as i64],
            )
            .await?;
        Ok(())
    }

    pub async fn list_debate_arguments(&self, session_id: &str) -> Result<Vec<DebateArgumentRow>> {
        let rows = self
            .conn_db
            .query(
                "SELECT session_id, argument_json, created_at FROM debate_arguments WHERE session_id = ?1 ORDER BY created_at ASC",
                db::db_params![session_id],
            )
            .await?;
        rows.iter().map(map_debate_argument_row).collect()
    }

    pub async fn upsert_debate_verdict(
        &self,
        session_id: &str,
        verdict_json: &str,
        updated_at: u64,
    ) -> Result<()> {
        self.conn_db
            .execute(
                "INSERT OR REPLACE INTO debate_verdicts (session_id, verdict_json, updated_at) VALUES (?1, ?2, ?3)",
                db::db_params![session_id, verdict_json, updated_at as i64],
            )
            .await?;
        Ok(())
    }

    pub async fn get_debate_verdict(&self, session_id: &str) -> Result<Option<DebateVerdictRow>> {
        let row = self
            .conn_db
            .query_opt(
                "SELECT session_id, verdict_json, updated_at FROM debate_verdicts WHERE session_id = ?1",
                db::db_params![session_id],
            )
            .await?;
        row.map(|row| map_debate_verdict_row(&row)).transpose()
    }
}
