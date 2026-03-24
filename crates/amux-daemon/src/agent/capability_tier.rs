//! Capability tier system -- progressive feature disclosure driven by operator model.
//!
//! The daemon resolves a [`CapabilityTier`] from operator-model signals, an
//! optional self-assessment, and an optional user override.  The tier drives
//! which features are visible to the operator and controls the progressive
//! disclosure queue.

use serde::{Deserialize, Serialize};

use super::operator_model::RiskTolerance;

// ---------------------------------------------------------------------------
// CapabilityTier enum
// ---------------------------------------------------------------------------

/// Progressive capability tier.  Order matters for `PartialOrd`/`Ord` -- each
/// successive variant represents a higher tier.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityTier {
    Newcomer,
    Familiar,
    PowerUser,
    Expert,
}

impl std::fmt::Display for CapabilityTier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Newcomer => write!(f, "newcomer"),
            Self::Familiar => write!(f, "familiar"),
            Self::PowerUser => write!(f, "power_user"),
            Self::Expert => write!(f, "expert"),
        }
    }
}

impl CapabilityTier {
    /// Parse from a string (e.g. from protocol messages or config).
    pub fn from_str_loose(s: &str) -> Option<Self> {
        match s.to_lowercase().trim() {
            "newcomer" => Some(Self::Newcomer),
            "familiar" => Some(Self::Familiar),
            "power_user" | "poweruser" => Some(Self::PowerUser),
            "expert" => Some(Self::Expert),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------
// TierSignals -- inputs to resolve_tier()
// ---------------------------------------------------------------------------

/// Aggregated signals used to compute the current capability tier.
#[derive(Debug, Clone)]
pub(super) struct TierSignals {
    pub session_count: u64,
    pub unique_tools_used: usize,
    pub goal_runs_completed: u64,
    pub risk_tolerance: RiskTolerance,
    pub user_self_assessment: Option<CapabilityTier>,
    pub user_override: Option<CapabilityTier>,
}

// ---------------------------------------------------------------------------
// resolve_tier() -- pure function
// ---------------------------------------------------------------------------

/// Compute the effective tier from the given signals.
///
/// Rules:
/// 1. `user_override` always wins (D-03).
/// 2. Compute a *behavioral* tier from session/tool/goal signals.
/// 3. `self_assessment` can *elevate* the behavioral tier but never demote it
///    (D-01: hybrid, elevates only).
pub(super) fn resolve_tier(signals: &TierSignals) -> CapabilityTier {
    // Rule 1: override takes precedence
    if let Some(tier) = signals.user_override {
        return tier;
    }

    // Rule 2: behavioral tier from signals
    let behavioral = if signals.goal_runs_completed >= 10
        && signals.unique_tools_used >= 8
        && signals.risk_tolerance == RiskTolerance::Aggressive
    {
        CapabilityTier::Expert
    } else if signals.goal_runs_completed >= 3 && signals.unique_tools_used >= 5 {
        CapabilityTier::PowerUser
    } else if signals.session_count >= 5 && signals.unique_tools_used >= 3 {
        CapabilityTier::Familiar
    } else {
        CapabilityTier::Newcomer
    };

    // Rule 3: self-assessment elevates only
    if let Some(assessment) = signals.user_self_assessment {
        if assessment > behavioral {
            return assessment;
        }
    }

    behavioral
}

// ---------------------------------------------------------------------------
// TierFeatureFlags -- per-tier feature visibility
// ---------------------------------------------------------------------------

/// Feature visibility flags driven by the current tier (D-04).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct TierFeatureFlags {
    pub show_goal_runs: bool,
    pub show_task_queue: bool,
    pub show_gateway_config: bool,
    pub show_subagents: bool,
    pub show_advanced_settings: bool,
    pub show_memory_controls: bool,
}

/// Map a tier to its visible feature set.
pub(super) fn tier_features_visible(tier: CapabilityTier) -> TierFeatureFlags {
    match tier {
        CapabilityTier::Newcomer => TierFeatureFlags {
            show_goal_runs: false,
            show_task_queue: false,
            show_gateway_config: false,
            show_subagents: false,
            show_advanced_settings: false,
            show_memory_controls: false,
        },
        CapabilityTier::Familiar => TierFeatureFlags {
            show_goal_runs: true,
            show_task_queue: true,
            show_gateway_config: true,
            show_subagents: false,
            show_advanced_settings: false,
            show_memory_controls: false,
        },
        CapabilityTier::PowerUser => TierFeatureFlags {
            show_goal_runs: true,
            show_task_queue: true,
            show_gateway_config: true,
            show_subagents: true,
            show_advanced_settings: true,
            show_memory_controls: false,
        },
        CapabilityTier::Expert => TierFeatureFlags {
            show_goal_runs: true,
            show_task_queue: true,
            show_gateway_config: true,
            show_subagents: true,
            show_advanced_settings: true,
            show_memory_controls: true,
        },
    }
}

// ---------------------------------------------------------------------------
// DisclosureQueue -- one-per-session feature draining (D-13)
// ---------------------------------------------------------------------------

/// A single feature disclosure entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeatureDisclosure {
    pub feature_id: String,
    pub tier: CapabilityTier,
    pub title: String,
    pub description: String,
}

/// Queue of features awaiting progressive disclosure.  At most one feature is
/// surfaced per session (D-13).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DisclosureQueue {
    pub pending_features: Vec<FeatureDisclosure>,
    pub disclosed_features: Vec<String>,
    pub last_disclosure_session: u64,
}

impl DisclosureQueue {
    /// Returns the next feature to disclose, or `None` if we already disclosed
    /// one this session or if the queue is empty.
    pub fn next_disclosure(&self, current_session: u64) -> Option<&FeatureDisclosure> {
        if self.last_disclosure_session == current_session {
            return None;
        }
        self.pending_features.first()
    }

    /// Mark a feature as disclosed and update the session watermark.
    pub fn mark_disclosed(&mut self, feature_id: &str, current_session: u64) {
        self.pending_features
            .retain(|f| f.feature_id != feature_id);
        if !self.disclosed_features.iter().any(|id| id == feature_id) {
            self.disclosed_features.push(feature_id.to_string());
        }
        self.last_disclosure_session = current_session;
    }
}

// ---------------------------------------------------------------------------
// TierConfig -- persisted in agent config
// ---------------------------------------------------------------------------

/// Tier settings persisted in `config.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct TierConfig {
    pub enabled: bool,
    pub user_override: Option<CapabilityTier>,
    pub user_self_assessment: Option<CapabilityTier>,
}

impl Default for TierConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            user_override: None,
            user_self_assessment: None,
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_signals() -> TierSignals {
        TierSignals {
            session_count: 0,
            unique_tools_used: 0,
            goal_runs_completed: 0,
            risk_tolerance: RiskTolerance::Moderate,
            user_self_assessment: None,
            user_override: None,
        }
    }

    #[test]
    fn resolve_tier_no_signals_returns_newcomer() {
        let signals = make_signals();
        assert_eq!(resolve_tier(&signals), CapabilityTier::Newcomer);
    }

    #[test]
    fn resolve_tier_familiar_thresholds() {
        let mut signals = make_signals();
        signals.session_count = 5;
        signals.unique_tools_used = 3;
        assert_eq!(resolve_tier(&signals), CapabilityTier::Familiar);
    }

    #[test]
    fn resolve_tier_power_user_thresholds() {
        let mut signals = make_signals();
        signals.goal_runs_completed = 3;
        signals.unique_tools_used = 5;
        assert_eq!(resolve_tier(&signals), CapabilityTier::PowerUser);
    }

    #[test]
    fn resolve_tier_expert_thresholds() {
        let mut signals = make_signals();
        signals.goal_runs_completed = 10;
        signals.unique_tools_used = 8;
        signals.risk_tolerance = RiskTolerance::Aggressive;
        assert_eq!(resolve_tier(&signals), CapabilityTier::Expert);
    }

    #[test]
    fn user_override_expert_always_returns_expert() {
        let mut signals = make_signals();
        signals.user_override = Some(CapabilityTier::Expert);
        assert_eq!(resolve_tier(&signals), CapabilityTier::Expert);
    }

    #[test]
    fn user_override_newcomer_returns_newcomer_despite_high_signals() {
        let mut signals = make_signals();
        signals.session_count = 100;
        signals.unique_tools_used = 20;
        signals.goal_runs_completed = 50;
        signals.risk_tolerance = RiskTolerance::Aggressive;
        signals.user_override = Some(CapabilityTier::Newcomer);
        assert_eq!(resolve_tier(&signals), CapabilityTier::Newcomer);
    }

    #[test]
    fn self_assessment_elevates_behavioral() {
        let mut signals = make_signals();
        signals.session_count = 5;
        signals.unique_tools_used = 3;
        // behavioral = Familiar
        signals.user_self_assessment = Some(CapabilityTier::PowerUser);
        assert_eq!(resolve_tier(&signals), CapabilityTier::PowerUser);
    }

    #[test]
    fn self_assessment_does_not_demote_behavioral() {
        let mut signals = make_signals();
        signals.session_count = 5;
        signals.unique_tools_used = 3;
        // behavioral = Familiar
        signals.user_self_assessment = Some(CapabilityTier::Newcomer);
        assert_eq!(resolve_tier(&signals), CapabilityTier::Familiar);
    }

    #[test]
    fn tier_features_newcomer_sees_fewest() {
        let flags = tier_features_visible(CapabilityTier::Newcomer);
        assert!(!flags.show_goal_runs);
        assert!(!flags.show_task_queue);
        assert!(!flags.show_gateway_config);
        assert!(!flags.show_subagents);
        assert!(!flags.show_advanced_settings);
        assert!(!flags.show_memory_controls);
    }

    #[test]
    fn tier_features_expert_sees_all() {
        let flags = tier_features_visible(CapabilityTier::Expert);
        assert!(flags.show_goal_runs);
        assert!(flags.show_task_queue);
        assert!(flags.show_gateway_config);
        assert!(flags.show_subagents);
        assert!(flags.show_advanced_settings);
        assert!(flags.show_memory_controls);
    }

    #[test]
    fn disclosure_queue_returns_none_same_session() {
        let queue = DisclosureQueue {
            pending_features: vec![FeatureDisclosure {
                feature_id: "goal_runs".to_string(),
                tier: CapabilityTier::Familiar,
                title: "Goal Runs".to_string(),
                description: "Decompose objectives into steps".to_string(),
            }],
            disclosed_features: vec![],
            last_disclosure_session: 42,
        };
        assert!(queue.next_disclosure(42).is_none());
    }

    #[test]
    fn disclosure_queue_returns_first_pending_different_session() {
        let queue = DisclosureQueue {
            pending_features: vec![FeatureDisclosure {
                feature_id: "goal_runs".to_string(),
                tier: CapabilityTier::Familiar,
                title: "Goal Runs".to_string(),
                description: "Decompose objectives into steps".to_string(),
            }],
            disclosed_features: vec![],
            last_disclosure_session: 41,
        };
        let disclosure = queue.next_disclosure(42);
        assert!(disclosure.is_some());
        assert_eq!(disclosure.unwrap().feature_id, "goal_runs");
    }

    #[test]
    fn capability_tier_ordering() {
        assert!(CapabilityTier::Newcomer < CapabilityTier::Familiar);
        assert!(CapabilityTier::Familiar < CapabilityTier::PowerUser);
        assert!(CapabilityTier::PowerUser < CapabilityTier::Expert);
    }
}
