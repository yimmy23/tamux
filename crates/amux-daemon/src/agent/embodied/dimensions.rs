//! Pure computation functions for all 5 embodied scalar dimensions.
//!
//! Each function computes a single dimension from structural signals only --
//! no I/O, no async, no side effects. Designed for direct unit testing and
//! composition via `compute_embodied_metadata` in the parent module.

/// Compute difficulty (0.0-1.0) from error rate and retry count (EMBD-01).
///
/// 0.6 weight on error rate, 0.4 on retry factor. Retry factor caps at 5.
/// - error_rate: fraction of recent calls that errored (0.0..=1.0)
/// - retry_count: number of retries attempted for the current action
pub fn compute_difficulty(error_rate: f64, retry_count: u32) -> f64 {
    let retry_factor = (retry_count as f64 / 5.0).min(1.0);
    (0.6 * error_rate + 0.4 * retry_factor).clamp(0.0, 1.0)
}

/// Compute familiarity (0.0-1.0) from episodic memory FTS5 hit count (EMBD-01).
///
/// 0 hits = 0.0 (novel), 5+ hits = 1.0 (very familiar).
/// Linear scaling capped at 5.
pub fn compute_familiarity(episodic_hit_count: usize) -> f64 {
    let capped = episodic_hit_count.min(5) as f64;
    capped / 5.0
}

/// Compute trajectory score (-1.0 to 1.0) from progress/failure ratio (EMBD-01).
///
/// Positive = converging toward goal, negative = diverging, 0.0 = stalled/no data.
/// Per locked decision: trajectory = ratio of progress events vs retry/failure events.
pub fn compute_trajectory_score(progress_count: u32, failure_count: u32) -> f64 {
    let total = progress_count + failure_count;
    if total == 0 {
        return 0.0;
    }
    let ratio = progress_count as f64 / total as f64;
    // Map [0.0, 1.0] to [-1.0, 1.0]
    (ratio * 2.0 - 1.0).clamp(-1.0, 1.0)
}

/// Compute temperature (0.0-1.0) from operator message urgency signals (EMBD-02).
///
/// Structural signal only: message count in recent window + inter-message timing.
/// Per research open question: use message frequency, NOT sentiment parsing.
///
/// - `recent_message_count`: messages from operator in last 5 minutes
/// - `avg_gap_secs`: average seconds between operator messages (0 if only 1 message)
pub fn compute_temperature(recent_message_count: u32, avg_gap_secs: u64) -> f64 {
    if recent_message_count == 0 {
        return 0.0;
    }
    // Frequency component: 3+ messages in 5 min = high urgency
    let freq = (recent_message_count as f64 / 3.0).min(1.0);
    // Pacing component: rapid-fire (< 30s gaps) = high urgency
    let pacing = if avg_gap_secs == 0 {
        0.5
    } else {
        (1.0 - (avg_gap_secs as f64 / 120.0).min(1.0)).max(0.0)
    };
    (0.6 * freq + 0.4 * pacing).clamp(0.0, 1.0)
}

/// Compute weight (0.0-1.0) for conceptual mass / blast radius (EMBD-03).
///
/// Heavy actions (config changes, deployments, deletes) = close to 1.0.
/// Light actions (reads, queries, searches) = close to 0.0.
/// Unknown tools default to 0.5 (medium weight).
pub fn compute_weight(tool_name: &str) -> f64 {
    match tool_name {
        // Heavy actions (state-changing, destructive)
        "execute_command"
        | "execute_managed_command"
        | "write_file"
        | "delete_file"
        | "deploy"
        | "create_session" => 0.8,
        // Medium actions (state-changing but bounded)
        "edit_file" | "create_file" | "install_package" => 0.5,
        // Light actions (read-only)
        "read_file" | "search_files" | "list_files" | "list_directory" | "web_search"
        | "web_read" | "symbol_search" => 0.2,
        // Default for unknown tools
        _ => 0.5,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // compute_difficulty
    // -----------------------------------------------------------------------

    #[test]
    fn difficulty_zero_when_no_errors_no_retries() {
        assert_eq!(compute_difficulty(0.0, 0), 0.0);
    }

    #[test]
    fn difficulty_max_when_all_errors_max_retries() {
        assert_eq!(compute_difficulty(1.0, 5), 1.0);
    }

    #[test]
    fn difficulty_intermediate_value() {
        // 0.6 * 0.5 + 0.4 * (2/5) = 0.3 + 0.16 = 0.46
        let result = compute_difficulty(0.5, 2);
        assert!(
            (result - 0.46).abs() < 0.001,
            "expected ~0.46, got {result}"
        );
    }

    // -----------------------------------------------------------------------
    // compute_familiarity
    // -----------------------------------------------------------------------

    #[test]
    fn familiarity_zero_when_novel() {
        assert_eq!(compute_familiarity(0), 0.0);
    }

    #[test]
    fn familiarity_max_at_five_hits() {
        assert_eq!(compute_familiarity(5), 1.0);
    }

    #[test]
    fn familiarity_intermediate() {
        assert_eq!(compute_familiarity(3), 0.6);
    }

    #[test]
    fn familiarity_capped_above_five() {
        assert_eq!(compute_familiarity(10), 1.0);
    }

    // -----------------------------------------------------------------------
    // compute_trajectory_score
    // -----------------------------------------------------------------------

    #[test]
    fn trajectory_max_when_all_progress() {
        assert_eq!(compute_trajectory_score(5, 0), 1.0);
    }

    #[test]
    fn trajectory_min_when_all_failure() {
        assert_eq!(compute_trajectory_score(0, 5), -1.0);
    }

    #[test]
    fn trajectory_zero_when_no_data() {
        assert_eq!(compute_trajectory_score(0, 0), 0.0);
    }

    #[test]
    fn trajectory_zero_when_balanced() {
        assert_eq!(compute_trajectory_score(3, 3), 0.0);
    }

    // -----------------------------------------------------------------------
    // compute_temperature
    // -----------------------------------------------------------------------

    #[test]
    fn temperature_zero_when_no_messages() {
        assert_eq!(compute_temperature(0, 999), 0.0);
    }

    #[test]
    fn temperature_high_when_frequent_rapid_messages() {
        let result = compute_temperature(5, 30);
        // freq = min(5/3, 1.0) = 1.0
        // pacing = max(1.0 - 30/120, 0.0) = 0.75
        // 0.6 * 1.0 + 0.4 * 0.75 = 0.9
        assert!(result > 0.7, "expected high temp, got {result}");
    }

    #[test]
    fn temperature_low_when_infrequent_slow_messages() {
        let result = compute_temperature(1, 600);
        // freq = min(1/3, 1.0) = 0.333
        // pacing = max(1.0 - 600/120, 0.0) = max(1.0 - 5.0, 0.0) = 0.0
        // 0.6 * 0.333 + 0.4 * 0.0 = 0.2
        assert!(result < 0.3, "expected low temp, got {result}");
    }

    // -----------------------------------------------------------------------
    // compute_weight
    // -----------------------------------------------------------------------

    #[test]
    fn weight_heavy_for_execute_command() {
        assert!(compute_weight("execute_command") >= 0.7);
    }

    #[test]
    fn weight_light_for_read_file() {
        assert!(compute_weight("read_file") <= 0.3);
    }

    #[test]
    fn weight_default_for_unknown_tool() {
        assert_eq!(compute_weight("unknown_tool"), 0.5);
    }
}
