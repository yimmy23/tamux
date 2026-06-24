use super::*;

fn map_temporal_pattern_row(row: &db::Row) -> anyhow::Result<TemporalPatternRow> {
    Ok(TemporalPatternRow {
        id: Some(row.get(0)?),
        pattern_type: row.get(1)?,
        timescale: row.get(2)?,
        pattern_description: row.get(3)?,
        context_filter: row.get(4)?,
        frequency: row.get::<i64>(5)?.max(0) as u64,
        last_observed_ms: row.get::<i64>(6)?.max(0) as u64,
        first_observed_ms: row.get::<i64>(7)?.max(0) as u64,
        confidence: row.get(8)?,
        decay_rate: row.get(9)?,
        created_at_ms: row.get::<i64>(10)?.max(0) as u64,
    })
}

fn map_temporal_prediction_row(row: &db::Row) -> anyhow::Result<TemporalPredictionRow> {
    Ok(TemporalPredictionRow {
        id: Some(row.get(0)?),
        pattern_id: row.get(1)?,
        predicted_action: row.get(2)?,
        predicted_at_ms: row.get::<i64>(3)?.max(0) as u64,
        confidence: row.get(4)?,
        actual_action: row.get(5)?,
        was_accepted: row.get::<Option<i64>>(6)?.map(|value| value != 0),
        accuracy_score: row.get(7)?,
    })
}

fn map_precomputation_log_row(row: &db::Row) -> anyhow::Result<PrecomputationLogRow> {
    Ok(PrecomputationLogRow {
        id: Some(row.get(0)?),
        prediction_id: row.get(1)?,
        precomputation_type: row.get(2)?,
        precomputation_details: row.get(3)?,
        started_at_ms: row.get::<i64>(4)?.max(0) as u64,
        completed_at_ms: row.get::<Option<i64>>(5)?.map(|value| value.max(0) as u64),
        was_used: row.get::<Option<i64>>(6)?.map(|value| value != 0),
    })
}

fn map_intent_model_row(row: &db::Row) -> anyhow::Result<IntentModelRow> {
    Ok(IntentModelRow {
        id: Some(row.get(0)?),
        agent_id: row.get(1)?,
        model_blob: row.get(2)?,
        created_at_ms: row.get::<i64>(3)?.max(0) as u64,
        accuracy_score: row.get(4)?,
    })
}

impl HistoryStore {
    pub async fn insert_temporal_pattern(&self, row: &TemporalPatternRow) -> Result<i64> {
        let row = row.clone();
        self.conn_db
            .execute_returning_rowid(
                "INSERT INTO temporal_patterns (pattern_type, timescale, pattern_description, context_filter, frequency, last_observed_ms, first_observed_ms, confidence, decay_rate, created_at_ms) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                db::db_params![
                    row.pattern_type,
                    row.timescale,
                    row.pattern_description,
                    row.context_filter,
                    row.frequency as i64,
                    row.last_observed_ms as i64,
                    row.first_observed_ms as i64,
                    row.confidence,
                    row.decay_rate,
                    row.created_at_ms as i64,
                ],
            )
            .await
    }

    pub async fn list_temporal_patterns(
        &self,
        pattern_type: &str,
        limit: usize,
    ) -> Result<Vec<TemporalPatternRow>> {
        let limit = limit.max(1) as i64;
        let rows = self
            .read_db
            .query(
                "SELECT id, pattern_type, timescale, pattern_description, context_filter, frequency, last_observed_ms, first_observed_ms, confidence, decay_rate, created_at_ms
                 FROM temporal_patterns
                 WHERE pattern_type = ?1
                 ORDER BY confidence DESC, created_at_ms DESC, id DESC
                 LIMIT ?2",
                db::db_params![pattern_type, limit],
            )
            .await?;
        rows.iter().map(map_temporal_pattern_row).collect()
    }

    pub async fn insert_temporal_prediction(&self, row: &TemporalPredictionRow) -> Result<i64> {
        let row = row.clone();
        self.conn_db
            .execute_returning_rowid(
                "INSERT INTO temporal_predictions (pattern_id, predicted_action, predicted_at_ms, confidence, actual_action, was_accepted, accuracy_score) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                db::db_params![
                    row.pattern_id,
                    row.predicted_action,
                    row.predicted_at_ms as i64,
                    row.confidence,
                    row.actual_action,
                    row.was_accepted.map(|value| if value { 1i64 } else { 0i64 }),
                    row.accuracy_score,
                ],
            )
            .await
    }

    pub async fn list_temporal_predictions(
        &self,
        pattern_id: i64,
        limit: usize,
    ) -> Result<Vec<TemporalPredictionRow>> {
        let limit = limit.max(1) as i64;
        let rows = self
            .read_db
            .query(
                "SELECT id, pattern_id, predicted_action, predicted_at_ms, confidence, actual_action, was_accepted, accuracy_score
                 FROM temporal_predictions
                 WHERE pattern_id = ?1
                 ORDER BY predicted_at_ms DESC, id DESC
                 LIMIT ?2",
                db::db_params![pattern_id, limit],
            )
            .await?;
        rows.iter().map(map_temporal_prediction_row).collect()
    }

    pub async fn insert_precomputation_log(&self, row: &PrecomputationLogRow) -> Result<i64> {
        let row = row.clone();
        self.conn_db
            .execute_returning_rowid(
                "INSERT INTO precomputation_log (prediction_id, precomputation_type, precomputation_details, started_at_ms, completed_at_ms, was_used) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                db::db_params![
                    row.prediction_id,
                    row.precomputation_type,
                    row.precomputation_details,
                    row.started_at_ms as i64,
                    row.completed_at_ms.map(|value| value as i64),
                    row.was_used.map(|value| if value { 1i64 } else { 0i64 }),
                ],
            )
            .await
    }

    pub async fn list_precomputation_log(
        &self,
        prediction_id: i64,
    ) -> Result<Vec<PrecomputationLogRow>> {
        let rows = self
            .read_db
            .query(
                "SELECT id, prediction_id, precomputation_type, precomputation_details, started_at_ms, completed_at_ms, was_used
                 FROM precomputation_log
                 WHERE prediction_id = ?1
                 ORDER BY started_at_ms DESC, id DESC",
                db::db_params![prediction_id],
            )
            .await?;
        rows.iter().map(map_precomputation_log_row).collect()
    }

    pub async fn update_precomputation_usage(
        &self,
        precomputation_id: i64,
        was_used: bool,
        completed_at_ms: u64,
    ) -> Result<()> {
        self.conn_db
            .execute(
                "UPDATE precomputation_log
                     SET was_used = ?2,
                         completed_at_ms = COALESCE(completed_at_ms, ?3)
                     WHERE id = ?1",
                db::db_params![
                    precomputation_id,
                    if was_used { 1i64 } else { 0i64 },
                    completed_at_ms as i64,
                ],
            )
            .await?;
        Ok(())
    }

    pub async fn upsert_intent_model(&self, row: &IntentModelRow) -> Result<()> {
        let row = row.clone();
        self.conn_db
            .execute(
                "INSERT INTO intent_models (agent_id, model_blob, created_at_ms, accuracy_score) VALUES (?1, ?2, ?3, ?4)
                     ON CONFLICT(agent_id) DO UPDATE SET model_blob = excluded.model_blob, created_at_ms = excluded.created_at_ms, accuracy_score = excluded.accuracy_score",
                db::db_params![
                    row.agent_id,
                    row.model_blob,
                    row.created_at_ms as i64,
                    row.accuracy_score,
                ],
            )
            .await?;
        Ok(())
    }

    pub async fn get_intent_model(&self, agent_id: &str) -> Result<Option<IntentModelRow>> {
        let row = self
            .read_db
            .query_opt(
                "SELECT id, agent_id, model_blob, created_at_ms, accuracy_score
                     FROM intent_models
                     WHERE agent_id = ?1",
                db::db_params![agent_id],
            )
            .await?;
        row.map(|row| map_intent_model_row(&row)).transpose()
    }
}
