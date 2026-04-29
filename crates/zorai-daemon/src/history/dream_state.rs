use super::*;

impl HistoryStore {
    pub async fn insert_dream_cycle(&self, row: &DreamCycleRow) -> Result<i64> {
        let row = row.clone();
        self.conn
            .call(move |conn| {
                conn.execute(
                    "INSERT INTO dream_cycles (started_at_ms, completed_at_ms, idle_duration_ms, tasks_analyzed, counterfactuals_generated, counterfactuals_successful, status) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                    params![
                        row.started_at_ms as i64,
                        row.completed_at_ms.map(|value| value as i64),
                        row.idle_duration_ms as i64,
                        row.tasks_analyzed as i64,
                        row.counterfactuals_generated as i64,
                        row.counterfactuals_successful as i64,
                        row.status,
                    ],
                )?;
                Ok(conn.last_insert_rowid())
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn list_dream_cycles(&self, limit: usize) -> Result<Vec<DreamCycleRow>> {
        let limit = limit.max(1) as i64;
        self.read_conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT id, started_at_ms, completed_at_ms, idle_duration_ms, tasks_analyzed, counterfactuals_generated, counterfactuals_successful, status
                     FROM dream_cycles
                     ORDER BY started_at_ms DESC, id DESC
                     LIMIT ?1",
                )?;
                let rows = stmt.query_map(params![limit], |row| {
                    Ok(DreamCycleRow {
                        id: Some(row.get(0)?),
                        started_at_ms: row.get::<_, i64>(1)?.max(0) as u64,
                        completed_at_ms: row
                            .get::<_, Option<i64>>(2)?
                            .map(|value| value.max(0) as u64),
                        idle_duration_ms: row.get::<_, i64>(3)?.max(0) as u64,
                        tasks_analyzed: row.get::<_, i64>(4)?.max(0) as u64,
                        counterfactuals_generated: row.get::<_, i64>(5)?.max(0) as u64,
                        counterfactuals_successful: row.get::<_, i64>(6)?.max(0) as u64,
                        status: row.get(7)?,
                    })
                })?;
                rows.collect::<std::result::Result<Vec<_>, _>>()
                    .map_err(Into::into)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn finish_dream_cycle(
        &self,
        cycle_id: i64,
        completed_at_ms: u64,
        tasks_analyzed: u64,
        counterfactuals_generated: u64,
        counterfactuals_successful: u64,
        status: &str,
    ) -> Result<()> {
        let status = status.to_string();
        self.conn
            .call(move |conn| {
                conn.execute(
                    "UPDATE dream_cycles
                     SET completed_at_ms = ?2,
                         tasks_analyzed = ?3,
                         counterfactuals_generated = ?4,
                         counterfactuals_successful = ?5,
                         status = ?6
                     WHERE id = ?1",
                    params![
                        cycle_id,
                        completed_at_ms as i64,
                        tasks_analyzed as i64,
                        counterfactuals_generated as i64,
                        counterfactuals_successful as i64,
                        status,
                    ],
                )?;
                Ok(())
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn insert_counterfactual_evaluation(
        &self,
        row: &CounterfactualEvaluationRow,
    ) -> Result<i64> {
        let row = row.clone();
        let inserted_id = self
            .conn
            .call(move |conn| {
                conn.execute(
                    "INSERT INTO counterfactual_evaluations (dream_cycle_id, source_task_id, variation_type, counterfactual_description, estimated_token_saving, estimated_time_saving_ms, estimated_revision_reduction, score, threshold_met, created_at_ms) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                    params![
                        row.dream_cycle_id,
                        row.source_task_id,
                        row.variation_type,
                        row.counterfactual_description,
                        row.estimated_token_saving,
                        row.estimated_time_saving_ms,
                        row.estimated_revision_reduction.map(|value| value as i64),
                        row.score,
                        if row.threshold_met { 1i64 } else { 0i64 },
                        row.created_at_ms as i64,
                    ],
                )?;
                Ok(conn.last_insert_rowid())
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        Ok(inserted_id)
    }

    pub async fn list_counterfactual_evaluations(
        &self,
        dream_cycle_id: i64,
    ) -> Result<Vec<CounterfactualEvaluationRow>> {
        self.read_conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT id, dream_cycle_id, source_task_id, variation_type, counterfactual_description, estimated_token_saving, estimated_time_saving_ms, estimated_revision_reduction, score, threshold_met, created_at_ms
                     FROM counterfactual_evaluations
                     WHERE dream_cycle_id = ?1
                     ORDER BY created_at_ms DESC, id DESC",
                )?;
                let rows = stmt.query_map(params![dream_cycle_id], |row| {
                    Ok(CounterfactualEvaluationRow {
                        id: Some(row.get(0)?),
                        dream_cycle_id: row.get(1)?,
                        source_task_id: row.get(2)?,
                        variation_type: row.get(3)?,
                        counterfactual_description: row.get(4)?,
                        estimated_token_saving: row.get(5)?,
                        estimated_time_saving_ms: row.get(6)?,
                        estimated_revision_reduction: row
                            .get::<_, Option<i64>>(7)?
                            .map(|value| value.max(0) as u64),
                        score: row.get(8)?,
                        threshold_met: row.get::<_, i64>(9)? != 0,
                        created_at_ms: row.get::<_, i64>(10)?.max(0) as u64,
                    })
                })?;
                rows.collect::<std::result::Result<Vec<_>, _>>()
                    .map_err(Into::into)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }
}
