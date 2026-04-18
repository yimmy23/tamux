use anyhow::Result;
use rusqlite::params;

use super::types::{ConsensusQualityMetric, PersistedRoleAssignment};
use crate::agent::collaboration::CollaborationSession;
use crate::agent::engine::AgentEngine;

pub(crate) fn outcome_score(outcome: &str) -> f64 {
    match outcome {
        "success" | "completed" | "accepted" => 1.0,
        "cancelled" => 0.25,
        _ => 0.0,
    }
}

pub(crate) fn build_quality_metric(
    session: &CollaborationSession,
    assignment: &PersistedRoleAssignment,
    outcome: &str,
    updated_at_ms: u64,
) -> ConsensusQualityMetric {
    let predicted_confidence = session
        .bids
        .iter()
        .find(|bid| bid.task_id == assignment.primary_agent_id)
        .map(|bid| bid.confidence)
        .unwrap_or(0.0);
    let actual_outcome_score = outcome_score(outcome);
    ConsensusQualityMetric {
        task_id: assignment.task_id.clone(),
        predicted_confidence,
        actual_outcome_score,
        prediction_error: (predicted_confidence - actual_outcome_score).abs(),
        updated_at_ms,
    }
}

impl AgentEngine {
    pub(crate) async fn persist_role_assignment(
        &self,
        assignment: PersistedRoleAssignment,
    ) -> Result<()> {
        let observers_json = serde_json::to_string(&assignment.observers)?;
        self.history
            .conn
            .call(move |conn| {
                conn.execute(
                    "INSERT INTO role_assignments (
                        task_id, round_id, primary_agent_id, reviewer_agent_id, observers, assigned_at_ms, outcome
                     ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                    params![
                        assignment.task_id,
                        assignment.round_id as i64,
                        assignment.primary_agent_id,
                        assignment.reviewer_agent_id,
                        observers_json,
                        assignment.assigned_at_ms as i64,
                        assignment.outcome,
                    ],
                )?;
                Ok(())
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub(crate) async fn persist_consensus_quality_metric(
        &self,
        metric: ConsensusQualityMetric,
    ) -> Result<()> {
        self.history
            .conn
            .call(move |conn| {
                conn.execute(
                    "INSERT INTO consensus_quality_metrics (
                        task_id, predicted_confidence, actual_outcome_score, prediction_error, updated_at_ms
                     ) VALUES (?1, ?2, ?3, ?4, ?5)",
                    params![
                        metric.task_id,
                        metric.predicted_confidence,
                        metric.actual_outcome_score,
                        metric.prediction_error,
                        metric.updated_at_ms as i64,
                    ],
                )?;
                Ok(())
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub(crate) async fn mark_role_assignment_outcome(
        &self,
        task_id: &str,
        outcome: &str,
    ) -> Result<()> {
        let task_id = task_id.to_string();
        let outcome = outcome.to_string();
        self.history
            .conn
            .call(move |conn| {
                conn.execute(
                    "UPDATE role_assignments SET outcome = ?2 WHERE id = (
                        SELECT id FROM role_assignments WHERE task_id = ?1 ORDER BY assigned_at_ms DESC, id DESC LIMIT 1
                     )",
                    params![task_id, outcome],
                )?;
                Ok(())
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }
}
