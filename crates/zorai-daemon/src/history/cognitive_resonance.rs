use super::*;

impl HistoryStore {
    pub async fn insert_cognitive_resonance_sample(
        &self,
        row: &CognitiveResonanceSampleRow,
    ) -> Result<i64> {
        let row = row.clone();
        self.conn
            .call(move |conn| {
                conn.execute(
                    "INSERT INTO cognitive_resonance_samples (sampled_at_ms, revision_velocity_ms, session_entropy, approval_latency_ms, tool_hesitation_count, cognitive_state, state_confidence, resonance_score, verbosity_adjustment, risk_adjustment, proactiveness_adjustment, memory_urgency_adjustment) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
                    params![
                        row.sampled_at_ms as i64,
                        row.revision_velocity_ms.map(|value| value as i64),
                        row.session_entropy,
                        row.approval_latency_ms.map(|value| value as i64),
                        row.tool_hesitation_count as i64,
                        row.cognitive_state,
                        row.state_confidence,
                        row.resonance_score,
                        row.verbosity_adjustment,
                        row.risk_adjustment,
                        row.proactiveness_adjustment,
                        row.memory_urgency_adjustment,
                    ],
                )?;
                Ok(conn.last_insert_rowid())
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn list_cognitive_resonance_samples(
        &self,
        limit: usize,
    ) -> Result<Vec<CognitiveResonanceSampleRow>> {
        let limit = limit.max(1) as i64;
        self.read_conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT id, sampled_at_ms, revision_velocity_ms, session_entropy, approval_latency_ms, tool_hesitation_count, cognitive_state, state_confidence, resonance_score, verbosity_adjustment, risk_adjustment, proactiveness_adjustment, memory_urgency_adjustment
                     FROM cognitive_resonance_samples
                     ORDER BY sampled_at_ms DESC, id DESC
                     LIMIT ?1",
                )?;
                let rows = stmt.query_map(params![limit], |row| {
                    Ok(CognitiveResonanceSampleRow {
                        id: Some(row.get(0)?),
                        sampled_at_ms: row.get::<_, i64>(1)?.max(0) as u64,
                        revision_velocity_ms: row
                            .get::<_, Option<i64>>(2)?
                            .map(|value| value.max(0) as u64),
                        session_entropy: row.get(3)?,
                        approval_latency_ms: row
                            .get::<_, Option<i64>>(4)?
                            .map(|value| value.max(0) as u64),
                        tool_hesitation_count: row.get::<_, i64>(5)?.max(0) as u64,
                        cognitive_state: row.get(6)?,
                        state_confidence: row.get(7)?,
                        resonance_score: row.get(8)?,
                        verbosity_adjustment: row.get(9)?,
                        risk_adjustment: row.get(10)?,
                        proactiveness_adjustment: row.get(11)?,
                        memory_urgency_adjustment: row.get(12)?,
                    })
                })?;
                rows.collect::<std::result::Result<Vec<_>, _>>()
                    .map_err(Into::into)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn insert_behavior_adjustment_log(
        &self,
        row: &BehaviorAdjustmentLogRow,
    ) -> Result<i64> {
        let row = row.clone();
        self.conn
            .call(move |conn| {
                conn.execute(
                    "INSERT INTO behavior_adjustments_log (adjusted_at_ms, parameter, old_value, new_value, trigger_reason, resonance_score) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    params![
                        row.adjusted_at_ms as i64,
                        row.parameter,
                        row.old_value,
                        row.new_value,
                        row.trigger_reason,
                        row.resonance_score,
                    ],
                )?;
                Ok(conn.last_insert_rowid())
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn list_behavior_adjustment_log(
        &self,
        limit: usize,
    ) -> Result<Vec<BehaviorAdjustmentLogRow>> {
        let limit = limit.max(1) as i64;
        self.read_conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT id, adjusted_at_ms, parameter, old_value, new_value, trigger_reason, resonance_score
                     FROM behavior_adjustments_log
                     ORDER BY adjusted_at_ms DESC, id DESC
                     LIMIT ?1",
                )?;
                let rows = stmt.query_map(params![limit], |row| {
                    Ok(BehaviorAdjustmentLogRow {
                        id: Some(row.get(0)?),
                        adjusted_at_ms: row.get::<_, i64>(1)?.max(0) as u64,
                        parameter: row.get(2)?,
                        old_value: row.get(3)?,
                        new_value: row.get(4)?,
                        trigger_reason: row.get(5)?,
                        resonance_score: row.get(6)?,
                    })
                })?;
                rows.collect::<std::result::Result<Vec<_>, _>>()
                    .map_err(Into::into)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }
}
