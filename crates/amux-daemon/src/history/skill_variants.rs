use super::*;

impl HistoryStore {
    pub async fn settle_skill_variant_usage(
        &self,
        thread_id: Option<&str>,
        task_id: Option<&str>,
        goal_run_id: Option<&str>,
        outcome: &str,
    ) -> Result<(usize, Vec<String>)> {
        let normalized_outcome = outcome.trim().to_ascii_lowercase();
        if !matches!(
            normalized_outcome.as_str(),
            "success" | "failure" | "cancelled"
        ) {
            anyhow::bail!("invalid skill usage outcome '{outcome}'");
        }

        let thread_id_owned = thread_id.map(str::to_string);
        let task_id_owned = task_id.map(str::to_string);
        let goal_run_id_owned = goal_run_id.map(str::to_string);
        let outcome_clone = normalized_outcome.clone();

        let thread_id = thread_id.map(str::to_string);
        let task_id = task_id.map(str::to_string);
        let goal_run_id = goal_run_id.map(str::to_string);
        let outcome = outcome.to_string();
        let (pending_len, skill_names) = self.conn.call(move |conn| {
            let mut stmt = conn.prepare(
                "SELECT usage_id, variant_id FROM skill_variant_usage \
                 WHERE resolved_at IS NULL AND ( \
                    (?1 IS NOT NULL AND task_id = ?1) OR \
                    (?2 IS NOT NULL AND goal_run_id = ?2) OR \
                    (?3 IS NOT NULL AND task_id IS NULL AND goal_run_id IS NULL AND thread_id = ?3) \
                 )",
            )?;
            let rows = stmt.query_map(params![task_id_owned, goal_run_id_owned, thread_id_owned], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?;
            let pending = rows.filter_map(|row| row.ok()).collect::<Vec<_>>();
            if pending.is_empty() {
                return Ok((0usize, BTreeSet::<String>::new()));
            }

            let resolved_at = now_ts() as i64;
            let variant_ids = pending
                .iter()
                .map(|(_, variant_id)| variant_id.clone())
                .collect::<BTreeSet<_>>();
            let mut success_counts = BTreeMap::<String, usize>::new();
            let mut failure_counts = BTreeMap::<String, usize>::new();
            for (usage_id, variant_id) in &pending {
                conn.execute(
                    "UPDATE skill_variant_usage SET resolved_at = ?2, outcome = ?3 WHERE usage_id = ?1",
                    params![usage_id, resolved_at, outcome_clone.as_str()],
                )?;
                if outcome_clone == "success" {
                    *success_counts.entry(variant_id.clone()).or_default() += 1;
                } else {
                    *failure_counts.entry(variant_id.clone()).or_default() += 1;
                }
            }

            for (variant_id, count) in success_counts {
                conn.execute(
                    "UPDATE skill_variants \
                     SET success_count = success_count + ?2, updated_at = ?3 \
                     WHERE variant_id = ?1",
                    params![variant_id, count as i64, resolved_at],
                )?;
            }
            for (variant_id, count) in failure_counts {
                conn.execute(
                    "UPDATE skill_variants \
                     SET failure_count = failure_count + ?2, updated_at = ?3 \
                     WHERE variant_id = ?1",
                    params![variant_id, count as i64, resolved_at],
                )?;
            }

            let skill_names = variant_ids
                .into_iter()
                .filter_map(|variant_id| {
                    conn.query_row(
                        "SELECT skill_name FROM skill_variants WHERE variant_id = ?1",
                        params![variant_id],
                        |row| row.get::<_, String>(0),
                    )
                    .optional()
                    .ok()
                    .flatten()
                })
                .collect::<BTreeSet<_>>();

            Ok((pending.len(), skill_names))
        }).await.map_err(|e| anyhow::anyhow!("{e}"))?;

        if pending_len == 0 {
            return Ok((0, Vec::new()));
        }

        self.append_telemetry(
            "cognitive",
            json!({
                "timestamp": now_ts() as i64,
                "kind": "skill_variant_settled",
                "thread_id": thread_id,
                "task_id": task_id,
                "goal_run_id": goal_run_id,
                "outcome": normalized_outcome,
                "count": pending_len,
            }),
        )
        .await?;

        for skill_name in &skill_names {
            let _ = self.rebalance_skill_variants(&skill_name).await;
            if normalized_outcome == "success" {
                let _ = self.maybe_branch_skill_variants(&skill_name).await;
                let _ = self.maybe_merge_skill_variants(&skill_name).await;
            }
        }
        Ok((pending_len, skill_names.into_iter().collect()))
    }

    pub async fn rebalance_skill_variants(
        &self,
        skill_name: &str,
    ) -> Result<Vec<SkillVariantRecord>> {
        let skill_name = skill_name.to_string();
        let skill_name = skill_name.to_string();
        self.conn.call(move |conn| {
            let mut stmt = conn.prepare(
                "SELECT variant_id, skill_name, variant_name, relative_path, parent_variant_id, version, context_tags_json, use_count, success_count, failure_count, status, last_used_at, created_at, updated_at \
                 FROM skill_variants WHERE skill_name = ?1",
            )?;
            let rows = stmt.query_map(params![skill_name], map_skill_variant_row)?;
            let mut variants = rows.filter_map(|row| row.ok()).collect::<Vec<_>>();
            if variants.is_empty() {
                return Ok(Vec::new());
            }

            let now = now_ts();
            let canonical = variants
                .iter()
                .find(|variant| variant.is_canonical())
                .cloned();
            let canonical_success_rate = canonical
                .as_ref()
                .map(SkillVariantRecord::success_rate)
                .unwrap_or(0.0);
            let promoted_variant_id = variants
                .iter()
                .filter(|variant| !variant.is_canonical())
                .filter(|variant| {
                    variant.use_count >= SKILL_PROMOTION_MIN_USES
                        && variant.success_count >= SKILL_PROMOTION_MIN_SUCCESS_COUNT
                        && variant.success_rate() >= SKILL_PROMOTION_SUCCESS_RATE_THRESHOLD
                        && variant.success_rate() > canonical_success_rate + SKILL_PROMOTION_MARGIN
                })
                .max_by(|left, right| compare_skill_variants(left, right, &[]))
                .map(|variant| variant.variant_id.clone());

            for variant in &mut variants {
                let next_status =
                    rebalance_skill_variant_status(variant, promoted_variant_id.as_deref(), now);
                if next_status != variant.status {
                    conn.execute(
                        "UPDATE skill_variants SET status = ?2, updated_at = ?3 WHERE variant_id = ?1",
                        params![variant.variant_id, next_status, now as i64],
                    )?;
                    variant.status = next_status.to_string();
                    variant.updated_at = now;
                }
            }

            variants.sort_by(|left, right| compare_skill_variants(left, right, &[]));
            Ok(variants)
        }).await.map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn maybe_branch_skill_variants(
        &self,
        skill_name: &str,
    ) -> Result<Vec<SkillVariantRecord>> {
        let skill_name_owned = skill_name.to_string();
        let skill_name = skill_name.to_string();
        let candidates = self.conn.call(move |conn| {
            let mut stmt = conn.prepare(
                "SELECT skill_variants.variant_id, skill_variants.relative_path, skill_variants.context_tags_json, skill_variant_usage.context_tags_json \
                 FROM skill_variant_usage \
                 JOIN skill_variants ON skill_variants.variant_id = skill_variant_usage.variant_id \
                 WHERE skill_variants.skill_name = ?1 AND skill_variant_usage.outcome = 'success'",
            )?;
            let rows = stmt.query_map(params![skill_name_owned], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                ))
            })?;

            let mut candidates = BTreeMap::<(String, String), BranchCandidate>::new();
            for row in rows.filter_map(|row| row.ok()) {
                let (variant_id, relative_path, variant_tags_json, usage_tags_json) = row;
                let variant_tags =
                    serde_json::from_str::<Vec<String>>(&variant_tags_json).unwrap_or_default();
                let usage_tags =
                    serde_json::from_str::<Vec<String>>(&usage_tags_json).unwrap_or_default();
                let mismatch = usage_tags
                    .into_iter()
                    .map(|value| value.to_ascii_lowercase())
                    .filter(|tag| {
                        !variant_tags
                            .iter()
                            .any(|existing| existing.eq_ignore_ascii_case(tag))
                    })
                    .collect::<BTreeSet<_>>();
                if mismatch.len() < 2 {
                    continue;
                }
                let branch_tags = mismatch.into_iter().take(3).collect::<Vec<_>>();
                let branch_key = branch_tags.join("-");
                let entry = candidates
                    .entry((variant_id.clone(), branch_key.clone()))
                    .or_insert_with(|| BranchCandidate {
                        source_variant_id: variant_id.clone(),
                        source_relative_path: relative_path.clone(),
                        branch_tags: branch_tags.clone(),
                        success_count: 0,
                    });
                entry.success_count += 1;
            }
            Ok(candidates)
        }).await.map_err(|e| anyhow::anyhow!("{e}"))?;

        let existing = self.list_skill_variants(Some(&skill_name), 200).await?;
        let mut created = Vec::new();
        for candidate in candidates.into_values() {
            if candidate.success_count < 3 {
                continue;
            }
            if existing.iter().any(|variant| {
                variant.status != "archived"
                    && skill_variant_covers_branch_tags(variant, &candidate.branch_tags)
            }) {
                continue;
            }
            if let Some(record) = self
                .create_branched_skill_variant(&skill_name, &candidate)
                .await?
            {
                created.push(record);
            }
        }

        if !created.is_empty() {
            let _ = self.rebalance_skill_variants(&skill_name).await;
        }
        Ok(created)
    }

    pub async fn maybe_merge_skill_variants(
        &self,
        skill_name: &str,
    ) -> Result<Vec<SkillVariantRecord>> {
        let skill_name_owned = skill_name.to_string();
        let skill_name = skill_name.to_string();
        let variants = self.conn.call(move |conn| {
            let mut stmt = conn.prepare(
                "SELECT variant_id, skill_name, variant_name, relative_path, parent_variant_id, version, context_tags_json, use_count, success_count, failure_count, status, last_used_at, created_at, updated_at \
                 FROM skill_variants WHERE skill_name = ?1",
            )?;
            let rows = stmt.query_map(params![skill_name_owned], map_skill_variant_row)?;
            Ok(rows.filter_map(|row| row.ok()).collect::<Vec<_>>())
        }).await.map_err(|e| anyhow::anyhow!("{e}"))?;

        if variants.is_empty() {
            return Ok(Vec::new());
        }

        let Some(canonical) = variants
            .iter()
            .find(|variant| variant.is_canonical())
            .cloned()
        else {
            return Ok(Vec::new());
        };
        let canonical_path = self.skills_root().join(&canonical.relative_path);
        if !canonical_path.exists() {
            return Ok(Vec::new());
        }

        let canonical_content = std::fs::read_to_string(&canonical_path).with_context(|| {
            format!(
                "failed to read canonical skill {}",
                canonical_path.display()
            )
        })?;
        let canonical_merge_body = extract_mergeable_variant_body(&canonical_content);
        let now = now_ts();
        let mut merged_ids = Vec::new();
        let mut merged_notes = Vec::new();
        let mut merged_sections = Vec::new();
        for variant in variants.iter().filter(|variant| {
            !variant.is_canonical()
                && variant.status != "archived"
                && variant.status != "merged"
                && variant.use_count >= SKILL_MERGE_MIN_USES
                && variant.success_rate() >= SKILL_MERGE_SUCCESS_RATE_THRESHOLD
                && variant.parent_variant_id.as_deref() == Some(canonical.variant_id.as_str())
        }) {
            let variant_path = self.skills_root().join(&variant.relative_path);
            if !variant_path.exists() {
                continue;
            }
            let variant_content = std::fs::read_to_string(&variant_path).with_context(|| {
                format!("failed to read skill variant {}", variant_path.display())
            })?;
            let variant_merge_body = extract_mergeable_variant_body(&variant_content);
            let similarity = skill_content_similarity(&canonical_merge_body, &variant_merge_body);
            if similarity < SKILL_MERGE_SIMILARITY_THRESHOLD {
                continue;
            }
            merged_ids.push(variant.variant_id.clone());
            merged_notes.push(skill_merge_note(variant, similarity));
            merged_sections.push(skill_merge_section(variant, &variant_content, similarity));
        }

        if merged_ids.is_empty() {
            return Ok(Vec::new());
        }

        let merged_content = append_skill_merge_sections(
            &append_skill_merge_notes(&canonical_content, &merged_notes),
            &merged_sections,
        );
        if merged_content != canonical_content {
            std::fs::write(&canonical_path, merged_content).with_context(|| {
                format!(
                    "failed to update canonical skill with merged contexts {}",
                    canonical_path.display()
                )
            })?;
            let _ = self.register_skill_document(&canonical_path).await?;
        }

        let merged_ids_clone = merged_ids.clone();
        self.conn.call(move |conn| {
            for variant_id in &merged_ids_clone {
                conn.execute(
                    "UPDATE skill_variants SET status = 'merged', updated_at = ?2 WHERE variant_id = ?1",
                    params![variant_id, now as i64],
                )?;
            }
            Ok(())
        }).await.map_err(|e| anyhow::anyhow!("{e}"))?;

        let merged_notes_len = merged_notes.len();
        self.append_telemetry(
            "cognitive",
            json!({
                "timestamp": now,
                "kind": "skill_variant_merged",
                "skill_name": skill_name,
                "variant_ids": merged_ids,
                "merged_count": merged_notes_len,
            }),
        )
        .await?;

        self.list_skill_variants(Some(&skill_name), 200).await
    }

    async fn create_branched_skill_variant(
        &self,
        skill_name: &str,
        candidate: &BranchCandidate,
    ) -> Result<Option<SkillVariantRecord>> {
        let source_path = self.skills_root().join(&candidate.source_relative_path);
        if !source_path.exists() {
            return Ok(None);
        }

        let source_content = std::fs::read_to_string(&source_path)
            .with_context(|| format!("failed to read source skill {}", source_path.display()))?;
        let title = extract_markdown_title(&source_content).unwrap_or_else(|| {
            skill_name
                .split('-')
                .map(|part| {
                    let mut chars = part.chars();
                    match chars.next() {
                        Some(first) => format!("{}{}", first.to_ascii_uppercase(), chars.as_str()),
                        None => String::new(),
                    }
                })
                .collect::<Vec<_>>()
                .join(" ")
        });
        let branch_slug = candidate.branch_tags.join("-");
        let branch_path = self
            .skills_root()
            .join("generated")
            .join(format!("{skill_name}--{branch_slug}.md"));
        if branch_path.exists() {
            return self.register_skill_document(&branch_path).await.map(Some);
        }

        let mut body = format!(
            "# {} ({})\n\n> Auto-branched from `{}` (variant `{}`) after {} successful consultations in contexts: {}.\n\n## When To Use\nUse this variant when the workspace context includes: {}.\n\n",
            title,
            candidate.branch_tags.join(", "),
            candidate.source_relative_path,
            candidate.source_variant_id,
            candidate.success_count,
            candidate.branch_tags.join(", "),
            candidate.branch_tags.join(", "),
        );
        body.push_str(&source_content);

        if let Some(parent) = branch_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&branch_path, body)
            .with_context(|| format!("failed to write branched skill {}", branch_path.display()))?;
        self.register_skill_document(&branch_path).await.map(Some)
    }
}
