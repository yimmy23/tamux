//! Trajectory computation: converging, diverging, or stalled.
//!
//! Computes a trajectory ratio from progress vs failure counts in a sliding
//! window (AWAR-04). The ratio ranges from -1.0 (pure diverging) through
//! 0.0 (stalled) to 1.0 (pure converging).

use super::tracker::OutcomeWindow;

/// The direction the agent is heading in its current work.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TrajectoryDirection {
    Converging,
    Diverging,
    Stalled,
}

/// Trajectory state with direction, ratio, and human-readable label.
#[derive(Debug, Clone)]
pub struct TrajectoryState {
    pub direction: TrajectoryDirection,
    /// Ratio from -1.0 (fully diverging) to 1.0 (fully converging).
    pub progress_ratio: f64,
    /// Human-readable label: "converging", "diverging", or "stalled".
    pub label: &'static str,
}

/// Compute a raw trajectory ratio from progress and failure counts.
///
/// Returns a value in the range -1.0..=1.0:
/// - Positive (converging): progress_count > failure_count
/// - Negative (diverging): failure_count > progress_count
/// - Zero: equal counts or no events
pub fn compute_trajectory(progress_count: u32, failure_count: u32) -> f64 {
    let total = progress_count + failure_count;
    if total == 0 {
        return 0.0;
    }
    let diff = progress_count as f64 - failure_count as f64;
    diff / total as f64
}

/// Compute the full trajectory state from an OutcomeWindow.
pub fn compute_trajectory_state(window: &OutcomeWindow) -> TrajectoryState {
    let ratio = compute_trajectory(window.total_progress_count, window.total_failure_count);
    let (direction, label) = if ratio > 0.1 {
        (TrajectoryDirection::Converging, "converging")
    } else if ratio < -0.1 {
        (TrajectoryDirection::Diverging, "diverging")
    } else {
        (TrajectoryDirection::Stalled, "stalled")
    };
    TrajectoryState {
        direction,
        progress_ratio: ratio,
        label,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compute_trajectory_returns_zero_when_no_events() {
        assert_eq!(compute_trajectory(0, 0), 0.0);
    }

    #[test]
    fn compute_trajectory_positive_when_progress_exceeds_failure() {
        let ratio = compute_trajectory(8, 2);
        assert!(ratio > 0.0, "expected positive, got {ratio}");
        // 8-2 / 10 = 0.6
        assert!((ratio - 0.6).abs() < 0.01);
    }

    #[test]
    fn compute_trajectory_negative_when_failure_exceeds_progress() {
        let ratio = compute_trajectory(1, 9);
        assert!(ratio < 0.0, "expected negative, got {ratio}");
        // 1-9 / 10 = -0.8
        assert!((ratio - (-0.8)).abs() < 0.01);
    }

    #[test]
    fn compute_trajectory_state_stalled_when_near_zero() {
        let mut w = OutcomeWindow::new("e1".to_string(), "thread".to_string());
        w.total_progress_count = 5;
        w.total_failure_count = 5;
        let state = compute_trajectory_state(&w);
        assert_eq!(state.direction, TrajectoryDirection::Stalled);
        assert_eq!(state.label, "stalled");
    }

    #[test]
    fn compute_trajectory_state_converging() {
        let mut w = OutcomeWindow::new("e1".to_string(), "thread".to_string());
        w.total_progress_count = 8;
        w.total_failure_count = 2;
        let state = compute_trajectory_state(&w);
        assert_eq!(state.direction, TrajectoryDirection::Converging);
        assert_eq!(state.label, "converging");
        assert!(state.progress_ratio > 0.1);
    }

    #[test]
    fn compute_trajectory_state_diverging() {
        let mut w = OutcomeWindow::new("e1".to_string(), "thread".to_string());
        w.total_progress_count = 1;
        w.total_failure_count = 9;
        let state = compute_trajectory_state(&w);
        assert_eq!(state.direction, TrajectoryDirection::Diverging);
        assert_eq!(state.label, "diverging");
        assert!(state.progress_ratio < -0.1);
    }

    #[test]
    fn compute_trajectory_state_stalled_with_no_events() {
        let w = OutcomeWindow::new("e1".to_string(), "thread".to_string());
        let state = compute_trajectory_state(&w);
        assert_eq!(state.direction, TrajectoryDirection::Stalled);
        assert_eq!(state.progress_ratio, 0.0);
    }
}
