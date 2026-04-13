//! Domain classification and per-domain confidence thresholds (UNCR-04, UNCR-05).
//!
//! Tools are classified into four domains that determine escalation behavior:
//! - Safety: destructive tools, system commands -- blocks on LOW
//! - Reliability: deployment, config changes -- blocks on Guessing only
//! - Business: standard file operations, code changes -- warns on LOW
//! - Research: web search, file reading, exploration -- surfaces all levels

use serde::{Deserialize, Serialize};

use crate::agent::explanation::ConfidenceBand;
use crate::agent::types::GoalRunStepKind;

/// Domain classification for tools and actions (UNCR-04).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DomainClassification {
    /// Destructive tools, system commands.
    Safety,
    /// Deployment, config changes.
    Reliability,
    /// Standard file operations, code changes.
    Business,
    /// Web search, file reading, exploration.
    Research,
}

/// Classify a tool/action name into a domain (UNCR-04).
pub fn classify_domain(tool_name: &str) -> DomainClassification {
    match tool_name {
        "execute_command"
        | "execute_managed_command"
        | "delete_file"
        | "kill_session"
        | "restart_session" => DomainClassification::Safety,
        "deploy" | "write_config" | "install_package" => DomainClassification::Reliability,
        "web_search" | "web_read" | "symbol_search" | "list_directory" | "list_files"
        | "search_files" => DomainClassification::Research,
        _ => DomainClassification::Business,
    }
}

/// Classify a GoalRunStepKind into a domain for confidence scoring.
///
/// Used when annotating plan steps (where we have the step kind, not a tool name).
pub fn classify_step_kind(kind: &GoalRunStepKind) -> DomainClassification {
    match kind {
        GoalRunStepKind::Command => DomainClassification::Safety,
        GoalRunStepKind::Research => DomainClassification::Research,
        GoalRunStepKind::Memory => DomainClassification::Business,
        GoalRunStepKind::Skill => DomainClassification::Business,
        GoalRunStepKind::Reason => DomainClassification::Research,
        GoalRunStepKind::Specialist(_) => DomainClassification::Business,
        GoalRunStepKind::Divergent => DomainClassification::Research,
        GoalRunStepKind::Debate => DomainClassification::Research,
        GoalRunStepKind::Unknown => DomainClassification::Business,
    }
}

/// Per-domain confidence thresholds controlling when actions block (UNCR-05).
///
/// An action blocks when its confidence band is strictly below the threshold
/// for that domain.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DomainThresholds {
    /// Safety blocks below Likely (i.e., blocks Uncertain and Guessing = LOW).
    #[serde(default = "default_safety_block")]
    pub safety_block_below: ConfidenceBand,
    /// Reliability blocks below Uncertain (i.e., blocks Guessing only).
    #[serde(default = "default_reliability_block")]
    pub reliability_block_below: ConfidenceBand,
    /// Business blocks below Guessing (effectively warns on LOW, never blocks).
    #[serde(default = "default_business_block")]
    pub business_block_below: ConfidenceBand,
    /// Research never blocks.
    #[serde(default = "default_research_block")]
    pub research_block_below: ConfidenceBand,
}

fn default_safety_block() -> ConfidenceBand {
    ConfidenceBand::Likely
}
fn default_reliability_block() -> ConfidenceBand {
    ConfidenceBand::Uncertain
}
fn default_business_block() -> ConfidenceBand {
    ConfidenceBand::Guessing
}
fn default_research_block() -> ConfidenceBand {
    // Never blocks -- set to an impossible-to-reach threshold
    ConfidenceBand::Guessing
}

impl Default for DomainThresholds {
    fn default() -> Self {
        Self {
            safety_block_below: default_safety_block(),
            reliability_block_below: default_reliability_block(),
            business_block_below: default_business_block(),
            research_block_below: default_research_block(),
        }
    }
}

/// Convert a ConfidenceBand to a numeric ordering for comparison.
/// Higher = more confident.
fn band_ord(band: ConfidenceBand) -> u8 {
    match band {
        ConfidenceBand::Confident => 3,
        ConfidenceBand::Likely => 2,
        ConfidenceBand::Uncertain => 1,
        ConfidenceBand::Guessing => 0,
    }
}

impl DomainThresholds {
    /// Check whether an action should be blocked given its domain and confidence band.
    ///
    /// Returns true when the band is strictly below the domain's threshold.
    /// For Research with default thresholds, this only blocks if band < Guessing,
    /// which is impossible -- so Research never blocks.
    pub fn should_block(&self, domain: DomainClassification, band: ConfidenceBand) -> bool {
        let threshold = match domain {
            DomainClassification::Safety => self.safety_block_below,
            DomainClassification::Reliability => self.reliability_block_below,
            DomainClassification::Business => self.business_block_below,
            DomainClassification::Research => self.research_block_below,
        };
        // Block if band is strictly below threshold.
        // For Safety (threshold=Likely): blocks Uncertain and Guessing
        // For Research (threshold=Guessing): blocks nothing (nothing is below Guessing)
        band_ord(band) < band_ord(threshold)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_domain_execute_command_is_safety() {
        assert_eq!(
            classify_domain("execute_command"),
            DomainClassification::Safety
        );
    }

    #[test]
    fn classify_domain_web_search_is_research() {
        assert_eq!(
            classify_domain("web_search"),
            DomainClassification::Research
        );
    }

    #[test]
    fn classify_domain_read_file_is_business() {
        assert_eq!(classify_domain("read_file"), DomainClassification::Business);
    }

    #[test]
    fn classify_domain_unknown_tool_is_business() {
        assert_eq!(
            classify_domain("unknown_tool"),
            DomainClassification::Business
        );
    }

    #[test]
    fn default_thresholds_safety_blocks_low() {
        let t = DomainThresholds::default();
        // Safety blocks below Likely -> Uncertain (LOW) is blocked
        assert!(t.should_block(DomainClassification::Safety, ConfidenceBand::Uncertain));
        assert!(t.should_block(DomainClassification::Safety, ConfidenceBand::Guessing));
    }

    #[test]
    fn default_thresholds_business_warns_low() {
        let t = DomainThresholds::default();
        // Business blocks below Guessing -> nothing is below Guessing -> never blocks
        assert!(!t.should_block(DomainClassification::Business, ConfidenceBand::Guessing));
        assert!(!t.should_block(DomainClassification::Business, ConfidenceBand::Uncertain));
    }

    #[test]
    fn default_thresholds_research_surfaces_all() {
        let t = DomainThresholds::default();
        // Research never blocks
        assert!(!t.should_block(DomainClassification::Research, ConfidenceBand::Guessing));
        assert!(!t.should_block(DomainClassification::Research, ConfidenceBand::Uncertain));
        assert!(!t.should_block(DomainClassification::Research, ConfidenceBand::Likely));
    }

    #[test]
    fn should_block_true_for_safety_with_low_band() {
        let t = DomainThresholds::default();
        // LOW maps to Uncertain or Guessing. Both are below Likely threshold.
        assert!(t.should_block(DomainClassification::Safety, ConfidenceBand::Guessing));
    }

    #[test]
    fn should_block_false_for_research_with_low_band() {
        let t = DomainThresholds::default();
        assert!(!t.should_block(DomainClassification::Research, ConfidenceBand::Guessing));
    }

    #[test]
    fn should_block_false_for_safety_with_high_band() {
        let t = DomainThresholds::default();
        assert!(!t.should_block(DomainClassification::Safety, ConfidenceBand::Confident));
    }
}
