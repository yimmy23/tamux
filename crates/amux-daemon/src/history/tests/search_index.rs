use super::*;

#[test]
fn tantivy_index_returns_ranked_history_documents() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let index = crate::history::search_index::SearchIndex::open(dir.path())?;

    index.upsert(crate::history::search_index::SearchDocument {
        source_kind: crate::history::search_index::SearchSourceKind::HistoryEntry,
        source_id: "exec-1".to_string(),
        title: "cargo build --workspace".to_string(),
        body: "Verify the daemon build stays green".to_string(),
        tags: vec!["managed-command".to_string()],
        workspace_id: Some("workspace-1".to_string()),
        thread_id: None,
        agent_id: None,
        timestamp: 10,
        metadata_json: None,
    })?;
    index.upsert(crate::history::search_index::SearchDocument {
        source_kind: crate::history::search_index::SearchSourceKind::HistoryEntry,
        source_id: "exec-2".to_string(),
        title: "npm lint".to_string(),
        body: "Check frontend style".to_string(),
        tags: vec!["managed-command".to_string()],
        workspace_id: Some("workspace-1".to_string()),
        thread_id: None,
        agent_id: None,
        timestamp: 20,
        metadata_json: None,
    })?;

    let hits = index.search(crate::history::search_index::SearchRequest {
        query: "daemon build".to_string(),
        limit: 5,
        source_kinds: vec![crate::history::search_index::SearchSourceKind::HistoryEntry],
        workspace_id: Some("workspace-1".to_string()),
        thread_id: None,
        agent_id: None,
    })?;

    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].source_id, "exec-1");
    assert_eq!(
        hits[0].source_kind,
        crate::history::search_index::SearchSourceKind::HistoryEntry
    );
    assert!(hits[0].score > 0.0);
    assert!(hits[0].snippet.as_deref().unwrap_or("").contains("daemon"));

    Ok(())
}

#[test]
fn tantivy_index_filters_guidelines_by_source_kind() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let index = crate::history::search_index::SearchIndex::open(dir.path())?;

    index.upsert(crate::history::search_index::SearchDocument {
        source_kind: crate::history::search_index::SearchSourceKind::Guideline,
        source_id: "coding/debugging.md".to_string(),
        title: "Debugging Task".to_string(),
        body: "Use systematic debugging before proposing fixes.".to_string(),
        tags: vec!["systematic-debugging".to_string()],
        workspace_id: None,
        thread_id: None,
        agent_id: None,
        timestamp: 30,
        metadata_json: None,
    })?;
    index.upsert(crate::history::search_index::SearchDocument {
        source_kind: crate::history::search_index::SearchSourceKind::HistoryEntry,
        source_id: "exec-1".to_string(),
        title: "debug command".to_string(),
        body: "A historical debug command".to_string(),
        tags: vec![],
        workspace_id: None,
        thread_id: None,
        agent_id: None,
        timestamp: 40,
        metadata_json: None,
    })?;

    let hits = index.search(crate::history::search_index::SearchRequest {
        query: "debugging fixes".to_string(),
        limit: 5,
        source_kinds: vec![crate::history::search_index::SearchSourceKind::Guideline],
        workspace_id: None,
        thread_id: None,
        agent_id: None,
    })?;

    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].source_id, "coding/debugging.md");
    assert_eq!(
        hits[0].source_kind,
        crate::history::search_index::SearchSourceKind::Guideline
    );

    Ok(())
}

#[test]
fn tantivy_index_batches_multiple_document_upserts() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let index = crate::history::search_index::SearchIndex::open(dir.path())?;

    index.upsert_many(vec![
        crate::history::search_index::SearchDocument {
            source_kind: crate::history::search_index::SearchSourceKind::AgentMessage,
            source_id: "batch-message-1".to_string(),
            title: "old batch message".to_string(),
            body: "old batch content".to_string(),
            tags: vec!["agent-message".to_string()],
            workspace_id: None,
            thread_id: Some("thread-1".to_string()),
            agent_id: None,
            timestamp: 10,
            metadata_json: None,
        },
        crate::history::search_index::SearchDocument {
            source_kind: crate::history::search_index::SearchSourceKind::AgentMessage,
            source_id: "batch-message-2".to_string(),
            title: "fresh batch message".to_string(),
            body: "fresh batch content".to_string(),
            tags: vec!["agent-message".to_string()],
            workspace_id: None,
            thread_id: Some("thread-1".to_string()),
            agent_id: None,
            timestamp: 20,
            metadata_json: None,
        },
    ])?;
    index.upsert_many(vec![crate::history::search_index::SearchDocument {
        source_kind: crate::history::search_index::SearchSourceKind::AgentMessage,
        source_id: "batch-message-1".to_string(),
        title: "updated batch message".to_string(),
        body: "updated batch content".to_string(),
        tags: vec!["agent-message".to_string()],
        workspace_id: None,
        thread_id: Some("thread-1".to_string()),
        agent_id: None,
        timestamp: 30,
        metadata_json: None,
    }])?;

    let hits = index.search(crate::history::search_index::SearchRequest {
        query: "batch content".to_string(),
        limit: 5,
        source_kinds: vec![crate::history::search_index::SearchSourceKind::AgentMessage],
        workspace_id: None,
        thread_id: Some("thread-1".to_string()),
        agent_id: None,
    })?;

    assert_eq!(hits.len(), 2);
    assert!(hits.iter().any(|hit| hit.source_id == "batch-message-1"));
    assert!(hits.iter().any(|hit| hit.source_id == "batch-message-2"));
    assert!(!hits.iter().any(|hit| {
        hit.source_id == "batch-message-1"
            && hit.snippet.as_deref().unwrap_or("").contains("old batch")
    }));

    Ok(())
}

#[test]
fn tantivy_index_serializes_concurrent_upserts() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let index = std::sync::Arc::new(crate::history::search_index::SearchIndex::open(dir.path())?);
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(8));
    let handles = (0..8)
        .map(|idx| {
            let index = std::sync::Arc::clone(&index);
            let barrier = std::sync::Arc::clone(&barrier);
            std::thread::spawn(move || {
                barrier.wait();
                index.upsert(crate::history::search_index::SearchDocument {
                    source_kind: crate::history::search_index::SearchSourceKind::AgentMessage,
                    source_id: format!("message-{idx}"),
                    title: format!("agent message {idx}"),
                    body: "concurrent tantivy writer lock regression".to_string(),
                    tags: vec!["agent-message".to_string()],
                    workspace_id: None,
                    thread_id: Some("thread-1".to_string()),
                    agent_id: None,
                    timestamp: idx,
                    metadata_json: None,
                })
            })
        })
        .collect::<Vec<_>>();

    for handle in handles {
        handle.join().expect("upsert thread should not panic")?;
    }

    let hits = index.search(crate::history::search_index::SearchRequest {
        query: "concurrent writer regression".to_string(),
        limit: 8,
        source_kinds: vec![crate::history::search_index::SearchSourceKind::AgentMessage],
        workspace_id: None,
        thread_id: Some("thread-1".to_string()),
        agent_id: None,
    })?;

    assert_eq!(hits.len(), 8);

    Ok(())
}

#[test]
fn tantivy_index_serializes_concurrent_upserts_across_handles() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let left_index =
        std::sync::Arc::new(crate::history::search_index::SearchIndex::open(dir.path())?);
    let right_index =
        std::sync::Arc::new(crate::history::search_index::SearchIndex::open(dir.path())?);
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(8));
    let handles = (0..8)
        .map(|idx| {
            let index = if idx % 2 == 0 {
                std::sync::Arc::clone(&left_index)
            } else {
                std::sync::Arc::clone(&right_index)
            };
            let barrier = std::sync::Arc::clone(&barrier);
            std::thread::spawn(move || {
                barrier.wait();
                index.upsert(crate::history::search_index::SearchDocument {
                    source_kind: crate::history::search_index::SearchSourceKind::AgentMessage,
                    source_id: format!("multi-handle-message-{idx}"),
                    title: format!("agent message {idx}"),
                    body: "multi handle concurrent tantivy writer regression".to_string(),
                    tags: vec!["agent-message".to_string()],
                    workspace_id: None,
                    thread_id: Some("thread-1".to_string()),
                    agent_id: None,
                    timestamp: idx,
                    metadata_json: None,
                })
            })
        })
        .collect::<Vec<_>>();

    for handle in handles {
        handle.join().expect("upsert thread should not panic")?;
    }

    let hits = left_index.search(crate::history::search_index::SearchRequest {
        query: "multi handle concurrent regression".to_string(),
        limit: 8,
        source_kinds: vec![crate::history::search_index::SearchSourceKind::AgentMessage],
        workspace_id: None,
        thread_id: Some("thread-1".to_string()),
        agent_id: None,
    })?;

    assert_eq!(hits.len(), 8);

    Ok(())
}

#[test]
fn tantivy_index_retries_when_external_writer_lock_is_busy() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let index = crate::history::search_index::SearchIndex::open(dir.path())?;
    let raw_index = tantivy::Index::open_in_dir(dir.path())?;
    let held_writer = raw_index.writer::<tantivy::schema::TantivyDocument>(50_000_000)?;
    let release_handle = std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(50));
        drop(held_writer);
    });

    index.upsert(crate::history::search_index::SearchDocument {
        source_kind: crate::history::search_index::SearchSourceKind::AgentMessage,
        source_id: "external-lock-message".to_string(),
        title: "external writer lock message".to_string(),
        body: "external tantivy writer lock retry regression".to_string(),
        tags: vec!["agent-message".to_string()],
        workspace_id: None,
        thread_id: Some("thread-1".to_string()),
        agent_id: None,
        timestamp: 10,
        metadata_json: None,
    })?;
    release_handle
        .join()
        .expect("writer release thread should not panic");

    let hits = index.search(crate::history::search_index::SearchRequest {
        query: "external retry regression".to_string(),
        limit: 1,
        source_kinds: vec![crate::history::search_index::SearchSourceKind::AgentMessage],
        workspace_id: None,
        thread_id: Some("thread-1".to_string()),
        agent_id: None,
    })?;

    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].source_id, "external-lock-message");

    Ok(())
}

#[test]
fn tantivy_index_gives_up_quickly_when_external_writer_lock_stays_busy() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let index = crate::history::search_index::SearchIndex::open(dir.path())?;
    let raw_index = tantivy::Index::open_in_dir(dir.path())?;
    let _held_writer = raw_index.writer::<tantivy::schema::TantivyDocument>(50_000_000)?;

    let started = std::time::Instant::now();
    let result = index.upsert(crate::history::search_index::SearchDocument {
        source_kind: crate::history::search_index::SearchSourceKind::AgentMessage,
        source_id: "long-external-lock-message".to_string(),
        title: "long external writer lock message".to_string(),
        body: "long external tantivy writer lock should not block search paths".to_string(),
        tags: vec!["agent-message".to_string()],
        workspace_id: None,
        thread_id: Some("thread-1".to_string()),
        agent_id: None,
        timestamp: 20,
        metadata_json: None,
    });

    assert!(
        started.elapsed() < std::time::Duration::from_millis(500),
        "upsert waited too long for an external tantivy writer lock"
    );
    let error = result.expect_err("upsert should give up while the external lock is held");
    assert!(
        error.to_string().contains("open tantivy index writer"),
        "{error:?}"
    );

    Ok(())
}
