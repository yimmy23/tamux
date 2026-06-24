use super::*;

impl HistoryStore {
    pub async fn insert_cognitive_resonance_sample(
        &self,
        row: &CognitiveResonanceSampleRow,
    ) -> Result<i64> {
        self.conn_db
            .execute_returning_rowid(
                "INSERT INTO cognitive_resonance_samples (sampled_at_ms, revision_velocity_ms, session_entropy, approval_latency_ms, tool_hesitation_count, cognitive_state, state_confidence, resonance_score, verbosity_adjustment, risk_adjustment, proactiveness_adjustment, memory_urgency_adjustment) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
                db::db_params![
                    row.sampled_at_ms as i64,
                    row.revision_velocity_ms.map(|value| value as i64),
                    row.session_entropy,
                    row.approval_latency_ms.map(|value| value as i64),
                    row.tool_hesitation_count as i64,
                    row.cognitive_state.clone(),
                    row.state_confidence,
                    row.resonance_score,
                    row.verbosity_adjustment,
                    row.risk_adjustment,
                    row.proactiveness_adjustment,
                    row.memory_urgency_adjustment,
                ],
            )
            .await
    }

    pub async fn list_cognitive_resonance_samples(
        &self,
        limit: usize,
    ) -> Result<Vec<CognitiveResonanceSampleRow>> {
        let limit = limit.max(1) as i64;
        let rows = self
            .read_db
            .query(
                "SELECT id, sampled_at_ms, revision_velocity_ms, session_entropy, approval_latency_ms, tool_hesitation_count, cognitive_state, state_confidence, resonance_score, verbosity_adjustment, risk_adjustment, proactiveness_adjustment, memory_urgency_adjustment
                 FROM cognitive_resonance_samples
                 ORDER BY sampled_at_ms DESC, id DESC
                 LIMIT ?1",
                db::db_params![limit],
            )
            .await?;
        rows.iter().map(map_cognitive_resonance_sample_row).collect()
    }

    pub async fn insert_behavior_adjustment_log(
        &self,
        row: &BehaviorAdjustmentLogRow,
    ) -> Result<i64> {
        self.conn_db
            .execute_returning_rowid(
                "INSERT INTO behavior_adjustments_log (adjusted_at_ms, parameter, old_value, new_value, trigger_reason, resonance_score) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                db::db_params![
                    row.adjusted_at_ms as i64,
                    row.parameter.clone(),
                    row.old_value,
                    row.new_value,
                    row.trigger_reason.clone(),
                    row.resonance_score,
                ],
            )
            .await
    }

    pub async fn list_behavior_adjustment_log(
        &self,
        limit: usize,
    ) -> Result<Vec<BehaviorAdjustmentLogRow>> {
        let limit = limit.max(1) as i64;
        let rows = self
            .read_db
            .query(
                "SELECT id, adjusted_at_ms, parameter, old_value, new_value, trigger_reason, resonance_score
                 FROM behavior_adjustments_log
                 ORDER BY adjusted_at_ms DESC, id DESC
                 LIMIT ?1",
                db::db_params![limit],
            )
            .await?;
        rows.iter().map(map_behavior_adjustment_log_row).collect()
    }
}
