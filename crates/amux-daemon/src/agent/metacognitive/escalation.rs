//! Escalation pathways — graduated intervention from self-correction to external notification.
//!
//! This module provides pure data types and helper functions for escalation.
//! The actual escalation transitions (evaluate + apply) happen in the AgentEngine
//! call sites; this module provides `escalation_audit_data()` to build audit entries
//! and event payloads that the AgentEngine can persist/broadcast.

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Core types
// ---------------------------------------------------------------------------

/// Graduated escalation levels from autonomous self-correction up to external
/// notification.  Ordered so that `L0 < L1 < L2 < L3`.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum EscalationLevel {
    /// Level 0: Self-correction — auto-retry, strategy rotation, context refresh.
    SelfCorrection,
    /// Level 1: Sub-agent — spawn expert, handover, result integration.
    SubAgent,
    /// Level 2: User — generate escalation message, handle response, timeout.
    User,
    /// Level 3: External — notification via gateway, pause execution.
    External,
}

impl EscalationLevel {
    /// Return the short label for this level (e.g. "L0", "L1", "L2", "L3").
    pub fn as_label(&self) -> &'static str {
        match self {
            Self::SelfCorrection => "L0",
            Self::SubAgent => "L1",
            Self::User => "L2",
            Self::External => "L3",
        }
    }
}

/// Tracks the current position within the escalation pathway together with
/// historical events that led to it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EscalationState {
    pub current_level: EscalationLevel,
    pub attempts_at_level: u32,
    pub total_escalations: u32,
    pub escalation_history: Vec<EscalationEvent>,
    pub started_at: u64,
}

/// A single recorded escalation event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EscalationEvent {
    pub level: EscalationLevel,
    pub reason: String,
    pub timestamp: u64,
    pub outcome: Option<String>,
}

/// Thresholds that govern when escalation should occur.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EscalationCriteria {
    /// Maximum retries at L0 before escalating to L1.
    pub max_self_correction_attempts: u32,
    /// Maximum retries at L1 before escalating to L2.
    pub max_subagent_attempts: u32,
    /// Seconds to wait for a user response at L2 before escalating to L3.
    pub user_response_timeout_secs: u64,
}

/// The output of an escalation evaluation — describes whether escalation
/// should happen and, if so, where to go.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EscalationDecision {
    pub should_escalate: bool,
    pub target_level: EscalationLevel,
    pub reason: String,
    /// An optional human-/system-readable message for the target audience.
    pub message: Option<String>,
}

// ---------------------------------------------------------------------------
// Defaults
// ---------------------------------------------------------------------------

impl Default for EscalationCriteria {
    fn default() -> Self {
        Self {
            max_self_correction_attempts: 2,
            max_subagent_attempts: 1,
            user_response_timeout_secs: 300,
        }
    }
}

// ---------------------------------------------------------------------------
// EscalationState implementation
// ---------------------------------------------------------------------------

impl EscalationState {
    /// Create a new state starting at `SelfCorrection` (L0).
    pub fn new(now: u64) -> Self {
        Self {
            current_level: EscalationLevel::SelfCorrection,
            attempts_at_level: 0,
            total_escalations: 0,
            escalation_history: Vec::new(),
            started_at: now,
        }
    }

    /// Evaluate whether escalation should occur given the current state and
    /// criteria.
    ///
    /// * If `succeeded` is `true` for the current level the decision is to
    ///   stay (no escalation).
    /// * Otherwise the attempt counter is compared against the relevant
    ///   threshold and, when exceeded, escalation to the next level is
    ///   recommended.
    pub fn evaluate(&self, criteria: &EscalationCriteria, succeeded: bool) -> EscalationDecision {
        // Success at any level -> no escalation.
        if succeeded {
            return EscalationDecision {
                should_escalate: false,
                target_level: self.current_level,
                reason: "Succeeded at current level".into(),
                message: None,
            };
        }

        match self.current_level {
            EscalationLevel::SelfCorrection => {
                if self.attempts_at_level >= criteria.max_self_correction_attempts {
                    EscalationDecision {
                        should_escalate: true,
                        target_level: EscalationLevel::SubAgent,
                        reason: format!(
                            "Self-correction failed after {} attempts",
                            self.attempts_at_level
                        ),
                        message: None,
                    }
                } else {
                    EscalationDecision {
                        should_escalate: false,
                        target_level: self.current_level,
                        reason: "Retrying self-correction".into(),
                        message: None,
                    }
                }
            }
            EscalationLevel::SubAgent => {
                if self.attempts_at_level >= criteria.max_subagent_attempts {
                    EscalationDecision {
                        should_escalate: true,
                        target_level: EscalationLevel::User,
                        reason: format!(
                            "Sub-agent failed after {} attempts",
                            self.attempts_at_level
                        ),
                        message: Some(
                            "Sub-agent could not resolve the issue; escalating to user.".into(),
                        ),
                    }
                } else {
                    EscalationDecision {
                        should_escalate: false,
                        target_level: self.current_level,
                        reason: "Retrying sub-agent".into(),
                        message: None,
                    }
                }
            }
            EscalationLevel::User => {
                // At L2 any failure (e.g. timeout) triggers L3.
                EscalationDecision {
                    should_escalate: true,
                    target_level: EscalationLevel::External,
                    reason: "User escalation timed out or was unsuccessful".into(),
                    message: Some("Escalating to external notification.".into()),
                }
            }
            EscalationLevel::External => {
                // Terminal level — cannot escalate further.
                EscalationDecision {
                    should_escalate: false,
                    target_level: EscalationLevel::External,
                    reason: "Already at maximum escalation level".into(),
                    message: None,
                }
            }
        }
    }

    /// Apply an [`EscalationDecision`] to mutate the state.
    pub fn apply(&mut self, decision: &EscalationDecision, now: u64) {
        const MAX_ESCALATION_HISTORY: usize = 100;

        if decision.should_escalate {
            self.escalation_history.push(EscalationEvent {
                level: decision.target_level,
                reason: decision.reason.clone(),
                timestamp: now,
                outcome: None,
            });

            if self.escalation_history.len() > MAX_ESCALATION_HISTORY {
                self.escalation_history
                    .drain(..self.escalation_history.len() - MAX_ESCALATION_HISTORY);
            }

            self.current_level = decision.target_level;
            self.attempts_at_level = 0;
            self.total_escalations += 1;
        } else {
            self.attempts_at_level += 1;
        }
    }

    /// Return the current escalation level.
    pub fn current_level(&self) -> EscalationLevel {
        self.current_level
    }

    /// Reset the state back to L0 (`SelfCorrection`).
    pub fn reset(&mut self, now: u64) {
        self.current_level = EscalationLevel::SelfCorrection;
        self.attempts_at_level = 0;
        self.escalation_history.clear();
        self.started_at = now;
    }

    /// Cancel active escalation and return control to user. Per D-13/TRNS-05.
    ///
    /// Returns `Ok(message)` on success describing what was cancelled.
    /// Returns `Err` if there is no active escalation to cancel (at L0 with
    /// zero total escalations).
    ///
    /// Race condition handling (Pitfall 6): if the escalation has already
    /// resolved back to L0, this returns success with an informational message
    /// rather than failing.
    pub fn cancel_escalation(&mut self, now: u64) -> anyhow::Result<String> {
        // Race condition: already at L0 with no history of escalation — nothing to cancel.
        if self.current_level == EscalationLevel::SelfCorrection && self.total_escalations == 0 {
            anyhow::bail!("No active escalation to cancel");
        }

        // Race condition: already resolved back to L0 — succeed gracefully.
        if self.current_level == EscalationLevel::SelfCorrection {
            return Ok("Escalation already resolved. You have control.".to_string());
        }

        let prev_level = self.current_level;
        self.escalation_history.push(EscalationEvent {
            level: prev_level,
            reason: "User cancelled escalation (I'll handle this)".to_string(),
            timestamp: now,
            outcome: Some("cancelled_by_user".to_string()),
        });

        // Reset to L0 — user takes over.
        self.current_level = EscalationLevel::SelfCorrection;
        self.attempts_at_level = 0;

        Ok(format!(
            "Escalation cancelled at {}. You now have control.",
            prev_level.as_label()
        ))
    }
}

// ---------------------------------------------------------------------------
// Helper
// ---------------------------------------------------------------------------

/// Build a human-readable escalation message appropriate for `level`.
pub fn build_escalation_message(
    level: EscalationLevel,
    goal_title: &str,
    step_title: &str,
    reason: &str,
) -> String {
    match level {
        EscalationLevel::SelfCorrection => {
            format!(
                "[Self-Correction] Retrying step '{}' for goal '{}': {}",
                step_title, goal_title, reason
            )
        }
        EscalationLevel::SubAgent => {
            format!(
                "[Sub-Agent] Spawning expert for step '{}' in goal '{}': {}",
                step_title, goal_title, reason
            )
        }
        EscalationLevel::User => {
            format!(
                "[User Escalation] Assistance needed for goal '{}', step '{}': {}",
                goal_title, step_title, reason
            )
        }
        EscalationLevel::External => {
            format!(
                "[External Notification] Goal '{}' — step '{}' requires external intervention. Details: {}",
                goal_title, step_title, reason
            )
        }
    }
}

// ---------------------------------------------------------------------------
// Audit entry helper (per D-06/TRNS-05)
// ---------------------------------------------------------------------------

/// Data produced by `escalation_audit_data` for the AgentEngine to persist and broadcast.
#[derive(Debug, Clone)]
pub struct EscalationAuditData {
    /// Unique ID for the audit entry.
    pub audit_id: String,
    /// Timestamp (epoch millis).
    pub timestamp: u64,
    /// Human-readable summary / explanation.
    pub summary: String,
    /// The from-level label (e.g. "L0").
    pub from_label: String,
    /// The to-level label (e.g. "L1").
    pub to_label: String,
    /// Reason for escalation.
    pub reason: String,
    /// Number of attempts at the from-level.
    pub attempts: u32,
    /// Serialized JSON with escalation details for raw_data_json.
    pub raw_data_json: String,
}

/// Build audit entry data for an escalation level transition.
///
/// Called from AgentEngine integration code after `EscalationState::apply()` when
/// `decision.should_escalate` is true. The returned `EscalationAuditData` contains
/// all fields needed to create an `AuditEntryRow` and broadcast `EscalationUpdate` +
/// `AuditAction` events.
///
/// Uses `generate_explanation("escalation", ...)` from the explanation module. When
/// causal factors exceed the template threshold (> 2), falls back to a formatted string.
pub fn escalation_audit_data(
    from_level: &EscalationLevel,
    to_level: &EscalationLevel,
    reason: &str,
    attempts: u32,
    thread_id: Option<&str>,
    causal_factors: &[serde_json::Value],
    now: u64,
) -> EscalationAuditData {
    let from_label = from_level.as_label().to_string();
    let to_label = to_level.as_label().to_string();

    let data = serde_json::json!({
        "from_level": from_label,
        "to_level": to_label,
        "reason": reason,
        "causal_factors": causal_factors,
        "thread_id": thread_id,
        "attempts": attempts,
    });

    // generate_explanation for "escalation" returns NeedsLlm when factors > 2,
    // Template otherwise. We need to import from the parent module.
    let summary = {
        let factors_count = causal_factors.len();
        if factors_count > 2 {
            // NeedsLlm case — use fallback template
            format!(
                "Escalating from {} to {}: {} ({} causal factors)",
                from_label, to_label, reason, factors_count
            )
        } else {
            format!("Escalating from {} to {}: {}", from_label, to_label, reason)
        }
    };

    let raw_data_json = serde_json::to_string(&data).unwrap_or_default();

    EscalationAuditData {
        audit_id: format!("audit-esc-{}", uuid::Uuid::new_v4()),
        timestamp: now,
        summary,
        from_label,
        to_label,
        reason: reason.to_string(),
        attempts,
        raw_data_json,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn default_criteria() -> EscalationCriteria {
        EscalationCriteria::default()
    }

    // 1. New state starts at SelfCorrection
    #[test]
    fn new_state_starts_at_self_correction() {
        let state = EscalationState::new(1000);
        assert_eq!(state.current_level(), EscalationLevel::SelfCorrection);
        assert_eq!(state.attempts_at_level, 0);
        assert_eq!(state.total_escalations, 0);
        assert!(state.escalation_history.is_empty());
    }

    // 2. Success at current level -> no escalation
    #[test]
    fn success_at_current_level_no_escalation() {
        let state = EscalationState::new(1000);
        let decision = state.evaluate(&default_criteria(), true);
        assert!(!decision.should_escalate);
        assert_eq!(decision.target_level, EscalationLevel::SelfCorrection);
    }

    // 3. L0 fails twice -> escalate to L1
    #[test]
    fn l0_fails_twice_escalates_to_l1() {
        let mut state = EscalationState::new(1000);
        let criteria = default_criteria();

        // First failure — still under threshold.
        let d1 = state.evaluate(&criteria, false);
        assert!(!d1.should_escalate);
        state.apply(&d1, 1001);
        assert_eq!(state.attempts_at_level, 1);

        // Second failure — still under threshold (need >= 2 attempts recorded).
        let d2 = state.evaluate(&criteria, false);
        assert!(!d2.should_escalate);
        state.apply(&d2, 1002);
        assert_eq!(state.attempts_at_level, 2);

        // Third evaluation — now at threshold, should escalate.
        let d3 = state.evaluate(&criteria, false);
        assert!(d3.should_escalate);
        assert_eq!(d3.target_level, EscalationLevel::SubAgent);
    }

    // 4. L1 fails once -> escalate to L2
    #[test]
    fn l1_fails_once_escalates_to_l2() {
        let mut state = EscalationState::new(1000);
        state.current_level = EscalationLevel::SubAgent;

        let criteria = default_criteria(); // max_subagent_attempts = 1

        // First failure — under threshold.
        let d1 = state.evaluate(&criteria, false);
        assert!(!d1.should_escalate);
        state.apply(&d1, 1001);

        // Second evaluation — at threshold.
        let d2 = state.evaluate(&criteria, false);
        assert!(d2.should_escalate);
        assert_eq!(d2.target_level, EscalationLevel::User);
    }

    // 5. L2 -> escalate to L3
    #[test]
    fn l2_escalates_to_l3() {
        let mut state = EscalationState::new(1000);
        state.current_level = EscalationLevel::User;

        let decision = state.evaluate(&default_criteria(), false);
        assert!(decision.should_escalate);
        assert_eq!(decision.target_level, EscalationLevel::External);
    }

    // 6. L3 stays at L3 (no further escalation)
    #[test]
    fn l3_stays_at_l3() {
        let mut state = EscalationState::new(1000);
        state.current_level = EscalationLevel::External;

        let decision = state.evaluate(&default_criteria(), false);
        assert!(!decision.should_escalate);
        assert_eq!(decision.target_level, EscalationLevel::External);
    }

    // 7. Apply updates state correctly
    #[test]
    fn apply_updates_state_correctly() {
        let mut state = EscalationState::new(1000);

        // Escalation decision.
        let decision = EscalationDecision {
            should_escalate: true,
            target_level: EscalationLevel::SubAgent,
            reason: "test escalation".into(),
            message: None,
        };
        state.apply(&decision, 2000);

        assert_eq!(state.current_level(), EscalationLevel::SubAgent);
        assert_eq!(state.attempts_at_level, 0);
        assert_eq!(state.total_escalations, 1);
        assert_eq!(state.escalation_history.len(), 1);
    }

    // 8. Reset returns to L0
    #[test]
    fn reset_returns_to_l0() {
        let mut state = EscalationState::new(1000);
        state.current_level = EscalationLevel::External;
        state.attempts_at_level = 5;

        state.reset(3000);

        assert_eq!(state.current_level(), EscalationLevel::SelfCorrection);
        assert_eq!(state.attempts_at_level, 0);
        assert_eq!(state.started_at, 3000);
    }

    // 9. Escalation history tracks events
    #[test]
    fn escalation_history_tracks_events() {
        let mut state = EscalationState::new(1000);

        let d1 = EscalationDecision {
            should_escalate: true,
            target_level: EscalationLevel::SubAgent,
            reason: "first".into(),
            message: None,
        };
        state.apply(&d1, 2000);

        let d2 = EscalationDecision {
            should_escalate: true,
            target_level: EscalationLevel::User,
            reason: "second".into(),
            message: Some("help".into()),
        };
        state.apply(&d2, 3000);

        assert_eq!(state.escalation_history.len(), 2);
        assert_eq!(state.escalation_history[0].reason, "first");
        assert_eq!(state.escalation_history[0].timestamp, 2000);
        assert_eq!(state.escalation_history[1].reason, "second");
        assert_eq!(state.escalation_history[1].level, EscalationLevel::User);
    }

    // 10. Message for User level includes goal title
    #[test]
    fn message_for_user_level_includes_goal_title() {
        let msg = build_escalation_message(
            EscalationLevel::User,
            "Deploy Service",
            "Run migrations",
            "migration failed",
        );
        assert!(msg.contains("Deploy Service"));
        assert!(msg.contains("Run migrations"));
        assert!(msg.contains("migration failed"));
    }

    // 11. Message for External level includes details
    #[test]
    fn message_for_external_level_includes_details() {
        let msg = build_escalation_message(
            EscalationLevel::External,
            "Critical Pipeline",
            "Health check",
            "service unreachable",
        );
        assert!(msg.contains("Critical Pipeline"));
        assert!(msg.contains("Health check"));
        assert!(msg.contains("service unreachable"));
        assert!(msg.contains("External Notification"));
    }

    // 12. Total escalations counter increments
    #[test]
    fn total_escalations_counter_increments() {
        let mut state = EscalationState::new(1000);

        let escalate = |target: EscalationLevel, reason: &str| EscalationDecision {
            should_escalate: true,
            target_level: target,
            reason: reason.into(),
            message: None,
        };

        state.apply(&escalate(EscalationLevel::SubAgent, "a"), 2000);
        assert_eq!(state.total_escalations, 1);

        state.apply(&escalate(EscalationLevel::User, "b"), 3000);
        assert_eq!(state.total_escalations, 2);

        state.apply(&escalate(EscalationLevel::External, "c"), 4000);
        assert_eq!(state.total_escalations, 3);
    }

    // 13. Non-escalation apply increments attempts_at_level
    #[test]
    fn non_escalation_increments_attempts() {
        let mut state = EscalationState::new(1000);

        let no_escalate = EscalationDecision {
            should_escalate: false,
            target_level: EscalationLevel::SelfCorrection,
            reason: "retry".into(),
            message: None,
        };

        state.apply(&no_escalate, 2000);
        assert_eq!(state.attempts_at_level, 1);
        assert_eq!(state.total_escalations, 0);
        assert!(state.escalation_history.is_empty());

        state.apply(&no_escalate, 3000);
        assert_eq!(state.attempts_at_level, 2);
    }

    // 14. Default criteria has expected values
    #[test]
    fn default_criteria_values() {
        let c = EscalationCriteria::default();
        assert_eq!(c.max_self_correction_attempts, 2);
        assert_eq!(c.max_subagent_attempts, 1);
        assert_eq!(c.user_response_timeout_secs, 300);
    }

    // 15. EscalationLevel ordering
    #[test]
    fn escalation_level_ordering() {
        assert!(EscalationLevel::SelfCorrection < EscalationLevel::SubAgent);
        assert!(EscalationLevel::SubAgent < EscalationLevel::User);
        assert!(EscalationLevel::User < EscalationLevel::External);
    }

    // 16. Full escalation walkthrough L0 -> L1 -> L2 -> L3
    #[test]
    fn full_escalation_walkthrough() {
        let mut state = EscalationState::new(0);
        let criteria = default_criteria();

        // L0: fail twice, then escalate.
        for t in 1..=2 {
            let d = state.evaluate(&criteria, false);
            state.apply(&d, t);
        }
        let d = state.evaluate(&criteria, false);
        assert!(d.should_escalate);
        state.apply(&d, 3);
        assert_eq!(state.current_level(), EscalationLevel::SubAgent);

        // L1: fail once, then escalate.
        let d = state.evaluate(&criteria, false);
        state.apply(&d, 4);
        let d = state.evaluate(&criteria, false);
        assert!(d.should_escalate);
        state.apply(&d, 5);
        assert_eq!(state.current_level(), EscalationLevel::User);

        // L2: immediate escalation on failure.
        let d = state.evaluate(&criteria, false);
        assert!(d.should_escalate);
        state.apply(&d, 6);
        assert_eq!(state.current_level(), EscalationLevel::External);

        // L3: stays.
        let d = state.evaluate(&criteria, false);
        assert!(!d.should_escalate);
        assert_eq!(state.current_level(), EscalationLevel::External);

        assert_eq!(state.total_escalations, 3);
        assert_eq!(state.escalation_history.len(), 3);
    }

    // 17. EscalationLevel::as_label returns correct labels
    #[test]
    fn escalation_level_labels() {
        assert_eq!(EscalationLevel::SelfCorrection.as_label(), "L0");
        assert_eq!(EscalationLevel::SubAgent.as_label(), "L1");
        assert_eq!(EscalationLevel::User.as_label(), "L2");
        assert_eq!(EscalationLevel::External.as_label(), "L3");
    }

    // 18. escalation_audit_data produces correct simple summary
    #[test]
    fn escalation_audit_data_simple() {
        let data = escalation_audit_data(
            &EscalationLevel::SelfCorrection,
            &EscalationLevel::SubAgent,
            "timeout after 2 retries",
            2,
            Some("thread-1"),
            &[serde_json::json!("factor1")],
            5000,
        );
        assert!(data.audit_id.starts_with("audit-esc-"));
        assert_eq!(data.timestamp, 5000);
        assert_eq!(data.from_label, "L0");
        assert_eq!(data.to_label, "L1");
        assert!(data.summary.contains("L0"));
        assert!(data.summary.contains("L1"));
        assert!(data.summary.contains("timeout"));
        assert_eq!(data.attempts, 2);
    }

    // 19. escalation_audit_data with many causal factors includes count
    #[test]
    fn escalation_audit_data_complex() {
        let data = escalation_audit_data(
            &EscalationLevel::SubAgent,
            &EscalationLevel::User,
            "multiple failures",
            1,
            None,
            &[
                serde_json::json!("f1"),
                serde_json::json!("f2"),
                serde_json::json!("f3"),
            ],
            6000,
        );
        assert!(data.summary.contains("3 causal factors"));
        assert!(data.summary.contains("L1"));
        assert!(data.summary.contains("L2"));
    }

    // 20. escalation_audit_data raw_data_json is valid JSON
    #[test]
    fn escalation_audit_data_raw_json_valid() {
        let data = escalation_audit_data(
            &EscalationLevel::User,
            &EscalationLevel::External,
            "user timeout",
            0,
            Some("t-42"),
            &[],
            7000,
        );
        let parsed: serde_json::Value =
            serde_json::from_str(&data.raw_data_json).expect("valid JSON");
        assert_eq!(parsed["from_level"], "L2");
        assert_eq!(parsed["to_level"], "L3");
        assert_eq!(parsed["reason"], "user timeout");
        assert_eq!(parsed["thread_id"], "t-42");
    }

    // 21. cancel_escalation at L0 with no history fails
    #[test]
    fn cancel_escalation_no_active_fails() {
        let mut state = EscalationState::new(1000);
        let result = state.cancel_escalation(2000);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("No active escalation"));
    }

    // 22. cancel_escalation at active level resets to L0
    #[test]
    fn cancel_escalation_resets_to_l0() {
        let mut state = EscalationState::new(1000);
        // Escalate to L1 first.
        let decision = EscalationDecision {
            should_escalate: true,
            target_level: EscalationLevel::SubAgent,
            reason: "test".into(),
            message: None,
        };
        state.apply(&decision, 2000);
        assert_eq!(state.current_level(), EscalationLevel::SubAgent);

        let result = state.cancel_escalation(3000);
        assert!(result.is_ok());
        let msg = result.unwrap();
        assert!(msg.contains("L1"));
        assert!(msg.contains("cancelled"));
        assert_eq!(state.current_level(), EscalationLevel::SelfCorrection);
        assert_eq!(state.attempts_at_level, 0);
        // History should include the cancel event.
        let last = state.escalation_history.last().unwrap();
        assert_eq!(last.outcome.as_deref(), Some("cancelled_by_user"));
    }

    // 23. cancel_escalation race condition: already resolved back to L0
    #[test]
    fn cancel_escalation_already_resolved() {
        let mut state = EscalationState::new(1000);
        // Escalate then reset (simulating resolution).
        let decision = EscalationDecision {
            should_escalate: true,
            target_level: EscalationLevel::SubAgent,
            reason: "test".into(),
            message: None,
        };
        state.apply(&decision, 2000);
        state.current_level = EscalationLevel::SelfCorrection;
        state.attempts_at_level = 0;

        let result = state.cancel_escalation(3000);
        assert!(result.is_ok());
        assert!(result.unwrap().contains("already resolved"));
    }

    // 24. cancel_escalation at L2 includes correct label
    #[test]
    fn cancel_escalation_at_l2() {
        let mut state = EscalationState::new(1000);
        state.current_level = EscalationLevel::User;
        state.total_escalations = 2;

        let result = state.cancel_escalation(2000);
        assert!(result.is_ok());
        let msg = result.unwrap();
        assert!(msg.contains("L2"));
        assert_eq!(state.current_level(), EscalationLevel::SelfCorrection);
    }
}
