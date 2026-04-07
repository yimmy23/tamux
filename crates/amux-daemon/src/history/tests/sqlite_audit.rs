use super::*;

/// FOUN-01: WAL journal mode is active after HistoryStore construction.
#[tokio::test]
async fn wal_mode_enabled() -> Result<()> {
    let (store, root) = make_test_store().await?;
    let mode: String = store
        .conn
        .call(|conn| {
            conn.query_row("PRAGMA journal_mode", [], |row| row.get(0))
                .map_err(Into::into)
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    assert_eq!(mode.to_lowercase(), "wal");
    fs::remove_dir_all(root)?;
    Ok(())
}

/// FOUN-06: All 5 WAL pragmas are applied on connection open.
#[tokio::test]
async fn wal_pragmas_applied() -> Result<()> {
    let (store, root) = make_test_store().await?;
    let pragmas = store
        .conn
        .call(|conn| {
            let journal_mode: String =
                conn.query_row("PRAGMA journal_mode", [], |row| row.get(0))?;
            let synchronous: i64 = conn.query_row("PRAGMA synchronous", [], |row| row.get(0))?;
            let foreign_keys: i64 = conn.query_row("PRAGMA foreign_keys", [], |row| row.get(0))?;
            let wal_autocheckpoint: i64 =
                conn.query_row("PRAGMA wal_autocheckpoint", [], |row| row.get(0))?;
            let busy_timeout: i64 = conn.query_row("PRAGMA busy_timeout", [], |row| row.get(0))?;
            Ok((
                journal_mode,
                synchronous,
                foreign_keys,
                wal_autocheckpoint,
                busy_timeout,
            ))
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    assert_eq!(pragmas.0.to_lowercase(), "wal");
    assert_eq!(pragmas.1, 1); // NORMAL
    assert_eq!(pragmas.2, 1); // ON
    assert_eq!(pragmas.3, 1000);
    assert_eq!(pragmas.4, 5000);
    fs::remove_dir_all(root)?;
    Ok(())
}

/// FOUN-02: HistoryStore can perform a basic async roundtrip through .call().
#[tokio::test]
async fn async_connection_roundtrip() -> Result<()> {
    let (store, root) = make_test_store().await?;
    let thread = AgentDbThread {
        id: "test-thread-1".to_string(),
        workspace_id: None,
        surface_id: None,
        pane_id: None,
        agent_name: Some("test-agent".to_string()),
        title: "Test Thread".to_string(),
        created_at: 1000,
        updated_at: 1000,
        message_count: 0,
        total_tokens: 0,
        last_preview: String::new(),
        metadata_json: None,
    };
    store.create_thread(&thread).await?;
    let loaded = store.get_thread("test-thread-1").await?;
    assert!(loaded.is_some());
    let loaded = loaded.unwrap();
    assert_eq!(loaded.title, "Test Thread");
    assert_eq!(loaded.agent_name, Some("test-agent".to_string()));
    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn replace_thread_snapshot_replaces_messages_without_losing_thread_row() -> Result<()> {
    let (store, root) = make_test_store().await?;
    let thread_id = "snapshot-thread";
    let thread = AgentDbThread {
        id: thread_id.to_string(),
        workspace_id: None,
        surface_id: None,
        pane_id: None,
        agent_name: Some("test-agent".to_string()),
        title: "Original Thread".to_string(),
        created_at: 1000,
        updated_at: 1000,
        message_count: 1,
        total_tokens: 1,
        last_preview: "old body".to_string(),
        metadata_json: None,
    };
    let old_message = AgentDbMessage {
        id: "old-message".to_string(),
        thread_id: thread_id.to_string(),
        created_at: 1001,
        role: "assistant".to_string(),
        content: "old body".to_string(),
        provider: None,
        model: None,
        input_tokens: Some(0),
        output_tokens: Some(1),
        total_tokens: Some(1),
        reasoning: None,
        tool_calls_json: None,
        metadata_json: None,
    };
    store.create_thread(&thread).await?;
    store.add_message(&old_message).await?;

    let refreshed_thread = AgentDbThread {
        title: "Updated Thread".to_string(),
        updated_at: 2000,
        message_count: 1,
        total_tokens: 2,
        last_preview: "new body".to_string(),
        ..thread.clone()
    };
    let new_message = AgentDbMessage {
        id: "new-message".to_string(),
        thread_id: thread_id.to_string(),
        created_at: 2001,
        role: "assistant".to_string(),
        content: "new body".to_string(),
        provider: None,
        model: None,
        input_tokens: Some(0),
        output_tokens: Some(2),
        total_tokens: Some(2),
        reasoning: None,
        tool_calls_json: None,
        metadata_json: None,
    };

    store
        .replace_thread_snapshot(&refreshed_thread, &[new_message.clone()])
        .await?;

    let loaded_thread = store
        .get_thread(thread_id)
        .await?
        .expect("thread should still exist after snapshot replacement");
    assert_eq!(loaded_thread.title, "Updated Thread");

    let loaded_messages = store.list_messages(thread_id, Some(10)).await?;
    assert_eq!(loaded_messages.len(), 1);
    assert_eq!(loaded_messages[0].id, new_message.id);
    assert_eq!(loaded_messages[0].content, new_message.content);

    fs::remove_dir_all(root)?;
    Ok(())
}

/// FOUN-01 + FOUN-02: Concurrent reads and writes do not produce "database is locked" errors.
#[tokio::test]
async fn concurrent_read_write() -> Result<()> {
    let (store, root) = make_test_store().await?;
    let mut handles = Vec::new();
    for i in 0..8 {
        let store_clone = store.clone();
        handles.push(tokio::spawn(async move {
            let thread = AgentDbThread {
                id: format!("concurrent-thread-{i}"),
                workspace_id: None,
                surface_id: None,
                pane_id: None,
                agent_name: Some("test-agent".to_string()),
                title: format!("Concurrent Thread {i}"),
                created_at: 1000 + i as i64,
                updated_at: 1000 + i as i64,
                message_count: 0,
                total_tokens: 0,
                last_preview: String::new(),
                metadata_json: None,
            };
            store_clone.create_thread(&thread).await?;
            let loaded = store_clone.list_threads().await?;
            assert!(!loaded.is_empty());
            Ok::<(), anyhow::Error>(())
        }));
    }
    for handle in handles {
        handle.await??;
    }
    let all_threads = store.list_threads().await?;
    assert_eq!(all_threads.len(), 8);
    fs::remove_dir_all(root)?;
    Ok(())
}

// ── action_audit user_action column tests (BEAT-09/D-04) ────────────

#[tokio::test]
async fn ensure_column_adds_user_action_to_action_audit() -> Result<()> {
    let (store, root) = make_test_store().await?;
    // Verify the column exists by inserting and querying
    let has = store
        .conn
        .call(|conn| Ok(table_has_column(conn, "action_audit", "user_action")?))
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    assert!(has, "user_action column should exist after init_schema");
    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn dismiss_audit_entry_sets_user_action() -> Result<()> {
    let (store, root) = make_test_store().await?;
    let entry = AuditEntryRow {
        id: "test-dismiss-1".to_string(),
        timestamp: 1000,
        action_type: "heartbeat".to_string(),
        summary: "Test entry".to_string(),
        explanation: None,
        confidence: None,
        confidence_band: None,
        causal_trace_id: None,
        thread_id: None,
        goal_run_id: None,
        task_id: None,
        raw_data_json: None,
    };
    store.insert_action_audit(&entry).await?;
    store.dismiss_audit_entry("test-dismiss-1").await?;

    let user_action: Option<String> = store
        .conn
        .call(|conn| {
            conn.query_row(
                "SELECT user_action FROM action_audit WHERE id = ?1",
                ["test-dismiss-1"],
                |row| row.get(0),
            )
            .map_err(|e| e.into())
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    assert_eq!(user_action.as_deref(), Some("dismissed"));
    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn count_dismissals_by_type_returns_correct_counts() -> Result<()> {
    let (store, root) = make_test_store().await?;
    // Insert 3 heartbeat entries, dismiss 2
    for i in 0..3 {
        let entry = AuditEntryRow {
            id: format!("hb-{}", i),
            timestamp: 1000 + i,
            action_type: "heartbeat".to_string(),
            summary: format!("HB entry {}", i),
            explanation: None,
            confidence: None,
            confidence_band: None,
            causal_trace_id: None,
            thread_id: None,
            goal_run_id: None,
            task_id: None,
            raw_data_json: None,
        };
        store.insert_action_audit(&entry).await?;
    }
    store.dismiss_audit_entry("hb-0").await?;
    store.dismiss_audit_entry("hb-1").await?;

    // Insert 1 escalation entry, dismiss it
    let esc_entry = AuditEntryRow {
        id: "esc-0".to_string(),
        timestamp: 2000,
        action_type: "escalation".to_string(),
        summary: "Escalation".to_string(),
        explanation: None,
        confidence: None,
        confidence_band: None,
        causal_trace_id: None,
        thread_id: None,
        goal_run_id: None,
        task_id: None,
        raw_data_json: None,
    };
    store.insert_action_audit(&esc_entry).await?;
    store.dismiss_audit_entry("esc-0").await?;

    let counts = store.count_dismissals_by_type(0).await?;
    assert_eq!(counts.get("heartbeat").copied(), Some(2));
    assert_eq!(counts.get("escalation").copied(), Some(1));

    let shown = store.count_shown_by_type(0).await?;
    assert_eq!(shown.get("heartbeat").copied(), Some(3));
    assert_eq!(shown.get("escalation").copied(), Some(1));

    fs::remove_dir_all(root)?;
    Ok(())
}
