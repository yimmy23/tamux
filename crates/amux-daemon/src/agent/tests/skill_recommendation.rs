use super::{
    discover_community_skills, discover_local_skills, extract_skill_metadata,
    SkillRecommendationAction, SkillRecommendationConfidence,
};
use crate::agent::types::SkillRecommendationConfig;
use crate::history::HistoryStore;
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
async fn confidence_tier_is_none_when_scores_do_not_clear_threshold() -> Result<()> {
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
async fn never_used_skill_does_not_look_recent_after_catalog_sync() -> Result<()> {
    let root = tempdir()?;
    let store = HistoryStore::new_test_store(root.path()).await?;
    let skills_root = root.path().join("skills");
    let builtin = skills_root.join("builtin");

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
    let builtin = skills_root.join("builtin");

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

    let indexed = store.list_skill_variants(None, 10).await?;

    assert_eq!(indexed.len(), 1);
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
    let builtin = skills_root.join("builtin");

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
