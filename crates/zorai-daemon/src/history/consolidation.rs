use super::*;

fn map_memory_tombstone_row(row: &db::Row) -> anyhow::Result<MemoryTombstoneRow> {
    Ok(MemoryTombstoneRow {
        id: row.get(0)?,
        target: row.get(1)?,
        original_content: row.get(2)?,
        fact_key: row.get(3)?,
        replaced_by: row.get(4)?,
        replaced_at: row.get(5)?,
        source_kind: row.get(6)?,
        provenance_id: row.get(7)?,
        created_at: row.get(8)?,
    })
}

fn map_execution_trace_row(row: &db::Row) -> anyhow::Result<ExecutionTraceRow> {
    Ok(ExecutionTraceRow {
        id: row.get(0)?,
        goal_run_id: row.get(1)?,
        task_id: row.get(2)?,
        task_type: row.get(3)?,
        outcome: row.get(4)?,
        quality_score: row.get(5)?,
        tool_sequence_json: row.get(6)?,
        metrics_json: row.get(7)?,
        duration_ms: row.get(8)?,
        tokens_used: row.get(9)?,
        created_at: row.get(10)?,
    })
}

const MEMORY_TOMBSTONE_COLUMNS: &str = "id, target, original_content, fact_key, replaced_by, replaced_at, source_kind, provenance_id, created_at";

const EXECUTION_TRACE_COLUMNS: &str = "id, goal_run_id, task_id, task_type, outcome, quality_score, tool_sequence_json, metrics_json, duration_ms, tokens_used, created_at";

impl HistoryStore {
    pub async fn insert_memory_tombstone(
        &self,
        id: &str,
        target: &str,
        original_content: &str,
        fact_key: Option<&str>,
        replaced_by: Option<&str>,
        source_kind: &str,
        provenance_id: Option<&str>,
        created_at: u64,
    ) -> Result<()> {
        let now = created_at as i64;
        self.conn_db
            .execute(
                "INSERT OR REPLACE INTO memory_tombstones (id, target, original_content, fact_key, replaced_by, replaced_at, source_kind, provenance_id, created_at, deleted_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, NULL)",
                db::db_params![
                    id,
                    target,
                    original_content,
                    fact_key.map(str::to_string),
                    replaced_by.map(str::to_string),
                    now,
                    source_kind,
                    provenance_id.map(str::to_string),
                    now
                ],
            )
            .await?;
        Ok(())
    }

    pub async fn list_memory_tombstones(
        &self,
        target: Option<&str>,
        limit: usize,
    ) -> Result<Vec<MemoryTombstoneRow>> {
        let rows = if let Some(target) = target {
            self.read_db
                .query(
                    &format!("SELECT {MEMORY_TOMBSTONE_COLUMNS} FROM memory_tombstones WHERE target = ?1 AND deleted_at IS NULL ORDER BY created_at DESC LIMIT ?2"),
                    db::db_params![target, limit as i64],
                )
                .await?
        } else {
            self.read_db
                .query(
                    &format!("SELECT {MEMORY_TOMBSTONE_COLUMNS} FROM memory_tombstones WHERE deleted_at IS NULL ORDER BY created_at DESC LIMIT ?1"),
                    db::db_params![limit as i64],
                )
                .await?
        };
        rows.iter().map(map_memory_tombstone_row).collect()
    }

    pub async fn delete_expired_tombstones(&self, max_age_ms: u64, now: u64) -> Result<usize> {
        let cutoff = (now as i64) - (max_age_ms as i64);
        let count = self
            .conn_db
            .execute(
                "UPDATE memory_tombstones SET deleted_at = ?2 WHERE created_at < ?1 AND deleted_at IS NULL",
                db::db_params![cutoff, now_ts() as i64],
            )
            .await?;
        Ok(count as usize)
    }

    pub async fn restore_tombstone(
        &self,
        tombstone_id: &str,
    ) -> Result<Option<MemoryTombstoneRow>> {
        let mut txn = self.conn_db.transaction().await?;
        let row = txn
            .query_opt(
                &format!("SELECT {MEMORY_TOMBSTONE_COLUMNS} FROM memory_tombstones WHERE id = ?1 AND deleted_at IS NULL"),
                db::db_params![tombstone_id],
            )
            .await?
            .map(|row| map_memory_tombstone_row(&row))
            .transpose()?;
        if row.is_some() {
            txn.execute(
                "UPDATE memory_tombstones SET deleted_at = ?2 WHERE id = ?1 AND deleted_at IS NULL",
                db::db_params![tombstone_id, now_ts() as i64],
            )
            .await?;
        }
        txn.commit().await?;
        Ok(row)
    }

    pub async fn get_consolidation_state(&self, key: &str) -> Result<Option<String>> {
        let row = self
            .read_db
            .query_opt(
                "SELECT value FROM consolidation_state WHERE key = ?1 AND deleted_at IS NULL",
                db::db_params![key],
            )
            .await?;
        row.map(|row| row.get::<String>(0)).transpose()
    }

    /// Batched variant of `get_consolidation_state`. Returns a map of
    /// key -> value for the keys present in the table; missing keys are
    /// simply absent from the map. Used by hydrate paths that previously
    /// looped per-task with sequential `get_consolidation_state` awaits
    /// (N+1 at daemon startup).
    pub async fn get_consolidation_states_batch(
        &self,
        keys: &[String],
    ) -> Result<std::collections::HashMap<String, String>> {
        if keys.is_empty() {
            return Ok(std::collections::HashMap::new());
        }
        let placeholders = std::iter::repeat("?")
            .take(keys.len())
            .collect::<Vec<_>>()
            .join(", ");
        let sql = format!(
            "SELECT key, value FROM consolidation_state \
             WHERE key IN ({placeholders}) AND deleted_at IS NULL"
        );
        let values = keys.iter().map(|key| db::Value::Text(key.clone())).collect();
        let rows = self
            .read_db
            .query(&sql, db::Params::Positional(values))
            .await?;
        let mut out = std::collections::HashMap::with_capacity(keys.len());
        for row in rows.iter() {
            out.insert(row.get::<String>(0)?, row.get::<String>(1)?);
        }
        Ok(out)
    }

    pub async fn set_consolidation_state(&self, key: &str, value: &str, now: u64) -> Result<()> {
        self.conn_db
            .execute(
                "INSERT OR REPLACE INTO consolidation_state (key, value, updated_at, deleted_at) VALUES (?1, ?2, ?3, NULL)",
                db::db_params![key, value, now as i64],
            )
            .await?;
        Ok(())
    }

    pub async fn delete_consolidation_state(&self, key: &str) -> Result<()> {
        self.conn_db
            .execute(
                "UPDATE consolidation_state SET deleted_at = ?2 WHERE key = ?1 AND deleted_at IS NULL",
                db::db_params![key, now_ts() as i64],
            )
            .await?;
        Ok(())
    }

    /// List consolidation_state entries whose key starts with `prefix`.
    pub async fn list_consolidation_state_by_prefix(
        &self,
        prefix: &str,
    ) -> Result<Vec<(String, String)>> {
        let like = format!("{}%", prefix);
        let rows = self
            .read_db
            .query(
                "SELECT key, value FROM consolidation_state WHERE key LIKE ?1 AND deleted_at IS NULL",
                db::db_params![like],
            )
            .await?;
        rows.iter()
            .filter_map(|row| match (row.get::<String>(0), row.get::<String>(1)) {
                (Ok(a), Ok(b)) => Some((a, b)),
                _ => None,
            })
            .map(Ok)
            .collect()
    }

    pub async fn first_consolidation_state_by_prefix_value(
        &self,
        prefix: &str,
        value: &str,
    ) -> Result<Option<(String, String)>> {
        let like = format!("{}%", prefix);
        let row = self
            .read_db
            .query_opt(
                "SELECT key, value \
                 FROM consolidation_state \
                 WHERE key LIKE ?1 AND value = ?2 AND deleted_at IS NULL \
                 ORDER BY updated_at ASC, key ASC \
                 LIMIT 1",
                db::db_params![like, value],
            )
            .await?;
        row.map(|row| Ok((row.get::<String>(0)?, row.get::<String>(1)?)))
            .transpose()
    }

    /// List skill variants matching a given status string, up to `limit` rows.
    pub async fn list_skill_variants_by_status(
        &self,
        status: &str,
        limit: usize,
    ) -> Result<Vec<SkillVariantRecord>> {
        let page = self
            .list_skill_variants_by_status_page(status, None, limit)
            .await?;
        Ok(page.variants)
    }

    pub async fn list_skill_variants_by_status_page(
        &self,
        status: &str,
        cursor: Option<&str>,
        limit: usize,
    ) -> Result<SkillVariantPage> {
        let variants = self
            .conn_db
            .query(
                "SELECT variant_id, skill_name, variant_name, relative_path, parent_variant_id, version, context_tags_json, use_count, success_count, failure_count, fitness_score, status, last_used_at, created_at, updated_at \
                 FROM skill_variants WHERE status = ?1 ORDER BY updated_at ASC",
                db::db_params![status],
            )
            .await?
            .iter()
            .filter_map(|row| map_skill_variant_row_db(row).ok())
            .collect();
        super::page_skill_variants(variants, cursor, limit)
    }

    pub async fn list_recent_successful_traces(
        &self,
        after_timestamp: u64,
        limit: usize,
    ) -> Result<Vec<ExecutionTraceRow>> {
        let rows = self
            .read_db
            .query(
                &format!("SELECT {EXECUTION_TRACE_COLUMNS} FROM execution_traces WHERE outcome = 'success' AND created_at > ?1 ORDER BY created_at ASC LIMIT ?2"),
                db::db_params![after_timestamp as i64, limit as i64],
            )
            .await?;
        rows.iter().map(map_execution_trace_row).collect()
    }

    pub async fn get_successful_execution_trace_by_id(
        &self,
        trace_id: &str,
    ) -> Result<Option<ExecutionTraceRow>> {
        let row = self
            .read_db
            .query_opt(
                &format!("SELECT {EXECUTION_TRACE_COLUMNS} \
                     FROM execution_traces \
                     WHERE id = ?1 AND outcome = 'success' \
                     LIMIT 1"),
                db::db_params![trace_id],
            )
            .await?;
        row.map(|row| map_execution_trace_row(&row)).transpose()
    }

    pub async fn trace_has_skill_consultation(
        &self,
        thread_id: Option<&str>,
        task_id: Option<&str>,
        goal_run_id: Option<&str>,
    ) -> Result<bool> {
        let row = self
            .read_db
            .query_opt(
                "SELECT COUNT(1) FROM skill_variant_usage \
                 WHERE ( \
                    (?1 IS NOT NULL AND task_id = ?1) OR \
                    (?2 IS NOT NULL AND goal_run_id = ?2) OR \
                    (?3 IS NOT NULL AND task_id IS NULL AND goal_run_id IS NULL AND thread_id = ?3) \
                 )",
                db::db_params![
                    task_id.map(str::to_string),
                    goal_run_id.map(str::to_string),
                    thread_id.map(str::to_string)
                ],
            )
            .await?;
        Ok(match row {
            Some(row) => row.get::<i64>(0)? > 0,
            None => false,
        })
    }

    pub(crate) async fn append_telemetry(
        &self,
        kind: &str,
        payload: serde_json::Value,
    ) -> Result<()> {
        let line = serde_json::to_string(&payload)?;
        let telemetry_dir = self.telemetry_dir.clone();
        let worm_dir = self.worm_dir.clone();
        let log_path = telemetry_dir.join(format!("{}.jsonl", kind));
        let worm_path = worm_dir.join(format!("{}-ledger.jsonl", kind));

        append_line(&log_path, &line)?;

        let (prev_hash, seq) = match self.get_worm_chain_tip(kind).await? {
            Some(tip) => (tip.hash, tip.seq + 1),
            None => {
                let (prev_hash, seq) = read_last_worm_entry(&worm_path);
                (prev_hash, seq as i64)
            }
        };

        let payload_json = serde_json::to_string(&payload)?;
        let hash = hex_hash(&format!("{}{}", prev_hash, payload_json));
        let worm_line = serde_json::to_string(&json!({
            "seq": seq,
            "prev_hash": prev_hash,
            "hash": hash,
            "payload": payload,
        }))?;
        append_line(&worm_path, &worm_line)?;
        self.set_worm_chain_tip(kind, seq, &hash).await?;
        Ok(())
    }

    /// Detect sequences of 3+ consecutive successful managed commands
    /// that completed within a 5-minute window.
    pub async fn detect_skill_candidates(&self) -> Result<Vec<(String, Vec<HistorySearchHit>)>> {
        let rows = self
            .conn_db
            .query(
                "SELECT id, kind, title, excerpt, path, timestamp FROM history_entries \
             WHERE kind = 'managed-command' \
             ORDER BY timestamp DESC LIMIT 20",
                db::Params::None,
            )
            .await?;

        let hits: Vec<HistorySearchHit> = rows
            .iter()
            .filter_map(|row| {
                Some(HistorySearchHit {
                    id: row.get(0).ok()?,
                    kind: row.get(1).ok()?,
                    title: row.get(2).ok()?,
                    excerpt: row.get(3).ok()?,
                    path: row.get(4).ok()?,
                    timestamp: row.get::<i64>(5).ok()? as u64,
                    score: 0.0,
                })
            })
            .collect();
        let mut candidates = Vec::new();

        let mut run: Vec<HistorySearchHit> = Vec::new();
        for hit in &hits {
            if hit.excerpt.contains("exit=Some(0)") {
                if run.is_empty()
                    || (run.last().unwrap().timestamp.abs_diff(hit.timestamp) < 300)
                {
                    run.push(hit.clone());
                } else {
                    if run.len() >= 3 {
                        let title = format!("Workflow: {}", run.first().unwrap().title);
                        candidates.push((title, run.clone()));
                    }
                    run = vec![hit.clone()];
                }
            } else {
                if run.len() >= 3 {
                    let title = format!("Workflow: {}", run.first().unwrap().title);
                    candidates.push((title, run.clone()));
                }
                run.clear();
            }
        }
        if run.len() >= 3 {
            let title = format!("Workflow: {}", run.first().unwrap().title);
            candidates.push((title, run));
        }

        Ok(candidates)
    }
}
