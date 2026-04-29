use super::*;

impl HistoryStore {
    pub async fn insert_heartbeat_history(
        &self,
        id: &str,
        cycle_timestamp: i64,
        checks_json: &str,
        synthesis_json: Option<&str>,
        actionable: bool,
        digest_text: Option<&str>,
        llm_tokens_used: i64,
        duration_ms: i64,
        status: &str,
    ) -> Result<()> {
        let id = id.to_string();
        let checks_json = checks_json.to_string();
        let synthesis_json = synthesis_json.map(|s| s.to_string());
        let digest_text = digest_text.map(|s| s.to_string());
        let status = status.to_string();
        self.conn.call(move |conn| {
            conn.execute(
                "INSERT OR REPLACE INTO heartbeat_history \
                 (id, cycle_timestamp, checks_json, synthesis_json, actionable, digest_text, llm_tokens_used, duration_ms, status) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    id, cycle_timestamp, checks_json, synthesis_json,
                    actionable as i32, digest_text, llm_tokens_used, duration_ms, status
                ],
            )?;
            Ok(())
        }).await.map_err(|e| anyhow::anyhow!("insert_heartbeat_history: {e}"))
    }

    /// List recent heartbeat history entries. Per D-12.
    pub async fn list_heartbeat_history(&self, limit: usize) -> Result<Vec<HeartbeatHistoryRow>> {
        self.conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT id, cycle_timestamp, checks_json, synthesis_json, actionable, \
                 digest_text, llm_tokens_used, duration_ms, status \
                 FROM heartbeat_history ORDER BY cycle_timestamp DESC LIMIT ?1",
                )?;
                let rows = stmt
                    .query_map([limit as i64], |row| {
                        Ok(HeartbeatHistoryRow {
                            id: row.get(0)?,
                            cycle_timestamp: row.get(1)?,
                            checks_json: row.get(2)?,
                            synthesis_json: row.get(3)?,
                            actionable: row.get::<_, i32>(4)? != 0,
                            digest_text: row.get(5)?,
                            llm_tokens_used: row.get(6)?,
                            duration_ms: row.get(7)?,
                            status: row.get(8)?,
                        })
                    })?
                    .collect::<std::result::Result<Vec<_>, _>>()?;
                Ok(rows)
            })
            .await
            .map_err(|e| anyhow::anyhow!("list_heartbeat_history: {e}"))
    }

    // -----------------------------------------------------------------------
    // Action audit CRUD (per D-06/TRNS-03)
    // -----------------------------------------------------------------------

    /// Insert or replace an action audit entry.
    pub async fn insert_action_audit(&self, entry: &AuditEntryRow) -> Result<()> {
        let entry = entry.clone();
        self.conn.call(move |conn| {
            conn.execute(
                "INSERT OR REPLACE INTO action_audit \
                 (id, timestamp, action_type, summary, explanation, confidence, \
                  confidence_band, causal_trace_id, thread_id, goal_run_id, task_id, raw_data_json) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
                rusqlite::params![
                    entry.id,
                    entry.timestamp,
                    entry.action_type,
                    entry.summary,
                    entry.explanation,
                    entry.confidence,
                    entry.confidence_band,
                    entry.causal_trace_id,
                    entry.thread_id,
                    entry.goal_run_id,
                    entry.task_id,
                    entry.raw_data_json,
                ],
            )?;
            Ok(())
        }).await.map_err(|e| anyhow::anyhow!("insert_action_audit: {e}"))?;
        Ok(())
    }

    /// List action audit entries with optional filters.
    pub async fn list_action_audit(
        &self,
        action_types: Option<&[String]>,
        since: Option<i64>,
        limit: usize,
    ) -> Result<Vec<AuditEntryRow>> {
        let action_types = action_types.map(|a| a.to_vec());
        self.conn
            .call(move |conn| {
                let mut sql = String::from(
                    "SELECT id, timestamp, action_type, summary, explanation, confidence, \
                 confidence_band, causal_trace_id, thread_id, goal_run_id, task_id, raw_data_json \
                 FROM action_audit",
                );
                let mut conditions: Vec<String> = Vec::new();

                if let Some(ref types) = action_types {
                    if !types.is_empty() {
                        let placeholders: Vec<String> = types
                            .iter()
                            .enumerate()
                            .map(|(i, _)| format!("?{}", i + 100))
                            .collect();
                        conditions.push(format!("action_type IN ({})", placeholders.join(",")));
                    }
                }
                if since.is_some() {
                    conditions.push("timestamp >= ?50".to_string());
                }
                if !conditions.is_empty() {
                    sql.push_str(" WHERE ");
                    sql.push_str(&conditions.join(" AND "));
                }
                sql.push_str(" ORDER BY timestamp DESC LIMIT ?99");

                let stmt = conn.prepare(&sql)?;

                // Bind parameters dynamically
                let _param_idx = 1;
                let params: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();

                // Rewrite: use a simpler approach with raw SQL and named params
                drop(stmt);
                drop(params);

                // Build the query more simply
                let mut where_parts: Vec<String> = Vec::new();
                let mut bind_values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();

                if let Some(ref types) = action_types {
                    if !types.is_empty() {
                        let placeholders: Vec<String> =
                            (0..types.len()).map(|i| format!("?{}", i + 1)).collect();
                        where_parts.push(format!("action_type IN ({})", placeholders.join(",")));
                        for t in types {
                            bind_values.push(Box::new(t.clone()));
                        }
                    }
                }
                let since_idx = bind_values.len() + 1;
                if let Some(ts) = since {
                    where_parts.push(format!("timestamp >= ?{}", since_idx));
                    bind_values.push(Box::new(ts));
                }
                let limit_idx = bind_values.len() + 1;
                bind_values.push(Box::new(limit as i64));

                let mut final_sql = String::from(
                    "SELECT id, timestamp, action_type, summary, explanation, confidence, \
                 confidence_band, causal_trace_id, thread_id, goal_run_id, task_id, raw_data_json \
                 FROM action_audit",
                );
                if !where_parts.is_empty() {
                    final_sql.push_str(" WHERE ");
                    final_sql.push_str(&where_parts.join(" AND "));
                }
                final_sql.push_str(&format!(" ORDER BY timestamp DESC LIMIT ?{}", limit_idx));

                let mut stmt = conn.prepare(&final_sql)?;
                let params_ref: Vec<&dyn rusqlite::types::ToSql> =
                    bind_values.iter().map(|b| b.as_ref()).collect();
                let rows = stmt
                    .query_map(params_ref.as_slice(), |row| {
                        Ok(AuditEntryRow {
                            id: row.get(0)?,
                            timestamp: row.get(1)?,
                            action_type: row.get(2)?,
                            summary: row.get(3)?,
                            explanation: row.get(4)?,
                            confidence: row.get(5)?,
                            confidence_band: row.get(6)?,
                            causal_trace_id: row.get(7)?,
                            thread_id: row.get(8)?,
                            goal_run_id: row.get(9)?,
                            task_id: row.get(10)?,
                            raw_data_json: row.get(11)?,
                        })
                    })?
                    .collect::<std::result::Result<Vec<_>, _>>()?;
                Ok(rows)
            })
            .await
            .map_err(|e| anyhow::anyhow!("list_action_audit: {e}"))
    }

    /// Delete oldest audit entries exceeding retention limits. Returns count deleted.
    pub async fn cleanup_action_audit(
        &self,
        max_entries: usize,
        max_age_days: u32,
    ) -> Result<usize> {
        self.conn
            .call(move |conn| {
                let mut deleted = 0usize;
                // Delete by age
                if max_age_days > 0 {
                    let cutoff = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_millis() as i64
                        - (max_age_days as i64 * 86_400 * 1000);
                    deleted += conn.execute(
                        "UPDATE action_audit SET deleted_at = ?2 WHERE timestamp < ?1 AND deleted_at IS NULL",
                        params![cutoff, now_ts() as i64],
                    )? as usize;
                }
                // Delete excess entries (keep newest max_entries)
                if max_entries > 0 {
                    deleted += conn.execute(
                        "UPDATE action_audit SET deleted_at = ?2 WHERE deleted_at IS NULL AND id NOT IN \
                     (SELECT id FROM action_audit WHERE deleted_at IS NULL ORDER BY timestamp DESC LIMIT ?1)",
                        params![max_entries as i64, now_ts() as i64],
                    )? as usize;
                }
                Ok(deleted)
            })
            .await
            .map_err(|e| anyhow::anyhow!("cleanup_action_audit: {e}"))
    }

    /// Mark an audit entry as dismissed by the user. Per BEAT-09/D-04.
    pub async fn dismiss_audit_entry(&self, entry_id: &str) -> Result<()> {
        let entry_id = entry_id.to_string();
        self.conn
            .call(move |conn| {
                conn.execute(
                    "UPDATE action_audit SET user_action = 'dismissed' WHERE id = ?1",
                    [&entry_id],
                )?;
                Ok(())
            })
            .await
            .map_err(|e| anyhow::anyhow!("dismiss_audit_entry: {e}"))
    }

    /// Count dismissed audit entries per action_type since a given timestamp (ms).
    pub async fn count_dismissals_by_type(
        &self,
        since_timestamp: i64,
    ) -> Result<std::collections::HashMap<String, u64>> {
        let since = since_timestamp;
        self.conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT action_type, COUNT(*) FROM action_audit \
                 WHERE user_action = 'dismissed' AND timestamp >= ?1 \
                 GROUP BY action_type",
                )?;
                let rows = stmt
                    .query_map([since], |row| {
                        Ok((row.get::<_, String>(0)?, row.get::<_, u64>(1)?))
                    })?
                    .collect::<std::result::Result<Vec<_>, _>>()?;
                Ok(rows.into_iter().collect())
            })
            .await
            .map_err(|e| anyhow::anyhow!("count_dismissals_by_type: {e}"))
    }

    /// Count total audit entries per action_type since a given timestamp (ms).
    pub async fn count_shown_by_type(
        &self,
        since_timestamp: i64,
    ) -> Result<std::collections::HashMap<String, u64>> {
        let since = since_timestamp;
        self.conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT action_type, COUNT(*) FROM action_audit \
                 WHERE timestamp >= ?1 \
                 GROUP BY action_type",
                )?;
                let rows = stmt
                    .query_map([since], |row| {
                        Ok((row.get::<_, String>(0)?, row.get::<_, u64>(1)?))
                    })?
                    .collect::<std::result::Result<Vec<_>, _>>()?;
                Ok(rows.into_iter().collect())
            })
            .await
            .map_err(|e| anyhow::anyhow!("count_shown_by_type: {e}"))
    }

    /// Count audit entries where user acted on them, per action_type since a given timestamp (ms).
    pub async fn count_acted_on_by_type(
        &self,
        since_timestamp: i64,
    ) -> Result<std::collections::HashMap<String, u64>> {
        let since = since_timestamp;
        self.conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT action_type, COUNT(*) FROM action_audit \
                 WHERE user_action = 'acted_on' AND timestamp >= ?1 \
                 GROUP BY action_type",
                )?;
                let rows = stmt
                    .query_map([since], |row| {
                        Ok((row.get::<_, String>(0)?, row.get::<_, u64>(1)?))
                    })?
                    .collect::<std::result::Result<Vec<_>, _>>()?;
                Ok(rows.into_iter().collect())
            })
            .await
            .map_err(|e| anyhow::anyhow!("count_acted_on_by_type: {e}"))
    }
}
