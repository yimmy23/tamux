//! Context restoration — retrieve archived context by query or thread.

use serde::{Deserialize, Serialize};

use super::archive::ArchiveEntry;
#[allow(unused_imports)]
use super::context_item::ContextItem;

// ---------------------------------------------------------------------------
// Request / Result types
// ---------------------------------------------------------------------------

/// Parameters for a context restoration request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RestorationRequest {
    /// The thread whose archives should be searched.
    pub thread_id: String,
    /// Optional free-text query to rank results by relevance.
    pub query: Option<String>,
    /// Maximum number of items to return.
    pub max_items: usize,
    /// Maximum total token budget for the returned items.
    pub max_tokens: u32,
}

impl Default for RestorationRequest {
    fn default() -> Self {
        Self {
            thread_id: String::new(),
            query: None,
            max_items: 5,
            max_tokens: 4000,
        }
    }
}

/// A single item restored from the archive.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RestoredItem {
    /// The archive entry id this was restored from.
    pub archive_id: String,
    /// The restored content text.
    pub content: String,
    /// Optional summary of the content.
    pub summary: Option<String>,
    /// Relevance score (0.0–1.0, higher = more relevant).
    pub relevance_score: f64,
    /// Original creation timestamp of the context item.
    pub original_timestamp: u64,
    /// Estimated token count for this item.
    pub tokens: u32,
}

/// Result of a context restoration operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RestorationResult {
    /// The restored items, ordered by relevance descending.
    pub items: Vec<RestoredItem>,
    /// Sum of tokens across all returned items.
    pub total_tokens: u32,
    /// The query that was used (echoed back for callers).
    pub query_used: Option<String>,
}

// ---------------------------------------------------------------------------
// Functions
// ---------------------------------------------------------------------------

/// Rank archive entries by relevance and select items that fit within the
/// `max_items` and `max_tokens` budget specified in the request.
///
/// Entries are sorted by computed relevance descending. Items are included
/// greedily until either limit is reached. When a query is present, relevance
/// is based on keyword overlap with the entry's compressed content, summary,
/// and metadata tags. Without a query, relevance falls back to recency.
pub fn rank_and_select(
    entries: &[ArchiveEntry],
    request: &RestorationRequest,
) -> Vec<RestoredItem> {
    // Build scored list.
    let mut scored: Vec<(f64, &ArchiveEntry)> = entries
        .iter()
        .map(|e| {
            let score = compute_relevance(e, request.query.as_deref());
            (score, e)
        })
        .collect();

    // Sort by score descending (highest relevance first).
    scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

    let mut selected = Vec::new();
    let mut token_budget = request.max_tokens;

    for (score, entry) in scored {
        if selected.len() >= request.max_items {
            break;
        }
        let entry_tokens = entry.token_count_compressed;
        if entry_tokens > token_budget {
            // Skip entries that would exceed the budget but keep looking for
            // smaller ones — greedy-knapsack behaviour.
            continue;
        }

        token_budget -= entry_tokens;
        selected.push(RestoredItem {
            archive_id: entry.id.clone(),
            content: entry.compressed_content.clone(),
            summary: entry.summary.clone(),
            relevance_score: score,
            original_timestamp: entry.archived_at,
            tokens: entry_tokens,
        });
    }

    selected
}

/// Build a human-readable context block from restoration results, suitable
/// for injection into an LLM prompt.
///
/// Example output:
/// ```text
/// [Restored context from archive]
/// - [1700000000] Summary: User asked about config loading
/// - [1700000120] Another conversation about builds...
/// ```
pub fn build_restoration_context(results: &RestorationResult) -> String {
    if results.items.is_empty() {
        return String::new();
    }

    let mut out = String::from("[Restored context from archive]\n");

    for item in &results.items {
        let description = match &item.summary {
            Some(s) => format!("Summary: {s}"),
            None => {
                let preview: String = item.content.chars().take(80).collect();
                let ellipsis = if item.content.chars().count() > 80 {
                    "..."
                } else {
                    ""
                };
                format!("{preview}{ellipsis}")
            }
        };
        out.push_str(&format!(
            "- [{}] {}\n",
            item.original_timestamp, description
        ));
    }

    out
}

/// Build a prompt string that an agent tool could use to search archived
/// context for the given query.
pub fn format_restoration_prompt(query: &str) -> String {
    format!("Search archived context for: {query}")
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Extract tags from the metadata JSON value, if it is an object containing a
/// `"tags"` array of strings.
fn extract_tags(metadata: Option<&serde_json::Value>) -> Vec<String> {
    metadata
        .and_then(|v| v.get("tags"))
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default()
}

/// Compute a simple relevance score for an archive entry against an optional
/// query string. When no query is provided the score is based purely on
/// recency (more recent = higher score, normalised to 0–1).
fn compute_relevance(entry: &ArchiveEntry, query: Option<&str>) -> f64 {
    match query {
        Some(q) => {
            let q_lower = q.to_lowercase();
            let terms: Vec<&str> = q_lower.split_whitespace().collect();
            if terms.is_empty() {
                return 0.5;
            }

            let content_lower = entry.compressed_content.to_lowercase();
            let summary_lower = entry.summary.as_deref().unwrap_or_default().to_lowercase();
            let tags = extract_tags(entry.metadata.as_ref());
            let tags_lower: Vec<String> = tags.iter().map(|t| t.to_lowercase()).collect();

            let mut matched = 0usize;
            for term in &terms {
                if content_lower.contains(term)
                    || summary_lower.contains(term)
                    || tags_lower.iter().any(|t| t.contains(term))
                {
                    matched += 1;
                }
            }

            matched as f64 / terms.len() as f64
        }
        None => {
            // No query — score purely by timestamp (normalised to 0–1 assuming
            // timestamps span the last ~30 days in milliseconds).
            let max_age: f64 = 30.0 * 24.0 * 3600.0 * 1000.0;
            let now_approx = entry.archived_at as f64 + max_age;
            (entry.archived_at as f64 / now_approx).min(1.0)
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Helper to build an `ArchiveEntry` for testing.
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
        let mut e = make_entry(id, content, tokens, archived_at);
        e.summary = Some(summary.into());
        e
    }

    fn make_entry_with_tags(
        id: &str,
        content: &str,
        tokens: u32,
        archived_at: u64,
        tags: &[&str],
    ) -> ArchiveEntry {
        let mut e = make_entry(id, content, tokens, archived_at);
        let tag_values: Vec<serde_json::Value> = tags
            .iter()
            .map(|t| serde_json::Value::String((*t).into()))
            .collect();
        e.metadata = Some(json!({ "tags": tag_values }));
        e
    }

    // -----------------------------------------------------------------------
    // 1. rank_and_select respects max_items
    // -----------------------------------------------------------------------

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

    // -----------------------------------------------------------------------
    // 2. rank_and_select respects max_tokens
    // -----------------------------------------------------------------------

    #[test]
    fn rank_and_select_respects_max_tokens() {
        // 5 entries of 100 tokens each, but budget is only 250.
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
        let total: u32 = result.iter().map(|r| r.tokens).sum();
        assert!(total <= 250);
        assert_eq!(result.len(), 2); // 2 * 100 = 200 <= 250, 3 * 100 = 300 > 250
    }

    // -----------------------------------------------------------------------
    // 3. rank_and_select sorts by relevance descending
    // -----------------------------------------------------------------------

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
        // "high" should be first because it matches both terms (content + tag).
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

    // -----------------------------------------------------------------------
    // 4. build_restoration_context formats readable output
    // -----------------------------------------------------------------------

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
        // Second item has no summary, so content preview should appear.
        assert!(ctx.contains("Another conversation about builds"));
    }

    // -----------------------------------------------------------------------
    // 5. Empty results produces empty context
    // -----------------------------------------------------------------------

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

    // -----------------------------------------------------------------------
    // 6. Default request has reasonable values
    // -----------------------------------------------------------------------

    #[test]
    fn default_request_has_reasonable_values() {
        let req = RestorationRequest::default();
        assert_eq!(req.max_items, 5);
        assert_eq!(req.max_tokens, 4000);
        assert!(req.thread_id.is_empty());
        assert!(req.query.is_none());
    }

    // -----------------------------------------------------------------------
    // 7. format_restoration_prompt includes query
    // -----------------------------------------------------------------------

    #[test]
    fn format_restoration_prompt_includes_query() {
        let prompt = format_restoration_prompt("config loading");
        assert!(prompt.contains("config loading"));
        assert!(prompt.starts_with("Search archived context for:"));
    }

    // -----------------------------------------------------------------------
    // 8. RestoredItem token counting
    // -----------------------------------------------------------------------

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

        // The tokens field faithfully records the value provided.
        assert_eq!(item.tokens, 42);

        // Cross-check: token estimate from ContextItem utility matches
        // expected heuristic for this content.
        let estimated = ContextItem::estimate_tokens(&item.content);
        assert_eq!(estimated, 7); // 11 chars / 4 + 4

        // Multiple items' tokens sum correctly (used by RestorationResult).
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
        let total: u32 = items.iter().map(|i| i.tokens).sum();
        assert_eq!(total, 350);
    }

    // -----------------------------------------------------------------------
    // Extra edge-case tests
    // -----------------------------------------------------------------------

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
        // The 5000-token entry should be skipped.
        assert!(result.iter().all(|r| r.archive_id != "big"));
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
        // Should contain the 80-char preview plus "..."
        assert!(ctx.contains("..."));
        // Full 200-char content should NOT appear.
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
}
