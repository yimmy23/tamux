use super::*;

// ── Consolidation state tests (Phase 5) ──────────────────────────────

async fn wait_for_search_hits(
    store: &HistoryStore,
    query: &str,
    limit: usize,
) -> Result<(String, Vec<HistorySearchHit>)> {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    loop {
        let result = store.search(query, limit).await?;
        if !result.1.is_empty() || std::time::Instant::now() >= deadline {
            return Ok(result);
        }
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
}

async fn wait_for_search_hits_matching<F>(
    store: &HistoryStore,
    query: &str,
    limit: usize,
    mut matches: F,
) -> Result<(String, Vec<HistorySearchHit>)>
where
    F: FnMut(&[HistorySearchHit]) -> bool,
{
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    loop {
        let result = store.search(query, limit).await?;
        if matches(&result.1) || std::time::Instant::now() >= deadline {
            return Ok(result);
        }
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
}

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

    store.delete_consolidation_state("last_watermark").await?;
    let val = store.get_consolidation_state("last_watermark").await?;
    assert!(val.is_none());

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
        .await
        .map_err(|e| anyhow::anyhow!("add agent message: {e}"))?;

    let (summary, hits) = store.search("build", 8).await?;

    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].title, "cargo build --workspace");
    assert!(summary.contains("Found 1 historical matches"));

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn search_uses_only_sqlite_fts_projection() -> Result<()> {
    let (store, root) = make_test_store().await?;

    store
        .record_managed_finish(&ManagedHistoryRecord {
            execution_id: "exec-sqlite-fts".to_string(),
            session_id: "session-1".to_string(),
            workspace_id: Some("workspace-1".to_string()),
            command: "cargo test sqlite fts".to_string(),
            rationale: "Verify sqlite fts history search projection".to_string(),
            source: "test".to_string(),
            exit_code: Some(0),
            duration_ms: Some(250),
            snapshot_path: None,
        })
        .await
        .map_err(|e| anyhow::anyhow!("upsert agent event: {e}"))?;

    store
        .conn
        .call(|conn| {
            conn.execute("DELETE FROM history_fts WHERE id = 'exec-sqlite-fts'", [])?;
            Ok(())
        })
        .await
        .map_err(|e| anyhow::anyhow!("add agent message: {e}"))?;

    let (summary, hits) = store.search("sqlite fts projection", 8).await?;

    assert!(hits.is_empty());
    assert!(summary.contains("No prior runs matched"));

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn search_does_not_index_capability_documents_without_vector_embeddings() -> Result<()> {
    let (store, root) = make_test_store().await?;

    store
        .create_thread(&AgentDbThread {
            id: "thread-search".to_string(),
            workspace_id: None,
            surface_id: None,
            pane_id: None,
            agent_name: Some("agent-search".to_string()),
            title: "Search support thread".to_string(),
            created_at: 1_717_199_990,
            updated_at: 1_717_199_990,
            message_count: 0,
            total_tokens: 0,
            last_preview: String::new(),
            metadata_json: None,
        })
        .await
        .map_err(|e| anyhow::anyhow!("create thread: {e}"))?;
    store
        .add_message(&AgentDbMessage {
            id: "msg-tool-failure".to_string(),
            thread_id: "thread-search".to_string(),
            created_at: 1_717_200_000,
            role: "assistant".to_string(),
            content: "Tool call failed while running migration cleanup".to_string(),
            provider: Some("openai".to_string()),
            model: Some("gpt-test".to_string()),
            input_tokens: None,
            output_tokens: None,
            total_tokens: None,
            cost_usd: None,
            reasoning: Some("Need inspect failed tool calls before retrying".to_string()),
            tool_calls_json: Some(r#"[{"name":"bash","status":"failed"}]"#.to_string()),
            metadata_json: None,
        })
        .await
        .map_err(|e| anyhow::anyhow!("add agent message: {e}"))?;
    store
        .upsert_agent_event(&AgentEventRow {
            id: "event-meta".to_string(),
            category: "behavioral".to_string(),
            kind: "metacognition".to_string(),
            pane_id: None,
            workspace_id: None,
            surface_id: None,
            session_id: None,
            payload_json: r#"{"note":"operator asked for counterfactual search"}"#.to_string(),
            timestamp: 1_717_200_010,
        })
        .await
        .map_err(|e| anyhow::anyhow!("upsert agent event: {e}"))?;
    store
        .insert_causal_trace(
            "trace-failure",
            Some("thread-search"),
            None,
            Some("task-1"),
            "tool_call",
            "failed_tool_calls",
            r#"{"tool":"bash","command":"cargo test"}"#,
            r#"[]"#,
            "context",
            r#"[{"description":"failed migration cleanup"}]"#,
            r#"{"Failure":{"reason":"test failure"}} "#,
            Some("gpt-test"),
            1_717_200_020,
        )
        .await
        .map_err(|e| anyhow::anyhow!("insert causal trace: {e}"))?;
    store
        .insert_action_audit(&AuditEntryRow {
            id: "audit-counterfactual".to_string(),
            timestamp: 1_717_200_030,
            action_type: "counterfactual".to_string(),
            summary: "Counterfactual suggested safer migration cleanup".to_string(),
            explanation: Some("Use targeted SQL inspection before broad edits".to_string()),
            confidence: Some(0.8),
            confidence_band: Some("high".to_string()),
            causal_trace_id: Some("trace-failure".to_string()),
            thread_id: Some("thread-search".to_string()),
            goal_run_id: None,
            task_id: Some("task-1".to_string()),
            raw_data_json: None,
        })
        .await
        .map_err(|e| anyhow::anyhow!("insert action audit: {e}"))?;
    let dream_cycle_id = store
        .insert_dream_cycle(&DreamCycleRow {
            id: None,
            started_at_ms: 1_717_200_035,
            completed_at_ms: None,
            idle_duration_ms: 1000,
            tasks_analyzed: 1,
            counterfactuals_generated: 1,
            counterfactuals_successful: 1,
            status: "running".to_string(),
        })
        .await
        .map_err(|e| anyhow::anyhow!("insert dream cycle: {e}"))?;
    store
        .insert_counterfactual_evaluation(&CounterfactualEvaluationRow {
            id: None,
            dream_cycle_id,
            source_task_id: "task-1".to_string(),
            variation_type: "tool_sequence".to_string(),
            counterfactual_description: "Use read_file before shelling out to inspect migrations"
                .to_string(),
            estimated_token_saving: Some(32.0),
            estimated_time_saving_ms: Some(2000),
            estimated_revision_reduction: Some(1),
            score: 0.91,
            threshold_met: true,
            created_at_ms: 1_717_200_040,
        })
        .await
        .map_err(|e| anyhow::anyhow!("insert counterfactual evaluation: {e}"))?;
    store
        .upsert_meta_cognition_model("agent-search", 0.0, 1_717_200_050)
        .await
        .map_err(|e| anyhow::anyhow!("upsert metacognition model: {e}"))?;
    store
        .replace_cognitive_biases(
            1,
            &[CognitiveBiasRow {
                id: 0,
                model_id: 1,
                name: "anchoring".to_string(),
                trigger_pattern_json: r#"["first failing tool result"]"#.to_string(),
                mitigation_prompt: "Search counterfactual and causal trace history first"
                    .to_string(),
                severity: 0.7,
                occurrence_count: 2,
            }],
        )
        .await
        .map_err(|e| anyhow::anyhow!("replace cognitive biases: {e}"))?;

    let (summary, hits) = store
        .search("counterfactual failed tool migration", 10)
        .await?;

    assert!(hits.is_empty(), "{hits:?}");
    assert!(summary.contains("No prior runs matched"), "{summary}");

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
async fn tool_output_preview_path_uses_thread_artifact_preview_layout() -> Result<()> {
    let (store, root) = make_test_store().await?;

    let path = store.tool_output_preview_path("thread-123", None, "bash_command", 1_713_000_000);

    assert_eq!(
        path,
        root.join("threads")
            .join("thread-123")
            .join("artifacts")
            .join("previews")
            .join("bash_command-1713000000.txt")
    );

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn tool_output_preview_path_uses_goal_named_thread_preview_layout() -> Result<()> {
    let (store, root) = make_test_store().await?;

    let path =
        store.tool_output_preview_path("thread-123", Some("goal-456"), "web_search", 1_713_000_001);

    assert_eq!(
        path,
        root.join("threads")
            .join("thread-123")
            .join("artifacts")
            .join("previews")
            .join("web_search-goal-456-1713000001.txt")
    );

    fs::remove_dir_all(root)?;
    Ok(())
}
