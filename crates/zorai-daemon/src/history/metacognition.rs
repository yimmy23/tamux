use super::*;

impl HistoryStore {
    pub async fn upsert_meta_cognition_model(
        &self,
        agent_id: &str,
        calibration_offset: f64,
        last_updated_at: u64,
    ) -> Result<()> {
        let agent_id = agent_id.to_string();
        self.conn
            .call(move |conn| {
                conn.execute(
                    "INSERT OR REPLACE INTO meta_cognition_model (id, agent_id, calibration_offset, last_updated_at) VALUES (1, ?1, ?2, ?3)",
                    params![agent_id, calibration_offset, last_updated_at as i64],
                )?;
                Ok(())
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn get_meta_cognition_model(&self) -> Result<Option<MetaCognitionModelRow>> {
        self.conn
            .call(move |conn| {
                conn.query_row(
                    "SELECT id, agent_id, calibration_offset, last_updated_at FROM meta_cognition_model WHERE id = 1",
                    [],
                    |row| {
                        Ok(MetaCognitionModelRow {
                            id: row.get(0)?,
                            agent_id: row.get(1)?,
                            calibration_offset: row.get(2)?,
                            last_updated_at: row.get::<_, i64>(3)?.max(0) as u64,
                        })
                    },
                )
                .optional()
                .map_err(Into::into)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn replace_cognitive_biases(
        &self,
        model_id: i64,
        rows: &[CognitiveBiasRow],
    ) -> Result<()> {
        let rows = rows.to_vec();
        self.conn
            .call(move |conn| {
                let tx = conn.transaction()?;
                tx.execute(
                    "UPDATE cognitive_biases SET deleted_at = ?2 WHERE model_id = ?1 AND deleted_at IS NULL",
                    params![model_id, now_ts() as i64],
                )?;
                for row in rows {
                    tx.execute(
                        "INSERT INTO cognitive_biases (model_id, name, trigger_pattern_json, mitigation_prompt, severity, occurrence_count, deleted_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, NULL)",
                        params![
                            row.model_id,
                            row.name,
                            row.trigger_pattern_json,
                            row.mitigation_prompt,
                            row.severity,
                            row.occurrence_count as i64,
                        ],
                    )?;
                }
                tx.commit()?;
                Ok(())
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn list_cognitive_biases(&self, model_id: i64) -> Result<Vec<CognitiveBiasRow>> {
        self.conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT id, model_id, name, trigger_pattern_json, mitigation_prompt, severity, occurrence_count FROM cognitive_biases WHERE model_id = ?1 AND deleted_at IS NULL ORDER BY severity DESC, id ASC",
                )?;
                let rows = stmt.query_map(params![model_id], |row| {
                    Ok(CognitiveBiasRow {
                        id: row.get(0)?,
                        model_id: row.get(1)?,
                        name: row.get(2)?,
                        trigger_pattern_json: row.get(3)?,
                        mitigation_prompt: row.get(4)?,
                        severity: row.get(5)?,
                        occurrence_count: row.get::<_, i64>(6)?.max(0) as u64,
                    })
                })?;
                rows.collect::<std::result::Result<Vec<_>, _>>().map_err(Into::into)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn replace_workflow_profiles(
        &self,
        model_id: i64,
        rows: &[WorkflowProfileRow],
    ) -> Result<()> {
        let rows = rows.to_vec();
        self.conn
            .call(move |conn| {
                let tx = conn.transaction()?;
                tx.execute(
                    "UPDATE workflow_profiles SET deleted_at = ?2 WHERE model_id = ?1 AND deleted_at IS NULL",
                    params![model_id, now_ts() as i64],
                )?;
                for row in rows {
                    tx.execute(
                        "INSERT INTO workflow_profiles (model_id, name, avg_success_rate, avg_steps, typical_tools_json, deleted_at) VALUES (?1, ?2, ?3, ?4, ?5, NULL)",
                        params![
                            row.model_id,
                            row.name,
                            row.avg_success_rate,
                            row.avg_steps as i64,
                            row.typical_tools_json,
                        ],
                    )?;
                }
                tx.commit()?;
                Ok(())
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn list_workflow_profiles(&self, model_id: i64) -> Result<Vec<WorkflowProfileRow>> {
        self.conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT id, model_id, name, avg_success_rate, avg_steps, typical_tools_json FROM workflow_profiles WHERE model_id = ?1 AND deleted_at IS NULL ORDER BY avg_success_rate DESC, id ASC",
                )?;
                let rows = stmt.query_map(params![model_id], |row| {
                    Ok(WorkflowProfileRow {
                        id: row.get(0)?,
                        model_id: row.get(1)?,
                        name: row.get(2)?,
                        avg_success_rate: row.get(3)?,
                        avg_steps: row.get::<_, i64>(4)?.max(0) as u64,
                        typical_tools_json: row.get(5)?,
                    })
                })?;
                rows.collect::<std::result::Result<Vec<_>, _>>().map_err(Into::into)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }
}
