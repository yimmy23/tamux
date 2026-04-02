//! Uncertainty quantification module (UNCR-01 through UNCR-08).
//!
//! Provides structural confidence scoring for goal plan steps, domain-specific
//! escalation routing, operator-configurable thresholds, and a calibration
//! feedback loop.
//!
//! Confidence derives from 4 structural signals only -- NOT LLM self-assessment:
//! tool_success_rate, episodic_familiarity, blast_radius_score, approach_novelty.

pub mod calibration;
pub mod confidence;
pub mod domains;

use serde::{Deserialize, Serialize};

/// Action to take after evaluating plan confidence (UNCR-08).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlanConfidenceAction {
    /// All steps are HIGH or MEDIUM -- proceed autonomously.
    Proceed,
    /// At least one step is LOW in a blocking domain -- require operator approval.
    RequireApproval,
}

/// Uncertainty quantification configuration.
///
/// Added to AgentConfig with `#[serde(default)]`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UncertaintyConfig {
    /// Whether uncertainty quantification is enabled (default true).
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    /// Per-domain confidence thresholds for blocking/warning.
    #[serde(default)]
    pub domain_thresholds: domains::DomainThresholds,
    /// Minimum observations before trusting raw confidence bands (default 50).
    #[serde(default = "default_calibration_threshold")]
    pub calibration_threshold: usize,
}

fn default_enabled() -> bool {
    true
}

fn default_calibration_threshold() -> usize {
    50
}

impl Default for UncertaintyConfig {
    fn default() -> Self {
        Self {
            enabled: default_enabled(),
            domain_thresholds: domains::DomainThresholds::default(),
            calibration_threshold: default_calibration_threshold(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uncertainty_config_default_has_sensible_values() {
        let config = UncertaintyConfig::default();
        assert!(config.enabled);
        assert_eq!(config.calibration_threshold, 50);
    }
}
