//! Tool effectiveness tracking — per-tool and per-composition success metrics.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Per-tool statistics
// ---------------------------------------------------------------------------

/// Accumulated statistics for a single tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolStats {
    pub tool_name: String,
    pub total_calls: u32,
    pub successful_calls: u32,
    pub failed_calls: u32,
    pub total_duration_ms: u64,
    pub total_tokens: u64,
    pub last_used_at: u64,
}

impl ToolStats {
    fn new(name: &str) -> Self {
        Self {
            tool_name: name.to_string(),
            total_calls: 0,
            successful_calls: 0,
            failed_calls: 0,
            total_duration_ms: 0,
            total_tokens: 0,
            last_used_at: 0,
        }
    }
}

// ---------------------------------------------------------------------------
// Composition statistics
// ---------------------------------------------------------------------------

/// Statistics for a specific sequence of tools used together.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompositionStats {
    pub tool_sequence: Vec<String>,
    pub total_uses: u32,
    pub completions: u32,
    pub avg_steps_to_success: f64,
    pub last_used_at: u64,
}

// ---------------------------------------------------------------------------
// Effectiveness tracker
// ---------------------------------------------------------------------------

/// Tracks per-tool call outcomes and multi-tool composition success rates.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EffectivenessTracker {
    tools: HashMap<String, ToolStats>,
    compositions: Vec<CompositionStats>,
    max_compositions: usize,
}

impl Default for EffectivenessTracker {
    fn default() -> Self {
        Self::new(100)
    }
}

impl EffectivenessTracker {
    /// Create a new tracker with the given composition history limit.
    pub fn new(max_compositions: usize) -> Self {
        Self {
            tools: HashMap::new(),
            compositions: Vec::new(),
            max_compositions,
        }
    }

    /// Record the outcome of a single tool invocation.
    pub fn record_tool_call(
        &mut self,
        tool_name: &str,
        succeeded: bool,
        duration_ms: u64,
        tokens: u64,
        now: u64,
    ) {
        let stats = self
            .tools
            .entry(tool_name.to_string())
            .or_insert_with(|| ToolStats::new(tool_name));

        stats.total_calls += 1;
        if succeeded {
            stats.successful_calls += 1;
        } else {
            stats.failed_calls += 1;
        }
        stats.total_duration_ms += duration_ms;
        stats.total_tokens += tokens;
        stats.last_used_at = now;
    }

    /// Record the outcome of a multi-tool composition (sequence).
    pub fn record_composition(
        &mut self,
        sequence: &[String],
        completed: bool,
        steps: u32,
        now: u64,
    ) {
        // Look for an existing entry with the same sequence.
        if let Some(cs) = self
            .compositions
            .iter_mut()
            .find(|c| c.tool_sequence == sequence)
        {
            cs.total_uses += 1;
            if completed {
                // Running average of steps-to-success.
                let prev_completions = cs.completions as f64;
                cs.completions += 1;
                cs.avg_steps_to_success = (cs.avg_steps_to_success * prev_completions
                    + steps as f64)
                    / cs.completions as f64;
            }
            cs.last_used_at = now;
        } else {
            // Evict the oldest composition if we are at capacity.
            if self.compositions.len() >= self.max_compositions {
                // Remove the entry with the smallest `last_used_at`.
                if let Some(idx) = self
                    .compositions
                    .iter()
                    .enumerate()
                    .min_by_key(|(_, c)| c.last_used_at)
                    .map(|(i, _)| i)
                {
                    self.compositions.swap_remove(idx);
                }
            }

            let avg = if completed { steps as f64 } else { 0.0 };
            self.compositions.push(CompositionStats {
                tool_sequence: sequence.to_vec(),
                total_uses: 1,
                completions: u32::from(completed),
                avg_steps_to_success: avg,
                last_used_at: now,
            });
        }
    }

    // -----------------------------------------------------------------------
    // Per-tool queries
    // -----------------------------------------------------------------------

    /// Success rate for a given tool (0.0–1.0), or `None` if unknown.
    pub fn tool_success_rate(&self, tool_name: &str) -> Option<f64> {
        self.tools.get(tool_name).map(|s| {
            if s.total_calls == 0 {
                0.0
            } else {
                s.successful_calls as f64 / s.total_calls as f64
            }
        })
    }

    /// Average call duration in milliseconds, or `None` if unknown.
    pub fn tool_avg_duration(&self, tool_name: &str) -> Option<f64> {
        self.tools.get(tool_name).map(|s| {
            if s.total_calls == 0 {
                0.0
            } else {
                s.total_duration_ms as f64 / s.total_calls as f64
            }
        })
    }

    /// Average token usage per call, or `None` if unknown.
    pub fn tool_avg_tokens(&self, tool_name: &str) -> Option<f64> {
        self.tools.get(tool_name).map(|s| {
            if s.total_calls == 0 {
                0.0
            } else {
                s.total_tokens as f64 / s.total_calls as f64
            }
        })
    }

    // -----------------------------------------------------------------------
    // Ranking helpers
    // -----------------------------------------------------------------------

    /// Tools with the highest success rate. Only tools with at least 5 calls
    /// are considered. Returns up to `top_n` entries as `(tool_name, rate)`.
    pub fn most_effective_tools(&self, top_n: usize) -> Vec<(&str, f64)> {
        let mut ranked: Vec<(&str, f64)> = self
            .tools
            .values()
            .filter(|s| s.total_calls >= 5)
            .map(|s| {
                (
                    s.tool_name.as_str(),
                    s.successful_calls as f64 / s.total_calls as f64,
                )
            })
            .collect();
        ranked.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        ranked.truncate(top_n);
        ranked
    }

    /// Tools with the lowest success rate. Only tools with at least 5 calls
    /// are considered. Returns up to `top_n` entries as `(tool_name, rate)`.
    pub fn least_effective_tools(&self, top_n: usize) -> Vec<(&str, f64)> {
        let mut ranked: Vec<(&str, f64)> = self
            .tools
            .values()
            .filter(|s| s.total_calls >= 5)
            .map(|s| {
                (
                    s.tool_name.as_str(),
                    s.successful_calls as f64 / s.total_calls as f64,
                )
            })
            .collect();
        ranked.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
        ranked.truncate(top_n);
        ranked
    }

    // -----------------------------------------------------------------------
    // Composition queries
    // -----------------------------------------------------------------------

    /// Completion rate for a known tool-sequence, or `None` if unrecorded.
    pub fn composition_completion_rate(&self, sequence: &[String]) -> Option<f64> {
        self.compositions
            .iter()
            .find(|c| c.tool_sequence == sequence)
            .map(|c| {
                if c.total_uses == 0 {
                    0.0
                } else {
                    c.completions as f64 / c.total_uses as f64
                }
            })
    }

    // -----------------------------------------------------------------------
    // Reporting
    // -----------------------------------------------------------------------

    /// Build a human-readable effectiveness report.
    pub fn build_effectiveness_report(&self) -> String {
        let mut lines = Vec::new();
        lines.push("=== Tool Effectiveness Report ===".to_string());
        lines.push(String::new());

        // Per-tool summary (sorted by name for determinism).
        let mut tool_names: Vec<&String> = self.tools.keys().collect();
        tool_names.sort();

        if tool_names.is_empty() {
            lines.push("No tool usage recorded.".to_string());
        } else {
            lines.push(format!("Tracked tools: {}", tool_names.len()));
            lines.push(String::new());

            for name in &tool_names {
                let s = &self.tools[*name];
                let rate = if s.total_calls > 0 {
                    s.successful_calls as f64 / s.total_calls as f64 * 100.0
                } else {
                    0.0
                };
                lines.push(format!(
                    "  {}: {} calls, {:.1}% success, avg {:.0} ms, avg {:.0} tokens",
                    s.tool_name,
                    s.total_calls,
                    rate,
                    if s.total_calls > 0 {
                        s.total_duration_ms as f64 / s.total_calls as f64
                    } else {
                        0.0
                    },
                    if s.total_calls > 0 {
                        s.total_tokens as f64 / s.total_calls as f64
                    } else {
                        0.0
                    },
                ));
            }
        }

        // Top / bottom rankings.
        let top = self.most_effective_tools(5);
        if !top.is_empty() {
            lines.push(String::new());
            lines.push("Most effective (>= 5 calls):".to_string());
            for (name, rate) in &top {
                lines.push(format!("  {}: {:.1}%", name, rate * 100.0));
            }
        }

        let bottom = self.least_effective_tools(5);
        if !bottom.is_empty() {
            lines.push(String::new());
            lines.push("Least effective (>= 5 calls):".to_string());
            for (name, rate) in &bottom {
                lines.push(format!("  {}: {:.1}%", name, rate * 100.0));
            }
        }

        // Compositions summary.
        if !self.compositions.is_empty() {
            lines.push(String::new());
            lines.push(format!("Tracked compositions: {}", self.compositions.len()));
            for cs in &self.compositions {
                let rate = if cs.total_uses > 0 {
                    cs.completions as f64 / cs.total_uses as f64 * 100.0
                } else {
                    0.0
                };
                lines.push(format!(
                    "  [{}]: {} uses, {:.1}% completion",
                    cs.tool_sequence.join(" -> "),
                    cs.total_uses,
                    rate,
                ));
            }
        }

        lines.join("\n")
    }
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_tracker_is_empty() {
        let tracker = EffectivenessTracker::default();
        assert!(tracker.tools.is_empty());
        assert!(tracker.compositions.is_empty());
        assert_eq!(tracker.max_compositions, 100);
    }

    #[test]
    fn record_tool_call_creates_stats() {
        let mut tracker = EffectivenessTracker::default();
        tracker.record_tool_call("grep", true, 50, 100, 1000);

        assert!(tracker.tools.contains_key("grep"));
        let s = &tracker.tools["grep"];
        assert_eq!(s.total_calls, 1);
        assert_eq!(s.successful_calls, 1);
        assert_eq!(s.failed_calls, 0);
        assert_eq!(s.total_duration_ms, 50);
        assert_eq!(s.total_tokens, 100);
        assert_eq!(s.last_used_at, 1000);
    }

    #[test]
    fn multiple_calls_update_stats_correctly() {
        let mut tracker = EffectivenessTracker::default();
        tracker.record_tool_call("read", true, 10, 50, 100);
        tracker.record_tool_call("read", false, 20, 60, 200);
        tracker.record_tool_call("read", true, 30, 70, 300);

        let s = &tracker.tools["read"];
        assert_eq!(s.total_calls, 3);
        assert_eq!(s.successful_calls, 2);
        assert_eq!(s.failed_calls, 1);
        assert_eq!(s.total_duration_ms, 60);
        assert_eq!(s.total_tokens, 180);
        assert_eq!(s.last_used_at, 300);
    }

    #[test]
    fn success_rate_calculation() {
        let mut tracker = EffectivenessTracker::default();
        tracker.record_tool_call("edit", true, 10, 50, 1);
        tracker.record_tool_call("edit", true, 10, 50, 2);
        tracker.record_tool_call("edit", false, 10, 50, 3);
        tracker.record_tool_call("edit", true, 10, 50, 4);

        let rate = tracker.tool_success_rate("edit").unwrap();
        assert!((rate - 0.75).abs() < f64::EPSILON);

        // Unknown tool returns None.
        assert!(tracker.tool_success_rate("unknown").is_none());
    }

    #[test]
    fn avg_duration_calculation() {
        let mut tracker = EffectivenessTracker::default();
        tracker.record_tool_call("bash", true, 100, 0, 1);
        tracker.record_tool_call("bash", true, 200, 0, 2);

        let avg = tracker.tool_avg_duration("bash").unwrap();
        assert!((avg - 150.0).abs() < f64::EPSILON);
    }

    #[test]
    fn avg_tokens_calculation() {
        let mut tracker = EffectivenessTracker::default();
        tracker.record_tool_call("write", true, 0, 300, 1);
        tracker.record_tool_call("write", true, 0, 500, 2);

        let avg = tracker.tool_avg_tokens("write").unwrap();
        assert!((avg - 400.0).abs() < f64::EPSILON);
    }

    #[test]
    fn most_effective_tools_sorted_correctly() {
        let mut tracker = EffectivenessTracker::default();

        // "good" tool: 9/10 = 90%
        for i in 0..10 {
            tracker.record_tool_call("good", i < 9, 10, 10, i as u64);
        }
        // "ok" tool: 6/10 = 60%
        for i in 0..10 {
            tracker.record_tool_call("ok", i < 6, 10, 10, i as u64);
        }
        // "great" tool: 10/10 = 100%
        for i in 0..10 {
            tracker.record_tool_call("great", true, 10, 10, i as u64);
        }

        let top = tracker.most_effective_tools(3);
        assert_eq!(top.len(), 3);
        assert_eq!(top[0].0, "great");
        assert!((top[0].1 - 1.0).abs() < f64::EPSILON);
        assert_eq!(top[1].0, "good");
        assert!((top[1].1 - 0.9).abs() < f64::EPSILON);
        assert_eq!(top[2].0, "ok");
        assert!((top[2].1 - 0.6).abs() < f64::EPSILON);
    }

    #[test]
    fn least_effective_tools_sorted_correctly() {
        let mut tracker = EffectivenessTracker::default();

        for i in 0..10 {
            tracker.record_tool_call("bad", i < 2, 10, 10, i as u64);
        }
        for i in 0..10 {
            tracker.record_tool_call("worse", i < 1, 10, 10, i as u64);
        }
        for i in 0..10 {
            tracker.record_tool_call("decent", i < 7, 10, 10, i as u64);
        }

        let bottom = tracker.least_effective_tools(2);
        assert_eq!(bottom.len(), 2);
        assert_eq!(bottom[0].0, "worse");
        assert!((bottom[0].1 - 0.1).abs() < f64::EPSILON);
        assert_eq!(bottom[1].0, "bad");
        assert!((bottom[1].1 - 0.2).abs() < f64::EPSILON);
    }

    #[test]
    fn tools_with_fewer_than_5_calls_excluded_from_rankings() {
        let mut tracker = EffectivenessTracker::default();

        // 4 calls — should be excluded.
        for _ in 0..4 {
            tracker.record_tool_call("few", true, 10, 10, 1);
        }
        // 5 calls — should be included.
        for _ in 0..5 {
            tracker.record_tool_call("enough", true, 10, 10, 1);
        }

        let top = tracker.most_effective_tools(10);
        assert_eq!(top.len(), 1);
        assert_eq!(top[0].0, "enough");

        let bottom = tracker.least_effective_tools(10);
        assert_eq!(bottom.len(), 1);
        assert_eq!(bottom[0].0, "enough");
    }

    #[test]
    fn composition_recording() {
        let mut tracker = EffectivenessTracker::default();
        let seq = vec!["read".to_string(), "edit".to_string(), "bash".to_string()];

        tracker.record_composition(&seq, true, 3, 100);
        tracker.record_composition(&seq, false, 0, 200);
        tracker.record_composition(&seq, true, 5, 300);

        assert_eq!(tracker.compositions.len(), 1);
        let cs = &tracker.compositions[0];
        assert_eq!(cs.total_uses, 3);
        assert_eq!(cs.completions, 2);
        // avg_steps_to_success = (3 + 5) / 2 = 4.0
        assert!((cs.avg_steps_to_success - 4.0).abs() < f64::EPSILON);
        assert_eq!(cs.last_used_at, 300);
    }

    #[test]
    fn composition_completion_rate() {
        let mut tracker = EffectivenessTracker::default();
        let seq = vec!["a".to_string(), "b".to_string()];

        tracker.record_composition(&seq, true, 2, 1);
        tracker.record_composition(&seq, false, 0, 2);
        tracker.record_composition(&seq, true, 3, 3);
        tracker.record_composition(&seq, false, 0, 4);

        let rate = tracker.composition_completion_rate(&seq).unwrap();
        assert!((rate - 0.5).abs() < f64::EPSILON);

        // Unknown sequence returns None.
        let unknown = vec!["x".to_string()];
        assert!(tracker.composition_completion_rate(&unknown).is_none());
    }

    #[test]
    fn effectiveness_report_includes_tool_names() {
        let mut tracker = EffectivenessTracker::default();
        tracker.record_tool_call("grep", true, 10, 100, 1);
        tracker.record_tool_call("read", false, 20, 200, 2);

        let report = tracker.build_effectiveness_report();
        assert!(report.contains("grep"));
        assert!(report.contains("read"));
        assert!(report.contains("Tool Effectiveness Report"));
    }

    #[test]
    fn composition_eviction_when_at_capacity() {
        let mut tracker = EffectivenessTracker::new(2);

        let seq1 = vec!["a".to_string()];
        let seq2 = vec!["b".to_string()];
        let seq3 = vec!["c".to_string()];

        tracker.record_composition(&seq1, true, 1, 10);
        tracker.record_composition(&seq2, true, 1, 20);
        // This should evict seq1 (oldest by last_used_at).
        tracker.record_composition(&seq3, true, 1, 30);

        assert_eq!(tracker.compositions.len(), 2);
        assert!(tracker.composition_completion_rate(&seq1).is_none());
        assert!(tracker.composition_completion_rate(&seq2).is_some());
        assert!(tracker.composition_completion_rate(&seq3).is_some());
    }
}
