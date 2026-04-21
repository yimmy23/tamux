use super::*;

// ── Consolidation state tests (Phase 5) ──────────────────────────────

#[tokio::test]
async fn consolidation_state_set_get_round_trips() -> Result<()> {
    let (store, root) = make_test_store().await?;

    // Initially empty
    let val = store.get_consolidation_state("last_watermark").await?;
    assert!(val.is_none());

    // Set and get
    store
        .set_consolidation_state("last_watermark", "12345", 1000)
        .await?;
    let val = store.get_consolidation_state("last_watermark").await?;
    assert_eq!(val.as_deref(), Some("12345"));

    // Overwrite
    store
        .set_consolidation_state("last_watermark", "99999", 2000)
        .await?;
    let val = store.get_consolidation_state("last_watermark").await?;
    assert_eq!(val.as_deref(), Some("99999"));

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn search_returns_history_hits_from_fts_join() -> Result<()> {
    let (store, root) = make_test_store().await?;

    store
        .record_managed_finish(&ManagedHistoryRecord {
            execution_id: "exec-1".to_string(),
            session_id: "session-1".to_string(),
            workspace_id: Some("workspace-1".to_string()),
            command: "cargo build --workspace".to_string(),
            rationale: "Verify the daemon build stays green".to_string(),
            source: "test".to_string(),
            exit_code: Some(0),
            duration_ms: Some(250),
            snapshot_path: None,
        })
        .await?;

    let (summary, hits) = store.search("build", 8).await?;

    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].title, "cargo build --workspace");
    assert!(summary.contains("Found 1 historical matches"));

    fs::remove_dir_all(root)?;
    Ok(())
}

// ── Successful trace query test (Phase 5) ────────────────────────────

#[tokio::test]
async fn list_recent_successful_traces_with_watermark() -> Result<()> {
    let (store, root) = make_test_store().await?;

    // Insert traces with different outcomes
    store
        .insert_execution_trace(
            "tr-1",
            None,
            None,
            Some("task-1"),
            "research",
            "success",
            Some(0.9),
            "[]",
            "{}",
            100,
            50,
            "svarog",
            1000,
            1000,
            1000,
        )
        .await?;
    store
        .insert_execution_trace(
            "tr-2",
            None,
            None,
            Some("task-2"),
            "coding",
            "failure",
            Some(0.3),
            "[]",
            "{}",
            200,
            80,
            "svarog",
            2000,
            2000,
            2000,
        )
        .await?;
    store
        .insert_execution_trace(
            "tr-3",
            None,
            None,
            Some("task-3"),
            "research",
            "success",
            Some(0.8),
            "[]",
            "{}",
            150,
            60,
            "svarog",
            3000,
            3000,
            3000,
        )
        .await?;

    // Query after watermark=1500 should only return tr-3 (success after watermark)
    let traces = store.list_recent_successful_traces(1500, 100).await?;
    assert_eq!(traces.len(), 1);
    assert_eq!(traces[0].id, "tr-3");
    assert_eq!(traces[0].outcome.as_deref(), Some("success"));

    // Query after watermark=0 should return both successful traces
    let traces = store.list_recent_successful_traces(0, 100).await?;
    assert_eq!(traces.len(), 2);
    // ASC order
    assert_eq!(traces[0].id, "tr-1");
    assert_eq!(traces[1].id, "tr-3");

    fs::remove_dir_all(root)?;
    Ok(())
}

// -----------------------------------------------------------------------
// Skill variant status and retrieval tests (SKIL-01, SKIL-02)
// -----------------------------------------------------------------------

#[tokio::test]
async fn update_skill_variant_status_changes_status_and_updated_at() -> Result<()> {
    let (store, root) = make_test_store().await?;
    store.init_schema().await?;

    // Register a skill to create a variant
    let skill_dir = root.join("skills").join("generated").join("test-skill");
    fs::create_dir_all(&skill_dir)?;
    let skill_path = skill_dir.join("canonical.md");
    fs::write(&skill_path, "# Test Skill\nA test skill document.")?;
    let record = store.register_skill_document(&skill_path).await?;
    assert_eq!(record.status, "active");

    // Update status to "testing"
    store
        .update_skill_variant_status(&record.variant_id, "testing")
        .await?;

    // Verify the status changed
    let updated = store.get_skill_variant(&record.variant_id).await?;
    assert!(updated.is_some());
    let updated = updated.unwrap();
    assert_eq!(updated.status, "testing");
    assert!(updated.updated_at >= record.updated_at);

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn get_skill_variant_returns_none_for_missing() -> Result<()> {
    let (store, root) = make_test_store().await?;
    store.init_schema().await?;

    let result = store.get_skill_variant("nonexistent-variant-id").await?;
    assert!(result.is_none());

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn get_skill_variant_returns_some_for_existing() -> Result<()> {
    let (store, root) = make_test_store().await?;
    store.init_schema().await?;

    let skill_dir = root
        .join("skills")
        .join("generated")
        .join("retrieval-skill");
    fs::create_dir_all(&skill_dir)?;
    let skill_path = skill_dir.join("canonical.md");
    fs::write(
        &skill_path,
        "# Retrieval Skill\nA skill for retrieval test.",
    )?;
    let record = store.register_skill_document(&skill_path).await?;

    let fetched = store.get_skill_variant(&record.variant_id).await?;
    assert!(fetched.is_some());
    let fetched = fetched.unwrap();
    assert_eq!(fetched.variant_id, record.variant_id);
    assert_eq!(fetched.skill_name, record.skill_name);

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn tool_output_preview_path_uses_thread_cache_layout() -> Result<()> {
    let (store, root) = make_test_store().await?;

    let path = store.tool_output_preview_path("thread-123", None, "bash_command", 1_713_000_000);

    assert_eq!(
        path,
        root.join(".cache")
            .join("tools")
            .join("thread-thread-123")
            .join("bash_command-1713000000.txt")
    );

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn tool_output_preview_path_uses_goal_cache_layout() -> Result<()> {
    let (store, root) = make_test_store().await?;

    let path = store.tool_output_preview_path(
        "thread-123",
        Some("goal-456"),
        "web_search",
        1_713_000_001,
    );

    assert_eq!(
        path,
        root.join(".cache")
            .join("tools")
            .join("goal-goal-456")
            .join("web_search-thread-123-1713000001.txt")
    );

    fs::remove_dir_all(root)?;
    Ok(())
}
