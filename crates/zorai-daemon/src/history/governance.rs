use super::*;

impl HistoryStore {
    pub async fn insert_approval_inbox_entry(&self, row: &ApprovalInboxEntry) -> Result<()> {
        let row = row.clone();
        let request_json = serde_json::to_string(&row.request)?;
        let approval_json = serde_json::to_string(&row.approval)?;
        let constraints_json = serde_json::to_string(&row.constraints)?;
        let transition_kind_json = serde_json::to_string(&row.transition_kind)?;
        self.conn
            .call(move |conn| {
                conn.execute(
                    "INSERT OR REPLACE INTO approval_inbox \
                     (approval_id, session_id, workspace_id, execution_id, request_json, approval_json, policy_fingerprint, constraints_json, transition_kind, requested_at, expires_at, gateway_surface, gateway_channel, gateway_thread, rendered_prompt) \
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
                    params![
                        row.approval_id,
                        row.session_id,
                        row.workspace_id,
                        row.execution_id,
                        request_json,
                        approval_json,
                        row.policy_fingerprint,
                        constraints_json,
                        transition_kind_json,
                        row.requested_at as i64,
                        row.expires_at.map(|value| value as i64),
                        row.gateway_surface,
                        row.gateway_channel,
                        row.gateway_thread,
                        row.rendered_prompt,
                    ],
                )?;
                Ok(())
            })
            .await
            .map_err(|e| anyhow::anyhow!("insert_approval_inbox_entry: {e}"))
    }

    pub async fn list_pending_inbox_entries(&self) -> Result<Vec<ApprovalInboxEntry>> {
        self.read_conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT approval_id, session_id, workspace_id, execution_id, request_json, approval_json, policy_fingerprint, constraints_json, transition_kind, requested_at, expires_at, gateway_surface, gateway_channel, gateway_thread, rendered_prompt \
                     FROM approval_inbox ORDER BY requested_at DESC",
                )?;
                let rows = stmt.query_map([], |row| {
                    let request_json: String = row.get(4)?;
                    let approval_json: String = row.get(5)?;
                    let constraints_json: String = row.get(7)?;
                    let transition_kind_json: String = row.get(8)?;
                    Ok(ApprovalInboxEntry {
                        approval_id: row.get(0)?,
                        session_id: row.get(1)?,
                        workspace_id: row.get(2)?,
                        execution_id: row.get(3)?,
                        request: serde_json::from_str(&request_json).map_err(|e| {
                            rusqlite::Error::ToSqlConversionFailure(Box::new(e))
                        })?,
                        approval: serde_json::from_str(&approval_json).map_err(|e| {
                            rusqlite::Error::ToSqlConversionFailure(Box::new(e))
                        })?,
                        policy_fingerprint: row.get(6)?,
                        constraints: serde_json::from_str(&constraints_json).map_err(|e| {
                            rusqlite::Error::ToSqlConversionFailure(Box::new(e))
                        })?,
                        transition_kind: serde_json::from_str(&transition_kind_json).map_err(
                            |e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)),
                        )?,
                        requested_at: row.get::<_, i64>(9)? as u64,
                        expires_at: row.get::<_, Option<i64>>(10)?.map(|value| value as u64),
                        gateway_surface: row.get(11)?,
                        gateway_channel: row.get(12)?,
                        gateway_thread: row.get(13)?,
                        rendered_prompt: row.get(14)?,
                    })
                })?;
                Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
            })
            .await
            .map_err(|e| anyhow::anyhow!("list_pending_inbox_entries: {e}"))
    }

    pub async fn remove_inbox_entry(&self, approval_id: &str) -> Result<()> {
        let approval_id = approval_id.to_string();
        self.conn
            .call(move |conn| {
                conn.execute(
                    "DELETE FROM approval_inbox WHERE approval_id = ?1",
                    params![approval_id],
                )?;
                Ok(())
            })
            .await
            .map_err(|e| anyhow::anyhow!("remove_inbox_entry: {e}"))
    }

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
