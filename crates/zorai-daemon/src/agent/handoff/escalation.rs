//! Escalation trigger evaluation for handoff rules.
//!
//! Evaluates escalation triggers (ConfidenceBelow, ToolFails, TimeExceeds)
//! against runtime state and returns the first matching action.

use super::{HandoffEscalationAction, HandoffEscalationRule, HandoffEscalationTrigger};

/// Map a confidence band name to a numeric ordering for comparison.
///
/// Supports both internal names (guessing, uncertain, likely, confident)
/// and user-facing aliases (low, medium, high). Case-insensitive.
/// Unknown names default to 0 (lowest).
pub fn confidence_band_order(band: &str) -> u8 {
    match band.to_lowercase().as_str() {
        "guessing" => 0,
        "uncertain" | "low" => 1,
        "likely" | "medium" => 2,
        "confident" | "high" => 3,
        _ => 0,
    }
}

/// Evaluate escalation rules against runtime state.
///
/// Returns the action from the first matching trigger, or None.
/// - ConfidenceBelow: fires when current confidence < threshold
/// - ToolFails: fires when consecutive_failures >= threshold
/// - TimeExceeds: fires when elapsed_secs >= threshold
pub fn evaluate_escalation_triggers(
    rules: &[HandoffEscalationRule],
    consecutive_failures: u32,
    elapsed_secs: u64,
    confidence_band: &str,
) -> Option<HandoffEscalationAction> {
    let current_order = confidence_band_order(confidence_band);

    for rule in rules {
        let fires = match &rule.trigger {
            HandoffEscalationTrigger::ConfidenceBelow(threshold) => {
                current_order < confidence_band_order(threshold)
            }
            HandoffEscalationTrigger::ToolFails(threshold) => consecutive_failures >= *threshold,
            HandoffEscalationTrigger::TimeExceeds(threshold) => elapsed_secs >= *threshold,
        };

        if fires {
            return Some(rule.action.clone());
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_rule(
        trigger: HandoffEscalationTrigger,
        action: HandoffEscalationAction,
    ) -> HandoffEscalationRule {
        HandoffEscalationRule { trigger, action }
    }

    #[test]
    fn test_confidence_band_order_guessing() {
        assert_eq!(confidence_band_order("guessing"), 0);
    }

    #[test]
    fn test_confidence_band_order_uncertain_and_low() {
        assert_eq!(confidence_band_order("uncertain"), 1);
        assert_eq!(confidence_band_order("low"), 1);
    }

    #[test]
    fn test_confidence_band_order_likely_and_medium() {
        assert_eq!(confidence_band_order("likely"), 2);
        assert_eq!(confidence_band_order("medium"), 2);
    }

    #[test]
    fn test_confidence_band_order_confident_and_high() {
        assert_eq!(confidence_band_order("confident"), 3);
        assert_eq!(confidence_band_order("high"), 3);
    }

    #[test]
    fn test_confidence_band_order_case_insensitive() {
        assert_eq!(confidence_band_order("CONFIDENT"), 3);
        assert_eq!(confidence_band_order("Guessing"), 0);
        assert_eq!(confidence_band_order("LOW"), 1);
    }

    #[test]
    fn test_confidence_band_order_unknown_defaults_to_zero() {
        assert_eq!(confidence_band_order("unknown"), 0);
        assert_eq!(confidence_band_order(""), 0);
    }

    #[test]
    fn test_confidence_below_fires_guessing_below_low() {
        // "low" = 1, "guessing" = 0 => 0 < 1 => fires
        let rules = vec![make_rule(
            HandoffEscalationTrigger::ConfidenceBelow("low".to_string()),
            HandoffEscalationAction::HandBack,
        )];
        let result = evaluate_escalation_triggers(&rules, 0, 0, "guessing");
        assert!(result.is_some());
    }

    #[test]
    fn test_confidence_below_fires_low_below_medium() {
        let rules = vec![make_rule(
            HandoffEscalationTrigger::ConfidenceBelow("medium".to_string()),
            HandoffEscalationAction::HandBack,
        )];
        let result = evaluate_escalation_triggers(&rules, 0, 0, "low");
        assert!(result.is_some());
    }

    #[test]
    fn test_confidence_below_does_not_fire_high_above_medium() {
        let rules = vec![make_rule(
            HandoffEscalationTrigger::ConfidenceBelow("medium".to_string()),
            HandoffEscalationAction::HandBack,
        )];
        let result = evaluate_escalation_triggers(&rules, 0, 0, "high");
        assert!(result.is_none());
    }

    #[test]
    fn test_tool_fails_fires_at_threshold() {
        let rules = vec![make_rule(
            HandoffEscalationTrigger::ToolFails(3),
            HandoffEscalationAction::AbortWithReport,
        )];
        let result = evaluate_escalation_triggers(&rules, 3, 0, "confident");
        assert!(result.is_some());
    }

    #[test]
    fn test_tool_fails_does_not_fire_below_threshold() {
        let rules = vec![make_rule(
            HandoffEscalationTrigger::ToolFails(3),
            HandoffEscalationAction::AbortWithReport,
        )];
        let result = evaluate_escalation_triggers(&rules, 2, 0, "confident");
        assert!(result.is_none());
    }

    #[test]
    fn test_time_exceeds_fires_at_threshold() {
        let rules = vec![make_rule(
            HandoffEscalationTrigger::TimeExceeds(300),
            HandoffEscalationAction::RetryWithNewContext,
        )];
        let result = evaluate_escalation_triggers(&rules, 0, 300, "confident");
        assert!(result.is_some());
    }

    #[test]
    fn test_returns_first_matching_action() {
        let rules = vec![
            make_rule(
                HandoffEscalationTrigger::ToolFails(5),
                HandoffEscalationAction::HandBack,
            ),
            make_rule(
                HandoffEscalationTrigger::TimeExceeds(100),
                HandoffEscalationAction::AbortWithReport,
            ),
        ];
        // Only time trigger fires (failures=0 < 5)
        let result = evaluate_escalation_triggers(&rules, 0, 200, "confident");
        assert!(matches!(
            result,
            Some(HandoffEscalationAction::AbortWithReport)
        ));
    }

    #[test]
    fn test_returns_none_when_no_triggers_fire() {
        let rules = vec![
            make_rule(
                HandoffEscalationTrigger::ToolFails(5),
                HandoffEscalationAction::HandBack,
            ),
            make_rule(
                HandoffEscalationTrigger::TimeExceeds(300),
                HandoffEscalationAction::AbortWithReport,
            ),
        ];
        let result = evaluate_escalation_triggers(&rules, 0, 100, "confident");
        assert!(result.is_none());
    }
}
