use super::*;

impl HistoryStore {
    pub async fn upsert_critique_session(
        &self,
        session_id: &str,
        session_json: &str,
        updated_at: u64,
    ) -> Result<()> {
        let session_id = session_id.to_string();
        let session_json = session_json.to_string();
        self.conn
            .call(move |conn| {
                conn.execute(
                    "INSERT OR REPLACE INTO critique_sessions (session_id, session_json, updated_at) VALUES (?1, ?2, ?3)",
                    params![session_id, session_json, updated_at as i64],
                )?;
                Ok(())
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn get_critique_session(
        &self,
        session_id: &str,
    ) -> Result<Option<CritiqueSessionRow>> {
        let session_id = session_id.to_string();
        self.conn
            .call(move |conn| {
                conn.query_row(
                    "SELECT session_id, session_json, updated_at FROM critique_sessions WHERE session_id = ?1",
                    params![session_id],
                    |row| {
                        Ok(CritiqueSessionRow {
                            session_id: row.get(0)?,
                            session_json: row.get(1)?,
                            updated_at: row.get::<_, i64>(2)? as u64,
                        })
                    },
                )
                .optional()
                .map_err(Into::into)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn list_recent_critique_sessions_for_tool(
        &self,
        tool_name: &str,
        limit: u32,
    ) -> Result<Vec<CritiqueSessionRow>> {
        let tool_name = tool_name.to_string();
        self.conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT session_id, session_json, updated_at
                     FROM critique_sessions
                     WHERE json_extract(session_json, '$.tool_name') = ?1
                     ORDER BY updated_at DESC
                     LIMIT ?2",
                )?;
                let rows = stmt.query_map(params![tool_name, limit], |row| {
                    Ok(CritiqueSessionRow {
                        session_id: row.get(0)?,
                        session_json: row.get(1)?,
                        updated_at: row.get::<_, i64>(2)? as u64,
                    })
                })?;
                rows.collect::<std::result::Result<Vec<_>, _>>()
                    .map_err(Into::into)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
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
        let session_id = session_id.to_string();
        let role = role.to_string();
        let claim = claim.to_string();
        let evidence_json = evidence_json.to_string();
        self.conn
            .call(move |conn| {
                conn.execute(
                    "INSERT INTO critique_arguments (session_id, role, claim, weight, evidence_json, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    params![session_id, role, claim, weight, evidence_json, created_at as i64],
                )?;
                Ok(())
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
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
        let session_id = session_id.to_string();
        let decision = decision.to_string();
        let resolution_json = resolution_json.to_string();
        self.conn
            .call(move |conn| {
                conn.execute(
                    "INSERT OR REPLACE INTO critique_resolutions (session_id, decision, resolution_json, risk_score, confidence, resolved_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    params![
                        session_id,
                        decision,
                        resolution_json,
                        risk_score,
                        confidence,
                        resolved_at as i64,
                    ],
                )?;
                Ok(())
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }
}
