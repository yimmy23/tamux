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
        let rows_for_index = rows.clone();
        self.conn
            .call(move |conn| {
                let tx = conn.transaction()?;
                tx.execute("DELETE FROM cognitive_biases WHERE model_id = ?1", params![model_id])?;
                for row in rows {
                    tx.execute(
                        "INSERT INTO cognitive_biases (model_id, name, trigger_pattern_json, mitigation_prompt, severity, occurrence_count) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
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
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        for row in rows_for_index {
            let bias_name = row.name.clone();
            self.upsert_search_document(super::search_index::SearchDocument {
                source_kind: super::search_index::SearchSourceKind::MetaCognition,
                source_id: format!("cognitive_bias:{model_id}:{bias_name}"),
                title: format!("cognitive bias {bias_name}"),
                body: format!("{}\n{}", row.trigger_pattern_json, row.mitigation_prompt),
                tags: vec!["cognitive_bias".to_string(), bias_name],
                workspace_id: None,
                thread_id: None,
                agent_id: None,
                timestamp: 0,
                metadata_json: Some(
                    serde_json::to_string(&serde_json::json!({
                        "model_id": row.model_id,
                        "severity": row.severity,
                        "occurrence_count": row.occurrence_count,
                    }))
                    .unwrap_or_else(|_| "{}".to_string()),
                ),
            });
        }
        Ok(())
    }

    pub async fn list_cognitive_biases(&self, model_id: i64) -> Result<Vec<CognitiveBiasRow>> {
        self.conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT id, model_id, name, trigger_pattern_json, mitigation_prompt, severity, occurrence_count FROM cognitive_biases WHERE model_id = ?1 ORDER BY severity DESC, id ASC",
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
                tx.execute("DELETE FROM workflow_profiles WHERE model_id = ?1", params![model_id])?;
                for row in rows {
                    tx.execute(
                        "INSERT INTO workflow_profiles (model_id, name, avg_success_rate, avg_steps, typical_tools_json) VALUES (?1, ?2, ?3, ?4, ?5)",
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
                    "SELECT id, model_id, name, avg_success_rate, avg_steps, typical_tools_json FROM workflow_profiles WHERE model_id = ?1 ORDER BY avg_success_rate DESC, id ASC",
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
