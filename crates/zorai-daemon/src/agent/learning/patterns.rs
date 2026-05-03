//! Pattern recognition — mine success and failure patterns from execution traces.

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Classification of an observed pattern.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PatternType {
    /// Tool sequence that frequently succeeds.
    SuccessSequence,
    /// Tool sequence that frequently fails.
    FailureSequence,
    /// Tool that's commonly used for a task type.
    CommonTool,
}

/// A single mined tool-usage pattern.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolPattern {
    pub id: String,
    pub pattern_type: PatternType,
    pub tool_sequence: Vec<String>,
    pub task_type: String,
    pub occurrences: u32,
    pub success_rate: f64,
    /// Derived confidence based on occurrences and consistency.
    pub confidence: f64,
    pub last_seen_at: u64,
    pub created_at: u64,
}

// ---------------------------------------------------------------------------
// PatternStore
// ---------------------------------------------------------------------------

/// In-memory, serializable store of mined patterns.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatternStore {
    patterns: Vec<ToolPattern>,
    max_patterns: usize,
    decay_days: u32,
}

impl Default for PatternStore {
    fn default() -> Self {
        Self {
            patterns: Vec::new(),
            max_patterns: 200,
            decay_days: 30,
        }
    }
}

/// Seconds per day — used for decay calculations.
const SECONDS_PER_DAY: u64 = 86_400;

impl PatternStore {
    /// Create a new store with explicit capacity and decay window.
    pub fn new(max_patterns: usize, decay_days: u32) -> Self {
        Self {
            patterns: Vec::new(),
            max_patterns,
            decay_days,
        }
    }

    /// Number of patterns currently stored.
    pub fn len(&self) -> usize {
        self.patterns.len()
    }

    /// Whether the store is empty.
    pub fn is_empty(&self) -> bool {
        self.patterns.is_empty()
    }

    /// Record an observed tool sequence.
    ///
    /// If a pattern with the same `tool_sequence` and `task_type` already
    /// exists, its counters are updated in place. Otherwise a new pattern is
    /// created. When the store exceeds `max_patterns` the oldest pattern
    /// (by `last_seen_at`) is evicted.
    pub fn record_sequence(
        &mut self,
        tool_sequence: &[String],
        task_type: &str,
        succeeded: bool,
        now: u64,
    ) {
        if let Some(existing) = self
            .patterns
            .iter_mut()
            .find(|p| p.tool_sequence == tool_sequence && p.task_type == task_type)
        {
            // Update existing pattern.
            let total = existing.occurrences as f64 * existing.success_rate;
            existing.occurrences += 1;
            let new_successes = if succeeded { total + 1.0 } else { total };
            existing.success_rate = new_successes / existing.occurrences as f64;
            existing.confidence = compute_confidence(existing.occurrences, existing.success_rate);
            existing.last_seen_at = now;

            // Re-derive pattern type.
            existing.pattern_type = if existing.success_rate >= 0.5 {
                PatternType::SuccessSequence
            } else {
                PatternType::FailureSequence
            };
        } else {
            // Create a new pattern.
            let success_rate = if succeeded { 1.0 } else { 0.0 };
            let pattern_type = if succeeded {
                PatternType::SuccessSequence
            } else {
                PatternType::FailureSequence
            };

            let pattern = ToolPattern {
                id: format!("pat_{}", uuid::Uuid::new_v4()),
                pattern_type,
                tool_sequence: tool_sequence.to_vec(),
                task_type: task_type.to_string(),
                occurrences: 1,
                success_rate,
                confidence: compute_confidence(1, success_rate),
                last_seen_at: now,
                created_at: now,
            };

            self.patterns.push(pattern);

            // Enforce capacity — evict the oldest pattern when over limit.
            if self.patterns.len() > self.max_patterns {
                if let Some(oldest_idx) = self
                    .patterns
                    .iter()
                    .enumerate()
                    .min_by_key(|(_, p)| p.last_seen_at)
                    .map(|(i, _)| i)
                {
                    self.patterns.remove(oldest_idx);
                }
            }
        }
    }

    /// Find patterns matching a task type and pattern classification, sorted
    /// by confidence descending.
    pub fn find_patterns(&self, task_type: &str, pattern_type: PatternType) -> Vec<&ToolPattern> {
        let mut matches: Vec<&ToolPattern> = self
            .patterns
            .iter()
            .filter(|p| p.task_type == task_type && p.pattern_type == pattern_type)
            .collect();
        matches.sort_by(|a, b| {
            b.confidence
                .partial_cmp(&a.confidence)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        matches
    }

    /// Suggest tools for a task type based on successful patterns.
    ///
    /// Returns a deduplicated list of tool names drawn from
    /// `SuccessSequence` patterns, ordered by pattern confidence.
    pub fn suggest_tools(&self, task_type: &str) -> Vec<String> {
        let success = self.find_patterns(task_type, PatternType::SuccessSequence);

        let mut seen = std::collections::HashSet::new();
        let mut tools = Vec::new();

        for pattern in success {
            for tool in &pattern.tool_sequence {
                if seen.insert(tool.clone()) {
                    tools.push(tool.clone());
                }
            }
        }

        tools
    }

    /// Remove stale patterns and reduce confidence of aging ones.
    ///
    /// Patterns whose `last_seen_at` is older than `decay_days` are removed
    /// entirely. Patterns older than half the decay window have their
    /// confidence scaled down proportionally.
    pub fn decay(&mut self, now: u64) {
        let cutoff = now.saturating_sub(self.decay_days as u64 * SECONDS_PER_DAY);
        let half_cutoff = now.saturating_sub((self.decay_days as u64 * SECONDS_PER_DAY) / 2);

        // Remove expired patterns.
        self.patterns.retain(|p| p.last_seen_at >= cutoff);

        // Reduce confidence of aging patterns (older than half the window).
        for pattern in &mut self.patterns {
            if pattern.last_seen_at < half_cutoff {
                let age_secs = now.saturating_sub(pattern.last_seen_at);
                let window_secs = self.decay_days as u64 * SECONDS_PER_DAY;
                // Linear decay factor: 1.0 at half-window, 0.0 at full window.
                let factor = 1.0 - (age_secs as f64 / window_secs as f64).min(1.0);
                pattern.confidence =
                    compute_confidence(pattern.occurrences, pattern.success_rate) * factor.max(0.0);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Derive a confidence score from occurrence count and success rate.
///
/// `confidence = min(1.0, sqrt(occurrences / 10)) * success_rate`
pub fn compute_confidence(occurrences: u32, success_rate: f64) -> f64 {
    let base = ((occurrences as f64) / 10.0).sqrt().min(1.0);
    base * success_rate
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn seq(names: &[&str]) -> Vec<String> {
        names.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn new_store_is_empty() {
        let store = PatternStore::default();
        assert_eq!(store.len(), 0);
        assert!(store.is_empty());
    }

    #[test]
    fn record_sequence_creates_pattern() {
        let mut store = PatternStore::default();
        store.record_sequence(&seq(&["read", "write"]), "coding", true, 1000);
        assert_eq!(store.len(), 1);
    }

    #[test]
    fn record_same_sequence_increments_occurrences() {
        let mut store = PatternStore::default();
        let tools = seq(&["read", "write"]);
        store.record_sequence(&tools, "coding", true, 1000);
        store.record_sequence(&tools, "coding", true, 2000);
        assert_eq!(store.len(), 1);
        assert_eq!(store.patterns[0].occurrences, 2);
    }

    #[test]
    fn success_rate_updated_on_repeated_recordings() {
        let mut store = PatternStore::default();
        let tools = seq(&["search"]);
        store.record_sequence(&tools, "debug", true, 1000);
        assert!((store.patterns[0].success_rate - 1.0).abs() < f64::EPSILON);

        store.record_sequence(&tools, "debug", false, 2000);
        assert!((store.patterns[0].success_rate - 0.5).abs() < f64::EPSILON);

        store.record_sequence(&tools, "debug", true, 3000);
        // 2 successes out of 3
        let expected = 2.0 / 3.0;
        assert!((store.patterns[0].success_rate - expected).abs() < 1e-9);
    }

    #[test]
    fn find_patterns_filters_by_task_type() {
        let mut store = PatternStore::default();
        store.record_sequence(&seq(&["a"]), "coding", true, 1000);
        store.record_sequence(&seq(&["b"]), "review", true, 1000);

        let coding = store.find_patterns("coding", PatternType::SuccessSequence);
        assert_eq!(coding.len(), 1);
        assert_eq!(coding[0].task_type, "coding");
    }

    #[test]
    fn find_patterns_filters_by_pattern_type() {
        let mut store = PatternStore::default();
        store.record_sequence(&seq(&["a"]), "coding", true, 1000);
        store.record_sequence(&seq(&["b"]), "coding", false, 1000);

        let successes = store.find_patterns("coding", PatternType::SuccessSequence);
        assert_eq!(successes.len(), 1);
        assert_eq!(successes[0].tool_sequence, seq(&["a"]));

        let failures = store.find_patterns("coding", PatternType::FailureSequence);
        assert_eq!(failures.len(), 1);
        assert_eq!(failures[0].tool_sequence, seq(&["b"]));
    }

    #[test]
    fn suggest_tools_returns_from_success_patterns() {
        let mut store = PatternStore::default();
        store.record_sequence(
            &seq(&[zorai_protocol::tool_names::READ_FILE, "edit_file"]),
            "coding",
            true,
            1000,
        );
        store.record_sequence(
            &seq(&["search", zorai_protocol::tool_names::READ_FILE]),
            "coding",
            true,
            2000,
        );

        let suggested = store.suggest_tools("coding");
        // Should contain all three unique tools.
        assert!(suggested.contains(&zorai_protocol::tool_names::READ_FILE.to_string()));
        assert!(suggested.contains(&"edit_file".to_string()));
        assert!(suggested.contains(&"search".to_string()));
        // No duplicates.
        assert_eq!(suggested.len(), 3);
    }

    #[test]
    fn decay_removes_old_patterns() {
        let mut store = PatternStore::new(200, 30);
        let old_time = 0_u64;
        store.record_sequence(&seq(&["a"]), "task", true, old_time);

        let now = 31 * SECONDS_PER_DAY; // 31 days later
        store.decay(now);
        assert_eq!(store.len(), 0);
    }

    #[test]
    fn max_patterns_limit_enforced() {
        let mut store = PatternStore::new(3, 30);
        for i in 0..4 {
            store.record_sequence(&seq(&[&format!("tool_{i}")]), "task", true, i as u64 * 1000);
        }
        assert_eq!(store.len(), 3);
        // The oldest pattern (tool_0 at time 0) should have been evicted.
        assert!(store
            .patterns
            .iter()
            .all(|p| p.tool_sequence != seq(&["tool_0"])));
    }

    #[test]
    fn confidence_increases_with_occurrences() {
        let c1 = compute_confidence(1, 1.0);
        let c5 = compute_confidence(5, 1.0);
        let c10 = compute_confidence(10, 1.0);
        assert!(c1 < c5);
        assert!(c5 < c10);
        assert!((c10 - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn empty_store_suggest_tools_returns_empty() {
        let store = PatternStore::default();
        assert!(store.suggest_tools("anything").is_empty());
    }

    #[test]
    fn decay_reduces_confidence_of_aging_patterns() {
        let mut store = PatternStore::new(200, 30);
        let tools = seq(&["x"]);
        // Record many times to build high confidence.
        for i in 0..10 {
            store.record_sequence(&tools, "task", true, i);
        }
        let original_confidence = store.patterns[0].confidence;
        assert!((original_confidence - 1.0).abs() < f64::EPSILON);

        // Advance to 20 days (past the half-window of 15 days).
        let now = 20 * SECONDS_PER_DAY;
        store.decay(now);
        assert_eq!(store.len(), 1); // still within window
        assert!(store.patterns[0].confidence < original_confidence);
    }

    #[test]
    fn different_task_types_create_separate_patterns() {
        let mut store = PatternStore::default();
        let tools = seq(&["read"]);
        store.record_sequence(&tools, "coding", true, 1000);
        store.record_sequence(&tools, "review", true, 2000);
        assert_eq!(store.len(), 2);
    }
}
