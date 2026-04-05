use super::*;

impl TuiModel {
    fn approval_rationale_for_thread(&self, thread_id: Option<&str>) -> Option<String> {
        let prefix = "Policy escalation requested operator guidance:";
        let thread_id = thread_id?;
        self.chat
            .threads()
            .iter()
            .find(|thread| thread.id == thread_id)
            .and_then(|thread| {
                thread.messages.iter().rev().find_map(|message| {
                    (message.role == chat::MessageRole::System)
                        .then_some(message.content.as_str())
                        .and_then(|content| content.strip_prefix(prefix))
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                        .map(str::to_string)
                })
            })
    }

    pub(in crate::app) fn thread_title_for_id(&self, thread_id: Option<&str>) -> Option<String> {
        let thread_id = thread_id?;
        self.chat
            .threads()
            .iter()
            .find(|thread| thread.id == thread_id)
            .map(|thread| thread.title.clone())
    }

    fn upsert_task_backed_approval(&mut self, task: &task::AgentTask) {
        let Some(approval_id) = task.awaiting_approval_id.as_deref() else {
            return;
        };
        let existing = self.approval.approval_by_id(approval_id).cloned();
        let task_description =
            (!task.description.trim().is_empty()).then(|| task.description.clone());
        let task_command = task
            .command
            .clone()
            .filter(|command| !command.trim().is_empty());
        let thread_rationale = self.approval_rationale_for_thread(task.thread_id.as_deref());
        self.approval
            .reduce(crate::state::ApprovalAction::ApprovalRequired(
                crate::state::PendingApproval {
                    approval_id: approval_id.to_string(),
                    task_id: task.id.clone(),
                    task_title: Some(task.title.clone()).filter(|title| !title.trim().is_empty()),
                    thread_id: task.thread_id.clone(),
                    thread_title: existing
                        .as_ref()
                        .and_then(|approval| approval.thread_title.clone())
                        .or_else(|| self.thread_title_for_id(task.thread_id.as_deref())),
                    workspace_id: existing
                        .as_ref()
                        .and_then(|approval| approval.workspace_id.clone())
                        .or_else(|| self.current_workspace_id().map(str::to_string)),
                    rationale: existing
                        .as_ref()
                        .and_then(|approval| approval.rationale.clone())
                        .or(thread_rationale)
                        .or(task_description),
                    reasons: existing
                        .as_ref()
                        .map(|approval| approval.reasons.clone())
                        .unwrap_or_default(),
                    command: existing
                        .as_ref()
                        .and_then(|approval| {
                            (approval.command != "Awaiting approval details from daemon")
                                .then(|| approval.command.clone())
                        })
                        .or(task_command)
                        .unwrap_or_else(|| "Awaiting approval details from daemon".to_string()),
                    risk_level: existing
                        .as_ref()
                        .map(|approval| approval.risk_level)
                        .unwrap_or(crate::state::RiskLevel::Medium),
                    blast_radius: existing
                        .as_ref()
                        .map(|approval| approval.blast_radius.clone())
                        .or_else(|| task.blocked_reason.clone())
                        .unwrap_or_else(|| "task".to_string()),
                    received_at: existing
                        .as_ref()
                        .map(|approval| approval.received_at)
                        .unwrap_or_else(|| Self::current_unix_ms().max(0) as u64),
                    seen_at: existing.as_ref().and_then(|approval| approval.seen_at),
                },
            ));
    }

    fn sync_pending_approvals_from_tasks(&mut self) {
        let mut active_ids = std::collections::HashSet::new();
        let tasks = self.tasks.tasks().to_vec();
        for task in &tasks {
            if let Some(approval_id) = task.awaiting_approval_id.as_deref() {
                active_ids.insert(approval_id.to_string());
                self.upsert_task_backed_approval(task);
            }
        }

        let stale_ids: Vec<String> = self
            .approval
            .pending_approvals()
            .iter()
            .filter(|approval| !active_ids.contains(&approval.approval_id))
            .map(|approval| approval.approval_id.clone())
            .collect();
        for approval_id in stale_ids {
            self.approval
                .reduce(crate::state::ApprovalAction::ClearResolved(approval_id));
        }
    }

    pub(in crate::app) fn handle_thread_list_event(
        &mut self,
        threads: Vec<crate::wire::AgentThread>,
    ) {
        let threads = threads
            .into_iter()
            .filter(|thread| {
                !crate::wire::is_weles_thread(thread)
                    && !thread.id.starts_with("handoff:")
                    && !thread.title.trim().to_ascii_lowercase().starts_with("handoff ")
            })
            .map(conversion::convert_thread)
            .collect();
        self.chat
            .reduce(chat::ChatAction::ThreadListReceived(threads));
        self.sync_pending_approvals_from_tasks();
    }

    pub(in crate::app) fn handle_thread_detail_event(&mut self, thread: crate::wire::AgentThread) {
        if crate::wire::is_weles_thread(&thread)
            || thread.id.starts_with("handoff:")
            || thread.title.trim().to_ascii_lowercase().starts_with("handoff ")
        {
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
        self.sync_pending_approvals_from_tasks();
        self.send_daemon_command(DaemonCommand::RequestThreadTodos(thread_id.clone()));
        self.send_daemon_command(DaemonCommand::RequestThreadWorkContext(thread_id));
    }

    pub(in crate::app) fn handle_thread_created_event(
        &mut self,
        thread_id: String,
        title: String,
        agent_name: Option<String>,
    ) {
        if Self::is_hidden_agent_thread(&thread_id, Some(title.as_str())) {
            return;
        }
        let is_internal = Self::is_internal_agent_thread(&thread_id, Some(title.as_str()));
        if is_internal {
            self.chat.reduce(chat::ChatAction::ThreadDetailReceived(
                crate::state::chat::AgentThread {
                    id: thread_id,
                    agent_name,
                    title,
                    ..Default::default()
                },
            ));
            self.sync_pending_approvals_from_tasks();
            return;
        }
        self.chat.reduce(chat::ChatAction::ThreadCreated {
            thread_id: thread_id.clone(),
            title: title.clone(),
        });
        if agent_name.is_some() {
            self.chat.reduce(chat::ChatAction::ThreadDetailReceived(
                crate::state::chat::AgentThread {
                    id: thread_id,
                    agent_name,
                    title,
                    ..Default::default()
                },
            ));
        }
        self.sync_pending_approvals_from_tasks();
    }

    pub(in crate::app) fn handle_thread_reload_required_event(&mut self, thread_id: String) {
        if Self::is_hidden_agent_thread(&thread_id, None)
            || self.should_ignore_internal_thread_activity(&thread_id)
        {
            return;
        }
        self.send_daemon_command(DaemonCommand::RequestThread(thread_id.clone()));
        self.send_daemon_command(DaemonCommand::RequestThreadTodos(thread_id.clone()));
        self.send_daemon_command(DaemonCommand::RequestThreadWorkContext(thread_id));
        self.status_line = "Thread reloaded from daemon".to_string();
    }

    pub(in crate::app) fn handle_task_list_event(&mut self, tasks: Vec<crate::wire::AgentTask>) {
        let tasks = tasks.into_iter().map(conversion::convert_task).collect();
        self.tasks.reduce(task::TaskAction::TaskListReceived(tasks));
        self.sync_pending_approvals_from_tasks();
    }

    pub(in crate::app) fn handle_task_update_event(&mut self, task_item: crate::wire::AgentTask) {
        let converted = conversion::convert_task(task_item);
        let previous_approval_id = self
            .tasks
            .task_by_id(converted.id.as_str())
            .and_then(|task| task.awaiting_approval_id.clone());
        self.tasks
            .reduce(task::TaskAction::TaskUpdate(converted.clone()));
        if let Some(previous_approval_id) = previous_approval_id.filter(|approval_id| {
            Some(approval_id.as_str()) != converted.awaiting_approval_id.as_deref()
        }) {
            self.approval
                .reduce(crate::state::ApprovalAction::ClearResolved(
                    previous_approval_id,
                ));
        }
        self.upsert_task_backed_approval(&converted);
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
