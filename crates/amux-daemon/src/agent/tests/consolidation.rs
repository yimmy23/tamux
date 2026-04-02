use super::*;

#[test]
fn idle_returns_true_when_all_conditions_met() {
    assert!(is_idle_for_consolidation(
        0,
        0,
        0,
        Some(1000),
        1000 + DEFAULT_IDLE_THRESHOLD_MS,
        DEFAULT_IDLE_THRESHOLD_MS,
    ));
}

#[test]
fn idle_returns_false_with_active_task() {
    assert!(!is_idle_for_consolidation(
        1,
        0,
        0,
        Some(0),
        DEFAULT_IDLE_THRESHOLD_MS + 1,
        DEFAULT_IDLE_THRESHOLD_MS,
    ));
}

#[test]
fn idle_returns_false_with_active_goal_run() {
    assert!(!is_idle_for_consolidation(
        0,
        1,
        0,
        Some(0),
        DEFAULT_IDLE_THRESHOLD_MS + 1,
        DEFAULT_IDLE_THRESHOLD_MS,
    ));
}

#[test]
fn idle_returns_false_with_active_stream() {
    assert!(!is_idle_for_consolidation(
        0,
        0,
        1,
        Some(0),
        DEFAULT_IDLE_THRESHOLD_MS + 1,
        DEFAULT_IDLE_THRESHOLD_MS,
    ));
}

#[test]
fn idle_returns_false_with_recent_presence() {
    assert!(!is_idle_for_consolidation(
        0,
        0,
        0,
        Some(10_000),
        10_001,
        DEFAULT_IDLE_THRESHOLD_MS,
    ));
}

#[test]
fn idle_returns_true_when_no_presence_recorded() {
    assert!(is_idle_for_consolidation(
        0,
        0,
        0,
        None,
        1000,
        DEFAULT_IDLE_THRESHOLD_MS,
    ));
}

#[test]
fn decay_returns_half_at_half_life() {
    let now = 1_000_000_000u64;
    let half_life_ms = (DEFAULT_HALF_LIFE_HOURS * 3_600_000.0) as u64;
    let last_confirmed = now - half_life_ms;
    let confidence = compute_decay_confidence(last_confirmed, now, DEFAULT_HALF_LIFE_HOURS);
    assert!(
        (confidence - 0.5).abs() < 0.01,
        "expected ~0.5, got {confidence}"
    );
}

#[test]
fn decay_returns_near_one_for_just_confirmed() {
    let now = 1_000_000_000u64;
    let confidence = compute_decay_confidence(now, now, DEFAULT_HALF_LIFE_HOURS);
    assert!(
        (confidence - 1.0).abs() < 0.001,
        "expected ~1.0, got {confidence}"
    );
}

#[test]
fn decay_returns_zero_for_zero_timestamp() {
    let confidence = compute_decay_confidence(0, 1_000_000, DEFAULT_HALF_LIFE_HOURS);
    assert_eq!(confidence, 0.0);
}

#[test]
fn decay_returns_zero_for_nonpositive_half_life() {
    let confidence = compute_decay_confidence(500_000, 1_000_000, 0.0);
    assert_eq!(confidence, 0.0);
    let confidence = compute_decay_confidence(500_000, 1_000_000, -5.0);
    assert_eq!(confidence, 0.0);
}

#[test]
fn decay_clamps_to_valid_range() {
    let c1 = compute_decay_confidence(1, 2, DEFAULT_HALF_LIFE_HOURS);
    assert!((0.0..=1.0).contains(&c1));

    let c2 = compute_decay_confidence(1, u64::MAX / 2, DEFAULT_HALF_LIFE_HOURS);
    assert!((0.0..=1.0).contains(&c2));
}

#[test]
fn decay_handles_very_large_age_without_panic() {
    let confidence = compute_decay_confidence(0, 5_000_000_000, DEFAULT_HALF_LIFE_HOURS);
    assert_eq!(confidence, 0.0);

    let confidence = compute_decay_confidence(1, 5_000_000_000, DEFAULT_HALF_LIFE_HOURS);
    assert!((0.0..=1.0).contains(&confidence));
    assert!(confidence < 0.001);
}
