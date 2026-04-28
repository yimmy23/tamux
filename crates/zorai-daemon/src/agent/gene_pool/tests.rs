#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use crate::agent::engine::AgentEngine;
    use crate::agent::gene_pool::runtime::build_gene_pool_runtime_snapshot;
    use crate::agent::types::AgentConfig;
    use crate::history::{ExecutionTraceRow, SkillVariantRecord};
    use crate::session_manager::SessionManager;

    fn variant(
        variant_id: &str,
        skill_name: &str,
        variant_name: &str,
        status: &str,
        context_tags: &[&str],
        success_count: u32,
        failure_count: u32,
        fitness_score: f64,
    ) -> SkillVariantRecord {
        SkillVariantRecord {
            variant_id: variant_id.to_string(),
            skill_name: skill_name.to_string(),
            variant_name: variant_name.to_string(),
            relative_path: format!("generated/{skill_name}--{variant_name}.md"),
            parent_variant_id: None,
            version: "v1".to_string(),
            context_tags: context_tags.iter().map(|tag| tag.to_string()).collect(),
            use_count: success_count + failure_count,
            success_count,
            failure_count,
            fitness_score,
            status: status.to_string(),
            last_used_at: Some(2_000),
            created_at: 1_000,
            updated_at: 2_000,
        }
    }

    fn trace(
        id: &str,
        task_type: &str,
        tool_sequence: &[&str],
        quality_score: f64,
    ) -> ExecutionTraceRow {
        ExecutionTraceRow {
            id: id.to_string(),
            goal_run_id: None,
            task_id: Some(format!("task-{id}")),
            task_type: Some(task_type.to_string()),
            outcome: Some("success".to_string()),
            quality_score: Some(quality_score),
            tool_sequence_json: Some(
                serde_json::to_string(
                    &tool_sequence
                        .iter()
                        .map(|tool| tool.to_string())
                        .collect::<Vec<_>>(),
                )
                .expect("tool sequence json"),
            ),
            metrics_json: Some("{}".to_string()),
            duration_ms: Some(800),
            tokens_used: Some(200),
            created_at: 1_500,
        }
    }

    #[test]
    fn learning_worker_builds_gene_pool_snapshot_with_candidate_and_lifecycle_actions() {
        let traces = vec![trace(
            "trace-1",
            "build-pipeline",
            &["read_file", "apply_patch", "cargo_test"],
            0.92,
        )];
        let variants = vec![
            variant(
                "variant-draft",
                "build-pipeline",
                "frontend",
                "draft",
                &["frontend"],
                4,
                0,
                5.0,
            ),
            variant(
                "variant-active-weak",
                "build-pipeline",
                "legacy",
                "active",
                &["legacy"],
                1,
                5,
                -4.0,
            ),
        ];

        let snapshot = build_gene_pool_runtime_snapshot(&traces, &variants, 2_000);

        assert!(snapshot
            .candidates
            .iter()
            .any(|candidate| candidate.proposed_skill_name.contains("build-pipeline")));
        assert!(snapshot
            .lifecycle_actions
            .iter()
            .any(|action| action.action == "promote"
                && action.variant_id.as_deref() == Some("variant-draft")));
        assert!(snapshot
            .lifecycle_actions
            .iter()
            .any(|action| action.action == "retire"
                && action.variant_id.as_deref() == Some("variant-active-weak")));
        assert!(!snapshot.fitness_history.is_empty());
    }

    #[test]
    fn learning_worker_proposes_cross_breed_for_strong_skill_variants() {
        let traces = vec![trace(
            "trace-2",
            "build-pipeline",
            &["read_file", "apply_patch", "cargo_test"],
            0.96,
        )];
        let variants = vec![
            variant(
                "variant-a",
                "build-pipeline",
                "frontend",
                "active",
                &["frontend", "react"],
                6,
                0,
                6.0,
            ),
            variant(
                "variant-b",
                "build-pipeline",
                "backend",
                "active",
                &["backend", "rust"],
                5,
                0,
                5.5,
            ),
        ];

        let snapshot = build_gene_pool_runtime_snapshot(&traces, &variants, 3_000);

        assert!(snapshot.cross_breed_proposals.iter().any(|proposal| {
            proposal.left_parent_variant_id == "variant-a"
                || proposal.right_parent_variant_id == "variant-a"
        }));
        assert!(snapshot
            .lifecycle_actions
            .iter()
            .any(|action| action.action == "cross_breed"));
    }

    #[tokio::test]
    async fn refresh_gene_pool_runtime_persists_snapshot_and_applies_actions() {
        let root = tempdir().expect("tempdir");
        let manager = SessionManager::new_test(root.path()).await;
        let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

        let canonical = root.path().join("skills/generated/build-pipeline.md");
        let candidate = root
            .path()
            .join("skills/generated/build-pipeline--frontend.md");
        std::fs::create_dir_all(canonical.parent().expect("skills dir")).expect("skills dir");
        std::fs::write(&canonical, "# Build Pipeline\nRun cargo build.\n").expect("canonical");
        std::fs::write(&candidate, "# Build Pipeline Frontend\nRun cargo test.\n")
            .expect("candidate");

        let active = engine
            .history
            .register_skill_document(&canonical)
            .await
            .expect("active variant");
        let promoted_candidate = engine
            .history
            .register_skill_document(&candidate)
            .await
            .expect("candidate variant");
        engine
            .history
            .update_skill_variant_status(&promoted_candidate.variant_id, "draft")
            .await
            .expect("set draft");

        let promoted_candidate_id = promoted_candidate.variant_id.clone();
        let active_id = active.variant_id.clone();
        engine
            .history
            .conn
            .call(move |conn| {
                conn.execute(
                    "UPDATE skill_variants SET use_count = 4, success_count = 4, failure_count = 0, fitness_score = 6.0 WHERE variant_id = ?1",
                    rusqlite::params![promoted_candidate_id],
                )?;
                conn.execute(
                    "UPDATE skill_variants SET use_count = 6, success_count = 1, failure_count = 5, fitness_score = -5.0 WHERE variant_id = ?1",
                    rusqlite::params![active_id],
                )?;
                Ok(())
            })
            .await
            .expect("seed variant scores");

        engine
            .history
            .insert_execution_trace(
                "trace-gene-pool",
                Some("thread-1"),
                None,
                Some("task-1"),
                "build-pipeline",
                "success",
                Some(0.95),
                "[\"read_file\",\"apply_patch\",\"cargo_test\"]",
                "{}",
                900,
                320,
                "weles",
                1_000,
                1_500,
                1_500,
            )
            .await
            .expect("execution trace");

        let snapshot = engine
            .refresh_gene_pool_runtime()
            .await
            .expect("gene pool runtime refresh");

        assert!(snapshot
            .lifecycle_actions
            .iter()
            .any(|action| action.action == "promote"));
        assert!(snapshot
            .lifecycle_actions
            .iter()
            .any(|action| action.action == "retire"));
        assert!(!snapshot.fitness_history.is_empty());

        let persisted = engine
            .history
            .get_consolidation_state("gene_pool_runtime_snapshot")
            .await
            .expect("load snapshot state")
            .expect("snapshot should persist");
        assert!(persisted.contains("build-pipeline"));

        let refreshed_candidate = engine
            .history
            .get_skill_variant(&promoted_candidate.variant_id)
            .await
            .expect("load candidate")
            .expect("candidate should exist");
        assert_eq!(refreshed_candidate.status, "active");

        let refreshed_active = engine
            .history
            .get_skill_variant(&active.variant_id)
            .await
            .expect("load active")
            .expect("active variant should exist");
        assert_eq!(refreshed_active.status, "archived");

        let fitness_rows = engine
            .history
            .read_conn
            .call(move |conn| {
                Ok(
                    conn.query_row("SELECT COUNT(*) FROM gene_fitness_history", [], |row| {
                        row.get::<_, i64>(0)
                    })?,
                )
            })
            .await
            .expect("fitness row count");
        assert!(fitness_rows >= 2);
    }
}
