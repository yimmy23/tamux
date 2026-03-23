//! Skill discovery — pure functions for evaluating execution traces as skill candidates.
//!
//! This module provides the core logic for deciding whether a completed execution
//! trace qualifies as a skill-drafting candidate based on complexity, quality, and
//! novelty relative to known patterns.

use std::collections::HashSet;

use super::learning::patterns::ToolPattern;
use super::types::SkillDiscoveryConfig;

// ---------------------------------------------------------------------------
// Complexity threshold
// ---------------------------------------------------------------------------

/// Determine whether an execution trace meets the complexity threshold for
/// skill-drafting candidacy.
///
/// Returns `true` when:
/// - `outcome` is `"success"`, AND
/// - `tool_count` exceeds `config.min_tool_count`, AND
/// - at least one of: `replan_count >= config.min_replan_count` OR
///   `quality_score > config.min_quality_score`
pub(super) fn meets_complexity_threshold(
    tool_count: usize,
    replan_count: u32,
    quality_score: Option<f64>,
    outcome: &str,
    config: &SkillDiscoveryConfig,
) -> bool {
    if outcome != "success" {
        return false;
    }
    let tool_gate = tool_count > config.min_tool_count;
    let replan_gate = replan_count >= config.min_replan_count;
    let quality_gate = quality_score.map_or(false, |q| q > config.min_quality_score);
    tool_gate && (replan_gate || quality_gate)
}

// ---------------------------------------------------------------------------
// Jaccard similarity
// ---------------------------------------------------------------------------

/// Compute the Jaccard similarity coefficient between two string slices.
///
/// Returns 1.0 when both slices are empty (two empty sets are identical),
/// 0.0 when the intersection is empty, or |A intersect B| / |A union B|.
pub(super) fn jaccard_similarity(a: &[String], b: &[String]) -> f64 {
    if a.is_empty() && b.is_empty() {
        return 1.0;
    }
    let set_a: HashSet<&str> = a.iter().map(|s| s.as_str()).collect();
    let set_b: HashSet<&str> = b.iter().map(|s| s.as_str()).collect();
    let intersection = set_a.intersection(&set_b).count();
    let union = set_a.union(&set_b).count();
    if union == 0 {
        return 1.0;
    }
    intersection as f64 / union as f64
}

// ---------------------------------------------------------------------------
// Novelty detection
// ---------------------------------------------------------------------------

/// Determine whether a tool sequence is novel relative to known patterns.
///
/// Takes pre-fetched patterns (not the PatternStore directly) for testability.
/// Returns `true` when no existing pattern has a Jaccard similarity >=
/// `similarity_threshold` with the candidate sequence.
pub(super) fn is_novel_sequence(
    tool_sequence: &[String],
    _task_type: &str,
    patterns: &[&ToolPattern],
    similarity_threshold: f64,
) -> bool {
    for pattern in patterns {
        let sim = jaccard_similarity(tool_sequence, &pattern.tool_sequence);
        if sim >= similarity_threshold {
            return false;
        }
    }
    true
}

// ---------------------------------------------------------------------------
// JSON extraction
// ---------------------------------------------------------------------------

/// Parse a JSON array of strings into a `Vec<String>`.
///
/// Returns an empty vec on `None` or parse failure.
pub(super) fn extract_tool_sequence_from_json(json: Option<&str>) -> Vec<String> {
    json.and_then(|s| serde_json::from_str::<Vec<String>>(s).ok())
        .unwrap_or_default()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::learning::patterns::{PatternType, ToolPattern};

    fn default_config() -> SkillDiscoveryConfig {
        SkillDiscoveryConfig::default()
    }

    fn make_pattern(tools: &[&str]) -> ToolPattern {
        ToolPattern {
            id: "test-pattern".to_string(),
            pattern_type: PatternType::SuccessSequence,
            tool_sequence: tools.iter().map(|s| s.to_string()).collect(),
            task_type: "coding".to_string(),
            occurrences: 5,
            success_rate: 0.9,
            confidence: 0.8,
            last_seen_at: 1000,
            created_at: 500,
        }
    }

    fn seq(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    // -----------------------------------------------------------------------
    // meets_complexity_threshold
    // -----------------------------------------------------------------------

    #[test]
    fn skill_discovery_complexity_returns_false_when_outcome_not_success() {
        let cfg = default_config();
        assert!(!meets_complexity_threshold(20, 2, Some(0.95), "failure", &cfg));
    }

    #[test]
    fn skill_discovery_complexity_returns_false_when_tool_count_at_threshold() {
        let cfg = default_config();
        // tool_count == min_tool_count (8), not >, so false
        assert!(!meets_complexity_threshold(8, 2, Some(0.95), "success", &cfg));
    }

    #[test]
    fn skill_discovery_complexity_returns_true_with_replan() {
        let cfg = default_config();
        // tool_count > 8, replan_count >= 1, outcome success
        assert!(meets_complexity_threshold(10, 1, None, "success", &cfg));
    }

    #[test]
    fn skill_discovery_complexity_returns_true_with_quality() {
        let cfg = default_config();
        // tool_count > 8, replan_count=0, quality > 0.8, outcome success
        assert!(meets_complexity_threshold(10, 0, Some(0.85), "success", &cfg));
    }

    #[test]
    fn skill_discovery_complexity_returns_false_no_replan_no_quality() {
        let cfg = default_config();
        // tool_count > 8, replan_count=0, quality <= 0.8
        assert!(!meets_complexity_threshold(10, 0, Some(0.8), "success", &cfg));
        assert!(!meets_complexity_threshold(10, 0, None, "success", &cfg));
    }

    // -----------------------------------------------------------------------
    // jaccard_similarity
    // -----------------------------------------------------------------------

    #[test]
    fn skill_discovery_jaccard_identical_sets() {
        let a = seq(&["A", "B", "C"]);
        let b = seq(&["A", "B", "C"]);
        assert!((jaccard_similarity(&a, &b) - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn skill_discovery_jaccard_disjoint_sets() {
        let a = seq(&["A", "B"]);
        let b = seq(&["C", "D"]);
        assert!((jaccard_similarity(&a, &b) - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn skill_discovery_jaccard_partial_overlap() {
        let a = seq(&["A", "B", "C"]);
        let b = seq(&["B", "C", "D"]);
        // intersection={B,C}=2, union={A,B,C,D}=4 => 0.5
        assert!((jaccard_similarity(&a, &b) - 0.5).abs() < f64::EPSILON);
    }

    #[test]
    fn skill_discovery_jaccard_empty_sets() {
        let a: Vec<String> = vec![];
        let b: Vec<String> = vec![];
        assert!((jaccard_similarity(&a, &b) - 1.0).abs() < f64::EPSILON);
    }

    // -----------------------------------------------------------------------
    // is_novel_sequence
    // -----------------------------------------------------------------------

    #[test]
    fn skill_discovery_novel_when_no_patterns_match() {
        let candidate = seq(&["X", "Y", "Z"]);
        let pattern = make_pattern(&["A", "B", "C"]);
        let patterns = vec![&pattern];
        assert!(is_novel_sequence(&candidate, "coding", &patterns, 0.7));
    }

    #[test]
    fn skill_discovery_not_novel_when_pattern_similar() {
        let candidate = seq(&["A", "B", "C"]);
        let pattern = make_pattern(&["A", "B", "C"]);
        let patterns = vec![&pattern];
        // similarity=1.0 >= 0.7 threshold
        assert!(!is_novel_sequence(&candidate, "coding", &patterns, 0.7));
    }

    // -----------------------------------------------------------------------
    // extract_tool_sequence_from_json
    // -----------------------------------------------------------------------

    #[test]
    fn skill_discovery_extract_tool_sequence_valid_json() {
        let json = r#"["file_read", "terminal_exec", "file_write"]"#;
        let result = extract_tool_sequence_from_json(Some(json));
        assert_eq!(result, vec!["file_read", "terminal_exec", "file_write"]);
    }

    #[test]
    fn skill_discovery_extract_tool_sequence_none() {
        let result = extract_tool_sequence_from_json(None);
        assert!(result.is_empty());
    }

    #[test]
    fn skill_discovery_extract_tool_sequence_invalid_json() {
        let result = extract_tool_sequence_from_json(Some("not json"));
        assert!(result.is_empty());
    }
}
