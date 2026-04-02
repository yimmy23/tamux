//! Heuristic learning — optimal configurations learned from execution patterns.

use serde::{Deserialize, Serialize};

/// Minimum number of samples required before a heuristic is considered reliable.
const MIN_SAMPLES: u32 = 5;

/// Learned context-window allocation for a given task type.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextHeuristic {
    pub task_type: String,
    pub optimal_tokens: u32,
    pub sample_count: u32,
    pub avg_actual_tokens: u32,
    pub success_rate_at_optimal: f64,
}

/// Learned effectiveness score for a particular tool within a task type.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolHeuristic {
    pub tool_name: String,
    pub task_type: String,
    pub effectiveness_score: f64, // 0.0-1.0
    pub avg_duration_ms: u64,
    pub usage_count: u32,
}

/// Learned replan strategy for a particular stuck reason.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplanHeuristic {
    pub stuck_reason: String,
    pub strategy: String,
    pub success_rate: f64,
    pub sample_count: u32,
}

/// Aggregated store of all learned heuristics.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HeuristicStore {
    pub context_heuristics: Vec<ContextHeuristic>,
    pub tool_heuristics: Vec<ToolHeuristic>,
    pub replan_heuristics: Vec<ReplanHeuristic>,
}

impl HeuristicStore {
    /// Update running average for context allocation after a task execution.
    ///
    /// Maintains a running average of actual token usage and tracks the success
    /// rate at the currently recorded optimal level.
    pub fn update_context(&mut self, task_type: &str, tokens_used: u32, succeeded: bool) {
        if let Some(h) = self
            .context_heuristics
            .iter_mut()
            .find(|h| h.task_type == task_type)
        {
            let old_count = h.sample_count;
            h.sample_count += 1;
            let new_count = h.sample_count;

            // Running average of actual token usage.
            h.avg_actual_tokens = ((h.avg_actual_tokens as u64 * old_count as u64
                + tokens_used as u64)
                / new_count as u64) as u32;

            // Running average of success rate.
            let old_successes = (h.success_rate_at_optimal * old_count as f64).round() as u32;
            let new_successes = old_successes + u32::from(succeeded);
            h.success_rate_at_optimal = new_successes as f64 / new_count as f64;

            // Optimal tokens tracks the average actual usage (best predictor of need).
            h.optimal_tokens = h.avg_actual_tokens;
        } else {
            self.context_heuristics.push(ContextHeuristic {
                task_type: task_type.to_string(),
                optimal_tokens: tokens_used,
                sample_count: 1,
                avg_actual_tokens: tokens_used,
                success_rate_at_optimal: if succeeded { 1.0 } else { 0.0 },
            });
        }
    }

    /// Update tool effectiveness for a given tool within a task type.
    pub fn update_tool(
        &mut self,
        tool_name: &str,
        task_type: &str,
        succeeded: bool,
        duration_ms: u64,
    ) {
        if let Some(h) = self
            .tool_heuristics
            .iter_mut()
            .find(|h| h.tool_name == tool_name && h.task_type == task_type)
        {
            let old_count = h.usage_count;
            h.usage_count += 1;
            let new_count = h.usage_count;

            // Running average of duration.
            h.avg_duration_ms =
                (h.avg_duration_ms * old_count as u64 + duration_ms) / new_count as u64;

            // Running average of effectiveness (success rate).
            let old_successes = (h.effectiveness_score * old_count as f64).round() as u32;
            let new_successes = old_successes + u32::from(succeeded);
            h.effectiveness_score = new_successes as f64 / new_count as f64;
        } else {
            self.tool_heuristics.push(ToolHeuristic {
                tool_name: tool_name.to_string(),
                task_type: task_type.to_string(),
                effectiveness_score: if succeeded { 1.0 } else { 0.0 },
                avg_duration_ms: duration_ms,
                usage_count: 1,
            });
        }
    }

    /// Update replan strategy effectiveness for a given stuck reason.
    pub fn update_replan(&mut self, stuck_reason: &str, strategy: &str, succeeded: bool) {
        if let Some(h) = self
            .replan_heuristics
            .iter_mut()
            .find(|h| h.stuck_reason == stuck_reason && h.strategy == strategy)
        {
            let old_count = h.sample_count;
            h.sample_count += 1;
            let new_count = h.sample_count;

            let old_successes = (h.success_rate * old_count as f64).round() as u32;
            let new_successes = old_successes + u32::from(succeeded);
            h.success_rate = new_successes as f64 / new_count as f64;
        } else {
            self.replan_heuristics.push(ReplanHeuristic {
                stuck_reason: stuck_reason.to_string(),
                strategy: strategy.to_string(),
                success_rate: if succeeded { 1.0 } else { 0.0 },
                sample_count: 1,
            });
        }
    }

    /// Returns the optimal token count for a task type if enough samples have
    /// been collected (at least [`MIN_SAMPLES`]).
    pub fn suggest_context_budget(&self, task_type: &str) -> Option<u32> {
        self.context_heuristics
            .iter()
            .find(|h| h.task_type == task_type && h.sample_count >= MIN_SAMPLES)
            .map(|h| h.optimal_tokens)
    }

    /// Returns the top-N most effective tool names for a given task type,
    /// sorted by effectiveness score descending.
    pub fn suggest_tools(&self, task_type: &str, top_n: usize) -> Vec<String> {
        let mut matching: Vec<&ToolHeuristic> = self
            .tool_heuristics
            .iter()
            .filter(|h| h.task_type == task_type)
            .collect();

        matching.sort_by(|a, b| {
            b.effectiveness_score
                .partial_cmp(&a.effectiveness_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        matching
            .into_iter()
            .take(top_n)
            .map(|h| h.tool_name.clone())
            .collect()
    }

    /// Returns the best replan strategy for a given stuck reason, choosing the
    /// strategy with the highest success rate (minimum [`MIN_SAMPLES`] required).
    pub fn suggest_replan_strategy(&self, stuck_reason: &str) -> Option<String> {
        self.replan_heuristics
            .iter()
            .filter(|h| h.stuck_reason == stuck_reason && h.sample_count >= MIN_SAMPLES)
            .max_by(|a, b| {
                a.success_rate
                    .partial_cmp(&b.success_rate)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|h| h.strategy.clone())
    }

    /// Generates system-prompt hints based on the learned heuristics for the
    /// given task type. Returns an empty string if no heuristics are available.
    pub fn build_system_prompt_hints(&self, task_type: &str) -> String {
        let mut hints = String::new();

        // Context budget hint.
        if let Some(budget) = self.suggest_context_budget(task_type) {
            hints.push_str(&format!(
                "- Suggested context budget for '{}': {} tokens\n",
                task_type, budget
            ));
        }

        // Tool recommendations.
        let tools = self.suggest_tools(task_type, 3);
        if !tools.is_empty() {
            hints.push_str(&format!(
                "- Recommended tools for '{}': {}\n",
                task_type,
                tools.join(", ")
            ));
        }

        // Tool-specific effectiveness details.
        for th in self
            .tool_heuristics
            .iter()
            .filter(|h| h.task_type == task_type)
        {
            hints.push_str(&format!(
                "- Tool '{}' effectiveness: {:.0}% (avg {}ms, {} uses)\n",
                th.tool_name,
                th.effectiveness_score * 100.0,
                th.avg_duration_ms,
                th.usage_count,
            ));
        }

        hints
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_store_is_empty() {
        let store = HeuristicStore::default();
        assert!(store.context_heuristics.is_empty());
        assert!(store.tool_heuristics.is_empty());
        assert!(store.replan_heuristics.is_empty());
    }

    #[test]
    fn update_context_tracks_running_average() {
        let mut store = HeuristicStore::default();
        store.update_context("coding", 1000, true);
        store.update_context("coding", 2000, true);
        store.update_context("coding", 3000, true);

        let h = &store.context_heuristics[0];
        assert_eq!(h.sample_count, 3);
        assert_eq!(h.avg_actual_tokens, 2000);
        assert_eq!(h.optimal_tokens, 2000);
        assert!((h.success_rate_at_optimal - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn update_context_tracks_success_rate() {
        let mut store = HeuristicStore::default();
        store.update_context("coding", 1000, true);
        store.update_context("coding", 1000, false);

        let h = &store.context_heuristics[0];
        assert_eq!(h.sample_count, 2);
        assert!((h.success_rate_at_optimal - 0.5).abs() < f64::EPSILON);
    }

    #[test]
    fn update_tool_tracks_effectiveness() {
        let mut store = HeuristicStore::default();
        store.update_tool("grep", "coding", true, 100);
        store.update_tool("grep", "coding", true, 200);
        store.update_tool("grep", "coding", false, 300);

        let h = &store.tool_heuristics[0];
        assert_eq!(h.usage_count, 3);
        assert_eq!(h.avg_duration_ms, 200);
        // 2 successes out of 3 => ~0.6667
        assert!((h.effectiveness_score - 2.0 / 3.0).abs() < 0.01);
    }

    #[test]
    fn update_replan_tracks_strategy_success() {
        let mut store = HeuristicStore::default();
        store.update_replan("loop_detected", "backtrack", true);
        store.update_replan("loop_detected", "backtrack", false);
        store.update_replan("loop_detected", "backtrack", true);

        let h = &store.replan_heuristics[0];
        assert_eq!(h.sample_count, 3);
        // 2 out of 3
        assert!((h.success_rate - 2.0 / 3.0).abs() < 0.01);
    }

    #[test]
    fn suggest_context_budget_none_without_enough_samples() {
        let mut store = HeuristicStore::default();
        // Only 3 samples — below MIN_SAMPLES (5).
        for _ in 0..3 {
            store.update_context("coding", 1000, true);
        }
        assert!(store.suggest_context_budget("coding").is_none());
    }

    #[test]
    fn suggest_context_budget_returns_value_with_enough_samples() {
        let mut store = HeuristicStore::default();
        for _ in 0..5 {
            store.update_context("coding", 1500, true);
        }
        assert_eq!(store.suggest_context_budget("coding"), Some(1500));
    }

    #[test]
    fn suggest_tools_returns_sorted_by_effectiveness() {
        let mut store = HeuristicStore::default();
        // Tool A: 50% effective
        store.update_tool("tool_a", "coding", true, 100);
        store.update_tool("tool_a", "coding", false, 100);
        // Tool B: 100% effective
        store.update_tool("tool_b", "coding", true, 100);
        store.update_tool("tool_b", "coding", true, 100);
        // Tool C: 0% effective
        store.update_tool("tool_c", "coding", false, 100);
        store.update_tool("tool_c", "coding", false, 100);

        let suggestions = store.suggest_tools("coding", 3);
        assert_eq!(suggestions, vec!["tool_b", "tool_a", "tool_c"]);
    }

    #[test]
    fn suggest_replan_strategy_picks_highest_success_rate() {
        let mut store = HeuristicStore::default();
        // Strategy A: 40% success (2 out of 5)
        for i in 0..5 {
            store.update_replan("stuck", "strategy_a", i < 2);
        }
        // Strategy B: 80% success (4 out of 5)
        for i in 0..5 {
            store.update_replan("stuck", "strategy_b", i < 4);
        }

        let best = store.suggest_replan_strategy("stuck");
        assert_eq!(best, Some("strategy_b".to_string()));
    }

    #[test]
    fn build_system_prompt_hints_includes_relevant_info() {
        let mut store = HeuristicStore::default();

        // Add enough context samples for budget suggestion.
        for _ in 0..6 {
            store.update_context("coding", 2000, true);
        }
        // Add tool data.
        store.update_tool("grep", "coding", true, 50);

        let hints = store.build_system_prompt_hints("coding");

        assert!(hints.contains("Suggested context budget"));
        assert!(hints.contains("2000"));
        assert!(hints.contains("grep"));
        assert!(hints.contains("Recommended tools"));
    }

    #[test]
    fn multiple_task_types_tracked_independently() {
        let mut store = HeuristicStore::default();

        for _ in 0..6 {
            store.update_context("coding", 2000, true);
            store.update_context("review", 500, true);
        }

        store.update_tool("grep", "coding", true, 100);
        store.update_tool("diff", "review", true, 50);

        assert_eq!(store.suggest_context_budget("coding"), Some(2000));
        assert_eq!(store.suggest_context_budget("review"), Some(500));

        let coding_tools = store.suggest_tools("coding", 5);
        assert_eq!(coding_tools, vec!["grep"]);
        assert!(!coding_tools.contains(&"diff".to_string()));

        let review_tools = store.suggest_tools("review", 5);
        assert_eq!(review_tools, vec!["diff"]);
        assert!(!review_tools.contains(&"grep".to_string()));
    }

    #[test]
    fn build_system_prompt_hints_empty_for_unknown_task() {
        let store = HeuristicStore::default();
        let hints = store.build_system_prompt_hints("unknown_task");
        assert!(hints.is_empty());
    }

    #[test]
    fn suggest_replan_strategy_none_without_enough_samples() {
        let mut store = HeuristicStore::default();
        store.update_replan("stuck", "retry", true);
        // Only 1 sample — below MIN_SAMPLES.
        assert!(store.suggest_replan_strategy("stuck").is_none());
    }
}
