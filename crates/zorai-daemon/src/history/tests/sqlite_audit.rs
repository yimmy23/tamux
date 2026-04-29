use super::*;
use crate::history::schema_helpers::table_has_column;

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

#[tokio::test]
async fn read_connection_uses_wal_and_query_only_mode() -> Result<()> {
    let (store, root) = make_test_store().await?;
    let read_pragmas = store
        .read_conn
        .call(|conn: &mut rusqlite::Connection| {
            let journal_mode: String =
                conn.query_row("PRAGMA journal_mode", [], |row| row.get(0))?;
            let query_only: i64 = conn.query_row("PRAGMA query_only", [], |row| row.get(0))?;
            let busy_timeout: i64 = conn.query_row("PRAGMA busy_timeout", [], |row| row.get(0))?;
            Ok((journal_mode, query_only, busy_timeout))
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    assert_eq!(read_pragmas.0.to_lowercase(), "wal");
    assert_eq!(read_pragmas.1, 1);
    assert_eq!(read_pragmas.2, 5000);

    let write_query_only: i64 = store
        .conn
        .call(|conn: &mut rusqlite::Connection| {
            conn.query_row("PRAGMA query_only", [], |row| row.get(0))
                .map_err(Into::into)
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    assert_eq!(write_query_only, 0);

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn agent_messages_have_cursor_friendly_thread_created_id_index() -> Result<()> {
    let (store, root) = make_test_store().await?;
    let indexed_columns = store
        .conn
        .call(|conn| {
            let mut stmt = conn.prepare("PRAGMA index_info('idx_messages_thread_created_id')")?;
            let rows = stmt.query_map([], |row| row.get::<_, String>(2))?;
            Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    assert_eq!(
        indexed_columns,
        vec![
            "thread_id".to_string(),
            "created_at".to_string(),
            "id".to_string()
        ]
    );

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
async fn init_schema_migrates_legacy_agent_messages_before_deleted_at_index() -> Result<()> {
    let root = std::env::temp_dir().join(format!("zorai-history-test-{}", Uuid::new_v4()));
    let history_dir = root.join("history");
    fs::create_dir_all(&history_dir)?;
    let db_path = history_dir.join("command-history.db");

    {
        let conn = rusqlite::Connection::open(&db_path)?;
        conn.execute_batch(
            "CREATE TABLE agent_threads (
                id             TEXT PRIMARY KEY,
                workspace_id   TEXT,
                surface_id     TEXT,
                pane_id        TEXT,
                agent_name     TEXT,
                title          TEXT NOT NULL,
                created_at     INTEGER NOT NULL,
                updated_at     INTEGER NOT NULL,
                message_count  INTEGER NOT NULL DEFAULT 0,
                total_tokens   INTEGER NOT NULL DEFAULT 0,
                last_preview   TEXT NOT NULL DEFAULT '',
                metadata_json  TEXT
            );
            CREATE TABLE agent_messages (
                id              TEXT PRIMARY KEY,
                thread_id       TEXT NOT NULL REFERENCES agent_threads(id) ON DELETE CASCADE,
                created_at      INTEGER NOT NULL,
                role            TEXT NOT NULL,
                content         TEXT NOT NULL DEFAULT '',
                provider        TEXT,
                model           TEXT,
                input_tokens    INTEGER,
                output_tokens   INTEGER,
                total_tokens    INTEGER,
                cost_usd        REAL,
                reasoning       TEXT,
                tool_calls_json TEXT,
                metadata_json   TEXT
            );",
        )?;
    }

    let store = HistoryStore::new_test_store(&root).await?;
    let (has_deleted_at, index_columns) = store
        .conn
        .call(|conn| {
            let has_deleted_at = table_has_column(conn, "agent_messages", "deleted_at")?;
            let mut stmt =
                conn.prepare("PRAGMA index_info('idx_messages_thread_deleted_created')")?;
            let rows = stmt.query_map([], |row| row.get::<_, String>(2))?;
            let index_columns = rows.collect::<std::result::Result<Vec<_>, _>>()?;
            Ok((has_deleted_at, index_columns))
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    assert!(has_deleted_at);
    assert_eq!(
        index_columns,
        vec![
            "thread_id".to_string(),
            "deleted_at".to_string(),
            "created_at".to_string(),
            "id".to_string()
        ]
    );

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn agent_message_cost_round_trips_through_history() -> Result<()> {
    let (store, root) = make_test_store().await?;
    let thread_id = "cost-thread";
    store
        .create_thread(&AgentDbThread {
            id: thread_id.to_string(),
            workspace_id: None,
            surface_id: None,
            pane_id: None,
            agent_name: Some("Svarog".to_string()),
            title: "Cost".to_string(),
            created_at: 1000,
            updated_at: 1000,
            message_count: 0,
            total_tokens: 0,
            last_preview: String::new(),
            metadata_json: None,
        })
        .await?;
    store
        .add_message(&AgentDbMessage {
            id: "m-cost".to_string(),
            thread_id: thread_id.to_string(),
            created_at: 1010,
            role: "assistant".to_string(),
            content: "priced".to_string(),
            provider: Some("openai".to_string()),
            model: Some("gpt-5.4-mini".to_string()),
            input_tokens: Some(11),
            output_tokens: Some(7),
            total_tokens: Some(18),
            cost_usd: Some(0.0123),
            reasoning: None,
            tool_calls_json: None,
            metadata_json: None,
        })
        .await?;

    let loaded = store.list_messages(thread_id, Some(10)).await?;
    assert_eq!(loaded[0].cost_usd, Some(0.0123));
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
        cost_usd: None,
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
        cost_usd: None,
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

#[tokio::test]
async fn replace_thread_snapshot_removes_pruned_messages_from_sqlite_search() -> Result<()> {
    let (store, root) = make_test_store().await?;
    let thread_id = "snapshot-search-prune-thread";
    let thread = AgentDbThread {
        id: thread_id.to_string(),
        workspace_id: None,
        surface_id: None,
        pane_id: None,
        agent_name: Some("test-agent".to_string()),
        title: "Snapshot Search Prune".to_string(),
        created_at: 1000,
        updated_at: 1000,
        message_count: 1,
        total_tokens: 1,
        last_preview: "obsolete marker body".to_string(),
        metadata_json: None,
    };
    let old_message = AgentDbMessage {
        id: "old-search-message".to_string(),
        thread_id: thread_id.to_string(),
        created_at: 1001,
        role: "assistant".to_string(),
        content: "obsolete marker body".to_string(),
        provider: None,
        model: None,
        input_tokens: Some(0),
        output_tokens: Some(1),
        total_tokens: Some(1),
        cost_usd: None,
        reasoning: None,
        tool_calls_json: None,
        metadata_json: None,
    };
    store.create_thread(&thread).await?;
    store.add_message(&old_message).await?;

    let refreshed_thread = AgentDbThread {
        updated_at: 2000,
        message_count: 1,
        total_tokens: 1,
        last_preview: "fresh marker body".to_string(),
        ..thread
    };
    let new_message = AgentDbMessage {
        id: "new-search-message".to_string(),
        thread_id: thread_id.to_string(),
        created_at: 2001,
        role: "assistant".to_string(),
        content: "fresh marker body".to_string(),
        provider: None,
        model: None,
        input_tokens: Some(0),
        output_tokens: Some(1),
        total_tokens: Some(1),
        cost_usd: None,
        reasoning: None,
        tool_calls_json: None,
        metadata_json: None,
    };

    store
        .replace_thread_snapshot(&refreshed_thread, &[new_message])
        .await?;

    let mut hits = Vec::new();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    while std::time::Instant::now() < deadline {
        hits = store.search("obsolete marker", 5).await?.1;
        if hits.iter().all(|hit| hit.id != old_message.id) {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
    assert!(
        hits.iter().all(|hit| hit.id != old_message.id),
        "pruned message remained searchable: {hits:?}"
    );

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn replace_thread_snapshot_does_not_regress_to_stale_snapshot() -> Result<()> {
    let (store, root) = make_test_store().await?;
    let thread_id = "thread-stale-snapshot-guard";
    let base_thread = AgentDbThread {
        id: thread_id.to_string(),
        workspace_id: None,
        surface_id: None,
        pane_id: None,
        agent_name: Some("Svarog".to_string()),
        title: "Stale Snapshot Guard".to_string(),
        created_at: 100,
        updated_at: 200,
        message_count: 2,
        total_tokens: 0,
        last_preview: "older".to_string(),
        metadata_json: None,
    };
    let older_messages = vec![
        AgentDbMessage {
            id: "m1".to_string(),
            thread_id: thread_id.to_string(),
            created_at: 110,
            role: "user".to_string(),
            content: "older 1".to_string(),
            provider: None,
            model: None,
            input_tokens: Some(0),
            output_tokens: Some(0),
            total_tokens: Some(0),
            cost_usd: None,
            reasoning: None,
            tool_calls_json: None,
            metadata_json: None,
        },
        AgentDbMessage {
            id: "m2".to_string(),
            thread_id: thread_id.to_string(),
            created_at: 120,
            role: "assistant".to_string(),
            content: "older 2".to_string(),
            provider: None,
            model: None,
            input_tokens: Some(0),
            output_tokens: Some(0),
            total_tokens: Some(0),
            cost_usd: None,
            reasoning: None,
            tool_calls_json: None,
            metadata_json: None,
        },
    ];
    store
        .replace_thread_snapshot(&base_thread, &older_messages)
        .await?;

    let newer_thread = AgentDbThread {
        updated_at: 300,
        message_count: 4,
        last_preview: "newer 4".to_string(),
        ..base_thread.clone()
    };
    let mut newer_messages = older_messages.clone();
    newer_messages.push(AgentDbMessage {
        id: "m3".to_string(),
        thread_id: thread_id.to_string(),
        created_at: 210,
        role: "user".to_string(),
        content: "newer 3".to_string(),
        provider: None,
        model: None,
        input_tokens: Some(0),
        output_tokens: Some(0),
        total_tokens: Some(0),
        cost_usd: None,
        reasoning: None,
        tool_calls_json: None,
        metadata_json: None,
    });
    newer_messages.push(AgentDbMessage {
        id: "m4".to_string(),
        thread_id: thread_id.to_string(),
        created_at: 220,
        role: "assistant".to_string(),
        content: "newer 4".to_string(),
        provider: None,
        model: None,
        input_tokens: Some(0),
        output_tokens: Some(0),
        total_tokens: Some(0),
        cost_usd: None,
        reasoning: None,
        tool_calls_json: None,
        metadata_json: None,
    });
    store
        .replace_thread_snapshot(&newer_thread, &newer_messages)
        .await?;

    let stale_thread = AgentDbThread {
        updated_at: 250,
        message_count: 2,
        last_preview: "stale older".to_string(),
        ..base_thread
    };
    store
        .replace_thread_snapshot(&stale_thread, &older_messages)
        .await?;

    let loaded_thread = store
        .get_thread(thread_id)
        .await?
        .expect("thread should remain persisted");
    let loaded_messages = store.list_messages(thread_id, None).await?;

    assert_eq!(loaded_thread.updated_at, 300);
    assert_eq!(loaded_thread.message_count, 4);
    assert_eq!(loaded_thread.last_preview, "newer 4");
    assert_eq!(loaded_messages.len(), 4);
    assert_eq!(
        loaded_messages
            .iter()
            .map(|message| message.content.as_str())
            .collect::<Vec<_>>(),
        vec!["older 1", "older 2", "newer 3", "newer 4"]
    );

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn reconcile_thread_snapshot_updates_changed_messages_without_pruning_missing_ones(
) -> Result<()> {
    let (store, root) = make_test_store().await?;
    let thread_id = "thread-reconcile-snapshot";
    let base_thread = AgentDbThread {
        id: thread_id.to_string(),
        workspace_id: None,
        surface_id: None,
        pane_id: None,
        agent_name: Some("Svarog".to_string()),
        title: "Reconcile Thread".to_string(),
        created_at: 100,
        updated_at: 200,
        message_count: 2,
        total_tokens: 5,
        last_preview: "old assistant".to_string(),
        metadata_json: Some("{\"phase\":\"old\"}".to_string()),
    };
    let initial_messages = vec![
        AgentDbMessage {
            id: "m1".to_string(),
            thread_id: thread_id.to_string(),
            created_at: 110,
            role: "user".to_string(),
            content: "hello".to_string(),
            provider: None,
            model: None,
            input_tokens: Some(1),
            output_tokens: Some(0),
            total_tokens: Some(1),
            cost_usd: None,
            reasoning: None,
            tool_calls_json: None,
            metadata_json: Some("{\"v\":1}".to_string()),
        },
        AgentDbMessage {
            id: "m2".to_string(),
            thread_id: thread_id.to_string(),
            created_at: 120,
            role: "assistant".to_string(),
            content: "old assistant".to_string(),
            provider: Some("github-copilot".to_string()),
            model: Some("gpt-5.4".to_string()),
            input_tokens: Some(2),
            output_tokens: Some(3),
            total_tokens: Some(5),
            cost_usd: None,
            reasoning: Some("old reasoning".to_string()),
            tool_calls_json: None,
            metadata_json: Some("{\"v\":1}".to_string()),
        },
    ];
    store
        .reconcile_thread_snapshot(&base_thread, &initial_messages)
        .await?;

    let updated_thread = AgentDbThread {
        updated_at: 300,
        message_count: 2,
        total_tokens: 9,
        last_preview: "new assistant".to_string(),
        metadata_json: Some("{\"phase\":\"new\"}".to_string()),
        ..base_thread
    };
    let updated_messages = vec![
        initial_messages[0].clone(),
        AgentDbMessage {
            id: "m2".to_string(),
            thread_id: thread_id.to_string(),
            created_at: 120,
            role: "assistant".to_string(),
            content: "new assistant".to_string(),
            provider: Some("github-copilot".to_string()),
            model: Some("gpt-5.4".to_string()),
            input_tokens: Some(4),
            output_tokens: Some(5),
            total_tokens: Some(9),
            cost_usd: None,
            reasoning: Some("new reasoning".to_string()),
            tool_calls_json: None,
            metadata_json: Some("{\"v\":2}".to_string()),
        },
    ];
    store
        .reconcile_thread_snapshot(&updated_thread, &updated_messages)
        .await?;

    let loaded_thread = store
        .get_thread(thread_id)
        .await?
        .expect("thread should exist after reconcile");
    let loaded_messages = store.list_messages(thread_id, None).await?;

    assert_eq!(loaded_thread.message_count, 2);
    assert_eq!(loaded_thread.last_preview, "new assistant");
    assert_eq!(
        loaded_thread.metadata_json.as_deref(),
        Some("{\"phase\":\"new\"}")
    );
    assert_eq!(loaded_messages.len(), 2);
    assert_eq!(loaded_messages[1].content, "new assistant");
    assert_eq!(
        loaded_messages[1].reasoning.as_deref(),
        Some("new reasoning")
    );
    assert_eq!(loaded_messages[1].total_tokens, Some(9));
    assert_eq!(
        loaded_messages[1].metadata_json.as_deref(),
        Some("{\"v\":2}")
    );

    let pruned_thread = AgentDbThread {
        updated_at: 400,
        message_count: 1,
        total_tokens: 1,
        last_preview: "hello".to_string(),
        metadata_json: Some("{\"phase\":\"pruned\"}".to_string()),
        ..updated_thread
    };
    store
        .reconcile_thread_snapshot(&pruned_thread, &[initial_messages[0].clone()])
        .await?;

    let pruned_messages = store.list_messages(thread_id, None).await?;
    assert_eq!(
        pruned_messages
            .iter()
            .map(|message| message.id.as_str())
            .collect::<Vec<_>>(),
        vec!["m1", "m2"],
        "reconcile snapshots may be partial and must not tombstone omitted messages"
    );
    assert_eq!(store.restore_messages(thread_id, &["m2"]).await?, 0);

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn reconcile_thread_snapshot_keeps_existing_prefix_for_new_suffix_snapshot() -> Result<()> {
    let (store, root) = make_test_store().await?;
    let thread_id = "thread-reconcile-new-suffix";
    let base_thread = AgentDbThread {
        id: thread_id.to_string(),
        workspace_id: None,
        surface_id: None,
        pane_id: None,
        agent_name: Some("Svarog".to_string()),
        title: "Suffix Snapshot".to_string(),
        created_at: 100,
        updated_at: 200,
        message_count: 2,
        total_tokens: 0,
        last_preview: "old assistant".to_string(),
        metadata_json: None,
    };
    let initial_messages = vec![
        AgentDbMessage {
            id: "m1".to_string(),
            thread_id: thread_id.to_string(),
            created_at: 110,
            role: "user".to_string(),
            content: "old prompt".to_string(),
            provider: None,
            model: None,
            input_tokens: Some(0),
            output_tokens: Some(0),
            total_tokens: Some(0),
            cost_usd: None,
            reasoning: None,
            tool_calls_json: None,
            metadata_json: None,
        },
        AgentDbMessage {
            id: "m2".to_string(),
            thread_id: thread_id.to_string(),
            created_at: 120,
            role: "assistant".to_string(),
            content: "old assistant".to_string(),
            provider: Some("github-copilot".to_string()),
            model: Some("gpt-5.4".to_string()),
            input_tokens: Some(0),
            output_tokens: Some(0),
            total_tokens: Some(0),
            cost_usd: None,
            reasoning: None,
            tool_calls_json: None,
            metadata_json: None,
        },
    ];
    store
        .reconcile_thread_snapshot(&base_thread, &initial_messages)
        .await?;

    let suffix_thread = AgentDbThread {
        updated_at: 300,
        message_count: 1,
        last_preview: "new prompt".to_string(),
        ..base_thread
    };
    let suffix_messages = vec![AgentDbMessage {
        id: "m3".to_string(),
        thread_id: thread_id.to_string(),
        created_at: 310,
        role: "user".to_string(),
        content: "new prompt".to_string(),
        provider: None,
        model: None,
        input_tokens: Some(0),
        output_tokens: Some(0),
        total_tokens: Some(0),
        cost_usd: None,
        reasoning: None,
        tool_calls_json: None,
        metadata_json: None,
    }];
    store
        .reconcile_thread_snapshot(&suffix_thread, &suffix_messages)
        .await?;

    let loaded_thread = store
        .get_thread(thread_id)
        .await?
        .expect("thread should exist after suffix reconcile");
    let loaded_messages = store.list_messages(thread_id, None).await?;

    assert_eq!(loaded_thread.message_count, 3);
    assert_eq!(loaded_thread.last_preview, "new prompt");
    assert_eq!(
        loaded_messages
            .iter()
            .map(|message| message.id.as_str())
            .collect::<Vec<_>>(),
        vec!["m1", "m2", "m3"]
    );

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn delete_messages_soft_deletes_and_restore_makes_visible_again() -> Result<()> {
    let (store, root) = make_test_store().await?;
    let thread_id = "thread-soft-delete-messages";
    store
        .create_thread(&AgentDbThread {
            id: thread_id.to_string(),
            workspace_id: None,
            surface_id: None,
            pane_id: None,
            agent_name: Some("Svarog".to_string()),
            title: "Soft Delete Messages".to_string(),
            created_at: 100,
            updated_at: 100,
            message_count: 0,
            total_tokens: 0,
            last_preview: String::new(),
            metadata_json: None,
        })
        .await?;

    for (id, created_at, content) in [
        ("m1", 110, "keep visible"),
        ("m2", 120, "trash marker searchable"),
        ("m3", 130, "latest deleted preview"),
    ] {
        store
            .add_message(&AgentDbMessage {
                id: id.to_string(),
                thread_id: thread_id.to_string(),
                created_at,
                role: "user".to_string(),
                content: content.to_string(),
                provider: None,
                model: None,
                input_tokens: Some(1),
                output_tokens: Some(0),
                total_tokens: Some(1),
                cost_usd: None,
                reasoning: None,
                tool_calls_json: None,
                metadata_json: None,
            })
            .await?;
    }

    assert_eq!(store.delete_messages(thread_id, &["m2", "m3"]).await?, 2);

    let visible = store.list_messages(thread_id, None).await?;
    assert_eq!(
        visible
            .iter()
            .map(|message| message.id.as_str())
            .collect::<Vec<_>>(),
        vec!["m1"]
    );
    let with_trash = store.list_messages_with_deleted(thread_id, None).await?;
    assert_eq!(
        with_trash
            .iter()
            .map(|message| message.id.as_str())
            .collect::<Vec<_>>(),
        vec!["m1", "m2", "m3"]
    );
    let (window, total_count, loaded_start, loaded_end) =
        store.list_message_window(thread_id, 10, 0).await?;
    assert_eq!(total_count, 1);
    assert_eq!((loaded_start, loaded_end), (0, 1));
    assert_eq!(
        window
            .iter()
            .map(|message| message.id.as_str())
            .collect::<Vec<_>>(),
        vec!["m1"]
    );
    let recent = store.list_recent_messages(thread_id, 10).await?;
    assert_eq!(
        recent
            .iter()
            .map(|message| message.id.as_str())
            .collect::<Vec<_>>(),
        vec!["m1"]
    );
    let after_cursor = store
        .list_messages_after_cursor(thread_id, None, None)
        .await?;
    assert_eq!(
        after_cursor
            .iter()
            .map(|message| message.id.as_str())
            .collect::<Vec<_>>(),
        vec!["m1"]
    );
    let thread = store
        .get_thread(thread_id)
        .await?
        .expect("thread should remain after soft delete");
    assert_eq!(thread.message_count, 1);
    assert_eq!(thread.last_preview, "keep visible");

    let mut hits = Vec::new();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    while std::time::Instant::now() < deadline {
        hits = store.search("trash marker searchable", 5).await?.1;
        if hits.iter().all(|hit| hit.id != "m2") {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
    assert!(
        hits.iter().all(|hit| hit.id != "m2"),
        "soft-deleted message remained searchable: {hits:?}"
    );

    assert_eq!(store.restore_messages(thread_id, &["m2", "m3"]).await?, 2);
    let restored = store.list_messages(thread_id, None).await?;
    assert_eq!(
        restored
            .iter()
            .map(|message| message.id.as_str())
            .collect::<Vec<_>>(),
        vec!["m1", "m2", "m3"]
    );
    let thread = store
        .get_thread(thread_id)
        .await?
        .expect("thread should remain after restore");
    assert_eq!(thread.message_count, 3);
    assert_eq!(thread.last_preview, "latest deleted preview");

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn list_messages_with_limit_returns_latest_messages_in_chronological_order() -> Result<()> {
    let (store, root) = make_test_store().await?;
    let thread_id = "limited-latest-thread";

    store
        .create_thread(&AgentDbThread {
            id: thread_id.to_string(),
            workspace_id: None,
            surface_id: None,
            pane_id: None,
            agent_name: Some("test-agent".to_string()),
            title: "Latest limited slice".to_string(),
            created_at: 1_000,
            updated_at: 1_000,
            message_count: 0,
            total_tokens: 0,
            last_preview: String::new(),
            metadata_json: None,
        })
        .await?;

    for index in 0..8 {
        store
            .add_message(&AgentDbMessage {
                id: format!("msg-{index}"),
                thread_id: thread_id.to_string(),
                created_at: 1_000 + index,
                role: "user".to_string(),
                content: format!("message-{index}"),
                provider: None,
                model: None,
                input_tokens: Some(0),
                output_tokens: Some(0),
                total_tokens: Some(0),
                cost_usd: None,
                reasoning: None,
                tool_calls_json: None,
                metadata_json: None,
            })
            .await?;
    }

    let loaded_messages = store.list_messages(thread_id, Some(3)).await?;
    let contents = loaded_messages
        .iter()
        .map(|message| message.content.as_str())
        .collect::<Vec<_>>();
    assert_eq!(contents, vec!["message-5", "message-6", "message-7"]);

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
async fn init_schema_migrates_legacy_causal_traces_before_family_index() -> Result<()> {
    let root = std::env::temp_dir().join(format!("zorai-history-test-{}", Uuid::new_v4()));
    let history_dir = root.join("history");
    fs::create_dir_all(&history_dir)?;
    let db_path = history_dir.join("command-history.db");

    {
        let conn = rusqlite::Connection::open(&db_path)?;
        conn.execute_batch(
            "CREATE TABLE causal_traces (
                id                    TEXT PRIMARY KEY,
                thread_id             TEXT,
                goal_run_id           TEXT,
                task_id               TEXT,
                decision_type         TEXT NOT NULL,
                selected_json         TEXT NOT NULL,
                rejected_options_json TEXT,
                context_hash          TEXT,
                causal_factors_json   TEXT,
                outcome_json          TEXT NOT NULL,
                model_used            TEXT,
                created_at            INTEGER NOT NULL
            );",
        )?;
    }

    let store = HistoryStore::new_test_store(&root).await?;
    let (has_trace_family, index_columns) = store
        .conn
        .call(|conn| {
            let has_trace_family = table_has_column(conn, "causal_traces", "trace_family")?;
            let mut stmt = conn.prepare("PRAGMA index_info('idx_causal_traces_family')")?;
            let rows = stmt.query_map([], |row| row.get::<_, String>(2))?;
            let index_columns = rows.collect::<std::result::Result<Vec<_>, _>>()?;
            Ok((has_trace_family, index_columns))
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    assert!(has_trace_family);
    assert_eq!(
        index_columns,
        vec!["trace_family".to_string(), "created_at".to_string()]
    );

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
