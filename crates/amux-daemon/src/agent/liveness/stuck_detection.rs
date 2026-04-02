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
#[path = "tests/stuck_detection.rs"]
mod tests;
