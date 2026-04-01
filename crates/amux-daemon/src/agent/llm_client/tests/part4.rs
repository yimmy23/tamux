#[test]
fn retry_delay_scales_with_attempt_multiplier() {
    assert_eq!(compute_retry_delay_ms_for_attempt(5_000, 1), 5_000);
    assert_eq!(compute_retry_delay_ms_for_attempt(5_000, 2), 10_000);
    assert_eq!(compute_retry_delay_ms_for_attempt(5_000, 3), 15_000);
}

#[test]
fn retry_delay_caps_at_one_minute() {
    assert_eq!(compute_retry_delay_ms_for_attempt(5_000, 20), 60_000);
}