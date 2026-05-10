use super::*;

/// Helper — build a healthy snapshot with sensible defaults.
fn healthy_snapshot() -> DetectionSnapshot {
    DetectionSnapshot {
        entity_id: "entity-1".into(),
        entity_type: "task".into(),
        last_progress_at: Some(1000),
        started_at: 900,
        max_duration_secs: Some(600),
        consecutive_errors: 0,
        total_errors: 0,
        total_tool_calls: 10,
        recent_tool_names: vec!["read".into(), "write".into(), "grep".into()],
        context_utilization_pct: 40,
    }
}


#[test]
fn healthy_snapshot_returns_none() {
    let detector = StuckDetector::default();
    let snap = healthy_snapshot();
    let result = detector.analyze(&snap, 1010);
    assert!(result.is_none(), "healthy snapshot should return None");
}


#[test]
fn timeout_detected_when_exceeding_max_duration() {
    let detector = StuckDetector::default();
    let mut snap = healthy_snapshot();
    snap.started_at = 100;
    snap.max_duration_secs = Some(200);
    snap.last_progress_at = Some(350);
    snap.consecutive_errors = 0;
    snap.context_utilization_pct = 20;
    snap.recent_tool_names = vec!["a".into(), "b".into(), "c".into()];

    let now = 400;
    let analysis = detector.analyze(&snap, now).unwrap();
    assert_eq!(analysis.reason, StuckReason::Timeout);
    assert!(analysis.confidence >= 0.8);
    assert!(analysis.evidence.contains("300"));
    assert!(analysis.evidence.contains("200"));
}


#[test]
fn no_progress_detected_when_idle_exceeds_threshold() {
    let detector = StuckDetector::default();
    let mut snap = healthy_snapshot();
    snap.last_progress_at = Some(100);
    snap.max_duration_secs = None;
    snap.consecutive_errors = 0;
    snap.context_utilization_pct = 20;
    snap.recent_tool_names = vec!["a".into(), "b".into(), "c".into()];

    let now = 500;
    let analysis = detector.analyze(&snap, now).unwrap();
    assert_eq!(analysis.reason, StuckReason::NoProgress);
    assert!(analysis.evidence.contains("400"));
}

#[test]
fn no_progress_detected_with_no_last_progress() {
    let detector = StuckDetector::default();
    let mut snap = healthy_snapshot();
    snap.last_progress_at = None;
    snap.started_at = 100;
    snap.max_duration_secs = None;
    snap.consecutive_errors = 0;
    snap.context_utilization_pct = 20;
    snap.recent_tool_names = vec!["a".into(), "b".into(), "c".into()];

    let now = 500;
    let analysis = detector.analyze(&snap, now).unwrap();
    assert_eq!(analysis.reason, StuckReason::NoProgress);
}


#[test]
fn error_loop_detected_with_3_consecutive_errors() {
    let detector = StuckDetector::default();
    let mut snap = healthy_snapshot();
    snap.consecutive_errors = 3;
    snap.total_errors = 3;

    let analysis = detector.analyze(&snap, 1010).unwrap();
    assert_eq!(analysis.reason, StuckReason::ErrorLoop);
    assert!(analysis.evidence.contains("3 consecutive errors"));
}


#[test]
fn error_loop_not_detected_with_2_consecutive_errors() {
    let detector = StuckDetector::default();
    let mut snap = healthy_snapshot();
    snap.consecutive_errors = 2;
    snap.total_errors = 2;

    let result = detector.analyze(&snap, 1010);
    assert!(
        result.is_none(),
        "2 consecutive errors should not trigger detection"
    );
}


#[test]
fn tool_loop_detected_with_abab_pattern() {
    let detector = StuckDetector::default();
    let mut snap = healthy_snapshot();
    snap.recent_tool_names = vec!["read".into(), "write".into(), "read".into(), "write".into()];

    let analysis = detector.analyze(&snap, 1010).unwrap();
    assert_eq!(analysis.reason, StuckReason::ToolCallLoop);
    assert!(analysis.evidence.contains("loop"));
    assert!(analysis.evidence.contains("read"));
    assert!(analysis.evidence.contains("write"));
}


#[test]
fn tool_loop_detected_with_aaaa_pattern() {
    let detector = StuckDetector::default();
    let mut snap = healthy_snapshot();
    snap.recent_tool_names = vec!["read".into(), "read".into(), "read".into(), "read".into()];

    let analysis = detector.analyze(&snap, 1010).unwrap();
    assert_eq!(analysis.reason, StuckReason::ToolCallLoop);
    assert!(analysis.evidence.contains("read"));
}


#[test]
fn resource_exhaustion_detected_at_91_percent() {
    let detector = StuckDetector::default();
    let mut snap = healthy_snapshot();
    snap.context_utilization_pct = 91;

    let analysis = detector.analyze(&snap, 1010).unwrap();
    assert_eq!(analysis.reason, StuckReason::ResourceExhaustion);
    assert!(analysis.evidence.contains("91%"));
}


#[test]
fn resource_exhaustion_not_detected_at_89_percent() {
    let detector = StuckDetector::default();
    let mut snap = healthy_snapshot();
    snap.context_utilization_pct = 89;

    let result = detector.analyze(&snap, 1010);
    assert!(
        result.is_none(),
        "89% should not trigger resource exhaustion"
    );
}


#[test]
fn multiple_issues_highest_confidence_wins() {
    let detector = StuckDetector::default();
    let mut snap = healthy_snapshot();
    snap.started_at = 100;
    snap.max_duration_secs = Some(50);
    snap.consecutive_errors = 3;
    snap.last_progress_at = Some(1000);
    snap.context_utilization_pct = 20;
    snap.recent_tool_names = vec!["a".into(), "b".into(), "c".into()];

    let now = 400;
    let analysis = detector.analyze(&snap, now).unwrap();
    assert_eq!(analysis.reason, StuckReason::Timeout);
    assert!(analysis.confidence > 0.9);
}


#[test]
fn default_thresholds_are_reasonable() {
    let detector = StuckDetector::default();
    assert_eq!(detector.no_progress_timeout_secs, 300);
    assert_eq!(detector.error_loop_threshold, 3);
    assert_eq!(detector.tool_loop_min_length, 4);
    assert_eq!(detector.resource_exhaustion_pct, 90);
}


#[test]
fn custom_thresholds_work() {
    let detector = StuckDetector {
        no_progress_timeout_secs: 60,
        error_loop_threshold: 5,
        tool_loop_min_length: 6,
        resource_exhaustion_pct: 80,
    };

    let mut snap = healthy_snapshot();
    snap.consecutive_errors = 3;
    assert!(
        detector.analyze(&snap, 1010).is_none(),
        "3 errors with threshold 5 should not trigger"
    );

    snap.consecutive_errors = 5;
    let analysis = detector.analyze(&snap, 1010).unwrap();
    assert_eq!(analysis.reason, StuckReason::ErrorLoop);

    let mut snap2 = healthy_snapshot();
    snap2.context_utilization_pct = 85;
    snap2.consecutive_errors = 0;
    let analysis2 = detector.analyze(&snap2, 1010).unwrap();
    assert_eq!(analysis2.reason, StuckReason::ResourceExhaustion);

    let mut snap3 = healthy_snapshot();
    snap3.last_progress_at = Some(900);
    snap3.max_duration_secs = None;
    snap3.consecutive_errors = 0;
    snap3.context_utilization_pct = 20;
    snap3.recent_tool_names = vec!["a".into(), "b".into(), "c".into()];
    let now = 970;
    let analysis3 = detector.analyze(&snap3, now).unwrap();
    assert_eq!(analysis3.reason, StuckReason::NoProgress);
}


#[test]
fn intervention_selection_maps_correctly() {
    assert_eq!(
        suggest_intervention(StuckReason::Timeout, 0.9),
        InterventionAction::EscalateToUser
    );

    assert_eq!(
        suggest_intervention(StuckReason::ResourceExhaustion, 0.8),
        InterventionAction::CompressContext
    );

    assert_eq!(
        suggest_intervention(StuckReason::ToolCallLoop, 0.95),
        InterventionAction::EscalateToParent
    );

    assert_eq!(
        suggest_intervention(StuckReason::ToolCallLoop, 0.6),
        InterventionAction::SelfAssess
    );

    assert_eq!(
        suggest_intervention(StuckReason::ErrorLoop, 0.95),
        InterventionAction::RetryFromCheckpoint
    );

    assert_eq!(
        suggest_intervention(StuckReason::ErrorLoop, 0.7),
        InterventionAction::CompressContext
    );

    assert_eq!(
        suggest_intervention(StuckReason::NoProgress, 0.9),
        InterventionAction::RetryFromCheckpoint
    );

    assert_eq!(
        suggest_intervention(StuckReason::NoProgress, 0.5),
        InterventionAction::SelfAssess
    );
}


#[test]
fn entity_type_preserved_in_analysis() {
    let detector = StuckDetector::default();

    let mut snap = healthy_snapshot();
    snap.entity_type = "task".into();
    snap.consecutive_errors = 5;
    let analysis = detector.analyze(&snap, 1010).unwrap();
    assert_eq!(analysis.entity_type, "task");
    assert_eq!(analysis.entity_id, "entity-1");

    snap.entity_type = "goal_run".into();
    snap.entity_id = "goal-42".into();
    let analysis = detector.analyze(&snap, 1010).unwrap();
    assert_eq!(analysis.entity_type, "goal_run");
    assert_eq!(analysis.entity_id, "goal-42");
}


#[test]
fn evidence_strings_are_descriptive() {
    let detector = StuckDetector::default();

    let mut snap = healthy_snapshot();
    snap.started_at = 100;
    snap.max_duration_secs = Some(200);
    snap.last_progress_at = Some(350);
    snap.consecutive_errors = 0;
    snap.context_utilization_pct = 20;
    snap.recent_tool_names = vec!["a".into(), "b".into(), "c".into()];
    let analysis = detector.analyze(&snap, 400).unwrap();
    assert!(
        analysis.evidence.contains("elapsed") && analysis.evidence.contains("max_duration"),
        "timeout evidence should mention elapsed and max_duration: {}",
        analysis.evidence
    );

    let mut snap2 = healthy_snapshot();
    snap2.consecutive_errors = 4;
    let analysis2 = detector.analyze(&snap2, 1010).unwrap();
    assert!(
        analysis2.evidence.contains("4 consecutive errors"),
        "error loop evidence should mention count: {}",
        analysis2.evidence
    );

    let mut snap3 = healthy_snapshot();
    snap3.context_utilization_pct = 95;
    snap3.consecutive_errors = 0;
    let analysis3 = detector.analyze(&snap3, 1010).unwrap();
    assert!(
        analysis3.evidence.contains("95%"),
        "resource exhaustion evidence should mention percentage: {}",
        analysis3.evidence
    );

    let mut snap4 = healthy_snapshot();
    snap4.recent_tool_names = vec!["bash".into(), "grep".into(), "bash".into(), "grep".into()];
    snap4.consecutive_errors = 0;
    snap4.context_utilization_pct = 20;
    let analysis4 = detector.analyze(&snap4, 1010).unwrap();
    assert!(
        analysis4.evidence.contains("bash") && analysis4.evidence.contains("grep"),
        "tool loop evidence should mention tool names: {}",
        analysis4.evidence
    );

    let mut snap5 = healthy_snapshot();
    snap5.last_progress_at = Some(100);
    snap5.max_duration_secs = None;
    snap5.consecutive_errors = 0;
    snap5.context_utilization_pct = 20;
    snap5.recent_tool_names = vec!["a".into(), "b".into(), "c".into()];
    let analysis5 = detector.analyze(&snap5, 500).unwrap();
    assert!(
        analysis5.evidence.contains("no progress") && analysis5.evidence.contains("threshold"),
        "no progress evidence should mention idle time and threshold: {}",
        analysis5.evidence
    );
}


#[test]
fn tool_loop_not_detected_with_short_sequence() {
    let detector = StuckDetector::default();
    let mut snap = healthy_snapshot();
    snap.recent_tool_names = vec!["read".into(), "write".into(), "read".into()];
    let result = detector.analyze(&snap, 1010);
    assert!(result.is_none(), "3 entries should not trigger tool loop");
}


#[test]
fn timeout_not_triggered_without_max_duration() {
    let snap = DetectionSnapshot {
        entity_id: "e1".into(),
        entity_type: "task".into(),
        last_progress_at: Some(1000),
        started_at: 0,
        max_duration_secs: None,
        consecutive_errors: 0,
        total_errors: 0,
        total_tool_calls: 10,
        recent_tool_names: vec!["a".into(), "b".into(), "c".into()],
        context_utilization_pct: 20,
    };
    let result = detect_timeout(&snap, 999_999);
    assert!(
        result.is_none(),
        "no max_duration_secs should mean no timeout"
    );
}


#[test]
fn confidence_values_in_valid_range() {
    let detector = StuckDetector::default();

    let mut snap = healthy_snapshot();
    snap.started_at = 0;
    snap.max_duration_secs = Some(1);
    snap.last_progress_at = Some(999_998);
    snap.consecutive_errors = 0;
    snap.context_utilization_pct = 20;
    snap.recent_tool_names = vec!["a".into(), "b".into(), "c".into()];
    let analysis = detector.analyze(&snap, 999_999).unwrap();
    assert!(
        analysis.confidence >= 0.0 && analysis.confidence <= 1.0,
        "confidence should be 0.0..=1.0, got {}",
        analysis.confidence
    );

    let mut snap2 = healthy_snapshot();
    snap2.context_utilization_pct = 100;
    snap2.consecutive_errors = 0;
    let analysis2 = detector.analyze(&snap2, 1010).unwrap();
    assert!(
        analysis2.confidence >= 0.0 && analysis2.confidence <= 1.0,
        "confidence should be 0.0..=1.0, got {}",
        analysis2.confidence
    );
}


#[test]
fn resource_exhaustion_not_at_exactly_threshold() {
    let detector = StuckDetector::default();
    let mut snap = healthy_snapshot();
    snap.context_utilization_pct = 90;
    let result = detector.analyze(&snap, 1010);
    assert!(
        result.is_none(),
        "exactly at threshold (90%) should not trigger"
    );
}
