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

#[cfg(test)]
#[path = "health_monitor/tests.rs"]
mod tests;
