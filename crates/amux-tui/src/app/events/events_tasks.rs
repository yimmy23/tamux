use super::*;

impl TuiModel {
    pub(in crate::app) fn handle_thread_list_event(
        &mut self,
        threads: Vec<crate::wire::AgentThread>,
    ) {
        let threads = threads
            .into_iter()
            .filter(|thread| !crate::wire::is_weles_thread(thread))
            .map(conversion::convert_thread)
            .collect();
        self.chat
            .reduce(chat::ChatAction::ThreadListReceived(threads));
    }

    pub(in crate::app) fn handle_thread_detail_event(&mut self, thread: crate::wire::AgentThread) {
        if crate::wire::is_weles_thread(&thread) {
            return;
        }
        self.anticipatory
            .reduce(crate::state::AnticipatoryAction::Clear);
        let thread_id = thread.id.clone();
        let should_select_thread = self.chat.active_thread_id().is_none();
        if self.chat.active_thread_id() == Some(thread_id.as_str()) {
            self.clear_chat_drag_selection();
        }
        self.chat.reduce(chat::ChatAction::ThreadDetailReceived(
            conversion::convert_thread(thread),
        ));
        if should_select_thread {
            self.chat
                .reduce(chat::ChatAction::SelectThread(thread_id.clone()));
        }
        self.send_daemon_command(DaemonCommand::RequestThreadTodos(thread_id.clone()));
        self.send_daemon_command(DaemonCommand::RequestThreadWorkContext(thread_id));
    }

    pub(in crate::app) fn handle_thread_created_event(&mut self, thread_id: String, title: String) {
        if Self::is_hidden_agent_thread(&thread_id, Some(title.as_str())) {
            return;
        }
        self.chat
            .reduce(chat::ChatAction::ThreadCreated { thread_id, title });
    }

    pub(in crate::app) fn handle_thread_reload_required_event(&mut self, thread_id: String) {
        self.send_daemon_command(DaemonCommand::RequestThread(thread_id.clone()));
        self.send_daemon_command(DaemonCommand::RequestThreadTodos(thread_id.clone()));
        self.send_daemon_command(DaemonCommand::RequestThreadWorkContext(thread_id));
        self.status_line = "Thread reloaded from daemon".to_string();
    }

    pub(in crate::app) fn handle_task_list_event(&mut self, tasks: Vec<crate::wire::AgentTask>) {
        let tasks = tasks.into_iter().map(conversion::convert_task).collect();
        self.tasks.reduce(task::TaskAction::TaskListReceived(tasks));
    }

    pub(in crate::app) fn handle_task_update_event(&mut self, task_item: crate::wire::AgentTask) {
        self.tasks
            .reduce(task::TaskAction::TaskUpdate(conversion::convert_task(
                task_item,
            )));
    }

    pub(in crate::app) fn handle_goal_run_list_event(&mut self, runs: Vec<crate::wire::GoalRun>) {
        let runs = runs.into_iter().map(conversion::convert_goal_run).collect();
        self.tasks
            .reduce(task::TaskAction::GoalRunListReceived(runs));
    }

    pub(in crate::app) fn handle_goal_run_started_event(&mut self, run: crate::wire::GoalRun) {
        let run = conversion::convert_goal_run(run);
        let target = sidebar::SidebarItemTarget::GoalRun {
            goal_run_id: run.id.clone(),
            step_id: None,
        };
        self.tasks.reduce(task::TaskAction::GoalRunUpdate(run));
        self.open_sidebar_target(target);
        self.status_line = "Goal run started".to_string();
    }

    pub(in crate::app) fn handle_goal_run_detail_event(&mut self, run: crate::wire::GoalRun) {
        self.tasks.reduce(task::TaskAction::GoalRunDetailReceived(
            conversion::convert_goal_run(run),
        ));
    }

    pub(in crate::app) fn handle_goal_run_update_event(&mut self, run: crate::wire::GoalRun) {
        self.tasks.reduce(task::TaskAction::GoalRunUpdate(
            conversion::convert_goal_run(run),
        ));
    }

    pub(in crate::app) fn handle_goal_run_checkpoints_event(
        &mut self,
        goal_run_id: String,
        checkpoints: Vec<crate::wire::CheckpointSummary>,
    ) {
        self.tasks
            .reduce(task::TaskAction::GoalRunCheckpointsReceived {
                goal_run_id,
                checkpoints: checkpoints
                    .into_iter()
                    .map(conversion::convert_checkpoint_summary)
                    .collect(),
            });
    }

    pub(in crate::app) fn handle_thread_todos_event(
        &mut self,
        thread_id: String,
        items: Vec<crate::wire::TodoItem>,
    ) {
        self.tasks.reduce(task::TaskAction::ThreadTodosReceived {
            thread_id,
            items: items.into_iter().map(conversion::convert_todo).collect(),
        });
    }

    pub(in crate::app) fn handle_work_context_event(
        &mut self,
        context: crate::wire::ThreadWorkContext,
    ) {
        self.tasks.reduce(task::TaskAction::WorkContextReceived(
            conversion::convert_work_context(context),
        ));
        self.ensure_task_view_preview();
    }

    pub(in crate::app) fn handle_git_diff_event(
        &mut self,
        repo_path: String,
        file_path: Option<String>,
        diff: String,
    ) {
        self.tasks.reduce(task::TaskAction::GitDiffReceived {
            repo_path,
            file_path,
            diff,
        });
    }

    pub(in crate::app) fn handle_file_preview_event(
        &mut self,
        path: String,
        content: String,
        truncated: bool,
        is_text: bool,
    ) {
        self.tasks
            .reduce(task::TaskAction::FilePreviewReceived(task::FilePreview {
                path,
                content,
                truncated,
                is_text,
            }));
    }
}
