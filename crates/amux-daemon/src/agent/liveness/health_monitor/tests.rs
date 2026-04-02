use super::*;

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

#[test]
fn new_monitor_starts_healthy() {
    let monitor = HealthMonitor::new(0);
    assert_eq!(monitor.state(), HealthState::Healthy);
}

#[test]
fn single_degraded_tick_stays_healthy_due_to_hysteresis() {
    let mut monitor = HealthMonitor::new(0);
    let report = monitor.check(&degraded_indicators(), 1);
    assert_eq!(report.state, HealthState::Healthy);
    assert!(!report.changed);
    assert!(report.previous_state.is_none());
}

#[test]
fn two_consecutive_degraded_ticks_transition_to_degraded() {
    let mut monitor = HealthMonitor::new(0);
    monitor.check(&degraded_indicators(), 1);
    let report = monitor.check(&degraded_indicators(), 2);
    assert_eq!(report.state, HealthState::Degraded);
    assert!(report.changed);
    assert_eq!(report.previous_state, Some(HealthState::Healthy));
}

#[test]
fn three_healthy_ticks_from_degraded_return_to_healthy() {
    let mut monitor = HealthMonitor::new(0);

    monitor.check(&degraded_indicators(), 1);
    monitor.check(&degraded_indicators(), 2);
    assert_eq!(monitor.state(), HealthState::Degraded);

    monitor.check(&healthy_indicators(), 3);
    monitor.check(&healthy_indicators(), 4);
    assert_eq!(monitor.state(), HealthState::Degraded);
    let report = monitor.check(&healthy_indicators(), 5);
    assert_eq!(report.state, HealthState::Healthy);
    assert!(report.changed);
    assert_eq!(report.previous_state, Some(HealthState::Degraded));
}

#[test]
fn crash_is_immediate_no_hysteresis() {
    let mut monitor = HealthMonitor::new(0);
    let report = monitor.check(&crashed_indicators(), 1);
    assert_eq!(report.state, HealthState::Crashed);
    assert!(report.changed);
    assert_eq!(report.previous_state, Some(HealthState::Healthy));
}

#[test]
fn waiting_for_input_detection() {
    let mut monitor = HealthMonitor::new(0);
    let report = monitor.check(&waiting_indicators(), 1);
    assert_eq!(report.state, HealthState::WaitingForInput);
    assert!(report.changed);
}

#[test]
fn stuck_requires_three_consecutive_stuck_ticks_from_degraded() {
    let mut monitor = HealthMonitor::new(0);

    monitor.check(&degraded_indicators(), 1);
    monitor.check(&degraded_indicators(), 2);
    assert_eq!(monitor.state(), HealthState::Degraded);

    let now = 1000;
    monitor.check(&stuck_indicators(now), now);
    monitor.check(&stuck_indicators(now + 1), now + 1);
    assert_eq!(monitor.state(), HealthState::Degraded);
    let report = monitor.check(&stuck_indicators(now + 2), now + 2);
    assert_eq!(report.state, HealthState::Stuck);
    assert!(report.changed);
}

#[test]
fn reset_returns_to_healthy() {
    let mut monitor = HealthMonitor::new(0);
    monitor.check(&crashed_indicators(), 1);
    assert_eq!(monitor.state(), HealthState::Crashed);
    monitor.reset(2);
    assert_eq!(monitor.state(), HealthState::Healthy);
}

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

#[test]
fn state_transition_records_changed_flag_correctly() {
    let mut monitor = HealthMonitor::new(0);

    let report = monitor.check(&healthy_indicators(), 1);
    assert!(!report.changed);
    assert!(report.previous_state.is_none());

    monitor.check(&degraded_indicators(), 2);
    let report = monitor.check(&degraded_indicators(), 3);
    assert!(report.changed);
    assert_eq!(report.previous_state, Some(HealthState::Healthy));

    let report = monitor.check(&degraded_indicators(), 4);
    assert!(!report.changed);
    assert!(report.previous_state.is_none());
}

#[test]
fn crash_from_consecutive_errors_alone() {
    let mut monitor = HealthMonitor::new(0);
    let ind = HealthIndicators {
        consecutive_errors: 5,
        error_rate: 0.4,
        ..healthy_indicators()
    };
    let report = monitor.check(&ind, 1);
    assert_eq!(report.state, HealthState::Crashed);
    assert!(report.changed);
}

#[test]
fn stuck_to_degraded_requires_two_non_stuck_ticks() {
    let mut monitor = HealthMonitor::new(0);

    monitor.check(&degraded_indicators(), 1);
    monitor.check(&degraded_indicators(), 2);
    assert_eq!(monitor.state(), HealthState::Degraded);

    let now = 1000;
    monitor.check(&stuck_indicators(now), now);
    monitor.check(&stuck_indicators(now + 1), now + 1);
    monitor.check(&stuck_indicators(now + 2), now + 2);
    assert_eq!(monitor.state(), HealthState::Stuck);

    monitor.check(&healthy_indicators(), now + 3);
    assert_eq!(monitor.state(), HealthState::Stuck);

    let report = monitor.check(&healthy_indicators(), now + 4);
    assert_eq!(report.state, HealthState::Degraded);
    assert!(report.changed);
    assert_eq!(report.previous_state, Some(HealthState::Stuck));
}
