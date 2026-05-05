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
        let threads = threads
            .into_iter()
            .filter(|thread| !self.deleted_thread_ids.contains(&thread.id))
            .collect::<Vec<_>>();
        for thread in &threads {
            self.missing_runtime_thread_ids.remove(&thread.id);
        }
        let active_thread_id = self.chat.active_thread_id().map(str::to_string);
        let pending_loading_thread_id = self.thread_loading_id.clone();
        // ThreadList events can be paginated, filtered, or IPC-truncated refresh pages; absence
        // from one page is not a deletion signal. Keep locally cached threads until ThreadDeleted
        // or an explicit detail/list entry replaces them.
        let preserve_missing_threads = self
            .chat
            .threads()
            .iter()
            .filter(|existing| !threads.iter().any(|thread| thread.id == existing.id))
            .cloned()
            .collect::<Vec<_>>();
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
        let mut threads = threads
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
            .collect::<Vec<_>>();
        for existing_thread in preserve_missing_threads {
            if !threads
                .iter()
                .any(|thread| thread.id == existing_thread.id)
            {
                threads.push(existing_thread);
            }
        }
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
        let is_empty_workspace_runtime_detail = thread_id.starts_with("workspace-thread:")
            && thread.messages.is_empty()
            && thread.total_message_count == 0;
        if is_empty_workspace_runtime_detail {
            self.empty_hydrated_runtime_thread_ids
                .insert(thread_id.clone());
        } else {
            self.empty_hydrated_runtime_thread_ids.remove(&thread_id);
        }
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

}
