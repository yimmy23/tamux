use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub(crate) struct GenePoolCandidate {
    pub trace_id: String,
    pub proposed_skill_name: String,
    pub task_type: String,
    pub context_tags: Vec<String>,
    pub quality_score: f64,
    pub tool_sequence: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub(crate) struct GenePoolArenaScore {
    pub variant_id: String,
    pub skill_name: String,
    pub variant_name: String,
    pub status: String,
    pub arena_score: f64,
    pub success_rate: f64,
    pub fitness_score: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub(crate) struct GenePoolFitnessSnapshot {
    pub variant_id: String,
    pub recorded_at_ms: u64,
    pub fitness_score: f64,
    pub use_count: u32,
    pub success_rate: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub(crate) struct GenePoolCrossBreedProposal {
    pub left_parent_variant_id: String,
    pub right_parent_variant_id: String,
    pub skill_name: String,
    pub co_usage_rate: f64,
    pub proposed_at_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub(crate) struct GenePoolLifecycleAction {
    pub action: String,
    pub variant_id: Option<String>,
    pub reason: String,
    pub left_parent_variant_id: Option<String>,
    pub right_parent_variant_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub(crate) struct GenePoolRuntimeSnapshot {
    pub generated_at_ms: u64,
    pub candidates: Vec<GenePoolCandidate>,
    pub arena_scores: Vec<GenePoolArenaScore>,
    pub fitness_history: Vec<GenePoolFitnessSnapshot>,
    pub cross_breed_proposals: Vec<GenePoolCrossBreedProposal>,
    pub lifecycle_actions: Vec<GenePoolLifecycleAction>,
    pub summary: String,
}
