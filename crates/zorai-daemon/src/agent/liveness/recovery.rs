//! Recovery system — strategies for resuming stuck or crashed goal runs.

use serde::{Deserialize, Serialize};

use crate::agent::types::StuckReason;

#[allow(unused_imports)]
use super::state_layers::*;

/// What recovery action to take for a stuck goal run.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum RecoveryStrategy {
    /// Load checkpoint, inject recovery context, retry from that point.
    CheckpointRetry { checkpoint_id: String },
    /// Compress context to free space, retry from current position.
    CompressAndRetry,
    /// Try a narrower specialist subagent before bothering the operator.
    /// `role` is the specialist role name (e.g., "debugger", "researcher")
    /// the planner believes is best matched to the failure pattern.
    SpawnSpecialist { role: String, message: String },
    /// Escalate to user with options.
    EscalateToUser { message: String },
    /// Notify a configured external channel (webhook, ops bus, paging sink)
    /// when the operator may not see the surface in time. Only emitted when
    /// the planner has been told an external sink is available.
    NotifyExternal { channel: String, message: String },
}

/// Result of executing a recovery strategy.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum RecoveryOutcome {
    /// The goal run was successfully recovered and resumed.
    Recovered { resumed_at_step: usize },
    /// The recovery attempt failed.
    Failed { reason: String },
    /// The problem was escalated to a human or parent agent.
    Escalated { to: String },
}

/// Record of a single recovery attempt against a goal run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoveryAttempt {
    pub id: String,
    pub goal_run_id: String,
    pub strategy: RecoveryStrategy,
    pub attempt_number: u32,
    pub created_at: u64,
    pub outcome: Option<RecoveryOutcome>,
}

/// Decides which recovery strategy to use based on the stuck reason,
/// prior attempt count, and checkpoint availability.
pub struct RecoveryPlanner {
    /// Maximum number of automatic retries before escalating.
    max_auto_retries: u32,
    /// Base delay in seconds for exponential backoff.
    backoff_base_secs: u64,
    /// Specialist role to consider for `SpawnSpecialist` before
    /// escalating to the operator. `None` means no specialist is
    /// available, so the planner skips that ladder step.
    specialist_role: Option<String>,
    /// External notification sink identifier (e.g., "webhook:default").
    /// `None` means no sink is configured, so `NotifyExternal` is never
    /// emitted.
    external_channel: Option<String>,
}

impl Default for RecoveryPlanner {
    fn default() -> Self {
        Self {
            max_auto_retries: 3,
            backoff_base_secs: 30,
            specialist_role: None,
            external_channel: None,
        }
    }
}

impl RecoveryPlanner {
    /// Builder: configure a specialist role the planner can route to as
    /// the sub-agent-help rung of the recovery ladder.
    pub fn with_specialist_role(mut self, role: impl Into<String>) -> Self {
        self.specialist_role = Some(role.into());
        self
    }

    /// Builder: configure an external notification channel for the
    /// final rung of the recovery ladder.
    pub fn with_external_channel(mut self, channel: impl Into<String>) -> Self {
        self.external_channel = Some(channel.into());
        self
    }
}

/// Maximum backoff in seconds (5 minutes).
const MAX_BACKOFF_SECS: u64 = 300;

impl RecoveryPlanner {
    /// Choose a recovery strategy based on the situation.
    ///
    /// Graduated escalation ladder:
    /// 1. **Self-correction** — `CheckpointRetry` / `CompressAndRetry` on
    ///    early attempts.
    /// 2. **Sub-agent help** — `SpawnSpecialist` if a specialist role is
    ///    configured and self-correction has already been tried.
    /// 3. **Operator escalation** — `EscalateToUser` when no automated rung
    ///    remains.
    /// 4. **External escalation** — `NotifyExternal` when an external sink
    ///    is configured and the situation warrants out-of-band paging
    ///    (timeout exhaustion, post-operator-escalation attempts).
    ///
    /// Reason-specific overrides:
    /// - **Timeout** skips to operator escalation, then to external if
    ///   configured and retries have been exhausted.
    /// - **ResourceExhaustion** tries `CompressAndRetry` once, then
    ///   escalates.
    pub fn plan_recovery(
        &self,
        stuck_reason: StuckReason,
        attempt_count: u32,
        has_checkpoint: bool,
    ) -> RecoveryStrategy {
        if stuck_reason == StuckReason::Timeout {
            // After repeated unacknowledged timeouts, fall through to the
            // external channel if one is registered — the operator likely
            // isn't watching the surface.
            if attempt_count >= self.max_auto_retries {
                if let Some(channel) = &self.external_channel {
                    return RecoveryStrategy::NotifyExternal {
                        channel: channel.clone(),
                        message: "Goal run repeatedly timed out without operator response.".into(),
                    };
                }
            }
            return RecoveryStrategy::EscalateToUser {
                message: "Goal run timed out. Please review and decide whether to extend \
                          the deadline, retry, or abort."
                    .into(),
            };
        }

        if stuck_reason == StuckReason::ResourceExhaustion {
            return if attempt_count == 0 {
                RecoveryStrategy::CompressAndRetry
            } else {
                RecoveryStrategy::EscalateToUser {
                    message: "Context budget is nearly exhausted even after compression. \
                              Manual intervention required."
                        .into(),
                }
            };
        }

        // Final rung for non-Timeout/non-ResourceExhaustion reasons.
        if attempt_count >= self.max_auto_retries.saturating_sub(1) {
            // If a specialist hasn't yet been tried, try them before the
            // operator. We treat "specialist attempt" as the rung exactly
            // at `max_auto_retries - 1`; the operator escalation moves up
            // by one attempt so the sub-agent rung is reachable.
            if let Some(role) = &self.specialist_role {
                if attempt_count == self.max_auto_retries.saturating_sub(1) {
                    return RecoveryStrategy::SpawnSpecialist {
                        role: role.clone(),
                        message: format!(
                            "Self-correction failed after {} attempt(s); routing to \
                             specialist '{}' before escalating to the operator.",
                            attempt_count, role
                        ),
                    };
                }
            }
            // Operator escalation when no specialist rung remains.
            // If an external channel is configured and even the operator
            // rung has already fired, fall through to external paging.
            if self.specialist_role.is_some() && attempt_count > self.max_auto_retries {
                if let Some(channel) = &self.external_channel {
                    return RecoveryStrategy::NotifyExternal {
                        channel: channel.clone(),
                        message: format!(
                            "Goal run remains stuck after {} attempt(s) and operator \
                             escalation. Paging external channel.",
                            attempt_count
                        ),
                    };
                }
            }
            return RecoveryStrategy::EscalateToUser {
                message: format!(
                    "Automatic recovery has been attempted {} time(s) without success. \
                     Please intervene manually.",
                    attempt_count
                ),
            };
        }

        if attempt_count == 0 {
            if has_checkpoint {
                RecoveryStrategy::CheckpointRetry {
                    checkpoint_id: String::new(),
                }
            } else {
                RecoveryStrategy::CompressAndRetry
            }
        } else {
            RecoveryStrategy::CompressAndRetry
        }
    }

    /// Compute exponential backoff delay capped at [`MAX_BACKOFF_SECS`].
    ///
    /// Formula: `base * 2^attempt`, capped at 300 s.
    pub fn compute_backoff_secs(&self, attempt: u32) -> u64 {
        let delay = self
            .backoff_base_secs
            .saturating_mul(1u64 << attempt.min(31));
        delay.min(MAX_BACKOFF_SECS)
    }

    /// Build a human-readable message describing the stuck situation.
    pub fn build_recovery_message(
        goal_title: &str,
        stuck_reason: StuckReason,
        attempt: u32,
    ) -> String {
        let reason_text = match stuck_reason {
            StuckReason::NoProgress => "no progress detected for the configured timeout",
            StuckReason::ErrorLoop => "the same error is repeating in a loop",
            StuckReason::ToolCallLoop => "tool calls are cycling without making progress",
            StuckReason::ResourceExhaustion => "the context budget is nearly exhausted",
            StuckReason::Timeout => "the maximum allowed duration was exceeded",
        };

        format!(
            "Goal '{}' is stuck: {}. This is recovery attempt {}/3. \
             Options: retry from checkpoint, compress context and retry, or abort.",
            goal_title, reason_text, attempt,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn planner() -> RecoveryPlanner {
        RecoveryPlanner::default()
    }

    #[test]
    fn first_attempt_with_checkpoint_uses_checkpoint_retry() {
        let strategy = planner().plan_recovery(StuckReason::NoProgress, 0, true);
        assert!(
            matches!(strategy, RecoveryStrategy::CheckpointRetry { .. }),
            "expected CheckpointRetry, got {:?}",
            strategy
        );
    }

    #[test]
    fn first_attempt_without_checkpoint_uses_compress_and_retry() {
        let strategy = planner().plan_recovery(StuckReason::NoProgress, 0, false);
        assert!(
            matches!(strategy, RecoveryStrategy::CompressAndRetry),
            "expected CompressAndRetry, got {:?}",
            strategy
        );
    }

    #[test]
    fn second_attempt_uses_compress_and_retry() {
        let strategy = planner().plan_recovery(StuckReason::ErrorLoop, 1, true);
        assert!(
            matches!(strategy, RecoveryStrategy::CompressAndRetry),
            "expected CompressAndRetry, got {:?}",
            strategy
        );
    }

    #[test]
    fn third_attempt_escalates_to_user() {
        let strategy = planner().plan_recovery(StuckReason::ErrorLoop, 2, true);
        assert!(
            matches!(strategy, RecoveryStrategy::EscalateToUser { .. }),
            "expected EscalateToUser, got {:?}",
            strategy
        );
    }

    #[test]
    fn timeout_always_escalates() {
        for attempt in 0..5 {
            let strategy = planner().plan_recovery(StuckReason::Timeout, attempt, true);
            assert!(
                matches!(strategy, RecoveryStrategy::EscalateToUser { .. }),
                "expected EscalateToUser for timeout at attempt {}, got {:?}",
                attempt,
                strategy
            );
        }
    }

    #[test]
    fn resource_exhaustion_first_try_compresses() {
        let strategy = planner().plan_recovery(StuckReason::ResourceExhaustion, 0, true);
        assert!(
            matches!(strategy, RecoveryStrategy::CompressAndRetry),
            "expected CompressAndRetry for ResourceExhaustion attempt 0, got {:?}",
            strategy
        );
    }

    #[test]
    fn resource_exhaustion_second_try_escalates() {
        let strategy = planner().plan_recovery(StuckReason::ResourceExhaustion, 1, true);
        assert!(
            matches!(strategy, RecoveryStrategy::EscalateToUser { .. }),
            "expected EscalateToUser for ResourceExhaustion attempt 1, got {:?}",
            strategy
        );
    }

    #[test]
    fn backoff_exponential_sequence() {
        let p = planner();
        assert_eq!(p.compute_backoff_secs(0), 30);
        assert_eq!(p.compute_backoff_secs(1), 60);
        assert_eq!(p.compute_backoff_secs(2), 120);
        assert_eq!(p.compute_backoff_secs(3), 240);
    }

    #[test]
    fn backoff_caps_at_five_minutes() {
        let p = planner();
        assert_eq!(p.compute_backoff_secs(4), 300);
        assert_eq!(p.compute_backoff_secs(10), 300);
        assert_eq!(p.compute_backoff_secs(31), 300);
    }

    #[test]
    fn recovery_message_includes_goal_title() {
        let msg = RecoveryPlanner::build_recovery_message("Deploy v2", StuckReason::NoProgress, 1);
        assert!(
            msg.contains("Deploy v2"),
            "message should contain goal title: {}",
            msg
        );
    }

    #[test]
    fn recovery_message_includes_attempt_count() {
        let msg = RecoveryPlanner::build_recovery_message("Deploy v2", StuckReason::ErrorLoop, 2);
        assert!(
            msg.contains("2/3"),
            "message should contain attempt count: {}",
            msg
        );
    }

    #[test]
    fn default_planner_has_reasonable_defaults() {
        let p = RecoveryPlanner::default();
        assert_eq!(p.max_auto_retries, 3);
        assert_eq!(p.backoff_base_secs, 30);
    }

    #[test]
    fn recovery_attempt_serialization_roundtrip() {
        let attempt = RecoveryAttempt {
            id: "ra_1".into(),
            goal_run_id: "goal_1".into(),
            strategy: RecoveryStrategy::CompressAndRetry,
            attempt_number: 1,
            created_at: 12345,
            outcome: Some(RecoveryOutcome::Recovered { resumed_at_step: 3 }),
        };
        let json = serde_json::to_string(&attempt).unwrap();
        let restored: RecoveryAttempt = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.id, "ra_1");
        assert_eq!(restored.attempt_number, 1);
        assert!(matches!(
            restored.outcome,
            Some(RecoveryOutcome::Recovered { resumed_at_step: 3 })
        ));
    }

    #[test]
    fn recovery_outcome_failed_roundtrip() {
        let outcome = RecoveryOutcome::Failed {
            reason: "disk full".into(),
        };
        let json = serde_json::to_string(&outcome).unwrap();
        let restored: RecoveryOutcome = serde_json::from_str(&json).unwrap();
        assert!(matches!(restored, RecoveryOutcome::Failed { reason } if reason == "disk full"));
    }

    #[test]
    fn recovery_outcome_escalated_roundtrip() {
        let outcome = RecoveryOutcome::Escalated { to: "user".into() };
        let json = serde_json::to_string(&outcome).unwrap();
        let restored: RecoveryOutcome = serde_json::from_str(&json).unwrap();
        assert!(matches!(restored, RecoveryOutcome::Escalated { to } if to == "user"));
    }

    // --- Ladder extension tests (sub-agent + external escalation) ---

    fn specialist_planner() -> RecoveryPlanner {
        RecoveryPlanner::default().with_specialist_role("debugger")
    }

    fn full_ladder_planner() -> RecoveryPlanner {
        RecoveryPlanner::default()
            .with_specialist_role("debugger")
            .with_external_channel("webhook:ops")
    }

    #[test]
    fn specialist_rung_fires_before_operator_when_configured() {
        let strategy = specialist_planner().plan_recovery(StuckReason::ErrorLoop, 2, true);
        assert!(
            matches!(strategy, RecoveryStrategy::SpawnSpecialist { ref role, .. } if role == "debugger"),
            "expected SpawnSpecialist at attempt 2 with specialist configured, got {:?}",
            strategy
        );
    }

    #[test]
    fn specialist_rung_is_skipped_when_no_specialist_configured() {
        // Default planner — no specialist. Should escalate directly.
        let strategy = planner().plan_recovery(StuckReason::ErrorLoop, 2, true);
        assert!(
            matches!(strategy, RecoveryStrategy::EscalateToUser { .. }),
            "expected EscalateToUser when no specialist is configured, got {:?}",
            strategy
        );
    }

    #[test]
    fn operator_escalation_still_fires_after_specialist_attempt() {
        // Attempt 3 = past the specialist rung but before external.
        let strategy = full_ladder_planner().plan_recovery(StuckReason::ErrorLoop, 3, true);
        assert!(
            matches!(strategy, RecoveryStrategy::EscalateToUser { .. }),
            "expected EscalateToUser after specialist attempt, got {:?}",
            strategy
        );
    }

    #[test]
    fn external_channel_fires_after_operator_when_configured() {
        // Attempt 4 = past operator rung, full ladder.
        let strategy = full_ladder_planner().plan_recovery(StuckReason::ErrorLoop, 4, true);
        assert!(
            matches!(strategy, RecoveryStrategy::NotifyExternal { ref channel, .. } if channel == "webhook:ops"),
            "expected NotifyExternal at attempt 4 with full ladder, got {:?}",
            strategy
        );
    }

    #[test]
    fn external_channel_skipped_when_not_configured() {
        // Specialist configured but no external — should stay at EscalateToUser.
        let strategy = specialist_planner().plan_recovery(StuckReason::ErrorLoop, 5, true);
        assert!(
            matches!(strategy, RecoveryStrategy::EscalateToUser { .. }),
            "expected EscalateToUser when external is not configured, got {:?}",
            strategy
        );
    }

    #[test]
    fn timeout_pages_external_after_repeated_unacknowledged_attempts() {
        // Timeout reason: external paging fires when attempt >= max_auto_retries
        // and a channel is configured.
        let strategy = full_ladder_planner().plan_recovery(StuckReason::Timeout, 3, true);
        assert!(
            matches!(strategy, RecoveryStrategy::NotifyExternal { ref channel, .. } if channel == "webhook:ops"),
            "expected NotifyExternal for repeated Timeout with external configured, got {:?}",
            strategy
        );
    }

    #[test]
    fn timeout_escalates_to_user_when_no_external_configured() {
        // No external channel → timeout always escalates to user.
        let strategy = planner().plan_recovery(StuckReason::Timeout, 3, true);
        assert!(
            matches!(strategy, RecoveryStrategy::EscalateToUser { .. }),
            "expected EscalateToUser for Timeout without external, got {:?}",
            strategy
        );
    }

    #[test]
    fn early_attempts_still_use_self_correction_with_full_ladder() {
        // Adding sub-agent + external sinks must not disrupt the
        // self-correction rung.
        let p = full_ladder_planner();
        assert!(matches!(
            p.plan_recovery(StuckReason::ErrorLoop, 0, true),
            RecoveryStrategy::CheckpointRetry { .. }
        ));
        assert!(matches!(
            p.plan_recovery(StuckReason::ErrorLoop, 1, true),
            RecoveryStrategy::CompressAndRetry
        ));
    }

    #[test]
    fn spawn_specialist_serialization_roundtrip() {
        let strategy = RecoveryStrategy::SpawnSpecialist {
            role: "researcher".into(),
            message: "Routing to specialist".into(),
        };
        let json = serde_json::to_string(&strategy).unwrap();
        let restored: RecoveryStrategy = serde_json::from_str(&json).unwrap();
        assert!(matches!(
            restored,
            RecoveryStrategy::SpawnSpecialist { ref role, .. } if role == "researcher"
        ));
    }

    #[test]
    fn notify_external_serialization_roundtrip() {
        let strategy = RecoveryStrategy::NotifyExternal {
            channel: "webhook:ops".into(),
            message: "out-of-band page".into(),
        };
        let json = serde_json::to_string(&strategy).unwrap();
        let restored: RecoveryStrategy = serde_json::from_str(&json).unwrap();
        assert!(matches!(
            restored,
            RecoveryStrategy::NotifyExternal { ref channel, .. } if channel == "webhook:ops"
        ));
    }
}
