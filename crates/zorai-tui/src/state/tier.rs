//! Capability tier state for TUI -- pre-computed visibility flags.
//!
//! Flags are computed once when the tier changes, not on every render frame
//! (Pitfall 4: TUI widget rendering performance).

use serde::{Deserialize, Serialize};

/// Pre-computed feature visibility for the current capability tier.
/// Widgets check these booleans directly -- no tier logic per frame.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TierState {
    pub current_tier: String,
    pub show_goal_runs: bool,
    pub show_task_queue: bool,
    pub show_gateway_config: bool,
    pub show_memory_controls: bool,
    pub show_subagents: bool,
    pub show_advanced_settings: bool,
}

impl Default for TierState {
    fn default() -> Self {
        Self::from_tier("newcomer")
    }
}

impl TierState {
    /// Compute visibility flags from a tier string.
    /// Called once on tier change, not per render frame.
    pub fn from_tier(tier: &str) -> Self {
        let tier_ord = match tier {
            "newcomer" => 0,
            "familiar" => 1,
            "power_user" => 2,
            "expert" => 3,
            _ => 0,
        };
        Self {
            current_tier: tier.to_string(),
            show_goal_runs: tier_ord >= 1,
            show_task_queue: tier_ord >= 1,
            show_gateway_config: tier_ord >= 1,
            show_memory_controls: tier_ord >= 3,
            show_subagents: tier_ord >= 2,
            show_advanced_settings: tier_ord >= 2,
        }
    }

    /// Update tier from a tier_changed daemon event.
    pub fn on_tier_changed(&mut self, new_tier: &str) {
        *self = Self::from_tier(new_tier);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn newcomer_hides_all_advanced() {
        let state = TierState::from_tier("newcomer");
        assert!(!state.show_goal_runs);
        assert!(!state.show_task_queue);
        assert!(!state.show_gateway_config);
        assert!(!state.show_subagents);
        assert!(!state.show_advanced_settings);
        assert!(!state.show_memory_controls);
    }

    #[test]
    fn familiar_shows_goals_tasks_gateway() {
        let state = TierState::from_tier("familiar");
        assert!(state.show_goal_runs);
        assert!(state.show_task_queue);
        assert!(state.show_gateway_config);
        assert!(!state.show_subagents);
        assert!(!state.show_advanced_settings);
        assert!(!state.show_memory_controls);
    }

    #[test]
    fn power_user_shows_subagents_advanced() {
        let state = TierState::from_tier("power_user");
        assert!(state.show_goal_runs);
        assert!(state.show_task_queue);
        assert!(state.show_gateway_config);
        assert!(state.show_subagents);
        assert!(state.show_advanced_settings);
        assert!(!state.show_memory_controls);
    }

    #[test]
    fn expert_shows_everything() {
        let state = TierState::from_tier("expert");
        assert!(state.show_goal_runs);
        assert!(state.show_task_queue);
        assert!(state.show_gateway_config);
        assert!(state.show_subagents);
        assert!(state.show_advanced_settings);
        assert!(state.show_memory_controls);
    }

    #[test]
    fn unknown_tier_defaults_to_newcomer() {
        let state = TierState::from_tier("unknown");
        assert!(!state.show_goal_runs);
        assert!(!state.show_subagents);
    }

    #[test]
    fn on_tier_changed_updates_flags() {
        let mut state = TierState::from_tier("newcomer");
        assert!(!state.show_goal_runs);
        state.on_tier_changed("familiar");
        assert!(state.show_goal_runs);
        assert_eq!(state.current_tier, "familiar");
    }
}
