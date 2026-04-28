use super::*;

impl HistoryStore {
    pub async fn insert_temporal_pattern(&self, row: &TemporalPatternRow) -> Result<i64> {
        let row = row.clone();
        self.conn
            .call(move |conn| {
                conn.execute(
                    "INSERT INTO temporal_patterns (pattern_type, timescale, pattern_description, context_filter, frequency, last_observed_ms, first_observed_ms, confidence, decay_rate, created_at_ms) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                    params![
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
                )?;
                Ok(conn.last_insert_rowid())
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn list_temporal_patterns(
        &self,
        pattern_type: &str,
        limit: usize,
    ) -> Result<Vec<TemporalPatternRow>> {
        let pattern_type = pattern_type.to_string();
        let limit = limit.max(1) as i64;
        self.read_conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT id, pattern_type, timescale, pattern_description, context_filter, frequency, last_observed_ms, first_observed_ms, confidence, decay_rate, created_at_ms
                     FROM temporal_patterns
                     WHERE pattern_type = ?1
                     ORDER BY confidence DESC, created_at_ms DESC, id DESC
                     LIMIT ?2",
                )?;
                let rows = stmt.query_map(params![pattern_type, limit], |row| {
                    Ok(TemporalPatternRow {
                        id: Some(row.get(0)?),
                        pattern_type: row.get(1)?,
                        timescale: row.get(2)?,
                        pattern_description: row.get(3)?,
                        context_filter: row.get(4)?,
                        frequency: row.get::<_, i64>(5)?.max(0) as u64,
                        last_observed_ms: row.get::<_, i64>(6)?.max(0) as u64,
                        first_observed_ms: row.get::<_, i64>(7)?.max(0) as u64,
                        confidence: row.get(8)?,
                        decay_rate: row.get(9)?,
                        created_at_ms: row.get::<_, i64>(10)?.max(0) as u64,
                    })
                })?;
                rows.collect::<std::result::Result<Vec<_>, _>>()
                    .map_err(Into::into)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn insert_temporal_prediction(&self, row: &TemporalPredictionRow) -> Result<i64> {
        let row = row.clone();
        self.conn
            .call(move |conn| {
                conn.execute(
                    "INSERT INTO temporal_predictions (pattern_id, predicted_action, predicted_at_ms, confidence, actual_action, was_accepted, accuracy_score) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                    params![
                        row.pattern_id,
                        row.predicted_action,
                        row.predicted_at_ms as i64,
                        row.confidence,
                        row.actual_action,
                        row.was_accepted.map(|value| if value { 1i64 } else { 0i64 }),
                        row.accuracy_score,
                    ],
                )?;
                Ok(conn.last_insert_rowid())
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn list_temporal_predictions(
        &self,
        pattern_id: i64,
        limit: usize,
    ) -> Result<Vec<TemporalPredictionRow>> {
        let limit = limit.max(1) as i64;
        self.read_conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT id, pattern_id, predicted_action, predicted_at_ms, confidence, actual_action, was_accepted, accuracy_score
                     FROM temporal_predictions
                     WHERE pattern_id = ?1
                     ORDER BY predicted_at_ms DESC, id DESC
                     LIMIT ?2",
                )?;
                let rows = stmt.query_map(params![pattern_id, limit], |row| {
                    Ok(TemporalPredictionRow {
                        id: Some(row.get(0)?),
                        pattern_id: row.get(1)?,
                        predicted_action: row.get(2)?,
                        predicted_at_ms: row.get::<_, i64>(3)?.max(0) as u64,
                        confidence: row.get(4)?,
                        actual_action: row.get(5)?,
                        was_accepted: row.get::<_, Option<i64>>(6)?.map(|value| value != 0),
                        accuracy_score: row.get(7)?,
                    })
                })?;
                rows.collect::<std::result::Result<Vec<_>, _>>()
                    .map_err(Into::into)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn insert_precomputation_log(&self, row: &PrecomputationLogRow) -> Result<i64> {
        let row = row.clone();
        self.conn
            .call(move |conn| {
                conn.execute(
                    "INSERT INTO precomputation_log (prediction_id, precomputation_type, precomputation_details, started_at_ms, completed_at_ms, was_used) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    params![
                        row.prediction_id,
                        row.precomputation_type,
                        row.precomputation_details,
                        row.started_at_ms as i64,
                        row.completed_at_ms.map(|value| value as i64),
                        row.was_used.map(|value| if value { 1i64 } else { 0i64 }),
                    ],
                )?;
                Ok(conn.last_insert_rowid())
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn list_precomputation_log(
        &self,
        prediction_id: i64,
    ) -> Result<Vec<PrecomputationLogRow>> {
        self.read_conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT id, prediction_id, precomputation_type, precomputation_details, started_at_ms, completed_at_ms, was_used
                     FROM precomputation_log
                     WHERE prediction_id = ?1
                     ORDER BY started_at_ms DESC, id DESC",
                )?;
                let rows = stmt.query_map(params![prediction_id], |row| {
                    Ok(PrecomputationLogRow {
                        id: Some(row.get(0)?),
                        prediction_id: row.get(1)?,
                        precomputation_type: row.get(2)?,
                        precomputation_details: row.get(3)?,
                        started_at_ms: row.get::<_, i64>(4)?.max(0) as u64,
                        completed_at_ms: row
                            .get::<_, Option<i64>>(5)?
                            .map(|value| value.max(0) as u64),
                        was_used: row.get::<_, Option<i64>>(6)?.map(|value| value != 0),
                    })
                })?;
                rows.collect::<std::result::Result<Vec<_>, _>>()
                    .map_err(Into::into)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn update_precomputation_usage(
        &self,
        precomputation_id: i64,
        was_used: bool,
        completed_at_ms: u64,
    ) -> Result<()> {
        self.conn
            .call(move |conn| {
                conn.execute(
                    "UPDATE precomputation_log
                     SET was_used = ?2,
                         completed_at_ms = COALESCE(completed_at_ms, ?3)
                     WHERE id = ?1",
                    params![
                        precomputation_id,
                        if was_used { 1i64 } else { 0i64 },
                        completed_at_ms as i64,
                    ],
                )?;
                Ok(())
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn upsert_intent_model(&self, row: &IntentModelRow) -> Result<()> {
        let row = row.clone();
        self.conn
            .call(move |conn| {
                conn.execute(
                    "INSERT INTO intent_models (agent_id, model_blob, created_at_ms, accuracy_score) VALUES (?1, ?2, ?3, ?4)
                     ON CONFLICT(agent_id) DO UPDATE SET model_blob = excluded.model_blob, created_at_ms = excluded.created_at_ms, accuracy_score = excluded.accuracy_score",
                    params![
                        row.agent_id,
                        row.model_blob,
                        row.created_at_ms as i64,
                        row.accuracy_score,
                    ],
                )?;
                Ok(())
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn get_intent_model(&self, agent_id: &str) -> Result<Option<IntentModelRow>> {
        let agent_id = agent_id.to_string();
        self.read_conn
            .call(move |conn| {
                Ok(conn
                    .query_row(
                        "SELECT id, agent_id, model_blob, created_at_ms, accuracy_score
                     FROM intent_models
                     WHERE agent_id = ?1",
                        params![agent_id],
                        |row| {
                            Ok(IntentModelRow {
                                id: Some(row.get(0)?),
                                agent_id: row.get(1)?,
                                model_blob: row.get(2)?,
                                created_at_ms: row.get::<_, i64>(3)?.max(0) as u64,
                                accuracy_score: row.get(4)?,
                            })
                        },
                    )
                    .optional()?)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }
}
