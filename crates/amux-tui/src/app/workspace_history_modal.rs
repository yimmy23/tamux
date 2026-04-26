use crossterm::event::{KeyCode, KeyModifiers};

use crate::state::{modal, sidebar};

impl super::TuiModel {
    pub(super) fn open_workspace_history_modal(&mut self, task_id: String) {
        let Some(task) = self.workspace.task_by_id(&task_id) else {
            self.status_line = "Workspace task not found".to_string();
            return;
        };
        let count = workspace_history_entries(task).len();
        self.pending_workspace_history_task_id = Some(task_id);
        self.modal.reduce(modal::ModalAction::Push(
            modal::ModalKind::WorkspaceTaskHistory,
        ));
        self.modal.set_picker_item_count(count);
        self.status_line = "Workspace task history".to_string();
    }

    pub(super) fn workspace_history_modal_body(&self) -> String {
        let Some(task_id) = self.pending_workspace_history_task_id.as_deref() else {
            return "No workspace task selected".to_string();
        };
        let Some(task) = self.workspace.task_by_id(task_id) else {
            return "Workspace task not found".to_string();
        };
        let entries = workspace_history_entries(task);
        if entries.is_empty() {
            return "No previous thread or goal runs".to_string();
        }
        let selected = self.modal.picker_cursor();
        let mut lines = vec![
            format!("Task: {}", task.title),
            "Previous runtimes, newest first:".to_string(),
            String::new(),
        ];
        for (index, entry) in entries.iter().enumerate() {
            let marker = if index == selected { ">" } else { " " };
            let target = entry
                .agent_task_id
                .as_deref()
                .map(|task_id| {
                    entry
                        .title
                        .as_deref()
                        .map(|title| format!("Task {task_id} · {title}"))
                        .unwrap_or_else(|| format!("Task {task_id}"))
                })
                .or_else(|| {
                    entry
                        .thread_id
                        .as_deref()
                        .map(|thread_id| format!("Thread {thread_id}"))
                })
                .or_else(|| {
                    entry
                        .goal_run_id
                        .as_deref()
                        .map(|goal_run_id| format!("Goal {goal_run_id}"))
                })
                .unwrap_or_else(|| "Runtime not recorded".to_string());
            let source = entry
                .source
                .as_deref()
                .map(|source| format!(" · {source}"))
                .unwrap_or_default();
            lines.push(format!("{marker} {}. {}{}", index + 1, target, source));
            if let Some(thread_id) = entry.thread_id.as_deref() {
                lines.push(format!("   thread: {thread_id}"));
            }
            if let Some(goal_run_id) = entry.goal_run_id.as_deref() {
                lines.push(format!("   goal: {goal_run_id}"));
            }
            if let Some(path) = entry.review_path.as_deref() {
                lines.push(format!("   review: {path}"));
            }
            if let Some(feedback) = entry.review_feedback.as_deref() {
                lines.push(format!("   {}", one_line(feedback, 82)));
            }
        }
        lines.push(String::new());
        lines.push("Enter open selected - Up/Down select - Esc close".to_string());
        lines.join("\n")
    }

    pub(super) fn handle_workspace_history_modal_key(
        &mut self,
        code: KeyCode,
        _modifiers: KeyModifiers,
    ) -> bool {
        match code {
            KeyCode::Esc => self.close_top_modal(),
            KeyCode::Enter => self.submit_workspace_history_modal(),
            _ => {}
        }
        false
    }

    pub(super) fn submit_workspace_history_modal(&mut self) {
        let Some(task_id) = self.pending_workspace_history_task_id.clone() else {
            self.close_top_modal();
            return;
        };
        let Some(task) = self.workspace.task_by_id(&task_id) else {
            self.close_top_modal();
            self.status_line = "Workspace task not found".to_string();
            return;
        };
        let entries = workspace_history_entries(task);
        let Some(entry) = entries.get(self.modal.picker_cursor()).cloned() else {
            self.status_line = "No workspace history entry selected".to_string();
            return;
        };
        self.close_top_modal();
        self.open_workspace_runtime_history_entry(entry);
    }

    fn open_workspace_runtime_history_entry(
        &mut self,
        entry: amux_protocol::WorkspaceTaskRuntimeHistoryEntry,
    ) {
        if let Some(goal_run_id) = entry.goal_run_id {
            self.open_sidebar_target(sidebar::SidebarItemTarget::GoalRun {
                goal_run_id: goal_run_id.clone(),
                step_id: None,
            });
            self.set_mission_control_return_to_workspace(true);
            self.status_line = format!("Opened historical workspace goal {goal_run_id}");
            return;
        }
        if let Some(thread_id) = entry.thread_id {
            self.open_thread_conversation(thread_id.clone());
            self.set_mission_control_return_to_workspace(true);
            self.status_line = format!("Opened historical workspace thread {thread_id}");
            return;
        }
        if let Some(task_id) = entry.agent_task_id {
            if let Some(thread_id) = self
                .tasks
                .task_by_id(&task_id)
                .and_then(|task| task.thread_id.clone())
            {
                self.open_thread_conversation(thread_id.clone());
                self.set_mission_control_return_to_workspace(true);
                self.status_line = format!("Opened workspace history task thread {thread_id}");
                return;
            }
            self.open_sidebar_target(sidebar::SidebarItemTarget::Task {
                task_id: task_id.clone(),
            });
            self.send_daemon_command(crate::state::DaemonCommand::ListTasks);
            self.set_mission_control_return_to_workspace(true);
            self.status_line = format!("Opened workspace history task {task_id}");
            return;
        }
        self.status_line = "Workspace history entry has no thread or goal".to_string();
    }
}

fn workspace_history_entries(
    task: &amux_protocol::WorkspaceTask,
) -> Vec<amux_protocol::WorkspaceTaskRuntimeHistoryEntry> {
    let mut entries = task.runtime_history.clone();
    if task.thread_id.is_none() && task.goal_run_id.is_none() {
        return entries;
    }
    let has_active_runtime = entries.iter().any(|entry| {
        (task.thread_id.is_some() && entry.thread_id == task.thread_id)
            || (task.goal_run_id.is_some() && entry.goal_run_id == task.goal_run_id)
    });
    if !has_active_runtime {
        entries.insert(
            0,
            amux_protocol::WorkspaceTaskRuntimeHistoryEntry {
                task_type: task.task_type.clone(),
                thread_id: task.thread_id.clone(),
                goal_run_id: task.goal_run_id.clone(),
                agent_task_id: None,
                source: Some("workspace_runtime".to_string()),
                title: Some(task.title.clone()),
                review_path: None,
                review_feedback: None,
                archived_at: task.started_at.unwrap_or(task.updated_at),
            },
        );
    }
    entries
}

fn one_line(value: &str, max_chars: usize) -> String {
    let compact = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if compact.chars().count() <= max_chars {
        return compact;
    }
    let mut result = compact
        .chars()
        .take(max_chars.saturating_sub(3))
        .collect::<String>();
    result.push_str("...");
    result
}
