//! Health monitoring — periodic assessment with hysteresis to avoid state flapping.

use super::state_layers::{HealthIndicators, HealthState};

/// Tracks agent health over time, using hysteresis counters to prevent
/// rapid state oscillation (flapping) on noisy indicator streams.
pub struct HealthMonitor {
    current_state: HealthState,
    state_entered_at: u64,
    degraded_ticks: u32,
    healthy_ticks: u32,
}

/// Snapshot produced by each [`HealthMonitor::check`] call.
#[derive(Debug, Clone)]
pub struct HealthReport {
    /// Current health state after evaluation.
    pub state: HealthState,
    /// Whether the state changed during this tick.
    pub changed: bool,
    /// The previous state if a transition occurred.
    pub previous_state: Option<HealthState>,
    /// The indicators that were evaluated.
    pub indicators: HealthIndicators,
    /// Human-readable suggestions based on the current state.
    pub recommendations: Vec<String>,
}

// ---------------------------------------------------------------------------
// Hysteresis thresholds (consecutive ticks required for a transition)
// ---------------------------------------------------------------------------
const HEALTHY_TO_DEGRADED_TICKS: u32 = 2;
const DEGRADED_TO_STUCK_TICKS: u32 = 3;
const DEGRADED_TO_HEALTHY_TICKS: u32 = 3;
const STUCK_TO_DEGRADED_TICKS: u32 = 2;

// ---------------------------------------------------------------------------
// Indicator thresholds
// ---------------------------------------------------------------------------
const DEGRADED_ERROR_RATE: f64 = 0.3;
const DEGRADED_CONTEXT_UTIL_PCT: u32 = 85;
const DEGRADED_TOOL_FREQ_MIN: f64 = 0.5;

const STUCK_ERROR_RATE: f64 = 0.5;
const STUCK_CONTEXT_UTIL_PCT: u32 = 95;
/// Seconds without progress before we consider the agent stuck.
const STUCK_NO_PROGRESS_SECS: u64 = 5 * 60;

const CRASH_ERROR_RATE: f64 = 0.8;
const CRASH_CONSECUTIVE_ERRORS: u32 = 5;

impl HealthMonitor {
    /// Create a new monitor starting in the [`HealthState::Healthy`] state.
    pub fn new(now: u64) -> Self {
        Self {
            current_state: HealthState::Healthy,
            state_entered_at: now,
            degraded_ticks: 0,
            healthy_ticks: 0,
        }
    }

    /// Evaluate the latest `indicators` and return a [`HealthReport`].
    ///
    /// The method applies hysteresis rules so that transient spikes in error
    /// rate or context usage do not cause the monitor to flip between states
    /// on every tick.
    pub fn check(&mut self, indicators: &HealthIndicators, now: u64) -> HealthReport {
        let previous_state = self.current_state;

        // ---------------------------------------------------------------
        // Immediate transitions (no hysteresis)
        // ---------------------------------------------------------------

        // Crashed: error_rate > 0.8 OR consecutive_errors >= 5
        if is_crashed(indicators) {
            self.transition_to(HealthState::Crashed, now);
            return self.report(previous_state, indicators);
        }

        // WaitingForInput: no tool calls and no errors — the agent is idle.
        if is_waiting_for_input(indicators) {
            self.transition_to(HealthState::WaitingForInput, now);
            return self.report(previous_state, indicators);
        }

        // ---------------------------------------------------------------
        // Classify the *current tick's* raw signal
        // ---------------------------------------------------------------
        let tick_is_stuck = is_stuck_indicators(indicators, now);
        let tick_is_degraded = is_degraded_indicators(indicators);
        let tick_is_healthy = !tick_is_degraded && !tick_is_stuck;

        // Update consecutive-tick counters.
        if tick_is_healthy {
            self.healthy_ticks += 1;
            self.degraded_ticks = 0;
        } else {
            self.healthy_ticks = 0;
            self.degraded_ticks += 1;
        }

        // ---------------------------------------------------------------
        // Hysteresis-guarded transitions
        // ---------------------------------------------------------------
        match self.current_state {
            HealthState::Healthy => {
                if self.degraded_ticks >= HEALTHY_TO_DEGRADED_TICKS {
                    self.transition_to(HealthState::Degraded, now);
                }
            }
            HealthState::Degraded => {
                if tick_is_stuck && self.degraded_ticks >= DEGRADED_TO_STUCK_TICKS {
                    self.transition_to(HealthState::Stuck, now);
                } else if self.healthy_ticks >= DEGRADED_TO_HEALTHY_TICKS {
                    self.transition_to(HealthState::Healthy, now);
                }
            }
            HealthState::Stuck => {
                // Recovering from Stuck requires consecutive non-stuck ticks.
                if !tick_is_stuck && self.healthy_ticks >= STUCK_TO_DEGRADED_TICKS {
                    self.transition_to(HealthState::Degraded, now);
                }
            }
            // Crashed and WaitingForInput are handled above (immediate).
            // They stay until a `reset()` or a subsequent check reclassifies.
            HealthState::Crashed | HealthState::WaitingForInput => {
                // Re-evaluate: if indicators no longer meet the immediate
                // criteria we fell through to here, allow recovery.
                if tick_is_healthy && self.healthy_ticks >= DEGRADED_TO_HEALTHY_TICKS {
                    self.transition_to(HealthState::Healthy, now);
                } else if tick_is_degraded {
                    self.transition_to(HealthState::Degraded, now);
                }
            }
        }

        self.report(previous_state, indicators)
    }

    /// Return the current [`HealthState`].
    pub fn state(&self) -> HealthState {
        self.current_state
    }

    /// Reset the monitor back to [`HealthState::Healthy`], clearing all
    /// hysteresis counters.
    pub fn reset(&mut self, now: u64) {
        self.current_state = HealthState::Healthy;
        self.state_entered_at = now;
        self.degraded_ticks = 0;
        self.healthy_ticks = 0;
    }

    // -- private helpers --------------------------------------------------

    fn transition_to(&mut self, new_state: HealthState, now: u64) {
        if self.current_state != new_state {
            self.current_state = new_state;
            self.state_entered_at = now;
            self.degraded_ticks = 0;
            self.healthy_ticks = 0;
        }
    }

    fn report(&self, previous_state: HealthState, indicators: &HealthIndicators) -> HealthReport {
        let changed = self.current_state != previous_state;
        HealthReport {
            state: self.current_state,
            changed,
            previous_state: if changed { Some(previous_state) } else { None },
            indicators: indicators.clone(),
            recommendations: compute_recommendations(self.current_state, indicators),
        }
    }
}

// ---------------------------------------------------------------------------
// Indicator classification helpers
// ---------------------------------------------------------------------------

fn is_crashed(ind: &HealthIndicators) -> bool {
    ind.error_rate > CRASH_ERROR_RATE || ind.consecutive_errors >= CRASH_CONSECUTIVE_ERRORS
}

fn is_waiting_for_input(ind: &HealthIndicators) -> bool {
    ind.total_tool_calls == 0 && ind.consecutive_errors == 0
}

fn is_degraded_indicators(ind: &HealthIndicators) -> bool {
    ind.error_rate > DEGRADED_ERROR_RATE
        || ind.context_utilization_pct > DEGRADED_CONTEXT_UTIL_PCT
        || (ind.total_tool_calls > 0 && ind.tool_call_frequency < DEGRADED_TOOL_FREQ_MIN)
}

fn is_stuck_indicators(ind: &HealthIndicators, now: u64) -> bool {
    if ind.error_rate > STUCK_ERROR_RATE || ind.context_utilization_pct > STUCK_CONTEXT_UTIL_PCT {
        return true;
    }

    // No progress for 5+ minutes.
    if let Some(last) = ind.last_progress_at {
        if now.saturating_sub(last) >= STUCK_NO_PROGRESS_SECS {
            return true;
        }
    }

    false
}

// ---------------------------------------------------------------------------
// Recommendation engine
// ---------------------------------------------------------------------------

/// Produce actionable, human-readable suggestions based on the current state
/// and the measured indicators.
pub fn compute_recommendations(state: HealthState, indicators: &HealthIndicators) -> Vec<String> {
    let mut recs = Vec::new();

    match state {
        HealthState::Healthy => {
            // Nothing urgent, but we can still give minor advice.
            if indicators.context_utilization_pct > 70 {
                recs.push(
                    "Context utilization is above 70% — consider summarising context soon."
                        .to_string(),
                );
            }
        }
        HealthState::Degraded => {
            if indicators.error_rate > DEGRADED_ERROR_RATE {
                recs.push(format!(
                    "Error rate is {:.0}% — review recent tool failures and retry strategy.",
                    indicators.error_rate * 100.0,
                ));
            }
            if indicators.context_utilization_pct > DEGRADED_CONTEXT_UTIL_PCT {
                recs.push(format!(
                    "Context utilization at {}% — compact or summarise the conversation.",
                    indicators.context_utilization_pct,
                ));
            }
            if indicators.total_tool_calls > 0
                && indicators.tool_call_frequency < DEGRADED_TOOL_FREQ_MIN
            {
                recs.push(
                    "Tool call frequency is low — the agent may be spinning without acting."
                        .to_string(),
                );
            }
        }
        HealthState::Stuck => {
            recs.push(
                "Agent appears stuck — consider replanning or resetting context.".to_string(),
            );
            if indicators.error_rate > STUCK_ERROR_RATE {
                recs.push(
                    "Error rate exceeds 50% — investigate persistent tool failures.".to_string(),
                );
            }
            if indicators.context_utilization_pct > STUCK_CONTEXT_UTIL_PCT {
                recs.push(
                    "Context is nearly full — force a context compaction or start a new thread."
                        .to_string(),
                );
            }
        }
        HealthState::Crashed => {
            recs.push(
                "Agent has crashed — inspect logs and restart or recover from checkpoint."
                    .to_string(),
            );
            if indicators.consecutive_errors >= CRASH_CONSECUTIVE_ERRORS {
                recs.push(format!(
                    "{} consecutive errors — the tool environment may be broken.",
                    indicators.consecutive_errors,
                ));
            }
        }
        HealthState::WaitingForInput => {
            recs.push("Agent is idle, waiting for user input or external event.".to_string());
        }
    }

    recs
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Convenience builder for [`HealthIndicators`] with sane defaults.
    fn healthy_indicators() -> HealthIndicators {
        HealthIndicators {
            last_progress_at: Some(1000),
            tool_call_frequency: 5.0,
            error_rate: 0.0,
            context_growth_rate: 100.0,
            context_utilization_pct: 30,
            consecutive_errors: 0,
            total_tool_calls: 50,
            successful_tool_calls: 50,
        }
    }

    fn degraded_indicators() -> HealthIndicators {
        HealthIndicators {
            error_rate: 0.35,
            ..healthy_indicators()
        }
    }

    fn stuck_indicators(now: u64) -> HealthIndicators {
        HealthIndicators {
            error_rate: 0.55,
            context_utilization_pct: 96,
            last_progress_at: Some(now.saturating_sub(400)),
            ..healthy_indicators()
        }
    }

    fn crashed_indicators() -> HealthIndicators {
        HealthIndicators {
            error_rate: 0.85,
            consecutive_errors: 6,
            ..healthy_indicators()
        }
    }

    fn waiting_indicators() -> HealthIndicators {
        HealthIndicators {
            total_tool_calls: 0,
            successful_tool_calls: 0,
            consecutive_errors: 0,
            error_rate: 0.0,
            tool_call_frequency: 0.0,
            context_utilization_pct: 10,
            context_growth_rate: 0.0,
            last_progress_at: None,
        }
    }

    // -- Test 1 ----------------------------------------------------------
    #[test]
    fn new_monitor_starts_healthy() {
        let monitor = HealthMonitor::new(0);
        assert_eq!(monitor.state(), HealthState::Healthy);
    }

    // -- Test 2 ----------------------------------------------------------
    #[test]
    fn single_degraded_tick_stays_healthy_due_to_hysteresis() {
        let mut monitor = HealthMonitor::new(0);
        let report = monitor.check(&degraded_indicators(), 1);
        assert_eq!(report.state, HealthState::Healthy);
        assert!(!report.changed);
        assert!(report.previous_state.is_none());
    }

    // -- Test 3 ----------------------------------------------------------
    #[test]
    fn two_consecutive_degraded_ticks_transition_to_degraded() {
        let mut monitor = HealthMonitor::new(0);
        monitor.check(&degraded_indicators(), 1);
        let report = monitor.check(&degraded_indicators(), 2);
        assert_eq!(report.state, HealthState::Degraded);
        assert!(report.changed);
        assert_eq!(report.previous_state, Some(HealthState::Healthy));
    }

    // -- Test 4 ----------------------------------------------------------
    #[test]
    fn three_healthy_ticks_from_degraded_return_to_healthy() {
        let mut monitor = HealthMonitor::new(0);

        // Drive to Degraded first.
        monitor.check(&degraded_indicators(), 1);
        monitor.check(&degraded_indicators(), 2);
        assert_eq!(monitor.state(), HealthState::Degraded);

        // Three consecutive healthy ticks.
        monitor.check(&healthy_indicators(), 3);
        monitor.check(&healthy_indicators(), 4);
        assert_eq!(monitor.state(), HealthState::Degraded); // not yet
        let report = monitor.check(&healthy_indicators(), 5);
        assert_eq!(report.state, HealthState::Healthy);
        assert!(report.changed);
        assert_eq!(report.previous_state, Some(HealthState::Degraded));
    }

    // -- Test 5 ----------------------------------------------------------
    #[test]
    fn crash_is_immediate_no_hysteresis() {
        let mut monitor = HealthMonitor::new(0);
        let report = monitor.check(&crashed_indicators(), 1);
        assert_eq!(report.state, HealthState::Crashed);
        assert!(report.changed);
        assert_eq!(report.previous_state, Some(HealthState::Healthy));
    }

    // -- Test 6 ----------------------------------------------------------
    #[test]
    fn waiting_for_input_detection() {
        let mut monitor = HealthMonitor::new(0);
        let report = monitor.check(&waiting_indicators(), 1);
        assert_eq!(report.state, HealthState::WaitingForInput);
        assert!(report.changed);
    }

    // -- Test 7 ----------------------------------------------------------
    #[test]
    fn stuck_requires_three_consecutive_stuck_ticks_from_degraded() {
        let mut monitor = HealthMonitor::new(0);

        // First get to Degraded.
        monitor.check(&degraded_indicators(), 1);
        monitor.check(&degraded_indicators(), 2);
        assert_eq!(monitor.state(), HealthState::Degraded);

        // Now three stuck ticks from Degraded.
        let now = 1000;
        monitor.check(&stuck_indicators(now), now);
        monitor.check(&stuck_indicators(now + 1), now + 1);
        assert_eq!(monitor.state(), HealthState::Degraded); // not yet (only 2)
        let report = monitor.check(&stuck_indicators(now + 2), now + 2);
        assert_eq!(report.state, HealthState::Stuck);
        assert!(report.changed);
    }

    // -- Test 8 ----------------------------------------------------------
    #[test]
    fn reset_returns_to_healthy() {
        let mut monitor = HealthMonitor::new(0);
        monitor.check(&crashed_indicators(), 1);
        assert_eq!(monitor.state(), HealthState::Crashed);
        monitor.reset(2);
        assert_eq!(monitor.state(), HealthState::Healthy);
    }

    // -- Test 9 ----------------------------------------------------------
    #[test]
    fn recommendations_include_actionable_suggestions() {
        let recs = compute_recommendations(HealthState::Degraded, &degraded_indicators());
        assert!(!recs.is_empty());
        assert!(recs.iter().any(|r| r.contains("Error rate")));

        let recs = compute_recommendations(HealthState::Crashed, &crashed_indicators());
        assert!(recs.iter().any(|r| r.contains("crashed")));
        assert!(recs.iter().any(|r| r.contains("consecutive errors")));

        let recs = compute_recommendations(HealthState::WaitingForInput, &waiting_indicators());
        assert!(recs.iter().any(|r| r.contains("idle")));
    }

    // -- Test 10 ---------------------------------------------------------
    #[test]
    fn state_transition_records_changed_flag_correctly() {
        let mut monitor = HealthMonitor::new(0);

        // No change — should report changed = false.
        let report = monitor.check(&healthy_indicators(), 1);
        assert!(!report.changed);
        assert!(report.previous_state.is_none());

        // Drive to Degraded — should report changed = true.
        monitor.check(&degraded_indicators(), 2);
        let report = monitor.check(&degraded_indicators(), 3);
        assert!(report.changed);
        assert_eq!(report.previous_state, Some(HealthState::Healthy));

        // Stay Degraded — should report changed = false again.
        let report = monitor.check(&degraded_indicators(), 4);
        assert!(!report.changed);
        assert!(report.previous_state.is_none());
    }

    // -- Test 11 (bonus) -------------------------------------------------
    #[test]
    fn crash_from_consecutive_errors_alone() {
        let mut monitor = HealthMonitor::new(0);
        let ind = HealthIndicators {
            consecutive_errors: 5,
            error_rate: 0.4, // below crash threshold, but consecutive errors suffice
            ..healthy_indicators()
        };
        let report = monitor.check(&ind, 1);
        assert_eq!(report.state, HealthState::Crashed);
        assert!(report.changed);
    }

    // -- Test 12 (bonus) -------------------------------------------------
    #[test]
    fn stuck_to_degraded_requires_two_non_stuck_ticks() {
        let mut monitor = HealthMonitor::new(0);

        // Drive to Degraded, then to Stuck.
        monitor.check(&degraded_indicators(), 1);
        monitor.check(&degraded_indicators(), 2);
        assert_eq!(monitor.state(), HealthState::Degraded);

        let now = 1000;
        monitor.check(&stuck_indicators(now), now);
        monitor.check(&stuck_indicators(now + 1), now + 1);
        monitor.check(&stuck_indicators(now + 2), now + 2);
        assert_eq!(monitor.state(), HealthState::Stuck);

        // One healthy tick is not enough.
        monitor.check(&healthy_indicators(), now + 3);
        assert_eq!(monitor.state(), HealthState::Stuck);

        // Second healthy tick triggers Stuck → Degraded.
        let report = monitor.check(&healthy_indicators(), now + 4);
        assert_eq!(report.state, HealthState::Degraded);
        assert!(report.changed);
        assert_eq!(report.previous_state, Some(HealthState::Stuck));
    }
}
