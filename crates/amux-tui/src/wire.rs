#![allow(dead_code)]

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WelesReviewMetaVm {
    #[serde(default)]
    pub weles_reviewed: bool,
    #[serde(default)]
    pub verdict: String,
    #[serde(default)]
    pub reasons: Vec<String>,
    #[serde(default)]
    pub audit_id: Option<String>,
    #[serde(default)]
    pub security_override_mode: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum MessageRole {
    System,
    User,
    Assistant,
    Tool,
    #[default]
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AgentMessage {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub role: MessageRole,
    #[serde(default)]
    pub content: String,

    #[serde(default)]
    pub reasoning: Option<String>,

    #[serde(default)]
    pub provider_final_result_json: Option<String>,

    #[serde(default)]
    pub tool_name: Option<String>,
    #[serde(default)]
    pub tool_arguments: Option<String>,
    #[serde(default)]
    pub tool_call_id: Option<String>,
    #[serde(default)]
    pub tool_status: Option<String>,
    #[serde(default)]
    pub weles_review: Option<WelesReviewMetaVm>,

    #[serde(default)]
    pub input_tokens: u64,
    #[serde(default)]
    pub output_tokens: u64,
    #[serde(default)]
    pub tps: Option<f64>,
    #[serde(default)]
    pub generation_ms: Option<u64>,
    #[serde(default)]
    pub cost: Option<f64>,

    #[serde(default)]
    pub is_streaming: bool,
    #[serde(default)]
    pub message_kind: String,
    #[serde(default)]
    pub compaction_strategy: Option<String>,
    #[serde(default)]
    pub compaction_payload: Option<String>,
    #[serde(default)]
    pub timestamp: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AgentThread {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub agent_name: Option<String>,
    #[serde(default)]
    pub title: String,

    #[serde(default)]
    pub created_at: u64,
    #[serde(default)]
    pub updated_at: u64,

    #[serde(default)]
    pub messages: Vec<AgentMessage>,

    #[serde(default)]
    pub total_input_tokens: u64,
    #[serde(default)]
    pub total_output_tokens: u64,
}

const PERSONA_ID_MARKER: &str = "Agent persona id:";
const WELES_AGENT_ID: &str = "weles";
const WELES_GOVERNANCE_SCOPE: &str = "governance";
const WELES_VITALITY_SCOPE: &str = "vitality";

fn extract_persona_id(content: &str) -> Option<String> {
    content.lines().find_map(|line| {
        let (marker, value) = line.split_once(':')?;
        if marker.trim() != PERSONA_ID_MARKER.trim_end_matches(':') {
            return None;
        }
        let normalized = value.trim().to_ascii_lowercase();
        if normalized.is_empty() {
            None
        } else {
            Some(normalized)
        }
    })
}

pub fn is_weles_thread(thread: &AgentThread) -> bool {
    thread.messages.iter().any(|message| {
        extract_persona_id(&message.content).is_some_and(|persona_id| {
            matches!(
                persona_id.as_str(),
                WELES_AGENT_ID | WELES_GOVERNANCE_SCOPE | WELES_VITALITY_SCOPE
            )
        })
    })
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Queued,
    InProgress,
    AwaitingApproval,
    Blocked,
    FailedAnalyzing,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AgentTask {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub thread_id: Option<String>,
    #[serde(default)]
    pub status: Option<TaskStatus>,
    #[serde(default)]
    pub progress: u8,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub goal_run_id: Option<String>,
    #[serde(default)]
    pub goal_step_title: Option<String>,
    #[serde(default)]
    pub command: Option<String>,
    #[serde(default)]
    pub awaiting_approval_id: Option<String>,
    #[serde(default)]
    pub blocked_reason: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum GoalRunStatus {
    Queued,
    Planning,
    Running,
    AwaitingApproval,
    Paused,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GoalRun {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub thread_id: Option<String>,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub status: Option<GoalRunStatus>,
    #[serde(default)]
    pub current_step_title: Option<String>,
    #[serde(default)]
    pub child_task_count: u32,
    #[serde(default)]
    pub approval_count: u32,
    #[serde(default)]
    pub last_error: Option<String>,
    #[serde(default)]
    pub goal: String,
    #[serde(default)]
    pub current_step_index: usize,
    #[serde(default)]
    pub reflection_summary: Option<String>,
    #[serde(default)]
    pub memory_updates: Vec<String>,
    #[serde(default)]
    pub generated_skill_path: Option<String>,
    #[serde(default)]
    pub child_task_ids: Vec<String>,
    #[serde(default)]
    pub steps: Vec<GoalRunStep>,
    #[serde(default)]
    pub events: Vec<GoalRunEvent>,
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
pub struct FetchedModel {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub context_window: Option<u32>,
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
