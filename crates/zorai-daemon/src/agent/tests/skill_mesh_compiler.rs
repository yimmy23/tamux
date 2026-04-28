use std::path::PathBuf;

use anyhow::Result as AnyResult;

use super::skill_mesh::compiler::{
    compile_skill_document, sample_compile_context_for_tests, SkillMeshCompileMode,
};
use super::skill_mesh::store::{
    apply_watch_event, sample_delete_event, sample_persistent_mesh_store_named,
    sample_rename_event,
};
use super::skill_mesh::watcher::SkillMeshWatchEvent;
use super::skill_mesh::watcher::debounce_skill_events;

fn sample_skill_markdown() -> &'static str {
    r#"---
name: systematic-debugging
description: Debug failures by tracing root cause before patching.
keywords: [debug, root cause, workflow]
triggers: [failing test, bug investigation]
---

# Systematic Debugging

## Triggers

- rust panic
- flaky build
"#
}

#[tokio::test]
async fn compiler_generates_synthetic_queries_and_capability_path() -> AnyResult<()> {
    let compiled = compile_skill_document(
        PathBuf::from("skills/development/systematic-debugging/SKILL.md"),
        sample_skill_markdown(),
        sample_compile_context_for_tests(SkillMeshCompileMode::Deterministic),
    )
    .await?;

    assert!(!compiled.synthetic_queries.is_empty());
    assert!(!compiled.capability_path.is_empty());
    assert_eq!(compiled.skill_name, "systematic-debugging");

    Ok(())
}

#[test]
fn debounced_watcher_emits_one_compile_job_for_burst_writes() {
    let events = vec![
        SkillMeshWatchEvent::write(PathBuf::from("skills/debug/SKILL.md")),
        SkillMeshWatchEvent::write(PathBuf::from("skills/debug/SKILL.md")),
        SkillMeshWatchEvent::write(PathBuf::from("skills/debug/SKILL.md")),
    ];

    let jobs = debounce_skill_events(events, std::time::Duration::from_millis(500));

    assert_eq!(jobs.len(), 1);
}

#[tokio::test]
async fn watcher_rename_and_delete_invalidate_compiled_documents() -> AnyResult<()> {
    let mesh = sample_persistent_mesh_store_named("watcher-rename-delete").await;
    mesh.upsert_document(sample_skill_mesh_document()).await?;

    apply_watch_event(&mesh, sample_rename_event()).await?;
    apply_watch_event(&mesh, sample_delete_event()).await?;

    assert!(mesh
        .pending_recompile_jobs()
        .await
        .iter()
        .any(|job| job.kind.is_invalidation()));

    Ok(())
}

#[tokio::test]
async fn compiler_falls_back_without_provider() -> AnyResult<()> {
    let compiled = compile_skill_document(
        PathBuf::from("skills/development/systematic-debugging/SKILL.md"),
        sample_skill_markdown(),
        sample_compile_context_for_tests(SkillMeshCompileMode::Deterministic),
    )
    .await?;

    assert!(compiled.summary.is_some());
    assert!(compiled.synthetic_queries.iter().any(|query| query.contains("debug")));

    Ok(())
}

#[tokio::test]
async fn compiled_mesh_state_survives_restart() -> AnyResult<()> {
    let store = sample_persistent_mesh_store_named("restart-persistence").await;
    store.upsert_document(sample_skill_mesh_document()).await?;
    let key = sample_skill_mesh_document().document_key();

    drop(store);

    let reopened = sample_persistent_mesh_store_named("restart-persistence").await;
    assert!(reopened.get_document(&key).await?.is_some());

    Ok(())
}

#[tokio::test]
async fn compile_version_or_trust_changes_trigger_recompile() -> AnyResult<()> {
    let mesh = sample_persistent_mesh_store_named("compile-version-trust").await;
    mesh.upsert_document(sample_skill_mesh_document()).await?;

    mesh.bump_compile_version_for_tests().await;
    mesh.update_trust_inputs_for_tests().await;

    assert!(!mesh.pending_recompile_jobs().await.is_empty());

    Ok(())
}
