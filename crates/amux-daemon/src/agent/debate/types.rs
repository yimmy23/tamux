#![allow(dead_code)]

use serde::{Deserialize, Serialize};

use crate::agent::handoff::divergent::Framing;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DebateStatus {
    InProgress,
    Completed,
    Failed,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RoleKind {
    Proponent,
    Skeptic,
    Synthesizer,
}

impl RoleKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Proponent => "proponent",
            Self::Skeptic => "skeptic",
            Self::Synthesizer => "synthesizer",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DebateRole {
    pub role: RoleKind,
    pub agent_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub system_prompt_override: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Argument {
    pub id: String,
    pub round: u8,
    pub role: RoleKind,
    pub agent_id: String,
    pub content: String,
    #[serde(default)]
    pub evidence_refs: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub responds_to: Option<String>,
    pub timestamp_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DebateVerdict {
    pub consensus_points: Vec<String>,
    pub unresolved_tensions: Vec<String>,
    pub recommended_action: String,
    pub confidence: f64,
    pub synthesizer_agent: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DebateSession {
    pub id: String,
    pub topic: String,
    pub framings: Vec<Framing>,
    pub max_rounds: u8,
    pub current_round: u8,
    pub roles: Vec<DebateRole>,
    pub arguments: Vec<Argument>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verdict: Option<DebateVerdict>,
    pub status: DebateStatus,
    pub created_at_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completed_at_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thread_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub goal_run_id: Option<String>,
}
