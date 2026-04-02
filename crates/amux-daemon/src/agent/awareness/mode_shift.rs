//! Mode shift decision logic.
//!
//! Evaluates whether the agent should shift strategy based on two signals:
//! 1. Diminishing returns detected by AwarenessMonitor
//! 2. Counter-who confirmation of repeated approaches (AWAR-03)
//!
//! Mode shifts fire ONLY when BOTH signals agree -- counter-who acts as a
//! false positive guard to prevent interrupting legitimate repetitive work.

/// The result of a mode shift evaluation.
#[derive(Debug, Clone)]
pub struct ModeShiftDecision {
    pub should_shift: bool,
    pub reason: String,
    pub suggested_strategy: String,
}

/// Strategy rotation pool for suggested alternative approaches.
const STRATEGIES: &[&str] = &[
    "try different tool",
    "broaden search",
    "ask operator",
    "simplify approach",
];

/// Evaluate whether a mode shift should occur.
///
/// Per locked decision AWAR-03: counter-who is consulted before ALL mode shifts.
/// Returns `should_shift: true` ONLY when BOTH `diminishing_reason.is_some()`
/// AND `counter_who_confirms == true`.
///
/// `shift_index` is used to rotate through strategy suggestions (typically
/// derived from the entity's total failure count or similar counter).
pub fn evaluate_mode_shift(
    diminishing_reason: Option<String>,
    counter_who_confirms: bool,
) -> ModeShiftDecision {
    evaluate_mode_shift_with_index(diminishing_reason, counter_who_confirms, 0)
}

/// Same as `evaluate_mode_shift` but with explicit strategy rotation index.
pub fn evaluate_mode_shift_with_index(
    diminishing_reason: Option<String>,
    counter_who_confirms: bool,
    shift_index: usize,
) -> ModeShiftDecision {
    match diminishing_reason {
        Some(reason) if counter_who_confirms => {
            let strategy = STRATEGIES[shift_index % STRATEGIES.len()];
            ModeShiftDecision {
                should_shift: true,
                reason,
                suggested_strategy: strategy.to_string(),
            }
        }
        Some(reason) => {
            // Awareness detected diminishing returns but counter-who says
            // the repetition is legitimate -- suppress the mode shift.
            ModeShiftDecision {
                should_shift: false,
                reason: format!("Suppressed: {reason} (counter-who did not confirm)"),
                suggested_strategy: String::new(),
            }
        }
        None => ModeShiftDecision {
            should_shift: false,
            reason: "No diminishing returns detected".to_string(),
            suggested_strategy: String::new(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_shift_when_no_diminishing_returns() {
        let decision = evaluate_mode_shift(None, false);
        assert!(!decision.should_shift);
    }

    #[test]
    fn no_shift_when_counter_who_does_not_confirm() {
        let decision = evaluate_mode_shift(Some("stuck on same pattern".to_string()), false);
        assert!(!decision.should_shift);
        assert!(decision.reason.contains("Suppressed"));
    }

    #[test]
    fn shift_when_both_awareness_and_counter_who_agree() {
        let decision = evaluate_mode_shift(Some("3+ same tool calls".to_string()), true);
        assert!(decision.should_shift);
        assert!(!decision.suggested_strategy.is_empty());
    }

    #[test]
    fn strategy_rotates_with_index() {
        let d0 = evaluate_mode_shift_with_index(Some("stuck".to_string()), true, 0);
        let d1 = evaluate_mode_shift_with_index(Some("stuck".to_string()), true, 1);
        let d2 = evaluate_mode_shift_with_index(Some("stuck".to_string()), true, 2);
        let d3 = evaluate_mode_shift_with_index(Some("stuck".to_string()), true, 3);
        assert_eq!(d0.suggested_strategy, "try different tool");
        assert_eq!(d1.suggested_strategy, "broaden search");
        assert_eq!(d2.suggested_strategy, "ask operator");
        assert_eq!(d3.suggested_strategy, "simplify approach");
        // Wraps around
        let d4 = evaluate_mode_shift_with_index(Some("stuck".to_string()), true, 4);
        assert_eq!(d4.suggested_strategy, "try different tool");
    }

    #[test]
    fn no_shift_when_no_diminishing_and_counter_who_confirms() {
        let decision = evaluate_mode_shift(None, true);
        assert!(!decision.should_shift);
    }
}
