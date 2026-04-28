use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub(crate) struct MorphogenesisAffinity {
    pub agent_id: String,
    pub domain: String,
    pub affinity_score: f64,
    pub task_count: u64,
    pub success_count: u64,
    pub failure_count: u64,
    pub last_updated_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub(crate) struct AffinityUpdate {
    pub agent_id: String,
    pub domain: String,
    pub old_affinity: f64,
    pub new_affinity: f64,
    pub trigger_type: String,
    pub task_id: Option<String>,
    pub updated_at_ms: u64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum MorphogenesisOutcome {
    Success,
    Partial,
    Failure,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum AdaptationType {
    Added,
    Removed,
    Updated,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub(crate) struct SoulAdaptation {
    pub agent_id: String,
    pub domain: String,
    pub adaptation_type: AdaptationType,
    pub soul_snippet: String,
    pub old_soul_hash: Option<String>,
    pub new_soul_hash: Option<String>,
    pub created_at_ms: u64,
}
