use super::*;

#[tokio::test]
async fn init_schema_migrates_legacy_agent_tasks_before_goal_run_index() -> Result<()> {
    let (store, root) = make_test_store().await?;
    // Drop existing tables and recreate with legacy schema (missing columns)
    store.conn.call(|conn| {
        conn.execute_batch("DROP TABLE IF EXISTS agent_tasks")?;
        conn.execute_batch(
        "
        CREATE TABLE agent_tasks (
            id                   TEXT PRIMARY KEY,
            title                TEXT NOT NULL,
            description          TEXT NOT NULL,
            status               TEXT NOT NULL,
            priority             TEXT NOT NULL,
            progress             INTEGER NOT NULL DEFAULT 0,
            created_at           INTEGER NOT NULL,
            started_at           INTEGER,
            completed_at         INTEGER,
            error                TEXT,
            result               TEXT,
            thread_id            TEXT,
            source               TEXT NOT NULL DEFAULT 'user',
            notify_on_complete   INTEGER NOT NULL DEFAULT 0,
            notify_channels_json TEXT NOT NULL DEFAULT '[]',
            command              TEXT,
            retry_count          INTEGER NOT NULL DEFAULT 0,
            max_retries          INTEGER NOT NULL DEFAULT 3,
            next_retry_at        INTEGER,
            blocked_reason       TEXT,
            awaiting_approval_id TEXT,
            lane_id              TEXT,
            last_error           TEXT
        );
        CREATE INDEX IF NOT EXISTS idx_agent_tasks_status ON agent_tasks(status, priority, created_at DESC);
        ",
    )?;
        Ok(())
    }).await.map_err(|e| anyhow::anyhow!("{e}"))?;

    store.init_schema().await?;

    let has_cols = store.conn.call(|conn| {
        let has_session = table_has_column(conn, "agent_tasks", "session_id")?;
        let has_scheduled = table_has_column(conn, "agent_tasks", "scheduled_at")?;
        let has_goal_run = table_has_column(conn, "agent_tasks", "goal_run_id")?;
        let has_override_provider = table_has_column(conn, "agent_tasks", "override_provider")?;
        let has_override_prompt = table_has_column(conn, "agent_tasks", "override_system_prompt")?;
        let has_sub_agent_def = table_has_column(conn, "agent_tasks", "sub_agent_def_id")?;
        let index_name: Option<String> = conn
            .query_row(
                "SELECT name FROM sqlite_master WHERE type = 'index' AND name = 'idx_agent_tasks_goal_run'",
                [],
                |row| row.get(0),
            )
            .optional()?;
        Ok((
            has_session,
            has_scheduled,
            has_goal_run,
            has_override_provider,
            has_override_prompt,
            has_sub_agent_def,
            index_name,
        ))
    }).await.map_err(|e| anyhow::anyhow!("{e}"))?;

    assert!(has_cols.0);
    assert!(has_cols.1);
    assert!(has_cols.2);
    assert!(has_cols.3);
    assert!(has_cols.4);
    assert!(has_cols.5);
    assert_eq!(has_cols.6.as_deref(), Some("idx_agent_tasks_goal_run"));

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn goal_run_event_todo_snapshot_round_trips() -> Result<()> {
    let (store, root) = make_test_store().await?;

    let goal_run = GoalRun {
        id: "goal-1".to_string(),
        title: "Goal".to_string(),
        goal: "Do the thing".to_string(),
        client_request_id: None,
        status: GoalRunStatus::Running,
        priority: TaskPriority::Normal,
        created_at: 1,
        updated_at: 2,
        started_at: Some(1),
        completed_at: None,
        thread_id: Some("thread-1".to_string()),
        session_id: None,
        current_step_index: 0,
        current_step_title: Some("Inspect".to_string()),
        current_step_kind: Some(GoalRunStepKind::Research),
        replan_count: 0,
        max_replans: 2,
        plan_summary: Some("Plan".to_string()),
        reflection_summary: None,
        memory_updates: Vec::new(),
        generated_skill_path: None,
        last_error: None,
        failure_cause: None,
        child_task_ids: Vec::new(),
        child_task_count: 0,
        approval_count: 0,
        awaiting_approval_id: None,
        active_task_id: None,
        duration_ms: None,
        steps: vec![GoalRunStep {
            id: "step-1".to_string(),
            position: 0,
            title: "Inspect".to_string(),
            instructions: "Inspect state".to_string(),
            kind: GoalRunStepKind::Research,
            success_criteria: "Know state".to_string(),
            session_id: None,
            status: GoalRunStepStatus::InProgress,
            task_id: None,
            summary: None,
            error: None,
            started_at: Some(1),
            completed_at: None,
        }],
        events: vec![GoalRunEvent {
            id: "event-1".to_string(),
            timestamp: 3,
            phase: "todo".to_string(),
            message: "goal todo updated".to_string(),
            details: None,
            step_index: Some(0),
            todo_snapshot: vec![crate::agent::types::TodoItem {
                id: "todo-1".to_string(),
                content: "Inspect state".to_string(),
                status: crate::agent::types::TodoStatus::InProgress,
                position: 0,
                step_index: Some(0),
                created_at: 3,
                updated_at: 3,
            }],
        }],
        total_prompt_tokens: 0,
        total_completion_tokens: 0,
        estimated_cost_usd: None,
        autonomy_level: Default::default(),
        authorship_tag: None,
    };

    store.upsert_goal_run(&goal_run).await?;
    let loaded = store
        .get_goal_run("goal-1")
        .await?
        .expect("goal run should exist after upsert");

    assert_eq!(loaded.events.len(), 1);
    assert_eq!(loaded.events[0].step_index, Some(0));
    assert_eq!(loaded.events[0].todo_snapshot.len(), 1);
    assert_eq!(loaded.events[0].todo_snapshot[0].content, "Inspect state");

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn memory_provenance_write_round_trips() -> Result<()> {
    let (store, root) = make_test_store().await?;
    store.init_schema().await?;

    let fact_keys = vec!["shell".to_string(), "editor".to_string()];
    store
        .record_memory_provenance(&MemoryProvenanceRecord {
            id: "mem-1",
            target: "USER.md",
            mode: "append",
            source_kind: "tool",
            content: "- shell: bash",
            fact_keys: &fact_keys,
            thread_id: Some("thread-1"),
            task_id: Some("task-1"),
            goal_run_id: None,
            created_at: 42,
        })
        .await?;

    let row = store.conn.call(|conn| {
        conn.query_row(
            "SELECT target, mode, source_kind, content, fact_keys_json FROM memory_provenance WHERE id = 'mem-1'",
            [],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?, row.get::<_, String>(3)?, row.get::<_, String>(4)?)),
        ).map_err(Into::into)
    }).await.map_err(|e| anyhow::anyhow!("{e}"))?;
    assert_eq!(row.0, "USER.md");
    assert_eq!(row.1, "append");
    assert_eq!(row.2, "tool");
    assert_eq!(row.3, "- shell: bash");
    assert_eq!(
        serde_json::from_str::<Vec<String>>(&row.4)?,
        vec!["shell".to_string(), "editor".to_string()]
    );

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn memory_provenance_report_marks_old_entries_uncertain() -> Result<()> {
    let (store, root) = make_test_store().await?;
    store.init_schema().await?;
    let recent_keys = vec!["shell".to_string()];
    let old_keys = vec!["editor".to_string()];
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;

    store
        .record_memory_provenance(&MemoryProvenanceRecord {
            id: "recent",
            target: "USER.md",
            mode: "append",
            source_kind: "tool",
            content: "- shell: bash",
            fact_keys: &recent_keys,
            thread_id: None,
            task_id: None,
            goal_run_id: None,
            created_at: now_ms,
        })
        .await?;
    store
        .record_memory_provenance(&MemoryProvenanceRecord {
            id: "old",
            target: "MEMORY.md",
            mode: "append",
            source_kind: "goal_reflection",
            content: "- editor: helix",
            fact_keys: &old_keys,
            thread_id: None,
            task_id: None,
            goal_run_id: None,
            created_at: now_ms.saturating_sub(40 * 86_400_000),
        })
        .await?;

    let report = store.memory_provenance_report(None, 10).await?;
    assert_eq!(report.total_entries, 2);
    assert_eq!(report.summary_by_status.get("active").copied(), Some(1));
    assert_eq!(report.summary_by_status.get("uncertain").copied(), Some(1));

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn confirm_uncertain_memory_provenance_updates_status() -> Result<()> {
    let (store, root) = make_test_store().await?;
    store.init_schema().await?;
    let old_keys = vec!["editor".to_string()];
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;

    store
        .record_memory_provenance(&MemoryProvenanceRecord {
            id: "old-confirmable",
            target: "MEMORY.md",
            mode: "append",
            source_kind: "goal_reflection",
            content: "- editor: helix",
            fact_keys: &old_keys,
            thread_id: None,
            task_id: None,
            goal_run_id: None,
            created_at: now_ms.saturating_sub(40 * 86_400_000),
        })
        .await?;

    let before = store.memory_provenance_report(None, 10).await?;
    assert_eq!(before.entries[0].status, "uncertain");
    assert!(before.entries[0].confidence < 0.55);

    let updated = store
        .confirm_memory_provenance_entry("old-confirmable", now_ms)
        .await?;
    assert!(updated, "expected confirmation update to touch one row");

    let after = store.memory_provenance_report(None, 10).await?;
    assert_eq!(after.entries[0].status, "confirmed");
    assert!(after.entries[0].confidence >= 0.95);
    assert_eq!(after.summary_by_status.get("confirmed").copied(), Some(1));

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn retract_memory_provenance_entry_updates_status() -> Result<()> {
    let (store, root) = make_test_store().await?;
    store.init_schema().await?;
    let fact_keys = vec!["editor".to_string()];
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;

    store
        .record_memory_provenance(&MemoryProvenanceRecord {
            id: "retractable-memory-entry",
            target: "MEMORY.md",
            mode: "append",
            source_kind: "goal_reflection",
            content: "- editor: helix",
            fact_keys: &fact_keys,
            thread_id: None,
            task_id: None,
            goal_run_id: None,
            created_at: now_ms,
        })
        .await?;

    let before = store.memory_provenance_report(None, 10).await?;
    assert_eq!(before.entries[0].status, "active");

    let updated = store
        .retract_memory_provenance_entry("retractable-memory-entry", now_ms)
        .await?;
    assert!(updated, "expected retract update to touch one row");

    let after = store.memory_provenance_report(None, 10).await?;
    assert_eq!(after.entries[0].status, "retracted");
    assert_eq!(after.summary_by_status.get("retracted").copied(), Some(1));

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn remove_memory_provenance_records_retract_relationship() -> Result<()> {
    let (store, root) = make_test_store().await?;
    store.init_schema().await?;
    let fact_keys = vec!["editor".to_string()];
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;

    store
        .record_memory_provenance(&MemoryProvenanceRecord {
            id: "memory-editor-active",
            target: "MEMORY.md",
            mode: "append",
            source_kind: "goal_reflection",
            content: "- editor: helix",
            fact_keys: &fact_keys,
            thread_id: None,
            task_id: None,
            goal_run_id: None,
            created_at: now_ms.saturating_sub(1000),
        })
        .await?;

    store
        .record_memory_provenance(&MemoryProvenanceRecord {
            id: "memory-editor-remove",
            target: "MEMORY.md",
            mode: "remove",
            source_kind: "operator_correction",
            content: "- editor: helix",
            fact_keys: &fact_keys,
            thread_id: None,
            task_id: None,
            goal_run_id: None,
            created_at: now_ms,
        })
        .await?;

    let report = store.memory_provenance_report(Some("MEMORY.md"), 10).await?;
    let remove_entry = report
        .entries
        .iter()
        .find(|entry| entry.id == "memory-editor-remove")
        .expect("remove provenance entry should exist");

    assert_eq!(remove_entry.relationships.len(), 1);
    assert_eq!(remove_entry.relationships[0].relation_type, "retracts");
    assert_eq!(remove_entry.relationships[0].related_entry_id, "memory-editor-active");
    assert_eq!(remove_entry.relationships[0].fact_key.as_deref(), Some("editor"));

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn collaboration_session_round_trips() -> Result<()> {
    let (store, root) = make_test_store().await?;
    store.init_schema().await?;
    store
        .upsert_collaboration_session(
            "task-parent",
            r#"{"id":"c1","parent_task_id":"task-parent"}"#,
            42,
        )
        .await?;

    let rows = store.list_collaboration_sessions().await?;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].parent_task_id, "task-parent");
    assert_eq!(rows[0].updated_at, 42);
    assert!(rows[0].session_json.contains("\"id\":\"c1\""));

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn provenance_report_validates_hash_and_signature() -> Result<()> {
    let (store, root) = make_test_store().await?;
    store.init_schema().await?;
    let first = serde_json::json!({"step": 1});
    let second = serde_json::json!({"step": 2});
    store
        .record_provenance_event(&ProvenanceEventRecord {
            event_type: "goal_created",
            summary: "goal created",
            details: &first,
            agent_id: "test-agent",
            goal_run_id: Some("goal-1"),
            task_id: None,
            thread_id: Some("thread-1"),
            approval_id: None,
            causal_trace_id: None,
            compliance_mode: "soc2",
            sign: true,
            created_at: 1_000,
        })
        .await?;
    store
        .record_provenance_event(&ProvenanceEventRecord {
            event_type: "step_completed",
            summary: "step completed",
            details: &second,
            agent_id: "test-agent",
            goal_run_id: Some("goal-1"),
            task_id: Some("task-1"),
            thread_id: Some("thread-1"),
            approval_id: None,
            causal_trace_id: None,
            compliance_mode: "soc2",
            sign: true,
            created_at: 2_000,
        })
        .await?;

    let entries = read_provenance_entries(&root.join("semantic-logs").join("provenance.jsonl"))?;
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].signature_scheme.as_deref(), Some("ed25519"));
    assert_eq!(entries[1].signature_scheme.as_deref(), Some("ed25519"));

    let report = store.provenance_report(10)?;
    assert_eq!(report.total_entries, 2);
    assert_eq!(report.valid_hash_entries, 2);
    assert_eq!(report.valid_chain_entries, 2);
    assert_eq!(report.valid_signature_entries, 2);

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn provenance_report_keeps_legacy_signature_validation() -> Result<()> {
    let (store, root) = make_test_store().await?;
    store.init_schema().await?;
    let legacy_key_path = root.join("provenance-signing.key");
    let legacy_key = "legacy-signing-key";
    fs::write(&legacy_key_path, legacy_key)?;

    let details = serde_json::json!({"legacy": true});
    let entry_hash = compute_provenance_hash(
        0,
        1_000,
        "legacy_event",
        "legacy provenance event",
        &details,
        "genesis",
        "legacy-agent",
        None,
        None,
        Some("thread-legacy"),
        None,
        None,
        "soc2",
    );
    let entry = ProvenanceLogEntry {
        sequence: 0,
        timestamp: 1_000,
        event_type: "legacy_event".to_string(),
        summary: "legacy provenance event".to_string(),
        details,
        prev_hash: "genesis".to_string(),
        entry_hash: entry_hash.clone(),
        signature: Some(sign_provenance_hash(legacy_key, &entry_hash)),
        signature_scheme: None,
        agent_id: "legacy-agent".to_string(),
        goal_run_id: None,
        task_id: None,
        thread_id: Some("thread-legacy".to_string()),
        approval_id: None,
        causal_trace_id: None,
        compliance_mode: "soc2".to_string(),
    };
    fs::write(
        root.join("semantic-logs").join("provenance.jsonl"),
        format!("{}\n", serde_json::to_string(&entry)?),
    )?;

    let report = store.provenance_report(10)?;
    assert_eq!(report.total_entries, 1);
    assert_eq!(report.signed_entries, 1);
    assert_eq!(report.valid_signature_entries, 1);

    fs::remove_dir_all(root)?;
    Ok(())
}
