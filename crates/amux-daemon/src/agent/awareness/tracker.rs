//! Per-entity outcome tracking with three-tier sliding windows.
//!
//! Each entity (thread, goal run, session) gets an OutcomeWindow that records
//! tool call outcomes and computes success rates at three time scales:
//! - Short-term: last 5 actions (AWAR-05)
//! - Medium-term: last 30 minutes (AWAR-05)
//! - Long-term: full session (AWAR-05)

use std::collections::VecDeque;

/// Number of recent actions for short-term success rate computation.
pub const SHORT_TERM_COUNT: usize = 5;

/// Duration in seconds for medium-term success rate computation (30 minutes).
pub const MEDIUM_TERM_SECS: u64 = 30 * 60;

/// A single recorded tool call outcome.
#[derive(Debug, Clone)]
pub struct OutcomeEntry {
    pub timestamp: u64,
    pub tool_name: String,
    /// SHA-256 first 16 chars of tool_name:args, reuses counter_who pattern.
    pub args_hash: String,
    pub success: bool,
    pub is_progress: bool,
}

/// Per-entity sliding window of tool call outcomes with computed rates.
#[derive(Debug, Clone)]
pub struct OutcomeWindow {
    pub entity_id: String,
    pub entity_type: String,
    pub recent_outcomes: VecDeque<OutcomeEntry>,
    /// Success rate computed from last SHORT_TERM_COUNT actions.
    pub short_term_success_rate: f64,
    /// Success rate computed from entries within last MEDIUM_TERM_SECS.
    pub medium_term_success_rate: f64,
    /// Success rate computed from all entries in window.
    pub long_term_success_rate: f64,
    /// Count of consecutive entries with the same args_hash.
    pub consecutive_same_pattern: u32,
    /// Timestamp of the last entry where is_progress was true.
    pub last_progress_at: u64,
    /// Total number of entries where is_progress was true.
    pub total_progress_count: u32,
    /// Total number of entries where success was false.
    pub total_failure_count: u32,
}

impl OutcomeWindow {
    /// Create a new empty OutcomeWindow for the given entity.
    pub fn new(entity_id: String, entity_type: String) -> Self {
        Self {
            entity_id,
            entity_type,
            recent_outcomes: VecDeque::new(),
            short_term_success_rate: 1.0,
            medium_term_success_rate: 1.0,
            long_term_success_rate: 1.0,
            consecutive_same_pattern: 0,
            last_progress_at: 0,
            total_progress_count: 0,
            total_failure_count: 0,
        }
    }

    /// Push a new outcome entry, capping at `max_entries`. Oldest dropped on overflow.
    pub fn push(&mut self, entry: OutcomeEntry, max_entries: usize) {
        // Update consecutive_same_pattern
        if let Some(last) = self.recent_outcomes.back() {
            if last.args_hash == entry.args_hash {
                self.consecutive_same_pattern += 1;
            } else {
                self.consecutive_same_pattern = 1;
            }
        } else {
            self.consecutive_same_pattern = 1;
        }

        // Update progress/failure counters
        if entry.is_progress {
            self.total_progress_count += 1;
            self.last_progress_at = entry.timestamp;
        }
        if !entry.success {
            self.total_failure_count += 1;
        }

        // Push and cap
        self.recent_outcomes.push_back(entry);
        while self.recent_outcomes.len() > max_entries {
            self.recent_outcomes.pop_front();
        }
    }

    /// Recompute all three success rates from the VecDeque contents.
    /// `now_ms` is the current time in milliseconds for medium-term window.
    pub fn recompute_rates(&mut self, now_ms: u64) {
        // Short-term: last SHORT_TERM_COUNT actions
        let short_term_entries: Vec<&OutcomeEntry> = self
            .recent_outcomes
            .iter()
            .rev()
            .take(SHORT_TERM_COUNT)
            .collect();
        if short_term_entries.is_empty() {
            self.short_term_success_rate = 1.0;
        } else {
            let successes = short_term_entries.iter().filter(|e| e.success).count();
            self.short_term_success_rate = successes as f64 / short_term_entries.len() as f64;
        }

        // Medium-term: entries within last MEDIUM_TERM_SECS
        let medium_cutoff = now_ms.saturating_sub(MEDIUM_TERM_SECS * 1000);
        let medium_entries: Vec<&OutcomeEntry> = self
            .recent_outcomes
            .iter()
            .filter(|e| e.timestamp >= medium_cutoff)
            .collect();
        if medium_entries.is_empty() {
            self.medium_term_success_rate = 1.0;
        } else {
            let successes = medium_entries.iter().filter(|e| e.success).count();
            self.medium_term_success_rate = successes as f64 / medium_entries.len() as f64;
        }

        // Long-term: all entries in window
        if self.recent_outcomes.is_empty() {
            self.long_term_success_rate = 1.0;
        } else {
            let successes = self.recent_outcomes.iter().filter(|e| e.success).count();
            self.long_term_success_rate = successes as f64 / self.recent_outcomes.len() as f64;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_entry(
        tool: &str,
        hash: &str,
        success: bool,
        is_progress: bool,
        ts: u64,
    ) -> OutcomeEntry {
        OutcomeEntry {
            timestamp: ts,
            tool_name: tool.to_string(),
            args_hash: hash.to_string(),
            success,
            is_progress,
        }
    }

    #[test]
    fn new_outcome_window_has_defaults() {
        let w = OutcomeWindow::new("thread-1".to_string(), "thread".to_string());
        assert!(w.recent_outcomes.is_empty());
        assert_eq!(w.short_term_success_rate, 1.0);
        assert_eq!(w.medium_term_success_rate, 1.0);
        assert_eq!(w.long_term_success_rate, 1.0);
        assert_eq!(w.consecutive_same_pattern, 0);
        assert_eq!(w.total_progress_count, 0);
        assert_eq!(w.total_failure_count, 0);
    }

    #[test]
    fn push_caps_at_max_entries() {
        let mut w = OutcomeWindow::new("e1".to_string(), "thread".to_string());
        for i in 0..250 {
            w.push(
                make_entry("tool", &format!("h{i}"), true, false, i as u64),
                200,
            );
        }
        assert_eq!(w.recent_outcomes.len(), 200);
        // Oldest entries should be dropped
        assert_eq!(w.recent_outcomes.front().unwrap().args_hash, "h50");
    }

    #[test]
    fn short_term_success_rate_from_last_5() {
        let mut w = OutcomeWindow::new("e1".to_string(), "thread".to_string());
        // Push 10 entries: first 5 success, last 5 failure
        for i in 0..5 {
            w.push(
                make_entry("tool", &format!("h{i}"), true, false, i as u64),
                200,
            );
        }
        for i in 5..10 {
            w.push(
                make_entry("tool", &format!("h{i}"), false, false, i as u64),
                200,
            );
        }
        w.recompute_rates(10);
        // Short-term = last 5 = all failures
        assert_eq!(w.short_term_success_rate, 0.0);
    }

    #[test]
    fn medium_term_success_rate_from_last_30_minutes() {
        let now = 2_000_000u64;
        let medium_cutoff = now - MEDIUM_TERM_SECS * 1000; // 30 min ago in ms
        let mut w = OutcomeWindow::new("e1".to_string(), "thread".to_string());

        // Entry well before the window (should be excluded)
        w.push(
            make_entry("tool", "old", false, false, medium_cutoff - 10000),
            200,
        );
        // Entry within window (success)
        w.push(make_entry("tool", "new1", true, false, now - 1000), 200);
        // Entry within window (failure)
        w.push(make_entry("tool", "new2", false, false, now - 500), 200);

        w.recompute_rates(now);
        // Medium-term: 2 entries within window, 1 success -> 0.5
        assert!((w.medium_term_success_rate - 0.5).abs() < 0.01);
    }

    #[test]
    fn long_term_success_rate_from_all_entries() {
        let mut w = OutcomeWindow::new("e1".to_string(), "thread".to_string());
        w.push(make_entry("tool", "h1", true, false, 1), 200);
        w.push(make_entry("tool", "h2", true, false, 2), 200);
        w.push(make_entry("tool", "h3", false, false, 3), 200);
        w.recompute_rates(4);
        // 2 out of 3 = 0.666...
        assert!((w.long_term_success_rate - 2.0 / 3.0).abs() < 0.01);
    }

    #[test]
    fn consecutive_same_pattern_increments_and_resets() {
        let mut w = OutcomeWindow::new("e1".to_string(), "thread".to_string());
        w.push(make_entry("tool", "aaa", true, false, 1), 200);
        assert_eq!(w.consecutive_same_pattern, 1);
        w.push(make_entry("tool", "aaa", true, false, 2), 200);
        assert_eq!(w.consecutive_same_pattern, 2);
        w.push(make_entry("tool", "aaa", false, false, 3), 200);
        assert_eq!(w.consecutive_same_pattern, 3);
        // Different hash resets
        w.push(make_entry("tool", "bbb", true, false, 4), 200);
        assert_eq!(w.consecutive_same_pattern, 1);
    }

    #[test]
    fn progress_and_failure_counters() {
        let mut w = OutcomeWindow::new("e1".to_string(), "thread".to_string());
        w.push(make_entry("tool", "h1", true, true, 100), 200);
        w.push(make_entry("tool", "h2", false, false, 200), 200);
        w.push(make_entry("tool", "h3", true, true, 300), 200);
        assert_eq!(w.total_progress_count, 2);
        assert_eq!(w.total_failure_count, 1);
        assert_eq!(w.last_progress_at, 300);
    }
}
