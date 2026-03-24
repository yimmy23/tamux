// Local wire type copies (will be replaced by crate::wire imports in Task 9)
// These mirror the types in state.rs
#![allow(dead_code)]

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

#[derive(Debug, Clone, Default)]
pub struct AgentTask {
    pub id: String,
    pub title: String,
    pub thread_id: Option<String>,
    pub status: Option<TaskStatus>,
    pub progress: u8,
    pub session_id: Option<String>,
    pub goal_run_id: Option<String>,
    pub goal_step_title: Option<String>,
    pub awaiting_approval_id: Option<String>,
    pub blocked_reason: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GoalRunStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Default)]
pub struct GoalRunStep {
    pub id: String,
    pub title: String,
    pub status: Option<GoalRunStatus>,
    pub order: u32,
    pub instructions: String,
    pub kind: String,
    pub task_id: Option<String>,
    pub summary: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct GoalRunEvent {
    pub id: String,
    pub timestamp: u64,
    pub phase: String,
    pub message: String,
    pub details: Option<String>,
    pub step_index: Option<usize>,
    pub todo_snapshot: Vec<TodoItem>,
}

#[derive(Debug, Clone, Default)]
pub struct GoalRun {
    pub id: String,
    pub title: String,
    pub thread_id: Option<String>,
    pub session_id: Option<String>,
    pub status: Option<GoalRunStatus>,
    pub current_step_title: Option<String>,
    pub child_task_count: u32,
    pub approval_count: u32,
    pub last_error: Option<String>,
    pub goal: String,
    pub current_step_index: usize,
    pub reflection_summary: Option<String>,
    pub memory_updates: Vec<String>,
    pub generated_skill_path: Option<String>,
    pub child_task_ids: Vec<String>,
    pub steps: Vec<GoalRunStep>,
    pub events: Vec<GoalRunEvent>,
    pub created_at: u64,
    pub updated_at: u64,
}

#[derive(Debug, Clone, Default)]
pub struct GoalRunCheckpointSummary {
    pub id: String,
    pub checkpoint_type: String,
    pub step_index: Option<usize>,
    pub task_count: usize,
    pub context_summary_preview: Option<String>,
    pub created_at: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeartbeatOutcome {
    Ok,
    Warn,
    Error,
}

#[derive(Debug, Clone, Default)]
pub struct HeartbeatItem {
    pub id: String,
    pub label: String,
    pub outcome: Option<HeartbeatOutcome>,
    pub message: Option<String>,
    pub timestamp: u64,
}

#[derive(Debug, Clone, Default)]
pub struct HeartbeatDigestVm {
    pub cycle_id: String,
    pub actionable: bool,
    pub digest: String,
    pub items: Vec<HeartbeatDigestItemVm>,
    pub checked_at: u64,
    pub explanation: Option<String>,
}

#[derive(Debug, Clone)]
pub struct HeartbeatDigestItemVm {
    pub priority: u8,
    pub check_type: String,
    pub title: String,
    pub suggestion: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TodoStatus {
    Pending,
    InProgress,
    Completed,
    Blocked,
}

#[derive(Debug, Clone, Default)]
pub struct TodoItem {
    pub id: String,
    pub content: String,
    pub status: Option<TodoStatus>,
    pub position: usize,
    pub step_index: Option<usize>,
    pub created_at: u64,
    pub updated_at: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkContextEntryKind {
    RepoChange,
    Artifact,
    GeneratedSkill,
}

#[derive(Debug, Clone, Default)]
pub struct WorkContextEntry {
    pub path: String,
    pub previous_path: Option<String>,
    pub kind: Option<WorkContextEntryKind>,
    pub source: String,
    pub change_kind: Option<String>,
    pub repo_root: Option<String>,
    pub goal_run_id: Option<String>,
    pub step_index: Option<usize>,
    pub session_id: Option<String>,
    pub is_text: bool,
    pub updated_at: u64,
}

#[derive(Debug, Clone, Default)]
pub struct ThreadWorkContext {
    pub thread_id: String,
    pub entries: Vec<WorkContextEntry>,
}

#[derive(Debug, Clone, Default)]
pub struct FilePreview {
    pub path: String,
    pub content: String,
    pub truncated: bool,
    pub is_text: bool,
}

// ── TaskAction ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum TaskAction {
    TaskListReceived(Vec<AgentTask>),
    TaskUpdate(AgentTask),
    GoalRunListReceived(Vec<GoalRun>),
    GoalRunDetailReceived(GoalRun),
    GoalRunUpdate(GoalRun),
    GoalRunCheckpointsReceived {
        goal_run_id: String,
        checkpoints: Vec<GoalRunCheckpointSummary>,
    },
    ThreadTodosReceived {
        thread_id: String,
        items: Vec<TodoItem>,
    },
    WorkContextReceived(ThreadWorkContext),
    GitDiffReceived {
        repo_path: String,
        file_path: Option<String>,
        diff: String,
    },
    FilePreviewReceived(FilePreview),
    SelectWorkPath {
        thread_id: String,
        path: Option<String>,
    },
    HeartbeatItemsReceived(Vec<HeartbeatItem>),
    HeartbeatDigestReceived(HeartbeatDigestVm),
}

// ── TaskState ─────────────────────────────────────────────────────────────────

pub struct TaskState {
    tasks: Vec<AgentTask>,
    goal_runs: Vec<GoalRun>,
    goal_run_checkpoints: std::collections::HashMap<String, Vec<GoalRunCheckpointSummary>>,
    thread_todos: std::collections::HashMap<String, Vec<TodoItem>>,
    work_contexts: std::collections::HashMap<String, ThreadWorkContext>,
    selected_work_paths: std::collections::HashMap<String, String>,
    git_diffs: std::collections::HashMap<String, String>,
    file_previews: std::collections::HashMap<String, FilePreview>,
    heartbeat_items: Vec<HeartbeatItem>,
    last_digest: Option<HeartbeatDigestVm>,
}

impl TaskState {
    pub fn new() -> Self {
        Self {
            tasks: Vec::new(),
            goal_runs: Vec::new(),
            goal_run_checkpoints: std::collections::HashMap::new(),
            thread_todos: std::collections::HashMap::new(),
            work_contexts: std::collections::HashMap::new(),
            selected_work_paths: std::collections::HashMap::new(),
            git_diffs: std::collections::HashMap::new(),
            file_previews: std::collections::HashMap::new(),
            heartbeat_items: Vec::new(),
            last_digest: None,
        }
    }

    pub fn tasks(&self) -> &[AgentTask] {
        &self.tasks
    }

    pub fn goal_runs(&self) -> &[GoalRun] {
        &self.goal_runs
    }

    pub fn heartbeat_items(&self) -> &[HeartbeatItem] {
        &self.heartbeat_items
    }

    pub fn last_digest(&self) -> Option<&HeartbeatDigestVm> {
        self.last_digest.as_ref()
    }

    pub fn todos_for_thread(&self, thread_id: &str) -> &[TodoItem] {
        self.thread_todos
            .get(thread_id)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    pub fn work_context_for_thread(&self, thread_id: &str) -> Option<&ThreadWorkContext> {
        self.work_contexts.get(thread_id)
    }

    pub fn selected_work_path(&self, thread_id: &str) -> Option<&str> {
        self.selected_work_paths.get(thread_id).map(String::as_str)
    }

    pub fn diff_for_path(&self, repo_root: &str, path: &str) -> Option<&str> {
        self.git_diffs
            .get(&format!("{repo_root}::{path}"))
            .map(String::as_str)
    }

    pub fn preview_for_path(&self, path: &str) -> Option<&FilePreview> {
        self.file_previews.get(path)
    }

    pub fn task_by_id(&self, id: &str) -> Option<&AgentTask> {
        self.tasks.iter().find(|t| t.id == id)
    }

    pub fn goal_run_by_id(&self, id: &str) -> Option<&GoalRun> {
        self.goal_runs.iter().find(|r| r.id == id)
    }

    pub fn checkpoints_for_goal_run(&self, goal_run_id: &str) -> &[GoalRunCheckpointSummary] {
        self.goal_run_checkpoints
            .get(goal_run_id)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    pub fn reduce(&mut self, action: TaskAction) {
        match action {
            TaskAction::TaskListReceived(tasks) => {
                self.tasks = tasks;
            }

            TaskAction::TaskUpdate(updated) => {
                if let Some(existing) = self.tasks.iter_mut().find(|t| t.id == updated.id) {
                    *existing = updated;
                } else {
                    self.tasks.push(updated);
                }
            }

            TaskAction::GoalRunListReceived(runs) => {
                self.goal_runs = runs;
            }

            TaskAction::GoalRunDetailReceived(run) | TaskAction::GoalRunUpdate(run) => {
                if let Some(existing) = self.goal_runs.iter_mut().find(|r| r.id == run.id) {
                    *existing = run;
                } else {
                    self.goal_runs.push(run);
                }
            }

            TaskAction::GoalRunCheckpointsReceived {
                goal_run_id,
                checkpoints,
            } => {
                self.goal_run_checkpoints.insert(goal_run_id, checkpoints);
            }

            TaskAction::ThreadTodosReceived { thread_id, items } => {
                self.thread_todos.insert(thread_id, items);
            }

            TaskAction::WorkContextReceived(context) => {
                let thread_id = context.thread_id.clone();
                let default_selection = context.entries.first().map(|entry| entry.path.clone());
                self.work_contexts.insert(thread_id.clone(), context);
                if let Some(selection) = default_selection {
                    self.selected_work_paths
                        .entry(thread_id)
                        .or_insert(selection);
                }
            }

            TaskAction::GitDiffReceived {
                repo_path,
                file_path,
                diff,
            } => {
                if let Some(file_path) = file_path {
                    self.git_diffs
                        .insert(format!("{repo_path}::{file_path}"), diff);
                }
            }

            TaskAction::FilePreviewReceived(preview) => {
                self.file_previews.insert(preview.path.clone(), preview);
            }

            TaskAction::SelectWorkPath { thread_id, path } => {
                if let Some(path) = path {
                    self.selected_work_paths.insert(thread_id, path);
                } else {
                    self.selected_work_paths.remove(&thread_id);
                }
            }

            TaskAction::HeartbeatItemsReceived(items) => {
                self.heartbeat_items = items;
            }

            TaskAction::HeartbeatDigestReceived(digest) => {
                self.last_digest = Some(digest);
            }
        }
    }
}

impl Default for TaskState {
    fn default() -> Self {
        Self::new()
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_task(id: &str, title: &str) -> AgentTask {
        AgentTask {
            id: id.into(),
            title: title.into(),
            ..Default::default()
        }
    }

    fn make_goal_run(id: &str, title: &str) -> GoalRun {
        GoalRun {
            id: id.into(),
            title: title.into(),
            ..Default::default()
        }
    }

    #[test]
    fn task_list_received_replaces_tasks() {
        let mut state = TaskState::new();
        state.reduce(TaskAction::TaskListReceived(vec![
            make_task("t1", "First"),
            make_task("t2", "Second"),
        ]));
        assert_eq!(state.tasks().len(), 2);

        // Replace with a smaller list
        state.reduce(TaskAction::TaskListReceived(vec![make_task("t3", "Third")]));
        assert_eq!(state.tasks().len(), 1);
        assert_eq!(state.tasks()[0].id, "t3");
    }

    #[test]
    fn task_update_upserts_by_id() {
        let mut state = TaskState::new();
        state.reduce(TaskAction::TaskListReceived(vec![make_task(
            "t1", "Original",
        )]));

        // Update existing task
        state.reduce(TaskAction::TaskUpdate(AgentTask {
            id: "t1".into(),
            title: "Updated".into(),
            status: Some(TaskStatus::InProgress),
            ..Default::default()
        }));
        assert_eq!(state.tasks().len(), 1);
        assert_eq!(state.tasks()[0].title, "Updated");
        assert_eq!(state.tasks()[0].status, Some(TaskStatus::InProgress));

        // Insert new task
        state.reduce(TaskAction::TaskUpdate(make_task("t2", "New")));
        assert_eq!(state.tasks().len(), 2);
    }

    #[test]
    fn goal_run_list_received_replaces_goal_runs() {
        let mut state = TaskState::new();
        state.reduce(TaskAction::GoalRunListReceived(vec![
            make_goal_run("g1", "Goal One"),
            make_goal_run("g2", "Goal Two"),
        ]));
        assert_eq!(state.goal_runs().len(), 2);

        state.reduce(TaskAction::GoalRunListReceived(vec![]));
        assert_eq!(state.goal_runs().len(), 0);
    }

    #[test]
    fn goal_run_detail_received_upserts() {
        let mut state = TaskState::new();
        state.reduce(TaskAction::GoalRunListReceived(vec![make_goal_run(
            "g1", "Original",
        )]));

        // Update via detail
        state.reduce(TaskAction::GoalRunDetailReceived(GoalRun {
            id: "g1".into(),
            title: "Detailed".into(),
            ..Default::default()
        }));
        assert_eq!(state.goal_runs().len(), 1);
        assert_eq!(state.goal_runs()[0].title, "Detailed");

        // Insert new via update
        state.reduce(TaskAction::GoalRunUpdate(make_goal_run("g2", "New Goal")));
        assert_eq!(state.goal_runs().len(), 2);
    }

    #[test]
    fn goal_run_checkpoints_received_replaces_goal_run_checkpoints() {
        let mut state = TaskState::new();
        state.reduce(TaskAction::GoalRunCheckpointsReceived {
            goal_run_id: "g1".into(),
            checkpoints: vec![GoalRunCheckpointSummary {
                id: "cp_1".into(),
                checkpoint_type: "manual".into(),
                task_count: 2,
                ..Default::default()
            }],
        });
        assert_eq!(state.checkpoints_for_goal_run("g1").len(), 1);
        assert_eq!(state.checkpoints_for_goal_run("g1")[0].id, "cp_1");

        state.reduce(TaskAction::GoalRunCheckpointsReceived {
            goal_run_id: "g1".into(),
            checkpoints: vec![],
        });
        assert!(state.checkpoints_for_goal_run("g1").is_empty());
    }

    #[test]
    fn heartbeat_items_received_replaces() {
        let mut state = TaskState::new();
        let items = vec![
            HeartbeatItem {
                id: "h1".into(),
                label: "Service A".into(),
                ..Default::default()
            },
            HeartbeatItem {
                id: "h2".into(),
                label: "Service B".into(),
                ..Default::default()
            },
        ];
        state.reduce(TaskAction::HeartbeatItemsReceived(items));
        assert_eq!(state.heartbeat_items().len(), 2);

        state.reduce(TaskAction::HeartbeatItemsReceived(vec![]));
        assert_eq!(state.heartbeat_items().len(), 0);
    }

    #[test]
    fn task_by_id_returns_correct_task() {
        let mut state = TaskState::new();
        state.reduce(TaskAction::TaskListReceived(vec![
            make_task("t1", "Alpha"),
            make_task("t2", "Beta"),
        ]));
        assert_eq!(
            state.task_by_id("t2").map(|t| t.title.as_str()),
            Some("Beta")
        );
        assert!(state.task_by_id("unknown").is_none());
    }

    #[test]
    fn heartbeat_digest_received_stores_and_replaces() {
        let mut state = TaskState::new();
        assert!(state.last_digest().is_none());
        state.reduce(TaskAction::HeartbeatDigestReceived(HeartbeatDigestVm {
            cycle_id: "c1".into(),
            actionable: true,
            digest: "2 items".into(),
            items: vec![HeartbeatDigestItemVm {
                priority: 1,
                check_type: "stale_todos".into(),
                title: "3 stale TODOs".into(),
                suggestion: "Review them".into(),
            }],
            checked_at: 1234567890,
            explanation: None,
        }));
        let d = state.last_digest().unwrap();
        assert_eq!(d.cycle_id, "c1");
        assert!(d.actionable);
        assert_eq!(d.items.len(), 1);
        assert_eq!(d.items[0].priority, 1);

        // Replace with new digest
        state.reduce(TaskAction::HeartbeatDigestReceived(HeartbeatDigestVm {
            cycle_id: "c2".into(),
            actionable: false,
            digest: "All clear".into(),
            items: vec![],
            checked_at: 1234567891,
            explanation: None,
        }));
        let d = state.last_digest().unwrap();
        assert_eq!(d.cycle_id, "c2");
        assert!(!d.actionable);
        assert!(d.items.is_empty());
    }
}
