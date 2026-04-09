use super::*;

impl HistoryStore {
    pub async fn insert_approval_record(&self, row: &ApprovalRecordRow) -> Result<()> {
        let row = row.clone();
        self.conn
            .call(move |conn| {
                conn.execute(
                    "INSERT OR REPLACE INTO approval_records \
                     (approval_id, run_id, task_id, goal_run_id, thread_id, transition_kind, stage_id, scope_summary, target_scope_json, constraints_json, risk_class, rationale_json, policy_fingerprint, requested_at, resolved_at, expires_at, resolution, invalidated_at, invalidation_reason) \
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19)",
                    params![
                        row.approval_id,
                        row.run_id,
                        row.task_id,
                        row.goal_run_id,
                        row.thread_id,
                        row.transition_kind,
                        row.stage_id,
                        row.scope_summary,
                        row.target_scope_json,
                        row.constraints_json,
                        row.risk_class,
                        row.rationale_json,
                        row.policy_fingerprint,
                        row.requested_at as i64,
                        row.resolved_at.map(|value| value as i64),
                        row.expires_at.map(|value| value as i64),
                        row.resolution,
                        row.invalidated_at.map(|value| value as i64),
                        row.invalidation_reason,
                    ],
                )?;
                Ok(())
            })
            .await
            .map_err(|e| anyhow::anyhow!("insert_approval_record: {e}"))
    }

    pub async fn resolve_approval_record(
        &self,
        approval_id: &str,
        resolution: &str,
        resolved_at: u64,
    ) -> Result<()> {
        let approval_id = approval_id.to_string();
        let resolution = resolution.to_string();
        self.conn
            .call(move |conn| {
                conn.execute(
                    "UPDATE approval_records \
                     SET resolution = ?2, resolved_at = ?3 \
                     WHERE approval_id = ?1",
                    params![approval_id, resolution, resolved_at as i64],
                )?;
                Ok(())
            })
            .await
            .map_err(|e| anyhow::anyhow!("resolve_approval_record: {e}"))
    }

    pub async fn invalidate_approval_record(
        &self,
        approval_id: &str,
        invalidation_reason: &str,
        invalidated_at: u64,
    ) -> Result<()> {
        let approval_id = approval_id.to_string();
        let invalidation_reason = invalidation_reason.to_string();
        self.conn
            .call(move |conn| {
                conn.execute(
                    "UPDATE approval_records \
                     SET invalidation_reason = ?2, invalidated_at = ?3 \
                     WHERE approval_id = ?1",
                    params![approval_id, invalidation_reason, invalidated_at as i64],
                )?;
                Ok(())
            })
            .await
            .map_err(|e| anyhow::anyhow!("invalidate_approval_record: {e}"))
    }

    pub async fn get_approval_record(
        &self,
        approval_id: &str,
    ) -> Result<Option<ApprovalRecordRow>> {
        let approval_id = approval_id.to_string();
        self.conn
            .call(move |conn| {
                conn.query_row(
                    "SELECT approval_id, run_id, task_id, goal_run_id, thread_id, transition_kind, stage_id, scope_summary, target_scope_json, constraints_json, risk_class, rationale_json, policy_fingerprint, requested_at, resolved_at, expires_at, resolution, invalidated_at, invalidation_reason \
                     FROM approval_records WHERE approval_id = ?1",
                    params![approval_id],
                    |row| {
                        Ok(ApprovalRecordRow {
                            approval_id: row.get(0)?,
                            run_id: row.get(1)?,
                            task_id: row.get(2)?,
                            goal_run_id: row.get(3)?,
                            thread_id: row.get(4)?,
                            transition_kind: row.get(5)?,
                            stage_id: row.get(6)?,
                            scope_summary: row.get(7)?,
                            target_scope_json: row.get(8)?,
                            constraints_json: row.get(9)?,
                            risk_class: row.get(10)?,
                            rationale_json: row.get(11)?,
                            policy_fingerprint: row.get(12)?,
                            requested_at: row.get::<_, i64>(13)? as u64,
                            resolved_at: row.get::<_, Option<i64>>(14)?.map(|value| value as u64),
                            expires_at: row.get::<_, Option<i64>>(15)?.map(|value| value as u64),
                            resolution: row.get(16)?,
                            invalidated_at: row.get::<_, Option<i64>>(17)?.map(|value| value as u64),
                            invalidation_reason: row.get(18)?,
                        })
                    },
                )
                .optional()
                .map_err(Into::into)
            })
            .await
            .map_err(|e| anyhow::anyhow!("get_approval_record: {e}"))
    }

    pub async fn insert_governance_evaluation(&self, row: &GovernanceEvaluationRow) -> Result<()> {
        let row = row.clone();
        self.conn
            .call(move |conn| {
                conn.execute(
                    "INSERT OR REPLACE INTO governance_evaluations \
                     (id, run_id, task_id, goal_run_id, thread_id, transition_kind, input_json, verdict_json, policy_fingerprint, created_at) \
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                    params![
                        row.id,
                        row.run_id,
                        row.task_id,
                        row.goal_run_id,
                        row.thread_id,
                        row.transition_kind,
                        row.input_json,
                        row.verdict_json,
                        row.policy_fingerprint,
                        row.created_at as i64,
                    ],
                )?;
                Ok(())
            })
            .await
            .map_err(|e| anyhow::anyhow!("insert_governance_evaluation: {e}"))
    }
}
