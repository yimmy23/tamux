/// Default memory fact decay half-life in hours (~69h, per D-04).
#[allow(dead_code)]
pub(crate) const DEFAULT_HALF_LIFE_HOURS: f64 = 69.0;

/// Default idle threshold in milliseconds (5 minutes, per D-01).
#[allow(dead_code)]
pub(crate) const DEFAULT_IDLE_THRESHOLD_MS: u64 = 5 * 60 * 1000;

/// Default consolidation tick budget in seconds (per D-02).
#[allow(dead_code)]
pub(crate) const DEFAULT_BUDGET_SECS: u64 = 30;

/// Check all four idle conditions required for consolidation per D-01.
/// Returns true only when ALL conditions are simultaneously met:
/// no active tasks, no active goal runs, no active streams, and operator idle > threshold.
pub(crate) fn is_idle_for_consolidation(
    active_task_count: usize,
    running_goal_count: usize,
    active_stream_count: usize,
    last_presence_at: Option<u64>,
    now: u64,
    idle_threshold_ms: u64,
) -> bool {
    if active_task_count > 0 || running_goal_count > 0 || active_stream_count > 0 {
        return false;
    }
    match last_presence_at {
        Some(last) => now.saturating_sub(last) >= idle_threshold_ms,
        None => true,
    }
}

/// Compute current confidence for a memory fact based on exponential decay.
/// Returns a value in 0.0..=1.0. A fact confirmed exactly `half_life_hours` ago
/// will have confidence ~0.5. Active facts (recently confirmed) stay near 1.0.
pub(crate) fn compute_decay_confidence(
    last_confirmed_at: u64,
    now: u64,
    half_life_hours: f64,
) -> f64 {
    if last_confirmed_at == 0 || half_life_hours <= 0.0 {
        return 0.0;
    }
    let age_ms = now.saturating_sub(last_confirmed_at) as f64;
    let age_hours = age_ms / 3_600_000.0;
    let lambda = 2.0_f64.ln() / half_life_hours;
    let confidence = (-lambda * age_hours).exp();
    confidence.clamp(0.0, 1.0)
}
