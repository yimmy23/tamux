use super::*;

#[test]
fn cognitive_style_prefers_terse_for_short_messages() {
    assert_eq!(
        verbosity_preference_for_length(6.0),
        VerbosityPreference::Terse
    );
    assert_eq!(reading_depth_for_length(6.0), ReadingDepth::Skim);
}

#[test]
fn risk_metrics_compute_category_rates_and_tolerance() {
    let mut risk = RiskFingerprint {
        approvals: 4,
        denials: 1,
        category_requests: HashMap::from([
            ("git".to_string(), 2),
            ("network_request".to_string(), 3),
        ]),
        category_approvals: HashMap::from([
            ("git".to_string(), 2),
            ("network_request".to_string(), 2),
        ]),
        ..RiskFingerprint::default()
    };

    refresh_risk_metrics(&mut risk);

    assert_eq!(risk.risk_tolerance, RiskTolerance::Aggressive);
    assert_eq!(
        risk.approval_rate_by_category.get("git").copied(),
        Some(1.0)
    );
    assert_eq!(
        risk.approval_rate_by_category
            .get("network_request")
            .copied(),
        Some(2.0 / 3.0)
    );
}

#[test]
fn classify_command_category_uses_command_heuristics_first() {
    assert_eq!(
        classify_command_category("rm -rf target", "highest"),
        "destructive_delete"
    );
    assert_eq!(
        classify_command_category("curl https://example.com", "moderate"),
        "network_request"
    );
}

#[test]
fn normalize_attention_surface_keeps_supported_separators() {
    assert_eq!(
        normalize_attention_surface(" modal:settings:SubAgents "),
        "modal:settings:subagents"
    );
}

#[test]
fn top_keys_orders_by_count_then_name() {
    let mut histogram = HashMap::new();
    histogram.insert("conversation:chat".to_string(), 4);
    histogram.insert("conversation:input".to_string(), 4);
    histogram.insert("modal:settings:provider".to_string(), 1);

    assert_eq!(
        top_keys(&histogram, 2),
        vec![
            "conversation:chat".to_string(),
            "conversation:input".to_string()
        ]
    );
}

#[test]
fn detect_revision_signal_finds_corrections() {
    assert_eq!(
        detect_revision_signal("Actually, use ripgrep instead."),
        RevisionSignal::Correction
    );
    assert_eq!(
        detect_revision_signal("Next time prefer the shorter answer."),
        RevisionSignal::Revision
    );
    assert_eq!(
        detect_revision_signal("Please run tests."),
        RevisionSignal::None
    );
}

#[test]
fn ema_update_basic_calculation() {
    let result = ema_update(10.0, 20.0, 0.3);
    assert!((result - 13.0).abs() < f64::EPSILON);
}

#[test]
fn ema_update_converges_toward_sample() {
    let mut value = 0.0;
    for _ in 0..50 {
        value = ema_update(value, 100.0, 0.3);
    }
    assert!((value - 100.0).abs() < 0.01);
}

#[test]
fn smooth_activity_histogram_applies_ema_to_all_24_hours() {
    let current: HashMap<u8, f64> = HashMap::new();
    let mut observation: HashMap<u8, u64> = HashMap::new();
    observation.insert(9, 5);
    observation.insert(14, 3);

    let smoothed = smooth_activity_histogram(&current, &observation, 0.3);
    assert_eq!(smoothed.len(), 24);
    assert!((smoothed[&9] - 1.5).abs() < f64::EPSILON);
    assert!((smoothed[&14] - 0.9).abs() < f64::EPSILON);
    assert!((smoothed[&0]).abs() < f64::EPSILON);
}

#[test]
fn smooth_activity_histogram_decays_unobserved_hours() {
    let mut current: HashMap<u8, f64> = HashMap::new();
    current.insert(9, 10.0);
    let observation: HashMap<u8, u64> = HashMap::new();

    let smoothed = smooth_activity_histogram(&current, &observation, 0.3);
    assert!((smoothed[&9] - 7.0).abs() < f64::EPSILON);
}

#[test]
fn record_attention_event_tracks_dwell_and_rapid_switches() {
    let mut model = OperatorModel::default();
    record_attention_event(&mut model, "conversation:chat", 1_000);
    record_attention_event(&mut model, "modal:settings", 6_000);
    record_attention_event(&mut model, "conversation:chat", 10_000);
    record_attention_event(&mut model, "conversation:chat", 50_000);

    assert_eq!(model.attention_topology.focus_event_count, 4);
    assert_eq!(model.attention_topology.rapid_switch_count, 2);
    assert_eq!(
        model.attention_topology.deep_focus_surface.as_deref(),
        Some("conversation:chat")
    );
    assert!(model.attention_topology.avg_surface_dwell_secs > 0.0);
}
