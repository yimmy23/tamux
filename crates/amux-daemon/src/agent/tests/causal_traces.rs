use super::*;

#[test]
fn estimated_success_probability_defaults_when_no_history() {
    assert!((estimated_success_probability(0, 0, false) - 0.65).abs() < f64::EPSILON);
    assert!((estimated_success_probability(0, 0, true) - 0.35).abs() < f64::EPSILON);
}

#[test]
fn plan_success_estimate_decreases_with_complexity() {
    assert!(estimate_plan_success(2, 0) > estimate_plan_success(6, 3));
}

#[test]
fn command_family_normalizes_prefix() {
    assert_eq!(command_family("git push origin main"), "git_push");
    assert_eq!(command_family("rm -rf build"), "rm__rf");
}

#[test]
fn summarize_outcome_preserves_recovery_for_near_miss() {
    let summary = summarize_outcome(
        crate::agent::learning::traces::CausalTraceOutcome::NearMiss {
            what_went_wrong: "command timed out".to_string(),
            how_recovered: "replanned into smaller steps".to_string(),
        },
    )
    .expect("near miss should summarize");

    assert!(summary.is_near_miss);
    assert_eq!(summary.reason, "command timed out");
    assert_eq!(
        summary.recovery.as_deref(),
        Some("replanned into smaller steps")
    );
}

#[test]
fn family_outcome_summary_tracks_failures_and_near_misses() {
    let mut summary = FamilyOutcomeSummary::default();
    summary.record(OutcomeSummary {
        reason: "permissions denied".to_string(),
        recovery: None,
        is_near_miss: false,
    });
    summary.record(OutcomeSummary {
        reason: "command timed out".to_string(),
        recovery: Some("replanned into smaller steps".to_string()),
        is_near_miss: true,
    });

    assert_eq!(summary.failure_count, 1);
    assert_eq!(summary.near_miss_count, 1);
    assert_eq!(summary.reasons.len(), 2);
    assert_eq!(summary.recoveries, vec!["replanned into smaller steps"]);
}
