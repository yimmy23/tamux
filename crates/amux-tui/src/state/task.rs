// Local wire type copies (will be replaced by crate::wire imports in Task 9)
// These mirror the types in state.rs
#![allow(dead_code)]

pub const GOAL_RUN_HISTORY_FETCH_DEBOUNCE_TICKS: u64 = 6;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskStatus {
    Queued,
    InProgress,
    AwaitingApproval,
    Blocked,
    FailedAnalyzing,
    BudgetExceeded,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Default)]
pub struct AgentTask {
    pub id: String,
    pub title: String,
    pub description: String,
    pub thread_id: Option<String>,
    pub parent_task_id: Option<String>,
    pub parent_thread_id: Option<String>,
    pub created_at: u64,
    pub status: Option<TaskStatus>,
    pub progress: u8,
    pub session_id: Option<String>,
    pub goal_run_id: Option<String>,
    pub goal_step_title: Option<String>,
    pub command: Option<String>,
    pub awaiting_approval_id: Option<String>,
    pub blocked_reason: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GoalRuntimeOwnerProfile {
    pub agent_label: String,
    pub provider: String,
    pub model: String,
    pub reasoning_effort: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GoalAgentAssignment {
    pub role_id: String,
    pub enabled: bool,
    pub provider: String,
    pub model: String,
    pub reasoning_effort: Option<String>,
    pub inherit_from_main: bool,
}

#[derive(Debug, Clone, Default)]
pub struct GoalRun {
    pub id: String,
    pub title: String,
    pub thread_id: Option<String>,
    pub root_thread_id: Option<String>,
    pub active_thread_id: Option<String>,
    pub execution_thread_ids: Vec<String>,
    pub session_id: Option<String>,
    pub status: Option<GoalRunStatus>,
    pub current_step_title: Option<String>,
    pub launch_assignment_snapshot: Vec<GoalAgentAssignment>,
    pub runtime_assignment_list: Vec<GoalAgentAssignment>,
    pub planner_owner_profile: Option<GoalRuntimeOwnerProfile>,
    pub current_step_owner_profile: Option<GoalRuntimeOwnerProfile>,
    pub child_task_count: u32,
    pub approval_count: u32,
    pub awaiting_approval_id: Option<String>,
    pub last_error: Option<String>,
    pub goal: String,
    pub current_step_index: usize,
    pub reflection_summary: Option<String>,
    pub memory_updates: Vec<String>,
    pub generated_skill_path: Option<String>,
    pub child_task_ids: Vec<String>,
    pub loaded_step_start: usize,
    pub loaded_step_end: usize,
    pub total_step_count: usize,
    pub loaded_event_start: usize,
    pub loaded_event_end: usize,
    pub total_event_count: usize,
    pub older_page_pending: bool,
    pub older_page_request_cooldown_until_tick: Option<u64>,
    pub sparse_update: bool,
    pub steps: Vec<GoalRunStep>,
    pub events: Vec<GoalRunEvent>,
    pub dossier: Option<GoalRunDossier>,
    pub created_at: u64,
    pub updated_at: u64,
}

#[derive(Debug, Clone, Default)]
pub struct GoalEvidenceRecord {
    pub id: String,
    pub title: String,
    pub source: Option<String>,
    pub uri: Option<String>,
    pub summary: Option<String>,
    pub captured_at: Option<u64>,
}

#[derive(Debug, Clone, Default)]
pub struct GoalProofCheckRecord {
    pub id: String,
    pub title: String,
    pub state: String,
    pub summary: Option<String>,
    pub evidence_ids: Vec<String>,
    pub resolved_at: Option<u64>,
}

#[derive(Debug, Clone, Default)]
pub struct GoalRunReportRecord {
    pub summary: String,
    pub state: String,
    pub notes: Vec<String>,
    pub evidence: Vec<GoalEvidenceRecord>,
    pub proof_checks: Vec<GoalProofCheckRecord>,
    pub generated_at: Option<u64>,
}

#[derive(Debug, Clone, Default)]
pub struct GoalResumeDecisionRecord {
    pub action: String,
    pub reason_code: String,
    pub reason: Option<String>,
    pub details: Vec<String>,
    pub decided_at: Option<u64>,
    pub projection_state: String,
}

#[derive(Debug, Clone, Default)]
pub struct GoalDeliveryUnitRecord {
    pub id: String,
    pub title: String,
    pub status: String,
    pub execution_binding: String,
    pub verification_binding: String,
    pub summary: Option<String>,
    pub proof_checks: Vec<GoalProofCheckRecord>,
    pub evidence: Vec<GoalEvidenceRecord>,
    pub report: Option<GoalRunReportRecord>,
}

#[derive(Debug, Clone, Default)]
pub struct GoalRunDossier {
    pub units: Vec<GoalDeliveryUnitRecord>,
    pub projection_state: String,
    pub latest_resume_decision: Option<GoalResumeDecisionRecord>,
    pub report: Option<GoalRunReportRecord>,
    pub summary: Option<String>,
    pub projection_error: Option<String>,
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
    GoalRunDeleted {
        goal_run_id: String,
    },
    ThreadTodosReceived {
        thread_id: String,
        goal_run_id: Option<String>,
        step_index: Option<usize>,
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
    tasks_revision: u64,
    goal_runs: Vec<GoalRun>,
    goal_run_checkpoints: std::collections::HashMap<String, Vec<GoalRunCheckpointSummary>>,
    thread_todos: std::collections::HashMap<String, Vec<TodoItem>>,
    goal_step_live_todos: std::collections::HashMap<String, Vec<TodoItem>>,
    goal_thread_ids: std::collections::HashMap<String, Vec<String>>,
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
            tasks_revision: 0,
            goal_runs: Vec::new(),
            goal_run_checkpoints: std::collections::HashMap::new(),
            thread_todos: std::collections::HashMap::new(),
            goal_step_live_todos: std::collections::HashMap::new(),
            goal_thread_ids: std::collections::HashMap::new(),
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

    pub fn tasks_revision(&self) -> u64 {
        self.tasks_revision
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

    pub fn spawned_tree_items(&self) -> &[AgentTask] {
        &self.tasks
    }

    pub fn goal_run_by_id(&self, id: &str) -> Option<&GoalRun> {
        self.goal_runs.iter().find(|r| r.id == id)
    }

    pub fn goal_run_by_id_mut(&mut self, id: &str) -> Option<&mut GoalRun> {
        self.goal_runs.iter_mut().find(|r| r.id == id)
    }

    pub fn thread_belongs_to_goal_run(&self, goal_run_id: &str, thread_id: &str) -> bool {
        self.goal_run_by_id(goal_run_id).is_some_and(|run| {
            goal_step_todo_thread_ids(self, run)
                .iter()
                .any(|candidate| candidate == thread_id)
        })
    }

    pub fn goal_thread_ids(&self, goal_run_id: &str) -> Vec<String> {
        let Some(run) = self.goal_run_by_id(goal_run_id) else {
            return Vec::new();
        };
        goal_step_todo_thread_ids(self, run)
    }

    pub fn checkpoints_for_goal_run(&self, goal_run_id: &str) -> &[GoalRunCheckpointSummary] {
        self.goal_run_checkpoints
            .get(goal_run_id)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    pub fn goal_steps_in_display_order(&self, goal_run_id: &str) -> Vec<&GoalRunStep> {
        let Some(run) = self.goal_run_by_id(goal_run_id) else {
            return Vec::new();
        };

        let mut steps: Vec<_> = run.steps.iter().collect();
        steps.sort_by_key(|step| step.order);
        steps
    }

    pub fn goal_step_todos(&self, goal_run_id: &str, step_index: usize) -> Vec<TodoItem> {
        let Some(run) = self.goal_run_by_id(goal_run_id) else {
            return Vec::new();
        };

        if let Some(live_todos) = self
            .goal_step_live_todos
            .get(&goal_step_live_todo_key(goal_run_id, step_index))
        {
            let mut todos = live_todos.clone();
            todos.sort_by_key(|todo| todo.position);
            return todos;
        }

        for event in run.events.iter().rev() {
            let mut todos = event
                .todo_snapshot
                .iter()
                .filter(|todo| todo.step_index.or(event.step_index) == Some(step_index))
                .cloned()
                .collect::<Vec<_>>();
            if !todos.is_empty() {
                todos.sort_by_key(|todo| todo.position);
                return todos;
            }
        }

        Vec::new()
    }

    pub fn goal_step_checkpoints(
        &self,
        goal_run_id: &str,
        step_index: usize,
    ) -> Vec<&GoalRunCheckpointSummary> {
        self.checkpoints_for_goal_run(goal_run_id)
            .iter()
            .filter(|checkpoint| checkpoint.step_index == Some(step_index))
            .collect()
    }

    pub fn goal_step_files(
        &self,
        goal_run_id: &str,
        thread_id: &str,
        step_index: usize,
    ) -> Vec<&WorkContextEntry> {
        let Some(context) = self.work_context_for_thread(thread_id) else {
            return Vec::new();
        };

        context
            .entries
            .iter()
            .filter(|entry| {
                entry.goal_run_id.as_deref() == Some(goal_run_id)
                    && entry.step_index == Some(step_index)
            })
            .collect()
    }

    pub fn goal_run_next_page_request(
        &self,
        goal_run_id: &str,
        current_tick: u64,
    ) -> Option<(Option<usize>, Option<usize>, Option<usize>, Option<usize>)> {
        let run = self.goal_run_by_id(goal_run_id)?;
        if run.older_page_pending
            || run
                .older_page_request_cooldown_until_tick
                .is_some_and(|until| current_tick < until)
        {
            return None;
        }

        let step_limit = run
            .loaded_step_start
            .min(run.loaded_step_end.saturating_sub(run.loaded_step_start));
        let event_limit = run
            .loaded_event_start
            .min(run.loaded_event_end.saturating_sub(run.loaded_event_start));
        let step_request =
            (step_limit > 0).then_some((run.loaded_step_start - step_limit, step_limit));
        let event_request =
            (event_limit > 0).then_some((run.loaded_event_start - event_limit, event_limit));

        if step_request.is_none() && event_request.is_none() {
            return None;
        }

        Some((
            step_request.map(|(offset, _)| offset),
            step_request.map(|(_, limit)| limit),
            event_request.map(|(offset, _)| offset),
            event_request.map(|(_, limit)| limit),
        ))
    }

    pub fn mark_goal_run_older_page_pending(
        &mut self,
        goal_run_id: &str,
        pending: bool,
        current_tick: u64,
        debounce_ticks: u64,
    ) {
        if let Some(run) = self.goal_run_by_id_mut(goal_run_id) {
            run.older_page_pending = pending;
            if pending {
                run.older_page_request_cooldown_until_tick =
                    Some(current_tick.saturating_add(debounce_ticks));
            }
        }
    }

    pub fn reduce(&mut self, action: TaskAction) {
        match action {
            TaskAction::TaskListReceived(tasks) => {
                self.tasks = tasks;
                self.tasks_revision = self.tasks_revision.wrapping_add(1);
                reconcile_goal_run_status_from_tasks(&self.tasks, &mut self.goal_runs);
            }

            TaskAction::TaskUpdate(updated) => {
                if let Some(existing) = self.tasks.iter_mut().find(|t| t.id == updated.id) {
                    let merged = merge_task_update(existing, updated);
                    *existing = merged;
                } else {
                    self.tasks.push(updated);
                }
                self.tasks_revision = self.tasks_revision.wrapping_add(1);
                reconcile_goal_run_status_from_tasks(&self.tasks, &mut self.goal_runs);
            }

            TaskAction::GoalRunListReceived(runs) => {
                self.goal_runs = runs.into_iter().map(normalize_goal_run_ranges).collect();
                self.goal_thread_ids.retain(|goal_run_id, _| {
                    self.goal_runs.iter().any(|run| run.id == *goal_run_id)
                });
            }

            TaskAction::GoalRunDetailReceived(run) => {
                let run = normalize_goal_run_ranges(run);
                self.goal_step_live_todos
                    .retain(|key, _| !key.starts_with(&format!("{}::", run.id)));
                if let Some(existing) = self.goal_runs.iter_mut().find(|r| r.id == run.id) {
                    merge_goal_run(existing, run, false);
                } else {
                    self.goal_runs.insert(0, run);
                }
            }

            TaskAction::GoalRunUpdate(run) => {
                let run = normalize_goal_run_ranges(run);
                if let Some(existing) = self.goal_runs.iter_mut().find(|r| r.id == run.id) {
                    merge_goal_run(existing, run, true);
                } else {
                    self.goal_runs.insert(0, run);
                }
            }

            TaskAction::GoalRunCheckpointsReceived {
                goal_run_id,
                checkpoints,
            } => {
                self.goal_run_checkpoints.insert(goal_run_id, checkpoints);
            }

            TaskAction::GoalRunDeleted { goal_run_id } => {
                self.goal_runs.retain(|run| run.id != goal_run_id);
                self.goal_run_checkpoints.remove(&goal_run_id);
                self.goal_step_live_todos
                    .retain(|key, _| !key.starts_with(&format!("{goal_run_id}::")));
                self.goal_thread_ids.remove(&goal_run_id);
                let previous_task_len = self.tasks.len();
                self.tasks
                    .retain(|task| task.goal_run_id.as_deref() != Some(goal_run_id.as_str()));
                if self.tasks.len() != previous_task_len {
                    self.tasks_revision = self.tasks_revision.wrapping_add(1);
                }
            }

            TaskAction::ThreadTodosReceived {
                thread_id,
                goal_run_id,
                step_index,
                items,
            } => {
                if let Some(goal_run_id) = goal_run_id {
                    remember_goal_thread(&mut self.goal_thread_ids, &goal_run_id, &thread_id);
                    let Some(step_index) = step_index else {
                        self.thread_todos.insert(thread_id, items);
                        return;
                    };
                    self.goal_step_live_todos.insert(
                        goal_step_live_todo_key(&goal_run_id, step_index),
                        items.clone(),
                    );
                }
                self.thread_todos.insert(thread_id, items);
            }

            TaskAction::WorkContextReceived(context) => {
                let thread_id = context.thread_id.clone();
                let default_selection = context.entries.first().map(|entry| entry.path.clone());
                for goal_run_id in context.entries.iter().filter_map(|entry| {
                    entry
                        .goal_run_id
                        .as_deref()
                        .filter(|goal_run_id| !goal_run_id.is_empty())
                }) {
                    remember_goal_thread(&mut self.goal_thread_ids, goal_run_id, &thread_id);
                }
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

fn goal_step_todo_thread_ids(state: &TaskState, run: &GoalRun) -> Vec<String> {
    fn push_unique(ids: &mut Vec<String>, id: &str) -> bool {
        if id.is_empty() || ids.iter().any(|existing| existing == id) {
            return false;
        }
        ids.push(id.to_string());
        true
    }

    let mut thread_ids = Vec::new();
    let mut task_ids = Vec::new();

    for thread_id in run
        .active_thread_id
        .iter()
        .chain(run.root_thread_id.iter())
        .chain(run.thread_id.iter())
    {
        push_unique(&mut thread_ids, thread_id);
    }
    for thread_id in &run.execution_thread_ids {
        push_unique(&mut thread_ids, thread_id);
    }
    for task in state
        .tasks()
        .iter()
        .filter(|task| task.goal_run_id.as_deref() == Some(run.id.as_str()))
    {
        push_unique(&mut task_ids, &task.id);
        if let Some(thread_id) = task.thread_id.as_deref() {
            push_unique(&mut thread_ids, thread_id);
        }
    }
    if let Some(goal_threads) = state.goal_thread_ids.get(&run.id) {
        for thread_id in goal_threads {
            push_unique(&mut thread_ids, thread_id);
        }
    }
    loop {
        let mut changed = false;
        for task in state.tasks() {
            let belongs_to_goal = task.goal_run_id.as_deref() == Some(run.id.as_str())
                || task
                    .parent_task_id
                    .as_deref()
                    .is_some_and(|parent_task_id| task_ids.iter().any(|id| id == parent_task_id))
                || task
                    .parent_thread_id
                    .as_deref()
                    .is_some_and(|parent_thread_id| {
                        thread_ids.iter().any(|id| id == parent_thread_id)
                    });
            if !belongs_to_goal {
                continue;
            }

            changed |= push_unique(&mut task_ids, &task.id);

            if let Some(thread_id) = task.thread_id.as_deref() {
                changed |= push_unique(&mut thread_ids, thread_id);
            }
        }
        if !changed {
            break;
        }
    }

    thread_ids
}

fn remember_goal_thread(
    goal_thread_ids: &mut std::collections::HashMap<String, Vec<String>>,
    goal_run_id: &str,
    thread_id: &str,
) {
    if goal_run_id.is_empty() || thread_id.is_empty() {
        return;
    }
    let threads = goal_thread_ids.entry(goal_run_id.to_string()).or_default();
    if !threads.iter().any(|existing| existing == thread_id) {
        threads.push(thread_id.to_string());
    }
}

fn goal_step_live_todo_key(goal_run_id: &str, step_index: usize) -> String {
    format!("{goal_run_id}::{step_index}")
}

fn reconcile_goal_run_status_from_tasks(tasks: &[AgentTask], goal_runs: &mut [GoalRun]) {
    for goal_run in goal_runs {
        if matches!(
            goal_run.status,
            Some(GoalRunStatus::Completed | GoalRunStatus::Failed | GoalRunStatus::Cancelled)
                | Some(GoalRunStatus::Planning)
        ) {
            continue;
        }

        let mut next_status = None;
        for task in tasks
            .iter()
            .filter(|task| task.goal_run_id.as_deref() == Some(goal_run.id.as_str()))
        {
            match task.status {
                Some(TaskStatus::AwaitingApproval) => {
                    next_status = Some(GoalRunStatus::AwaitingApproval);
                    break;
                }
                Some(
                    TaskStatus::Queued
                    | TaskStatus::InProgress
                    | TaskStatus::Blocked
                    | TaskStatus::FailedAnalyzing,
                ) => {
                    next_status = Some(GoalRunStatus::Running);
                }
                _ => {}
            }
        }

        if let Some(next_status) = next_status {
            goal_run.status = Some(next_status);
        }
    }
}

fn merge_task_update(existing: &AgentTask, mut updated: AgentTask) -> AgentTask {
    if updated.description.is_empty() {
        updated.description = existing.description.clone();
    }
    if updated.thread_id.is_none() {
        updated.thread_id = existing.thread_id.clone();
    }
    if updated.parent_task_id.is_none() {
        updated.parent_task_id = existing.parent_task_id.clone();
    }
    if updated.parent_thread_id.is_none() {
        updated.parent_thread_id = existing.parent_thread_id.clone();
    }
    if updated.created_at == 0 {
        updated.created_at = existing.created_at;
    }
    if updated.session_id.is_none() {
        updated.session_id = existing.session_id.clone();
    }
    if updated.goal_run_id.is_none() {
        updated.goal_run_id = existing.goal_run_id.clone();
    }
    if updated.goal_step_title.is_none() {
        updated.goal_step_title = existing.goal_step_title.clone();
    }
    if updated.command.is_none() {
        updated.command = existing.command.clone();
    }
    let effective_status = updated.status.or(existing.status);
    if updated.awaiting_approval_id.is_none()
        && matches!(effective_status, Some(TaskStatus::AwaitingApproval))
    {
        updated.awaiting_approval_id = existing.awaiting_approval_id.clone();
    }
    if updated.blocked_reason.is_none() {
        updated.blocked_reason = existing.blocked_reason.clone();
    }

    updated
}

impl super::spawned_tree::SpawnedAgentTreeSource for AgentTask {
    fn spawned_tree_identity(&self) -> &str {
        &self.id
    }

    fn spawned_tree_created_at(&self) -> u64 {
        self.created_at
    }

    fn spawned_tree_thread_id(&self) -> Option<&str> {
        self.thread_id.as_deref()
    }

    fn spawned_tree_parent_task_id(&self) -> Option<&str> {
        self.parent_task_id.as_deref()
    }

    fn spawned_tree_parent_thread_id(&self) -> Option<&str> {
        self.parent_thread_id.as_deref()
    }

    fn spawned_tree_status(&self) -> Option<TaskStatus> {
        self.status
    }
}

impl Default for TaskState {
    fn default() -> Self {
        Self::new()
    }
}

fn normalize_goal_run_ranges(mut run: GoalRun) -> GoalRun {
    if run.total_step_count == 0 {
        run.total_step_count = run.steps.len();
    }
    if run.loaded_step_end == 0 && !run.steps.is_empty() {
        run.loaded_step_end = run.loaded_step_start.saturating_add(run.steps.len());
    }
    if run.loaded_step_end < run.loaded_step_start {
        run.loaded_step_end = run.loaded_step_start;
    }
    if run.loaded_step_start == 0
        && run.loaded_step_end == 0
        && run.total_step_count == run.steps.len()
    {
        run.loaded_step_end = run.steps.len();
    }
    run.total_step_count = run.total_step_count.max(run.loaded_step_end);

    if run.total_event_count == 0 {
        run.total_event_count = run.events.len();
    }
    if run.loaded_event_end == 0 && !run.events.is_empty() {
        run.loaded_event_end = run.loaded_event_start.saturating_add(run.events.len());
    }
    if run.loaded_event_end < run.loaded_event_start {
        run.loaded_event_end = run.loaded_event_start;
    }
    if run.loaded_event_start == 0
        && run.loaded_event_end == 0
        && run.total_event_count == run.events.len()
    {
        run.loaded_event_end = run.events.len();
    }
    run.total_event_count = run.total_event_count.max(run.loaded_event_end);
    run
}

fn merge_range_vec<T: Clone>(
    existing_start: usize,
    existing_end: usize,
    existing_items: &[T],
    incoming_start: usize,
    incoming_end: usize,
    incoming_items: &[T],
) -> (usize, usize, Vec<T>) {
    if existing_items.is_empty() || existing_start == existing_end {
        return (incoming_start, incoming_end, incoming_items.to_vec());
    }
    if incoming_items.is_empty() || incoming_start == incoming_end {
        return (existing_start, existing_end, existing_items.to_vec());
    }

    if incoming_end <= existing_start {
        let mut merged = incoming_items.to_vec();
        merged.extend_from_slice(existing_items);
        return (incoming_start, existing_end, merged);
    }
    if existing_end <= incoming_start {
        let mut merged = existing_items.to_vec();
        merged.extend_from_slice(incoming_items);
        return (existing_start, incoming_end, merged);
    }

    let union_start = existing_start.min(incoming_start);
    let union_end = existing_end.max(incoming_end);
    let mut merged = Vec::with_capacity(union_end.saturating_sub(union_start));
    for absolute_idx in union_start..union_end {
        if absolute_idx >= incoming_start && absolute_idx < incoming_end {
            merged.push(incoming_items[absolute_idx - incoming_start].clone());
        } else if absolute_idx >= existing_start && absolute_idx < existing_end {
            merged.push(existing_items[absolute_idx - existing_start].clone());
        }
    }
    (union_start, union_end, merged)
}

fn merge_optional_field<T>(existing: &mut Option<T>, incoming: Option<T>, preserve_existing: bool) {
    if preserve_existing {
        if incoming.is_some() {
            *existing = incoming;
        }
    } else {
        *existing = incoming;
    }
}

fn merge_vec_field<T>(existing: &mut Vec<T>, incoming: Vec<T>, preserve_existing_when_empty: bool) {
    if preserve_existing_when_empty && incoming.is_empty() {
        return;
    }
    *existing = incoming;
}

fn merge_string_field(existing: &mut String, incoming: String, preserve_existing_when_empty: bool) {
    if preserve_existing_when_empty && incoming.is_empty() {
        return;
    }
    *existing = incoming;
}

fn merge_u32_field(existing: &mut u32, incoming: u32, preserve_existing_when_zero: bool) {
    if preserve_existing_when_zero && incoming == 0 && *existing != 0 {
        return;
    }
    *existing = incoming;
}

fn merge_u64_field(existing: &mut u64, incoming: u64, preserve_existing_when_zero: bool) {
    if preserve_existing_when_zero && incoming == 0 && *existing != 0 {
        return;
    }
    *existing = incoming;
}

fn merge_usize_field(existing: &mut usize, incoming: usize, preserve_existing_when_zero: bool) {
    if preserve_existing_when_zero && incoming == 0 && *existing != 0 {
        return;
    }
    *existing = incoming;
}

fn merge_goal_run(existing: &mut GoalRun, incoming: GoalRun, preserve_owner_metadata: bool) {
    let preserve_sparse_fields = preserve_owner_metadata && incoming.sparse_update;
    let older_page_request_cooldown_until_tick = existing
        .older_page_request_cooldown_until_tick
        .max(incoming.older_page_request_cooldown_until_tick);

    if preserve_sparse_fields {
        if existing.title.is_empty() {
            existing.title = incoming.title;
        }
    } else {
        existing.title = incoming.title;
    }
    merge_optional_field(
        &mut existing.thread_id,
        incoming.thread_id,
        preserve_sparse_fields,
    );
    merge_optional_field(
        &mut existing.session_id,
        incoming.session_id,
        preserve_sparse_fields,
    );
    merge_optional_field(
        &mut existing.status,
        incoming.status,
        preserve_sparse_fields,
    );
    merge_optional_field(
        &mut existing.current_step_title,
        incoming.current_step_title,
        preserve_sparse_fields,
    );
    merge_vec_field(
        &mut existing.launch_assignment_snapshot,
        incoming.launch_assignment_snapshot,
        preserve_sparse_fields,
    );
    merge_vec_field(
        &mut existing.runtime_assignment_list,
        incoming.runtime_assignment_list,
        preserve_sparse_fields,
    );
    merge_optional_field(
        &mut existing.root_thread_id,
        incoming.root_thread_id,
        preserve_sparse_fields,
    );
    merge_optional_field(
        &mut existing.active_thread_id,
        incoming.active_thread_id,
        preserve_sparse_fields,
    );
    merge_vec_field(
        &mut existing.execution_thread_ids,
        incoming.execution_thread_ids,
        preserve_sparse_fields,
    );
    if preserve_sparse_fields {
        existing.planner_owner_profile = incoming
            .planner_owner_profile
            .or(existing.planner_owner_profile.take());
        existing.current_step_owner_profile = incoming
            .current_step_owner_profile
            .or(existing.current_step_owner_profile.take());
    } else {
        existing.planner_owner_profile = incoming.planner_owner_profile;
        existing.current_step_owner_profile = incoming.current_step_owner_profile;
    }
    merge_u32_field(
        &mut existing.child_task_count,
        incoming.child_task_count,
        preserve_sparse_fields,
    );
    merge_u32_field(
        &mut existing.approval_count,
        incoming.approval_count,
        preserve_sparse_fields,
    );
    merge_optional_field(
        &mut existing.awaiting_approval_id,
        incoming.awaiting_approval_id,
        preserve_sparse_fields,
    );
    merge_optional_field(
        &mut existing.last_error,
        incoming.last_error,
        preserve_sparse_fields,
    );
    merge_string_field(&mut existing.goal, incoming.goal, preserve_sparse_fields);
    merge_usize_field(
        &mut existing.current_step_index,
        incoming.current_step_index,
        preserve_sparse_fields,
    );
    merge_optional_field(
        &mut existing.reflection_summary,
        incoming.reflection_summary,
        preserve_sparse_fields,
    );
    merge_vec_field(
        &mut existing.memory_updates,
        incoming.memory_updates,
        preserve_sparse_fields,
    );
    merge_optional_field(
        &mut existing.generated_skill_path,
        incoming.generated_skill_path,
        preserve_sparse_fields,
    );
    merge_vec_field(
        &mut existing.child_task_ids,
        incoming.child_task_ids,
        preserve_sparse_fields,
    );
    existing.dossier = merge_goal_run_dossier(
        existing.dossier.take(),
        incoming.dossier,
        preserve_sparse_fields,
    );
    merge_u64_field(
        &mut existing.created_at,
        incoming.created_at,
        preserve_sparse_fields,
    );
    merge_u64_field(
        &mut existing.updated_at,
        incoming.updated_at,
        preserve_sparse_fields,
    );
    if preserve_sparse_fields {
        existing.total_step_count = existing.total_step_count.max(incoming.total_step_count);
        existing.total_event_count = existing.total_event_count.max(incoming.total_event_count);
    } else {
        existing.total_step_count = incoming.total_step_count;
        existing.total_event_count = incoming.total_event_count;
    }

    let (loaded_step_start, loaded_step_end, steps) = merge_range_vec(
        existing.loaded_step_start,
        existing.loaded_step_end,
        &existing.steps,
        incoming.loaded_step_start,
        incoming.loaded_step_end,
        &incoming.steps,
    );
    existing.loaded_step_start = loaded_step_start;
    existing.loaded_step_end = loaded_step_end;
    existing.steps = steps;

    let (loaded_event_start, loaded_event_end, events) = merge_range_vec(
        existing.loaded_event_start,
        existing.loaded_event_end,
        &existing.events,
        incoming.loaded_event_start,
        incoming.loaded_event_end,
        &incoming.events,
    );
    existing.loaded_event_start = loaded_event_start;
    existing.loaded_event_end = loaded_event_end;
    existing.events = events;

    existing.older_page_pending = false;
    existing.older_page_request_cooldown_until_tick = older_page_request_cooldown_until_tick;
    existing.sparse_update = false;
}

fn merge_goal_run_dossier(
    existing: Option<GoalRunDossier>,
    incoming: Option<GoalRunDossier>,
    preserve_existing_when_missing: bool,
) -> Option<GoalRunDossier> {
    if !preserve_existing_when_missing {
        return incoming;
    }
    match (existing, incoming) {
        (None, dossier) | (dossier, None) => dossier,
        (Some(existing), Some(mut incoming)) => {
            if incoming.units.is_empty() {
                incoming.units = existing.units;
            }
            if incoming.projection_state.is_empty() {
                incoming.projection_state = existing.projection_state;
            }
            if incoming.latest_resume_decision.is_none() {
                incoming.latest_resume_decision = existing.latest_resume_decision;
            }
            if incoming.report.is_none() {
                incoming.report = existing.report;
            }
            if incoming.summary.is_none() {
                incoming.summary = existing.summary;
            }
            if incoming.projection_error.is_none() {
                incoming.projection_error = existing.projection_error;
            }
            Some(incoming)
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[path = "tests/task.rs"]
mod tests;
