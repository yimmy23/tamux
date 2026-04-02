//! Sub-agent supervisor — health monitoring, stuck detection, and intervention.

use crate::agent::liveness::stuck_detection::detect_tool_call_loop_evidence;
use crate::agent::types::{
    InterventionAction, InterventionLevel, StuckReason, SubagentHealthState, SupervisorConfig,
};

// ---------------------------------------------------------------------------
// Snapshot — a point-in-time view of a sub-agent's health indicators
// ---------------------------------------------------------------------------

/// A lightweight snapshot of a sub-agent's runtime metrics, used by the
/// supervisor to decide whether intervention is needed.
#[derive(Debug, Clone)]
pub struct SubagentSnapshot {
    /// Unique task identifier for the sub-agent.
    pub task_id: String,
    /// Unix timestamp of the most recent tool call, if any.
    pub last_tool_call_at: Option<u64>,
    /// Total number of tool calls made so far.
    pub tool_calls_total: u32,
    /// Number of tool calls that returned an error.
    pub tool_calls_failed: u32,
    /// Number of consecutive errors (resets on success).
    pub consecutive_errors: u32,
    /// The most recent tool names invoked, in order (newest last).
    pub recent_tool_names: Vec<String>,
    /// Percentage of the context window currently consumed (0–100).
    pub context_utilization_pct: u32,
    /// Unix timestamp when the sub-agent started.
    pub started_at: u64,
    /// Optional hard deadline in seconds from `started_at`.
    pub max_duration_secs: Option<u64>,
}

// ---------------------------------------------------------------------------
// SupervisorAction — the output of a health check
// ---------------------------------------------------------------------------

/// Describes what the supervisor decided after evaluating a sub-agent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SupervisorAction {
    /// The sub-agent's task id.
    pub task_id: String,
    /// The health state determined by the check.
    pub health_state: SubagentHealthState,
    /// Why the sub-agent is considered stuck, if applicable.
    pub reason: Option<StuckReason>,
    /// The intervention to take.
    pub action: InterventionAction,
    /// Human-readable evidence string explaining the decision.
    pub evidence: String,
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Analyse a single sub-agent's health and return an intervention action if
/// one is warranted.  Returns `None` when the agent is healthy **or** when
/// the intervention level is `Passive` and the issue is `NoProgress`.
pub fn check_health(
    snapshot: &SubagentSnapshot,
    config: &SupervisorConfig,
    now: u64,
) -> Option<SupervisorAction> {
    let (reason, evidence) = detect_stuck_reason(snapshot, config, now)?;

    let intervention = select_intervention(
        reason,
        config.intervention_level,
        0, // first attempt — callers track retry counts externally
        config.max_retries,
    );

    // Passive + NoProgress → suppress entirely.
    let intervention = intervention?;

    let health_state = match reason {
        StuckReason::Timeout | StuckReason::ResourceExhaustion => SubagentHealthState::Stuck,
        StuckReason::ErrorLoop | StuckReason::ToolCallLoop => SubagentHealthState::Degraded,
        StuckReason::NoProgress => SubagentHealthState::Stuck,
    };

    Some(SupervisorAction {
        task_id: snapshot.task_id.clone(),
        health_state,
        reason: Some(reason),
        action: intervention,
        evidence,
    })
}

/// Detect **why** a sub-agent is stuck.  Returns the reason together with a
/// human-readable evidence string, or `None` when the agent looks healthy.
///
/// Detection order matters — we check the most severe conditions first.
pub fn detect_stuck_reason(
    snapshot: &SubagentSnapshot,
    config: &SupervisorConfig,
    now: u64,
) -> Option<(StuckReason, String)> {
    // 1. Timeout — hard deadline exceeded.
    if let Some(max_dur) = snapshot.max_duration_secs {
        let elapsed = now.saturating_sub(snapshot.started_at);
        if elapsed > max_dur {
            return Some((
                StuckReason::Timeout,
                format!("elapsed {}s exceeds max_duration {}s", elapsed, max_dur),
            ));
        }
    }

    // 2. ResourceExhaustion — context window almost full.
    if snapshot.context_utilization_pct > 90 {
        return Some((
            StuckReason::ResourceExhaustion,
            format!(
                "context utilization at {}% (>90%)",
                snapshot.context_utilization_pct
            ),
        ));
    }

    // 3. ToolCallLoop — A→B→A→B cycling in recent tool names.
    if let Some(evidence) = detect_tool_call_loop(&snapshot.recent_tool_names) {
        return Some((StuckReason::ToolCallLoop, evidence));
    }

    // 4. ErrorLoop — 3+ consecutive errors.
    if snapshot.consecutive_errors >= 3 {
        return Some((
            StuckReason::ErrorLoop,
            format!("{} consecutive errors", snapshot.consecutive_errors),
        ));
    }

    // 5. NoProgress — no tool call within stuck_timeout_secs.
    let idle_secs = match snapshot.last_tool_call_at {
        Some(ts) => now.saturating_sub(ts),
        None => now.saturating_sub(snapshot.started_at),
    };
    if idle_secs >= config.stuck_timeout_secs {
        return Some((
            StuckReason::NoProgress,
            format!(
                "no tool calls for {}s (threshold {}s)",
                idle_secs, config.stuck_timeout_secs
            ),
        ));
    }

    None
}

/// Choose the right intervention action for the given stuck reason,
/// intervention level, and retry state.
///
/// Returns `None` when the policy says "do nothing" (e.g. `Passive` +
/// `NoProgress`).
pub fn select_intervention(
    reason: StuckReason,
    level: InterventionLevel,
    retry_count: u32,
    max_retries: u32,
) -> Option<InterventionAction> {
    match reason {
        StuckReason::NoProgress => match level {
            InterventionLevel::Passive => None,
            InterventionLevel::Normal => Some(InterventionAction::SelfAssess),
            InterventionLevel::Aggressive => Some(InterventionAction::RetryFromCheckpoint),
        },
        StuckReason::ErrorLoop => {
            if retry_count >= max_retries {
                Some(InterventionAction::EscalateToParent)
            } else {
                Some(InterventionAction::CompressContext)
            }
        }
        StuckReason::ToolCallLoop => Some(InterventionAction::EscalateToParent),
        StuckReason::ResourceExhaustion => Some(InterventionAction::CompressContext),
        StuckReason::Timeout => Some(InterventionAction::EscalateToUser),
    }
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Detect a repeating cycle in the recent tool names list.
///
/// Delegates to the shared [`detect_tool_call_loop_evidence`] utility in
/// `crate::agent::liveness::stuck_detection`.  Looks for the shortest
/// repeating pattern of length 1 or 2 that covers at least 4 consecutive
/// entries.
fn detect_tool_call_loop(names: &[String]) -> Option<String> {
    detect_tool_call_loop_evidence(names, 4)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper — build a healthy snapshot with sensible defaults.
    fn healthy_snapshot() -> SubagentSnapshot {
        SubagentSnapshot {
            task_id: "task-1".into(),
            last_tool_call_at: Some(1000),
            tool_calls_total: 10,
            tool_calls_failed: 0,
            consecutive_errors: 0,
            recent_tool_names: vec!["read".into(), "write".into(), "grep".into()],
            context_utilization_pct: 40,
            started_at: 900,
            max_duration_secs: Some(600),
        }
    }

    fn default_config() -> SupervisorConfig {
        SupervisorConfig::default()
    }

    // ----- healthy agent --------------------------------------------------

    #[test]
    fn healthy_agent_no_action() {
        let snap = healthy_snapshot();
        let cfg = default_config();
        let result = check_health(&snap, &cfg, 1010);
        assert!(
            result.is_none(),
            "healthy agent should not produce an action"
        );
    }

    // ----- NoProgress -----------------------------------------------------

    #[test]
    fn no_progress_detected_when_no_tool_calls() {
        let mut snap = healthy_snapshot();
        snap.last_tool_call_at = None;
        snap.started_at = 100;
        let cfg = default_config(); // stuck_timeout = 300
        let now = 500; // 400s since start, > 300
        let (reason, _evidence) = detect_stuck_reason(&snap, &cfg, now).unwrap();
        assert_eq!(reason, StuckReason::NoProgress);
    }

    #[test]
    fn no_progress_detected_with_stale_tool_call() {
        let mut snap = healthy_snapshot();
        snap.last_tool_call_at = Some(100);
        let cfg = default_config();
        let now = 500; // 400s since last tool call
        let (reason, _) = detect_stuck_reason(&snap, &cfg, now).unwrap();
        assert_eq!(reason, StuckReason::NoProgress);
    }

    #[test]
    fn no_progress_not_detected_within_threshold() {
        let mut snap = healthy_snapshot();
        snap.last_tool_call_at = Some(900);
        let cfg = default_config();
        let now = 1100; // 200s, below 300 threshold
        assert!(detect_stuck_reason(&snap, &cfg, now).is_none());
    }

    // ----- ErrorLoop ------------------------------------------------------

    #[test]
    fn error_loop_detected_with_3_consecutive() {
        let mut snap = healthy_snapshot();
        snap.consecutive_errors = 3;
        let cfg = default_config();
        let (reason, evidence) = detect_stuck_reason(&snap, &cfg, 1010).unwrap();
        assert_eq!(reason, StuckReason::ErrorLoop);
        assert!(evidence.contains("3 consecutive errors"));
    }

    #[test]
    fn error_loop_detected_with_5_consecutive() {
        let mut snap = healthy_snapshot();
        snap.consecutive_errors = 5;
        let cfg = default_config();
        let (reason, _) = detect_stuck_reason(&snap, &cfg, 1010).unwrap();
        assert_eq!(reason, StuckReason::ErrorLoop);
    }

    #[test]
    fn error_loop_not_detected_with_2_consecutive() {
        let mut snap = healthy_snapshot();
        snap.consecutive_errors = 2;
        let cfg = default_config();
        assert!(detect_stuck_reason(&snap, &cfg, 1010).is_none());
    }

    // ----- ToolCallLoop ---------------------------------------------------

    #[test]
    fn tool_call_loop_detected_with_abab_pattern() {
        let mut snap = healthy_snapshot();
        snap.recent_tool_names = vec!["read".into(), "write".into(), "read".into(), "write".into()];
        let cfg = default_config();
        let (reason, evidence) = detect_stuck_reason(&snap, &cfg, 1010).unwrap();
        assert_eq!(reason, StuckReason::ToolCallLoop);
        assert!(evidence.contains("loop"));
    }

    #[test]
    fn tool_call_loop_detected_with_single_repeating() {
        let mut snap = healthy_snapshot();
        snap.recent_tool_names = vec!["read".into(), "read".into(), "read".into(), "read".into()];
        let cfg = default_config();
        let (reason, _) = detect_stuck_reason(&snap, &cfg, 1010).unwrap();
        assert_eq!(reason, StuckReason::ToolCallLoop);
    }

    #[test]
    fn tool_call_loop_not_detected_with_short_sequence() {
        let mut snap = healthy_snapshot();
        snap.recent_tool_names = vec!["read".into(), "write".into(), "read".into()];
        let cfg = default_config();
        // Should not trigger — only 3 entries, need at least 4.
        assert!(detect_stuck_reason(&snap, &cfg, 1010).is_none());
    }

    // ----- ResourceExhaustion ---------------------------------------------

    #[test]
    fn resource_exhaustion_at_91_percent() {
        let mut snap = healthy_snapshot();
        snap.context_utilization_pct = 91;
        let cfg = default_config();
        let (reason, evidence) = detect_stuck_reason(&snap, &cfg, 1010).unwrap();
        assert_eq!(reason, StuckReason::ResourceExhaustion);
        assert!(evidence.contains("91%"));
    }

    #[test]
    fn resource_exhaustion_not_at_90_percent() {
        let mut snap = healthy_snapshot();
        snap.context_utilization_pct = 90;
        let cfg = default_config();
        // 90% is the boundary — only > 90 triggers.
        assert!(detect_stuck_reason(&snap, &cfg, 1010).is_none());
    }

    // ----- Timeout --------------------------------------------------------

    #[test]
    fn timeout_detected() {
        let mut snap = healthy_snapshot();
        snap.started_at = 100;
        snap.max_duration_secs = Some(200);
        let cfg = default_config();
        let now = 400; // elapsed 300 > max 200
        let (reason, evidence) = detect_stuck_reason(&snap, &cfg, now).unwrap();
        assert_eq!(reason, StuckReason::Timeout);
        assert!(evidence.contains("300"));
    }

    #[test]
    fn timeout_not_detected_within_limit() {
        let mut snap = healthy_snapshot();
        snap.started_at = 100;
        snap.max_duration_secs = Some(500);
        let cfg = default_config();
        let now = 400; // elapsed 300 < max 500
                       // Timeout should not fire.  Other checks may or may not fire, but
                       // let's ensure at least timeout isn't the reason.
        if let Some((reason, _)) = detect_stuck_reason(&snap, &cfg, now) {
            assert_ne!(reason, StuckReason::Timeout);
        }
    }

    // ----- Intervention selection -----------------------------------------

    #[test]
    fn passive_no_progress_returns_none() {
        let result = select_intervention(StuckReason::NoProgress, InterventionLevel::Passive, 0, 2);
        assert!(result.is_none());
    }

    #[test]
    fn passive_no_progress_check_health_returns_none() {
        let mut snap = healthy_snapshot();
        snap.last_tool_call_at = None;
        snap.started_at = 100;
        let mut cfg = default_config();
        cfg.intervention_level = InterventionLevel::Passive;
        let now = 500;
        let result = check_health(&snap, &cfg, now);
        assert!(
            result.is_none(),
            "passive mode should suppress NoProgress actions"
        );
    }

    #[test]
    fn normal_no_progress_returns_self_assess() {
        let action = select_intervention(StuckReason::NoProgress, InterventionLevel::Normal, 0, 2);
        assert_eq!(action, Some(InterventionAction::SelfAssess));
    }

    #[test]
    fn aggressive_no_progress_returns_retry() {
        let action =
            select_intervention(StuckReason::NoProgress, InterventionLevel::Aggressive, 0, 2);
        assert_eq!(action, Some(InterventionAction::RetryFromCheckpoint));
    }

    #[test]
    fn error_loop_first_retry_returns_compress() {
        let action = select_intervention(StuckReason::ErrorLoop, InterventionLevel::Normal, 0, 2);
        assert_eq!(action, Some(InterventionAction::CompressContext));
    }

    #[test]
    fn error_loop_retries_exhausted_escalates() {
        let action = select_intervention(StuckReason::ErrorLoop, InterventionLevel::Normal, 2, 2);
        assert_eq!(action, Some(InterventionAction::EscalateToParent));
    }

    #[test]
    fn tool_call_loop_escalates_to_parent() {
        let action =
            select_intervention(StuckReason::ToolCallLoop, InterventionLevel::Normal, 0, 2);
        assert_eq!(action, Some(InterventionAction::EscalateToParent));
    }

    #[test]
    fn resource_exhaustion_compresses() {
        let action = select_intervention(
            StuckReason::ResourceExhaustion,
            InterventionLevel::Aggressive,
            0,
            2,
        );
        assert_eq!(action, Some(InterventionAction::CompressContext));
    }

    #[test]
    fn timeout_escalates_to_user() {
        let action = select_intervention(StuckReason::Timeout, InterventionLevel::Normal, 0, 2);
        assert_eq!(action, Some(InterventionAction::EscalateToUser));
    }

    // ----- check_health integration --------------------------------------

    #[test]
    fn check_health_returns_correct_action_for_error_loop() {
        let mut snap = healthy_snapshot();
        snap.consecutive_errors = 4;
        let cfg = default_config();
        let action = check_health(&snap, &cfg, 1010).unwrap();
        assert_eq!(action.reason, Some(StuckReason::ErrorLoop));
        assert_eq!(action.action, InterventionAction::CompressContext);
        assert_eq!(action.health_state, SubagentHealthState::Degraded);
        assert_eq!(action.task_id, "task-1");
    }

    #[test]
    fn check_health_returns_stuck_for_timeout() {
        let mut snap = healthy_snapshot();
        snap.started_at = 0;
        snap.max_duration_secs = Some(100);
        let cfg = default_config();
        let action = check_health(&snap, &cfg, 200).unwrap();
        assert_eq!(action.health_state, SubagentHealthState::Stuck);
        assert_eq!(action.reason, Some(StuckReason::Timeout));
        assert_eq!(action.action, InterventionAction::EscalateToUser);
    }
}
