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

#[cfg(test)]
#[path = "restoration/tests.rs"]
mod tests;
