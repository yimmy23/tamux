impl TuiModel {
    fn chat_history_page_size(&self) -> usize {
        self.config.tui_chat_history_page_size.max(25) as usize
    }

    fn request_thread_page(
        &mut self,
        thread_id: String,
        message_limit: usize,
        message_offset: usize,
        show_loading: bool,
    ) {
        if show_loading {
            self.begin_thread_loading(thread_id.clone());
        }
        self.send_daemon_command(DaemonCommand::RequestThread {
            thread_id,
            message_limit: Some(message_limit),
            message_offset: Some(message_offset),
        });
    }

    fn request_latest_thread_page(&mut self, thread_id: String, show_loading: bool) {
        self.request_thread_page(thread_id, self.chat_history_page_size(), 0, show_loading);
    }

    fn thread_needs_expanded_latest_page(&self, thread_id: &str) -> bool {
        self.chat.threads().iter().any(|thread| {
            thread.id == thread_id
                && (!thread.thread_participants.is_empty()
                    || !thread.queued_participant_suggestions.is_empty())
        })
    }

    fn request_authoritative_thread_refresh(&mut self, thread_id: String, show_loading: bool) {
        let base_limit = self.chat_history_page_size();
        let message_limit = if self.thread_needs_expanded_latest_page(&thread_id) {
            base_limit.saturating_mul(2)
        } else {
            base_limit
        };
        self.request_thread_page(thread_id, message_limit, 0, show_loading);
    }

    fn maybe_request_older_chat_history(&mut self) {
        let Some(message_offset) = self.chat.active_thread_next_page_offset(self.tick_counter)
        else {
            return;
        };
        let Some(thread_id) = self.chat.active_thread_id().map(str::to_string) else {
            return;
        };
        let chat_area = self.pane_layout().chat;
        let Some(layout) = widgets::chat::scrollbar_layout(
            chat_area,
            &self.chat,
            &self.theme,
            self.tick_counter,
            self.retry_wait_start_selected,
        ) else {
            return;
        };
        if layout.max_scroll.saturating_sub(layout.scroll) > 3 {
            return;
        }

        self.chat.mark_active_thread_older_page_pending(
            true,
            self.tick_counter,
            chat::CHAT_HISTORY_FETCH_DEBOUNCE_TICKS,
        );
        self.request_thread_page(
            thread_id,
            self.chat_history_page_size(),
            message_offset,
            false,
        );
    }

    fn maybe_request_older_goal_run_history(&mut self) {
        let MainPaneView::Task(sidebar::SidebarItemTarget::GoalRun { goal_run_id, .. }) =
            &self.main_pane_view
        else {
            return;
        };
        if self.task_view_scroll > 3 {
            return;
        }

        let Some((step_offset, step_limit, event_offset, event_limit)) = self
            .tasks
            .goal_run_next_page_request(goal_run_id, self.tick_counter)
        else {
            return;
        };

        self.tasks.mark_goal_run_older_page_pending(
            goal_run_id,
            true,
            self.tick_counter,
            task::GOAL_RUN_HISTORY_FETCH_DEBOUNCE_TICKS,
        );
        self.send_daemon_command(DaemonCommand::RequestGoalRunDetailPage {
            goal_run_id: goal_run_id.clone(),
            step_offset,
            step_limit,
            event_offset,
            event_limit,
        });
    }

    fn maybe_schedule_chat_history_collapse(&mut self) {
        if self.chat.scroll_offset() != 0 {
            return;
        }
        if self.chat.active_thread().is_some_and(|thread| {
            thread.history_window_expanded && thread.collapse_deadline_tick.is_none()
        }) {
            self.chat.schedule_history_collapse(
                self.tick_counter,
                chat::CHAT_HISTORY_COLLAPSE_DELAY_TICKS,
            );
        }
    }

    pub(super) fn thread_picker_target_agent_id(
        tab: modal::ThreadPickerTab,
    ) -> Option<&'static str> {
        match tab {
            modal::ThreadPickerTab::Swarog => Some(amux_protocol::AGENT_ID_SWAROG),
            modal::ThreadPickerTab::Rarog => Some(amux_protocol::AGENT_ID_RAROG),
            modal::ThreadPickerTab::Weles => Some("weles"),
            modal::ThreadPickerTab::Playgrounds => None,
            modal::ThreadPickerTab::Internal => None,
        }
    }

    fn cleanup_concierge_on_navigate(&mut self) {
        if !self.concierge.auto_cleanup_on_navigate {
            return;
        }

        self.clear_chat_drag_selection();
        self.clear_work_context_drag_selection();
        self.ignore_pending_concierge_welcome = true;
        self.concierge
            .reduce(crate::state::ConciergeAction::WelcomeDismissed);
        self.chat.reduce(chat::ChatAction::DismissConciergeWelcome);
        self.send_daemon_command(DaemonCommand::DismissConciergeWelcome);

        if self.chat.active_thread_id() == Some("concierge") && self.assistant_busy() {
            let thread_id = "concierge".to_string();
            self.cancelled_thread_id = Some(thread_id.clone());
            self.chat.reduce(chat::ChatAction::ForceStopStreaming);
            self.clear_agent_activity_for(Some(thread_id.as_str()));
            self.send_daemon_command(DaemonCommand::StopStream { thread_id });
        }

        self.clear_pending_stop();
    }

    fn open_thread_conversation(&mut self, thread_id: String) {
        self.cleanup_concierge_on_navigate();
        self.clear_chat_drag_selection();
        self.clear_work_context_drag_selection();
        self.pending_new_thread_target_agent = None;
        self.chat
            .reduce(chat::ChatAction::SelectThread(thread_id.clone()));
        self.request_latest_thread_page(thread_id, true);
        self.main_pane_view = MainPaneView::Conversation;
        self.focus = FocusArea::Chat;
    }

    fn start_new_thread_view(&mut self) {
        self.start_new_thread_view_for_agent(None);
    }

    fn start_new_thread_view_for_agent(&mut self, target_agent_id: Option<&str>) {
        self.cleanup_concierge_on_navigate();
        self.clear_chat_drag_selection();
        self.clear_work_context_drag_selection();
        self.pending_new_thread_target_agent = target_agent_id.map(str::to_string);
        self.thread_loading_id = None;
        self.chat.reduce(chat::ChatAction::NewThread);
        self.main_pane_view = MainPaneView::Conversation;
        self.focus = FocusArea::Input;
        self.concierge
            .reduce(crate::state::ConciergeAction::WelcomeLoading(false));
    }

    fn request_concierge_welcome(&mut self) {
        self.ignore_pending_concierge_welcome = false;
        self.concierge
            .reduce(crate::state::ConciergeAction::WelcomeLoading(true));
        self.send_daemon_command(DaemonCommand::RequestConciergeWelcome);
    }

    fn set_main_pane_conversation(&mut self, focus: FocusArea) {
        self.main_pane_view = MainPaneView::Conversation;
        self.task_view_scroll = 0;
        self.focus = focus;
    }

    fn dismiss_active_main_pane(&mut self, focus: FocusArea) -> bool {
        match &self.main_pane_view {
            MainPaneView::Task(target) => {
                if let Some(thread_id) = self.target_thread_id(target) {
                    if self.tasks.selected_work_path(&thread_id).is_some() {
                        self.tasks.reduce(task::TaskAction::SelectWorkPath {
                            thread_id,
                            path: None,
                        });
                        self.focus = focus;
                        return true;
                    }
                }
                self.set_main_pane_conversation(focus);
                true
            }
            MainPaneView::Collaboration
            | MainPaneView::WorkContext
            | MainPaneView::FilePreview(_)
            | MainPaneView::GoalComposer => {
                self.set_main_pane_conversation(focus);
                true
            }
            MainPaneView::Conversation => false,
        }
    }

    fn should_toggle_work_context_from_sidebar(&self, thread_id: &str) -> bool {
        if !matches!(self.main_pane_view, MainPaneView::WorkContext) {
            return false;
        }

        match self.sidebar.active_tab() {
            SidebarTab::Files => {
                widgets::sidebar::selected_file_path(&self.tasks, &self.sidebar, Some(thread_id))
                    .is_some_and(|path| {
                        self.tasks.selected_work_path(thread_id) == Some(path.as_str())
                    })
            }
            SidebarTab::Todos => self
                .tasks
                .todos_for_thread(thread_id)
                .get(self.sidebar.selected_item())
                .is_some(),
            SidebarTab::Spawned => false,
            SidebarTab::Pinned => false,
        }
    }

    fn visible_concierge_action_count(&self) -> usize {
        let active_actions = self.chat.active_actions();
        if !active_actions.is_empty() {
            active_actions.len()
        } else {
            self.concierge.welcome_actions.len()
        }
    }

    fn select_visible_concierge_action(&mut self, action_index: usize) {
        let action_count = self.visible_concierge_action_count();
        self.concierge.selected_action = if action_count == 0 {
            0
        } else {
            action_index.min(action_count - 1)
        };
    }

    fn navigate_visible_concierge_action(&mut self, delta: i32) {
        let action_count = self.visible_concierge_action_count();
        if action_count == 0 {
            self.concierge.selected_action = 0;
        } else if delta > 0 {
            self.concierge.selected_action =
                (self.concierge.selected_action + delta as usize).min(action_count - 1);
        } else {
            self.concierge.selected_action = self
                .concierge
                .selected_action
                .saturating_sub((-delta) as usize);
        }
    }

    fn resolve_visible_concierge_action(
        &self,
        action_index: usize,
    ) -> Option<crate::state::ConciergeActionVm> {
        self.chat
            .active_actions()
            .get(action_index)
            .map(|action| crate::state::ConciergeActionVm {
                label: action.label.clone(),
                action_type: action.action_type.clone(),
                thread_id: action.thread_id.clone(),
            })
            .or_else(|| self.concierge.welcome_actions.get(action_index).cloned())
    }

    fn execute_concierge_action(&mut self, action_index: usize) {
        let Some(action) = self.resolve_visible_concierge_action(action_index) else {
            return;
        };
        self.run_concierge_action(action);
    }

    fn selected_inline_message_action_count(&self) -> usize {
        let Some(selected_message) = self.chat.selected_message() else {
            return 0;
        };
        let Some(message) = self
            .chat
            .active_thread()
            .and_then(|thread| thread.messages.get(selected_message))
        else {
            return 0;
        };
        let is_last_actionable = !message.actions.is_empty()
            && self
                .chat
                .active_actions()
                .first()
                .map(|action| &action.label)
                == message.actions.first().map(|action| &action.label);
        if is_last_actionable {
            0
        } else {
            widgets::chat::message_action_targets(
                &self.chat,
                selected_message,
                message,
                self.tick_counter,
            )
            .len()
        }
    }

    fn execute_concierge_message_action(&mut self, message_index: usize, action_index: usize) {
        let Some(action) = self
            .chat
            .active_thread()
            .and_then(|thread| thread.messages.get(message_index))
            .and_then(|message| message.actions.get(action_index))
            .cloned()
        else {
            return;
        };
        self.run_concierge_action(crate::state::ConciergeActionVm {
            label: action.label,
            action_type: action.action_type,
            thread_id: action.thread_id,
        });
    }

    fn run_concierge_action(&mut self, action: crate::state::ConciergeActionVm) {
        if let Some((question_id, answer)) = action
            .action_type
            .strip_prefix("operator_question_answer:")
            .and_then(|rest| {
                let (question_id, answer) = rest.split_once(':')?;
                Some((question_id.to_string(), answer.to_string()))
            })
        {
            self.send_daemon_command(DaemonCommand::AnswerOperatorQuestion {
                question_id,
                answer,
            });
            return;
        }

        match action.action_type.as_str() {
            "continue_session" => {
                if let Some(thread_id) = action.thread_id {
                    self.open_thread_conversation(thread_id);
                }
            }
            "start_new" => {
                self.start_new_thread_view();
            }
            "search" => {
                self.ignore_pending_concierge_welcome = true;
                self.concierge
                    .reduce(crate::state::ConciergeAction::WelcomeDismissed);
                self.send_daemon_command(DaemonCommand::DismissConciergeWelcome);
                self.chat
                    .reduce(chat::ChatAction::SelectThread("concierge".to_string()));
                self.request_latest_thread_page("concierge".to_string(), false);
                self.main_pane_view = MainPaneView::Conversation;
                self.focus = FocusArea::Input;
                self.set_input_text("Search history for: ");
                self.status_line = "Describe what you want to search and press Enter".to_string();
            }
            "dismiss" | "dismiss_welcome" => {
                self.cleanup_concierge_on_navigate();
                if self.chat.active_thread_id() == Some("concierge") {
                    self.chat.reduce(chat::ChatAction::NewThread);
                    self.main_pane_view = MainPaneView::Conversation;
                    self.focus = FocusArea::Input;
                }
            }
            "start_goal_run" => {
                self.cleanup_concierge_on_navigate();
                self.chat
                    .reduce(chat::ChatAction::SelectThread("concierge".to_string()));
                self.request_latest_thread_page("concierge".to_string(), false);
                self.main_pane_view = MainPaneView::Conversation;
                self.focus = FocusArea::Input;
                self.set_input_text("/goal ");
                self.status_line = "Describe your goal and press Enter".to_string();
            }
            "focus_chat" => {
                self.cleanup_concierge_on_navigate();
                self.chat
                    .reduce(chat::ChatAction::SelectThread("concierge".to_string()));
                self.request_latest_thread_page("concierge".to_string(), false);
                self.main_pane_view = MainPaneView::Conversation;
                self.focus = FocusArea::Input;
            }
            "open_settings" => {
                self.cleanup_concierge_on_navigate();
                self.open_settings_tab(SettingsTab::Auth);
            }
            "operator_profile_skip" => {
                let _ = self.skip_operator_profile_question();
            }
            "operator_profile_defer" => {
                let _ = self.defer_operator_profile_question();
            }
            "operator_profile_retry" => {
                self.send_daemon_command(DaemonCommand::RetryOperatorProfile);
                self.status_line = "Retrying operator profile operation…".to_string();
                self.show_input_notice(
                    "Retrying operator profile operation…",
                    InputNoticeKind::Success,
                    40,
                    true,
                );
            }
            _ => {}
        }
    }

    fn open_pending_action_confirm(&mut self, action: PendingConfirmAction) {
        self.pending_chat_action_confirm = Some(action);
        self.chat_action_confirm_accept_selected = true;
        if self.modal.top() != Some(modal::ModalKind::ChatActionConfirm) {
            self.modal.reduce(modal::ModalAction::Push(
                modal::ModalKind::ChatActionConfirm,
            ));
        }
    }

    fn close_chat_action_confirm(&mut self) {
        self.pending_chat_action_confirm = None;
        self.chat_action_confirm_accept_selected = true;
        if self.modal.top() == Some(modal::ModalKind::ChatActionConfirm) {
            self.close_top_modal();
        }
    }

    fn request_regenerate_message(&mut self, index: usize) {
        self.open_pending_action_confirm(PendingConfirmAction::RegenerateMessage {
            message_index: index,
        });
    }

    fn request_delete_message(&mut self, index: usize) {
        self.open_pending_action_confirm(PendingConfirmAction::DeleteMessage {
            message_index: index,
        });
    }

    fn confirm_pending_chat_action(&mut self) {
        let Some(pending) = self.pending_chat_action_confirm.take() else {
            return;
        };
        if self.modal.top() == Some(modal::ModalKind::ChatActionConfirm) {
            self.close_top_modal();
        }
        self.chat_action_confirm_accept_selected = true;
        match pending {
            PendingConfirmAction::RegenerateMessage { message_index } => {
                self.regenerate_from_message(message_index)
            }
            PendingConfirmAction::DeleteMessage { message_index } => {
                self.delete_message(message_index)
            }
            PendingConfirmAction::DeleteThread { thread_id, .. } => {
                self.send_daemon_command(DaemonCommand::DeleteThread {
                    thread_id: thread_id.clone(),
                });
                self.status_line = "Deleting thread...".to_string();
            }
            PendingConfirmAction::StopThread { thread_id, .. } => {
                if self.chat.active_thread_id() == Some(thread_id.as_str()) {
                    self.cancelled_thread_id = Some(thread_id.clone());
                    self.chat.reduce(chat::ChatAction::ForceStopStreaming);
                    self.clear_active_thread_activity();
                }
                self.send_daemon_command(DaemonCommand::StopStream { thread_id });
                self.status_line = "Stopping thread...".to_string();
            }
            PendingConfirmAction::ResumeThread { thread_id, .. } => {
                self.send_daemon_command(DaemonCommand::SendMessage {
                    thread_id: Some(thread_id),
                    content: "continue".to_string(),
                    content_blocks_json: None,
                    session_id: self.default_session_id.clone(),
                    target_agent_id: None,
                });
                self.status_line = "Resuming thread...".to_string();
            }
            PendingConfirmAction::DeleteGoalRun { goal_run_id, .. } => {
                self.send_daemon_command(DaemonCommand::DeleteGoalRun { goal_run_id });
                self.status_line = "Deleting goal run...".to_string();
            }
            PendingConfirmAction::PauseGoalRun { goal_run_id, .. } => {
                self.send_daemon_command(DaemonCommand::ControlGoalRun {
                    goal_run_id,
                    action: "pause".to_string(),
                });
                self.status_line = "Pausing goal run...".to_string();
            }
            PendingConfirmAction::ResumeGoalRun { goal_run_id, .. } => {
                self.send_daemon_command(DaemonCommand::ControlGoalRun {
                    goal_run_id,
                    action: "resume".to_string(),
                });
                self.status_line = "Resuming goal run...".to_string();
            }
            PendingConfirmAction::ReuseModelAsStt { model_id } => {
                self.set_audio_config_string("stt", "model", model_id.clone());
                self.status_line = format!("STT model: {}", model_id);
            }
        }
    }

    fn execute_selected_inline_message_action(&mut self) -> bool {
        let Some(message_index) = self.chat.selected_message() else {
            return false;
        };
        let Some(message) = self
            .chat
            .active_thread()
            .and_then(|thread| thread.messages.get(message_index))
        else {
            return false;
        };

        let action_index = self.chat.selected_message_action();
        let Some((_, target)) = widgets::chat::message_action_targets(
            &self.chat,
            message_index,
            message,
            self.tick_counter,
        )
        .into_iter()
        .nth(action_index) else {
            return false;
        };

        match target {
            chat::ChatHitTarget::MessageAction {
                message_index,
                action_index,
            } => {
                self.chat.select_message(Some(message_index));
                self.chat.select_message_action(action_index);
                self.execute_concierge_message_action(message_index, action_index);
                true
            }
            chat::ChatHitTarget::CopyMessage(index) => {
                self.chat.select_message(Some(index));
                self.copy_message(index);
                true
            }
            chat::ChatHitTarget::ResendMessage(index) => {
                self.chat.select_message(Some(index));
                self.resend_message(index);
                true
            }
            chat::ChatHitTarget::RegenerateMessage(index) => {
                self.chat.select_message(Some(index));
                self.request_regenerate_message(index);
                true
            }
            chat::ChatHitTarget::PinMessage(index) => {
                self.chat.select_message(Some(index));
                self.pin_message_for_compaction(index);
                true
            }
            chat::ChatHitTarget::UnpinMessage(index) => {
                self.chat.select_message(Some(index));
                self.unpin_message_for_compaction(index);
                true
            }
            chat::ChatHitTarget::DeleteMessage(index) => {
                self.chat.select_message(Some(index));
                self.request_delete_message(index);
                true
            }
            chat::ChatHitTarget::ToolFilePath { message_index } => {
                self.chat.select_message(Some(message_index));
                false
            }
            _ => false,
        }
    }

    fn update_held_modifier(&mut self, code: KeyCode, pressed: bool) {
        let modifier = match code {
            KeyCode::Modifier(
                ModifierKeyCode::LeftShift
                | ModifierKeyCode::RightShift
                | ModifierKeyCode::IsoLevel3Shift
                | ModifierKeyCode::IsoLevel5Shift,
            ) => Some(KeyModifiers::SHIFT),
            KeyCode::Modifier(ModifierKeyCode::LeftControl | ModifierKeyCode::RightControl) => {
                Some(KeyModifiers::CONTROL)
            }
            KeyCode::Modifier(ModifierKeyCode::LeftAlt | ModifierKeyCode::RightAlt) => {
                Some(KeyModifiers::ALT)
            }
            _ => None,
        };

        if let Some(modifier) = modifier {
            if pressed {
                self.held_key_modifiers.insert(modifier);
            } else {
                self.held_key_modifiers.remove(modifier);
            }
        }
    }

    fn input_notice_style(&self) -> Option<(&str, Style)> {
        self.input_notice.as_ref().map(|notice| {
            let style = match notice.kind {
                InputNoticeKind::Warning => Style::default().fg(Color::Indexed(214)),
                InputNoticeKind::Success => Style::default().fg(Color::Indexed(114)),
            };
            (notice.text.as_str(), style)
        })
    }

    fn toggle_notifications_modal(&mut self) {
        if self.modal.top() == Some(modal::ModalKind::Notifications) {
            self.close_top_modal();
        } else {
            let header_action = self.notifications.first_enabled_header_action();
            self.notifications
                .reduce(crate::state::NotificationsAction::FocusHeader(
                    header_action,
                ));
            self.modal
                .reduce(modal::ModalAction::Push(modal::ModalKind::Notifications));
        }
    }

    fn toggle_approval_center(&mut self) {
        if self.modal.top() == Some(modal::ModalKind::ApprovalCenter) {
            self.close_top_modal();
        } else {
            self.modal
                .reduce(modal::ModalAction::Push(modal::ModalKind::ApprovalCenter));
            self.send_daemon_command(DaemonCommand::ListTaskApprovalRules);
        }
    }

    fn current_workspace_id(&self) -> Option<&str> {
        let workspace = self.config.honcho_workspace_id.trim();
        if workspace.is_empty() {
            None
        } else {
            Some(workspace)
        }
    }

    fn visible_approval_ids(&self) -> Vec<String> {
        self.approval
            .visible_approvals(self.chat.active_thread_id(), self.current_workspace_id())
            .iter()
            .map(|approval| approval.approval_id.clone())
            .collect()
    }

    fn step_approval_selection(&mut self, delta: i32) {
        let visible = self.visible_approval_ids();
        if visible.is_empty() {
            return;
        }
        let current = self
            .approval
            .selected_approval_id()
            .and_then(|approval_id| visible.iter().position(|id| id == approval_id))
            .unwrap_or(0) as i32;
        let next = (current + delta).clamp(0, visible.len().saturating_sub(1) as i32) as usize;
        self.approval
            .reduce(crate::state::ApprovalAction::SelectApproval(
                visible[next].clone(),
            ));
    }

    fn select_approval_center_row(&mut self, index: usize) {
        let visible = self.visible_approval_ids();
        if let Some(approval_id) = visible.get(index) {
            self.approval
                .reduce(crate::state::ApprovalAction::SelectApproval(
                    approval_id.clone(),
                ));
        }
    }

    fn select_approval_center_rule_row(&mut self, index: usize) {
        if let Some(rule_id) = self
            .approval
            .saved_rules()
            .get(index)
            .map(|rule| rule.id.clone())
        {
            self.approval
                .reduce(crate::state::ApprovalAction::SelectRule(rule_id));
        }
    }

    fn create_task_approval_rule(&mut self, approval_id: String) {
        self.send_daemon_command(DaemonCommand::CreateTaskApprovalRule {
            approval_id: approval_id.clone(),
        });
        self.resolve_approval(approval_id, "allow_once");
        self.status_line = "Saved always-approve rule".to_string();
    }

    fn revoke_selected_task_approval_rule(&mut self) {
        let Some(rule_id) = self.approval.selected_rule().map(|rule| rule.id.clone()) else {
            return;
        };
        self.approval
            .reduce(crate::state::ApprovalAction::RemoveRule(rule_id.clone()));
        self.send_daemon_command(DaemonCommand::RevokeTaskApprovalRule { rule_id });
        self.status_line = "Revoked always-approve rule".to_string();
    }

    fn resolve_approval(&mut self, approval_id: String, decision: &str) {
        self.approval.reduce(crate::state::ApprovalAction::Resolve {
            approval_id: approval_id.clone(),
            decision: decision.to_string(),
        });
        self.send_daemon_command(DaemonCommand::ResolveTaskApproval {
            approval_id,
            decision: decision.to_string(),
        });
    }

    fn next_current_thread_approval_id(&self) -> Option<String> {
        let current_thread_id = self.chat.active_thread_id()?;
        self.approval
            .pending_approvals()
            .iter()
            .find(|approval| approval.thread_id.as_deref() == Some(current_thread_id))
            .map(|approval| approval.approval_id.clone())
    }

    fn current_unix_ms() -> i64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_millis() as i64)
            .unwrap_or(0)
    }

    fn upsert_notification_local(&mut self, notification: amux_protocol::InboxNotification) {
        self.notifications
            .reduce(crate::state::NotificationsAction::Upsert(
                notification.clone(),
            ));
        self.send_daemon_command(DaemonCommand::UpsertNotification(notification));
    }

    fn mark_notification_read(&mut self, notification_id: &str) {
        let Some(mut notification) = self
            .notifications
            .all_items()
            .iter()
            .find(|item| item.id == notification_id)
            .cloned()
        else {
            return;
        };
        if notification.read_at.is_some() {
            return;
        }
        let now = Self::current_unix_ms();
        notification.read_at = Some(now);
        notification.updated_at = now;
        self.upsert_notification_local(notification);
    }

    fn toggle_notification_expand(&mut self, notification_id: String) {
        self.mark_notification_read(&notification_id);
        self.notifications
            .reduce(crate::state::NotificationsAction::ToggleExpand(
                notification_id,
            ));
    }

    fn archive_notification(&mut self, notification_id: &str) {
        let Some(mut notification) = self
            .notifications
            .all_items()
            .iter()
            .find(|item| item.id == notification_id)
            .cloned()
        else {
            return;
        };
        let now = Self::current_unix_ms();
        notification.read_at.get_or_insert(now);
        notification.archived_at = Some(now);
        notification.updated_at = now;
        self.upsert_notification_local(notification);
    }

    fn delete_notification(&mut self, notification_id: &str) {
        let Some(mut notification) = self
            .notifications
            .all_items()
            .iter()
            .find(|item| item.id == notification_id)
            .cloned()
        else {
            return;
        };
        let now = Self::current_unix_ms();
        notification.read_at.get_or_insert(now);
        notification.deleted_at = Some(now);
        notification.updated_at = now;
        self.upsert_notification_local(notification);
    }

    fn mark_all_notifications_read(&mut self) {
        let ids = self
            .notifications
            .active_items()
            .into_iter()
            .filter(|item| item.read_at.is_none())
            .map(|item| item.id.clone())
            .collect::<Vec<_>>();
        for id in ids {
            self.mark_notification_read(&id);
        }
    }

    fn archive_read_notifications(&mut self) {
        let ids = self
            .notifications
            .active_items()
            .into_iter()
            .filter(|item| item.read_at.is_some())
            .map(|item| item.id.clone())
            .collect::<Vec<_>>();
        for id in ids {
            self.archive_notification(&id);
        }
    }

    fn execute_notification_row_action(&mut self, notification_id: &str, action_index: usize) {
        match action_index {
            0 => self.toggle_notification_expand(notification_id.to_string()),
            1 => self.mark_notification_read(notification_id),
            2 => self.archive_notification(notification_id),
            3 => self.delete_notification(notification_id),
            other => {
                self.execute_notification_action(notification_id, "", Some(other.saturating_sub(4)))
            }
        }
    }

    fn execute_notification_action(
        &mut self,
        notification_id: &str,
        action_id: &str,
        action_index: Option<usize>,
    ) {
        let Some(notification) = self
            .notifications
            .all_items()
            .iter()
            .find(|item| item.id == notification_id)
            .cloned()
        else {
            return;
        };
        let action = action_index
            .and_then(|index| notification.actions.get(index).cloned())
            .or_else(|| {
                notification
                    .actions
                    .iter()
                    .find(|candidate| candidate.id == action_id)
                    .cloned()
            });
        let Some(action) = action else {
            return;
        };
        self.mark_notification_read(notification_id);
        match action.action_type.as_str() {
            "open_thread" => {
                if let Some(thread_id) = action.target.as_deref() {
                    self.close_top_modal();
                    self.open_thread_conversation(thread_id.to_string());
                    self.status_line = format!("Opened thread {}", thread_id);
                }
            }
            "open_plugin_settings" => {
                self.open_settings_tab(SettingsTab::Plugins);
                if let Some(plugin_name) = action.target.as_deref() {
                    let selected_index = self
                        .plugin_settings
                        .plugins
                        .iter()
                        .position(|plugin| plugin.name == plugin_name);
                    if let Some(index) = selected_index {
                        self.plugin_settings.selected_index = index;
                    }
                    self.plugin_settings.list_mode = selected_index.is_none();
                    self.plugin_settings.detail_cursor = 0;
                    self.plugin_settings.test_result = None;
                    self.plugin_settings.schema_fields.clear();
                    self.plugin_settings.settings_values.clear();
                    self.send_daemon_command(DaemonCommand::PluginGet(plugin_name.to_string()));
                    self.send_daemon_command(DaemonCommand::PluginGetSettings(
                        plugin_name.to_string(),
                    ));
                    self.status_line = format!("Opened plugin settings for {}", plugin_name);
                }
            }
            _ => {
                self.status_line = format!("Notification action unavailable: {}", action.label);
            }
        }
    }
}
