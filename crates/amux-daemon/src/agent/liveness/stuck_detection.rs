//! Stuck detection — pattern-based analysis for identifying stuck agents.
//!
//! This module generalizes the Phase 1 supervisor patterns for use across all
//! tasks and goal runs (not just sub-agents).  It provides a [`StuckDetector`]
//! with configurable thresholds that analyses a [`DetectionSnapshot`] and
//! returns the highest-confidence [`StuckAnalysis`] when a problem is found.

use crate::agent::types::{InterventionAction, StuckReason};

// ---------------------------------------------------------------------------
// DetectionSnapshot — input to the detector
// ---------------------------------------------------------------------------

/// A point-in-time view of an entity's runtime metrics, used by the
/// [`StuckDetector`] to decide whether the entity is stuck.
#[derive(Debug, Clone)]
pub struct DetectionSnapshot {
    /// Unique identifier for the entity being monitored.
    pub entity_id: String,
    /// The kind of entity: `"task"` or `"goal_run"`.
    pub entity_type: String,
    /// Unix timestamp of the most recent progress event, if any.
    pub last_progress_at: Option<u64>,
    /// Unix timestamp when the entity started.
    pub started_at: u64,
    /// Optional hard deadline in seconds from `started_at`.
    pub max_duration_secs: Option<u64>,
    /// Number of consecutive errors (resets on success).
    pub consecutive_errors: u32,
    /// Total number of errors encountered so far.
    pub total_errors: u32,
    /// Total number of tool calls made so far.
    pub total_tool_calls: u32,
    /// The most recent tool names invoked, in order (newest last).
    pub recent_tool_names: Vec<String>,
    /// Percentage of the context window currently consumed (0–100).
    pub context_utilization_pct: u32,
}

// ---------------------------------------------------------------------------
// StuckAnalysis — output of the detector
// ---------------------------------------------------------------------------

/// Describes a detected stuck condition with confidence and suggested action.
#[derive(Debug, Clone)]
pub struct StuckAnalysis {
    /// The entity that is stuck.
    pub entity_id: String,
    /// The kind of entity: `"task"` or `"goal_run"`.
    pub entity_type: String,
    /// Why the entity is considered stuck.
    pub reason: StuckReason,
    /// Confidence in the diagnosis, from 0.0 (uncertain) to 1.0 (certain).
    pub confidence: f64,
    /// Recommended intervention action.
    pub suggested_intervention: InterventionAction,
    /// Human-readable evidence explaining the diagnosis.
    pub evidence: String,
}

// ---------------------------------------------------------------------------
// StuckDetector — configurable detector
// ---------------------------------------------------------------------------

/// A stuck detector with configurable thresholds.
///
/// Use [`StuckDetector::default()`] for reasonable defaults, or construct with
/// custom thresholds for specific use-cases.
#[derive(Debug, Clone)]
pub struct StuckDetector {
    /// Seconds without progress before flagging NoProgress (default: 300).
    pub no_progress_timeout_secs: u64,
    /// Consecutive error count that triggers ErrorLoop (default: 3).
    pub error_loop_threshold: u32,
    /// Minimum number of recent tool names required to detect a loop (default: 4).
    pub tool_loop_min_length: usize,
    /// Context utilization percentage above which ResourceExhaustion fires (default: 90).
    pub resource_exhaustion_pct: u32,
}

impl Default for StuckDetector {
    fn default() -> Self {
        Self {
            no_progress_timeout_secs: 300,
            error_loop_threshold: 3,
            tool_loop_min_length: 4,
            resource_exhaustion_pct: 90,
        }
    }
}

impl StuckDetector {
    /// Analyse a snapshot and return the highest-confidence stuck analysis,
    /// or `None` if the entity appears healthy.
    pub fn analyze(&self, snapshot: &DetectionSnapshot, now: u64) -> Option<StuckAnalysis> {
        // Collect all detected issues with their confidence scores.
        let mut candidates: Vec<(StuckReason, f64, String)> = Vec::new();

        if let Some((conf, evidence)) = detect_timeout(snapshot, now) {
            candidates.push((StuckReason::Timeout, conf, evidence));
        }
        if let Some((conf, evidence)) =
            detect_no_progress(snapshot, self.no_progress_timeout_secs, now)
        {
            candidates.push((StuckReason::NoProgress, conf, evidence));
        }
        if let Some((conf, evidence)) = detect_error_loop(snapshot, self.error_loop_threshold) {
            candidates.push((StuckReason::ErrorLoop, conf, evidence));
        }
        if let Some((conf, evidence)) = detect_tool_loop(snapshot, self.tool_loop_min_length) {
            candidates.push((StuckReason::ToolCallLoop, conf, evidence));
        }
        if let Some((conf, evidence)) =
            detect_resource_exhaustion(snapshot, self.resource_exhaustion_pct)
        {
            candidates.push((StuckReason::ResourceExhaustion, conf, evidence));
        }

        // Pick the highest confidence issue.
        candidates.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        let (reason, confidence, evidence) = candidates.into_iter().next()?;

        Some(StuckAnalysis {
            entity_id: snapshot.entity_id.clone(),
            entity_type: snapshot.entity_type.clone(),
            reason,
            confidence,
            suggested_intervention: suggest_intervention(reason, confidence),
            evidence,
        })
    }
}

// ---------------------------------------------------------------------------
// Public utility — shared cycle detection
// ---------------------------------------------------------------------------

/// Check whether the recent tool names contain a repeating cycle (period 1 or 2)
/// of at least `min_length` entries.
///
/// Returns `true` when a loop is detected, `false` otherwise.  This is the
/// shared implementation used by both the [`StuckDetector`] and the sub-agent
/// supervisor.
pub fn has_tool_call_loop(recent: &[String], min_length: usize) -> bool {
    if recent.len() < min_length {
        return false;
    }

    for period in 1..=2 {
        let check_len = std::cmp::max(min_length, 2 * period);
        if recent.len() < check_len {
            continue;
        }

        let tail = &recent[recent.len() - check_len..];
        let is_repeating = tail
            .iter()
            .enumerate()
            .all(|(i, name)| *name == tail[i % period]);

        if is_repeating {
            return true;
        }
    }

    false
}

/// Same as [`has_tool_call_loop`] but returns a human-readable evidence string
/// describing the detected pattern, or `None` when no loop is found.
pub fn detect_tool_call_loop_evidence(recent: &[String], min_length: usize) -> Option<String> {
    if recent.len() < min_length {
        return None;
    }

    for period in 1..=2 {
        let check_len = std::cmp::max(min_length, 2 * period);
        if recent.len() < check_len {
            continue;
        }

        let tail = &recent[recent.len() - check_len..];
        let is_repeating = tail
            .iter()
            .enumerate()
            .all(|(i, name)| *name == tail[i % period]);

        if is_repeating {
            let pattern: Vec<&str> = tail[..period].iter().map(|s| s.as_str()).collect();
            let repetitions = check_len / period;
            return Some(format!(
                "tool call loop detected: [{}] repeated {} times",
                pattern.join(" -> "),
                repetitions
            ));
        }
    }

    None
}

// ---------------------------------------------------------------------------
// Detection helpers (private)
// ---------------------------------------------------------------------------

/// Detect whether the entity has exceeded its `max_duration_secs` deadline.
fn detect_timeout(snapshot: &DetectionSnapshot, now: u64) -> Option<(f64, String)> {
    let max_dur = snapshot.max_duration_secs?;
    let elapsed = now.saturating_sub(snapshot.started_at);
    if elapsed > max_dur {
        // Confidence scales with how far past the deadline we are.
        let overshoot_ratio = (elapsed - max_dur) as f64 / max_dur as f64;
        let confidence = (0.8 + 0.2 * overshoot_ratio.min(1.0)).min(1.0);
        Some((
            confidence,
            format!(
                "elapsed {}s exceeds max_duration {}s (overshoot {:.0}%)",
                elapsed,
                max_dur,
                overshoot_ratio * 100.0
            ),
        ))
    } else {
        None
    }
}

/// Detect whether the entity has made no progress within the threshold.
fn detect_no_progress(
    snapshot: &DetectionSnapshot,
    threshold_secs: u64,
    now: u64,
) -> Option<(f64, String)> {
    let idle_secs = match snapshot.last_progress_at {
        Some(ts) => now.saturating_sub(ts),
        None => now.saturating_sub(snapshot.started_at),
    };
    if idle_secs >= threshold_secs {
        // Confidence grows as idle time exceeds the threshold.
        let ratio = idle_secs as f64 / threshold_secs as f64;
        let confidence = (0.5 + 0.5 * (ratio - 1.0).min(1.0)).min(1.0);
        Some((
            confidence,
            format!(
                "no progress for {}s (threshold {}s)",
                idle_secs, threshold_secs
            ),
        ))
    } else {
        None
    }
}

/// Detect whether the entity is in an error loop.
fn detect_error_loop(snapshot: &DetectionSnapshot, threshold: u32) -> Option<(f64, String)> {
    if snapshot.consecutive_errors >= threshold {
        // Confidence based on how many errors above threshold.
        let excess = (snapshot.consecutive_errors - threshold) as f64;
        let confidence = (0.7 + 0.1 * excess.min(3.0)).min(1.0);
        Some((
            confidence,
            format!(
                "{} consecutive errors (threshold {})",
                snapshot.consecutive_errors, threshold
            ),
        ))
    } else {
        None
    }
}

/// Detect whether recent tool calls form a repeating loop.
///
/// Delegates to the shared [`detect_tool_call_loop_evidence`] utility and
/// adds a confidence score based on the repetition length.
fn detect_tool_loop(snapshot: &DetectionSnapshot, min_length: usize) -> Option<(f64, String)> {
    let names = &snapshot.recent_tool_names;
    let evidence = detect_tool_call_loop_evidence(names, min_length)?;

    // Compute confidence from the number of repetitions.
    let check_len = std::cmp::max(min_length, 2);
    let repetitions = if names.len() >= check_len {
        // Period detection: try period 1 first, then 2.
        let mut reps = 2usize;
        for period in 1..=2 {
            let cl = std::cmp::max(min_length, 2 * period);
            if names.len() >= cl {
                reps = cl / period;
                break;
            }
        }
        reps
    } else {
        2
    };
    let confidence = (0.6 + 0.1 * (repetitions as f64 - 2.0).max(0.0)).min(1.0);
    Some((confidence, evidence))
}

/// Detect whether the context budget is nearly exhausted.
fn detect_resource_exhaustion(
    snapshot: &DetectionSnapshot,
    threshold_pct: u32,
) -> Option<(f64, String)> {
    if snapshot.context_utilization_pct > threshold_pct {
        let overshoot = (snapshot.context_utilization_pct - threshold_pct) as f64;
        let max_overshoot = (100 - threshold_pct) as f64;
        let confidence = if max_overshoot > 0.0 {
            (0.7 + 0.3 * (overshoot / max_overshoot)).min(1.0)
        } else {
            1.0
        };
        Some((
            confidence,
            format!(
                "context utilization at {}% (threshold {}%)",
                snapshot.context_utilization_pct, threshold_pct
            ),
        ))
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// Intervention selection
// ---------------------------------------------------------------------------

/// Choose an intervention action based on the stuck reason and confidence.
fn suggest_intervention(reason: StuckReason, confidence: f64) -> InterventionAction {
    match reason {
        StuckReason::Timeout => InterventionAction::EscalateToUser,
        StuckReason::ResourceExhaustion => InterventionAction::CompressContext,
        StuckReason::ToolCallLoop => {
            if confidence >= 0.9 {
                InterventionAction::EscalateToParent
            } else {
                InterventionAction::SelfAssess
            }
        }
        StuckReason::ErrorLoop => {
            if confidence >= 0.9 {
                InterventionAction::RetryFromCheckpoint
            } else {
                InterventionAction::CompressContext
            }
        }
        StuckReason::NoProgress => {
            if confidence >= 0.8 {
                InterventionAction::RetryFromCheckpoint
            } else {
                InterventionAction::SelfAssess
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
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

    // ----- 1. Healthy snapshot returns None --------------------------------

    #[test]
    fn healthy_snapshot_returns_none() {
        let detector = StuckDetector::default();
        let snap = healthy_snapshot();
        let result = detector.analyze(&snap, 1010);
        assert!(result.is_none(), "healthy snapshot should return None");
    }

    // ----- 2. Timeout detection with max_duration_secs --------------------

    #[test]
    fn timeout_detected_when_exceeding_max_duration() {
        let detector = StuckDetector::default();
        let mut snap = healthy_snapshot();
        snap.started_at = 100;
        snap.max_duration_secs = Some(200);
        // Ensure no other detectors fire: recent progress, no errors, low context.
        snap.last_progress_at = Some(350);
        snap.consecutive_errors = 0;
        snap.context_utilization_pct = 20;
        snap.recent_tool_names = vec!["a".into(), "b".into(), "c".into()];

        let now = 400; // elapsed 300 > max 200
        let analysis = detector.analyze(&snap, now).unwrap();
        assert_eq!(analysis.reason, StuckReason::Timeout);
        assert!(analysis.confidence >= 0.8);
        assert!(analysis.evidence.contains("300"));
        assert!(analysis.evidence.contains("200"));
    }

    // ----- 3. NoProgress detection ----------------------------------------

    #[test]
    fn no_progress_detected_when_idle_exceeds_threshold() {
        let detector = StuckDetector::default();
        let mut snap = healthy_snapshot();
        snap.last_progress_at = Some(100);
        snap.max_duration_secs = None; // disable timeout
        snap.consecutive_errors = 0;
        snap.context_utilization_pct = 20;
        snap.recent_tool_names = vec!["a".into(), "b".into(), "c".into()];

        let now = 500; // 400s idle, default threshold = 300
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

        let now = 500; // 400s since start, > 300 threshold
        let analysis = detector.analyze(&snap, now).unwrap();
        assert_eq!(analysis.reason, StuckReason::NoProgress);
    }

    // ----- 4. ErrorLoop with 3+ consecutive errors ------------------------

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

    // ----- 5. ErrorLoop with 2 consecutive (below threshold) returns None -

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

    // ----- 6. ToolCallLoop with A-B-A-B pattern ---------------------------

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

    // ----- 7. ToolCallLoop with A-A-A-A pattern ---------------------------

    #[test]
    fn tool_loop_detected_with_aaaa_pattern() {
        let detector = StuckDetector::default();
        let mut snap = healthy_snapshot();
        snap.recent_tool_names = vec!["read".into(), "read".into(), "read".into(), "read".into()];

        let analysis = detector.analyze(&snap, 1010).unwrap();
        assert_eq!(analysis.reason, StuckReason::ToolCallLoop);
        assert!(analysis.evidence.contains("read"));
    }

    // ----- 8. ResourceExhaustion at 91% -----------------------------------

    #[test]
    fn resource_exhaustion_detected_at_91_percent() {
        let detector = StuckDetector::default();
        let mut snap = healthy_snapshot();
        snap.context_utilization_pct = 91;

        let analysis = detector.analyze(&snap, 1010).unwrap();
        assert_eq!(analysis.reason, StuckReason::ResourceExhaustion);
        assert!(analysis.evidence.contains("91%"));
    }

    // ----- 9. ResourceExhaustion at 89% returns None ----------------------

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

    // ----- 10. Multiple issues: highest confidence wins -------------------

    #[test]
    fn multiple_issues_highest_confidence_wins() {
        let detector = StuckDetector::default();
        let mut snap = healthy_snapshot();
        // Trigger timeout (high confidence) and error loop (lower confidence).
        snap.started_at = 100;
        snap.max_duration_secs = Some(50);
        snap.consecutive_errors = 3;
        snap.last_progress_at = Some(1000);
        snap.context_utilization_pct = 20;
        snap.recent_tool_names = vec!["a".into(), "b".into(), "c".into()];

        let now = 400; // elapsed 300, max 50 => huge overshoot => confidence ~1.0
        let analysis = detector.analyze(&snap, now).unwrap();
        // Timeout should have the highest confidence due to massive overshoot.
        assert_eq!(analysis.reason, StuckReason::Timeout);
        assert!(analysis.confidence > 0.9);
    }

    // ----- 11. Default thresholds are reasonable --------------------------

    #[test]
    fn default_thresholds_are_reasonable() {
        let detector = StuckDetector::default();
        assert_eq!(detector.no_progress_timeout_secs, 300);
        assert_eq!(detector.error_loop_threshold, 3);
        assert_eq!(detector.tool_loop_min_length, 4);
        assert_eq!(detector.resource_exhaustion_pct, 90);
    }

    // ----- 12. Custom thresholds work -------------------------------------

    #[test]
    fn custom_thresholds_work() {
        let detector = StuckDetector {
            no_progress_timeout_secs: 60,
            error_loop_threshold: 5,
            tool_loop_min_length: 6,
            resource_exhaustion_pct: 80,
        };

        // With custom threshold of 5, 3 errors should NOT trigger.
        let mut snap = healthy_snapshot();
        snap.consecutive_errors = 3;
        assert!(
            detector.analyze(&snap, 1010).is_none(),
            "3 errors with threshold 5 should not trigger"
        );

        // But 5 should.
        snap.consecutive_errors = 5;
        let analysis = detector.analyze(&snap, 1010).unwrap();
        assert_eq!(analysis.reason, StuckReason::ErrorLoop);

        // Custom resource exhaustion at 80%.
        let mut snap2 = healthy_snapshot();
        snap2.context_utilization_pct = 85;
        snap2.consecutive_errors = 0;
        let analysis2 = detector.analyze(&snap2, 1010).unwrap();
        assert_eq!(analysis2.reason, StuckReason::ResourceExhaustion);

        // Custom no-progress threshold of 60s.
        let mut snap3 = healthy_snapshot();
        snap3.last_progress_at = Some(900);
        snap3.max_duration_secs = None;
        snap3.consecutive_errors = 0;
        snap3.context_utilization_pct = 20;
        snap3.recent_tool_names = vec!["a".into(), "b".into(), "c".into()];
        let now = 970; // 70s idle > 60s threshold
        let analysis3 = detector.analyze(&snap3, now).unwrap();
        assert_eq!(analysis3.reason, StuckReason::NoProgress);
    }

    // ----- 13. Intervention selection maps correctly ----------------------

    #[test]
    fn intervention_selection_maps_correctly() {
        // Timeout -> EscalateToUser
        assert_eq!(
            suggest_intervention(StuckReason::Timeout, 0.9),
            InterventionAction::EscalateToUser
        );

        // ResourceExhaustion -> CompressContext
        assert_eq!(
            suggest_intervention(StuckReason::ResourceExhaustion, 0.8),
            InterventionAction::CompressContext
        );

        // ToolCallLoop high confidence -> EscalateToParent
        assert_eq!(
            suggest_intervention(StuckReason::ToolCallLoop, 0.95),
            InterventionAction::EscalateToParent
        );

        // ToolCallLoop low confidence -> SelfAssess
        assert_eq!(
            suggest_intervention(StuckReason::ToolCallLoop, 0.6),
            InterventionAction::SelfAssess
        );

        // ErrorLoop high confidence -> RetryFromCheckpoint
        assert_eq!(
            suggest_intervention(StuckReason::ErrorLoop, 0.95),
            InterventionAction::RetryFromCheckpoint
        );

        // ErrorLoop low confidence -> CompressContext
        assert_eq!(
            suggest_intervention(StuckReason::ErrorLoop, 0.7),
            InterventionAction::CompressContext
        );

        // NoProgress high confidence -> RetryFromCheckpoint
        assert_eq!(
            suggest_intervention(StuckReason::NoProgress, 0.9),
            InterventionAction::RetryFromCheckpoint
        );

        // NoProgress low confidence -> SelfAssess
        assert_eq!(
            suggest_intervention(StuckReason::NoProgress, 0.5),
            InterventionAction::SelfAssess
        );
    }

    // ----- 14. Entity type preserved in analysis --------------------------

    #[test]
    fn entity_type_preserved_in_analysis() {
        let detector = StuckDetector::default();

        // Test with "task" entity type.
        let mut snap = healthy_snapshot();
        snap.entity_type = "task".into();
        snap.consecutive_errors = 5;
        let analysis = detector.analyze(&snap, 1010).unwrap();
        assert_eq!(analysis.entity_type, "task");
        assert_eq!(analysis.entity_id, "entity-1");

        // Test with "goal_run" entity type.
        snap.entity_type = "goal_run".into();
        snap.entity_id = "goal-42".into();
        let analysis = detector.analyze(&snap, 1010).unwrap();
        assert_eq!(analysis.entity_type, "goal_run");
        assert_eq!(analysis.entity_id, "goal-42");
    }

    // ----- 15. Evidence strings are descriptive ---------------------------

    #[test]
    fn evidence_strings_are_descriptive() {
        let detector = StuckDetector::default();

        // Timeout evidence should mention elapsed and max.
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

        // Error loop evidence should mention error count and threshold.
        let mut snap2 = healthy_snapshot();
        snap2.consecutive_errors = 4;
        let analysis2 = detector.analyze(&snap2, 1010).unwrap();
        assert!(
            analysis2.evidence.contains("4 consecutive errors"),
            "error loop evidence should mention count: {}",
            analysis2.evidence
        );

        // Resource exhaustion evidence should mention percentage.
        let mut snap3 = healthy_snapshot();
        snap3.context_utilization_pct = 95;
        snap3.consecutive_errors = 0;
        let analysis3 = detector.analyze(&snap3, 1010).unwrap();
        assert!(
            analysis3.evidence.contains("95%"),
            "resource exhaustion evidence should mention percentage: {}",
            analysis3.evidence
        );

        // Tool loop evidence should mention the tools in the loop.
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

        // NoProgress evidence should mention idle duration and threshold.
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

    // ----- 16. Tool loop not detected with short sequence -----------------

    #[test]
    fn tool_loop_not_detected_with_short_sequence() {
        let detector = StuckDetector::default();
        let mut snap = healthy_snapshot();
        snap.recent_tool_names = vec!["read".into(), "write".into(), "read".into()];
        // Only 3 entries, need at least 4.
        let result = detector.analyze(&snap, 1010);
        assert!(result.is_none(), "3 entries should not trigger tool loop");
    }

    // ----- 17. Timeout not triggered without max_duration_secs ------------

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

    // ----- 18. Confidence values are in valid range -----------------------

    #[test]
    fn confidence_values_in_valid_range() {
        let detector = StuckDetector::default();

        // Extreme timeout overshoot.
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

        // Context at 100%.
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

    // ----- 19. Resource exhaustion boundary at exactly threshold ----------

    #[test]
    fn resource_exhaustion_not_at_exactly_threshold() {
        let detector = StuckDetector::default();
        let mut snap = healthy_snapshot();
        snap.context_utilization_pct = 90; // exactly at threshold, not above
        let result = detector.analyze(&snap, 1010);
        assert!(
            result.is_none(),
            "exactly at threshold (90%) should not trigger"
        );
    }
}
