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
            // Filter traces that have a task_id (step-level), sort them chronologically,
            // then pick the requested step index so step 0 means the earliest step.
            let mut step_traces: Vec<_> = traces.iter().filter(|t| t.task_id.is_some()).collect();
            step_traces.sort_by_key(|trace| trace.created_at);
            if let Some(trace) = step_traces.get(idx).copied() {
                Some(trace)
            } else if let Some(trace) = step_traces.last().copied() {
                Some(trace)
            } else {
                let mut chronological = traces.iter().collect::<Vec<_>>();
                chronological.sort_by_key(|trace| trace.created_at);
                chronological
                    .get(idx)
                    .copied()
                    .or_else(|| chronological.last().copied())
            }
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
    use crate::agent::AgentConfig;
    use crate::session_manager::SessionManager;
    use tempfile::tempdir;

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

    #[tokio::test]
    async fn explain_action_step_index_uses_chronological_step_order() {
        let root = tempdir().expect("tempdir");
        let manager = SessionManager::new_test(root.path()).await;
        let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

        let earlier_selected = serde_json::json!({
            "option_type": "goal_plan",
            "reasoning": "first chronological step reasoning",
            "rejection_reason": null,
            "estimated_success_prob": 0.61,
            "arguments_hash": "ctx-early"
        })
        .to_string();
        let later_selected = serde_json::json!({
            "option_type": "goal_plan",
            "reasoning": "second chronological step reasoning",
            "rejection_reason": null,
            "estimated_success_prob": 0.73,
            "arguments_hash": "ctx-late"
        })
        .to_string();
        let rejected_json = serde_json::json!([
            {
                "option_type": "goal_plan_alt",
                "reasoning": "fallback alternative",
                "rejection_reason": "lower confidence",
                "estimated_success_prob": 0.42,
                "arguments_hash": "ctx-alt"
            }
        ])
        .to_string();
        let factors_json = serde_json::json!([
            {
                "factor_type": "pattern_match",
                "description": "matched prior plan pattern",
                "weight": 0.7
            }
        ])
        .to_string();
        let unresolved =
            serde_json::to_string(&crate::agent::learning::traces::CausalTraceOutcome::Unresolved)
                .expect("serialize unresolved outcome");

        engine
            .history
            .insert_causal_trace(
                "causal-explain-step-early",
                Some("thread-explain-step-order"),
                Some("goal-explain-step-order"),
                Some("task-step-0"),
                "plan_selection",
                crate::agent::learning::traces::DecisionType::PlanSelection.family_label(),
                &earlier_selected,
                &rejected_json,
                "ctx-early",
                &factors_json,
                &unresolved,
                Some("gpt-4o-mini"),
                1_717_190_001,
            )
            .await
            .expect("insert earlier causal trace");
        engine
            .history
            .insert_causal_trace(
                "causal-explain-step-late",
                Some("thread-explain-step-order"),
                Some("goal-explain-step-order"),
                Some("task-step-1"),
                "plan_selection",
                crate::agent::learning::traces::DecisionType::PlanSelection.family_label(),
                &later_selected,
                &rejected_json,
                "ctx-late",
                &factors_json,
                &unresolved,
                Some("gpt-4o-mini"),
                1_717_190_999,
            )
            .await
            .expect("insert later causal trace");

        let explanation = engine
            .handle_explain_action("goal-explain-step-order", Some(0))
            .await;

        assert_eq!(explanation.source, "causal_trace");
        assert_eq!(
            explanation.chosen_approach, "first chronological step reasoning",
            "step_index=0 should explain the earliest step, not the most recently created one"
        );
        assert_eq!(explanation.alternatives_considered.len(), 1);
        assert_eq!(
            explanation.alternatives_considered[0]
                .rejection_reason
                .as_deref(),
            Some("lower confidence")
        );
    }

    #[tokio::test]
    async fn explain_action_step_index_falls_back_to_earliest_goal_level_trace_when_no_step_traces_exist(
    ) {
        let root = tempdir().expect("tempdir");
        let manager = SessionManager::new_test(root.path()).await;
        let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

        let earlier_selected = serde_json::json!({
            "option_type": "goal_plan",
            "reasoning": "first goal-level reasoning",
            "rejection_reason": null,
            "estimated_success_prob": 0.58,
            "arguments_hash": "ctx-early-goal"
        })
        .to_string();
        let later_selected = serde_json::json!({
            "option_type": "goal_replan",
            "reasoning": "later goal-level reasoning",
            "rejection_reason": null,
            "estimated_success_prob": 0.67,
            "arguments_hash": "ctx-late-goal"
        })
        .to_string();
        let factors_json = serde_json::json!([
            {
                "factor_type": "pattern_match",
                "description": "matched prior goal-level pattern",
                "weight": 0.6
            }
        ])
        .to_string();
        let unresolved =
            serde_json::to_string(&crate::agent::learning::traces::CausalTraceOutcome::Unresolved)
                .expect("serialize unresolved outcome");

        engine
            .history
            .insert_causal_trace(
                "causal-explain-goal-level-early",
                Some("thread-explain-goal-level-order"),
                Some("goal-explain-goal-level-order"),
                None,
                "plan_selection",
                crate::agent::learning::traces::DecisionType::PlanSelection.family_label(),
                &earlier_selected,
                "[]",
                "ctx-early-goal",
                &factors_json,
                &unresolved,
                Some("gpt-4o-mini"),
                1_717_180_001,
            )
            .await
            .expect("insert earlier goal-level causal trace");
        engine
            .history
            .insert_causal_trace(
                "causal-explain-goal-level-late",
                Some("thread-explain-goal-level-order"),
                Some("goal-explain-goal-level-order"),
                None,
                "replan_selection",
                crate::agent::learning::traces::DecisionType::ReplanSelection.family_label(),
                &later_selected,
                "[]",
                "ctx-late-goal",
                &factors_json,
                &unresolved,
                Some("gpt-4o-mini"),
                1_717_180_999,
            )
            .await
            .expect("insert later goal-level causal trace");

        let explanation = engine
            .handle_explain_action("goal-explain-goal-level-order", Some(0))
            .await;

        assert_eq!(explanation.source, "causal_trace");
        assert_eq!(
            explanation.chosen_approach, "first goal-level reasoning",
            "step_index=0 should fall back to the earliest goal-level causal trace when no step traces exist"
        );
        assert_eq!(explanation.decision_point, "plan_selection");
    }
}
