use super::*;

fn map_implicit_signal_row(row: &db::Row) -> anyhow::Result<ImplicitSignalRow> {
    Ok(ImplicitSignalRow {
        id: row.get(0)?,
        session_id: row.get(1)?,
        signal_type: row.get(2)?,
        weight: row.get(3)?,
        timestamp_ms: row.get::<i64>(4)?.max(0) as u64,
        context_snapshot_json: row.get(5)?,
    })
}

fn map_intent_prediction_row(row: &db::Row) -> anyhow::Result<IntentPredictionRow> {
    Ok(IntentPredictionRow {
        id: row.get(0)?,
        session_id: row.get(1)?,
        context_state_hash: row.get(2)?,
        predicted_action: row.get(3)?,
        confidence: row.get(4)?,
        actual_action: row.get(5)?,
        was_correct: row.get::<Option<i64>>(6)?.map(|value| value != 0),
        created_at_ms: row.get::<i64>(7)?.max(0) as u64,
    })
}

fn map_system_outcome_prediction_row(row: &db::Row) -> anyhow::Result<SystemOutcomePredictionRow> {
    Ok(SystemOutcomePredictionRow {
        id: row.get(0)?,
        session_id: row.get(1)?,
        prediction_type: row.get(2)?,
        predicted_outcome: row.get(3)?,
        confidence: row.get(4)?,
        actual_outcome: row.get(5)?,
        was_correct: row.get::<Option<i64>>(6)?.map(|value| value != 0),
        created_at_ms: row.get::<i64>(7)?.max(0) as u64,
    })
}

fn map_satisfaction_score_row(row: &db::Row) -> anyhow::Result<SatisfactionScoreRow> {
    Ok(SatisfactionScoreRow {
        id: row.get(0)?,
        session_id: row.get(1)?,
        score: row.get(2)?,
        computed_at_ms: row.get::<i64>(3)?.max(0) as u64,
        label: row.get(4)?,
        signal_count: row.get::<i64>(5)?.max(0) as u64,
    })
}

impl HistoryStore {
    pub async fn insert_implicit_signal(&self, row: &ImplicitSignalRow) -> Result<()> {
        let row = row.clone();
        self.conn_db
            .execute(
                "INSERT INTO implicit_signals (id, session_id, signal_type, weight, timestamp_ms, context_snapshot_json) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                db::db_params![
                    row.id,
                    row.session_id,
                    row.signal_type,
                    row.weight,
                    row.timestamp_ms as i64,
                    row.context_snapshot_json,
                ],
            )
            .await?;
        Ok(())
    }

    pub async fn list_implicit_signals(
        &self,
        session_id: &str,
        limit: usize,
    ) -> Result<Vec<ImplicitSignalRow>> {
        let rows = self
            .read_db
            .query(
                "SELECT id, session_id, signal_type, weight, timestamp_ms, context_snapshot_json
                 FROM implicit_signals
                 WHERE session_id = ?1
                 ORDER BY timestamp_ms DESC, id DESC
                 LIMIT ?2",
                db::db_params![session_id, limit.max(1) as i64],
            )
            .await?;
        rows.iter().map(map_implicit_signal_row).collect()
    }

    pub async fn implicit_signal_exists(
        &self,
        session_id: &str,
        signal_type: &str,
    ) -> Result<bool> {
        let row = self
            .read_db
            .query_opt(
                "SELECT EXISTS(
                     SELECT 1
                     FROM implicit_signals
                     WHERE session_id = ?1
                       AND signal_type = ?2
                 )",
                db::db_params![session_id, signal_type],
            )
            .await?;
        Ok(match row {
            Some(row) => row.get::<i64>(0)? != 0,
            None => false,
        })
    }

    pub async fn latest_implicit_signal_by_types(
        &self,
        session_id: &str,
        signal_types: &[&str],
    ) -> Result<Option<ImplicitSignalRow>> {
        if signal_types.is_empty() {
            return Ok(None);
        }

        let placeholders = std::iter::repeat("?")
            .take(signal_types.len())
            .collect::<Vec<_>>()
            .join(", ");
        let mut values = Vec::<db::Value>::with_capacity(signal_types.len().saturating_add(1));
        values.push(db::Value::Text(session_id.to_string()));
        values.extend(
            signal_types
                .iter()
                .map(|signal_type| db::Value::Text(signal_type.to_string())),
        );

        let row = self
            .read_db
            .query_opt(
                &format!(
                    "SELECT id, session_id, signal_type, weight, timestamp_ms, context_snapshot_json
                     FROM implicit_signals
                     WHERE session_id = ?
                       AND signal_type IN ({placeholders})
                     ORDER BY timestamp_ms DESC, id DESC
                     LIMIT 1"
                ),
                db::Params::Positional(values),
            )
            .await?;
        row.map(|row| map_implicit_signal_row(&row)).transpose()
    }

    pub async fn list_implicit_signals_by_type(
        &self,
        session_id: &str,
        signal_type: &str,
        limit: usize,
    ) -> Result<Vec<ImplicitSignalRow>> {
        let rows = self
            .read_db
            .query(
                "SELECT id, session_id, signal_type, weight, timestamp_ms, context_snapshot_json
                 FROM implicit_signals
                 WHERE session_id = ?1
                   AND signal_type = ?2
                 ORDER BY timestamp_ms DESC, id DESC
                 LIMIT ?3",
                db::db_params![session_id, signal_type, limit.max(1) as i64],
            )
            .await?;
        rows.iter().map(map_implicit_signal_row).collect()
    }

    pub async fn insert_satisfaction_score(&self, row: &SatisfactionScoreRow) -> Result<()> {
        let row = row.clone();
        self.conn_db
            .execute(
                "INSERT INTO satisfaction_scores (id, session_id, score, computed_at_ms, label, signal_count) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                db::db_params![
                    row.id,
                    row.session_id,
                    row.score,
                    row.computed_at_ms as i64,
                    row.label,
                    row.signal_count as i64,
                ],
            )
            .await?;
        Ok(())
    }

    pub async fn insert_intent_prediction(&self, row: &IntentPredictionRow) -> Result<()> {
        let row = row.clone();
        let mut txn = self.conn_db.transaction().await?;
        let existing = txn
            .query_opt(
                "SELECT id FROM intent_predictions
                 WHERE session_id = ?1
                   AND context_state_hash = ?2
                   AND predicted_action = ?3
                   AND was_correct IS NULL
                   AND actual_action IS NULL
                 ORDER BY created_at_ms DESC, id DESC
                 LIMIT 1",
                db::db_params![
                    row.session_id.clone(),
                    row.context_state_hash.clone(),
                    row.predicted_action.clone()
                ],
            )
            .await?;
        let existing_id = existing.map(|row| row.get::<String>(0)).transpose()?;

        if let Some(existing_id) = existing_id {
            txn.execute(
                "UPDATE intent_predictions
                 SET confidence = ?2, created_at_ms = ?3
                 WHERE id = ?1",
                db::db_params![existing_id, row.confidence, row.created_at_ms as i64],
            )
            .await?;
        } else {
            txn.execute(
                "INSERT INTO intent_predictions (id, session_id, context_state_hash, predicted_action, confidence, actual_action, was_correct, created_at_ms) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                db::db_params![
                    row.id,
                    row.session_id,
                    row.context_state_hash,
                    row.predicted_action,
                    row.confidence,
                    row.actual_action,
                    row.was_correct.map(|value| if value { 1i64 } else { 0i64 }),
                    row.created_at_ms as i64,
                ],
            )
            .await?;
        }
        txn.commit().await?;
        Ok(())
    }

    pub async fn insert_system_outcome_prediction(
        &self,
        row: &SystemOutcomePredictionRow,
    ) -> Result<()> {
        let row = row.clone();
        self.conn_db
            .execute(
                "INSERT INTO system_outcome_predictions (id, session_id, prediction_type, predicted_outcome, confidence, actual_outcome, was_correct, created_at_ms) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                db::db_params![
                    row.id,
                    row.session_id,
                    row.prediction_type,
                    row.predicted_outcome,
                    row.confidence,
                    row.actual_outcome,
                    row.was_correct.map(|value| if value { 1i64 } else { 0i64 }),
                    row.created_at_ms as i64,
                ],
            )
            .await?;
        Ok(())
    }

    pub async fn list_intent_predictions(
        &self,
        session_id: &str,
        limit: usize,
    ) -> Result<Vec<IntentPredictionRow>> {
        let rows = self
            .read_db
            .query(
                "SELECT id, session_id, context_state_hash, predicted_action, confidence, actual_action, was_correct, created_at_ms
                 FROM intent_predictions
                 WHERE session_id = ?1
                 ORDER BY created_at_ms DESC, id DESC
                 LIMIT ?2",
                db::db_params![session_id, limit.max(1) as i64],
            )
            .await?;
        rows.iter().map(map_intent_prediction_row).collect()
    }

    pub async fn list_system_outcome_predictions(
        &self,
        session_id: &str,
        limit: usize,
    ) -> Result<Vec<SystemOutcomePredictionRow>> {
        let rows = self
            .read_db
            .query(
                "SELECT id, session_id, prediction_type, predicted_outcome, confidence, actual_outcome, was_correct, created_at_ms
                 FROM system_outcome_predictions
                 WHERE session_id = ?1
                 ORDER BY created_at_ms DESC, id DESC
                 LIMIT ?2",
                db::db_params![session_id, limit.max(1) as i64],
            )
            .await?;
        rows.iter().map(map_system_outcome_prediction_row).collect()
    }

    pub async fn resolve_latest_intent_prediction(
        &self,
        session_id: &str,
        actual_action: &str,
    ) -> Result<()> {
        let mut txn = self.conn_db.transaction().await?;
        let latest = txn
            .query_opt(
                "SELECT id, predicted_action FROM intent_predictions
                 WHERE session_id = ?1 AND was_correct IS NULL
                 ORDER BY created_at_ms DESC, id DESC
                 LIMIT 1",
                db::db_params![session_id],
            )
            .await?;
        let Some(latest) = latest else {
            txn.commit().await?;
            return Ok(());
        };
        let id = latest.get::<String>(0)?;
        let predicted_action = latest.get::<String>(1)?;
        let was_correct = predicted_action == actual_action;
        txn.execute(
            "UPDATE intent_predictions
             SET actual_action = ?2, was_correct = ?3
             WHERE id = ?1",
            db::db_params![id, actual_action, if was_correct { 1i64 } else { 0i64 }],
        )
        .await?;
        txn.commit().await?;
        Ok(())
    }

    pub async fn resolve_latest_system_outcome_prediction(
        &self,
        session_id: &str,
        actual_outcome: &str,
    ) -> Result<()> {
        let mut txn = self.conn_db.transaction().await?;
        let latest = txn
            .query_opt(
                "SELECT id, predicted_outcome FROM system_outcome_predictions
                 WHERE session_id = ?1 AND was_correct IS NULL
                 ORDER BY created_at_ms DESC, id DESC
                 LIMIT 1",
                db::db_params![session_id],
            )
            .await?;
        let Some(latest) = latest else {
            txn.commit().await?;
            return Ok(());
        };
        let id = latest.get::<String>(0)?;
        let predicted_outcome = latest.get::<String>(1)?;
        let was_correct = predicted_outcome == actual_outcome;
        txn.execute(
            "UPDATE system_outcome_predictions
             SET actual_outcome = ?2, was_correct = ?3
             WHERE id = ?1",
            db::db_params![id, actual_outcome, if was_correct { 1i64 } else { 0i64 }],
        )
        .await?;
        txn.commit().await?;
        Ok(())
    }

    pub async fn list_satisfaction_scores(
        &self,
        session_id: &str,
        limit: usize,
    ) -> Result<Vec<SatisfactionScoreRow>> {
        let rows = self
            .read_db
            .query(
                "SELECT id, session_id, score, computed_at_ms, label, signal_count
                 FROM satisfaction_scores
                 WHERE session_id = ?1
                 ORDER BY computed_at_ms DESC, id DESC
                 LIMIT ?2",
                db::db_params![session_id, limit.max(1) as i64],
            )
            .await?;
        rows.iter().map(map_satisfaction_score_row).collect()
    }

    pub async fn list_recent_implicit_signal_samples(
        &self,
        limit: usize,
    ) -> Result<Vec<(f64, u64)>> {
        let rows = self
            .read_db
            .query(
                "SELECT weight, timestamp_ms
                 FROM implicit_signals
                 ORDER BY timestamp_ms DESC, id DESC
                 LIMIT ?1",
                db::db_params![limit.max(1) as i64],
            )
            .await?;
        rows.iter()
            .map(|row| Ok((row.get::<f64>(0)?, row.get::<i64>(1)?.max(0) as u64)))
            .collect()
    }

    pub async fn recent_intent_prediction_success_rate(
        &self,
        predicted_action: &str,
        limit: usize,
    ) -> Result<Option<f64>> {
        let rows = self
            .read_db
            .query(
                "SELECT was_correct
                 FROM intent_predictions
                 WHERE predicted_action = ?1 AND was_correct IS NOT NULL
                 ORDER BY created_at_ms DESC, id DESC
                 LIMIT ?2",
                db::db_params![predicted_action, limit.max(1) as i64],
            )
            .await?;
        let values = rows
            .iter()
            .map(|row| Ok::<bool, anyhow::Error>(row.get::<i64>(0)? != 0))
            .collect::<anyhow::Result<Vec<_>>>()?;
        if values.is_empty() {
            return Ok(None);
        }
        let success = values.iter().filter(|value| **value).count() as f64;
        Ok(Some(success / values.len() as f64))
    }
}
