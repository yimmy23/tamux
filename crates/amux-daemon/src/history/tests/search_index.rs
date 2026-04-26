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
