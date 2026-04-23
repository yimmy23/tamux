use super::*;
use uuid::Uuid;

fn overlapping_fact_keys(left: &[String], right: &[String]) -> Vec<String> {
    let left: std::collections::BTreeSet<&str> = left.iter().map(String::as_str).collect();
    let right: std::collections::BTreeSet<&str> = right.iter().map(String::as_str).collect();
    left.intersection(&right)
        .map(|value| (*value).to_string())
        .collect()
}

fn relationship_rows_for_entry(
    conn: &rusqlite::Connection,
    entry_id: &str,
) -> rusqlite::Result<Vec<MemoryProvenanceRelationship>> {
    let mut stmt = conn.prepare(
        "SELECT target_entry_id, relation_type, fact_key FROM memory_provenance_relationships WHERE source_entry_id = ?1 ORDER BY created_at DESC",
    )?;
    let rows = stmt.query_map(params![entry_id], |row| {
        Ok(MemoryProvenanceRelationship {
            related_entry_id: row.get(0)?,
            relation_type: row.get(1)?,
            fact_key: row.get(2)?,
        })
    })?;
    rows.collect()
}

impl HistoryStore {
    /// Verify the hash-chain integrity of all WORM telemetry ledger files.
    pub fn verify_worm_integrity(&self) -> Result<Vec<WormIntegrityResult>> {
        let ledger_kinds = [
            "operational",
            "cognitive",
            "contextual",
            "provenance",
            "episodic",
        ];
        let mut results = Vec::with_capacity(ledger_kinds.len());

        for kind in &ledger_kinds {
            let worm_path = self.worm_dir.join(format!("{}-ledger.jsonl", kind));
            results.push(verify_ledger_file(kind, &worm_path));
        }

        Ok(results)
    }

    pub(super) fn skills_root(&self) -> PathBuf {
        self.skill_dir
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| self.skill_dir.clone())
    }

    pub(super) fn provenance_signing_material(&self) -> Result<ProvenanceSigningMaterial> {
        let root = self
            .telemetry_dir
            .parent()
            .unwrap_or(&self.telemetry_dir)
            .to_path_buf();
        let path = provenance_signing_material_path(&root);
        if path.exists() {
            return load_provenance_signing_material(&path);
        }
        let material = create_provenance_signing_material()?;
        persist_provenance_signing_material(&path, &material)?;
        Ok(material)
    }

    pub(super) fn stored_provenance_signing_material(&self) -> Option<ProvenanceSigningMaterial> {
        let root = self
            .telemetry_dir
            .parent()
            .unwrap_or(&self.telemetry_dir)
            .to_path_buf();
        let path = provenance_signing_material_path(&root);
        if !path.exists() {
            return None;
        }
        load_provenance_signing_material(&path).ok()
    }

    pub(super) fn legacy_provenance_signing_key(&self) -> Option<String> {
        let root = self
            .telemetry_dir
            .parent()
            .unwrap_or(&self.telemetry_dir)
            .to_path_buf();
        let path = legacy_provenance_signing_key_path(&root);
        std::fs::read_to_string(path)
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
    }

    pub async fn upsert_subagent_metrics(
        &self,
        task_id: &str,
        parent_task_id: Option<&str>,
        thread_id: Option<&str>,
        tool_calls_total: i64,
        tool_calls_succeeded: i64,
        tool_calls_failed: i64,
        tokens_consumed: i64,
        context_budget_tokens: Option<i64>,
        progress_rate: f64,
        last_progress_at: Option<u64>,
        stuck_score: f64,
        health_state: &str,
        created_at: u64,
        updated_at: u64,
    ) -> Result<()> {
        let task_id = task_id.to_string();
        let parent_task_id = parent_task_id.map(str::to_string);
        let thread_id = thread_id.map(str::to_string);
        let health_state = health_state.to_string();
        self.conn.call(move |conn| {
        conn.execute(
            "INSERT OR REPLACE INTO subagent_metrics \
             (task_id, parent_task_id, thread_id, tool_calls_total, tool_calls_succeeded, tool_calls_failed, tokens_consumed, context_budget_tokens, progress_rate, last_progress_at, stuck_score, health_state, created_at, updated_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
            params![
                task_id,
                parent_task_id,
                thread_id,
                tool_calls_total,
                tool_calls_succeeded,
                tool_calls_failed,
                tokens_consumed,
                context_budget_tokens,
                progress_rate,
                last_progress_at.map(|v| v as i64),
                stuck_score,
                health_state,
                created_at as i64,
                updated_at as i64,
            ],
        )?;
        Ok(())
        }).await.map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn record_memory_provenance(
        &self,
        record: &MemoryProvenanceRecord<'_>,
    ) -> Result<()> {
        let fact_keys_json = serde_json::to_string(record.fact_keys)?;
        let id = record.id.to_string();
        let target = record.target.to_string();
        let mode = record.mode.to_string();
        let source_kind = record.source_kind.to_string();
        let content = record.content.to_string();
        let thread_id = record.thread_id.map(str::to_string);
        let task_id = record.task_id.map(str::to_string);
        let goal_run_id = record.goal_run_id.map(str::to_string);
        let created_at = record.created_at;
        let fact_keys_owned: Vec<String> = record.fact_keys.to_vec();
        let relationship_target = match record.mode {
            "remove" => Some((
                target.clone(),
                id.clone(),
                fact_keys_owned.clone(),
                created_at,
                "retracts",
            )),
            "conflict" => Some((
                target.clone(),
                id.clone(),
                fact_keys_owned.clone(),
                created_at,
                "contradicts",
            )),
            _ => None,
        };

        let record = record;
        self.conn.call(move |conn| {
            conn.execute(
                "INSERT OR REPLACE INTO memory_provenance \
                 (id, target, mode, source_kind, content, fact_keys_json, thread_id, task_id, goal_run_id, created_at, confirmed_at, retracted_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, NULL, NULL)",
                params![
                    id,
                    target,
                    mode,
                    source_kind,
                    content,
                    fact_keys_json,
                    thread_id,
                    task_id,
                    goal_run_id,
                    created_at as i64,
                ],
            )?;
            if let Some((target, source_entry_id, fact_keys, created_at, relation_type)) = relationship_target {
                let mut stmt = conn.prepare(
                    "SELECT id, fact_keys_json FROM memory_provenance WHERE target = ?1 AND id != ?2 AND mode NOT IN ('remove', 'conflict') ORDER BY created_at DESC LIMIT 64",
                )?;
                let rows = stmt.query_map(params![target, source_entry_id], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                })?;
                for row in rows {
                    let (target_entry_id, fact_keys_json) = row?;
                    let target_fact_keys = serde_json::from_str::<Vec<String>>(&fact_keys_json)
                        .unwrap_or_default();
                    for fact_key in overlapping_fact_keys(&fact_keys, &target_fact_keys) {
                        conn.execute(
                            "INSERT OR IGNORE INTO memory_provenance_relationships (id, source_entry_id, target_entry_id, relation_type, fact_key, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                            params![
                                format!("memrel_{}", Uuid::new_v4()),
                                source_entry_id,
                                target_entry_id,
                                relation_type,
                                fact_key,
                                created_at as i64,
                            ],
                        )?;
                    }
                }
            }
            Ok(())
        }).await.map_err(|e| anyhow::anyhow!("{e}"))?;
        self.append_telemetry(
            "cognitive",
            json!({
                "timestamp": record.created_at as i64,
                "kind": "memory_write",
                "target": record.target,
                "mode": record.mode,
                "source_kind": record.source_kind,
                "thread_id": record.thread_id,
                "task_id": record.task_id,
                "goal_run_id": record.goal_run_id,
                "fact_keys": fact_keys_owned,
            }),
        )
        .await?;
        Ok(())
    }

    pub async fn confirm_memory_provenance_entry(
        &self,
        entry_id: &str,
        confirmed_at: u64,
    ) -> Result<bool> {
        let entry_id = entry_id.to_string();
        let confirmed_at = confirmed_at as i64;
        let updated = self
            .conn
            .call(move |conn| {
                let updated = conn.execute(
                    "UPDATE memory_provenance SET confirmed_at = ?2, retracted_at = NULL WHERE id = ?1",
                    params![entry_id, confirmed_at],
                )?;
                Ok(updated)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        Ok(updated > 0)
    }

    pub async fn retract_memory_provenance_entry(
        &self,
        entry_id: &str,
        retracted_at: u64,
    ) -> Result<bool> {
        let entry_id = entry_id.to_string();
        let retracted_at = retracted_at as i64;
        let updated = self
            .conn
            .call(move |conn| {
                let updated = conn.execute(
                    "UPDATE memory_provenance SET retracted_at = ?2, confirmed_at = NULL WHERE id = ?1",
                    params![entry_id, retracted_at],
                )?;
                Ok(updated)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        Ok(updated > 0)
    }

    pub async fn memory_provenance_report(
        &self,
        target: Option<&str>,
        limit: usize,
    ) -> Result<MemoryProvenanceReport> {
        let target = target.map(str::to_string);
        self.conn.call(move |conn| {
        let limit = limit.clamp(1, 200);
        let normalized_target = target
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);

        let mut entries = Vec::new();
        let mut summary_by_target = BTreeMap::new();
        let mut summary_by_source = BTreeMap::new();
        let mut summary_by_status = BTreeMap::new();

        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        match normalized_target.as_deref() {
            Some(target) => {
                let mut stmt = conn.prepare(
                    "SELECT id, target, mode, source_kind, content, fact_keys_json, thread_id, task_id, goal_run_id, created_at, confirmed_at, retracted_at \
                     FROM memory_provenance WHERE target = ?1 ORDER BY created_at DESC LIMIT ?2",
                )?;
                let rows = stmt.query_map(params![target, limit as i64], |row| {
                    Ok(memory_provenance_entry_from_row(row, now_ms))
                })?;
                for row in rows {
                    let mut entry = row?;
                    entry.relationships = relationship_rows_for_entry(conn, &entry.id)?;
                    *summary_by_target.entry(entry.target.clone()).or_insert(0) += 1;
                    *summary_by_source
                        .entry(entry.source_kind.clone())
                        .or_insert(0) += 1;
                    *summary_by_status.entry(entry.status.clone()).or_insert(0) += 1;
                    entries.push(entry);
                }
            }
            None => {
                let mut stmt = conn.prepare(
                    "SELECT id, target, mode, source_kind, content, fact_keys_json, thread_id, task_id, goal_run_id, created_at, confirmed_at, retracted_at \
                     FROM memory_provenance ORDER BY created_at DESC LIMIT ?1",
                )?;
                let rows = stmt.query_map(params![limit as i64], |row| {
                    Ok(memory_provenance_entry_from_row(row, now_ms))
                })?;
                for row in rows {
                    let mut entry = row?;
                    entry.relationships = relationship_rows_for_entry(conn, &entry.id)?;
                    *summary_by_target.entry(entry.target.clone()).or_insert(0) += 1;
                    *summary_by_source
                        .entry(entry.source_kind.clone())
                        .or_insert(0) += 1;
                    *summary_by_status.entry(entry.status.clone()).or_insert(0) += 1;
                    entries.push(entry);
                }
            }
        }

        Ok(MemoryProvenanceReport {
            total_entries: entries.len(),
            target_filter: normalized_target,
            summary_by_target,
            summary_by_source,
            summary_by_status,
            entries,
        })
        }).await.map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn record_provenance_event(&self, record: &ProvenanceEventRecord<'_>) -> Result<()> {
        let telemetry_path = self.telemetry_dir.join("provenance.jsonl");
        let previous_entry = read_last_provenance_entry(&telemetry_path);
        let sequence = previous_entry
            .as_ref()
            .map(|entry| entry.sequence.saturating_add(1))
            .unwrap_or(0);
        let prev_hash = previous_entry
            .as_ref()
            .map(|entry| entry.entry_hash.clone())
            .unwrap_or_else(|| "genesis".to_string());
        let entry_hash = compute_provenance_hash(
            sequence,
            record.created_at,
            record.event_type,
            record.summary,
            record.details,
            &prev_hash,
            record.agent_id,
            record.goal_run_id,
            record.task_id,
            record.thread_id,
            record.approval_id,
            record.causal_trace_id,
            record.compliance_mode,
        );
        let signing_material = if record.sign {
            Some(self.provenance_signing_material()?)
        } else {
            None
        };
        let signature = signing_material
            .as_ref()
            .map(|material| sign_provenance_hash_ed25519(material, &entry_hash))
            .transpose()?;
        let entry = ProvenanceLogEntry {
            sequence,
            timestamp: record.created_at,
            event_type: record.event_type.to_string(),
            summary: record.summary.to_string(),
            details: record.details.clone(),
            prev_hash,
            entry_hash,
            signature,
            signature_scheme: signing_material
                .as_ref()
                .map(|material| material.scheme.clone()),
            agent_id: record.agent_id.to_string(),
            goal_run_id: record.goal_run_id.map(str::to_string),
            task_id: record.task_id.map(str::to_string),
            thread_id: record.thread_id.map(str::to_string),
            approval_id: record.approval_id.map(str::to_string),
            causal_trace_id: record.causal_trace_id.map(str::to_string),
            compliance_mode: record.compliance_mode.to_string(),
        };

        self.append_telemetry("provenance", serde_json::to_value(entry)?)
            .await?;
        Ok(())
    }

    pub fn provenance_report(&self, limit: usize) -> Result<ProvenanceReport> {
        let entries = read_provenance_entries(&self.telemetry_dir.join("provenance.jsonl"))?;
        let signing_material = self.stored_provenance_signing_material();
        let legacy_signing_key = self.legacy_provenance_signing_key();
        let mut summary_by_event = BTreeMap::new();
        let mut valid_hash_entries = 0usize;
        let mut valid_signature_entries = 0usize;
        let mut valid_chain_entries = 0usize;
        let mut signed_entries = 0usize;
        let mut previous_hash = "genesis".to_string();
        let mut report_entries = Vec::new();

        for entry in entries.iter() {
            let expected_hash = compute_provenance_hash(
                entry.sequence,
                entry.timestamp,
                &entry.event_type,
                &entry.summary,
                &entry.details,
                &entry.prev_hash,
                &entry.agent_id,
                entry.goal_run_id.as_deref(),
                entry.task_id.as_deref(),
                entry.thread_id.as_deref(),
                entry.approval_id.as_deref(),
                entry.causal_trace_id.as_deref(),
                &entry.compliance_mode,
            );
            let hash_valid = entry.entry_hash == expected_hash;
            let chain_valid = entry.prev_hash == previous_hash;
            let signature_valid = match (&entry.signature, entry.signature_scheme.as_deref()) {
                (Some(signature), Some(scheme))
                    if scheme == provenance_signature_scheme_ed25519() =>
                {
                    signed_entries += 1;
                    signing_material.as_ref().is_some_and(|material| {
                        verify_provenance_signature_ed25519(material, &entry.entry_hash, signature)
                    })
                }
                (Some(signature), None) => {
                    signed_entries += 1;
                    legacy_signing_key.as_deref().is_some_and(|key| {
                        *signature == sign_provenance_hash(key, &entry.entry_hash)
                    })
                }
                (Some(_), Some(_)) => {
                    signed_entries += 1;
                    false
                }
                (None, _) => true,
            };
            if hash_valid {
                valid_hash_entries += 1;
            }
            if chain_valid {
                valid_chain_entries += 1;
            }
            if signature_valid {
                valid_signature_entries += 1;
            }
            *summary_by_event
                .entry(entry.event_type.clone())
                .or_insert(0) += 1;
            report_entries.push(ProvenanceReportEntry {
                sequence: entry.sequence,
                timestamp: entry.timestamp,
                event_type: entry.event_type.clone(),
                summary: entry.summary.clone(),
                signature_scheme: entry.signature_scheme.clone(),
                agent_id: entry.agent_id.clone(),
                goal_run_id: entry.goal_run_id.clone(),
                task_id: entry.task_id.clone(),
                thread_id: entry.thread_id.clone(),
                approval_id: entry.approval_id.clone(),
                causal_trace_id: entry.causal_trace_id.clone(),
                compliance_mode: entry.compliance_mode.clone(),
                hash_valid,
                signature_valid,
                chain_valid,
            });
            previous_hash = entry.entry_hash.clone();
        }

        report_entries.reverse();
        report_entries.truncate(limit.clamp(1, 500));

        Ok(ProvenanceReport {
            total_entries: entries.len(),
            signed_entries,
            valid_hash_entries,
            valid_signature_entries,
            valid_chain_entries,
            summary_by_event,
            entries: report_entries,
        })
    }

    pub fn generate_soc2_artifact(&self, period_days: u32) -> Result<PathBuf> {
        let entries = read_provenance_entries(&self.telemetry_dir.join("provenance.jsonl"))?;
        let cutoff = now_ts()
            .saturating_sub((period_days as u64).saturating_mul(86_400))
            .saturating_mul(1000);
        let recent = entries
            .into_iter()
            .filter(|entry| entry.timestamp >= cutoff)
            .collect::<Vec<_>>();
        let integrity = self.verify_worm_integrity()?;
        let artifact = json!({
            "generated_at": now_ts() * 1000,
            "period_days": period_days,
            "change_management": recent.iter().filter(|entry| matches!(entry.event_type.as_str(), "goal_created" | "plan_generated" | "step_started" | "step_completed" | "step_failed" | "replan_triggered" | "recovery_triggered" | "tool_call")).collect::<Vec<_>>(),
            "system_access": recent.iter().filter(|entry| matches!(entry.event_type.as_str(), "approval_requested" | "approval_granted" | "approval_denied" | "escalation_triggered")).collect::<Vec<_>>(),
            "data_integrity": integrity.iter().map(|item| json!({
                "kind": item.kind,
                "valid": item.valid,
                "total_entries": item.total_entries,
                "message": item.message,
            })).collect::<Vec<_>>(),
            "incident_log": recent.iter().filter(|entry| matches!(entry.event_type.as_str(), "step_failed" | "recovery_triggered" | "escalation_triggered")).collect::<Vec<_>>(),
        });

        let audit_dir = self
            .telemetry_dir
            .parent()
            .unwrap_or(&self.telemetry_dir)
            .join("audit")
            .join("soc2");
        std::fs::create_dir_all(&audit_dir)?;
        let path = audit_dir.join(format!("soc2-artifact-{}.json", now_ts()));
        std::fs::write(&path, serde_json::to_string_pretty(&artifact)?)?;
        Ok(path)
    }

    pub async fn get_subagent_metrics(&self, task_id: &str) -> Result<Option<SubagentMetrics>> {
        let task_id = task_id.to_string();
        self.conn.call(move |conn| {
        conn
            .query_row(
                "SELECT task_id, parent_task_id, thread_id, tool_calls_total, tool_calls_succeeded, tool_calls_failed, tokens_consumed, context_budget_tokens, progress_rate, last_progress_at, stuck_score, health_state, created_at, updated_at \
                 FROM subagent_metrics WHERE task_id = ?1",
                params![task_id],
                |row| {
                    Ok(SubagentMetrics {
                        task_id: row.get(0)?,
                        parent_task_id: row.get(1)?,
                        thread_id: row.get(2)?,
                        tool_calls_total: row.get(3)?,
                        tool_calls_succeeded: row.get(4)?,
                        tool_calls_failed: row.get(5)?,
                        tokens_consumed: row.get(6)?,
                        context_budget_tokens: row.get(7)?,
                        progress_rate: row.get(8)?,
                        last_progress_at: row.get::<_, Option<i64>>(9)?.map(|v| v as u64),
                        stuck_score: row.get(10)?,
                        health_state: row.get(11)?,
                        created_at: row.get::<_, i64>(12)? as u64,
                        updated_at: row.get::<_, i64>(13)? as u64,
                    })
                },
            )
            .optional()
            .map_err(Into::into)
        }).await.map_err(|e| anyhow::anyhow!("{e}"))
    }
}
