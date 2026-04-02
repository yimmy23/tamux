//! On-demand explainability: "Why did you do that?" queries (EXPL-01, EXPL-02, EXPL-03).
//!
//! Provides structured answers for past agent decisions by assembling context from:
//! 1. Causal traces (decision records with selected/rejected options)
//! 2. Episodic memory (past goal outcomes and session summaries)
//! 3. Negative knowledge constraints (ruled-out approaches)
//! 4. Fallback text (always returns something -- never empty per research Pitfall 4)
//!
//! This module is SEPARATE from `explanation.rs` which handles template-based
//! confidence band explanations (Phase 2 / D-03). This module handles the
//! Phase 4 "why did you do that?" query flow (EXPL).

use serde::{Deserialize, Serialize};

use super::engine::AgentEngine;
use crate::agent::learning::traces::{CausalFactor, DecisionOption};

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Structured response to a "Why did you do that?" query (EXPL-01, EXPL-02).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExplanationResponse {
    /// The action ID (goal_run_id) being explained.
    pub action_id: String,
    /// What decision point was reached (e.g., "plan_selection", "tool_selection").
    pub decision_point: String,
    /// What approach was chosen and why.
    pub chosen_approach: String,
    /// Alternatives that were considered but rejected (EXPL-03).
    pub alternatives_considered: Vec<AlternativeConsidered>,
    /// Reasons for the chosen approach.
    pub reasons: Vec<String>,
    /// Source of the explanation: "causal_trace", "episodic", "negative_knowledge", or "fallback".
    pub source: String,
}

/// An alternative approach that was considered but rejected.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlternativeConsidered {
    /// Description of the alternative approach.
    pub description: String,
    /// Why it was rejected (may be None if no reason was recorded).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rejection_reason: Option<String>,
}

// ---------------------------------------------------------------------------
// AgentEngine handler
// ---------------------------------------------------------------------------

impl AgentEngine {
    /// Handle a "Why did you do that?" query for a past action (EXPL-01, EXPL-02).
    ///
    /// Resolution order:
    /// 1. Causal traces for the given goal_run_id (most detailed)
    /// 2. Episodic memory matching the goal_run_id
    /// 3. Negative knowledge constraints related to the goal
    /// 4. Fallback (always returns something -- never empty)
    pub(crate) async fn handle_explain_action(
        &self,
        action_id: &str,
        step_index: Option<usize>,
    ) -> ExplanationResponse {
        // 1. Try causal traces
        if let Some(response) = self.explain_from_causal_traces(action_id, step_index).await {
            return response;
        }

        // 2. Try episodic memory
        if let Some(response) = self.explain_from_episodic(action_id).await {
            return response;
        }

        // 3. Try negative knowledge
        if let Some(response) = self.explain_from_negative_knowledge(action_id).await {
            return response;
        }

        // 4. Fallback -- always return something (research Pitfall 4)
        ExplanationResponse {
            action_id: action_id.to_string(),
            decision_point: "unknown".to_string(),
            chosen_approach: "This action was taken based on the goal plan step instructions. No detailed decision trace was recorded.".to_string(),
            alternatives_considered: Vec::new(),
            reasons: Vec::new(),
            source: "fallback".to_string(),
        }
    }

    /// Try to build an explanation from causal traces stored in the WORM ledger.
    async fn explain_from_causal_traces(
        &self,
        action_id: &str,
        step_index: Option<usize>,
    ) -> Option<ExplanationResponse> {
        let traces = self
            .history
            .list_causal_traces_for_goal_run(action_id, 20)
            .await
            .ok()?;

        if traces.is_empty() {
            return None;
        }

        // If step_index is provided, try to find a trace for that specific step
        let trace = if let Some(idx) = step_index {
            // Filter traces that have a task_id (step-level), pick the one at the step index
            let step_traces: Vec<_> = traces.iter().filter(|t| t.task_id.is_some()).collect();
            step_traces.get(idx).copied().or(traces.first())
        } else {
            traces.first()
        }?;

        // Parse selected option
        let selected: DecisionOption = serde_json::from_str(&trace.selected_json).ok()?;

        // Parse rejected options
        let rejected: Vec<DecisionOption> =
            serde_json::from_str(&trace.rejected_options_json).unwrap_or_default();

        // Parse causal factors
        let factors: Vec<CausalFactor> =
            serde_json::from_str(&trace.causal_factors_json).unwrap_or_default();

        let alternatives = rejected
            .iter()
            .map(|opt| AlternativeConsidered {
                description: opt.reasoning.clone(),
                rejection_reason: opt.rejection_reason.clone(),
            })
            .collect();

        let reasons = factors.iter().map(|f| f.description.clone()).collect();

        Some(ExplanationResponse {
            action_id: action_id.to_string(),
            decision_point: trace.decision_type.clone(),
            chosen_approach: selected.reasoning,
            alternatives_considered: alternatives,
            reasons,
            source: "causal_trace".to_string(),
        })
    }

    /// Try to build an explanation from episodic memory.
    async fn explain_from_episodic(&self, action_id: &str) -> Option<ExplanationResponse> {
        // Query episodes for this goal_run_id
        let episodes = self.list_episodes_for_goal_run(action_id).await.ok()?;

        if episodes.is_empty() {
            return None;
        }

        let episode = &episodes[0];
        let reasons: Vec<String> = episode
            .causal_chain
            .iter()
            .map(|step| format!("{}: {} -> {}", step.step, step.cause, step.effect))
            .collect();

        let outcome_str = match episode.outcome {
            super::episodic::EpisodeOutcome::Success => "succeeded",
            super::episodic::EpisodeOutcome::Failure => "failed",
            super::episodic::EpisodeOutcome::Partial => "partially completed",
            super::episodic::EpisodeOutcome::Abandoned => "was abandoned",
        };

        Some(ExplanationResponse {
            action_id: action_id.to_string(),
            decision_point: format!("{:?}", episode.episode_type),
            chosen_approach: format!("Goal run {} -- {}", outcome_str, episode.summary),
            alternatives_considered: Vec::new(),
            reasons,
            source: "episodic".to_string(),
        })
    }

    /// Try to build an explanation from negative knowledge constraints.
    async fn explain_from_negative_knowledge(
        &self,
        action_id: &str,
    ) -> Option<ExplanationResponse> {
        let constraints = self.query_active_constraints(Some(action_id)).await.ok()?;

        if constraints.is_empty() {
            return None;
        }

        let reasons: Vec<String> = constraints
            .iter()
            .map(|c| format!("Avoided '{}': {}", c.subject, c.description))
            .collect();

        Some(ExplanationResponse {
            action_id: action_id.to_string(),
            decision_point: "negative_knowledge".to_string(),
            chosen_approach: format!(
                "Action was influenced by {} known constraint(s) from past experience.",
                constraints.len()
            ),
            alternatives_considered: Vec::new(),
            reasons,
            source: "negative_knowledge".to_string(),
        })
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explanation_response_round_trips_serde() {
        let response = ExplanationResponse {
            action_id: "goal_abc123".to_string(),
            decision_point: "plan_selection".to_string(),
            chosen_approach: "Used a 3-step plan with command steps".to_string(),
            alternatives_considered: vec![
                AlternativeConsidered {
                    description: "Single-step bash approach".to_string(),
                    rejection_reason: Some("Too risky for production".to_string()),
                },
                AlternativeConsidered {
                    description: "Manual research-first approach".to_string(),
                    rejection_reason: None,
                },
            ],
            reasons: vec![
                "Past success with multi-step plans".to_string(),
                "Operator prefers conservative approaches".to_string(),
            ],
            source: "causal_trace".to_string(),
        };

        let json = serde_json::to_string(&response).expect("serialize");
        let decoded: ExplanationResponse = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(decoded.action_id, "goal_abc123");
        assert_eq!(decoded.decision_point, "plan_selection");
        assert_eq!(
            decoded.chosen_approach,
            "Used a 3-step plan with command steps"
        );
        assert_eq!(decoded.alternatives_considered.len(), 2);
        assert_eq!(
            decoded.alternatives_considered[0].description,
            "Single-step bash approach"
        );
        assert_eq!(
            decoded.alternatives_considered[0]
                .rejection_reason
                .as_deref(),
            Some("Too risky for production")
        );
        assert!(decoded.alternatives_considered[1]
            .rejection_reason
            .is_none());
        assert_eq!(decoded.reasons.len(), 2);
        assert_eq!(decoded.source, "causal_trace");
    }

    #[test]
    fn explanation_response_with_empty_alternatives() {
        let response = ExplanationResponse {
            action_id: "run_xyz".to_string(),
            decision_point: "tool_selection".to_string(),
            chosen_approach: "Used bash_command".to_string(),
            alternatives_considered: vec![],
            reasons: vec!["Only viable tool".to_string()],
            source: "episodic".to_string(),
        };

        let json = serde_json::to_string(&response).expect("serialize");
        let decoded: ExplanationResponse = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(decoded.source, "episodic");
        assert!(decoded.alternatives_considered.is_empty());
    }

    #[test]
    fn explanation_response_fallback_source() {
        let response = ExplanationResponse {
            action_id: "unknown_action".to_string(),
            decision_point: "unknown".to_string(),
            chosen_approach: "This action was taken based on the goal plan step instructions. No detailed decision trace was recorded.".to_string(),
            alternatives_considered: vec![],
            reasons: vec![],
            source: "fallback".to_string(),
        };

        let json = serde_json::to_string(&response).expect("serialize");
        let decoded: ExplanationResponse = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(decoded.source, "fallback");
        assert!(
            !decoded.chosen_approach.is_empty(),
            "fallback must not be empty"
        );
    }

    #[test]
    fn explanation_response_negative_knowledge_source() {
        let response = ExplanationResponse {
            action_id: "goal_fail".to_string(),
            decision_point: "planning".to_string(),
            chosen_approach: "Avoided npm install approach".to_string(),
            alternatives_considered: vec![],
            reasons: vec!["npm install previously failed with permission error".to_string()],
            source: "negative_knowledge".to_string(),
        };

        let json = serde_json::to_string(&response).expect("serialize");
        let decoded: ExplanationResponse = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(decoded.source, "negative_knowledge");
        assert_eq!(decoded.reasons.len(), 1);
    }
}
