use super::*;

impl HistoryStore {
    pub async fn inspect_skill_variants(
        &self,
        skill: &str,
        context_tags: &[String],
    ) -> Result<Vec<SkillVariantInspection>> {
        let normalized = normalize_skill_lookup(skill);
        if normalized.is_empty() {
            return Ok(Vec::new());
        }
        let context_tags = context_tags.to_vec();
        self.conn
            .call(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT variant_id, skill_name, variant_name, relative_path, parent_variant_id, version, context_tags_json, use_count, success_count, failure_count, status, last_used_at, created_at, updated_at \
                     FROM skill_variants",
                )?;
                let rows = stmt.query_map([], map_skill_variant_row)?;
                let mut variants = rows
                    .filter_map(|row| row.ok())
                    .filter(|record| skill_variant_matches(record, &normalized))
                    .collect::<Vec<_>>();
                variants.sort_by(|left, right| compare_skill_variants(left, right, &context_tags));
                let selected_id = variants.first().map(|record| record.variant_id.clone());
                let now = now_ts();
                Ok(variants
                    .into_iter()
                    .map(|record| SkillVariantInspection {
                        lifecycle_summary: describe_skill_variant_lifecycle(&record, now),
                        selection_summary: describe_skill_variant_selection(&record, &context_tags),
                        selected_for_context: selected_id.as_deref()
                            == Some(record.variant_id.as_str()),
                        record,
                    })
                    .collect::<Vec<_>>())
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn register_skill_document(&self, path: &Path) -> Result<SkillVariantRecord> {
        let skills_root = self.skills_root();
        let canonical = std::fs::canonicalize(path)
            .with_context(|| format!("failed to access skill document {}", path.display()))?;
        if !canonical.starts_with(&skills_root) {
            anyhow::bail!(
                "skill document {} must stay inside {}",
                canonical.display(),
                skills_root.display()
            );
        }

        let relative_path = canonical
            .strip_prefix(&skills_root)
            .unwrap_or(canonical.as_path())
            .to_string_lossy()
            .replace('\\', "/");
        let content = std::fs::read_to_string(&canonical)
            .with_context(|| format!("failed to read skill document {}", canonical.display()))?;
        let metadata = derive_skill_metadata(&relative_path, &content);
        let now = now_ts();
        let context_tags_json = serde_json::to_string(&metadata.context_tags)?;
        let skill_name = metadata.skill_name.clone();
        let variant_name = metadata.variant_name.clone();

        let path = path.clone();
        let variant_id = self.conn.call(move |conn| {
            let existing: Option<SkillVariantRecord> = conn
                .query_row(
                    "SELECT variant_id, skill_name, variant_name, relative_path, parent_variant_id, version, context_tags_json, use_count, success_count, failure_count, status, last_used_at, created_at, updated_at \
                     FROM skill_variants WHERE relative_path = ?1",
                    params![relative_path],
                    map_skill_variant_row,
                )
                .optional()?;

            let variant_id = existing
                .as_ref()
                .map(|record| record.variant_id.clone())
                .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
            let version = existing
                .as_ref()
                .map(|record| record.version.clone())
                .unwrap_or_else(|| {
                    let version_num: i64 = conn
                        .query_row(
                            "SELECT COUNT(*) FROM skill_variants WHERE skill_name = ?1",
                            params![skill_name.as_str()],
                            |row| row.get(0),
                        )
                        .unwrap_or(0);
                    format!("v{}.0", version_num + 1)
                });
            let parent_variant_id: Option<String> = if variant_name == "canonical" {
                None
            } else {
                conn.query_row(
                    "SELECT variant_id FROM skill_variants WHERE skill_name = ?1 AND variant_name = 'canonical' LIMIT 1",
                    params![skill_name.as_str()],
                    |row| row.get(0),
                )
                .optional()?
            };
            let created_at = existing
                .as_ref()
                .map(|record| record.created_at)
                .unwrap_or(now);
            let last_used_at = existing.as_ref().and_then(|record| record.last_used_at);
            let use_count = existing
                .as_ref()
                .map(|record| record.use_count)
                .unwrap_or(0);
            let success_count = existing
                .as_ref()
                .map(|record| record.success_count)
                .unwrap_or(0);
            let failure_count = existing
                .as_ref()
                .map(|record| record.failure_count)
                .unwrap_or(0);
            let status = existing
                .as_ref()
                .map(|record| record.status.clone())
                .unwrap_or_else(|| "active".to_string());

            conn.execute(
                "INSERT OR REPLACE INTO skill_variants \
                 (variant_id, skill_name, variant_name, relative_path, parent_variant_id, version, context_tags_json, use_count, success_count, failure_count, status, last_used_at, created_at, updated_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
                params![
                    variant_id,
                    skill_name,
                    variant_name,
                    relative_path,
                    parent_variant_id,
                    version,
                    context_tags_json,
                    use_count as i64,
                    success_count as i64,
                    failure_count as i64,
                    status,
                    last_used_at.map(|value| value as i64),
                    created_at as i64,
                    now as i64,
                ],
            )?;

            Ok(variant_id)
        }).await.map_err(|e| anyhow::anyhow!("{e}"))?;

        self.rebalance_skill_variants(&metadata.skill_name).await?;

        let vid = variant_id.clone();
        self.conn.call(move |conn| {
            conn.query_row(
                "SELECT variant_id, skill_name, variant_name, relative_path, parent_variant_id, version, context_tags_json, use_count, success_count, failure_count, status, last_used_at, created_at, updated_at \
                 FROM skill_variants WHERE variant_id = ?1",
                params![vid],
                map_skill_variant_row,
            )
            .map_err(Into::into)
        }).await.context("failed to load rebalanced skill variant")
    }

    pub async fn list_skill_variants(
        &self,
        query: Option<&str>,
        limit: usize,
    ) -> Result<Vec<SkillVariantRecord>> {
        let normalized_query = query
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| value.to_ascii_lowercase());
        let limit = limit.clamp(1, 200) as i64;

        let query = query.map(str::to_string);
        self.conn.call(move |conn| {
            let mut variants = if let Some(query) = normalized_query.as_deref() {
                let like = format!("%{query}%");
                let mut stmt = conn.prepare(
                    "SELECT variant_id, skill_name, variant_name, relative_path, parent_variant_id, version, context_tags_json, use_count, success_count, failure_count, status, last_used_at, created_at, updated_at \
                     FROM skill_variants \
                     WHERE lower(skill_name) LIKE ?1 OR lower(variant_name) LIKE ?1 OR lower(relative_path) LIKE ?1 OR lower(context_tags_json) LIKE ?1 \
                     ORDER BY updated_at DESC LIMIT ?2",
                )?;
                let rows = stmt.query_map(params![like, limit], map_skill_variant_row)?;
                rows.filter_map(|row| row.ok()).collect::<Vec<_>>()
            } else {
                let mut stmt = conn.prepare(
                    "SELECT variant_id, skill_name, variant_name, relative_path, parent_variant_id, version, context_tags_json, use_count, success_count, failure_count, status, last_used_at, created_at, updated_at \
                     FROM skill_variants \
                     ORDER BY updated_at DESC LIMIT ?1",
                )?;
                let rows = stmt.query_map(params![limit], map_skill_variant_row)?;
                rows.filter_map(|row| row.ok()).collect::<Vec<_>>()
            };

            variants.sort_by(|left, right| compare_skill_variants(left, right, &[]));
            Ok(variants)
        }).await.map_err(|e| anyhow::anyhow!("{e}"))
    }

    /// Update the maturity status of a skill variant and bump `updated_at`.
    pub async fn update_skill_variant_status(
        &self,
        variant_id: &str,
        new_status: &str,
    ) -> Result<()> {
        let variant_id = variant_id.to_string();
        let new_status = new_status.to_string();
        self.conn
            .call(move |conn| {
                let now = now_ts() as i64;
                conn.execute(
                    "UPDATE skill_variants SET status = ?2, updated_at = ?3 WHERE variant_id = ?1",
                    params![variant_id, new_status, now],
                )?;
                Ok(())
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    /// Retrieve a single skill variant by its `variant_id`.
    pub async fn get_skill_variant(&self, variant_id: &str) -> Result<Option<SkillVariantRecord>> {
        let variant_id = variant_id.to_string();
        self.conn
            .call(move |conn| {
                conn.query_row(
                    "SELECT variant_id, skill_name, variant_name, relative_path, parent_variant_id, version, context_tags_json, use_count, success_count, failure_count, status, last_used_at, created_at, updated_at \
                     FROM skill_variants WHERE variant_id = ?1",
                    params![variant_id],
                    map_skill_variant_row,
                )
                .optional()
                .map_err(Into::into)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn resolve_skill_variant(
        &self,
        skill: &str,
        context_tags: &[String],
    ) -> Result<Option<SkillVariantRecord>> {
        let normalized = normalize_skill_lookup(skill);
        if normalized.is_empty() {
            return Ok(None);
        }
        let context_tags = context_tags.to_vec();

        let skill = skill.to_string();
        self.conn.call(move |conn| {
            let mut stmt = conn.prepare(
                "SELECT variant_id, skill_name, variant_name, relative_path, parent_variant_id, version, context_tags_json, use_count, success_count, failure_count, status, last_used_at, created_at, updated_at \
                 FROM skill_variants",
            )?;
            let rows = stmt.query_map([], map_skill_variant_row)?;
            let mut candidates = rows
                .filter_map(|row| row.ok())
                .filter(|record| skill_variant_matches(record, &normalized))
                .filter(|record| record.status != "archived" && record.status != "merged")
                .collect::<Vec<_>>();
            if candidates.is_empty() {
                return Ok(None);
            }
            candidates.sort_by(|left, right| compare_skill_variants(left, right, &context_tags));
            Ok(candidates.into_iter().next())
        }).await.map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn record_skill_variant_use(
        &self,
        variant_id: &str,
        outcome: Option<bool>,
    ) -> Result<()> {
        let variant_id = variant_id.to_string();
        let variant_id = variant_id.to_string();
        self.conn.call(move |conn| {
            let now = now_ts() as i64;
            match outcome {
                Some(true) => {
                    conn.execute(
                        "UPDATE skill_variants \
                         SET use_count = use_count + 1, success_count = success_count + 1, last_used_at = ?2, updated_at = ?2 \
                         WHERE variant_id = ?1",
                        params![variant_id, now],
                    )?;
                }
                Some(false) => {
                    conn.execute(
                        "UPDATE skill_variants \
                         SET use_count = use_count + 1, failure_count = failure_count + 1, last_used_at = ?2, updated_at = ?2 \
                         WHERE variant_id = ?1",
                        params![variant_id, now],
                    )?;
                }
                None => {
                    conn.execute(
                        "UPDATE skill_variants \
                         SET use_count = use_count + 1, last_used_at = ?2, updated_at = ?2 \
                         WHERE variant_id = ?1",
                        params![variant_id, now],
                    )?;
                }
            }
            Ok(())
        }).await.map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn record_skill_variant_consultation(
        &self,
        record: &SkillVariantConsultationRecord<'_>,
    ) -> Result<()> {
        let now = record.consulted_at as i64;
        let context_tags_json = serde_json::to_string(record.context_tags)?;
        let usage_id = record.usage_id.to_string();
        let variant_id = record.variant_id.to_string();
        let thread_id = record.thread_id.map(str::to_string);
        let task_id = record.task_id.map(str::to_string);
        let goal_run_id = record.goal_run_id.map(str::to_string);
        let context_tags_owned: Vec<String> = record.context_tags.to_vec();

        let record = record.clone();
        self.conn.call(move |conn| {
            conn.execute(
                "INSERT OR REPLACE INTO skill_variant_usage \
                 (usage_id, variant_id, thread_id, task_id, goal_run_id, context_tags_json, consulted_at, resolved_at, outcome) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, NULL, NULL)",
                params![
                    usage_id,
                    variant_id,
                    thread_id,
                    task_id,
                    goal_run_id,
                    context_tags_json,
                    now,
                ],
            )?;
            conn.execute(
                "UPDATE skill_variants \
                 SET use_count = use_count + 1, last_used_at = ?2, updated_at = ?2 \
                 WHERE variant_id = ?1",
                params![variant_id, now],
            )?;
            Ok(())
        }).await.map_err(|e| anyhow::anyhow!("{e}"))?;
        self.append_telemetry(
            "cognitive",
            json!({
                "timestamp": now,
                "kind": "skill_variant_consulted",
                "variant_id": record.variant_id,
                "thread_id": record.thread_id,
                "task_id": record.task_id,
                "goal_run_id": record.goal_run_id,
                "context_tags": context_tags_owned,
            }),
        )
        .await?;
        Ok(())
    }
}
