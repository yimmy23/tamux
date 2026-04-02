//! Handoff broker — specialist profile matching, context bundling, escalation,
//! and audit for multi-agent task delegation.

pub mod acceptance;
pub mod audit;
pub mod broker;
pub mod context_bundle;
pub mod divergent;
pub mod escalation;
pub mod profiles;
pub mod schema;

pub use acceptance::ValidationResult;

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Proficiency levels
// ---------------------------------------------------------------------------

/// How proficient a specialist is at a given capability.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Proficiency {
    Expert,
    Advanced,
    Competent,
    Familiar,
}

impl Proficiency {
    /// Numeric weight used for capability scoring.
    pub fn weight(&self) -> f64 {
        match self {
            Proficiency::Expert => 1.0,
            Proficiency::Advanced => 0.75,
            Proficiency::Competent => 0.5,
            Proficiency::Familiar => 0.25,
        }
    }
}

// ---------------------------------------------------------------------------
// Capability tags
// ---------------------------------------------------------------------------

/// A capability tag with its associated proficiency level.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityTag {
    pub tag: String,
    pub proficiency: Proficiency,
}

// ---------------------------------------------------------------------------
// Escalation types
// ---------------------------------------------------------------------------

/// Trigger condition for an escalation rule.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HandoffEscalationTrigger {
    /// Confidence drops below the named band (e.g. "uncertain").
    ConfidenceBelow(String),
    /// Tool failures exceed the given count.
    ToolFails(u32),
    /// Elapsed time exceeds the given seconds.
    TimeExceeds(u64),
}

/// Action to take when an escalation trigger fires.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HandoffEscalationAction {
    /// Return the task to the caller.
    HandBack,
    /// Retry with enriched context.
    RetryWithNewContext,
    /// Forward to a named specialist.
    EscalateTo(String),
    /// Abort and produce a report.
    AbortWithReport,
}

/// A single escalation rule pairing a trigger with an action.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HandoffEscalationRule {
    pub trigger: HandoffEscalationTrigger,
    pub action: HandoffEscalationAction,
}

// ---------------------------------------------------------------------------
// Specialist profile
// ---------------------------------------------------------------------------

/// A specialist agent profile with capability tags, tool restrictions,
/// and escalation rules.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpecialistProfile {
    pub id: String,
    pub name: String,
    pub role: String,
    pub capabilities: Vec<CapabilityTag>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_filter: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub system_prompt_snippet: Option<String>,
    #[serde(default)]
    pub escalation_chain: Vec<HandoffEscalationRule>,
    #[serde(default)]
    pub is_builtin: bool,
}

// ---------------------------------------------------------------------------
// Context bundle (stub for Plan 02)
// ---------------------------------------------------------------------------

/// Reference to an episodic memory episode.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EpisodeRef {
    pub episode_id: String,
    pub summary: String,
    pub outcome: String,
}

/// A partial output from an earlier step.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartialOutput {
    pub step_index: usize,
    pub content: String,
    pub status: String,
}

/// Context passed to a specialist during handoff.
/// Fields only -- implementation deferred to Plan 02.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextBundle {
    pub task_spec: String,
    pub acceptance_criteria: String,
    #[serde(default)]
    pub episodic_refs: Vec<EpisodeRef>,
    #[serde(default)]
    pub negative_constraints: Vec<String>,
    #[serde(default)]
    pub partial_outputs: Vec<PartialOutput>,
    #[serde(default)]
    pub parent_context_summary: String,
    #[serde(default)]
    pub handoff_depth: u8,
    #[serde(default)]
    pub estimated_tokens: u32,
}

// ---------------------------------------------------------------------------
// Acceptance criteria (stub for Plan 02)
// ---------------------------------------------------------------------------

/// Criteria used to validate specialist output.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AcceptanceCriteria {
    pub description: String,
    #[serde(default)]
    pub structural_checks: Vec<String>,
    #[serde(default)]
    pub require_llm_validation: bool,
}

// ---------------------------------------------------------------------------
// Handoff result
// ---------------------------------------------------------------------------

/// Result of a successful handoff dispatch.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HandoffResult {
    pub task_id: String,
    pub specialist_profile_id: String,
    pub specialist_name: String,
    pub handoff_log_id: String,
    pub context_bundle_tokens: u32,
}

// ---------------------------------------------------------------------------
// HandoffBroker
// ---------------------------------------------------------------------------

/// Central broker that matches tasks to specialist profiles and manages
/// handoff orchestration.
#[derive(Debug, Clone)]
pub struct HandoffBroker {
    pub profiles: Vec<SpecialistProfile>,
    pub match_threshold: f64,
}

impl Default for HandoffBroker {
    fn default() -> Self {
        Self {
            profiles: profiles::default_specialist_profiles(),
            match_threshold: 0.3,
        }
    }
}
