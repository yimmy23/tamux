#![allow(dead_code)]

//! Template-based explanation generation and confidence band calculation.
//!
//! Provides plain-language explanations for autonomous agent actions per D-01/D-03.
//! Uses Rust `format!()` templates for simple actions and signals when LLM
//! synthesis is needed for complex multi-factor decisions.

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Confidence bands (per D-09)
// ---------------------------------------------------------------------------

/// Verbal confidence band mapped from a numeric probability.
///
/// Thresholds per D-09:
/// - Confident: >= 0.80
/// - Likely:    >= 0.60
/// - Uncertain: >= 0.40
/// - Guessing:  <  0.40
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConfidenceBand {
    Confident,
    Likely,
    Uncertain,
    Guessing,
}

impl ConfidenceBand {
    /// Returns the verbal label as a static string slice.
    pub fn as_str(&self) -> &str {
        match self {
            Self::Confident => "confident",
            Self::Likely => "likely",
            Self::Uncertain => "uncertain",
            Self::Guessing => "guessing",
        }
    }

    pub fn from_str(value: &str) -> Option<Self> {
        match value.trim().to_lowercase().as_str() {
            "confident" | "high" => Some(Self::Confident),
            "likely" | "medium" => Some(Self::Likely),
            "uncertain" | "low" => Some(Self::Uncertain),
            "guessing" => Some(Self::Guessing),
            _ => None,
        }
    }

    pub fn to_probability(self) -> f64 {
        match self {
            Self::Confident => 0.85,
            Self::Likely => 0.65,
            Self::Uncertain => 0.45,
            Self::Guessing => 0.25,
        }
    }
}

/// Map a numeric probability (0.0..=1.0) to a verbal confidence band.
pub fn confidence_band(probability: f64) -> ConfidenceBand {
    if probability >= 0.80 {
        ConfidenceBand::Confident
    } else if probability >= 0.60 {
        ConfidenceBand::Likely
    } else if probability >= 0.40 {
        ConfidenceBand::Uncertain
    } else {
        ConfidenceBand::Guessing
    }
}

/// Format confidence as user-visible text per D-09/D-10.
///
/// Returns `None` when probability >= threshold (high confidence suppressed
/// by default per D-10). When below threshold, returns a verbal qualifier
/// with the percentage.
pub fn format_confidence_text(probability: f64, threshold: f64) -> Option<String> {
    if probability >= threshold {
        return None;
    }
    let pct = (probability * 100.0).round() as u32;
    let text = match confidence_band(probability) {
        ConfidenceBand::Confident => return None,
        ConfidenceBand::Likely => format!("I'm fairly confident ({}%):", pct),
        ConfidenceBand::Uncertain => format!("I'm uncertain ({}%):", pct),
        ConfidenceBand::Guessing => format!("I'm guessing ({}%):", pct),
    };
    Some(text)
}

// ---------------------------------------------------------------------------
// Explanation generation (per D-03)
// ---------------------------------------------------------------------------

/// Result of attempting to generate an explanation from templates.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExplanationResult {
    /// A complete explanation generated from a Rust template.
    Template(String),
    /// The action is too complex for templates; requires an LLM synthesis call.
    NeedsLlm,
}

/// Generate a plain-language explanation for an autonomous action.
///
/// Template strings come from the UI-SPEC.md copywriting contract. When
/// causal factors exceed the threshold (> 2), returns `NeedsLlm` to signal
/// that a short LLM synthesis call should be used instead.
pub fn generate_explanation(action_type: &str, data: &serde_json::Value) -> ExplanationResult {
    match action_type {
        "stale_todo" => {
            let title = data["title"].as_str().unwrap_or("unknown");
            let hours = data["hours"].as_u64().unwrap_or(0);
            ExplanationResult::Template(format!(
                "Flagged TODO \"{}\" as stale because it hasn't been updated in {}h",
                title, hours
            ))
        }
        "stuck_goal" => {
            let title = data["title"].as_str().unwrap_or("unknown");
            let status = data["status"].as_str().unwrap_or("running");
            let hours = data["hours"].as_u64().unwrap_or(0);
            ExplanationResult::Template(format!(
                "Marked goal run \"{}\" as stuck because it's been {} for {}h with no progress",
                title, status, hours
            ))
        }
        "unreplied_message" => {
            let source = data["source"].as_str().unwrap_or("unknown");
            let hours = data["hours"].as_u64().unwrap_or(0);
            ExplanationResult::Template(format!(
                "Flagged unreplied message from {} ({}h ago)",
                source, hours
            ))
        }
        "repo_change" => {
            let count = data["count"].as_u64().unwrap_or(0);
            let repo = data["repo"].as_str().unwrap_or("unknown");
            ExplanationResult::Template(format!(
                "Detected {} file changes in {} since last check",
                count, repo
            ))
        }
        "tool_execution" => {
            let tool_name = data["tool_name"].as_str().unwrap_or("unknown");
            let session_id = data["session_id"].as_str().unwrap_or("unknown");
            ExplanationResult::Template(format!("Executed {} in session {}", tool_name, session_id))
        }
        "escalation" => {
            let factors = data["causal_factors"]
                .as_array()
                .map(|a| a.len())
                .unwrap_or(0);
            if factors > 2 {
                ExplanationResult::NeedsLlm
            } else {
                let from = data["from_level"].as_str().unwrap_or("L0");
                let to = data["to_level"].as_str().unwrap_or("L1");
                let reason = data["reason"].as_str().unwrap_or("unknown");
                ExplanationResult::Template(format!(
                    "Escalating from {} to {}: {}",
                    from, to, reason
                ))
            }
        }
        "subagent_spawn" => {
            let task_title = data["task_title"].as_str().unwrap_or("unknown");
            ExplanationResult::Template(format!(
                "Spawned sub-agent for \"{}\" because the task exceeds single-agent complexity",
                task_title
            ))
        }
        "skill_selection" => {
            let skill_name = data["skill_name"].as_str().unwrap_or("unknown");
            let confidence = data["confidence"].as_u64().unwrap_or(0);
            let rejected_count = data["rejected_count"].as_u64().unwrap_or(0);
            ExplanationResult::Template(format!(
                "Selected skill \"{}\" (confidence: {}%) over {} alternatives",
                skill_name, confidence, rejected_count
            ))
        }
        // Learning transparency templates (D-10)
        "schedule_learned" => {
            let peak_hours = data["peak_hours"].as_str().unwrap_or("unknown");
            ExplanationResult::Template(format!(
                "I've noticed you're usually active during {} UTC. I'll be more attentive during those hours and quieter outside them.",
                peak_hours
            ))
        }
        "check_deprioritized" => {
            let check_name = data["check_name"].as_str().unwrap_or("unknown");
            let weight = data["weight"].as_f64().unwrap_or(1.0);
            let pct = (weight * 100.0).round() as u32;
            ExplanationResult::Template(format!(
                "I've reduced the frequency of {} checks to {}% -- you haven't needed them recently. I'll increase it again if things change.",
                check_name, pct
            ))
        }
        "check_reprioritized" => {
            let check_name = data["check_name"].as_str().unwrap_or("unknown");
            ExplanationResult::Template(format!(
                "I've increased the frequency of {} checks again -- it seems like they're useful to you now.",
                check_name
            ))
        }
        other => ExplanationResult::Template(format!("Performed {}", other)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // -- Confidence band tests --

    #[test]
    fn confidence_band_confident() {
        assert_eq!(confidence_band(0.85), ConfidenceBand::Confident);
    }

    #[test]
    fn confidence_band_likely() {
        assert_eq!(confidence_band(0.72), ConfidenceBand::Likely);
    }

    #[test]
    fn confidence_band_uncertain() {
        assert_eq!(confidence_band(0.45), ConfidenceBand::Uncertain);
    }

    #[test]
    fn confidence_band_guessing() {
        assert_eq!(confidence_band(0.30), ConfidenceBand::Guessing);
    }

    #[test]
    fn confidence_band_boundary_80() {
        assert_eq!(confidence_band(0.80), ConfidenceBand::Confident);
    }

    #[test]
    fn confidence_band_boundary_60() {
        assert_eq!(confidence_band(0.60), ConfidenceBand::Likely);
    }

    #[test]
    fn confidence_band_boundary_40() {
        assert_eq!(confidence_band(0.40), ConfidenceBand::Uncertain);
    }

    // -- Confidence text formatting tests --

    #[test]
    fn format_confidence_suppressed_above_threshold() {
        assert_eq!(format_confidence_text(0.85, 0.80), None);
    }

    #[test]
    fn format_confidence_likely() {
        let text = format_confidence_text(0.72, 0.80).expect("should produce text");
        assert!(text.contains("fairly confident"), "got: {}", text);
        assert!(text.contains("72%"), "got: {}", text);
    }

    #[test]
    fn format_confidence_uncertain() {
        let text = format_confidence_text(0.45, 0.80).expect("should produce text");
        assert!(text.contains("uncertain"), "got: {}", text);
        assert!(text.contains("45%"), "got: {}", text);
    }

    #[test]
    fn format_confidence_guessing() {
        let text = format_confidence_text(0.30, 0.80).expect("should produce text");
        assert!(text.contains("guessing"), "got: {}", text);
        assert!(text.contains("30%"), "got: {}", text);
    }

    // -- ConfidenceBand::as_str tests --

    #[test]
    fn confidence_band_as_str() {
        assert_eq!(ConfidenceBand::Confident.as_str(), "confident");
        assert_eq!(ConfidenceBand::Likely.as_str(), "likely");
        assert_eq!(ConfidenceBand::Uncertain.as_str(), "uncertain");
        assert_eq!(ConfidenceBand::Guessing.as_str(), "guessing");
    }

    // -- Explanation template tests --

    #[test]
    fn explain_stale_todo() {
        let data = json!({"title": "Fix CI", "hours": 48});
        let result = generate_explanation("stale_todo", &data);
        assert_eq!(
            result,
            ExplanationResult::Template(
                "Flagged TODO \"Fix CI\" as stale because it hasn't been updated in 48h"
                    .to_string()
            )
        );
    }

    #[test]
    fn explain_stuck_goal() {
        let data = json!({"title": "Deploy", "status": "running", "hours": 6});
        let result = generate_explanation("stuck_goal", &data);
        match &result {
            ExplanationResult::Template(s) => {
                assert!(s.contains("stuck"), "got: {}", s);
            }
            _ => panic!("Expected Template, got {:?}", result),
        }
    }

    #[test]
    fn explain_escalation_needs_llm() {
        let data = json!({"causal_factors": [1, 2, 3]});
        let result = generate_explanation("escalation", &data);
        assert_eq!(result, ExplanationResult::NeedsLlm);
    }

    #[test]
    fn explain_escalation_simple() {
        let data = json!({"from_level": "L0", "to_level": "L1", "reason": "timeout", "causal_factors": [1]});
        let result = generate_explanation("escalation", &data);
        match &result {
            ExplanationResult::Template(s) => {
                assert!(s.contains("L0"), "got: {}", s);
                assert!(s.contains("L1"), "got: {}", s);
            }
            _ => panic!("Expected Template, got {:?}", result),
        }
    }

    #[test]
    fn explain_tool_execution() {
        let data = json!({"tool_name": "bash", "session_id": "abc-123"});
        let result = generate_explanation("tool_execution", &data);
        match &result {
            ExplanationResult::Template(s) => {
                assert!(s.contains("Executed"), "got: {}", s);
            }
            _ => panic!("Expected Template, got {:?}", result),
        }
    }

    #[test]
    fn explain_unreplied_message() {
        let data = json!({"source": "slack", "hours": 4});
        let result = generate_explanation("unreplied_message", &data);
        match &result {
            ExplanationResult::Template(s) => {
                assert!(s.contains("unreplied"), "got: {}", s);
            }
            _ => panic!("Expected Template, got {:?}", result),
        }
    }

    #[test]
    fn explain_repo_change() {
        let data = json!({"count": 12, "repo": "tamux"});
        let result = generate_explanation("repo_change", &data);
        match &result {
            ExplanationResult::Template(s) => {
                assert!(s.contains("file changes"), "got: {}", s);
            }
            _ => panic!("Expected Template, got {:?}", result),
        }
    }

    #[test]
    fn explain_subagent_spawn() {
        let data = json!({"task_title": "Refactor parser"});
        let result = generate_explanation("subagent_spawn", &data);
        match &result {
            ExplanationResult::Template(s) => {
                assert!(s.contains("sub-agent"), "got: {}", s);
            }
            _ => panic!("Expected Template, got {:?}", result),
        }
    }

    #[test]
    fn explain_skill_selection() {
        let data = json!({"skill_name": "code_review", "confidence": 85, "rejected_count": 3});
        let result = generate_explanation("skill_selection", &data);
        match &result {
            ExplanationResult::Template(s) => {
                assert!(s.contains("Selected skill"), "got: {}", s);
            }
            _ => panic!("Expected Template, got {:?}", result),
        }
    }

    #[test]
    fn explain_schedule_learned() {
        let data = json!({"peak_hours": "9:00, 10:00, 11:00"});
        let result = generate_explanation("schedule_learned", &data);
        match &result {
            ExplanationResult::Template(s) => {
                assert!(s.contains("9:00, 10:00, 11:00"), "got: {}", s);
                assert!(s.contains("noticed"), "got: {}", s);
            }
            _ => panic!("Expected Template, got {:?}", result),
        }
    }

    #[test]
    fn explain_check_deprioritized() {
        let data = json!({"check_name": "stale todos", "weight": 0.3});
        let result = generate_explanation("check_deprioritized", &data);
        match &result {
            ExplanationResult::Template(s) => {
                assert!(s.contains("stale todos"), "got: {}", s);
                assert!(s.contains("30%"), "got: {}", s);
                assert!(s.contains("reduced"), "got: {}", s);
            }
            _ => panic!("Expected Template, got {:?}", result),
        }
    }

    #[test]
    fn explain_check_reprioritized() {
        let data = json!({"check_name": "stuck goals"});
        let result = generate_explanation("check_reprioritized", &data);
        match &result {
            ExplanationResult::Template(s) => {
                assert!(s.contains("stuck goals"), "got: {}", s);
                assert!(s.contains("increased"), "got: {}", s);
            }
            _ => panic!("Expected Template, got {:?}", result),
        }
    }

    #[test]
    fn explain_unknown_action_fallback() {
        let data = json!({});
        let result = generate_explanation("unknown_action", &data);
        assert_eq!(
            result,
            ExplanationResult::Template("Performed unknown_action".to_string())
        );
    }
}
