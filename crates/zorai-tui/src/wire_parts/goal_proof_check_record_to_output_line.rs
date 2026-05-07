use serde::{Deserialize, Serialize};

use super::extract_persona_id_to_deserialize_goal_binding::{
    deserialize_goal_binding, GoalEvidenceRecord,
};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GoalProofCheckRecord {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub state: String,
    #[serde(default)]
    pub summary: Option<String>,
    #[serde(default)]
    pub evidence_ids: Vec<String>,
    #[serde(default)]
    pub resolved_at: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GoalRunReportRecord {
    #[serde(default)]
    pub summary: String,
    #[serde(default)]
    pub state: String,
    #[serde(default)]
    pub notes: Vec<String>,
    #[serde(default)]
    pub evidence: Vec<GoalEvidenceRecord>,
    #[serde(default)]
    pub proof_checks: Vec<GoalProofCheckRecord>,
    #[serde(default)]
    pub generated_at: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GoalResumeDecisionRecord {
    #[serde(default)]
    pub action: String,
    #[serde(default)]
    pub reason_code: String,
    #[serde(default)]
    pub reason: Option<String>,
    #[serde(default)]
    pub details: Vec<String>,
    #[serde(default)]
    pub decided_at: Option<u64>,
    #[serde(default)]
    pub projection_state: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GoalDeliveryUnitRecord {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub status: String,
    #[serde(default, deserialize_with = "deserialize_goal_binding")]
    pub execution_binding: String,
    #[serde(default, deserialize_with = "deserialize_goal_binding")]
    pub verification_binding: String,
    #[serde(default)]
    pub summary: Option<String>,
    #[serde(default)]
    pub proof_checks: Vec<GoalProofCheckRecord>,
    #[serde(default)]
    pub evidence: Vec<GoalEvidenceRecord>,
    #[serde(default)]
    pub report: Option<GoalRunReportRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GoalRunDossier {
    #[serde(default)]
    pub units: Vec<GoalDeliveryUnitRecord>,
    #[serde(default)]
    pub projection_state: String,
    #[serde(default)]
    pub latest_resume_decision: Option<GoalResumeDecisionRecord>,
    #[serde(default)]
    pub report: Option<GoalRunReportRecord>,
    #[serde(default)]
    pub summary: Option<String>,
    #[serde(default)]
    pub projection_error: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum GoalRunStepKind {
    #[default]
    Reason,
    Command,
    Research,
    Memory,
    Skill,
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum GoalRunStepStatus {
    Pending,
    InProgress,
    Completed,
    Failed,
    Skipped,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GoalRunStep {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub position: usize,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub instructions: String,
    #[serde(default)]
    pub kind: GoalRunStepKind,
    #[serde(default)]
    pub status: Option<GoalRunStepStatus>,
    #[serde(default)]
    pub task_id: Option<String>,
    #[serde(default)]
    pub summary: Option<String>,
    #[serde(default)]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GoalRunEvent {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub timestamp: u64,
    #[serde(default)]
    pub phase: String,
    #[serde(default)]
    pub message: String,
    #[serde(default)]
    pub details: Option<String>,
    #[serde(default)]
    pub step_index: Option<usize>,
    #[serde(default)]
    pub todo_snapshot: Vec<TodoItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CheckpointSummary {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub goal_run_id: String,
    #[serde(default)]
    pub checkpoint_type: String,
    #[serde(default)]
    pub step_index: Option<usize>,
    #[serde(default)]
    pub goal_status: String,
    #[serde(default)]
    pub task_count: usize,
    #[serde(default)]
    pub context_summary_preview: Option<String>,
    #[serde(default)]
    pub created_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RestoreOutcome {
    #[serde(default)]
    pub checkpoint_id: String,
    #[serde(default)]
    pub goal_run_id: String,
    #[serde(default)]
    pub restored_step_index: usize,
    #[serde(default)]
    pub tasks_restored: usize,
    #[serde(default)]
    pub context_restored: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TodoStatus {
    Pending,
    InProgress,
    Completed,
    Blocked,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TodoItem {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub content: String,
    #[serde(default)]
    pub status: Option<TodoStatus>,
    #[serde(default)]
    pub position: usize,
    #[serde(default)]
    pub step_index: Option<usize>,
    #[serde(default)]
    pub created_at: u64,
    #[serde(default)]
    pub updated_at: u64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkContextEntryKind {
    RepoChange,
    Artifact,
    GeneratedSkill,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WorkContextEntry {
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub previous_path: Option<String>,
    #[serde(default)]
    pub kind: Option<WorkContextEntryKind>,
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub change_kind: Option<String>,
    #[serde(default)]
    pub repo_root: Option<String>,
    #[serde(default)]
    pub goal_run_id: Option<String>,
    #[serde(default)]
    pub step_index: Option<usize>,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub is_text: bool,
    #[serde(default)]
    pub updated_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ThreadWorkContext {
    #[serde(default)]
    pub thread_id: String,
    #[serde(default)]
    pub entries: Vec<WorkContextEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct AnticipatoryItem {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub summary: String,
    #[serde(default)]
    pub bullets: Vec<String>,
    #[serde(default)]
    pub confidence: f64,
    #[serde(default)]
    pub goal_run_id: Option<String>,
    #[serde(default)]
    pub thread_id: Option<String>,
    #[serde(default)]
    pub preferred_client_surface: Option<String>,
    #[serde(default)]
    pub preferred_attention_surface: Option<String>,
    #[serde(default)]
    pub created_at: u64,
    #[serde(default)]
    pub updated_at: u64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum HeartbeatOutcome {
    Ok,
    Alert,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct HeartbeatItem {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub last_result: Option<HeartbeatOutcome>,
    #[serde(default)]
    pub last_message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AgentConfigSnapshot {
    #[serde(default)]
    pub provider: String,
    #[serde(default)]
    pub base_url: String,
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub api_key: String,
    #[serde(default)]
    pub assistant_id: String,
    #[serde(default)]
    pub auth_source: String,
    #[serde(default)]
    pub api_transport: String,
    #[serde(default)]
    pub reasoning_effort: String,
    #[serde(default)]
    pub context_window_tokens: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FetchedModelPricing {
    #[serde(default)]
    pub prompt: Option<String>,
    #[serde(default)]
    pub completion: Option<String>,
    #[serde(default)]
    pub image: Option<String>,
    #[serde(default)]
    pub request: Option<String>,
    #[serde(default)]
    pub web_search: Option<String>,
    #[serde(default)]
    pub internal_reasoning: Option<String>,
    #[serde(default)]
    pub input_cache_read: Option<String>,
    #[serde(default)]
    pub input_cache_write: Option<String>,
    #[serde(default)]
    pub audio: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FetchedModel {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub context_window: Option<u32>,
    #[serde(default)]
    pub pricing: Option<FetchedModelPricing>,
    #[serde(default)]
    pub metadata: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputKind {
    Info,
    Success,
    Warning,
    Error,
    Tool,
}

#[derive(Debug, Clone)]
pub struct OutputLine {
    pub timestamp: u64,
    pub kind: OutputKind,
    pub content: String,
}
