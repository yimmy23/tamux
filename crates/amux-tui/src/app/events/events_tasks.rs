use super::*;

impl TuiModel {
    pub(in crate::app) fn is_placeholder_goal_run_detail(
        &self,
        run: &crate::wire::GoalRun,
    ) -> bool {
        run.title.is_empty()
            && run.thread_id.is_none()
            && run.session_id.is_none()
            && run.status.is_none()
            && run.current_step_title.is_none()
            && run.planner_owner_profile.is_none()
            && run.current_step_owner_profile.is_none()
            && run.child_task_count == 0
            && run.approval_count == 0
            && run.awaiting_approval_id.is_none()
            && run.last_error.is_none()
            && run.goal.is_empty()
            && run.current_step_index == 0
            && run.reflection_summary.is_none()
            && run.memory_updates.is_empty()
            && run.generated_skill_path.is_none()
            && run.child_task_ids.is_empty()
            && run.loaded_step_start == 0
            && run.loaded_step_end == 0
            && run.total_step_count == 0
            && run.loaded_event_start == 0
            && run.loaded_event_end == 0
            && run.total_event_count == 0
            && run.steps.is_empty()
            && run.events.is_empty()
            && run.dossier.is_none()
    }

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

    fn upsert_goal_run_backed_approval(&mut self, run: &task::GoalRun) {
        let Some(approval_id) = run.awaiting_approval_id.as_deref() else {
            return;
        };
        let existing = self.approval.approval_by_id(approval_id).cloned();
        let thread_rationale = self.approval_rationale_for_thread(run.thread_id.as_deref());
        let fallback_command = run
            .current_step_title
            .as_ref()
            .map(|title| format!("review goal step: {title}"))
            .unwrap_or_else(|| "review goal approval".to_string());
        let fallback_blast_radius = run
            .current_step_title
            .clone()
            .unwrap_or_else(|| "goal run".to_string());

        self.approval
            .reduce(crate::state::ApprovalAction::ApprovalRequired(
                crate::state::PendingApproval {
                    approval_id: approval_id.to_string(),
                    task_id: existing
                        .as_ref()
                        .map(|approval| approval.task_id.clone())
                        .filter(|task_id| !task_id.trim().is_empty())
                        .or_else(|| run.child_task_ids.first().cloned())
                        .unwrap_or_else(|| run.id.clone()),
                    task_title: existing
                        .as_ref()
                        .and_then(|approval| approval.task_title.clone())
                        .or_else(|| {
                            Some(run.title.clone()).filter(|title| !title.trim().is_empty())
                        }),
                    thread_id: run.thread_id.clone(),
                    thread_title: existing
                        .as_ref()
                        .and_then(|approval| approval.thread_title.clone())
                        .or_else(|| self.thread_title_for_id(run.thread_id.as_deref())),
                    workspace_id: existing
                        .as_ref()
                        .and_then(|approval| approval.workspace_id.clone())
                        .or_else(|| self.current_workspace_id().map(str::to_string)),
                    rationale: existing
                        .as_ref()
                        .and_then(|approval| approval.rationale.clone())
                        .or(thread_rationale),
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
                        .unwrap_or(fallback_command),
                    risk_level: existing
                        .as_ref()
                        .map(|approval| approval.risk_level)
                        .unwrap_or(crate::state::RiskLevel::Medium),
                    blast_radius: existing
                        .as_ref()
                        .map(|approval| approval.blast_radius.clone())
                        .unwrap_or(fallback_blast_radius),
                    received_at: existing
                        .as_ref()
                        .map(|approval| approval.received_at)
                        .unwrap_or_else(|| Self::current_unix_ms().max(0) as u64),
                    seen_at: existing.as_ref().and_then(|approval| approval.seen_at),
                },
            ));
    }

    fn sync_pending_approvals_from_tasks(&mut self) {
        let tasks = self.tasks.tasks().to_vec();
        for task in &tasks {
            // Task snapshots hydrate approval details, but absence is not authoritative because
            // the daemon can cap or omit tasks that still have a live approval event.
            if task.awaiting_approval_id.is_some() {
                self.upsert_task_backed_approval(task);
            }
        }
    }

    fn sync_pending_approvals_from_goal_runs(&mut self) {
        let goal_runs = self.tasks.goal_runs().to_vec();
        for goal_run in &goal_runs {
            if goal_run.awaiting_approval_id.is_some() {
                self.upsert_goal_run_backed_approval(goal_run);
            }
        }
    }

    fn clear_replaced_task_approvals(
        &mut self,
        previous_tasks: &[task::AgentTask],
        next_tasks: &[task::AgentTask],
    ) {
        for previous_task in previous_tasks {
            let Some(previous_approval_id) = previous_task.awaiting_approval_id.as_deref() else {
                continue;
            };
            let Some(next_task) = next_tasks.iter().find(|task| task.id == previous_task.id) else {
                continue;
            };
            if next_task.awaiting_approval_id.as_deref() != Some(previous_approval_id) {
                self.approval
                    .reduce(crate::state::ApprovalAction::ClearResolved(
                        previous_approval_id.to_string(),
                    ));
            }
        }
    }

    fn clear_replaced_goal_run_approvals(
        &mut self,
        previous_runs: &[task::GoalRun],
        next_runs: &[task::GoalRun],
    ) {
        for previous_run in previous_runs {
            let Some(previous_approval_id) = previous_run.awaiting_approval_id.as_deref() else {
                continue;
            };
            let Some(next_run) = next_runs.iter().find(|run| run.id == previous_run.id) else {
                continue;
            };
            if next_run.awaiting_approval_id.as_deref() != Some(previous_approval_id) {
                self.approval
                    .reduce(crate::state::ApprovalAction::ClearResolved(
                        previous_approval_id.to_string(),
                    ));
            }
        }
    }

    pub(in crate::app) fn handle_thread_list_event(
        &mut self,
        threads: Vec<crate::wire::AgentThread>,
    ) {
        let active_thread_id = self.chat.active_thread_id().map(str::to_string);
        let pending_loading_thread_id = self.thread_loading_id.clone();
        let should_refresh_active_thread = active_thread_id.as_ref().is_some_and(|thread_id| {
            threads.iter().any(|thread| {
                thread.id == *thread_id
                    && !thread.id.starts_with("handoff:")
                    && !thread
                        .title
                        .trim()
                        .to_ascii_lowercase()
                        .starts_with("handoff ")
            })
        });
        let threads = threads
            .into_iter()
            .filter(|thread| {
                let is_internal =
                    Self::is_internal_agent_thread(&thread.id, Some(thread.title.as_str()));
                (is_internal || !crate::wire::is_weles_thread(thread))
                    && !thread.id.starts_with("handoff:")
                    && !thread
                        .title
                        .trim()
                        .to_ascii_lowercase()
                        .starts_with("handoff ")
            })
            .map(conversion::convert_thread)
            .collect();
        self.chat
            .reduce(chat::ChatAction::ThreadListReceived(threads));
        self.sync_open_thread_picker();
        self.sync_pending_approvals_from_tasks();
        if self.fallback_pending_reconnect_restore() {
            return;
        }
        if let Some(thread_id) = active_thread_id
            .as_ref()
            .filter(|_| should_refresh_active_thread)
            .cloned()
        {
            self.request_latest_thread_page(thread_id, true);
        }
        if self.chat.active_thread().is_none() {
            if active_thread_id.is_some()
                && active_thread_id == pending_loading_thread_id
                && self.chat.active_thread_id().is_none()
            {
                if let Some(thread_id) = pending_loading_thread_id {
                    self.chat
                        .reduce(chat::ChatAction::SelectThread(thread_id.clone()));
                    self.thread_loading_id = Some(thread_id);
                }
            } else {
                self.thread_loading_id = None;
            }
        }
    }

    pub(in crate::app) fn handle_thread_detail_event(&mut self, thread: crate::wire::AgentThread) {
        let is_internal = Self::is_internal_agent_thread(&thread.id, Some(thread.title.as_str()));
        if (!is_internal && crate::wire::is_weles_thread(&thread))
            || thread.id.starts_with("handoff:")
            || thread
                .title
                .trim()
                .to_ascii_lowercase()
                .starts_with("handoff ")
        {
            return;
        }
        self.anticipatory
            .reduce(crate::state::AnticipatoryAction::Clear);
        let live_suggestion_ids = thread
            .queued_participant_suggestions
            .iter()
            .map(|suggestion| suggestion.id.clone())
            .collect::<std::collections::HashSet<_>>();
        self.hidden_auto_response_suggestion_ids
            .retain(|suggestion_id| live_suggestion_ids.contains(suggestion_id));
        let thread_id = thread.id.clone();
        let should_preserve_prepend_anchor = self.chat.active_thread().is_some_and(|existing| {
            let incoming_total = thread.total_message_count.max(thread.messages.len());
            let incoming_end = if thread.loaded_message_end == 0 && !thread.messages.is_empty() {
                incoming_total
            } else {
                thread.loaded_message_end.max(thread.messages.len())
            };
            let incoming_start = if incoming_end >= thread.messages.len() {
                thread
                    .loaded_message_start
                    .min(incoming_end.saturating_sub(thread.messages.len()))
            } else {
                0
            };
            self.chat.active_thread_id() == Some(thread_id.as_str())
                && self.chat.scroll_offset() > 0
                && incoming_end == existing.loaded_message_start
                && incoming_start < incoming_end
        });
        let preserved_scroll = if should_preserve_prepend_anchor {
            widgets::chat::scrollbar_layout(
                self.pane_layout().chat,
                &self.chat,
                &self.theme,
                self.tick_counter,
                self.retry_wait_start_selected,
            )
            .map(|layout| layout.scroll)
            .unwrap_or_else(|| self.chat.scroll_offset())
        } else {
            0
        };
        let viewport_anchor = if should_preserve_prepend_anchor {
            None
        } else {
            self.capture_locked_chat_viewport(Some(thread_id.as_str()))
        };
        self.finish_thread_loading(&thread_id);
        let should_select_thread = self.chat.active_thread_id().is_none();
        if self.chat.active_thread_id() == Some(thread_id.as_str()) {
            self.clear_chat_drag_selection();
        }
        self.chat.reduce(chat::ChatAction::ThreadDetailReceived(
            conversion::convert_thread(thread),
        ));
        if self.active_auto_response_suggestion().is_some() {
            self.auto_response_selection = AutoResponseActionSelection::Yes;
        }
        self.sync_participant_queued_prompts_for_thread(&thread_id, &live_suggestion_ids);
        if should_preserve_prepend_anchor {
            self.chat.preserve_prepend_scroll_anchor(preserved_scroll);
        } else {
            self.restore_locked_chat_viewport(viewport_anchor);
        }
        if self.sidebar.active_tab() == crate::state::sidebar::SidebarTab::Pinned
            && !self.chat.active_thread_has_pinned_messages()
        {
            self.sidebar
                .reduce(crate::state::sidebar::SidebarAction::SwitchTab(
                    crate::state::sidebar::SidebarTab::Todos,
                ));
        }
        if should_select_thread {
            self.chat
                .reduce(chat::ChatAction::SelectThread(thread_id.clone()));
        }
        if self
            .pending_pinned_jump
            .as_ref()
            .is_some_and(|pending| pending.thread_id == thread_id)
        {
            let pending = self
                .pending_pinned_jump
                .as_ref()
                .expect("checked pending pinned jump")
                .clone();
            if let Some(message_index) = self
                .chat
                .active_thread_pinned_messages()
                .into_iter()
                .find(|message| {
                    (!pending.message_id.is_empty() && message.message_id == pending.message_id)
                        || message.absolute_index == pending.absolute_index
                })
                .and_then(|message| {
                    self.chat
                        .resolve_active_pinned_message_to_loaded_index(&message)
                })
            {
                self.pending_pinned_jump = None;
                self.main_pane_view = MainPaneView::Conversation;
                self.focus = FocusArea::Chat;
                self.chat.select_message(Some(message_index));
                self.status_line = "Pinned message".to_string();
            }
        }
        self.sync_pending_approvals_from_tasks();
        self.sync_open_thread_picker();
        self.send_daemon_command(DaemonCommand::RequestThreadTodos(thread_id.clone()));
        self.send_daemon_command(DaemonCommand::RequestThreadWorkContext(thread_id.clone()));
        self.finish_pending_reconnect_restore(&thread_id);
        let _ = self.maybe_request_auto_response_for_open_thread(&thread_id);
        let _ = self.maybe_auto_send_always_auto_response();
        self.sync_contextual_approval_overlay();
    }

    pub(in crate::app) fn handle_thread_created_event(
        &mut self,
        thread_id: String,
        title: String,
        agent_name: Option<String>,
    ) {
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
        self.sync_open_thread_picker();
        self.sync_pending_approvals_from_tasks();
    }

    pub(in crate::app) fn handle_thread_reload_required_event(&mut self, thread_id: String) {
        if Self::is_hidden_agent_thread(&thread_id, None)
            || self.should_ignore_internal_thread_activity(&thread_id)
        {
            return;
        }
        if self.chat.active_thread_id() != Some(thread_id.as_str()) {
            return;
        }
        // Reload is the authoritative replacement for any live stream state that was
        // truncated or otherwise downgraded before reaching the TUI.
        self.chat.reduce(chat::ChatAction::ResetStreaming);
        if !self.should_preserve_bootstrap_activity_on_reload(thread_id.as_str()) {
            self.clear_agent_activity_for(Some(thread_id.as_str()));
        }
        self.clear_pending_stop();
        self.request_authoritative_thread_refresh(thread_id.clone(), true);
        self.send_daemon_command(DaemonCommand::RequestThreadTodos(thread_id.clone()));
        self.send_daemon_command(DaemonCommand::RequestThreadWorkContext(thread_id));
        self.status_line = "Thread reloaded from daemon".to_string();
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
