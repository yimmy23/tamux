use super::*;

fn write_doc(root: &std::path::Path, relative: &str, content: &str) -> Result<std::path::PathBuf> {
    let path = root.join(relative);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&path, content)?;
    Ok(path)
}

#[tokio::test]
async fn syncing_installed_skill_documents_enqueues_missing_embeddings() -> Result<()> {
    let (store, root) = make_test_store().await?;
    let skills_root = root.join("skills");
    write_doc(
        &skills_root,
        "debug-rust/SKILL.md",
        r#"---
name: debug-rust
description: Debug Rust cargo failures.
---

# Debug Rust

Use cargo diagnostics and tests.
"#,
    )?;

    let summary = store
        .sync_semantic_documents_from_dir("skill", &skills_root, "text-embedding-3-small", 1536)
        .await?;

    assert_eq!(summary.discovered, 1);
    assert_eq!(summary.changed, 1);
    assert_eq!(summary.queued_embeddings, 1);

    let jobs = store
        .claim_embedding_jobs("text-embedding-3-small", 1536, 10)
        .await?;
    assert_eq!(jobs.len(), 1);
    assert_eq!(jobs[0].source_kind, "skill");
    assert_eq!(jobs[0].source_id, "debug-rust/SKILL.md");
    assert!(jobs[0].body.contains("Debug Rust"));
    Ok(())
}

#[tokio::test]
async fn syncing_unchanged_documents_skips_completed_embeddings() -> Result<()> {
    let (store, root) = make_test_store().await?;
    let guidelines_root = root.join("guidelines");
    write_doc(
        &guidelines_root,
        "coding-task.md",
        r#"---
name: coding-task
description: Implement code changes with tests.
---

# Coding Task
"#,
    )?;

    let first = store
        .sync_semantic_documents_from_dir(
            "guideline",
            &guidelines_root,
            "text-embedding-3-small",
            1536,
        )
        .await?;
    assert_eq!(first.queued_embeddings, 1);
    let jobs = store
        .claim_embedding_jobs("text-embedding-3-small", 1536, 10)
        .await?;
    assert_eq!(jobs.len(), 1);
    store
        .complete_embedding_job(&jobs[0], "text-embedding-3-small", 1536)
        .await?;

    let second = store
        .sync_semantic_documents_from_dir(
            "guideline",
            &guidelines_root,
            "text-embedding-3-small",
            1536,
        )
        .await?;
    assert_eq!(second.changed, 0);
    assert_eq!(second.queued_embeddings, 0);
    assert!(store
        .claim_embedding_jobs("text-embedding-3-small", 1536, 10)
        .await?
        .is_empty());
    Ok(())
}

#[tokio::test]
async fn syncing_removed_documents_queues_vector_deletions() -> Result<()> {
    let (store, root) = make_test_store().await?;
    let guidelines_root = root.join("guidelines");
    let path = write_doc(&guidelines_root, "old.md", "# Old guideline")?;
    let first = store
        .sync_semantic_documents_from_dir(
            "guideline",
            &guidelines_root,
            "text-embedding-3-small",
            1536,
        )
        .await?;
    assert_eq!(first.discovered, 1);
    let jobs = store
        .claim_embedding_jobs("text-embedding-3-small", 1536, 10)
        .await?;
    assert_eq!(jobs.len(), 1);
    store
        .complete_embedding_job(&jobs[0], "text-embedding-3-small", 1536)
        .await?;
    fs::remove_file(path)?;

    let summary = store
        .sync_semantic_documents_from_dir(
            "guideline",
            &guidelines_root,
            "text-embedding-3-small",
            1536,
        )
        .await?;

    assert_eq!(summary.removed, 1);
    let deletions = store.claim_embedding_deletions(10).await?;
    assert_eq!(deletions.len(), 1);
    assert_eq!(deletions[0].source_kind, "guideline");
    assert_eq!(deletions[0].source_id, "old.md");
    Ok(())
}
