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
#[path = "effectiveness/tests.rs"]
mod tests;
