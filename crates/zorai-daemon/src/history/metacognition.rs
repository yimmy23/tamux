use super::*;

fn map_meta_cognition_model_row(row: &db::Row) -> anyhow::Result<MetaCognitionModelRow> {
    Ok(MetaCognitionModelRow {
        id: row.get(0)?,
        agent_id: row.get(1)?,
        calibration_offset: row.get(2)?,
        last_updated_at: row.get::<i64>(3)?.max(0) as u64,
    })
}

fn map_cognitive_bias_row(row: &db::Row) -> anyhow::Result<CognitiveBiasRow> {
    Ok(CognitiveBiasRow {
        id: row.get(0)?,
        model_id: row.get(1)?,
        name: row.get(2)?,
        trigger_pattern_json: row.get(3)?,
        mitigation_prompt: row.get(4)?,
        severity: row.get(5)?,
        occurrence_count: row.get::<i64>(6)?.max(0) as u64,
    })
}

fn map_workflow_profile_row(row: &db::Row) -> anyhow::Result<WorkflowProfileRow> {
    Ok(WorkflowProfileRow {
        id: row.get(0)?,
        model_id: row.get(1)?,
        name: row.get(2)?,
        avg_success_rate: row.get(3)?,
        avg_steps: row.get::<i64>(4)?.max(0) as u64,
        typical_tools_json: row.get(5)?,
    })
}

impl HistoryStore {
    pub async fn upsert_meta_cognition_model(
        &self,
        agent_id: &str,
        calibration_offset: f64,
        last_updated_at: u64,
    ) -> Result<()> {
        self.conn_db
            .execute(
                "INSERT OR REPLACE INTO meta_cognition_model (id, agent_id, calibration_offset, last_updated_at) VALUES (1, ?1, ?2, ?3)",
                db::db_params![agent_id, calibration_offset, last_updated_at as i64],
            )
            .await?;
        Ok(())
    }

    pub async fn get_meta_cognition_model(&self) -> Result<Option<MetaCognitionModelRow>> {
        let row = self
            .conn_db
            .query_opt(
                "SELECT id, agent_id, calibration_offset, last_updated_at FROM meta_cognition_model WHERE id = 1",
                db::Params::None,
            )
            .await?;
        row.map(|row| map_meta_cognition_model_row(&row))
            .transpose()
    }

    pub async fn replace_cognitive_biases(
        &self,
        model_id: i64,
        rows: &[CognitiveBiasRow],
    ) -> Result<()> {
        let rows = rows.to_vec();
        let mut txn = self.conn_db.transaction().await?;
        txn.execute(
            "UPDATE cognitive_biases SET deleted_at = ?2 WHERE model_id = ?1 AND deleted_at IS NULL",
            db::db_params![model_id, now_ts() as i64],
        )
        .await?;
        for row in rows {
            txn.execute(
                "INSERT INTO cognitive_biases (model_id, name, trigger_pattern_json, mitigation_prompt, severity, occurrence_count, deleted_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, NULL)",
                db::db_params![
                    row.model_id,
                    row.name,
                    row.trigger_pattern_json,
                    row.mitigation_prompt,
                    row.severity,
                    row.occurrence_count as i64,
                ],
            )
            .await?;
        }
        txn.commit().await?;
        Ok(())
    }

    pub async fn list_cognitive_biases(&self, model_id: i64) -> Result<Vec<CognitiveBiasRow>> {
        let rows = self
            .conn_db
            .query(
                "SELECT id, model_id, name, trigger_pattern_json, mitigation_prompt, severity, occurrence_count FROM cognitive_biases WHERE model_id = ?1 AND deleted_at IS NULL ORDER BY severity DESC, id ASC",
                db::db_params![model_id],
            )
            .await?;
        rows.iter().map(map_cognitive_bias_row).collect()
    }

    pub async fn replace_workflow_profiles(
        &self,
        model_id: i64,
        rows: &[WorkflowProfileRow],
    ) -> Result<()> {
        let rows = rows.to_vec();
        let mut txn = self.conn_db.transaction().await?;
        txn.execute(
            "UPDATE workflow_profiles SET deleted_at = ?2 WHERE model_id = ?1 AND deleted_at IS NULL",
            db::db_params![model_id, now_ts() as i64],
        )
        .await?;
        for row in rows {
            txn.execute(
                "INSERT INTO workflow_profiles (model_id, name, avg_success_rate, avg_steps, typical_tools_json, deleted_at) VALUES (?1, ?2, ?3, ?4, ?5, NULL)",
                db::db_params![
                    row.model_id,
                    row.name,
                    row.avg_success_rate,
                    row.avg_steps as i64,
                    row.typical_tools_json,
                ],
            )
            .await?;
        }
        txn.commit().await?;
        Ok(())
    }

    pub async fn list_workflow_profiles(&self, model_id: i64) -> Result<Vec<WorkflowProfileRow>> {
        let rows = self
            .conn_db
            .query(
                "SELECT id, model_id, name, avg_success_rate, avg_steps, typical_tools_json FROM workflow_profiles WHERE model_id = ?1 AND deleted_at IS NULL ORDER BY avg_success_rate DESC, id ASC",
                db::db_params![model_id],
            )
            .await?;
        rows.iter().map(map_workflow_profile_row).collect()
    }
}
