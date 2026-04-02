//! Episodic memory — structured episode records, causal chains, negative knowledge,
//! counter-who state, and episode linking for cross-session learning.

pub mod counter_who;
pub mod links;
pub mod negative_knowledge;
pub mod privacy;
pub mod retrieval;
pub mod schema;
pub mod store;

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Episode types
// ---------------------------------------------------------------------------

/// A structured record of something that happened during agent operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Episode {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub goal_run_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thread_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(default)]
    pub goal_text: Option<String>,
    #[serde(default)]
    pub goal_type: Option<String>,
    pub episode_type: EpisodeType,
    pub summary: String,
    pub outcome: EpisodeOutcome,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub root_cause: Option<String>,
    #[serde(default)]
    pub entities: Vec<String>,
    #[serde(default)]
    pub causal_chain: Vec<CausalStep>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub solution_class: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tokens_used: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f64>,
    #[serde(default)]
    pub confidence_before: Option<f64>,
    #[serde(default)]
    pub confidence_after: Option<f64>,
    pub created_at: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<u64>,
}

impl Episode {
    /// Serialize entities to JSON string for SQLite storage.
    pub fn entities_json(&self) -> String {
        serde_json::to_string(&self.entities).unwrap_or_default()
    }

    /// Serialize causal chain to JSON string for SQLite storage.
    pub fn causal_chain_json(&self) -> String {
        serde_json::to_string(&self.causal_chain).unwrap_or_default()
    }
}

/// Classification of what kind of event an episode represents.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EpisodeType {
    GoalStart,
    GoalCompletion,
    GoalFailure,
    SessionEnd,
    Discovery,
}

/// The outcome of an episode.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EpisodeOutcome {
    Success,
    Failure,
    Partial,
    Abandoned,
}

/// A single step in a causal chain linking failures to root causes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CausalStep {
    pub step: String,
    pub cause: String,
    pub effect: String,
}

// ---------------------------------------------------------------------------
// Episode links
// ---------------------------------------------------------------------------

/// A directed relationship between two episodes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EpisodeLink {
    pub id: String,
    pub source_episode_id: String,
    pub target_episode_id: String,
    pub link_type: LinkType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evidence: Option<String>,
    pub created_at: u64,
}

/// The kind of relationship between two linked episodes.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LinkType {
    RetryOf,
    BuildsOn,
    Contradicts,
    Supersedes,
    CausedBy,
}

// ---------------------------------------------------------------------------
// Negative knowledge
// ---------------------------------------------------------------------------

/// A constraint representing something the agent has ruled out.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NegativeConstraint {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub episode_id: Option<String>,
    pub constraint_type: ConstraintType,
    pub subject: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub solution_class: Option<String>,
    pub description: String,
    pub confidence: f64,
    #[serde(default = "default_constraint_state")]
    pub state: ConstraintState,
    #[serde(default = "default_evidence_count")]
    pub evidence_count: u32,
    #[serde(default = "default_direct_observation")]
    pub direct_observation: bool,
    #[serde(default)]
    pub derived_from_constraint_ids: Vec<String>,
    #[serde(default)]
    pub related_subject_tokens: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub valid_until: Option<u64>,
    pub created_at: u64,
}

/// Lifecycle state of a negative constraint.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ConstraintState {
    Suspicious,
    Dying,
    Dead,
}

/// Classification of negative knowledge constraints.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ConstraintType {
    RuledOut,
    ImpossibleCombination,
    KnownLimitation,
}

fn default_constraint_state() -> ConstraintState {
    ConstraintState::Dying
}

fn default_evidence_count() -> u32 {
    1
}

fn default_direct_observation() -> bool {
    true
}

// ---------------------------------------------------------------------------
// Counter-who state
// ---------------------------------------------------------------------------

/// Persistent internal self-model: "what am I doing, what has changed, what has been tried."
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CounterWhoState {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub goal_run_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thread_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_focus: Option<String>,
    #[serde(default)]
    pub recent_changes: Vec<String>,
    #[serde(default)]
    pub tried_approaches: Vec<TriedApproach>,
    #[serde(default)]
    pub correction_patterns: Vec<CorrectionPattern>,
    #[serde(default)]
    pub active_constraint_ids: Vec<String>,
    #[serde(default)]
    pub updated_at: u64,
}

/// An approach that was tried, with its outcome.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TriedApproach {
    pub approach_hash: String,
    pub description: String,
    pub outcome: EpisodeOutcome,
    pub timestamp: u64,
}

/// A detected pattern of repeated corrections.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CorrectionPattern {
    pub pattern: String,
    pub correction_count: u32,
    pub last_correction_at: u64,
}

// ---------------------------------------------------------------------------
// Episodic store and config
// ---------------------------------------------------------------------------

/// In-memory state for the episodic memory subsystem.
#[derive(Debug, Clone, Default)]
pub struct EpisodicStore {
    pub counter_who: CounterWhoState,
    pub cached_constraints: Vec<NegativeConstraint>,
    pub config: EpisodicConfig,
}

/// Configuration for the episodic memory subsystem.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EpisodicConfig {
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    #[serde(default = "default_episode_ttl_days")]
    pub episode_ttl_days: u64,
    #[serde(default = "default_constraint_ttl_days")]
    pub constraint_ttl_days: u64,
    #[serde(default = "default_max_retrieval_episodes")]
    pub max_retrieval_episodes: usize,
    #[serde(default = "default_max_injection_tokens")]
    pub max_injection_tokens: usize,
    #[serde(default)]
    pub per_session_suppression: bool,
    #[serde(default)]
    pub suppressed_session_ids: Vec<String>,
}

fn default_enabled() -> bool {
    true
}
fn default_episode_ttl_days() -> u64 {
    90
}
fn default_constraint_ttl_days() -> u64 {
    30
}
fn default_max_retrieval_episodes() -> usize {
    5
}
fn default_max_injection_tokens() -> usize {
    1500
}

impl Default for EpisodicConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            episode_ttl_days: 90,
            constraint_ttl_days: 30,
            max_retrieval_episodes: 5,
            max_injection_tokens: 1500,
            per_session_suppression: false,
            suppressed_session_ids: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests;
