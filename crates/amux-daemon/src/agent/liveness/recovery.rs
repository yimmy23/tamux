//! Recovery system — strategies for resuming stuck or crashed goal runs.

use serde::{Deserialize, Serialize};

use crate::agent::types::StuckReason;

// Re-export state_layers items used in recovery context.
#[allow(unused_imports)]
use super::state_layers::*;

// ---------------------------------------------------------------------------
// Recovery strategy
// ---------------------------------------------------------------------------

/// What recovery action to take for a stuck goal run.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum RecoveryStrategy {
    /// Load checkpoint, inject recovery context, retry from that point.
    CheckpointRetry { checkpoint_id: String },
    /// Compress context to free space, retry from current position.
    CompressAndRetry,
    /// Escalate to user with options.
    EscalateToUser { message: String },
}

// ---------------------------------------------------------------------------
// Recovery outcome
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// Recovery attempt
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// Recovery planner
// ---------------------------------------------------------------------------

/// Decides which recovery strategy to use based on the stuck reason,
/// prior attempt count, and checkpoint availability.
pub struct RecoveryPlanner {
    /// Maximum number of automatic retries before escalating.
    max_auto_retries: u32,
    /// Base delay in seconds for exponential backoff.
    backoff_base_secs: u64,
}

impl Default for RecoveryPlanner {
    fn default() -> Self {
        Self {
            max_auto_retries: 3,
            backoff_base_secs: 30,
        }
    }
}

/// Maximum backoff in seconds (5 minutes).
const MAX_BACKOFF_SECS: u64 = 300;

impl RecoveryPlanner {
    /// Choose a recovery strategy based on the situation.
    ///
    /// Logic:
    /// - **Timeout** always escalates immediately — a human should decide.
    /// - **ResourceExhaustion** tries `CompressAndRetry` on the first attempt,
    ///   then escalates if that has already been tried.
    /// - For other reasons:
    ///   - attempt 0 with a checkpoint → `CheckpointRetry`
    ///   - attempt 0 without a checkpoint → `CompressAndRetry`
    ///   - attempt 1 → `CompressAndRetry`
    ///   - attempt 2+ → `EscalateToUser`
    pub fn plan_recovery(
        &self,
        stuck_reason: StuckReason,
        attempt_count: u32,
        has_checkpoint: bool,
    ) -> RecoveryStrategy {
        // Timeout is always user-actionable.
        if stuck_reason == StuckReason::Timeout {
            return RecoveryStrategy::EscalateToUser {
                message: "Goal run timed out. Please review and decide whether to extend \
                          the deadline, retry, or abort."
                    .into(),
            };
        }

        // Resource exhaustion — try compressing once, then escalate.
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

        // General strategy ladder for NoProgress, ErrorLoop, ToolCallLoop, etc.
        if attempt_count >= self.max_auto_retries.saturating_sub(1) {
            // Third attempt (index 2) or beyond → escalate.
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
                    checkpoint_id: String::new(), // caller fills in the actual id
                }
            } else {
                RecoveryStrategy::CompressAndRetry
            }
        } else {
            // attempt 1
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

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn planner() -> RecoveryPlanner {
        RecoveryPlanner::default()
    }

    // -- Strategy selection tests --

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

    // -- Backoff tests --

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
        // 30 * 2^4 = 480 → capped to 300
        assert_eq!(p.compute_backoff_secs(4), 300);
        assert_eq!(p.compute_backoff_secs(10), 300);
        assert_eq!(p.compute_backoff_secs(31), 300);
    }

    // -- Message tests --

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

    // -- Default planner tests --

    #[test]
    fn default_planner_has_reasonable_defaults() {
        let p = RecoveryPlanner::default();
        assert_eq!(p.max_auto_retries, 3);
        assert_eq!(p.backoff_base_secs, 30);
    }

    // -- RecoveryAttempt / RecoveryOutcome round-trip --

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
}
