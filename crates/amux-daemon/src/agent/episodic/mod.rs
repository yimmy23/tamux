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

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_episode() -> Episode {
        Episode {
            id: "ep-001".to_string(),
            goal_run_id: Some("goal-1".to_string()),
            thread_id: Some("thread-1".to_string()),
            session_id: Some("session-1".to_string()),
            goal_text: Some("Completed the deployment task".to_string()),
            goal_type: Some("goal_run".to_string()),
            episode_type: EpisodeType::GoalCompletion,
            summary: "Completed the deployment task".to_string(),
            outcome: EpisodeOutcome::Success,
            root_cause: Some("Config mismatch".to_string()),
            entities: vec!["deploy.yml".to_string(), "staging".to_string()],
            causal_chain: vec![CausalStep {
                step: "step-1".to_string(),
                cause: "wrong config path".to_string(),
                effect: "deployment failed initially".to_string(),
            }],
            solution_class: Some("config-fix".to_string()),
            duration_ms: Some(5000),
            tokens_used: Some(1200),
            confidence: Some(0.95),
            confidence_before: Some(0.7),
            confidence_after: Some(0.95),
            created_at: 1700000000000,
            expires_at: Some(1700000000000 + 90 * 86400 * 1000),
        }
    }

    #[test]
    fn episode_round_trip_serialization() {
        let episode = make_episode();
        let json = serde_json::to_string(&episode).expect("serialize");
        let deserialized: Episode = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(deserialized.id, episode.id);
        assert_eq!(deserialized.goal_run_id, episode.goal_run_id);
        assert_eq!(deserialized.thread_id, episode.thread_id);
        assert_eq!(deserialized.session_id, episode.session_id);
        assert_eq!(deserialized.goal_text, episode.goal_text);
        assert_eq!(deserialized.goal_type, episode.goal_type);
        assert_eq!(deserialized.episode_type, episode.episode_type);
        assert_eq!(deserialized.summary, episode.summary);
        assert_eq!(deserialized.outcome, episode.outcome);
        assert_eq!(deserialized.root_cause, episode.root_cause);
        assert_eq!(deserialized.entities, episode.entities);
        assert_eq!(deserialized.solution_class, episode.solution_class);
        assert_eq!(deserialized.duration_ms, episode.duration_ms);
        assert_eq!(deserialized.tokens_used, episode.tokens_used);
        assert_eq!(deserialized.confidence, episode.confidence);
        assert_eq!(deserialized.confidence_before, episode.confidence_before);
        assert_eq!(deserialized.confidence_after, episode.confidence_after);
        assert_eq!(deserialized.created_at, episode.created_at);
        assert_eq!(deserialized.expires_at, episode.expires_at);
        assert_eq!(deserialized.causal_chain.len(), 1);
        assert_eq!(deserialized.causal_chain[0].step, "step-1");
    }

    #[test]
    fn episode_type_serde_snake_case() {
        assert_eq!(
            serde_json::to_string(&EpisodeType::GoalStart).unwrap(),
            "\"goal_start\""
        );
        assert_eq!(
            serde_json::to_string(&EpisodeType::GoalCompletion).unwrap(),
            "\"goal_completion\""
        );
        assert_eq!(
            serde_json::to_string(&EpisodeType::GoalFailure).unwrap(),
            "\"goal_failure\""
        );
        assert_eq!(
            serde_json::to_string(&EpisodeType::SessionEnd).unwrap(),
            "\"session_end\""
        );
        assert_eq!(
            serde_json::to_string(&EpisodeType::Discovery).unwrap(),
            "\"discovery\""
        );
    }

    #[test]
    fn episode_outcome_serde_snake_case() {
        assert_eq!(
            serde_json::to_string(&EpisodeOutcome::Success).unwrap(),
            "\"success\""
        );
        assert_eq!(
            serde_json::to_string(&EpisodeOutcome::Failure).unwrap(),
            "\"failure\""
        );
        assert_eq!(
            serde_json::to_string(&EpisodeOutcome::Partial).unwrap(),
            "\"partial\""
        );
        assert_eq!(
            serde_json::to_string(&EpisodeOutcome::Abandoned).unwrap(),
            "\"abandoned\""
        );
    }

    #[test]
    fn episodic_config_default_values() {
        let config = EpisodicConfig::default();
        assert!(config.enabled);
        assert_eq!(config.episode_ttl_days, 90);
        assert_eq!(config.constraint_ttl_days, 30);
        assert_eq!(config.max_retrieval_episodes, 5);
        assert_eq!(config.max_injection_tokens, 1500);
        assert!(!config.per_session_suppression);
        assert!(config.suppressed_session_ids.is_empty());
    }

    #[test]
    fn link_type_serde_snake_case() {
        assert_eq!(
            serde_json::to_string(&LinkType::RetryOf).unwrap(),
            "\"retry_of\""
        );
        assert_eq!(
            serde_json::to_string(&LinkType::BuildsOn).unwrap(),
            "\"builds_on\""
        );
        assert_eq!(
            serde_json::to_string(&LinkType::Contradicts).unwrap(),
            "\"contradicts\""
        );
        assert_eq!(
            serde_json::to_string(&LinkType::Supersedes).unwrap(),
            "\"supersedes\""
        );
        assert_eq!(
            serde_json::to_string(&LinkType::CausedBy).unwrap(),
            "\"caused_by\""
        );
    }

    #[test]
    fn negative_constraint_round_trip_serialization() {
        let constraint = NegativeConstraint {
            id: "nc-001".to_string(),
            episode_id: Some("ep-001".to_string()),
            constraint_type: ConstraintType::RuledOut,
            subject: "npm install approach".to_string(),
            solution_class: Some("dependency-resolution".to_string()),
            description: "npm install causes version conflict with react 19".to_string(),
            confidence: 0.9,
            state: ConstraintState::Dead,
            evidence_count: 3,
            direct_observation: false,
            derived_from_constraint_ids: vec!["nc-parent".to_string()],
            related_subject_tokens: vec!["deploy".to_string(), "config".to_string()],
            valid_until: Some(1700000000000),
            created_at: 1699000000000,
        };

        let json = serde_json::to_string(&constraint).expect("serialize");
        let deserialized: NegativeConstraint = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(deserialized.id, constraint.id);
        assert_eq!(deserialized.episode_id, constraint.episode_id);
        assert_eq!(deserialized.constraint_type, ConstraintType::RuledOut);
        assert_eq!(deserialized.subject, constraint.subject);
        assert_eq!(deserialized.solution_class, constraint.solution_class);
        assert_eq!(deserialized.description, constraint.description);
        assert_eq!(deserialized.confidence, constraint.confidence);
        assert_eq!(deserialized.state, constraint.state);
        assert_eq!(deserialized.evidence_count, constraint.evidence_count);
        assert_eq!(
            deserialized.direct_observation,
            constraint.direct_observation
        );
        assert_eq!(
            deserialized.derived_from_constraint_ids,
            constraint.derived_from_constraint_ids
        );
        assert_eq!(
            deserialized.related_subject_tokens,
            constraint.related_subject_tokens
        );
        assert_eq!(deserialized.valid_until, constraint.valid_until);
        assert_eq!(deserialized.created_at, constraint.created_at);
    }

    #[test]
    fn negative_constraint_deserialization_applies_backward_compat_defaults() {
        let json = r#"{
            "id":"nc-legacy",
            "episode_id":"ep-001",
            "constraint_type":"ruled_out",
            "subject":"legacy deploy approach",
            "solution_class":"deployment",
            "description":"legacy payload without new metadata",
            "confidence":0.8,
            "valid_until":1700000000000,
            "created_at":1699000000000
        }"#;

        let deserialized: NegativeConstraint = serde_json::from_str(json).expect("deserialize");

        assert_eq!(deserialized.state, ConstraintState::Dying);
        assert_eq!(deserialized.evidence_count, 1);
        assert!(deserialized.direct_observation);
        assert!(deserialized.derived_from_constraint_ids.is_empty());
        assert!(deserialized.related_subject_tokens.is_empty());
    }

    #[test]
    fn counter_who_state_default_has_empty_fields() {
        let state = CounterWhoState::default();
        assert!(state.goal_run_id.is_none());
        assert!(state.thread_id.is_none());
        assert!(state.current_focus.is_none());
        assert!(state.recent_changes.is_empty());
        assert!(state.tried_approaches.is_empty());
        assert!(state.correction_patterns.is_empty());
        assert!(state.active_constraint_ids.is_empty());
        assert_eq!(state.updated_at, 0);
    }
}
