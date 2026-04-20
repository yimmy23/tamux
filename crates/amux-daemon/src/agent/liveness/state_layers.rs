//! State persistence layers — the 4-layer model for checkpointing agent state.
//!
//! Layer 1: Goal State — the GoalRun itself (plan, steps, status)
//! Layer 2: Execution State — active tasks, queue positions
//! Layer 3: Context State — thread summaries, compacted context
//! Layer 4: Runtime State — work context, TODOs, memory updates

use serde::{Deserialize, Serialize};

use crate::agent::types::{AgentTask, GoalRun, GoalRunStatus, ThreadWorkContext, TodoItem};

/// Schema version for forward-compatible checkpoint deserialization.
pub const CHECKPOINT_SCHEMA_VERSION: u32 = 1;

/// Type of checkpoint — why it was created.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CheckpointType {
    /// Automatic checkpoint before a goal run step.
    PreStep,
    /// Automatic checkpoint after a goal run step completes.
    PostStep,
    /// User-requested checkpoint.
    Manual,
    /// Checkpoint created before recovery attempt.
    PreRecovery,
    /// Periodic checkpoint during long-running execution.
    Periodic,
}

/// Summary of a checkpoint for listing (without full serialized state).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckpointSummary {
    pub id: String,
    pub goal_run_id: String,
    pub checkpoint_type: CheckpointType,
    pub step_index: Option<usize>,
    pub goal_status: GoalRunStatus,
    pub task_count: usize,
    pub context_summary_preview: Option<String>,
    pub created_at: u64,
}

/// Full checkpoint data — everything needed to restore a goal run to a
/// specific point in time.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckpointData {
    /// Schema version for forward compatibility.
    pub version: u32,
    /// Unique checkpoint identifier.
    pub id: String,
    /// Which goal run this checkpoint belongs to.
    pub goal_run_id: String,
    /// Why this checkpoint was created.
    pub checkpoint_type: CheckpointType,

    // -- Layer 1: Goal State --
    /// Serialized GoalRun at the time of checkpoint.
    pub goal_run: GoalRun,

    // -- Layer 2: Execution State --
    /// Snapshot of tasks related to this goal run.
    pub tasks_snapshot: Vec<AgentTask>,

    // -- Layer 3: Context State --
    /// Summary of the conversation context at checkpoint time.
    pub context_summary: Option<String>,
    /// Thread ID associated with this goal run.
    pub thread_id: Option<String>,
    /// Approximate token count of the context at checkpoint time.
    pub context_tokens: Option<u32>,

    // -- Layer 4: Runtime State --
    /// Work context entries (file artifacts, repo changes).
    pub work_context: Option<ThreadWorkContext>,
    /// TODO items at checkpoint time.
    pub todos: Vec<TodoItem>,
    /// Memory updates accumulated so far.
    pub memory_updates: Vec<String>,

    // -- Metadata --
    /// When this checkpoint was created.
    pub created_at: u64,
    /// Optional human-readable note.
    pub note: Option<String>,
}

impl CheckpointData {
    /// Create a new checkpoint with the current schema version.
    pub fn new(
        id: String,
        goal_run_id: String,
        checkpoint_type: CheckpointType,
        goal_run: GoalRun,
        now: u64,
    ) -> Self {
        Self {
            version: CHECKPOINT_SCHEMA_VERSION,
            id,
            goal_run_id,
            checkpoint_type,
            goal_run,
            tasks_snapshot: Vec::new(),
            context_summary: None,
            thread_id: None,
            context_tokens: None,
            work_context: None,
            todos: Vec::new(),
            memory_updates: Vec::new(),
            created_at: now,
            note: None,
        }
    }

    /// Build a summary for listing without the full state.
    pub fn to_summary(&self) -> CheckpointSummary {
        CheckpointSummary {
            id: self.id.clone(),
            goal_run_id: self.goal_run_id.clone(),
            checkpoint_type: self.checkpoint_type,
            step_index: Some(self.goal_run.current_step_index),
            goal_status: self.goal_run.status,
            task_count: self.tasks_snapshot.len(),
            context_summary_preview: self.context_summary.as_ref().map(|s| {
                if s.chars().count() > 120 {
                    let truncated: String = s.chars().take(117).collect();
                    format!("{truncated}…")
                } else {
                    s.clone()
                }
            }),
            created_at: self.created_at,
        }
    }
}

/// Outcome of attempting to restore from a checkpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RestoreOutcome {
    pub checkpoint_id: String,
    pub goal_run_id: String,
    pub restored_step_index: usize,
    pub tasks_restored: usize,
    pub context_restored: bool,
}

/// Health indicators collected over time for trend analysis.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HealthIndicators {
    /// Timestamp of last meaningful progress (successful tool call or step completion).
    pub last_progress_at: Option<u64>,
    /// Tool calls per minute over the last 5 minutes.
    pub tool_call_frequency: f64,
    /// Error rate (errors / total tool calls) over the last 10 calls.
    pub error_rate: f64,
    /// Tokens consumed per minute (context growth rate).
    pub context_growth_rate: f64,
    /// Current context utilization percentage.
    pub context_utilization_pct: u32,
    /// Number of consecutive errors.
    pub consecutive_errors: u32,
    /// Total tool calls since start.
    pub total_tool_calls: u32,
    /// Total successful tool calls.
    pub successful_tool_calls: u32,
}

/// Overall health state for an entity (task or goal run).
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum HealthState {
    #[default]
    Healthy,
    Degraded,
    Stuck,
    Crashed,
    WaitingForInput,
}

/// A health log entry for audit trail.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthLogEntry {
    pub id: String,
    pub entity_type: String,
    pub entity_id: String,
    pub health_state: HealthState,
    pub indicators: HealthIndicators,
    pub intervention: Option<String>,
    pub created_at: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::types::*;

    fn sample_goal_run() -> GoalRun {
        GoalRun {
            id: "goal_1".into(),
            title: "Test".into(),
            goal: "Do stuff".into(),
            client_request_id: None,
            status: GoalRunStatus::Running,
            priority: TaskPriority::Normal,
            created_at: 100,
            updated_at: 200,
            started_at: Some(110),
            completed_at: None,
            thread_id: Some("thread_1".into()),
            session_id: None,
            current_step_index: 1,
            current_step_title: Some("Step 2".into()),
            current_step_kind: None,
            planner_owner_profile: None,
            current_step_owner_profile: None,
            replan_count: 0,
            max_replans: 2,
            plan_summary: Some("Plan summary".into()),
            reflection_summary: None,
            memory_updates: vec!["learned X".into()],
            generated_skill_path: None,
            last_error: None,
            failure_cause: None,
            stopped_reason: None,
            child_task_ids: vec!["task_1".into()],
            child_task_count: 1,
            approval_count: 0,
            awaiting_approval_id: None,
            policy_fingerprint: None,
            approval_expires_at: None,
            containment_scope: None,
            compensation_status: None,
            compensation_summary: None,
            active_task_id: Some("task_1".into()),
            duration_ms: None,
            steps: vec![],
            events: vec![],
            dossier: None,
            total_prompt_tokens: 0,
            total_completion_tokens: 0,
            estimated_cost_usd: None,
            autonomy_level: Default::default(),
            authorship_tag: None,
            launch_assignment_snapshot: Vec::new(),
            runtime_assignment_list: Vec::new(),
            root_thread_id: None,
            active_thread_id: None,
            execution_thread_ids: Vec::new(),
        }
    }

    #[test]
    fn checkpoint_data_new_uses_current_schema_version() {
        let cp = CheckpointData::new(
            "cp_1".into(),
            "goal_1".into(),
            CheckpointType::PreStep,
            sample_goal_run(),
            1000,
        );
        assert_eq!(cp.version, CHECKPOINT_SCHEMA_VERSION);
        assert_eq!(cp.id, "cp_1");
        assert_eq!(cp.created_at, 1000);
        assert!(cp.tasks_snapshot.is_empty());
    }

    #[test]
    fn checkpoint_summary_truncates_long_context() {
        let mut cp = CheckpointData::new(
            "cp_2".into(),
            "goal_1".into(),
            CheckpointType::PostStep,
            sample_goal_run(),
            2000,
        );
        cp.context_summary = Some("x".repeat(200));
        let summary = cp.to_summary();
        assert!(summary.context_summary_preview.unwrap().len() <= 121);
    }

    #[test]
    fn checkpoint_summary_preserves_short_context() {
        let mut cp = CheckpointData::new(
            "cp_3".into(),
            "goal_1".into(),
            CheckpointType::Manual,
            sample_goal_run(),
            3000,
        );
        cp.context_summary = Some("short context".into());
        let summary = cp.to_summary();
        assert_eq!(
            summary.context_summary_preview.as_deref(),
            Some("short context")
        );
    }

    #[test]
    fn checkpoint_roundtrip_json() {
        let cp = CheckpointData::new(
            "cp_4".into(),
            "goal_1".into(),
            CheckpointType::PreRecovery,
            sample_goal_run(),
            4000,
        );
        let json = serde_json::to_string(&cp).unwrap();
        let restored: CheckpointData = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.id, "cp_4");
        assert_eq!(restored.goal_run.id, "goal_1");
        assert_eq!(restored.checkpoint_type, CheckpointType::PreRecovery);
    }

    #[test]
    fn health_indicators_default_is_healthy() {
        let indicators = HealthIndicators::default();
        assert_eq!(indicators.consecutive_errors, 0);
        assert_eq!(indicators.total_tool_calls, 0);
        assert!(indicators.last_progress_at.is_none());
    }
}
