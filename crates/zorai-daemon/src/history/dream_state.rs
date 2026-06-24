use super::*;

impl HistoryStore {
    pub async fn insert_dream_cycle(&self, row: &DreamCycleRow) -> Result<i64> {
        self.conn_db
            .execute_returning_rowid(
                "INSERT INTO dream_cycles (started_at_ms, completed_at_ms, idle_duration_ms, tasks_analyzed, counterfactuals_generated, counterfactuals_successful, status) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                db::db_params![
                    row.started_at_ms as i64,
                    row.completed_at_ms.map(|value| value as i64),
                    row.idle_duration_ms as i64,
                    row.tasks_analyzed as i64,
                    row.counterfactuals_generated as i64,
                    row.counterfactuals_successful as i64,
                    row.status.clone(),
                ],
            )
            .await
    }

    pub async fn list_dream_cycles(&self, limit: usize) -> Result<Vec<DreamCycleRow>> {
        let limit = limit.max(1) as i64;
        let rows = self
            .read_db
            .query(
                "SELECT id, started_at_ms, completed_at_ms, idle_duration_ms, tasks_analyzed, counterfactuals_generated, counterfactuals_successful, status
                 FROM dream_cycles
                 ORDER BY started_at_ms DESC, id DESC
                 LIMIT ?1",
                db::db_params![limit],
            )
            .await?;
        rows.iter().map(map_dream_cycle_row).collect()
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
        self.conn_db
            .execute(
                "UPDATE dream_cycles
                     SET completed_at_ms = ?2,
                         tasks_analyzed = ?3,
                         counterfactuals_generated = ?4,
                         counterfactuals_successful = ?5,
                         status = ?6
                     WHERE id = ?1",
                db::db_params![
                    cycle_id,
                    completed_at_ms as i64,
                    tasks_analyzed as i64,
                    counterfactuals_generated as i64,
                    counterfactuals_successful as i64,
                    status,
                ],
            )
            .await?;
        Ok(())
    }

    pub async fn insert_counterfactual_evaluation(
        &self,
        row: &CounterfactualEvaluationRow,
    ) -> Result<i64> {
        self.conn_db
            .execute_returning_rowid(
                "INSERT INTO counterfactual_evaluations (dream_cycle_id, source_task_id, variation_type, counterfactual_description, estimated_token_saving, estimated_time_saving_ms, estimated_revision_reduction, score, threshold_met, created_at_ms) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                db::db_params![
                    row.dream_cycle_id,
                    row.source_task_id.clone(),
                    row.variation_type.clone(),
                    row.counterfactual_description.clone(),
                    row.estimated_token_saving,
                    row.estimated_time_saving_ms,
                    row.estimated_revision_reduction.map(|value| value as i64),
                    row.score,
                    if row.threshold_met { 1i64 } else { 0i64 },
                    row.created_at_ms as i64,
                ],
            )
            .await
    }

    pub async fn list_counterfactual_evaluations(
        &self,
        dream_cycle_id: i64,
    ) -> Result<Vec<CounterfactualEvaluationRow>> {
        self.list_counterfactual_evaluations_maybe_limited(dream_cycle_id, None)
            .await
    }

    pub async fn list_counterfactual_evaluations_limited(
        &self,
        dream_cycle_id: i64,
        limit: usize,
    ) -> Result<Vec<CounterfactualEvaluationRow>> {
        self.list_counterfactual_evaluations_maybe_limited(dream_cycle_id, Some(limit))
            .await
    }

    async fn list_counterfactual_evaluations_maybe_limited(
        &self,
        dream_cycle_id: i64,
        limit: Option<usize>,
    ) -> Result<Vec<CounterfactualEvaluationRow>> {
        let limit = limit.map(|value| value.max(1) as i64);
        let mut sql = String::from(
            "SELECT id, dream_cycle_id, source_task_id, variation_type, counterfactual_description, estimated_token_saving, estimated_time_saving_ms, estimated_revision_reduction, score, threshold_met, created_at_ms
                 FROM counterfactual_evaluations
                 WHERE dream_cycle_id = ?1
                 ORDER BY created_at_ms DESC, id DESC",
        );
        let mut values = vec![db::Value::Integer(dream_cycle_id)];
        if let Some(limit) = limit {
            sql.push_str(" LIMIT ?2");
            values.push(db::Value::Integer(limit));
        }
        let rows = self
            .read_db
            .query(&sql, db::Params::Positional(values))
            .await?;
        rows.iter().map(map_counterfactual_evaluation_row).collect()
    }
}
