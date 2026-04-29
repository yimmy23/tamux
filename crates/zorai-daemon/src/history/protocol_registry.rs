use super::*;

impl HistoryStore {
    pub async fn upsert_emergent_protocol(&self, row: &EmergentProtocolRow) -> Result<()> {
        let row = row.clone();
        self.conn
            .call(move |conn| {
                conn.execute(
                    "INSERT OR REPLACE INTO emergent_protocols (protocol_id, token, description, agent_a, agent_b, thread_id, normalized_pattern, signal_kind, context_signature_json, created_at, activated_at, last_used_at, usage_count, success_rate, source_candidate_id) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
                    params![
                        row.protocol_id,
                        row.token,
                        row.description,
                        row.agent_a,
                        row.agent_b,
                        row.thread_id,
                        row.normalized_pattern,
                        row.signal_kind,
                        row.context_signature_json,
                        row.created_at as i64,
                        row.activated_at as i64,
                        row.last_used_at.map(|v| v as i64),
                        row.usage_count as i64,
                        row.success_rate,
                        row.source_candidate_id,
                    ],
                )?;
                Ok(())
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn get_emergent_protocol_by_token(
        &self,
        token: &str,
    ) -> Result<Option<EmergentProtocolRow>> {
        let token = token.to_string();
        self.read_conn
            .call(move |conn| {
                conn.query_row(
                    "SELECT protocol_id, token, description, agent_a, agent_b, thread_id, normalized_pattern, signal_kind, context_signature_json, created_at, activated_at, last_used_at, usage_count, success_rate, source_candidate_id FROM emergent_protocols WHERE token = ?1",
                    params![token],
                    |row| {
                        Ok(EmergentProtocolRow {
                            protocol_id: row.get(0)?,
                            token: row.get(1)?,
                            description: row.get(2)?,
                            agent_a: row.get(3)?,
                            agent_b: row.get(4)?,
                            thread_id: row.get(5)?,
                            normalized_pattern: row.get(6)?,
                            signal_kind: row.get(7)?,
                            context_signature_json: row.get(8)?,
                            created_at: row.get::<_, i64>(9)?.max(0) as u64,
                            activated_at: row.get::<_, i64>(10)?.max(0) as u64,
                            last_used_at: row.get::<_, Option<i64>>(11)?.map(|v| v.max(0) as u64),
                            usage_count: row.get::<_, i64>(12)?.max(0) as u64,
                            success_rate: row.get(13)?,
                            source_candidate_id: row.get(14)?,
                        })
                    },
                )
                .optional()
                .map_err(Into::into)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn get_emergent_protocol_by_pattern(
        &self,
        thread_id: &str,
        normalized_pattern: &str,
    ) -> Result<Option<EmergentProtocolRow>> {
        let thread_id = thread_id.to_string();
        let normalized_pattern = normalized_pattern.to_string();
        self.read_conn
            .call(move |conn| {
                conn.query_row(
                    "SELECT protocol_id, token, description, agent_a, agent_b, thread_id, normalized_pattern, signal_kind, context_signature_json, created_at, activated_at, last_used_at, usage_count, success_rate, source_candidate_id FROM emergent_protocols WHERE thread_id = ?1 AND normalized_pattern = ?2 ORDER BY activated_at DESC LIMIT 1",
                    params![thread_id, normalized_pattern],
                    |row| {
                        Ok(EmergentProtocolRow {
                            protocol_id: row.get(0)?,
                            token: row.get(1)?,
                            description: row.get(2)?,
                            agent_a: row.get(3)?,
                            agent_b: row.get(4)?,
                            thread_id: row.get(5)?,
                            normalized_pattern: row.get(6)?,
                            signal_kind: row.get(7)?,
                            context_signature_json: row.get(8)?,
                            created_at: row.get::<_, i64>(9)?.max(0) as u64,
                            activated_at: row.get::<_, i64>(10)?.max(0) as u64,
                            last_used_at: row.get::<_, Option<i64>>(11)?.map(|v| v.max(0) as u64),
                            usage_count: row.get::<_, i64>(12)?.max(0) as u64,
                            success_rate: row.get(13)?,
                            source_candidate_id: row.get(14)?,
                        })
                    },
                )
                .optional()
                .map_err(Into::into)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn get_emergent_protocol_by_id(
        &self,
        protocol_id: &str,
    ) -> Result<Option<EmergentProtocolRow>> {
        let protocol_id = protocol_id.to_string();
        self.read_conn
            .call(move |conn| {
                conn.query_row(
                    "SELECT protocol_id, token, description, agent_a, agent_b, thread_id, normalized_pattern, signal_kind, context_signature_json, created_at, activated_at, last_used_at, usage_count, success_rate, source_candidate_id FROM emergent_protocols WHERE protocol_id = ?1 LIMIT 1",
                    params![protocol_id],
                    |row| {
                        Ok(EmergentProtocolRow {
                            protocol_id: row.get(0)?,
                            token: row.get(1)?,
                            description: row.get(2)?,
                            agent_a: row.get(3)?,
                            agent_b: row.get(4)?,
                            thread_id: row.get(5)?,
                            normalized_pattern: row.get(6)?,
                            signal_kind: row.get(7)?,
                            context_signature_json: row.get(8)?,
                            created_at: row.get::<_, i64>(9)?.max(0) as u64,
                            activated_at: row.get::<_, i64>(10)?.max(0) as u64,
                            last_used_at: row.get::<_, Option<i64>>(11)?.map(|v| v.max(0) as u64),
                            usage_count: row.get::<_, i64>(12)?.max(0) as u64,
                            success_rate: row.get(13)?,
                            source_candidate_id: row.get(14)?,
                        })
                    },
                )
                .optional()
                .map_err(Into::into)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn list_emergent_protocols_for_thread(
        &self,
        thread_id: &str,
    ) -> Result<Vec<EmergentProtocolRow>> {
        let thread_id = thread_id.to_string();
        self.read_conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT protocol_id, token, description, agent_a, agent_b, thread_id, normalized_pattern, signal_kind, context_signature_json, created_at, activated_at, last_used_at, usage_count, success_rate, source_candidate_id FROM emergent_protocols WHERE thread_id = ?1 ORDER BY activated_at DESC",
                )?;
                let rows = stmt.query_map(params![thread_id], |row| {
                    Ok(EmergentProtocolRow {
                        protocol_id: row.get(0)?,
                        token: row.get(1)?,
                        description: row.get(2)?,
                        agent_a: row.get(3)?,
                        agent_b: row.get(4)?,
                        thread_id: row.get(5)?,
                        normalized_pattern: row.get(6)?,
                        signal_kind: row.get(7)?,
                        context_signature_json: row.get(8)?,
                        created_at: row.get::<_, i64>(9)?.max(0) as u64,
                        activated_at: row.get::<_, i64>(10)?.max(0) as u64,
                        last_used_at: row.get::<_, Option<i64>>(11)?.map(|v| v.max(0) as u64),
                        usage_count: row.get::<_, i64>(12)?.max(0) as u64,
                        success_rate: row.get(13)?,
                        source_candidate_id: row.get(14)?,
                    })
                })?;
                rows.collect::<std::result::Result<Vec<_>, _>>().map_err(Into::into)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn replace_protocol_steps(
        &self,
        protocol_id: &str,
        steps: &[ProtocolStepRow],
    ) -> Result<()> {
        let protocol_id = protocol_id.to_string();
        let steps = steps.to_vec();
        self.conn
            .call(move |conn| {
                let tx = conn.transaction()?;
                tx.execute(
                    "UPDATE protocol_steps SET deleted_at = ?2 WHERE protocol_id = ?1 AND deleted_at IS NULL",
                    params![protocol_id, now_ts() as i64],
                )?;
                for step in steps {
                    tx.execute(
                        "INSERT OR REPLACE INTO protocol_steps (protocol_id, step_index, intent, tool_name, args_template_json, deleted_at) VALUES (?1, ?2, ?3, ?4, ?5, NULL)",
                        params![
                            step.protocol_id,
                            step.step_index as i64,
                            step.intent,
                            step.tool_name,
                            step.args_template_json,
                        ],
                    )?;
                }
                tx.commit()?;
                Ok(())
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn list_protocol_steps(&self, protocol_id: &str) -> Result<Vec<ProtocolStepRow>> {
        let protocol_id = protocol_id.to_string();
        self.read_conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT protocol_id, step_index, intent, tool_name, args_template_json FROM protocol_steps WHERE protocol_id = ?1 AND deleted_at IS NULL ORDER BY step_index ASC",
                )?;
                let rows = stmt.query_map(params![protocol_id], |row| {
                    Ok(ProtocolStepRow {
                        protocol_id: row.get(0)?,
                        step_index: row.get::<_, i64>(1)?.max(0) as u64,
                        intent: row.get(2)?,
                        tool_name: row.get(3)?,
                        args_template_json: row.get(4)?,
                    })
                })?;
                rows.collect::<std::result::Result<Vec<_>, _>>().map_err(Into::into)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn insert_protocol_usage_log(&self, row: &ProtocolUsageLogRow) -> Result<()> {
        let row = row.clone();
        self.conn
            .call(move |conn| {
                conn.execute(
                    "INSERT INTO protocol_usage_log (id, protocol_id, used_at, execution_time_ms, success, fallback_reason) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    params![
                        row.id,
                        row.protocol_id,
                        row.used_at as i64,
                        row.execution_time_ms.map(|v| v as i64),
                        if row.success { 1i64 } else { 0i64 },
                        row.fallback_reason,
                    ],
                )?;
                Ok(())
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn update_emergent_protocol_usage_stats(
        &self,
        protocol_id: &str,
        last_used_at: u64,
        usage_count: u64,
        success_rate: f64,
    ) -> Result<()> {
        let protocol_id = protocol_id.to_string();
        self.conn
            .call(move |conn| {
                conn.execute(
                    "UPDATE emergent_protocols SET last_used_at = ?2, usage_count = ?3, success_rate = ?4 WHERE protocol_id = ?1",
                    params![protocol_id, last_used_at as i64, usage_count as i64, success_rate],
                )?;
                Ok(())
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn list_protocol_usage_log(
        &self,
        protocol_id: &str,
    ) -> Result<Vec<ProtocolUsageLogRow>> {
        let protocol_id = protocol_id.to_string();
        self.read_conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT id, protocol_id, used_at, execution_time_ms, success, fallback_reason FROM protocol_usage_log WHERE protocol_id = ?1 ORDER BY used_at DESC",
                )?;
                let rows = stmt.query_map(params![protocol_id], |row| {
                    Ok(ProtocolUsageLogRow {
                        id: row.get(0)?,
                        protocol_id: row.get(1)?,
                        used_at: row.get::<_, i64>(2)?.max(0) as u64,
                        execution_time_ms: row.get::<_, Option<i64>>(3)?.map(|v| v.max(0) as u64),
                        success: row.get::<_, i64>(4)? != 0,
                        fallback_reason: row.get(5)?,
                    })
                })?;
                rows.collect::<std::result::Result<Vec<_>, _>>().map_err(Into::into)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }
}
