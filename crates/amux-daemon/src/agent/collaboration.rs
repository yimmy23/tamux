//! Lightweight multi-agent collaboration state built on top of real subagent tasks.

use serde::{Deserialize, Serialize};

use super::*;

mod participants;
mod runtime;

pub(in crate::agent) use participants::detect_disagreements;
use participants::{infer_collaboration_role, normalize_topic};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub(in crate::agent) struct CollaborationSession {
    pub id: String,
    pub parent_task_id: String,
    pub thread_id: Option<String>,
    pub goal_run_id: Option<String>,
    pub mission: String,
    pub agents: Vec<CollaborativeAgent>,
    pub contributions: Vec<Contribution>,
    pub disagreements: Vec<Disagreement>,
    pub consensus: Option<Consensus>,
    pub updated_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(in crate::agent) struct CollaborativeAgent {
    pub task_id: String,
    pub title: String,
    pub role: String,
    pub confidence: f64,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(in crate::agent) struct Contribution {
    pub id: String,
    pub task_id: String,
    pub topic: String,
    pub position: String,
    pub evidence: Vec<String>,
    pub confidence: f64,
    pub created_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(in crate::agent) struct Disagreement {
    pub id: String,
    pub topic: String,
    pub agents: Vec<String>,
    pub positions: Vec<String>,
    pub confidence_gap: f64,
    pub resolution: String,
    #[serde(default)]
    pub votes: Vec<Vote>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub debate_session_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(in crate::agent) struct Vote {
    pub task_id: String,
    pub position: String,
    pub weight: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(in crate::agent) struct Consensus {
    pub topic: String,
    pub winner: String,
    pub rationale: String,
    pub votes: Vec<Vote>,
}

#[cfg(test)]
#[path = "collaboration/tests.rs"]
mod tests;
