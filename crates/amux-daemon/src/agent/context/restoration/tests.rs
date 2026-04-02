use super::*;
use serde_json::json;

fn make_entry(id: &str, content: &str, tokens: u32, archived_at: u64) -> ArchiveEntry {
    ArchiveEntry {
        id: id.into(),
        thread_id: "thread-1".into(),
        original_role: None,
        compressed_content: content.into(),
        summary: None,
        relevance_score: 0.0,
        token_count_original: tokens,
        token_count_compressed: tokens,
        metadata: None,
        archived_at,
        last_accessed_at: None,
    }
}

fn make_entry_with_summary(
    id: &str,
    content: &str,
    summary: &str,
    tokens: u32,
    archived_at: u64,
) -> ArchiveEntry {
    let mut entry = make_entry(id, content, tokens, archived_at);
    entry.summary = Some(summary.into());
    entry
}

fn make_entry_with_tags(
    id: &str,
    content: &str,
    tokens: u32,
    archived_at: u64,
    tags: &[&str],
) -> ArchiveEntry {
    let mut entry = make_entry(id, content, tokens, archived_at);
    let tag_values: Vec<serde_json::Value> = tags
        .iter()
        .map(|tag| serde_json::Value::String((*tag).into()))
        .collect();
    entry.metadata = Some(json!({ "tags": tag_values }));
    entry
}

#[test]
fn rank_and_select_respects_max_items() {
    let entries: Vec<ArchiveEntry> = (0..10)
        .map(|i| make_entry(&format!("e{i}"), "content", 10, 1000 + i))
        .collect();

    let req = RestorationRequest {
        thread_id: "thread-1".into(),
        query: None,
        max_items: 3,
        max_tokens: 10_000,
    };

    let result = rank_and_select(&entries, &req);
    assert_eq!(result.len(), 3);
}

#[test]
fn rank_and_select_respects_max_tokens() {
    let entries: Vec<ArchiveEntry> = (0..5)
        .map(|i| make_entry(&format!("e{i}"), "content", 100, 1000 + i))
        .collect();

    let req = RestorationRequest {
        thread_id: "thread-1".into(),
        query: None,
        max_items: 100,
        max_tokens: 250,
    };

    let result = rank_and_select(&entries, &req);
    let total: u32 = result.iter().map(|item| item.tokens).sum();
    assert!(total <= 250);
    assert_eq!(result.len(), 2);
}

#[test]
fn rank_and_select_sorts_by_relevance_descending() {
    let entries = vec![
        make_entry_with_tags("low", "unrelated stuff", 10, 1000, &[]),
        make_entry_with_tags(
            "high",
            "the answer to config loading",
            10,
            1000,
            &["config"],
        ),
        make_entry_with_tags("mid", "some config notes", 10, 1000, &[]),
    ];

    let req = RestorationRequest {
        thread_id: "thread-1".into(),
        query: Some("config loading".into()),
        max_items: 10,
        max_tokens: 10_000,
    };

    let result = rank_and_select(&entries, &req);
    assert!(result.len() >= 2);
    for i in 0..result.len() - 1 {
        assert!(
            result[i].relevance_score >= result[i + 1].relevance_score,
            "Items should be sorted by relevance descending: {} >= {}",
            result[i].relevance_score,
            result[i + 1].relevance_score,
        );
    }
    assert_eq!(result[0].archive_id, "high");
}

#[test]
fn build_restoration_context_formats_readable_output() {
    let results = RestorationResult {
        items: vec![
            RestoredItem {
                archive_id: "a1".into(),
                content: "Some old conversation".into(),
                summary: Some("User asked about config".into()),
                relevance_score: 0.9,
                original_timestamp: 1700000000,
                tokens: 50,
            },
            RestoredItem {
                archive_id: "a2".into(),
                content: "Another conversation about builds".into(),
                summary: None,
                relevance_score: 0.6,
                original_timestamp: 1700000120,
                tokens: 40,
            },
        ],
        total_tokens: 90,
        query_used: Some("config".into()),
    };

    let ctx = build_restoration_context(&results);
    assert!(ctx.starts_with("[Restored context from archive]"));
    assert!(ctx.contains("Summary: User asked about config"));
    assert!(ctx.contains("[1700000000]"));
    assert!(ctx.contains("[1700000120]"));
    assert!(ctx.contains("Another conversation about builds"));
}

#[test]
fn empty_results_produces_empty_context() {
    let results = RestorationResult {
        items: Vec::new(),
        total_tokens: 0,
        query_used: None,
    };

    let ctx = build_restoration_context(&results);
    assert!(ctx.is_empty());
}

#[test]
fn default_request_has_reasonable_values() {
    let req = RestorationRequest::default();
    assert_eq!(req.max_items, 5);
    assert_eq!(req.max_tokens, 4000);
    assert!(req.thread_id.is_empty());
    assert!(req.query.is_none());
}

#[test]
fn format_restoration_prompt_includes_query() {
    let prompt = format_restoration_prompt("config loading");
    assert!(prompt.contains("config loading"));
    assert!(prompt.starts_with("Search archived context for:"));
}

#[test]
fn restored_item_token_counting() {
    let item = RestoredItem {
        archive_id: "x".into(),
        content: "hello world".into(),
        summary: None,
        relevance_score: 1.0,
        original_timestamp: 0,
        tokens: 42,
    };

    assert_eq!(item.tokens, 42);

    let estimated = ContextItem::estimate_tokens(&item.content);
    assert_eq!(estimated, 7);

    let items = vec![
        RestoredItem {
            tokens: 100,
            ..item.clone()
        },
        RestoredItem {
            tokens: 200,
            ..item.clone()
        },
        RestoredItem { tokens: 50, ..item },
    ];
    let total: u32 = items.iter().map(|item| item.tokens).sum();
    assert_eq!(total, 350);
}

#[test]
fn rank_and_select_skips_oversized_entry_but_includes_smaller_ones() {
    let entries = vec![
        make_entry("big", "big content", 5000, 3000),
        make_entry("small1", "small content 1", 100, 2000),
        make_entry("small2", "small content 2", 100, 1000),
    ];

    let req = RestorationRequest {
        thread_id: "thread-1".into(),
        query: None,
        max_items: 10,
        max_tokens: 250,
    };

    let result = rank_and_select(&entries, &req);
    assert!(result.iter().all(|item| item.archive_id != "big"));
    assert_eq!(result.len(), 2);
}

#[test]
fn build_restoration_context_truncates_long_content_preview() {
    let long_content = "a".repeat(200);
    let results = RestorationResult {
        items: vec![RestoredItem {
            archive_id: "long".into(),
            content: long_content,
            summary: None,
            relevance_score: 0.5,
            original_timestamp: 999,
            tokens: 50,
        }],
        total_tokens: 50,
        query_used: None,
    };

    let ctx = build_restoration_context(&results);
    assert!(ctx.contains("..."));
    assert!(!ctx.contains(&"a".repeat(200)));
}

#[test]
fn rank_and_select_with_query_matches_summary() {
    let entries = vec![
        make_entry_with_summary(
            "with-summary",
            "some unrelated content",
            "database migration steps",
            10,
            1000,
        ),
        make_entry("no-match", "unrelated content entirely", 10, 2000),
    ];

    let req = RestorationRequest {
        thread_id: "thread-1".into(),
        query: Some("database migration".into()),
        max_items: 10,
        max_tokens: 10_000,
    };

    let result = rank_and_select(&entries, &req);
    assert_eq!(result[0].archive_id, "with-summary");
    assert!(result[0].relevance_score > result[1].relevance_score);
}
