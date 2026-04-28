//! Situational awareness module: per-entity failure tracking with three-tier
//! sliding windows, diminishing returns detection with counter-who false positive
//! guard, trajectory computation, and mode shift notification.
//!
//! The awareness subsystem tracks tool outcomes per-entity, detects diminishing
//! returns, computes trajectory, and triggers mode shifts -- all while consulting
//! counter-who (AWAR-03) to prevent false positives from legitimate repetitive work.

pub mod mode_shift;
pub mod tracker;
pub mod trajectory;

use std::collections::{HashMap, HashSet};

use tracker::{OutcomeEntry, OutcomeWindow};
use trajectory::TrajectoryState;

/// Maximum number of tracked entities before oldest are pruned.
pub const MAX_TRACKED_ENTITIES: usize = 100;

/// Maximum number of outcome entries per entity window.
pub const MAX_OUTCOMES_PER_WINDOW: usize = 200;

/// Minimum consecutive same-pattern count before diminishing returns fires.
const DIMINISHING_RETURNS_THRESHOLD: u32 = 3;

/// Short-term success rate below which diminishing returns may be flagged.
const DIMINISHING_RETURNS_SUCCESS_THRESHOLD: f64 = 0.3;

fn consecutive_same_tool_failures(window: &tracker::OutcomeWindow) -> Option<(String, u32)> {
    let last = window.recent_outcomes.back()?;
    if last.success {
        return None;
    }

    let tool_name = last.tool_name.clone();
    let count = window
        .recent_outcomes
        .iter()
        .rev()
        .take_while(|entry| !entry.success && entry.tool_name == tool_name)
        .count() as u32;

    Some((tool_name, count))
}

/// Central awareness monitor tracking per-entity tool call outcomes.
#[derive(Debug, Clone)]
pub struct AwarenessMonitor {
    windows: HashMap<String, OutcomeWindow>,
}

impl Default for AwarenessMonitor {
    fn default() -> Self {
        Self::new()
    }
}

impl AwarenessMonitor {
    /// Create a new empty monitor with no tracked entities.
    pub fn new() -> Self {
        Self {
            windows: HashMap::new(),
        }
    }

    /// Record a tool call outcome for the given entity.
    ///
    /// Creates a new OutcomeWindow if the entity is unseen.
    /// Caps total tracked entities at MAX_TRACKED_ENTITIES by removing the
    /// least recently active entity when full.
    pub fn record_outcome(
        &mut self,
        entity_id: &str,
        entity_type: &str,
        tool_name: &str,
        args_hash: &str,
        success: bool,
        is_progress: bool,
        now_ms: u64,
    ) {
        // Enforce entity cap: remove least recently active if at limit
        if !self.windows.contains_key(entity_id) && self.windows.len() >= MAX_TRACKED_ENTITIES {
            // Find entity with oldest last entry timestamp
            if let Some(oldest_id) = self
                .windows
                .iter()
                .min_by_key(|(_, w)| w.recent_outcomes.back().map(|e| e.timestamp).unwrap_or(0))
                .map(|(id, _)| id.clone())
            {
                self.windows.remove(&oldest_id);
            }
        }

        let window = self
            .windows
            .entry(entity_id.to_string())
            .or_insert_with(|| OutcomeWindow::new(entity_id.to_string(), entity_type.to_string()));

        let entry = OutcomeEntry {
            timestamp: now_ms,
            tool_name: tool_name.to_string(),
            args_hash: args_hash.to_string(),
            success,
            is_progress,
        };

        window.push(entry, MAX_OUTCOMES_PER_WINDOW);
        window.recompute_rates(now_ms);
    }

    /// Check whether diminishing returns have been detected for the given entity.
    ///
    /// Returns `Some(reason)` when:
    /// - short_term_success_rate < 0.3 AND
    /// - consecutive_same_pattern >= 3, or
    /// - consecutive failures keep hitting the same tool >= 3 times
    ///
    /// Returns `None` otherwise.
    pub fn check_diminishing_returns(&self, entity_id: &str) -> Option<String> {
        let window = self.windows.get(entity_id)?;

        if window.short_term_success_rate >= DIMINISHING_RETURNS_SUCCESS_THRESHOLD {
            return None;
        }

        if window.consecutive_same_pattern >= DIMINISHING_RETURNS_THRESHOLD {
            return Some(format!(
                "Diminishing returns: {} consecutive same-pattern calls with {:.0}% short-term success rate",
                window.consecutive_same_pattern,
                window.short_term_success_rate * 100.0,
            ));
        }

        let (tool_name, same_tool_failures) = consecutive_same_tool_failures(window)?;
        if same_tool_failures < DIMINISHING_RETURNS_THRESHOLD {
            return None;
        }

        Some(format!(
            "Diminishing returns: {} consecutive same-tool failures on {} with {:.0}% short-term success rate",
            same_tool_failures,
            tool_name,
            window.short_term_success_rate * 100.0,
        ))
    }

    /// Get the trajectory state for the given entity, if tracked.
    pub fn get_trajectory(&self, entity_id: &str) -> Option<TrajectoryState> {
        let window = self.windows.get(entity_id)?;
        Some(trajectory::compute_trajectory_state(window))
    }

    /// Remove windows for entities not in the active set.
    pub fn prune_completed_entities(&mut self, active_ids: &HashSet<String>) {
        self.windows.retain(|id, _| active_ids.contains(id));
    }

    /// Number of currently tracked entities.
    pub fn window_count(&self) -> usize {
        self.windows.len()
    }

    /// Get a reference to the OutcomeWindow for the given entity, if tracked.
    pub fn get_window(&self, entity_id: &str) -> Option<&OutcomeWindow> {
        self.windows.get(entity_id)
    }

    /// Compute the average short-term success rate across all tracked windows.
    ///
    /// Returns 0.8 if no windows exist (default healthy assumption, consumed
    /// by Plan 03 for confidence scoring).
    pub fn aggregate_short_term_success_rate(&self) -> f64 {
        if self.windows.is_empty() {
            return 0.8;
        }
        let sum: f64 = self
            .windows
            .values()
            .map(|w| w.short_term_success_rate)
            .sum();
        sum / self.windows.len() as f64
    }
}

// ---------------------------------------------------------------------------
// AgentEngine integration methods
// ---------------------------------------------------------------------------

use crate::agent::engine::AgentEngine;
use crate::agent::types::AgentEvent;

impl AgentEngine {
    /// Record a tool call outcome in the awareness monitor (AWAR-01).
    pub(crate) async fn record_awareness_outcome(
        &self,
        entity_id: &str,
        entity_type: &str,
        tool_name: &str,
        args_hash: &str,
        success: bool,
        is_progress: bool,
    ) {
        let now = super::now_millis();
        let mut monitor = self.awareness.write().await;
        monitor.record_outcome(
            entity_id,
            entity_type,
            tool_name,
            args_hash,
            success,
            is_progress,
            now,
        );
    }

    /// Check for diminishing returns and evaluate mode shift (AWAR-02 + AWAR-03).
    ///
    /// Consults counter-who before firing any mode shift (locked decision AWAR-03).
    pub(crate) async fn check_awareness_mode_shift(&self, entity_id: &str, thread_id: &str) {
        // 1. Check diminishing returns from awareness
        let diminishing = {
            let monitor = self.awareness.read().await;
            monitor.check_diminishing_returns(entity_id)
        };
        if diminishing.is_none() {
            return;
        }

        // 2. Consult counter-who (AWAR-03 locked decision)
        let counter_who_confirms = {
            let scope_id = crate::agent::agent_identity::current_agent_scope_id();
            let stores = self.episodic_store.read().await;
            let store = stores.get(&scope_id).cloned().unwrap_or_default();
            super::episodic::counter_who::detect_repeated_approaches(
                &store.counter_who.tried_approaches,
                3,
            )
            .is_some()
        };

        // 3. Evaluate mode shift
        let decision = mode_shift::evaluate_mode_shift(diminishing, counter_who_confirms);

        if decision.should_shift {
            let _ = self.event_tx.send(AgentEvent::ModeShift {
                thread_id: thread_id.to_string(),
                reason: decision.reason,
                previous_strategy: "current".to_string(),
                new_strategy: decision.suggested_strategy,
            });
        }
    }

    /// Get the trajectory state for the given entity, if tracked.
    pub(crate) async fn get_awareness_trajectory(
        &self,
        entity_id: &str,
    ) -> Option<trajectory::TrajectoryState> {
        let monitor = self.awareness.read().await;
        monitor.get_trajectory(entity_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_monitor_has_no_windows() {
        let monitor = AwarenessMonitor::new();
        assert_eq!(monitor.window_count(), 0);
    }

    #[test]
    fn record_outcome_creates_window_for_unseen_entity() {
        let mut monitor = AwarenessMonitor::new();
        monitor.record_outcome(
            "thread-1",
            "thread",
            "read_file",
            "abc123",
            true,
            false,
            1000,
        );
        assert_eq!(monitor.window_count(), 1);
        assert!(monitor.get_window("thread-1").is_some());
    }

    #[test]
    fn record_outcome_appends_to_existing_window() {
        let mut monitor = AwarenessMonitor::new();
        monitor.record_outcome(
            "thread-1",
            "thread",
            "read_file",
            "abc123",
            true,
            false,
            1000,
        );
        monitor.record_outcome(
            "thread-1",
            "thread",
            "write_file",
            "def456",
            true,
            true,
            2000,
        );
        assert_eq!(monitor.window_count(), 1);
        let w = monitor.get_window("thread-1").unwrap();
        assert_eq!(w.recent_outcomes.len(), 2);
    }

    #[test]
    fn window_caps_at_max_outcomes() {
        let mut monitor = AwarenessMonitor::new();
        for i in 0..250 {
            monitor.record_outcome(
                "e1",
                "thread",
                "tool",
                &format!("h{i}"),
                true,
                false,
                i as u64,
            );
        }
        let w = monitor.get_window("e1").unwrap();
        assert_eq!(w.recent_outcomes.len(), MAX_OUTCOMES_PER_WINDOW);
    }

    #[test]
    fn check_diminishing_returns_none_when_success_rate_high() {
        let mut monitor = AwarenessMonitor::new();
        // All successes with same pattern
        for i in 0..5 {
            monitor.record_outcome("e1", "thread", "tool", "same", true, false, i);
        }
        assert!(monitor.check_diminishing_returns("e1").is_none());
    }

    #[test]
    fn check_diminishing_returns_none_when_pattern_count_low() {
        let mut monitor = AwarenessMonitor::new();
        // 2 failures with same pattern (below threshold of 3)
        monitor.record_outcome("e1", "thread", "tool", "same", false, false, 1);
        monitor.record_outcome("e1", "thread", "tool", "same", false, false, 2);
        assert!(monitor.check_diminishing_returns("e1").is_none());
    }

    #[test]
    fn check_diminishing_returns_some_when_stuck() {
        let mut monitor = AwarenessMonitor::new();
        // 5 failures with same pattern -- consecutive_same_pattern >= 3, success_rate < 0.3
        for i in 0..5 {
            monitor.record_outcome("e1", "thread", "tool", "same", false, false, i);
        }
        let result = monitor.check_diminishing_returns("e1");
        assert!(result.is_some());
        let msg = result.unwrap();
        assert!(msg.contains("consecutive same-pattern"), "got: {msg}");
    }

    #[test]
    fn check_diminishing_returns_some_for_same_tool_failure_spiral_even_when_args_change() {
        let mut monitor = AwarenessMonitor::new();
        for i in 0..5 {
            monitor.record_outcome(
                "e1",
                "thread",
                "web_search",
                &format!("query-{i}"),
                false,
                false,
                i,
            );
        }

        let result = monitor.check_diminishing_returns("e1");
        assert!(result.is_some());
        let msg = result.unwrap();
        assert!(msg.contains("same-tool failures"), "got: {msg}");
        assert!(msg.contains("web_search"), "got: {msg}");
    }

    #[test]
    fn prune_completed_entities_removes_inactive() {
        let mut monitor = AwarenessMonitor::new();
        monitor.record_outcome("e1", "thread", "tool", "h1", true, false, 1);
        monitor.record_outcome("e2", "thread", "tool", "h2", true, false, 2);
        monitor.record_outcome("e3", "thread", "tool", "h3", true, false, 3);

        let mut active = HashSet::new();
        active.insert("e2".to_string());
        monitor.prune_completed_entities(&active);

        assert_eq!(monitor.window_count(), 1);
        assert!(monitor.get_window("e2").is_some());
        assert!(monitor.get_window("e1").is_none());
        assert!(monitor.get_window("e3").is_none());
    }

    #[test]
    fn total_entity_count_capped_at_max() {
        let mut monitor = AwarenessMonitor::new();
        for i in 0..120 {
            monitor.record_outcome(
                &format!("entity-{i}"),
                "thread",
                "tool",
                "hash",
                true,
                false,
                i as u64,
            );
        }
        assert!(monitor.window_count() <= MAX_TRACKED_ENTITIES);
    }

    #[test]
    fn aggregate_short_term_success_rate_default_when_empty() {
        let monitor = AwarenessMonitor::new();
        assert_eq!(monitor.aggregate_short_term_success_rate(), 0.8);
    }

    #[test]
    fn aggregate_short_term_success_rate_averages() {
        let mut monitor = AwarenessMonitor::new();
        // Entity 1: all successes -> 1.0
        for i in 0..5 {
            monitor.record_outcome("e1", "thread", "tool", &format!("h{i}"), true, false, i);
        }
        // Entity 2: all failures -> 0.0
        for i in 0..5 {
            monitor.record_outcome(
                "e2",
                "thread",
                "tool",
                &format!("h{i}"),
                false,
                false,
                100 + i,
            );
        }
        let avg = monitor.aggregate_short_term_success_rate();
        // (1.0 + 0.0) / 2 = 0.5
        assert!((avg - 0.5).abs() < 0.01);
    }

    #[test]
    fn get_trajectory_for_tracked_entity() {
        let mut monitor = AwarenessMonitor::new();
        monitor.record_outcome("e1", "thread", "tool", "h1", true, true, 1);
        monitor.record_outcome("e1", "thread", "tool", "h2", true, true, 2);
        let traj = monitor.get_trajectory("e1");
        assert!(traj.is_some());
    }

    #[test]
    fn get_trajectory_none_for_unknown() {
        let monitor = AwarenessMonitor::new();
        assert!(monitor.get_trajectory("unknown").is_none());
    }
}
