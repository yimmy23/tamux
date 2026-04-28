//! Core confidence computation from structural signals (UNCR-01, UNCR-06).
//!
//! Confidence derives from 4 structural signals:
//! - tool_success_rate: from awareness OutcomeWindow (0.0-1.0)
//! - episodic_familiarity: from embodied compute_familiarity (0.0-1.0)
//! - blast_radius_score: from causal_traces advisory (0.0-1.0)
//! - approach_novelty: from counter_who tried_approaches (0.0-1.0)
//!
//! Weighted formula (UNCR-06):
//!   score = 0.30 * tool_success + 0.25 * familiarity
//!         + 0.25 * (1 - blast_radius) + 0.20 * (1 - novelty)

use crate::agent::explanation::{confidence_band, ConfidenceBand};

use super::domains::{DomainClassification, DomainThresholds};

/// Input signals for confidence computation (all structural, no LLM self-assessment).
#[derive(Debug, Clone)]
pub struct ConfidenceSignals {
    /// Tool success rate from awareness window (0.0-1.0).
    pub tool_success_rate: f64,
    /// Episodic familiarity from embodied compute_familiarity (0.0-1.0).
    pub episodic_familiarity: f64,
    /// Blast radius score from causal traces advisory (0.0-1.0).
    pub blast_radius_score: f64,
    /// Approach novelty from counter_who tried_approaches (0.0-1.0).
    pub approach_novelty: f64,
    /// Optional verbal self-assessment from the planning LLM (UNCR-06).
    pub llm_self_assessment: Option<ConfidenceBand>,
}

/// Result of confidence assessment for a plan step.
#[derive(Debug, Clone)]
pub struct ConfidenceAssessment {
    /// The computed confidence band.
    pub band: ConfidenceBand,
    /// User-facing label: "HIGH", "MEDIUM", or "LOW".
    pub label: &'static str,
    /// Evidence strings explaining why confidence is not HIGH.
    pub evidence: Vec<String>,
    /// Domain classification for escalation routing.
    pub domain: DomainClassification,
    /// Whether this action should be blocked based on domain thresholds.
    pub should_block: bool,
}

/// Map ConfidenceBand to user-facing label (UNCR-01).
///
/// Per locked decision: labels are HIGH/MEDIUM/LOW, not the 4-tier band names.
/// Confident -> HIGH, Likely -> MEDIUM, Uncertain|Guessing -> LOW.
pub fn confidence_label(band: ConfidenceBand) -> &'static str {
    match band {
        ConfidenceBand::Confident => "HIGH",
        ConfidenceBand::Likely => "MEDIUM",
        ConfidenceBand::Uncertain | ConfidenceBand::Guessing => "LOW",
    }
}

/// Convert blast radius advisory risk_level string to 0.0-1.0 score.
pub fn blast_radius_to_score(risk_level: &str) -> f64 {
    match risk_level {
        "low" => 0.2,
        "medium" => 0.5,
        "high" => 0.9,
        _ => 0.5,
    }
}

/// Compute approach novelty (0.0-1.0) from counter_who tried_approaches.
///
/// 0 prior attempts = 1.0 (completely novel), 5+ = 0.0 (proven pattern).
pub fn approach_novelty_score(matching_prior_attempts: usize) -> f64 {
    let capped = matching_prior_attempts.min(5) as f64;
    1.0 - (capped / 5.0)
}

/// Compute confidence from structural signals only (UNCR-06).
///
/// Weights: 0.30 tool_success + 0.25 familiarity + 0.25 (1-blast) + 0.20 (1-novelty).
pub fn compute_step_confidence(
    signals: &ConfidenceSignals,
    domain: DomainClassification,
    thresholds: &DomainThresholds,
) -> ConfidenceAssessment {
    let structural_score = 0.30 * signals.tool_success_rate
        + 0.25 * signals.episodic_familiarity
        + 0.25 * (1.0 - signals.blast_radius_score)
        + 0.20 * (1.0 - signals.approach_novelty);
    let score = match signals.llm_self_assessment {
        Some(band) => 0.6 * structural_score + 0.4 * band.to_probability(),
        None => structural_score,
    };

    let band = confidence_band(score);
    let label = confidence_label(band);

    let mut evidence = Vec::new();
    if signals.tool_success_rate < 0.5 {
        evidence.push(format!(
            "Low tool success rate: {:.0}%",
            signals.tool_success_rate * 100.0
        ));
    }
    if signals.episodic_familiarity < 0.3 {
        evidence.push("Unfamiliar pattern (few similar past episodes)".to_string());
    }
    if signals.blast_radius_score > 0.7 {
        evidence.push("High blast radius: action could have wide impact".to_string());
    }
    if signals.approach_novelty > 0.7 {
        evidence.push("Novel approach (not previously attempted)".to_string());
    }
    if let Some(band) = signals.llm_self_assessment {
        evidence.push(format!("LLM self-assessment: {}", band.as_str()));
    }

    let should_block = thresholds.should_block(domain, band);

    ConfidenceAssessment {
        band,
        label,
        evidence,
        domain,
        should_block,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compute_step_confidence_all_high_signals_returns_high() {
        let signals = ConfidenceSignals {
            tool_success_rate: 1.0,
            episodic_familiarity: 1.0,
            blast_radius_score: 0.0,
            approach_novelty: 0.0,
            llm_self_assessment: None,
        };
        let thresholds = DomainThresholds::default();
        let result = compute_step_confidence(&signals, DomainClassification::Business, &thresholds);
        // score = 0.30*1.0 + 0.25*1.0 + 0.25*(1.0-0.0) + 0.20*(1.0-0.0) = 0.30+0.25+0.25+0.20 = 1.0
        assert_eq!(result.label, "HIGH");
        assert_eq!(result.band, ConfidenceBand::Confident);
    }

    #[test]
    fn compute_step_confidence_all_low_signals_returns_low() {
        let signals = ConfidenceSignals {
            tool_success_rate: 0.0,
            episodic_familiarity: 0.0,
            blast_radius_score: 1.0,
            approach_novelty: 1.0,
            llm_self_assessment: None,
        };
        let thresholds = DomainThresholds::default();
        let result = compute_step_confidence(&signals, DomainClassification::Business, &thresholds);
        // score = 0.30*0.0 + 0.25*0.0 + 0.25*(1.0-1.0) + 0.20*(1.0-1.0) = 0.0
        assert_eq!(result.label, "LOW");
        assert!(
            result.band == ConfidenceBand::Guessing || result.band == ConfidenceBand::Uncertain
        );
    }

    #[test]
    fn compute_step_confidence_mixed_signals_returns_medium() {
        let signals = ConfidenceSignals {
            tool_success_rate: 0.8,
            episodic_familiarity: 0.6,
            blast_radius_score: 0.3,
            approach_novelty: 0.4,
            llm_self_assessment: None,
        };
        let thresholds = DomainThresholds::default();
        let result = compute_step_confidence(&signals, DomainClassification::Business, &thresholds);
        // score = 0.30*0.8 + 0.25*0.6 + 0.25*0.7 + 0.20*0.6 = 0.24+0.15+0.175+0.12 = 0.685
        // 0.685 >= 0.60 -> Likely -> MEDIUM
        assert_eq!(result.label, "MEDIUM");
        assert_eq!(result.band, ConfidenceBand::Likely);
    }

    #[test]
    fn confidence_label_confident_is_high() {
        assert_eq!(confidence_label(ConfidenceBand::Confident), "HIGH");
    }

    #[test]
    fn confidence_label_likely_is_medium() {
        assert_eq!(confidence_label(ConfidenceBand::Likely), "MEDIUM");
    }

    #[test]
    fn confidence_label_uncertain_is_low() {
        assert_eq!(confidence_label(ConfidenceBand::Uncertain), "LOW");
    }

    #[test]
    fn confidence_label_guessing_is_low() {
        assert_eq!(confidence_label(ConfidenceBand::Guessing), "LOW");
    }

    #[test]
    fn blast_radius_to_score_low() {
        let score = blast_radius_to_score("low");
        assert!((score - 0.2).abs() < 0.01);
    }

    #[test]
    fn blast_radius_to_score_medium() {
        let score = blast_radius_to_score("medium");
        assert!((score - 0.5).abs() < 0.01);
    }

    #[test]
    fn blast_radius_to_score_high() {
        let score = blast_radius_to_score("high");
        assert!((score - 0.9).abs() < 0.01);
    }

    #[test]
    fn approach_novelty_score_no_prior_attempts() {
        assert_eq!(approach_novelty_score(0), 1.0);
    }

    #[test]
    fn approach_novelty_score_many_prior_attempts() {
        assert_eq!(approach_novelty_score(5), 0.0);
        assert_eq!(approach_novelty_score(10), 0.0); // capped at 5
    }

    #[test]
    fn approach_novelty_score_intermediate() {
        let score = approach_novelty_score(2);
        assert!((score - 0.6).abs() < 0.01);
    }

    #[test]
    fn llm_self_assessment_can_raise_borderline_structural_confidence() {
        let signals = ConfidenceSignals {
            tool_success_rate: 0.45,
            episodic_familiarity: 0.45,
            blast_radius_score: 0.35,
            approach_novelty: 0.55,
            llm_self_assessment: Some(ConfidenceBand::Confident),
        };
        let thresholds = DomainThresholds::default();
        let result = compute_step_confidence(&signals, DomainClassification::Business, &thresholds);
        assert_eq!(
            result.label, "MEDIUM",
            "hybrid confidence should blend structural score with LLM self-assessment rather than ignoring it"
        );
    }
}
