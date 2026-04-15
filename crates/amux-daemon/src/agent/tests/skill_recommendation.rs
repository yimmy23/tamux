use super::{
    discover_community_skills, discover_local_skills, extract_skill_metadata,
    SkillRecommendationAction, SkillRecommendationConfidence,
};
use crate::agent::types::SkillRecommendationConfig;
use crate::agent::{AgentConfig, AgentEngine};
use crate::history::HistoryStore;
use crate::session_manager::SessionManager;
use anyhow::Result;
use std::fs;
use tempfile::tempdir;

fn write_markdown(
    root: &std::path::Path,
    relative: &str,
    content: &str,
) -> Result<std::path::PathBuf> {
    let path = root.join(relative);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&path, content)?;
    Ok(path)
}

fn write_skill(
    root: &std::path::Path,
    skill_dir: &str,
    content: &str,
) -> Result<std::path::PathBuf> {
    write_markdown(root, &format!("{skill_dir}/SKILL.md"), content)
}

fn sample_task(id: &str, thread_id: &str) -> crate::agent::types::AgentTask {
    crate::agent::types::AgentTask {
        id: id.to_string(),
        title: id.to_string(),
        description: String::new(),
        status: crate::agent::types::TaskStatus::Queued,
        priority: crate::agent::types::TaskPriority::Normal,
        progress: 0,
        created_at: 0,
        started_at: None,
        completed_at: None,
        error: None,
        result: None,
        thread_id: Some(thread_id.to_string()),
        source: "user".to_string(),
        notify_on_complete: false,
        notify_channels: Vec::new(),
        dependencies: Vec::new(),
        command: None,
        session_id: None,
        goal_run_id: None,
        goal_run_title: None,
        goal_step_id: None,
        goal_step_title: None,
        parent_task_id: None,
        parent_thread_id: None,
        runtime: "daemon".to_string(),
        retry_count: 0,
        max_retries: 3,
        next_retry_at: None,
        scheduled_at: None,
        blocked_reason: None,
        awaiting_approval_id: None,
        policy_fingerprint: None,
        approval_expires_at: None,
        containment_scope: None,
        compensation_status: None,
        compensation_summary: None,
        lane_id: None,
        last_error: None,
        logs: Vec::new(),
        tool_whitelist: None,
        tool_blacklist: None,
        context_budget_tokens: None,
        context_overflow_action: None,
        termination_conditions: None,
        success_criteria: None,
        max_duration_secs: None,
        supervisor_config: None,
        override_provider: None,
        override_model: None,
        override_system_prompt: None,
        sub_agent_def_id: None,
    }
}

#[test]
fn extract_skill_metadata_reads_description_and_triggers() {
    let metadata = extract_skill_metadata(
        "builtin/brainstorming/SKILL.md",
        r#"---
name: brainstorming
description: Guide feature design before implementation.
keywords:
  - design
  - planning
triggers:
  - feature work
  - modifying behavior
---

# Brainstorming

Help turn ideas into plans.

## Triggers

- architecture change
- unclear requirements
"#,
    );

    assert_eq!(
        metadata.summary.as_deref(),
        Some("Guide feature design before implementation.")
    );
    assert!(metadata
        .triggers
        .iter()
        .any(|trigger| trigger == "feature work"));
    assert!(metadata
        .triggers
        .iter()
        .any(|trigger| trigger == "architecture change"));
    assert!(metadata.keywords.iter().any(|keyword| keyword == "design"));
    assert!(metadata.search_text.contains("Brainstorming"));
}

#[tokio::test]
async fn rank_skill_candidates_prefers_context_and_success() -> Result<()> {
    let root = tempdir()?;
    let store = HistoryStore::new_test_store(root.path()).await?;
    let skills_root = root.path().join("skills");
    let generated = skills_root.join("generated");

    let strong = write_skill(
        &generated,
        "debug-rust-build",
        r#"---
description: Debug Rust build and cargo test failures.
keywords: [rust, cargo, build]
triggers: [build failure, cargo test]
---

# Debug Rust Build

## Triggers
- cargo build fails
"#,
    )?;
    let weak_variant = write_skill(
        &generated,
        "debug-rust-build--legacy",
        r#"---
description: Older Rust build debugging flow.
keywords: [rust, build]
---

# Legacy Debug Rust Build
"#,
    )?;
    let other = write_skill(
        &generated,
        "debug-python-service",
        r#"---
description: Debug Python service startup issues.
keywords: [python, service]
triggers: [service crash]
---

# Debug Python Service
"#,
    )?;

    let strong_record = store.register_skill_document(&strong).await?;
    let weak_variant_record = store.register_skill_document(&weak_variant).await?;
    let other_record = store.register_skill_document(&other).await?;

    for _ in 0..4 {
        store
            .record_skill_variant_use(&strong_record.variant_id, Some(true))
            .await?;
    }
    store
        .record_skill_variant_use(&weak_variant_record.variant_id, Some(false))
        .await?;
    store
        .record_skill_variant_use(&other_record.variant_id, Some(true))
        .await?;

    let result = discover_local_skills(
        &store,
        &skills_root,
        "debug the rust cargo build failure in this backend workspace",
        &["rust".to_string(), "backend".to_string()],
        3,
        &SkillRecommendationConfig::default(),
    )
    .await?;

    assert_eq!(result.confidence, SkillRecommendationConfidence::Strong);
    assert_eq!(
        result.recommended_action,
        SkillRecommendationAction::ReadSkill
    );
    assert_eq!(
        result
            .recommendations
            .first()
            .map(|item| item.record.skill_name.as_str()),
        Some("debug-rust-build")
    );
    assert_eq!(
        result
            .recommendations
            .iter()
            .filter(|item| item.record.skill_name == "debug-rust-build")
            .count(),
        1
    );

    Ok(())
}

#[tokio::test]
async fn confidence_tier_is_none_and_action_is_none_when_scores_do_not_clear_threshold(
) -> Result<()> {
    let root = tempdir()?;
    let store = HistoryStore::new_test_store(root.path()).await?;
    let skills_root = root.path().join("skills");
    let generated = skills_root.join("generated");

    let skill = write_skill(
        &generated,
        "frontend-polish",
        r#"---
description: Polish a React UI flow.
keywords: [react, css]
---

# Frontend Polish
"#,
    )?;
    store.register_skill_document(&skill).await?;

    let result = discover_local_skills(
        &store,
        &skills_root,
        "debug a postgres replication timeout in production",
        &["database".to_string(), "infra".to_string()],
        3,
        &SkillRecommendationConfig::default(),
    )
    .await?;

    assert_eq!(result.confidence, SkillRecommendationConfidence::None);
    assert_eq!(result.recommended_action, SkillRecommendationAction::None);
    assert!(result.recommendations.is_empty());

    Ok(())
}

#[tokio::test]
async fn strong_match_without_hard_read_still_recommends_read_skill() -> Result<()> {
    let root = tempdir()?;
    let store = HistoryStore::new_test_store(root.path()).await?;
    let skills_root = root.path().join("skills");

    let skill_path = write_skill(
        &skills_root,
        "systematic-debugging",
        r#"---
description: Debug failing Rust code systematically.
keywords: [debug, rust, failure]
triggers: [fix bug, failing test, root cause]
---

# Systematic Debugging
"#,
    )?;
    let record = store.register_skill_document(&skill_path).await?;
    for _ in 0..4 {
        store
            .record_skill_variant_use(&record.variant_id, Some(true))
            .await?;
    }

    let result = discover_local_skills(
        &store,
        &skills_root,
        "debug this rust test failure and find the root cause",
        &["rust".to_string()],
        3,
        &SkillRecommendationConfig {
            require_read_on_strong_match: false,
            ..SkillRecommendationConfig::default()
        },
    )
    .await?;

    assert_eq!(result.confidence, SkillRecommendationConfidence::Strong);
    assert_eq!(
        result.recommended_action,
        SkillRecommendationAction::ReadSkill
    );

    Ok(())
}

#[tokio::test]
async fn nested_skill_catalogs_are_indexed_recursively() -> Result<()> {
    let root = tempdir()?;
    let store = HistoryStore::new_test_store(root.path()).await?;
    let skills_root = root.path().join("skills");

    write_markdown(
        &skills_root,
        "development/rust/debug/SKILL.md",
        r#"---
description: Debug Rust build and cargo test failures.
keywords: [rust, cargo, build]
---

# Debug Rust Build
"#,
    )?;

    super::sync_skill_catalog(&store, &skills_root).await?;

    let indexed = store.list_skill_variants(None, 10).await?;
    assert!(indexed
        .iter()
        .any(|variant| variant.relative_path == "development/rust/debug/SKILL.md"));

    Ok(())
}

#[tokio::test]
async fn matched_skill_discovery_paginates() -> Result<()> {
    let root = tempdir()?;
    let store = HistoryStore::new_test_store(root.path()).await?;
    let skills_root = root.path().join("skills");
    let generated = skills_root.join("generated");

    let rust_a = write_skill(
        &generated,
        "debug-rust-build",
        r#"---
description: Debug Rust build failures.
keywords: [rust, build, debug]
triggers: [build failure]
---

# Debug Rust Build
"#,
    )?;
    let rust_b = write_skill(
        &generated,
        "debug-rust-runtime",
        r#"---
description: Debug Rust runtime failures.
keywords: [rust, runtime, debug]
triggers: [runtime failure]
---

# Debug Rust Runtime
"#,
    )?;

    store.register_skill_document(&rust_a).await?;
    store.register_skill_document(&rust_b).await?;

    let result = discover_local_skills(
        &store,
        &skills_root,
        "debug rust failure",
        &["rust".to_string()],
        10,
        &SkillRecommendationConfig::default(),
    )
    .await?;

    let page_one = super::page_public_discovery_result(
        "debug rust failure",
        "debug rust failure",
        &["rust".to_string()],
        &result,
        &SkillRecommendationConfig::default(),
        None,
        1,
    )?;
    assert_eq!(page_one.candidates.len(), 1);
    assert!(page_one.next_cursor.is_some());

    let page_two = super::page_public_discovery_result(
        "debug rust failure",
        "debug rust failure",
        &["rust".to_string()],
        &result,
        &SkillRecommendationConfig::default(),
        page_one.next_cursor.as_deref(),
        1,
    )?;
    assert_eq!(page_two.candidates.len(), 1);
    assert_ne!(
        page_one.candidates[0].variant_id,
        page_two.candidates[0].variant_id
    );
    assert!(page_two.next_cursor.is_none());

    Ok(())
}

#[tokio::test]
async fn discover_local_skills_prefers_graph_linked_skill_when_heuristics_tie() -> Result<()> {
    let root = tempdir()?;
    let store = HistoryStore::new_test_store(root.path()).await?;
    let skills_root = root.path().join("skills");
    let generated = skills_root.join("generated");

    let alpha = write_skill(
        &generated,
        "alpha-debug-playbook",
        r#"---
description: Debug backend failures.
keywords: [debug, backend]
triggers: [backend failure]
---

# Alpha Debug Playbook
"#,
    )?;
    let zeta = write_skill(
        &generated,
        "zeta-debug-playbook",
        r#"---
description: Debug backend failures.
keywords: [debug, backend]
triggers: [backend failure]
---

# Zeta Debug Playbook
"#,
    )?;

    let alpha_record = store.register_skill_document(&alpha).await?;
    let zeta_record = store.register_skill_document(&zeta).await?;

    store
        .upsert_memory_node(
            "intent:debug backend failure",
            "debug backend failure",
            "intent",
            Some("normalized skill discovery intent"),
            1_717_181_701,
        )
        .await?;
    store
        .upsert_memory_node(
            &format!("skill:{}", alpha_record.variant_id),
            &alpha_record.skill_name,
            "skill_variant",
            Some("alpha skill graph node"),
            1_717_181_702,
        )
        .await?;
    store
        .upsert_memory_node(
            &format!("skill:{}", zeta_record.variant_id),
            &zeta_record.skill_name,
            "skill_variant",
            Some("zeta skill graph node"),
            1_717_181_703,
        )
        .await?;
    store
        .upsert_memory_edge(
            "intent:debug backend failure",
            &format!("skill:{}", zeta_record.variant_id),
            "intent_prefers_skill",
            5.0,
            1_717_181_704,
        )
        .await?;
    store
        .upsert_memory_edge(
            "intent:debug backend failure",
            &format!("skill:{}", alpha_record.variant_id),
            "intent_prefers_skill",
            1.0,
            1_717_181_705,
        )
        .await?;

    let result = discover_local_skills(
        &store,
        &skills_root,
        "debug backend failure",
        &[],
        5,
        &SkillRecommendationConfig {
            weak_match_threshold: 0.0,
            strong_match_threshold: 0.9,
            ..SkillRecommendationConfig::default()
        },
    )
    .await?;

    assert_eq!(
        result
            .recommendations
            .first()
            .map(|item| item.record.skill_name.as_str()),
        Some("zeta-debug-playbook"),
        "graph-linked skill with the stronger intent edge should outrank the weaker-linked skill when heuristics otherwise tie"
    );

    Ok(())
}

#[tokio::test]
async fn graph_distance_novelty_can_surface_less_explored_skill_path() -> Result<()> {
    let root = tempdir()?;
    let store = HistoryStore::new_test_store(root.path()).await?;
    let skills_root = root.path().join("skills");
    let generated = skills_root.join("generated");

    let direct = write_skill(
        &generated,
        "direct-debug-playbook",
        r#"---
description: Debug backend failures.
keywords: [debug, backend]
triggers: [backend failure]
---

# Direct Debug Playbook
"#,
    )?;
    let novel = write_skill(
        &generated,
        "novel-debug-playbook",
        r#"---
description: Debug backend failures.
keywords: [debug, backend]
triggers: [backend failure]
---

# Novel Debug Playbook
"#,
    )?;

    let direct_record = store.register_skill_document(&direct).await?;
    let novel_record = store.register_skill_document(&novel).await?;

    let direct_variant_id = direct_record.variant_id.clone();
    let novel_variant_id = novel_record.variant_id.clone();
    store
        .conn
        .call(move |conn| {
            conn.execute(
                "UPDATE skill_variants SET use_count = 10, success_count = 10, failure_count = 0, last_used_at = ?2, updated_at = ?2 WHERE variant_id = ?1",
                rusqlite::params![direct_variant_id, 1_717_181_706i64],
            )?;
            conn.execute(
                "UPDATE skill_variants SET use_count = 10, success_count = 10, failure_count = 0, last_used_at = ?2, updated_at = ?2 WHERE variant_id = ?1",
                rusqlite::params![novel_variant_id, 1_717_181_706i64],
            )?;
            Ok(())
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    store
        .upsert_memory_node(
            "intent:debug backend failure",
            "debug backend failure",
            "intent",
            Some("normalized skill discovery intent"),
            1_717_181_701,
        )
        .await?;
    store
        .upsert_memory_node(
            "intent:adjacent-debug-cluster",
            "adjacent-debug-cluster",
            "intent",
            Some("adjacent intent cluster"),
            1_717_181_702,
        )
        .await?;
    store
        .upsert_memory_node(
            &format!("skill:{}", direct_record.variant_id),
            &direct_record.skill_name,
            "skill_variant",
            Some("direct skill graph node"),
            1_717_181_703,
        )
        .await?;
    store
        .upsert_memory_node(
            &format!("skill:{}", novel_record.variant_id),
            &novel_record.skill_name,
            "skill_variant",
            Some("novel skill graph node"),
            1_717_181_704,
        )
        .await?;
    store
        .upsert_memory_edge(
            "intent:debug backend failure",
            &format!("skill:{}", direct_record.variant_id),
            "intent_prefers_skill",
            3.0,
            1_717_181_705,
        )
        .await?;
    store
        .upsert_memory_edge(
            "intent:debug backend failure",
            "intent:adjacent-debug-cluster",
            "intent_related_intent",
            3.0,
            1_717_181_706,
        )
        .await?;
    store
        .upsert_memory_edge(
            "intent:adjacent-debug-cluster",
            &format!("skill:{}", novel_record.variant_id),
            "intent_prefers_skill",
            3.0,
            1_717_181_707,
        )
        .await?;

    let result = discover_local_skills(
        &store,
        &skills_root,
        "debug backend failure",
        &[],
        5,
        &SkillRecommendationConfig {
            weak_match_threshold: 0.0,
            strong_match_threshold: 0.9,
            ..SkillRecommendationConfig::default()
        },
    )
    .await?;

    assert_eq!(
        result
            .recommendations
            .first()
            .map(|item| item.record.skill_name.as_str()),
        Some("novel-debug-playbook"),
        "graph-distance novelty should be able to surface the less-explored two-hop intent-skill path over the equally strong direct path"
    );

    Ok(())
}

#[tokio::test]
async fn novelty_preference_zero_keeps_direct_path_ahead_of_equally_strong_novel_path() -> Result<()>
{
    let root = tempdir()?;
    let store = HistoryStore::new_test_store(root.path()).await?;
    let skills_root = root.path().join("skills");
    let generated = skills_root.join("generated");

    let direct = write_skill(
        &generated,
        "direct-debug-playbook",
        r#"---
description: Debug backend failures.
keywords: [debug, backend]
triggers: [backend failure]
---

# Direct Debug Playbook
"#,
    )?;
    let novel = write_skill(
        &generated,
        "novel-debug-playbook",
        r#"---
description: Debug backend failures.
keywords: [debug, backend]
triggers: [backend failure]
---

# Novel Debug Playbook
"#,
    )?;

    let direct_record = store.register_skill_document(&direct).await?;
    let novel_record = store.register_skill_document(&novel).await?;

    let direct_variant_id = direct_record.variant_id.clone();
    let novel_variant_id = novel_record.variant_id.clone();
    store
        .conn
        .call(move |conn| {
            conn.execute(
                "UPDATE skill_variants SET use_count = 10, success_count = 10, failure_count = 0, last_used_at = ?2, updated_at = ?2 WHERE variant_id = ?1",
                rusqlite::params![direct_variant_id, 1_717_181_706i64],
            )?;
            conn.execute(
                "UPDATE skill_variants SET use_count = 10, success_count = 10, failure_count = 0, last_used_at = ?2, updated_at = ?2 WHERE variant_id = ?1",
                rusqlite::params![novel_variant_id, 1_717_181_706i64],
            )?;
            Ok(())
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    store
        .upsert_memory_node(
            "intent:debug backend failure",
            "debug backend failure",
            "intent",
            Some("normalized skill discovery intent"),
            1_717_181_701,
        )
        .await?;
    store
        .upsert_memory_node(
            "intent:adjacent-debug-cluster",
            "adjacent-debug-cluster",
            "intent",
            Some("adjacent intent cluster"),
            1_717_181_702,
        )
        .await?;
    store
        .upsert_memory_node(
            &format!("skill:{}", direct_record.variant_id),
            &direct_record.skill_name,
            "skill_variant",
            Some("direct skill graph node"),
            1_717_181_703,
        )
        .await?;
    store
        .upsert_memory_node(
            &format!("skill:{}", novel_record.variant_id),
            &novel_record.skill_name,
            "skill_variant",
            Some("novel skill graph node"),
            1_717_181_704,
        )
        .await?;
    store
        .upsert_memory_edge(
            "intent:debug backend failure",
            &format!("skill:{}", direct_record.variant_id),
            "intent_prefers_skill",
            3.0,
            1_717_181_705,
        )
        .await?;
    store
        .upsert_memory_edge(
            "intent:debug backend failure",
            "intent:adjacent-debug-cluster",
            "intent_related_intent",
            3.0,
            1_717_181_706,
        )
        .await?;
    store
        .upsert_memory_edge(
            "intent:adjacent-debug-cluster",
            &format!("skill:{}", novel_record.variant_id),
            "intent_prefers_skill",
            3.0,
            1_717_181_707,
        )
        .await?;

    let result = discover_local_skills(
        &store,
        &skills_root,
        "debug backend failure",
        &[],
        5,
        &SkillRecommendationConfig {
            weak_match_threshold: 0.0,
            strong_match_threshold: 0.9,
            novelty_distance_weight: 0.0,
            ..SkillRecommendationConfig::default()
        },
    )
    .await?;

    assert_eq!(
        result
            .recommendations
            .first()
            .map(|item| item.record.skill_name.as_str()),
        Some("direct-debug-playbook"),
        "when novelty preference is disabled, the equally strong direct path should remain ahead of the more novel two-hop path"
    );

    Ok(())
}

#[tokio::test]
async fn memory_node_retrieval_paths_can_seed_skill_recommendation_graph_signals() -> Result<()> {
    let root = tempdir()?;
    let store = HistoryStore::new_test_store(root.path()).await?;
    let skills_root = root.path().join("skills");
    let generated = skills_root.join("generated");

    let alpha = write_skill(
        &generated,
        "alpha-general-playbook",
        r#"---
description: General debugging guidance.
keywords: [debug]
---

# Alpha General Playbook
"#,
    )?;
    let zeta = write_skill(
        &generated,
        "zeta-general-playbook",
        r#"---
description: General debugging guidance.
keywords: [debug]
---

# Zeta General Playbook
"#,
    )?;

    let alpha_record = store.register_skill_document(&alpha).await?;
    let zeta_record = store.register_skill_document(&zeta).await?;

    let alpha_variant_id = alpha_record.variant_id.clone();
    let zeta_variant_id = zeta_record.variant_id.clone();
    store
        .conn
        .call(move |conn| {
            conn.execute(
                "UPDATE skill_variants SET use_count = 10, success_count = 10, failure_count = 0, last_used_at = ?2, updated_at = ?2 WHERE variant_id = ?1",
                rusqlite::params![alpha_variant_id, 1_717_181_706i64],
            )?;
            conn.execute(
                "UPDATE skill_variants SET use_count = 10, success_count = 10, failure_count = 0, last_used_at = ?2, updated_at = ?2 WHERE variant_id = ?1",
                rusqlite::params![zeta_variant_id, 1_717_181_706i64],
            )?;
            Ok(())
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    store
        .upsert_memory_node(
            "node:memory:incident-42",
            "incident bridge 42",
            "memory_fact",
            Some("operator mentioned incident bridge 42 while debugging backend failures"),
            1_717_181_701,
        )
        .await?;
    store
        .upsert_memory_node(
            "intent:backend-debugging",
            "backend debugging",
            "intent",
            Some("normalized backend debugging intent"),
            1_717_181_702,
        )
        .await?;
    store
        .upsert_memory_node(
            &format!("skill:{}", zeta_record.variant_id),
            &zeta_record.skill_name,
            "skill_variant",
            Some("zeta skill graph node"),
            1_717_181_703,
        )
        .await?;
    store
        .upsert_memory_edge(
            "node:memory:incident-42",
            "intent:backend-debugging",
            "memory_supports_intent",
            4.0,
            1_717_181_704,
        )
        .await?;
    store
        .upsert_memory_edge(
            "intent:backend-debugging",
            &format!("skill:{}", zeta_record.variant_id),
            "intent_prefers_skill",
            4.0,
            1_717_181_705,
        )
        .await?;

    let result = discover_local_skills(
        &store,
        &skills_root,
        "incident bridge 42",
        &[],
        5,
        &SkillRecommendationConfig {
            weak_match_threshold: 0.0,
            strong_match_threshold: 0.9,
            novelty_distance_weight: 0.0,
            ..SkillRecommendationConfig::default()
        },
    )
    .await?;

    assert_eq!(
        result
            .recommendations
            .first()
            .map(|item| item.record.skill_name.as_str()),
        Some("zeta-general-playbook"),
        "skill discovery should be able to traverse from a matching memory node into the shared graph and recover the linked skill path"
    );

    Ok(())
}

#[tokio::test]
async fn successful_skill_settlement_reinforces_memory_node_relevance_signal() -> Result<()> {
    let root = tempdir()?;
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let skill_path = write_skill(
        &root.path().join("skills").join("generated"),
        "zeta-general-playbook",
        r#"---
description: General debugging guidance.
keywords: [debug]
---

# Zeta General Playbook
"#,
    )?;

    let variant = engine.history.register_skill_document(&skill_path).await?;

    engine
        .history
        .upsert_memory_node(
            "node:memory:incident-42",
            "incident bridge 42",
            "memory_fact",
            Some("operator mentioned incident bridge 42 while debugging backend failures"),
            1_717_181_701,
        )
        .await?;
    engine
        .history
        .upsert_memory_node(
            "intent:incident bridge 42",
            "incident bridge 42",
            "intent",
            Some("query-shaped intent seed"),
            1_717_181_702,
        )
        .await?;
    engine
        .history
        .upsert_memory_node(
            "intent:backend-debugging",
            "backend debugging",
            "intent",
            Some("normalized backend debugging intent"),
            1_717_181_703,
        )
        .await?;
    engine
        .history
        .upsert_memory_edge(
            "node:memory:incident-42",
            "intent:incident bridge 42",
            "memory_supports_intent",
            1.0,
            1_717_181_704,
        )
        .await?;
    engine
        .history
        .upsert_memory_edge(
            "intent:incident bridge 42",
            "intent:backend-debugging",
            "intent_related_intent",
            1.0,
            1_717_181_705,
        )
        .await?;

    let edge_before = engine
        .history
        .list_memory_edges_for_node("node:memory:incident-42")
        .await?
        .into_iter()
        .find(|edge| {
            edge.relation_type == "memory_supports_intent"
                && (edge.target_node_id == "intent:incident bridge 42"
                    || edge.source_node_id == "intent:incident bridge 42")
        })
        .expect("memory-to-intent relevance edge should exist before settle");

    let thread_id = "thread-memory-feedback-success";
    let task_id = "task-memory-feedback-success";
    engine
        .record_skill_consultation(
            thread_id,
            Some(task_id),
            &variant,
            &["backend-debugging".to_string()],
        )
        .await;
    let task = sample_task(task_id, thread_id);
    assert_eq!(
        engine
            .settle_task_skill_consultations(&task, "success")
            .await,
        1
    );

    let edge_after = engine
        .history
        .list_memory_edges_for_node("node:memory:incident-42")
        .await?
        .into_iter()
        .find(|edge| {
            edge.relation_type == "memory_supports_intent"
                && (edge.target_node_id == "intent:incident bridge 42"
                    || edge.source_node_id == "intent:incident bridge 42")
        })
        .expect("memory-to-intent relevance edge should still exist after settle");

    assert!(
        edge_after.weight > edge_before.weight,
        "successful skill settlement should reinforce the memory-node relevance signal that seeded retrieval"
    );

    Ok(())
}

#[tokio::test]
async fn successful_skill_settlement_closes_the_memory_to_skill_discovery_loop() -> Result<()> {
    let root = tempdir()?;
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let skills_root = root.path().join("skills");
    let generated = skills_root.join("generated");

    let alpha = write_skill(
        &generated,
        "alpha-general-playbook",
        r#"---
description: General debugging guidance.
keywords: [debug]
---

# Alpha General Playbook
"#,
    )?;
    let zeta = write_skill(
        &generated,
        "zeta-general-playbook",
        r#"---
description: General debugging guidance.
keywords: [debug]
---

# Zeta General Playbook
"#,
    )?;

    let alpha_record = engine.history.register_skill_document(&alpha).await?;
    let zeta_record = engine.history.register_skill_document(&zeta).await?;

    for _ in 0..10 {
        engine
            .history
            .record_skill_variant_use(&alpha_record.variant_id, Some(true))
            .await?;
        engine
            .history
            .record_skill_variant_use(&zeta_record.variant_id, Some(true))
            .await?;
    }

    engine
        .history
        .upsert_memory_node(
            "node:memory:incident-42",
            "incident bridge 42",
            "memory_fact",
            Some("operator mentioned incident bridge 42 while debugging backend failures"),
            1_717_181_701,
        )
        .await?;
    engine
        .history
        .upsert_memory_node(
            "intent:incident-cluster",
            "incident cluster",
            "intent",
            Some("incident-context bridge intent"),
            1_717_181_702,
        )
        .await?;
    engine
        .history
        .upsert_memory_node(
            "intent:backend-debugging",
            "backend debugging",
            "intent",
            Some("backend debugging intent"),
            1_717_181_703,
        )
        .await?;
    engine
        .history
        .upsert_memory_node(
            &format!("skill:{}", alpha_record.variant_id),
            &alpha_record.skill_name,
            "skill_variant",
            Some("alpha skill graph node"),
            1_717_181_704,
        )
        .await?;
    engine
        .history
        .upsert_memory_node(
            &format!("skill:{}", zeta_record.variant_id),
            &zeta_record.skill_name,
            "skill_variant",
            Some("zeta skill graph node"),
            1_717_181_705,
        )
        .await?;
    engine
        .history
        .upsert_memory_edge(
            "node:memory:incident-42",
            "intent:incident-cluster",
            "memory_supports_intent",
            1.0,
            1_717_181_706,
        )
        .await?;
    engine
        .history
        .upsert_memory_edge(
            "intent:incident-cluster",
            &format!("skill:{}", alpha_record.variant_id),
            "intent_prefers_skill",
            2.0,
            1_717_181_707,
        )
        .await?;
    engine
        .history
        .upsert_memory_edge(
            "node:memory:incident-42",
            "intent:backend-debugging",
            "memory_supports_intent",
            0.5,
            1_717_181_708,
        )
        .await?;
    engine
        .history
        .upsert_memory_edge(
            "intent:backend-debugging",
            &format!("skill:{}", zeta_record.variant_id),
            "intent_prefers_skill",
            2.0,
            1_717_181_709,
        )
        .await?;

    let before = discover_local_skills(
        &engine.history,
        &skills_root,
        "incident bridge 42",
        &[],
        5,
        &SkillRecommendationConfig {
            weak_match_threshold: 0.0,
            strong_match_threshold: 0.9,
            novelty_distance_weight: 0.0,
            ..SkillRecommendationConfig::default()
        },
    )
    .await?;
    assert_eq!(
        before
            .recommendations
            .first()
            .map(|item| item.record.skill_name.as_str()),
        Some("alpha-general-playbook"),
        "before successful settlement, the stronger memory path should still favor alpha"
    );

    let edge_before = engine
        .history
        .list_memory_edges_for_node("node:memory:incident-42")
        .await?
        .into_iter()
        .find(|edge| {
            edge.relation_type == "memory_supports_intent"
                && (edge.target_node_id == "intent:backend-debugging"
                    || edge.source_node_id == "intent:backend-debugging")
        })
        .expect("memory-to-backend-debugging edge should exist before settle");

    let thread_id = "thread-memory-skill-loop-success";
    let task_id = "task-memory-skill-loop-success";
    engine
        .record_skill_consultation(
            thread_id,
            Some(task_id),
            &zeta_record,
            &["backend-debugging".to_string()],
        )
        .await;
    let task = sample_task(task_id, thread_id);
    assert_eq!(
        engine
            .settle_task_skill_consultations(&task, "success")
            .await,
        1
    );

    let edge_after = engine
        .history
        .list_memory_edges_for_node("node:memory:incident-42")
        .await?
        .into_iter()
        .find(|edge| {
            edge.relation_type == "memory_supports_intent"
                && (edge.target_node_id == "intent:backend-debugging"
                    || edge.source_node_id == "intent:backend-debugging")
        })
        .expect("memory-to-backend-debugging edge should exist after settle");
    assert!(
        edge_after.weight > edge_before.weight,
        "successful settlement should reinforce the memory->intent edge on the zeta path"
    );

    let after = discover_local_skills(
        &engine.history,
        &skills_root,
        "incident bridge 42",
        &[],
        5,
        &SkillRecommendationConfig {
            weak_match_threshold: 0.0,
            strong_match_threshold: 0.9,
            novelty_distance_weight: 0.0,
            ..SkillRecommendationConfig::default()
        },
    )
    .await?;
    assert_eq!(
        after
            .recommendations
            .first()
            .map(|item| item.record.skill_name.as_str()),
        Some("zeta-general-playbook"),
        "after successful settlement, the same memory-context query should flip toward zeta via the reinforced memory path"
    );

    Ok(())
}

#[tokio::test]
async fn successful_settled_consultation_biases_graph_backed_recommendation_ordering() -> Result<()>
{
    let root = tempdir()?;
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let skills_root = root.path().join("skills");
    let generated = skills_root.join("generated");

    let alpha = write_skill(
        &generated,
        "alpha-debug-playbook",
        r#"---
description: Debug backend failures.
keywords: [debug, backend]
triggers: [backend failure]
---

# Alpha Debug Playbook
"#,
    )?;
    let zeta = write_skill(
        &generated,
        "zeta-debug-playbook",
        r#"---
description: Debug backend failures.
keywords: [debug, backend]
triggers: [backend failure]
---

# Zeta Debug Playbook
"#,
    )?;

    let alpha_record = engine.history.register_skill_document(&alpha).await?;
    let zeta_record = engine.history.register_skill_document(&zeta).await?;

    engine
        .history
        .upsert_memory_node(
            "intent:debug backend failure",
            "debug backend failure",
            "intent",
            Some("normalized skill discovery intent"),
            1_717_181_701,
        )
        .await?;
    engine
        .history
        .upsert_memory_node(
            &format!("skill:{}", alpha_record.variant_id),
            &alpha_record.skill_name,
            "skill_variant",
            Some("alpha skill graph node"),
            1_717_181_702,
        )
        .await?;
    engine
        .history
        .upsert_memory_node(
            &format!("skill:{}", zeta_record.variant_id),
            &zeta_record.skill_name,
            "skill_variant",
            Some("zeta skill graph node"),
            1_717_181_703,
        )
        .await?;
    engine
        .history
        .upsert_memory_edge(
            "intent:debug backend failure",
            &format!("skill:{}", zeta_record.variant_id),
            "intent_prefers_skill",
            5.0,
            1_717_181_704,
        )
        .await?;
    engine
        .history
        .upsert_memory_edge(
            "intent:debug backend failure",
            &format!("skill:{}", alpha_record.variant_id),
            "intent_prefers_skill",
            1.0,
            1_717_181_705,
        )
        .await?;

    let alpha_variant_id = alpha_record.variant_id.clone();
    let zeta_variant_id = zeta_record.variant_id.clone();
    engine
        .history
        .conn
        .call(move |conn| {
            conn.execute(
                "UPDATE skill_variants SET use_count = 10, success_count = 10, failure_count = 0, last_used_at = ?2, updated_at = ?2 WHERE variant_id = ?1",
                rusqlite::params![alpha_variant_id, 1_717_181_706i64],
            )?;
            conn.execute(
                "UPDATE skill_variants SET use_count = 10, success_count = 10, failure_count = 0, last_used_at = ?2, updated_at = ?2 WHERE variant_id = ?1",
                rusqlite::params![zeta_variant_id, 1_717_181_706i64],
            )?;
            Ok(())
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    let thread_id = "thread-ranking-success";
    let task_id = "task-ranking-success";
    engine
        .record_skill_consultation(
            thread_id,
            Some(task_id),
            &alpha_record,
            &["debug backend failure".to_string()],
        )
        .await;
    let task = sample_task(task_id, thread_id);
    assert_eq!(
        engine
            .settle_task_skill_consultations(&task, "success")
            .await,
        1
    );

    let result = discover_local_skills(
        &engine.history,
        &skills_root,
        "debug backend failure",
        &[],
        5,
        &SkillRecommendationConfig {
            weak_match_threshold: 0.0,
            strong_match_threshold: 0.9,
            ..SkillRecommendationConfig::default()
        },
    )
    .await?;

    assert_eq!(
        result
            .recommendations
            .first()
            .map(|item| item.record.skill_name.as_str()),
        Some("alpha-debug-playbook"),
        "a successfully settled consultation should strengthen the consulted skill enough to outrank the previously stronger graph-linked peer for the same intent"
    );

    Ok(())
}

#[tokio::test]
async fn failed_settled_consultation_does_not_bias_graph_backed_recommendation_ordering(
) -> Result<()> {
    let root = tempdir()?;
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let skills_root = root.path().join("skills");
    let generated = skills_root.join("generated");

    let alpha = write_skill(
        &generated,
        "alpha-debug-playbook",
        r#"---
description: Debug backend failures.
keywords: [debug, backend]
triggers: [backend failure]
---

# Alpha Debug Playbook
"#,
    )?;
    let zeta = write_skill(
        &generated,
        "zeta-debug-playbook",
        r#"---
description: Debug backend failures.
keywords: [debug, backend]
triggers: [backend failure]
---

# Zeta Debug Playbook
"#,
    )?;

    let alpha_record = engine.history.register_skill_document(&alpha).await?;
    let zeta_record = engine.history.register_skill_document(&zeta).await?;

    engine
        .history
        .upsert_memory_node(
            "intent:debug backend failure",
            "debug backend failure",
            "intent",
            Some("normalized skill discovery intent"),
            1_717_181_701,
        )
        .await?;
    engine
        .history
        .upsert_memory_node(
            &format!("skill:{}", alpha_record.variant_id),
            &alpha_record.skill_name,
            "skill_variant",
            Some("alpha skill graph node"),
            1_717_181_702,
        )
        .await?;
    engine
        .history
        .upsert_memory_node(
            &format!("skill:{}", zeta_record.variant_id),
            &zeta_record.skill_name,
            "skill_variant",
            Some("zeta skill graph node"),
            1_717_181_703,
        )
        .await?;
    engine
        .history
        .upsert_memory_edge(
            "intent:debug backend failure",
            &format!("skill:{}", zeta_record.variant_id),
            "intent_prefers_skill",
            5.0,
            1_717_181_704,
        )
        .await?;
    engine
        .history
        .upsert_memory_edge(
            "intent:debug backend failure",
            &format!("skill:{}", alpha_record.variant_id),
            "intent_prefers_skill",
            1.0,
            1_717_181_705,
        )
        .await?;

    let alpha_variant_id = alpha_record.variant_id.clone();
    let zeta_variant_id = zeta_record.variant_id.clone();
    engine
        .history
        .conn
        .call(move |conn| {
            conn.execute(
                "UPDATE skill_variants SET use_count = 10, success_count = 10, failure_count = 0, last_used_at = ?2, updated_at = ?2 WHERE variant_id = ?1",
                rusqlite::params![alpha_variant_id, 1_717_181_706i64],
            )?;
            conn.execute(
                "UPDATE skill_variants SET use_count = 10, success_count = 10, failure_count = 0, last_used_at = ?2, updated_at = ?2 WHERE variant_id = ?1",
                rusqlite::params![zeta_variant_id, 1_717_181_706i64],
            )?;
            Ok(())
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    let thread_id = "thread-ranking-failure";
    let task_id = "task-ranking-failure";
    engine
        .record_skill_consultation(
            thread_id,
            Some(task_id),
            &alpha_record,
            &["debug backend failure".to_string()],
        )
        .await;
    let task = sample_task(task_id, thread_id);
    assert_eq!(
        engine
            .settle_task_skill_consultations(&task, "failure")
            .await,
        1
    );

    let result = discover_local_skills(
        &engine.history,
        &skills_root,
        "debug backend failure",
        &[],
        5,
        &SkillRecommendationConfig {
            weak_match_threshold: 0.0,
            strong_match_threshold: 0.9,
            ..SkillRecommendationConfig::default()
        },
    )
    .await?;

    assert_eq!(
        result
            .recommendations
            .first()
            .map(|item| item.record.skill_name.as_str()),
        Some("zeta-debug-playbook"),
        "a failed settled consultation should not dislodge the previously stronger graph-linked peer for the same intent: {:?}",
        result
            .recommendations
            .iter()
            .map(|item| (
                item.record.skill_name.clone(),
                item.score,
                item.reason.clone(),
                item.record.success_count,
                item.record.failure_count,
                item.record.use_count,
            ))
            .collect::<Vec<_>>()
    );

    Ok(())
}

#[tokio::test]
async fn cancelled_settled_consultation_does_not_bias_graph_backed_recommendation_ordering(
) -> Result<()> {
    let root = tempdir()?;
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let skills_root = root.path().join("skills");
    let generated = skills_root.join("generated");

    let alpha = write_skill(
        &generated,
        "alpha-debug-playbook",
        r#"---
description: Debug backend failures.
keywords: [debug, backend]
triggers: [backend failure]
---

# Alpha Debug Playbook
"#,
    )?;
    let zeta = write_skill(
        &generated,
        "zeta-debug-playbook",
        r#"---
description: Debug backend failures.
keywords: [debug, backend]
triggers: [backend failure]
---

# Zeta Debug Playbook
"#,
    )?;

    let alpha_record = engine.history.register_skill_document(&alpha).await?;
    let zeta_record = engine.history.register_skill_document(&zeta).await?;

    engine
        .history
        .upsert_memory_node(
            "intent:debug backend failure",
            "debug backend failure",
            "intent",
            Some("normalized skill discovery intent"),
            1_717_181_701,
        )
        .await?;
    engine
        .history
        .upsert_memory_node(
            &format!("skill:{}", alpha_record.variant_id),
            &alpha_record.skill_name,
            "skill_variant",
            Some("alpha skill graph node"),
            1_717_181_702,
        )
        .await?;
    engine
        .history
        .upsert_memory_node(
            &format!("skill:{}", zeta_record.variant_id),
            &zeta_record.skill_name,
            "skill_variant",
            Some("zeta skill graph node"),
            1_717_181_703,
        )
        .await?;
    engine
        .history
        .upsert_memory_edge(
            "intent:debug backend failure",
            &format!("skill:{}", zeta_record.variant_id),
            "intent_prefers_skill",
            5.0,
            1_717_181_704,
        )
        .await?;
    engine
        .history
        .upsert_memory_edge(
            "intent:debug backend failure",
            &format!("skill:{}", alpha_record.variant_id),
            "intent_prefers_skill",
            1.0,
            1_717_181_705,
        )
        .await?;

    let alpha_variant_id = alpha_record.variant_id.clone();
    let zeta_variant_id = zeta_record.variant_id.clone();
    engine
        .history
        .conn
        .call(move |conn| {
            conn.execute(
                "UPDATE skill_variants SET use_count = 10, success_count = 10, failure_count = 0, last_used_at = ?2, updated_at = ?2 WHERE variant_id = ?1",
                rusqlite::params![alpha_variant_id, 1_717_181_706i64],
            )?;
            conn.execute(
                "UPDATE skill_variants SET use_count = 10, success_count = 10, failure_count = 0, last_used_at = ?2, updated_at = ?2 WHERE variant_id = ?1",
                rusqlite::params![zeta_variant_id, 1_717_181_706i64],
            )?;
            Ok(())
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    let thread_id = "thread-ranking-cancelled";
    let task_id = "task-ranking-cancelled";
    engine
        .record_skill_consultation(
            thread_id,
            Some(task_id),
            &alpha_record,
            &["debug backend failure".to_string()],
        )
        .await;
    let task = sample_task(task_id, thread_id);
    assert_eq!(
        engine
            .settle_task_skill_consultations(&task, "cancelled")
            .await,
        1
    );

    let result = discover_local_skills(
        &engine.history,
        &skills_root,
        "debug backend failure",
        &[],
        5,
        &SkillRecommendationConfig {
            weak_match_threshold: 0.0,
            strong_match_threshold: 0.9,
            ..SkillRecommendationConfig::default()
        },
    )
    .await?;

    assert_eq!(
        result
            .recommendations
            .first()
            .map(|item| item.record.skill_name.as_str()),
        Some("zeta-debug-playbook"),
        "a cancelled settled consultation should not dislodge the previously stronger graph-linked peer for the same intent: {:?}",
        result
            .recommendations
            .iter()
            .map(|item| (
                item.record.skill_name.clone(),
                item.score,
                item.reason.clone(),
                item.record.success_count,
                item.record.failure_count,
                item.record.use_count,
            ))
            .collect::<Vec<_>>()
    );

    Ok(())
}

#[tokio::test]
async fn planning_skill_is_recommended_for_architecture_synthesis_requests() -> Result<()> {
    let root = tempdir()?;
    let store = HistoryStore::new_test_store(root.path()).await?;
    let skills_root = root.path().join("skills");
    let builtin = skills_root.clone();

    write_skill(
        &builtin,
        "brainstorming",
        r#"---
name: brainstorming
description: Guide feature design before implementation.
keywords:
  - design
  - planning
triggers:
  - feature work
  - modifying behavior
  - architecture change
---

# Brainstorming

Use this workflow for cross-document architecture synthesis and implementation planning.
"#,
    )?;

    let result = discover_local_skills(
        &store,
        &skills_root,
        "synthesize architecture across docs and plan implementation changes",
        &[],
        3,
        &SkillRecommendationConfig::default(),
    )
    .await?;

    assert_eq!(result.confidence, SkillRecommendationConfidence::Weak);
    assert_eq!(
        result.recommended_action,
        SkillRecommendationAction::ReadSkill
    );
    assert_eq!(
        result
            .recommendations
            .first()
            .map(|item| item.record.skill_name.as_str()),
        Some("brainstorming")
    );

    Ok(())
}

#[tokio::test]
async fn compact_rust_compile_patch_queries_still_match_build_debugging_skill() -> Result<()> {
    let root = tempdir()?;
    let store = HistoryStore::new_test_store(root.path()).await?;
    let skills_root = root.path().join("skills");

    write_skill(
        &skills_root,
        "debug-rust-build",
        r#"---
name: debug-rust-build
description: Debug Rust build and cargo test failures.
keywords: [rust, cargo, build]
triggers: [build failure, cargo test]
---

# Debug Rust Build

Use this workflow when Rust compilation or cargo builds fail and need investigation before patching.
"#,
    )?;

    for query in [
        "rust compile patch",
        "cargo compile fix",
        "compile error rust patch",
    ] {
        let result = discover_local_skills(
            &store,
            &skills_root,
            query,
            &["rust".to_string()],
            3,
            &SkillRecommendationConfig::default(),
        )
        .await?;

        assert_ne!(
            result.confidence,
            SkillRecommendationConfidence::None,
            "query `{query}` should find a local skill"
        );
        assert_eq!(
            result
                .recommendations
                .first()
                .map(|item| item.record.skill_name.as_str()),
            Some("debug-rust-build"),
            "query `{query}` should rank the rust build skill first"
        );
    }

    Ok(())
}

#[tokio::test]
async fn long_verbose_query_still_surfaces_relevant_audit_skill() -> Result<()> {
    let root = tempdir()?;
    let store = HistoryStore::new_test_store(root.path()).await?;
    let skills_root = root.path().join("skills");

    write_skill(
        &skills_root,
        "receiving-code-review",
        r#"---
name: receiving-code-review
description: Review git diffs and classify related versus unrelated changes before acting.
keywords: [audit, git, diff, review, rust, governance, safety]
triggers: [inspect worktree changes, classify related changes, review rust diffs]
---

# Receiving Code Review

Use this workflow to audit changed files and reason about diff scope safely.
"#,
    )?;

    let result = discover_local_skills(
        &store,
        &skills_root,
        "Audit modified git worktree files, inspect diffs, and map changed Rust files to orchestration safety governance RFC concepts to identify related vs unrelated changes",
        &["rust".to_string()],
        3,
        &SkillRecommendationConfig::default(),
    )
    .await?;

    assert_ne!(result.confidence, SkillRecommendationConfidence::None);
    assert_eq!(
        result
            .recommendations
            .first()
            .map(|item| item.record.skill_name.as_str()),
        Some("receiving-code-review")
    );

    Ok(())
}

#[tokio::test]
async fn never_used_skill_does_not_look_recent_after_catalog_sync() -> Result<()> {
    let root = tempdir()?;
    let store = HistoryStore::new_test_store(root.path()).await?;
    let skills_root = root.path().join("skills");
    let builtin = skills_root.clone();

    write_skill(
        &builtin,
        "debug-rust-build",
        r#"---
description: Debug Rust build and cargo test failures.
keywords: [rust, cargo, build]
---

# Debug Rust Build
"#,
    )?;

    let result = discover_local_skills(
        &store,
        &skills_root,
        "debug rust build timeout",
        &[],
        3,
        &SkillRecommendationConfig {
            weak_match_threshold: 0.60,
            ..SkillRecommendationConfig::default()
        },
    )
    .await?;

    assert_eq!(result.confidence, SkillRecommendationConfidence::None);
    assert!(result.recommendations.is_empty());

    Ok(())
}

#[tokio::test]
async fn catalog_sync_indexes_only_skill_entrypoints() -> Result<()> {
    let root = tempdir()?;
    let store = HistoryStore::new_test_store(root.path()).await?;
    let skills_root = root.path().join("skills");
    let builtin = skills_root.clone();

    write_skill(
        &builtin,
        "debug-rust-build",
        r#"---
description: Debug Rust build and cargo test failures.
keywords: [rust, cargo, build]
---

# Debug Rust Build
"#,
    )?;
    write_markdown(
        &builtin,
        "debug-rust-build/README.md",
        "# Notes\nThis is documentation, not a skill entrypoint.\n",
    )?;
    write_markdown(
        &builtin,
        "debug-rust-build/references/flow.md",
        "# Reference\nAdditional reference material.\n",
    )?;

    let result = discover_local_skills(
        &store,
        &skills_root,
        "debug rust build failure",
        &["rust".to_string()],
        3,
        &SkillRecommendationConfig::default(),
    )
    .await?;

    assert_eq!(result.recommendations.len(), 1);
    assert_eq!(
        result.recommendations[0].record.skill_name,
        "debug-rust-build"
    );

    Ok(())
}

#[tokio::test]
async fn discover_local_skills_errors_when_indexed_skill_file_is_missing() -> Result<()> {
    let root = tempdir()?;
    let store = HistoryStore::new_test_store(root.path()).await?;
    let skills_root = root.path().join("skills");
    let builtin = skills_root.clone();

    let skill_path = write_skill(
        &builtin,
        "debug-rust-build",
        r#"---
description: Debug Rust build and cargo test failures.
keywords: [rust, cargo, build]
---

# Debug Rust Build
"#,
    )?;
    store.register_skill_document(&skill_path).await?;
    fs::remove_file(&skill_path)?;

    let error = discover_local_skills(
        &store,
        &skills_root,
        "debug rust build failure",
        &["rust".to_string()],
        3,
        &SkillRecommendationConfig::default(),
    )
    .await
    .expect_err("missing skill file should be surfaced");

    assert!(error
        .to_string()
        .contains("failed to read skill recommendation file"));

    Ok(())
}

#[tokio::test]
async fn discover_local_skills_handles_legacy_builtin_paths_after_skill_move() -> Result<()> {
    let root = tempdir()?;
    let store = HistoryStore::new_test_store(root.path()).await?;
    let skills_root = root.path().join("skills");
    let legacy_skill_path = write_skill(
        &skills_root.join("builtin"),
        "cheatsheet",
        r#"---
description: Quick reference for available MCP tools.
keywords: [mcp, tools]
triggers: [tool reference]
---

# Cheatsheet
"#,
    )?;
    store.register_skill_document(&legacy_skill_path).await?;

    let migrated_skill_path = skills_root.join("cheatsheet").join("SKILL.md");
    fs::create_dir_all(
        migrated_skill_path
            .parent()
            .expect("migrated skill should have a parent"),
    )?;
    fs::rename(&legacy_skill_path, &migrated_skill_path)?;
    fs::remove_dir_all(skills_root.join("builtin"))?;

    let result = discover_local_skills(
        &store,
        &skills_root,
        "cheatsheet",
        &[],
        3,
        &SkillRecommendationConfig {
            weak_match_threshold: 0.0,
            strong_match_threshold: 0.9,
            ..SkillRecommendationConfig::default()
        },
    )
    .await?;

    assert_eq!(
        result
            .recommendations
            .first()
            .map(|item| item.record.skill_name.as_str()),
        Some("cheatsheet")
    );
    assert!(!result.recommendations.is_empty());
    assert_ne!(result.recommended_action, SkillRecommendationAction::None);

    Ok(())
}

#[tokio::test]
async fn discover_local_skills_handles_legacy_builtin_paths_after_taxonomy_move() -> Result<()> {
    let root = tempdir()?;
    let store = HistoryStore::new_test_store(root.path()).await?;
    let skills_root = root.path().join("skills");
    let legacy_skill_path = write_skill(
        &skills_root.join("builtin").join("superpowers"),
        "brainstorming",
        r#"---
name: brainstorming
description: Guide feature design before implementation.
keywords: [design, planning]
triggers: [feature work]
---

# Brainstorming
"#,
    )?;
    store.register_skill_document(&legacy_skill_path).await?;

    let migrated_skill_path = skills_root
        .join("development")
        .join("superpowers")
        .join("brainstorming")
        .join("SKILL.md");
    fs::create_dir_all(
        migrated_skill_path
            .parent()
            .expect("migrated skill should have a parent"),
    )?;
    fs::rename(&legacy_skill_path, &migrated_skill_path)?;
    fs::remove_dir_all(skills_root.join("builtin"))?;

    let result = discover_local_skills(
        &store,
        &skills_root,
        "brainstorming",
        &[],
        3,
        &SkillRecommendationConfig {
            weak_match_threshold: 0.0,
            strong_match_threshold: 0.9,
            ..SkillRecommendationConfig::default()
        },
    )
    .await?;

    assert_eq!(
        result
            .recommendations
            .first()
            .map(|item| item.record.skill_name.as_str()),
        Some("brainstorming")
    );
    assert!(!result.recommendations.is_empty());
    assert_ne!(result.recommended_action, SkillRecommendationAction::None);

    Ok(())
}

#[tokio::test]
async fn discover_local_skills_ignores_stale_reference_markdown_rows() -> Result<()> {
    let root = tempdir()?;
    let store = HistoryStore::new_test_store(root.path()).await?;
    let skills_root = root.path().join("skills");

    let stale_reference = write_markdown(
        &skills_root,
        "builtin/superpowers/brainstorming/visual-companion.md",
        "# Visual Companion\nReference helper, not a skill entrypoint.\n",
    )?;
    store.register_skill_document(&stale_reference).await?;

    write_skill(
        &skills_root.join("development").join("superpowers"),
        "brainstorming",
        r#"---
name: brainstorming
description: Guide feature design before implementation.
keywords: [design, planning]
triggers: [feature work]
---

# Brainstorming
"#,
    )?;
    fs::remove_dir_all(skills_root.join("builtin"))?;

    let result = discover_local_skills(
        &store,
        &skills_root,
        "brainstorming",
        &[],
        3,
        &SkillRecommendationConfig {
            weak_match_threshold: 0.0,
            strong_match_threshold: 0.9,
            ..SkillRecommendationConfig::default()
        },
    )
    .await?;

    assert_eq!(
        result
            .recommendations
            .first()
            .map(|item| item.record.skill_name.as_str()),
        Some("brainstorming")
    );

    Ok(())
}

#[tokio::test]
async fn discover_community_skills_matches_query_tokens_against_cached_registry() -> Result<()> {
    let root = tempdir()?;
    let registry_dir = root.path().join("registry");
    fs::create_dir_all(&registry_dir)?;
    fs::write(
        registry_dir.join("index.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "version": 1,
            "updated_at": 42,
            "skills": [{
                "name": "community-debugging-expert",
                "description": "Advanced panic debugging workflow from the registry.",
                "version": "1.0.0",
                "publisher_id": "publisher-1",
                "publisher_verified": true,
                "success_rate": 0.91,
                "use_count": 18,
                "content_hash": "abc123",
                "tamux_version": "0.3.1",
                "maturity_at_publish": "proven",
                "tags": ["debug", "rust", "panic"],
                "published_at": 42
            }]
        }))?,
    )?;

    let matches = discover_community_skills(
        root.path(),
        "http://127.0.0.1:9",
        "debug panic in rust service",
        5,
    )
    .await?;

    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0].name, "community-debugging-expert");

    Ok(())
}
