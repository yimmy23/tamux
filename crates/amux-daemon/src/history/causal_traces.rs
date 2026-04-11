use super::*;

fn settlement_factor_for_outcome(
    outcome_json: &str,
) -> Option<crate::agent::learning::traces::CausalFactor> {
    let outcome =
        serde_json::from_str::<crate::agent::learning::traces::CausalTraceOutcome>(outcome_json)
            .ok()?;
    match outcome {
        crate::agent::learning::traces::CausalTraceOutcome::Success => {
            Some(crate::agent::learning::traces::CausalFactor {
                factor_type: crate::agent::learning::traces::FactorType::PastSuccess,
                description: "selected plan completed successfully".to_string(),
                weight: 0.55,
            })
        }
        crate::agent::learning::traces::CausalTraceOutcome::Failure { reason } => {
            Some(crate::agent::learning::traces::CausalFactor {
                factor_type: crate::agent::learning::traces::FactorType::PastFailure,
                description: reason,
                weight: 0.85,
            })
        }
        _ => None,
    }
}

fn merged_causal_factors_json(
    current_json: &str,
    settlement_factor: Option<&crate::agent::learning::traces::CausalFactor>,
) -> String {
    let mut factors =
        serde_json::from_str::<Vec<crate::agent::learning::traces::CausalFactor>>(current_json)
            .unwrap_or_default();
    if let Some(factor) = settlement_factor {
        let exists = factors.iter().any(|current| {
            current.factor_type == factor.factor_type && current.description == factor.description
        });
        if !exists {
            factors.push(factor.clone());
        }
    }
    serde_json::to_string(&factors).unwrap_or_else(|_| current_json.to_string())
}

impl HistoryStore {
    pub async fn insert_execution_trace(
        &self,
        id: &str,
        thread_id: Option<&str>,
        goal_run_id: Option<&str>,
        task_id: Option<&str>,
        task_type: &str,
        outcome: &str,
        quality_score: Option<f64>,
        tool_sequence_json: &str,
        metrics_json: &str,
        duration_ms: u64,
        tokens_used: u32,
        agent_id: &str,
        started_at_ms: u64,
        completed_at_ms: u64,
        created_at: u64,
    ) -> Result<()> {
        let id = id.to_string();
        let thread_id = thread_id.map(str::to_string);
        let goal_run_id = goal_run_id.map(str::to_string);
        let task_id = task_id.map(str::to_string);
        let task_type = task_type.to_string();
        let outcome = outcome.to_string();
        let tool_sequence_json = tool_sequence_json.to_string();
        let metrics_json = metrics_json.to_string();
        let agent_id = agent_id.to_string();
        self.conn.call(move |conn| {
        conn.execute(
            "INSERT OR REPLACE INTO execution_traces (id, thread_id, goal_run_id, task_id, task_type, outcome, quality_score, tool_sequence_json, metrics_json, duration_ms, tokens_used, created_at, agent_id, started_at_ms, completed_at_ms) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
            params![id, thread_id, goal_run_id, task_id, task_type, outcome, quality_score, tool_sequence_json, metrics_json, duration_ms as i64, tokens_used as i64, created_at as i64, agent_id, started_at_ms as i64, completed_at_ms as i64],
        )?;
        Ok(())
        }).await.map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn list_execution_traces(
        &self,
        task_type: Option<&str>,
        limit: u32,
    ) -> Result<Vec<String>> {
        let task_type = task_type.map(str::to_string);
        self.conn.call(move |conn| {
        if let Some(task_type) = task_type {
            let mut stmt = conn.prepare(
                "SELECT metrics_json FROM execution_traces WHERE task_type = ?1 ORDER BY created_at DESC LIMIT ?2",
            )?;
            let rows = stmt.query_map(params![task_type, limit], |row| row.get(0))?;
            rows.collect::<std::result::Result<Vec<_>, _>>()
                .map_err(Into::into)
        } else {
            let mut stmt = conn.prepare(
                "SELECT metrics_json FROM execution_traces ORDER BY created_at DESC LIMIT ?1",
            )?;
            let rows = stmt.query_map(params![limit], |row| row.get(0))?;
            rows.collect::<std::result::Result<Vec<_>, _>>()
                .map_err(Into::into)
        }
        }).await.map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn insert_causal_trace(
        &self,
        id: &str,
        thread_id: Option<&str>,
        goal_run_id: Option<&str>,
        task_id: Option<&str>,
        decision_type: &str,
        selected_json: &str,
        rejected_options_json: &str,
        context_hash: &str,
        causal_factors_json: &str,
        outcome_json: &str,
        model_used: Option<&str>,
        created_at: u64,
    ) -> Result<()> {
        let id = id.to_string();
        let thread_id = thread_id.map(str::to_string);
        let goal_run_id = goal_run_id.map(str::to_string);
        let task_id = task_id.map(str::to_string);
        let decision_type = decision_type.to_string();
        let selected_json = selected_json.to_string();
        let rejected_options_json = rejected_options_json.to_string();
        let context_hash = context_hash.to_string();
        let causal_factors_json = causal_factors_json.to_string();
        let outcome_json = outcome_json.to_string();
        let model_used = model_used.map(str::to_string);
        self.conn.call(move |conn| {
        conn.execute(
            "INSERT OR REPLACE INTO causal_traces (id, thread_id, goal_run_id, task_id, decision_type, selected_json, rejected_options_json, context_hash, causal_factors_json, outcome_json, model_used, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            params![
                id,
                thread_id,
                goal_run_id,
                task_id,
                decision_type,
                selected_json,
                rejected_options_json,
                context_hash,
                causal_factors_json,
                outcome_json,
                model_used,
                created_at as i64
            ],
        )?;
        Ok(())
        }).await.map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn list_causal_traces_for_option(
        &self,
        option_type: &str,
        limit: u32,
    ) -> Result<Vec<String>> {
        let option_type = option_type.to_string();
        self.conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT outcome_json
             FROM causal_traces
             WHERE json_extract(selected_json, '$.option_type') = ?1
             ORDER BY created_at DESC
             LIMIT ?2",
                )?;
                let rows = stmt.query_map(params![option_type, limit], |row| row.get(0))?;
                rows.collect::<std::result::Result<Vec<_>, _>>()
                    .map_err(Into::into)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn list_recent_causal_trace_records(
        &self,
        option_type: &str,
        limit: u32,
    ) -> Result<Vec<CausalTraceRecord>> {
        let option_type = option_type.to_string();
        self.conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT selected_json, causal_factors_json, outcome_json, created_at
             FROM causal_traces
             WHERE json_extract(selected_json, '$.option_type') = ?1
             ORDER BY created_at DESC
             LIMIT ?2",
                )?;
                let rows = stmt.query_map(params![option_type, limit], |row| {
                    Ok(CausalTraceRecord {
                        selected_json: row.get(0)?,
                        causal_factors_json: row.get(1)?,
                        outcome_json: row.get(2)?,
                        created_at: row.get::<_, i64>(3)? as u64,
                    })
                })?;
                rows.collect::<std::result::Result<Vec<_>, _>>()
                    .map_err(Into::into)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    /// Query causal traces for a given goal_run_id, ordered by creation time descending.
    /// Used by the explainability handler (EXPL-01, EXPL-02).
    pub async fn list_causal_traces_for_goal_run(
        &self,
        goal_run_id: &str,
        limit: u32,
    ) -> Result<Vec<CausalTraceFullRecord>> {
        let goal_run_id = goal_run_id.to_string();
        self.conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT id, thread_id, goal_run_id, task_id, decision_type,
                            selected_json, rejected_options_json, context_hash,
                            causal_factors_json, outcome_json, model_used, created_at
                     FROM causal_traces
                     WHERE goal_run_id = ?1
                     ORDER BY created_at DESC
                     LIMIT ?2",
                )?;
                let rows = stmt.query_map(params![goal_run_id, limit], |row| {
                    Ok(CausalTraceFullRecord {
                        id: row.get(0)?,
                        thread_id: row.get(1)?,
                        goal_run_id: row.get(2)?,
                        task_id: row.get(3)?,
                        decision_type: row.get(4)?,
                        selected_json: row.get(5)?,
                        rejected_options_json: row.get(6)?,
                        context_hash: row.get(7)?,
                        causal_factors_json: row.get(8)?,
                        outcome_json: row.get(9)?,
                        model_used: row.get(10)?,
                        created_at: row.get::<_, i64>(11)? as u64,
                    })
                })?;
                rows.collect::<std::result::Result<Vec<_>, _>>()
                    .map_err(Into::into)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn settle_skill_selection_causal_traces(
        &self,
        thread_id: Option<&str>,
        task_id: Option<&str>,
        goal_run_id: Option<&str>,
        outcome_json: &str,
    ) -> Result<usize> {
        let thread_id = thread_id.map(str::to_string);
        let task_id = task_id.map(str::to_string);
        let goal_run_id = goal_run_id.map(str::to_string);
        let outcome_json = outcome_json.to_string();
        self.conn
            .call(move |conn| {
                let updated = conn.execute(
                    "UPDATE causal_traces
             SET outcome_json = ?4
             WHERE decision_type = 'skill_selection'
               AND json_extract(outcome_json, '$.type') = 'unresolved'
               AND (
                    (?1 IS NOT NULL AND task_id = ?1) OR
                    (?2 IS NOT NULL AND goal_run_id = ?2) OR
                    (?3 IS NOT NULL AND task_id IS NULL AND goal_run_id IS NULL AND thread_id = ?3)
               )",
                    params![task_id, goal_run_id, thread_id, outcome_json],
                )?;
                Ok(updated)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn settle_goal_plan_causal_traces(
        &self,
        goal_run_id: &str,
        outcome_json: &str,
    ) -> Result<usize> {
        let goal_run_id = goal_run_id.to_string();
        let outcome_json = outcome_json.to_string();
        let settlement_factor = settlement_factor_for_outcome(&outcome_json);
        self.conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT id, causal_factors_json
                     FROM causal_traces
                     WHERE goal_run_id = ?1
                       AND json_extract(outcome_json, '$.type') = 'unresolved'
                       AND json_extract(selected_json, '$.option_type') IN ('goal_plan', 'goal_replan')",
                )?;
                let rows = stmt.query_map(params![goal_run_id], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                })?;

                let mut updated = 0usize;
                for row in rows {
                    let (trace_id, causal_factors_json) = row?;
                    let merged =
                        merged_causal_factors_json(&causal_factors_json, settlement_factor.as_ref());
                    updated += conn.execute(
                        "UPDATE causal_traces SET outcome_json = ?2, causal_factors_json = ?3 WHERE id = ?1",
                        params![trace_id, outcome_json, merged],
                    )?;
                }
                Ok(updated)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }
}
