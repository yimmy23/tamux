use super::*;

#[test]
fn default_tracker_is_empty() {
    let tracker = EffectivenessTracker::default();
    assert!(tracker.tools.is_empty());
    assert!(tracker.compositions.is_empty());
    assert_eq!(tracker.max_compositions, 100);
}

#[test]
fn record_tool_call_creates_stats() {
    let mut tracker = EffectivenessTracker::default();
    tracker.record_tool_call("grep", true, 50, 100, 1000);

    assert!(tracker.tools.contains_key("grep"));
    let stats = &tracker.tools["grep"];
    assert_eq!(stats.total_calls, 1);
    assert_eq!(stats.successful_calls, 1);
    assert_eq!(stats.failed_calls, 0);
    assert_eq!(stats.total_duration_ms, 50);
    assert_eq!(stats.total_tokens, 100);
    assert_eq!(stats.last_used_at, 1000);
}

#[test]
fn multiple_calls_update_stats_correctly() {
    let mut tracker = EffectivenessTracker::default();
    tracker.record_tool_call("read", true, 10, 50, 100);
    tracker.record_tool_call("read", false, 20, 60, 200);
    tracker.record_tool_call("read", true, 30, 70, 300);

    let stats = &tracker.tools["read"];
    assert_eq!(stats.total_calls, 3);
    assert_eq!(stats.successful_calls, 2);
    assert_eq!(stats.failed_calls, 1);
    assert_eq!(stats.total_duration_ms, 60);
    assert_eq!(stats.total_tokens, 180);
    assert_eq!(stats.last_used_at, 300);
}

#[test]
fn success_rate_calculation() {
    let mut tracker = EffectivenessTracker::default();
    tracker.record_tool_call("edit", true, 10, 50, 1);
    tracker.record_tool_call("edit", true, 10, 50, 2);
    tracker.record_tool_call("edit", false, 10, 50, 3);
    tracker.record_tool_call("edit", true, 10, 50, 4);

    let rate = tracker.tool_success_rate("edit").unwrap();
    assert!((rate - 0.75).abs() < f64::EPSILON);
    assert!(tracker.tool_success_rate("unknown").is_none());
}

#[test]
fn avg_duration_calculation() {
    let mut tracker = EffectivenessTracker::default();
    tracker.record_tool_call("bash", true, 100, 0, 1);
    tracker.record_tool_call("bash", true, 200, 0, 2);

    let avg = tracker.tool_avg_duration("bash").unwrap();
    assert!((avg - 150.0).abs() < f64::EPSILON);
}

#[test]
fn avg_tokens_calculation() {
    let mut tracker = EffectivenessTracker::default();
    tracker.record_tool_call("write", true, 0, 300, 1);
    tracker.record_tool_call("write", true, 0, 500, 2);

    let avg = tracker.tool_avg_tokens("write").unwrap();
    assert!((avg - 400.0).abs() < f64::EPSILON);
}

#[test]
fn most_effective_tools_sorted_correctly() {
    let mut tracker = EffectivenessTracker::default();

    for i in 0..10 {
        tracker.record_tool_call("good", i < 9, 10, 10, i as u64);
    }
    for i in 0..10 {
        tracker.record_tool_call("ok", i < 6, 10, 10, i as u64);
    }
    for i in 0..10 {
        tracker.record_tool_call("great", true, 10, 10, i as u64);
    }

    let top = tracker.most_effective_tools(3);
    assert_eq!(top.len(), 3);
    assert_eq!(top[0].0, "great");
    assert!((top[0].1 - 1.0).abs() < f64::EPSILON);
    assert_eq!(top[1].0, "good");
    assert!((top[1].1 - 0.9).abs() < f64::EPSILON);
    assert_eq!(top[2].0, "ok");
    assert!((top[2].1 - 0.6).abs() < f64::EPSILON);
}

#[test]
fn least_effective_tools_sorted_correctly() {
    let mut tracker = EffectivenessTracker::default();

    for i in 0..10 {
        tracker.record_tool_call("bad", i < 2, 10, 10, i as u64);
    }
    for i in 0..10 {
        tracker.record_tool_call("worse", i < 1, 10, 10, i as u64);
    }
    for i in 0..10 {
        tracker.record_tool_call("decent", i < 7, 10, 10, i as u64);
    }

    let bottom = tracker.least_effective_tools(2);
    assert_eq!(bottom.len(), 2);
    assert_eq!(bottom[0].0, "worse");
    assert!((bottom[0].1 - 0.1).abs() < f64::EPSILON);
    assert_eq!(bottom[1].0, "bad");
    assert!((bottom[1].1 - 0.2).abs() < f64::EPSILON);
}

#[test]
fn tools_with_fewer_than_5_calls_excluded_from_rankings() {
    let mut tracker = EffectivenessTracker::default();

    for _ in 0..4 {
        tracker.record_tool_call("few", true, 10, 10, 1);
    }
    for _ in 0..5 {
        tracker.record_tool_call("enough", true, 10, 10, 1);
    }

    let top = tracker.most_effective_tools(10);
    assert_eq!(top.len(), 1);
    assert_eq!(top[0].0, "enough");

    let bottom = tracker.least_effective_tools(10);
    assert_eq!(bottom.len(), 1);
    assert_eq!(bottom[0].0, "enough");
}

#[test]
fn composition_recording() {
    let mut tracker = EffectivenessTracker::default();
    let sequence = vec!["read".to_string(), "edit".to_string(), "bash".to_string()];

    tracker.record_composition(&sequence, true, 3, 100);
    tracker.record_composition(&sequence, false, 0, 200);
    tracker.record_composition(&sequence, true, 5, 300);

    assert_eq!(tracker.compositions.len(), 1);
    let composition = &tracker.compositions[0];
    assert_eq!(composition.total_uses, 3);
    assert_eq!(composition.completions, 2);
    assert!((composition.avg_steps_to_success - 4.0).abs() < f64::EPSILON);
    assert_eq!(composition.last_used_at, 300);
}

#[test]
fn composition_completion_rate() {
    let mut tracker = EffectivenessTracker::default();
    let sequence = vec!["a".to_string(), "b".to_string()];

    tracker.record_composition(&sequence, true, 2, 1);
    tracker.record_composition(&sequence, false, 0, 2);
    tracker.record_composition(&sequence, true, 3, 3);
    tracker.record_composition(&sequence, false, 0, 4);

    let rate = tracker.composition_completion_rate(&sequence).unwrap();
    assert!((rate - 0.5).abs() < f64::EPSILON);

    let unknown = vec!["x".to_string()];
    assert!(tracker.composition_completion_rate(&unknown).is_none());
}

#[test]
fn effectiveness_report_includes_tool_names() {
    let mut tracker = EffectivenessTracker::default();
    tracker.record_tool_call("grep", true, 10, 100, 1);
    tracker.record_tool_call("read", false, 20, 200, 2);

    let report = tracker.build_effectiveness_report();
    assert!(report.contains("grep"));
    assert!(report.contains("read"));
    assert!(report.contains("Tool Effectiveness Report"));
}

#[test]
fn composition_eviction_when_at_capacity() {
    let mut tracker = EffectivenessTracker::new(2);

    let seq1 = vec!["a".to_string()];
    let seq2 = vec!["b".to_string()];
    let seq3 = vec!["c".to_string()];

    tracker.record_composition(&seq1, true, 1, 10);
    tracker.record_composition(&seq2, true, 1, 20);
    tracker.record_composition(&seq3, true, 1, 30);

    assert_eq!(tracker.compositions.len(), 2);
    assert!(tracker.composition_completion_rate(&seq1).is_none());
    assert!(tracker.composition_completion_rate(&seq2).is_some());
    assert!(tracker.composition_completion_rate(&seq3).is_some());
}
