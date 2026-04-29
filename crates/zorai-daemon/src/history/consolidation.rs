use super::*;

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
        let id = id.to_string();
        let target = target.to_string();
        let original_content = original_content.to_string();
        let fact_key = fact_key.map(str::to_string);
        let replaced_by = replaced_by.map(str::to_string);
        let source_kind = source_kind.to_string();
        let provenance_id = provenance_id.map(str::to_string);
        let now = created_at as i64;
        self.conn.call(move |conn| {
            conn.execute(
                "INSERT OR REPLACE INTO memory_tombstones (id, target, original_content, fact_key, replaced_by, replaced_at, source_kind, provenance_id, created_at, deleted_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, NULL)",
                params![id, target, original_content, fact_key, replaced_by, now, source_kind, provenance_id, now],
            )?;
            Ok(())
        }).await.map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn list_memory_tombstones(
        &self,
        target: Option<&str>,
        limit: usize,
    ) -> Result<Vec<MemoryTombstoneRow>> {
        let target = target.map(str::to_string);
        self.read_conn.call(move |conn| {
            if let Some(target) = target {
                let mut stmt = conn.prepare(
                    "SELECT id, target, original_content, fact_key, replaced_by, replaced_at, source_kind, provenance_id, created_at FROM memory_tombstones WHERE target = ?1 AND deleted_at IS NULL ORDER BY created_at DESC LIMIT ?2",
                )?;
                let rows = stmt.query_map(params![target, limit as i64], |row| {
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
                })?;
                rows.collect::<std::result::Result<Vec<_>, _>>().map_err(Into::into)
            } else {
                let mut stmt = conn.prepare(
                    "SELECT id, target, original_content, fact_key, replaced_by, replaced_at, source_kind, provenance_id, created_at FROM memory_tombstones WHERE deleted_at IS NULL ORDER BY created_at DESC LIMIT ?1",
                )?;
                let rows = stmt.query_map(params![limit as i64], |row| {
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
                })?;
                rows.collect::<std::result::Result<Vec<_>, _>>().map_err(Into::into)
            }
        }).await.map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn delete_expired_tombstones(&self, max_age_ms: u64, now: u64) -> Result<usize> {
        let cutoff = (now as i64) - (max_age_ms as i64);
        self.conn
            .call(move |conn| {
                let count = conn.execute(
                    "UPDATE memory_tombstones SET deleted_at = ?2 WHERE created_at < ?1 AND deleted_at IS NULL",
                    params![cutoff, now_ts() as i64],
                )?;
                Ok(count)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn restore_tombstone(
        &self,
        tombstone_id: &str,
    ) -> Result<Option<MemoryTombstoneRow>> {
        let tombstone_id = tombstone_id.to_string();
        self.conn.call(move |conn| {
            let row: Option<MemoryTombstoneRow> = conn.query_row(
                "SELECT id, target, original_content, fact_key, replaced_by, replaced_at, source_kind, provenance_id, created_at FROM memory_tombstones WHERE id = ?1 AND deleted_at IS NULL",
                params![tombstone_id],
                |row| {
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
                },
            ).optional()?;
            if row.is_some() {
                conn.execute(
                    "UPDATE memory_tombstones SET deleted_at = ?2 WHERE id = ?1 AND deleted_at IS NULL",
                    params![tombstone_id, now_ts() as i64],
                )?;
            }
            Ok(row)
        }).await.map_err(|e| anyhow::anyhow!("{e}"))
    }

    // ── Consolidation state CRUD (Phase 5) ───────────────────────────────

    pub async fn get_consolidation_state(&self, key: &str) -> Result<Option<String>> {
        let key = key.to_string();
        self.read_conn
            .call(move |conn| {
                let value: Option<String> = conn
                    .query_row(
                        "SELECT value FROM consolidation_state WHERE key = ?1 AND deleted_at IS NULL",
                        params![key],
                        |row| row.get(0),
                    )
                    .optional()?;
                Ok(value)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn set_consolidation_state(&self, key: &str, value: &str, now: u64) -> Result<()> {
        let key = key.to_string();
        let value = value.to_string();
        self.conn.call(move |conn| {
            conn.execute(
                "INSERT OR REPLACE INTO consolidation_state (key, value, updated_at, deleted_at) VALUES (?1, ?2, ?3, NULL)",
                params![key, value, now as i64],
            )?;
            Ok(())
        }).await.map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn delete_consolidation_state(&self, key: &str) -> Result<()> {
        let key = key.to_string();
        self.conn
            .call(move |conn| {
                conn.execute(
                    "UPDATE consolidation_state SET deleted_at = ?2 WHERE key = ?1 AND deleted_at IS NULL",
                    params![key, now_ts() as i64],
                )?;
                Ok(())
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    /// List consolidation_state entries whose key starts with `prefix`.
    pub async fn list_consolidation_state_by_prefix(
        &self,
        prefix: &str,
    ) -> Result<Vec<(String, String)>> {
        let like = format!("{}%", prefix);
        self.read_conn
            .call(move |conn| {
                let mut stmt =
                    conn.prepare("SELECT key, value FROM consolidation_state WHERE key LIKE ?1 AND deleted_at IS NULL")?;
                let rows = stmt.query_map(params![like], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                })?;
                Ok(rows.filter_map(|r| r.ok()).collect())
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
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
        let status = status.to_string();
        let variants = self
            .conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT variant_id, skill_name, variant_name, relative_path, parent_variant_id, version, context_tags_json, use_count, success_count, failure_count, fitness_score, status, last_used_at, created_at, updated_at \
                     FROM skill_variants WHERE status = ?1 ORDER BY updated_at ASC",
                )?;
                let rows = stmt.query_map(params![status], map_skill_variant_row)?;
                Ok(rows.filter_map(|r| r.ok()).collect())
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        super::page_skill_variants(variants, cursor, limit)
    }

    // ── Successful trace queries (Phase 5) ───────────────────────────────

    pub async fn list_recent_successful_traces(
        &self,
        after_timestamp: u64,
        limit: usize,
    ) -> Result<Vec<ExecutionTraceRow>> {
        self.read_conn.call(move |conn| {
            let mut stmt = conn.prepare(
                "SELECT id, goal_run_id, task_id, task_type, outcome, quality_score, tool_sequence_json, metrics_json, duration_ms, tokens_used, created_at FROM execution_traces WHERE outcome = 'success' AND created_at > ?1 ORDER BY created_at ASC LIMIT ?2",
            )?;
            let rows = stmt.query_map(params![after_timestamp as i64, limit as i64], |row| {
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
            })?;
            rows.collect::<std::result::Result<Vec<_>, _>>().map_err(Into::into)
        }).await.map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn trace_has_skill_consultation(
        &self,
        thread_id: Option<&str>,
        task_id: Option<&str>,
        goal_run_id: Option<&str>,
    ) -> Result<bool> {
        let thread_id = thread_id.map(str::to_string);
        let task_id = task_id.map(str::to_string);
        let goal_run_id = goal_run_id.map(str::to_string);
        self.read_conn
            .call(move |conn| {
                let count: i64 = conn.query_row(
                    "SELECT COUNT(1) FROM skill_variant_usage \
                     WHERE ( \
                        (?1 IS NOT NULL AND task_id = ?1) OR \
                        (?2 IS NOT NULL AND goal_run_id = ?2) OR \
                        (?3 IS NOT NULL AND task_id IS NULL AND goal_run_id IS NULL AND thread_id = ?3) \
                     )",
                    params![task_id, goal_run_id, thread_id],
                    |row| row.get(0),
                )?;
                Ok(count > 0)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
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
        self.conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT id, kind, title, excerpt, path, timestamp FROM history_entries \
             WHERE kind = 'managed-command' \
             ORDER BY timestamp DESC LIMIT 20",
                )?;
                let rows = stmt.query_map([], |row| {
                    Ok(HistorySearchHit {
                        id: row.get(0)?,
                        kind: row.get(1)?,
                        title: row.get(2)?,
                        excerpt: row.get(3)?,
                        path: row.get(4)?,
                        timestamp: row.get::<_, i64>(5)? as u64,
                        score: 0.0,
                    })
                })?;

                let hits: Vec<_> = rows.filter_map(|r| r.ok()).collect();
                let mut candidates = Vec::new();

                // Find runs of 3+ successful commands within 5-minute windows
                let mut run: Vec<HistorySearchHit> = Vec::new();
                for hit in &hits {
                    // Check if excerpt indicates success (exit=Some(0))
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
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }
}
