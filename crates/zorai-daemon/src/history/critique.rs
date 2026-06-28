use super::*;

fn map_critique_session_row(row: &db::Row) -> anyhow::Result<CritiqueSessionRow> {
    Ok(CritiqueSessionRow {
        session_id: row.get(0)?,
        session_json: row.get(1)?,
        updated_at: row.get::<i64>(2)? as u64,
    })
}

impl HistoryStore {
    pub async fn upsert_critique_session(
        &self,
        session_id: &str,
        session_json: &str,
        updated_at: u64,
    ) -> Result<()> {
        self.conn_db
            .execute(
                "INSERT OR REPLACE INTO critique_sessions (session_id, session_json, updated_at) VALUES (?1, ?2, ?3)",
                db::db_params![session_id, session_json, updated_at as i64],
            )
            .await?;
        Ok(())
    }

    pub async fn get_critique_session(
        &self,
        session_id: &str,
    ) -> Result<Option<CritiqueSessionRow>> {
        let row = self
            .conn_db
            .query_opt(
                "SELECT session_id, session_json, updated_at FROM critique_sessions WHERE session_id = ?1",
                db::db_params![session_id],
            )
            .await?;
        row.map(|row| map_critique_session_row(&row)).transpose()
    }

    pub async fn list_recent_critique_sessions_for_tool(
        &self,
        tool_name: &str,
        limit: u32,
    ) -> Result<Vec<CritiqueSessionRow>> {
        let rows = self
            .conn_db
            .query(
                "SELECT session_id, session_json, updated_at
                     FROM critique_sessions
                     WHERE json_extract(session_json, '$.tool_name') = ?1
                     ORDER BY updated_at DESC
                     LIMIT ?2",
                db::db_params![tool_name, limit as i64],
            )
            .await?;
        rows.iter().map(map_critique_session_row).collect()
    }

    pub async fn insert_critique_argument(
        &self,
        session_id: &str,
        role: &str,
        claim: &str,
        weight: f64,
        evidence_json: &str,
        created_at: u64,
    ) -> Result<()> {
        self.conn_db
            .execute(
                "INSERT INTO critique_arguments (session_id, role, claim, weight, evidence_json, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                db::db_params![session_id, role, claim, weight, evidence_json, created_at as i64],
            )
            .await?;
        Ok(())
    }

    pub async fn upsert_critique_resolution(
        &self,
        session_id: &str,
        decision: &str,
        resolution_json: &str,
        risk_score: f64,
        confidence: f64,
        resolved_at: u64,
    ) -> Result<()> {
        self.conn_db
            .execute(
                "INSERT OR REPLACE INTO critique_resolutions (session_id, decision, resolution_json, risk_score, confidence, resolved_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                db::db_params![
                    session_id,
                    decision,
                    resolution_json,
                    risk_score,
                    confidence,
                    resolved_at as i64,
                ],
            )
            .await?;
        Ok(())
    }
}
