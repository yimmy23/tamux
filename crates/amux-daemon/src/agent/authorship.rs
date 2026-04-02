//! Shared authorship metadata — tracks whether goal-run output came from operator,
//! agent, or joint collaboration.
//!
//! Attribution is metadata on `GoalRun` output, not inline commentary (AUTH-02).

use serde::{Deserialize, Serialize};

/// Tags the contribution source for a completed goal run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthorshipTag {
    /// Output driven entirely by operator-supplied goal text with no agent synthesis.
    Operator,
    /// Output driven entirely by agent self-initiation (e.g., heartbeat-triggered goal).
    Agent,
    /// Output is a joint collaboration — operator provided the goal, agent executed it.
    Joint,
}

impl Default for AuthorshipTag {
    fn default() -> Self {
        Self::Joint
    }
}

/// Classify the authorship of a goal run based on participation signals.
///
/// - Both operator goal text and agent synthesis -> `Joint`
/// - Only operator goal text (rare: agent did nothing) -> `Operator`
/// - Only agent synthesis (rare: self-initiated goal) -> `Agent`
/// - Neither (fallback) -> `Joint`
pub fn classify_authorship(
    has_operator_goal_text: bool,
    has_agent_synthesis: bool,
) -> AuthorshipTag {
    match (has_operator_goal_text, has_agent_synthesis) {
        (true, true) => AuthorshipTag::Joint,
        (true, false) => AuthorshipTag::Operator,
        (false, true) => AuthorshipTag::Agent,
        (false, false) => AuthorshipTag::Joint,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_joint() {
        assert_eq!(AuthorshipTag::default(), AuthorshipTag::Joint);
    }

    #[test]
    fn serde_round_trip() {
        for (tag, expected) in [
            (AuthorshipTag::Operator, "\"operator\""),
            (AuthorshipTag::Agent, "\"agent\""),
            (AuthorshipTag::Joint, "\"joint\""),
        ] {
            let json = serde_json::to_string(&tag).unwrap();
            assert_eq!(json, expected, "serialization mismatch for {tag:?}");
            let back: AuthorshipTag = serde_json::from_str(&json).unwrap();
            assert_eq!(back, tag, "deserialization mismatch for {tag:?}");
        }
    }

    #[test]
    fn classify_operator_only() {
        assert_eq!(classify_authorship(true, false), AuthorshipTag::Operator);
    }

    #[test]
    fn classify_agent_only() {
        assert_eq!(classify_authorship(false, true), AuthorshipTag::Agent);
    }

    #[test]
    fn classify_both_is_joint() {
        assert_eq!(classify_authorship(true, true), AuthorshipTag::Joint);
    }

    #[test]
    fn classify_neither_is_joint() {
        assert_eq!(classify_authorship(false, false), AuthorshipTag::Joint);
    }
}
