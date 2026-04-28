use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub(crate) struct ConsensusBidPrior {
    pub role: String,
    pub success_count: u64,
    pub failure_count: u64,
    pub prior_score: f64,
    pub last_updated_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub(crate) struct PersistedConsensusBid {
    pub task_id: String,
    pub round_id: u64,
    pub agent_id: String,
    pub confidence: f64,
    pub reasoning: String,
    pub availability: String,
    pub domain_affinity: f64,
    pub submitted_at_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub(crate) struct PersistedRoleAssignment {
    pub task_id: String,
    pub round_id: u64,
    pub primary_agent_id: String,
    pub reviewer_agent_id: Option<String>,
    pub observers: Vec<String>,
    pub assigned_at_ms: u64,
    pub outcome: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub(crate) struct ConsensusQualityMetric {
    pub task_id: String,
    pub predicted_confidence: f64,
    pub actual_outcome_score: f64,
    pub prediction_error: f64,
    pub updated_at_ms: u64,
}
