use super::*;
use super::task_status_to_task_state::*;
use super::goal_step_todo_thread_ids_to_merge_usize_field::*;
use super::merge_goal_run_dossier::*;
impl TaskState {
    pub fn new() -> Self {
        Self {
            tasks: Vec::new(),
            tasks_revision: 0,
            preview_revision: 0,
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

    pub fn preview_revision(&self) -> u64 {
        self.preview_revision
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

    pub fn is_goal_thread_id(&self, thread_id: &str) -> bool {
        if thread_id.is_empty() {
            return false;
        }
        self.all_goal_thread_ids()
            .iter()
            .any(|candidate| candidate == thread_id)
    }

    pub fn all_goal_thread_ids(&self) -> Vec<String> {
        let mut thread_ids = Vec::new();
        let mut task_ids = Vec::new();

        for run in &self.goal_runs {
            for thread_id in run
                .active_thread_id
                .iter()
                .chain(run.root_thread_id.iter())
                .chain(run.thread_id.iter())
            {
                push_unique_id(&mut thread_ids, thread_id);
            }
            for thread_id in &run.execution_thread_ids {
                push_unique_id(&mut thread_ids, thread_id);
            }
            if let Some(goal_threads) = self.goal_thread_ids.get(&run.id) {
                for thread_id in goal_threads {
                    push_unique_id(&mut thread_ids, thread_id);
                }
            }
        }

        for goal_threads in self.goal_thread_ids.values() {
            for thread_id in goal_threads {
                push_unique_id(&mut thread_ids, thread_id);
            }
        }

        for task in self
            .tasks()
            .iter()
            .filter(|task| task.goal_run_id.as_deref().is_some_and(|id| !id.is_empty()))
        {
            push_unique_id(&mut task_ids, &task.id);
            if let Some(thread_id) = task.thread_id.as_deref() {
                push_unique_id(&mut thread_ids, thread_id);
            }
        }

        loop {
            let mut changed = false;
            for task in self.tasks() {
                let belongs_to_goal = task.goal_run_id.as_deref().is_some_and(|id| !id.is_empty())
                    || task
                        .parent_task_id
                        .as_deref()
                        .is_some_and(|parent_task_id| {
                            task_ids.iter().any(|id| id == parent_task_id)
                        })
                    || task
                        .parent_thread_id
                        .as_deref()
                        .is_some_and(|parent_thread_id| {
                            thread_ids.iter().any(|id| id == parent_thread_id)
                        });
                if !belongs_to_goal {
                    continue;
                }

                changed |= push_unique_id(&mut task_ids, &task.id);

                if let Some(thread_id) = task.thread_id.as_deref() {
                    changed |= push_unique_id(&mut thread_ids, thread_id);
                }
            }
            if !changed {
                break;
            }
        }

        thread_ids
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
                    self.preview_revision = self.preview_revision.wrapping_add(1);
                }
            }

            TaskAction::FilePreviewReceived(preview) => {
                self.file_previews.insert(preview.path.clone(), preview);
                self.preview_revision = self.preview_revision.wrapping_add(1);
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

