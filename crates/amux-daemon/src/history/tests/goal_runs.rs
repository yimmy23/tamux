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
        let index_name: Option<String> = conn
            .query_row(
                "SELECT name FROM sqlite_master WHERE type = 'index' AND name = 'idx_agent_tasks_goal_run'",
                [],
                |row| row.get(0),
            )
            .optional()?;
        Ok((has_session, has_scheduled, has_goal_run, index_name))
    }).await.map_err(|e| anyhow::anyhow!("{e}"))?;

    assert!(has_cols.0);
    assert!(has_cols.1);
    assert!(has_cols.2);
    assert_eq!(has_cols.3.as_deref(), Some("idx_agent_tasks_goal_run"));

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

    let report = store.provenance_report(10)?;
    assert_eq!(report.total_entries, 2);
    assert_eq!(report.valid_hash_entries, 2);
    assert_eq!(report.valid_chain_entries, 2);
    assert_eq!(report.valid_signature_entries, 2);

    fs::remove_dir_all(root)?;
    Ok(())
}
