//! Context archive — store, search, and retrieve evicted context items.

use serde::{Deserialize, Serialize};

use super::context_item::*;

// ---------------------------------------------------------------------------
// Data structures
// ---------------------------------------------------------------------------

/// An archived context item with compressed content and metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchiveEntry {
    /// Unique identifier (matches the original `ContextItem::id`).
    pub id: String,
    /// The thread this entry belongs to.
    pub thread_id: String,
    /// Original role/source (e.g. "user", "assistant", "tool").
    pub original_role: Option<String>,
    /// Compressed or truncated version of the original content.
    pub compressed_content: String,
    /// Optional brief summary of the original content.
    pub summary: Option<String>,
    /// Relevance score at the time of archival.
    pub relevance_score: f64,
    /// Token count of the *original* content before compression.
    pub token_count_original: u32,
    /// Token count of the *compressed* content.
    pub token_count_compressed: u32,
    /// Arbitrary metadata (tags, provenance, etc.).
    pub metadata: Option<serde_json::Value>,
    /// Timestamp (epoch millis) when the entry was archived.
    pub archived_at: u64,
    /// Timestamp (epoch millis) of the most recent retrieval, if any.
    pub last_accessed_at: Option<u64>,
}

/// Configures how long and how many archived entries to keep.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetentionPolicy {
    /// Maximum age (in days) before an entry is purged.  Default: 30.
    pub max_age_days: u32,
    /// Maximum number of entries per thread.  Default: 500.
    pub max_entries_per_thread: usize,
}

impl Default for RetentionPolicy {
    fn default() -> Self {
        Self {
            max_age_days: 30,
            max_entries_per_thread: 500,
        }
    }
}

// ---------------------------------------------------------------------------
// ArchiveManager
// ---------------------------------------------------------------------------

/// In-memory manager that prepares entries for archival and evaluates
/// retention rules.  Actual persistence is delegated to `HistoryStore`
/// (Phase 2).
pub struct ArchiveManager {
    retention: RetentionPolicy,
}

impl ArchiveManager {
    /// Create a new `ArchiveManager` with the given retention policy.
    pub fn new(retention: RetentionPolicy) -> Self {
        Self { retention }
    }

    /// Convert a live [`ContextItem`] into an [`ArchiveEntry`] ready for
    /// storage.
    ///
    /// The content is compressed via [`compress_for_archive`] using a default
    /// maximum of 2 000 characters.
    pub fn prepare_for_archive(
        item: &ContextItem,
        thread_id: &str,
        now: u64,
    ) -> ArchiveEntry {
        const MAX_COMPRESSED_CHARS: usize = 2_000;

        let (compressed_content, summary) =
            compress_for_archive(&item.content, MAX_COMPRESSED_CHARS);

        let token_count_compressed = ContextItem::estimate_tokens(&compressed_content);

        ArchiveEntry {
            id: item.id.clone(),
            thread_id: thread_id.to_owned(),
            original_role: Some(item.source.clone()),
            compressed_content,
            summary,
            relevance_score: item.relevance_score,
            token_count_original: item.estimated_tokens,
            token_count_compressed,
            metadata: None,
            archived_at: now,
            last_accessed_at: None,
        }
    }

    /// Check whether an entry should still be retained according to the
    /// age-based portion of the retention policy.
    ///
    /// The per-thread count limit is *not* checked here because that requires
    /// a database query; the caller is responsible for enforcing it.
    pub fn should_retain(&self, entry: &ArchiveEntry, now: u64) -> bool {
        let max_age_ms = u64::from(self.retention.max_age_days) * 24 * 60 * 60 * 1_000;
        let age_ms = now.saturating_sub(entry.archived_at);
        age_ms <= max_age_ms
    }

    /// Build an FTS5-compatible query string from a free-form user query.
    ///
    /// Basic transformations:
    /// - Strips characters that are special to FTS5 (`"`, `*`, `(`, `)`,
    ///   `:`, `^`, `{`, `}`, `~`).
    /// - Splits into whitespace-delimited tokens.
    /// - Wraps each token in double quotes for exact matching.
    /// - Joins with `AND` (implicit in FTS5 when tokens are space-separated,
    ///   but we are explicit for clarity).
    pub fn build_search_query(query: &str) -> String {
        let cleaned: String = query
            .chars()
            .map(|c| match c {
                '"' | '*' | '(' | ')' | ':' | '^' | '{' | '}' | '~' => ' ',
                _ => c,
            })
            .collect();

        let tokens: Vec<String> = cleaned
            .split_whitespace()
            .filter(|t| !t.is_empty())
            .map(|t| format!("\"{t}\""))
            .collect();

        if tokens.is_empty() {
            String::new()
        } else {
            tokens.join(" AND ")
        }
    }
}

// ---------------------------------------------------------------------------
// Standalone helpers
// ---------------------------------------------------------------------------

/// Compress content for archival storage.
///
/// If the content is already shorter than `max_chars` it is returned as-is
/// with no summary.  Otherwise a brief summary is generated from the first
/// portion of the text and the content is truncated.
///
/// Returns `(compressed_content, summary)`.
pub fn compress_for_archive(content: &str, max_chars: usize) -> (String, Option<String>) {
    if content.len() <= max_chars {
        return (content.to_owned(), None);
    }

    // Build a simple extractive summary from the first few lines.
    let summary = build_extractive_summary(content);

    // Truncate at a character boundary, trying to land on a whitespace break.
    let truncated = smart_truncate(content, max_chars);

    (truncated, Some(summary))
}

/// Create a brief extractive summary by taking the first non-empty lines.
fn build_extractive_summary(content: &str) -> String {
    const MAX_SUMMARY_LINES: usize = 3;
    const MAX_SUMMARY_CHARS: usize = 300;

    let mut summary = String::new();
    let mut lines_taken = 0;

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if !summary.is_empty() {
            summary.push(' ');
        }
        summary.push_str(trimmed);
        lines_taken += 1;
        if lines_taken >= MAX_SUMMARY_LINES || summary.len() >= MAX_SUMMARY_CHARS {
            break;
        }
    }

    if summary.len() > MAX_SUMMARY_CHARS {
        smart_truncate(&summary, MAX_SUMMARY_CHARS)
    } else {
        summary
    }
}

/// Truncate `s` to at most `max_chars`, preferring to break on whitespace.
fn smart_truncate(s: &str, max_chars: usize) -> String {
    if s.len() <= max_chars {
        return s.to_owned();
    }

    // Walk back from the limit to find a whitespace boundary.
    let boundary = s[..max_chars]
        .rfind(char::is_whitespace)
        .unwrap_or(max_chars);

    let mut out = s[..boundary].to_owned();
    out.push_str("...");
    out
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Helper: build a `ContextItem` for testing.
    fn make_item(id: &str, content: &str) -> ContextItem {
        ContextItem {
            id: id.into(),
            item_type: ContextType::Conversation,
            content: content.into(),
            timestamp: 1_000,
            relevance_score: 0.42,
            access_count: 3,
            source: "user".into(),
            estimated_tokens: ContextItem::estimate_tokens(content),
        }
    }

    // 1. prepare_for_archive creates a valid entry --------------------------
    #[test]
    fn prepare_for_archive_creates_valid_entry() {
        let item = make_item("ctx-1", "Hello, world!");
        let entry = ArchiveManager::prepare_for_archive(&item, "thread-a", 5_000);

        assert_eq!(entry.id, "ctx-1");
        assert_eq!(entry.thread_id, "thread-a");
        assert_eq!(entry.original_role.as_deref(), Some("user"));
        assert_eq!(entry.archived_at, 5_000);
        assert!(entry.last_accessed_at.is_none());
        assert_eq!(entry.relevance_score, 0.42);
        assert!(entry.token_count_original > 0);
        assert!(entry.token_count_compressed > 0);
    }

    // 2. Short content preserved as-is -------------------------------------
    #[test]
    fn short_content_preserved_as_is() {
        let short = "Brief note.";
        let (compressed, summary) = compress_for_archive(short, 200);
        assert_eq!(compressed, short);
        assert!(summary.is_none());
    }

    // 3. Long content gets a summary ---------------------------------------
    #[test]
    fn long_content_gets_summary() {
        let long = "word ".repeat(1_000); // 5 000 chars
        let (compressed, summary) = compress_for_archive(&long, 200);

        assert!(compressed.len() <= 210); // 200 + "..."
        assert!(summary.is_some());
        assert!(!summary.unwrap().is_empty());
    }

    // 4. Retention policy respects max_age_days ----------------------------
    #[test]
    fn retention_policy_respects_max_age_days() {
        let policy = RetentionPolicy {
            max_age_days: 7,
            max_entries_per_thread: 100,
        };
        let mgr = ArchiveManager::new(policy);

        let entry = ArchiveEntry {
            id: "e1".into(),
            thread_id: "t1".into(),
            original_role: None,
            compressed_content: String::new(),
            summary: None,
            relevance_score: 0.0,
            token_count_original: 0,
            token_count_compressed: 0,
            metadata: None,
            archived_at: 0,
            last_accessed_at: None,
        };

        // 8 days in milliseconds — should exceed 7-day policy.
        let eight_days_ms = 8 * 24 * 60 * 60 * 1_000;
        assert!(!mgr.should_retain(&entry, eight_days_ms));
    }

    // 5. Fresh entries pass retention check ---------------------------------
    #[test]
    fn fresh_entries_pass_retention_check() {
        let mgr = ArchiveManager::new(RetentionPolicy::default());
        let entry = ArchiveEntry {
            id: "e2".into(),
            thread_id: "t1".into(),
            original_role: None,
            compressed_content: String::new(),
            summary: None,
            relevance_score: 0.5,
            token_count_original: 10,
            token_count_compressed: 5,
            metadata: None,
            archived_at: 1_000_000,
            last_accessed_at: None,
        };

        // Check 1 second later — still fresh.
        assert!(mgr.should_retain(&entry, 1_001_000));
    }

    // 6. Old entries fail retention check -----------------------------------
    #[test]
    fn old_entries_fail_retention_check() {
        let mgr = ArchiveManager::new(RetentionPolicy {
            max_age_days: 1,
            ..Default::default()
        });
        let entry = ArchiveEntry {
            id: "e3".into(),
            thread_id: "t1".into(),
            original_role: None,
            compressed_content: String::new(),
            summary: None,
            relevance_score: 0.5,
            token_count_original: 10,
            token_count_compressed: 5,
            metadata: None,
            archived_at: 0,
            last_accessed_at: None,
        };

        // 2 days later.
        let two_days_ms = 2 * 24 * 60 * 60 * 1_000;
        assert!(!mgr.should_retain(&entry, two_days_ms));
    }

    // 7. FTS query escaping handles special chars --------------------------
    #[test]
    fn fts_query_escaping_handles_special_chars() {
        let raw = r#"hello "world" foo:bar (baz) qux*"#;
        let query = ArchiveManager::build_search_query(raw);

        // No raw special chars should remain unquoted.
        assert!(!query.contains('*'));
        assert!(!query.contains('('));
        assert!(!query.contains(')'));
        assert!(!query.contains(':'));

        // Each token is double-quoted.
        assert!(query.contains("\"hello\""));
        assert!(query.contains("\"world\""));
        assert!(query.contains("\"foo\""));
        assert!(query.contains("\"bar\""));
        assert!(query.contains("\"baz\""));
        assert!(query.contains("\"qux\""));

        // Tokens joined with AND.
        assert!(query.contains(" AND "));
    }

    // 8. Default retention policy values -----------------------------------
    #[test]
    fn default_retention_policy_values() {
        let policy = RetentionPolicy::default();
        assert_eq!(policy.max_age_days, 30);
        assert_eq!(policy.max_entries_per_thread, 500);
    }

    // 9. Archive entry roundtrip JSON --------------------------------------
    #[test]
    fn archive_entry_roundtrip_json() {
        let entry = ArchiveEntry {
            id: "rt-1".into(),
            thread_id: "thread-rt".into(),
            original_role: Some("assistant".into()),
            compressed_content: "compressed text".into(),
            summary: Some("summary text".into()),
            relevance_score: 0.75,
            token_count_original: 100,
            token_count_compressed: 40,
            metadata: Some(json!({"tag": "test"})),
            archived_at: 123_456_789,
            last_accessed_at: Some(123_456_999),
        };

        let serialized = serde_json::to_string(&entry).expect("serialize");
        let deserialized: ArchiveEntry =
            serde_json::from_str(&serialized).expect("deserialize");

        assert_eq!(deserialized.id, entry.id);
        assert_eq!(deserialized.thread_id, entry.thread_id);
        assert_eq!(deserialized.original_role, entry.original_role);
        assert_eq!(deserialized.compressed_content, entry.compressed_content);
        assert_eq!(deserialized.summary, entry.summary);
        assert!((deserialized.relevance_score - entry.relevance_score).abs() < f64::EPSILON);
        assert_eq!(deserialized.token_count_original, entry.token_count_original);
        assert_eq!(
            deserialized.token_count_compressed,
            entry.token_count_compressed
        );
        assert_eq!(deserialized.metadata, entry.metadata);
        assert_eq!(deserialized.archived_at, entry.archived_at);
        assert_eq!(deserialized.last_accessed_at, entry.last_accessed_at);
    }

    // 10. Compression ratio is reasonable ----------------------------------
    #[test]
    fn compression_ratio_is_reasonable() {
        // 10 000 characters of content, compressed to at most 2 000.
        let long = "x".repeat(10_000);
        let (compressed, _) = compress_for_archive(&long, 2_000);

        // Compressed output should be noticeably smaller than the original.
        let ratio = compressed.len() as f64 / long.len() as f64;
        assert!(
            ratio < 0.25,
            "compression ratio {ratio:.2} should be < 0.25"
        );

        // But not empty.
        assert!(!compressed.is_empty());
    }

    // Bonus: empty query produces empty string -----------------------------
    #[test]
    fn empty_query_produces_empty_string() {
        assert!(ArchiveManager::build_search_query("").is_empty());
        assert!(ArchiveManager::build_search_query("   ").is_empty());
        assert!(ArchiveManager::build_search_query("***").is_empty());
    }

    // Bonus: prepare_for_archive with long content -------------------------
    #[test]
    fn prepare_for_archive_compresses_long_content() {
        let long_content = "Some detailed context. ".repeat(500);
        let item = make_item("ctx-long", &long_content);
        let entry = ArchiveManager::prepare_for_archive(&item, "thread-b", 9_000);

        // Compressed content should be significantly shorter.
        assert!(entry.compressed_content.len() < long_content.len());
        // Summary should be present.
        assert!(entry.summary.is_some());
        // Original token count should reflect the full content.
        assert!(entry.token_count_original > entry.token_count_compressed);
    }
}
