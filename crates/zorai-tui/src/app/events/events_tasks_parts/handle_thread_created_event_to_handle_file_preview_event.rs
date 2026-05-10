use super::super::super::*;
use super::super::*;

impl TuiModel {
    pub(in crate::app) fn handle_thread_created_event(
        &mut self,
        thread_id: String,
        title: String,
        agent_name: Option<String>,
    ) {
        let agent_name_for_filter_match = agent_name.clone();
        let was_missing_runtime_thread = self.missing_runtime_thread_ids.remove(&thread_id);
        self.empty_hydrated_runtime_thread_ids.remove(&thread_id);
        let pending_local_activity = self
            .chat
            .active_thread_id()
            .filter(|active_thread_id| active_thread_id.starts_with("local-"))
            .and_then(|active_thread_id| {
                self.thread_agent_activity
                    .remove(active_thread_id)
                    .map(|activity| (active_thread_id.to_string(), activity))
            });
        let migrated_bootstrap_activity = pending_local_activity
            .as_ref()
            .is_some_and(|(_, activity)| activity == "thinking");
        let migrated_pending_prompt_response = self
            .chat
            .active_thread_id()
            .filter(|active_thread_id| active_thread_id.starts_with("local-"))
            .is_some_and(|active_thread_id| {
                self.pending_prompt_response_threads
                    .remove(active_thread_id)
            });
        if Self::is_hidden_agent_thread(&thread_id, Some(title.as_str())) {
            return;
        }
        let is_internal = Self::is_internal_agent_thread(&thread_id, Some(title.as_str()));
        if is_internal {
            self.chat.reduce(chat::ChatAction::ThreadDetailReceived(
                crate::state::chat::AgentThread {
                    id: thread_id.clone(),
                    agent_name,
                    title,
                    ..Default::default()
                },
            ));
            self.sync_open_thread_picker();
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
                    id: thread_id.clone(),
                    agent_name,
                    title,
                    ..Default::default()
                },
            ));
        }
        if let Some((_local_thread_id, activity)) = pending_local_activity {
            self.thread_agent_activity
                .entry(thread_id.clone())
                .or_insert(activity);
        }
        if migrated_bootstrap_activity {
            self.mark_bootstrap_pending_activity_thread(thread_id.clone());
        }
        if migrated_pending_prompt_response {
            self.mark_pending_prompt_response_thread(thread_id.clone());
        }
        self.sync_open_thread_picker();
        self.sync_pending_approvals_from_tasks();

        let active_tab = self.modal.thread_picker_tab();
        if let Some(filter) = active_tab.agent_filter() {
            let matches_active = match agent_name_for_filter_match.as_deref() {
                Some(name) => agent_name_matches_filter(name, &filter),
                None => filter.eq_ignore_ascii_case(zorai_protocol::AGENT_HANDLE_SVAROG),
            };
            if matches_active {
                self.send_daemon_command(crate::state::DaemonCommand::RefreshThreadsForAgent {
                    agent_filter: Some(filter),
                });
            }
        }

        if was_missing_runtime_thread && self.chat.active_thread_id() == Some(thread_id.as_str()) {
            self.request_latest_thread_page(thread_id, true);
        }
    }

    pub(in crate::app) fn handle_thread_reload_required_event(&mut self, thread_id: String) {
        if Self::is_hidden_agent_thread(&thread_id, None)
            || self.should_ignore_internal_thread_activity(&thread_id)
        {
            return;
        }
        let is_active_thread = self.chat.active_thread_id() == Some(thread_id.as_str());
        let is_header_thread = self.thread_drives_current_header(&thread_id);
        if !is_active_thread && !is_header_thread {
            return;
        }
        if self
            .pending_local_message_delete_reload_suppression
            .contains_key(&thread_id)
        {
            let remaining_suppressed_reloads = {
                let remaining = self
                    .pending_local_message_delete_reload_suppression
                    .get_mut(&thread_id)
                    .expect("checked delete reload suppression entry");
                *remaining = remaining.saturating_sub(1);
                *remaining
            };
            let confirmed_local_delete = self
                .chat
                .confirm_local_deleted_message_for_thread(&thread_id);
            tracing::info!(
                thread_id = %thread_id,
                confirmed_local_delete,
                remaining_suppressed_reloads,
                "confirmed optimistic message delete without reloading thread"
            );
            if remaining_suppressed_reloads == 0 {
                self.pending_local_message_delete_reload_suppression
                    .remove(&thread_id);
            }
            self.status_line = "Message deleted".to_string();
            return;
        }
        self.empty_hydrated_runtime_thread_ids.remove(&thread_id);
        self.chat.reduce(chat::ChatAction::InvalidateContextWindow {
            thread_id: thread_id.clone(),
        });
        if is_active_thread {
            let has_live_text_stream = !self.chat.streaming_content().is_empty()
                || !self.chat.streaming_reasoning().is_empty();
            if !has_live_text_stream {
                self.chat.reduce(chat::ChatAction::ResetStreaming);
            }
            if !has_live_text_stream
                && !self.should_preserve_pending_thinking_activity_on_reload(thread_id.as_str())
            {
                self.clear_agent_activity_for(Some(thread_id.as_str()));
            }
            self.clear_pending_stop();
        }
        self.request_authoritative_thread_refresh(thread_id.clone(), true);
        self.send_daemon_command(DaemonCommand::RequestThreadTodos(thread_id.clone()));
        self.send_daemon_command(DaemonCommand::RequestThreadWorkContext(thread_id));
        self.status_line = "Thread reloaded from daemon".to_string();
    }

    fn thread_drives_current_header(&self, thread_id: &str) -> bool {
        let MainPaneView::Task(SidebarItemTarget::GoalRun { goal_run_id, .. }) =
            &self.main_pane_view
        else {
            return false;
        };
        let Some(run) = self.tasks.goal_run_by_id(goal_run_id) else {
            return false;
        };
        [
            run.active_thread_id.as_deref(),
            run.root_thread_id.as_deref(),
            run.thread_id.as_deref(),
        ]
        .into_iter()
        .flatten()
        .any(|candidate| candidate == thread_id)
    }

    pub(in crate::app) fn handle_task_list_event(&mut self, tasks: Vec<crate::wire::AgentTask>) {
        let previous_tasks = self.tasks.tasks().to_vec();
        let tasks: Vec<_> = tasks.into_iter().map(conversion::convert_task).collect();
        self.tasks
            .reduce(task::TaskAction::TaskListReceived(tasks.clone()));
        self.reconcile_goal_sidebar_selection_for_active_goal_pane();
        self.sync_goal_workspace_selection_for_active_goal_pane();
        self.clamp_detail_view_scroll();
        self.clear_replaced_task_approvals(&previous_tasks, &tasks);
        self.sync_pending_approvals_from_tasks();
        self.sync_contextual_approval_overlay();
    }

    pub(in crate::app) fn handle_task_update_event(&mut self, task_item: crate::wire::AgentTask) {
        let converted = conversion::convert_task(task_item);
        let previous_approval_id = self
            .tasks
            .task_by_id(converted.id.as_str())
            .and_then(|task| task.awaiting_approval_id.clone());
        self.tasks
            .reduce(task::TaskAction::TaskUpdate(converted.clone()));
        self.reconcile_goal_sidebar_selection_for_active_goal_pane();
        self.sync_goal_workspace_selection_for_active_goal_pane();
        self.clamp_detail_view_scroll();
        if let Some(previous_approval_id) = previous_approval_id.filter(|approval_id| {
            Some(approval_id.as_str()) != converted.awaiting_approval_id.as_deref()
        }) {
            self.approval
                .reduce(crate::state::ApprovalAction::ClearResolved(
                    previous_approval_id,
                ));
        }
        self.upsert_task_backed_approval(&converted);
        self.sync_contextual_approval_overlay();
    }

    pub(in crate::app) fn handle_goal_run_list_event(&mut self, runs: Vec<crate::wire::GoalRun>) {
        let previous_runs = self.tasks.goal_runs().to_vec();
        let runs: Vec<_> = runs.into_iter().map(conversion::convert_goal_run).collect();
        let present_goal_run_ids = runs
            .iter()
            .map(|run| run.id.clone())
            .collect::<std::collections::HashSet<_>>();
        self.tasks
            .reduce(task::TaskAction::GoalRunListReceived(runs.clone()));
        self.clear_replaced_goal_run_approvals(&previous_runs, &runs);
        self.sync_pending_approvals_from_goal_runs();
        self.pending_goal_hydration_refreshes
            .retain(|goal_run_id| present_goal_run_ids.contains(goal_run_id));
        self.reconcile_goal_sidebar_selection_for_active_goal_pane();
        self.sync_goal_workspace_selection_for_active_goal_pane();
        if self.modal.top() == Some(modal::ModalKind::GoalPicker) {
            self.sync_goal_picker_item_count();
        }
        self.clamp_detail_view_scroll();
        self.sync_contextual_approval_overlay();
    }

    pub(in crate::app) fn handle_goal_run_started_event(&mut self, run: crate::wire::GoalRun) {
        let run = conversion::convert_goal_run(run);
        let goal_run_id = run.id.clone();
        let target = sidebar::SidebarItemTarget::GoalRun {
            goal_run_id: goal_run_id.clone(),
            step_id: None,
        };
        self.tasks
            .reduce(task::TaskAction::GoalRunUpdate(run.clone()));
        self.upsert_goal_run_backed_approval(&run);
        self.open_sidebar_target(target);
        self.request_authoritative_goal_run_refresh(goal_run_id.clone());
        self.schedule_goal_hydration_refresh(goal_run_id);
        self.status_line = "Goal run started".to_string();
    }

    pub(in crate::app) fn handle_goal_run_detail_event(&mut self, run: crate::wire::GoalRun) {
        let previous_approval_id = self
            .tasks
            .goal_run_by_id(&run.id)
            .and_then(|goal_run| goal_run.awaiting_approval_id.clone());
        let should_preserve_prepend_anchor = matches!(
            &self.main_pane_view,
            MainPaneView::Task(sidebar::SidebarItemTarget::GoalRun { goal_run_id, .. })
                if goal_run_id == &run.id
        ) && self.task_view_scroll <= 3
            && self.tasks.goal_run_by_id(&run.id).is_some_and(|existing| {
                (run.loaded_step_end == existing.loaded_step_start
                    && run.loaded_step_start < run.loaded_step_end)
                    || (run.loaded_event_end == existing.loaded_event_start
                        && run.loaded_event_start < run.loaded_event_end)
            });
        let before_max_scroll = if should_preserve_prepend_anchor {
            self.current_detail_view_max_scroll()
        } else {
            0
        };
        let converted = conversion::convert_goal_run(run);
        let goal_run_id = converted.id.clone();
        self.tasks
            .reduce(task::TaskAction::GoalRunDetailReceived(converted));
        if let Some(previous_approval_id) = previous_approval_id.filter(|approval_id| {
            self.tasks
                .goal_run_by_id(&goal_run_id)
                .and_then(|goal_run| goal_run.awaiting_approval_id.as_deref())
                != Some(approval_id.as_str())
        }) {
            self.approval
                .reduce(crate::state::ApprovalAction::ClearResolved(
                    previous_approval_id,
                ));
        }
        if let Some(goal_run) = self.tasks.goal_run_by_id(&goal_run_id).cloned() {
            self.upsert_goal_run_backed_approval(&goal_run);
        }
        self.clear_goal_hydration_refresh(&goal_run_id);
        if should_preserve_prepend_anchor {
            let after_max_scroll = self.current_detail_view_max_scroll();
            self.task_view_scroll = self
                .task_view_scroll
                .saturating_add(after_max_scroll.saturating_sub(before_max_scroll));
        }
        self.reconcile_goal_sidebar_selection_for_active_goal_pane();
        self.sync_goal_workspace_selection_for_active_goal_pane();
        self.clamp_detail_view_scroll();
        self.sync_contextual_approval_overlay();
    }

    pub(in crate::app) fn handle_goal_run_update_event(&mut self, run: crate::wire::GoalRun) {
        let previous_approval_id = self
            .tasks
            .goal_run_by_id(&run.id)
            .and_then(|goal_run| goal_run.awaiting_approval_id.clone());
        let run = conversion::convert_goal_run(run);
        let active_goal_run_id = match &self.main_pane_view {
            MainPaneView::Task(sidebar::SidebarItemTarget::GoalRun { goal_run_id, .. }) => {
                Some(goal_run_id.clone())
            }
            _ => None,
        };
        self.tasks
            .reduce(task::TaskAction::GoalRunUpdate(run.clone()));
        if let Some(previous_approval_id) = previous_approval_id.filter(|approval_id| {
            self.tasks
                .goal_run_by_id(&run.id)
                .and_then(|goal_run| goal_run.awaiting_approval_id.as_deref())
                != Some(approval_id.as_str())
        }) {
            self.approval
                .reduce(crate::state::ApprovalAction::ClearResolved(
                    previous_approval_id,
                ));
        }
        self.upsert_goal_run_backed_approval(&run);
        if active_goal_run_id.as_deref() == Some(run.id.as_str()) {
            self.schedule_goal_hydration_refresh(run.id.clone());
        }
        self.reconcile_goal_sidebar_selection_for_active_goal_pane();
        self.sync_goal_workspace_selection_for_active_goal_pane();
        self.clamp_detail_view_scroll();
        self.sync_contextual_approval_overlay();
    }

    pub(in crate::app) fn handle_goal_run_checkpoints_event(
        &mut self,
        goal_run_id: String,
        checkpoints: Vec<crate::wire::CheckpointSummary>,
    ) {
        self.tasks
            .reduce(task::TaskAction::GoalRunCheckpointsReceived {
                goal_run_id: goal_run_id.clone(),
                checkpoints: checkpoints
                    .into_iter()
                    .map(conversion::convert_checkpoint_summary)
                    .collect(),
            });
        self.clear_goal_hydration_refresh(&goal_run_id);
        self.reconcile_goal_sidebar_selection_for_active_goal_pane();
        self.sync_goal_workspace_selection_for_active_goal_pane();
        self.clamp_detail_view_scroll();
    }

    pub(in crate::app) fn handle_thread_todos_event(
        &mut self,
        thread_id: String,
        goal_run_id: Option<String>,
        step_index: Option<usize>,
        items: Vec<crate::wire::TodoItem>,
    ) {
        let goal_run_binding = goal_run_id.clone();
        let active_goal_run_id = match &self.main_pane_view {
            MainPaneView::Task(sidebar::SidebarItemTarget::GoalRun { goal_run_id, .. }) => {
                Some(goal_run_id.clone())
            }
            _ => None,
        };
        self.tasks.reduce(task::TaskAction::ThreadTodosReceived {
            thread_id: thread_id.clone(),
            goal_run_id,
            step_index,
            items: items.into_iter().map(conversion::convert_todo).collect(),
        });
        if let Some(goal_run_id) = active_goal_run_id.filter(|active_goal_run_id| {
            goal_run_binding.as_deref() == Some(active_goal_run_id.as_str())
        }) {
            self.schedule_goal_hydration_refresh(goal_run_id);
        }
        self.clamp_detail_view_scroll();
    }

    pub(in crate::app) fn handle_work_context_event(
        &mut self,
        context: crate::wire::ThreadWorkContext,
    ) {
        let active_goal_run_id = match &self.main_pane_view {
            MainPaneView::Task(sidebar::SidebarItemTarget::GoalRun { goal_run_id, .. }) => {
                Some(goal_run_id.clone())
            }
            _ => None,
        };
        let thread_id = context.thread_id.clone();
        self.tasks.reduce(task::TaskAction::WorkContextReceived(
            conversion::convert_work_context(context),
        ));
        if let Some(goal_run_id) = active_goal_run_id.filter(|goal_run_id| {
            self.tasks
                .thread_belongs_to_goal_run(goal_run_id, &thread_id)
        }) {
            self.schedule_goal_hydration_refresh(goal_run_id);
        }
        self.reconcile_goal_sidebar_selection_for_active_goal_pane();
        self.sync_goal_workspace_selection_for_active_goal_pane();
        self.ensure_task_view_preview();
        self.clamp_detail_view_scroll();
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
        self.clamp_detail_view_scroll();
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
        self.clamp_detail_view_scroll();
    }
}

/// Whether a `ThreadCreated`'s `agent_name` field belongs to the same bucket
/// as the active picker filter. The match is intentionally lenient because
/// the daemon's persisted thread filter (`persisted_thread_agent_name_filter`)
/// resolves a single agent handle (e.g. "svarog") to a fan-out of canonical
/// IDs, public aliases, legacy aliases, and (for the main agent) treats
/// empty/null `agent_name` as the same bucket. A precise client-side mirror
/// would duplicate that whole resolver, so we match conservatively: any of
/// the obvious case-insensitive equivalents triggers a refresh, and the
/// daemon then returns the authoritative list.
fn agent_name_matches_filter(thread_agent_name: &str, filter: &str) -> bool {
    let thread = thread_agent_name.trim();
    let filter = filter.trim();
    if thread.eq_ignore_ascii_case(filter) {
        return true;
    }
    let svarog_aliases = [
        zorai_protocol::AGENT_HANDLE_SVAROG,
        zorai_protocol::AGENT_NAME_SWAROG,
        zorai_protocol::AGENT_ID_SWAROG,
    ];
    let filter_is_svarog = svarog_aliases
        .iter()
        .any(|alias| filter.eq_ignore_ascii_case(alias));
    let thread_is_svarog = svarog_aliases
        .iter()
        .any(|alias| thread.eq_ignore_ascii_case(alias));
    if filter_is_svarog && thread_is_svarog {
        return true;
    }
    let rarog_aliases = [
        zorai_protocol::AGENT_NAME_RAROG,
        zorai_protocol::AGENT_ID_RAROG,
    ];
    let filter_is_rarog = filter.eq_ignore_ascii_case("rarog")
        || rarog_aliases.iter().any(|a| filter.eq_ignore_ascii_case(a));
    let thread_is_rarog = thread.eq_ignore_ascii_case("rarog")
        || rarog_aliases.iter().any(|a| thread.eq_ignore_ascii_case(a));
    if filter_is_rarog && thread_is_rarog {
        return true;
    }
    false
}
