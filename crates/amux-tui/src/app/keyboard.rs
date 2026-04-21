use super::*;

#[path = "keyboard_enter.rs"]
mod enter;

impl TuiModel {
    fn matches_shift_char(code: KeyCode, modifiers: KeyModifiers, expected: char) -> bool {
        modifiers.contains(KeyModifiers::SHIFT)
            && matches!(code, KeyCode::Char(ch) if ch.eq_ignore_ascii_case(&expected))
    }

    fn pinned_shortcut_scope_active(&self) -> bool {
        !self.sidebar_uses_goal_sidebar()
            && self.sidebar_visible()
            && self.sidebar.active_tab() == sidebar::SidebarTab::Pinned
            && self.chat.active_thread_has_pinned_messages()
    }

    fn sidebar_navigation_tabs(&self) -> Vec<sidebar::SidebarTab> {
        widgets::sidebar::visible_tabs(&self.tasks, &self.chat, self.chat.active_thread_id())
    }

    fn step_sidebar_tab(&mut self, delta: i32) {
        if self.sidebar_uses_goal_sidebar() {
            self.step_goal_sidebar_tab(delta);
            return;
        }

        let tabs = self.sidebar_navigation_tabs();
        let Some(last_index) = tabs.len().checked_sub(1) else {
            return;
        };
        let current_index = tabs
            .iter()
            .position(|tab| *tab == self.sidebar.active_tab())
            .unwrap_or(0);
        let next_index = (current_index as i32 + delta).clamp(0, last_index as i32) as usize;
        self.activate_sidebar_tab(tabs[next_index]);
    }

    fn arm_pinned_shortcut_leader(&mut self) {
        self.pending_pinned_shortcut_leader = Some(PendingPinnedShortcutLeader::Active);
        self.status_line = "Pinned shortcuts: J jump, U unpin".to_string();
        self.show_input_notice(
            "Pinned shortcuts: Ctrl+K J jump, Ctrl+K U unpin",
            InputNoticeKind::Success,
            60,
            true,
        );
    }

    fn handle_pending_pinned_shortcut_leader(
        &mut self,
        code: KeyCode,
        modifiers: KeyModifiers,
    ) -> bool {
        if self.pending_pinned_shortcut_leader.is_none() {
            return false;
        }
        self.pending_pinned_shortcut_leader = None;

        if !self.pinned_shortcut_scope_active() {
            return false;
        }

        match code {
            KeyCode::Esc => {
                self.status_line = "Pinned shortcut cancelled".to_string();
                true
            }
            KeyCode::Char(ch)
                if !modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
            {
                match ch.to_ascii_lowercase() {
                    'j' => {
                        self.handle_sidebar_enter();
                        true
                    }
                    'u' => {
                        self.unpin_selected_sidebar_message();
                        true
                    }
                    _ => false,
                }
            }
            _ => false,
        }
    }

    pub(super) fn paste_from_clipboard(&mut self) {
        if let Ok(text) = arboard::Clipboard::new().and_then(|mut cb| cb.get_text()) {
            if !text.is_empty() {
                self.handle_paste(text);
            }
        }
    }

    pub fn handle_key_release(&mut self, code: KeyCode, _modifiers: KeyModifiers) {
        self.update_held_modifier(code, false);
        self.clear_dismissable_input_notice();
    }

    pub fn handle_key(&mut self, code: KeyCode, modifiers: KeyModifiers) -> bool {
        self.update_held_modifier(code, true);
        let modifiers = modifiers | self.held_key_modifiers;

        if matches!(code, KeyCode::Modifier(_)) {
            return false;
        }

        let ctrl = modifiers.contains(KeyModifiers::CONTROL);

        if self.should_show_operator_profile_onboarding() && ctrl {
            match code {
                KeyCode::Char('s') => {
                    if self.skip_operator_profile_question() {
                        return false;
                    }
                }
                KeyCode::Char('d') => {
                    if self.defer_operator_profile_question() {
                        return false;
                    }
                }
                KeyCode::Char('r') => {
                    self.retry_operator_profile_request();
                    self.status_line = "Retrying operator profile operation…".to_string();
                    self.show_input_notice(
                        "Retrying operator profile operation…",
                        InputNoticeKind::Success,
                        40,
                        true,
                    );
                    return false;
                }
                _ => {}
            }
        }

        if code == KeyCode::Char('e') && ctrl {
            if self.modal.top() == Some(modal::ModalKind::ErrorViewer) {
                self.last_error = None;
                self.error_active = false;
                self.close_top_modal();
            } else if self.last_error.is_some() {
                self.modal
                    .reduce(modal::ModalAction::Push(modal::ModalKind::ErrorViewer));
                self.error_active = false;
            }
            return false;
        }
        if code == KeyCode::Char('c') && ctrl {
            if self.assistant_busy() {
                self.cancelled_thread_id = self.chat.active_thread_id().map(String::from);
                self.chat.reduce(chat::ChatAction::ForceStopStreaming);
                self.clear_active_thread_activity();
                self.show_input_notice("Stopped stream", InputNoticeKind::Success, 100, false);
            } else if self.focus == FocusArea::Chat {
                if matches!(self.main_pane_view, MainPaneView::WorkContext) {
                    self.copy_work_context_content();
                } else if let Some(sel) = self.chat.selected_message() {
                    self.copy_message(sel);
                }
            }
            return false;
        }
        if code == KeyCode::Char('q') && ctrl {
            self.open_queued_prompts_modal();
            return false;
        }
        if code == KeyCode::Char('s') && ctrl {
            if self.modal.top() == Some(modal::ModalKind::GoalPicker) {
                return self.handle_key_modal(code, modifiers, modal::ModalKind::GoalPicker);
            }
            if self.focus == FocusArea::Chat
                && matches!(
                    self.main_pane_view,
                    MainPaneView::Task(sidebar::SidebarItemTarget::GoalRun { .. })
                )
                && self.request_selected_goal_run_toggle_confirmation()
            {
                return false;
            }
            if self.voice_player.is_some() {
                self.stop_voice_playback();
                return false;
            }
        }
        if code == KeyCode::Char('a') && ctrl {
            match self.modal.top() {
                Some(modal::ModalKind::ApprovalOverlay) => {}
                Some(modal::ModalKind::OperatorQuestionOverlay) => {}
                Some(modal::ModalKind::ApprovalCenter) => self.close_top_modal(),
                None => self.toggle_approval_center(),
                _ => {}
            }
            return false;
        }
        if let Some(modal_kind) = self.modal.top() {
            return self.handle_key_modal(code, modifiers, modal_kind);
        }

        if self.handle_pending_pinned_shortcut_leader(code, modifiers) {
            return false;
        }

        if self.focus == FocusArea::Chat
            && self.goal_workspace.focused_pane()
                == crate::state::goal_workspace::GoalWorkspacePane::CommandBar
            && matches!(
                self.main_pane_view,
                MainPaneView::Task(sidebar::SidebarItemTarget::GoalRun { .. })
            )
            && !modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
        {
            let target_mode = match code {
                KeyCode::Char('1') => Some(crate::state::goal_workspace::GoalWorkspaceMode::Goal),
                KeyCode::Char('2') => Some(crate::state::goal_workspace::GoalWorkspaceMode::Files),
                KeyCode::Char('3') => {
                    Some(crate::state::goal_workspace::GoalWorkspaceMode::Progress)
                }
                KeyCode::Char('4') => {
                    Some(crate::state::goal_workspace::GoalWorkspaceMode::ActiveAgent)
                }
                KeyCode::Char('5') => {
                    Some(crate::state::goal_workspace::GoalWorkspaceMode::Threads)
                }
                KeyCode::Char('6') => {
                    Some(crate::state::goal_workspace::GoalWorkspaceMode::NeedsAttention)
                }
                _ => None,
            };
            if let Some(mode) = target_mode {
                self.set_goal_workspace_mode(mode);
                return false;
            }
        }

        if self.focus == FocusArea::Sidebar
            && !self.sidebar_uses_goal_sidebar()
            && self.sidebar.active_tab() == sidebar::SidebarTab::Files
            && !modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
        {
            match code {
                KeyCode::Esc if !self.sidebar.files_filter().is_empty() => {
                    self.sidebar.clear_files_filter();
                    return false;
                }
                KeyCode::Backspace => {
                    if self.sidebar.pop_files_filter() {
                        return false;
                    }
                }
                KeyCode::Char(c) if !c.is_control() && c != '[' && c != ']' => {
                    self.sidebar.push_files_filter(c);
                    return false;
                }
                _ => {}
            }
        }

        if code == KeyCode::Backspace
            && self.focus == FocusArea::Sidebar
            && self.chat.can_go_back_thread()
        {
            self.go_back_thread();
            return false;
        }

        if !self.chat.active_actions().is_empty() && self.focus == FocusArea::Chat {
            match code {
                KeyCode::Left | KeyCode::Up => {
                    self.navigate_visible_concierge_action(-1);
                    return false;
                }
                KeyCode::Right | KeyCode::Down => {
                    self.navigate_visible_concierge_action(1);
                    return false;
                }
                KeyCode::Enter | KeyCode::Char(' ') => {
                    self.execute_concierge_action(self.concierge.selected_action);
                    return false;
                }
                _ => {}
            }
        }

        let retry_waiting = matches!(self.main_pane_view, MainPaneView::Conversation)
            && self
                .chat
                .retry_status()
                .is_some_and(|status| matches!(status.phase, chat::RetryPhase::Waiting));
        let auto_response_waiting = matches!(self.main_pane_view, MainPaneView::Conversation)
            && self.active_auto_response_suggestion().is_some()
            && matches!(self.focus, FocusArea::Chat | FocusArea::Input);
        if auto_response_waiting {
            let remaining_secs = self.active_auto_response_countdown_secs().unwrap_or(0);
            match code {
                KeyCode::Left | KeyCode::Char('h') => {
                    self.auto_response_selection = match self.auto_response_selection {
                        AutoResponseActionSelection::Yes => AutoResponseActionSelection::Yes,
                        AutoResponseActionSelection::No => AutoResponseActionSelection::Yes,
                        AutoResponseActionSelection::Always => AutoResponseActionSelection::No,
                    };
                    self.status_line = match self.auto_response_selection {
                        AutoResponseActionSelection::Yes => {
                            format!("Auto response: Yes in {}s", remaining_secs)
                        }
                        AutoResponseActionSelection::No => "Auto response: No".to_string(),
                        AutoResponseActionSelection::Always => {
                            "Auto response: Always for this thread".to_string()
                        }
                    };
                    return false;
                }
                KeyCode::Right | KeyCode::Char('l') => {
                    self.auto_response_selection = match self.auto_response_selection {
                        AutoResponseActionSelection::Yes => AutoResponseActionSelection::No,
                        AutoResponseActionSelection::No => AutoResponseActionSelection::Always,
                        AutoResponseActionSelection::Always => AutoResponseActionSelection::Always,
                    };
                    self.status_line = match self.auto_response_selection {
                        AutoResponseActionSelection::Yes => {
                            format!("Auto response: Yes in {}s", remaining_secs)
                        }
                        AutoResponseActionSelection::No => "Auto response: No".to_string(),
                        AutoResponseActionSelection::Always => {
                            "Auto response: Always for this thread".to_string()
                        }
                    };
                    return false;
                }
                KeyCode::Enter | KeyCode::Char(' ') => {
                    let _ = self.execute_active_auto_response_action(self.auto_response_selection);
                    return false;
                }
                _ => {}
            }
        }
        let retry_wait_accepts_keyboard =
            retry_waiting && matches!(self.focus, FocusArea::Chat | FocusArea::Input);
        if retry_wait_accepts_keyboard {
            match code {
                KeyCode::Left | KeyCode::Char('h') if matches!(self.focus, FocusArea::Chat) => {
                    self.retry_wait_start_selected = true;
                    self.status_line = "Retry prompt: Yes now".to_string();
                    return false;
                }
                KeyCode::Right | KeyCode::Char('l') if matches!(self.focus, FocusArea::Chat) => {
                    self.retry_wait_start_selected = false;
                    self.status_line = "Retry prompt: No".to_string();
                    return false;
                }
                KeyCode::Left => {
                    self.retry_wait_start_selected = true;
                    self.status_line = "Retry prompt: Yes now".to_string();
                    return false;
                }
                KeyCode::Right => {
                    self.retry_wait_start_selected = false;
                    self.status_line = "Retry prompt: No".to_string();
                    return false;
                }
                KeyCode::Enter | KeyCode::Char(' ') => {
                    if let Some(thread_id) = self.chat.active_thread_id().map(str::to_string) {
                        if self.retry_wait_start_selected {
                            self.chat.reduce(chat::ChatAction::ClearRetryStatus {
                                thread_id: thread_id.clone(),
                            });
                            self.send_daemon_command(DaemonCommand::RetryStreamNow { thread_id });
                            self.status_line = "Retrying now...".to_string();
                            self.set_active_thread_activity("retrying");
                        } else {
                            self.cancelled_thread_id = Some(thread_id.clone());
                            self.chat.reduce(chat::ChatAction::ForceStopStreaming);
                            self.clear_active_thread_activity();
                            self.send_daemon_command(DaemonCommand::StopStream { thread_id });
                            self.status_line = "Stopped retry loop".to_string();
                        }
                    }
                    return false;
                }
                _ => {}
            }
        }

        if self.focus == FocusArea::Chat
            && matches!(self.main_pane_view, MainPaneView::Conversation)
            && self.chat.selected_message().is_some()
        {
            let action_count = self.selected_inline_message_action_count();
            if action_count > 0 {
                match code {
                    KeyCode::Left => {
                        self.chat.navigate_selected_message_action(-1, action_count);
                        return false;
                    }
                    KeyCode::Right => {
                        self.chat.navigate_selected_message_action(1, action_count);
                        return false;
                    }
                    KeyCode::Enter | KeyCode::Char(' ') => {
                        if self.execute_selected_inline_message_action() {
                            return false;
                        }
                    }
                    _ => {}
                }
            }
        }

        if code == KeyCode::Enter
            && !modifiers
                .intersects(KeyModifiers::SHIFT | KeyModifiers::ALT | KeyModifiers::CONTROL)
            && self.should_show_provider_onboarding()
        {
            let pending_input = self.input.buffer().trim();
            if !pending_input.starts_with('/') {
                self.open_provider_setup();
                return false;
            }
        }

        if code != KeyCode::Esc {
            self.clear_pending_stop();
        }

        if code == KeyCode::Esc
            && matches!(self.main_pane_view, MainPaneView::GoalComposer)
            && self.cancel_goal_mission_control()
        {
            self.clear_pending_stop();
            return false;
        }

        match code {
            KeyCode::Char('p') if ctrl && self.focus != FocusArea::Chat => self
                .modal
                .reduce(modal::ModalAction::Push(modal::ModalKind::CommandPalette)),
            KeyCode::Char('t') if ctrl => {
                self.modal
                    .reduce(modal::ModalAction::Push(modal::ModalKind::ThreadPicker));
                self.sync_thread_picker_item_count();
            }
            KeyCode::Char('g') if ctrl => {
                self.modal
                    .reduce(modal::ModalAction::Push(modal::ModalKind::GoalPicker));
                self.sync_goal_picker_item_count();
                self.focus = FocusArea::Chat;
            }
            KeyCode::Char('o')
                if ctrl
                    && matches!(self.main_pane_view, MainPaneView::GoalComposer)
                    && self.mission_control_has_thread_target() =>
            {
                let _ = self.open_mission_control_goal_thread();
            }
            KeyCode::Char('n') if ctrl => {
                self.toggle_notifications_modal();
            }
            KeyCode::Char('b') if ctrl => {
                let current = self.show_sidebar_override.unwrap_or(self.width >= 80);
                self.show_sidebar_override = Some(!current);
            }
            KeyCode::Char('k') if ctrl && self.pinned_shortcut_scope_active() => {
                self.arm_pinned_shortcut_leader();
            }
            KeyCode::Char('d') if ctrl => {
                if matches!(
                    self.main_pane_view,
                    MainPaneView::Task(_) | MainPaneView::WorkContext
                ) {
                    self.step_detail_view_scroll((self.height / 2) as i32);
                } else {
                    let half_page = (self.height / 2) as i32;
                    self.chat.reduce(chat::ChatAction::ScrollChat(-half_page));
                }
            }
            KeyCode::Char('u') if ctrl => {
                if self.focus == FocusArea::Input {
                    self.input.reduce(input::InputAction::ClearLine);
                } else if matches!(
                    self.main_pane_view,
                    MainPaneView::Task(_) | MainPaneView::WorkContext
                ) {
                    self.step_detail_view_scroll(-((self.height / 2) as i32));
                } else {
                    let half_page = (self.height / 2) as i32;
                    self.chat.reduce(chat::ChatAction::ScrollChat(half_page));
                }
            }
            KeyCode::Char('r') if ctrl => {
                if self
                    .input_notice
                    .as_ref()
                    .is_some_and(|notice| notice.text.contains("operator profile"))
                {
                    self.retry_operator_profile_request();
                    self.status_line = "Retrying operator profile operation…".to_string();
                    self.show_input_notice(
                        "Retrying operator profile operation…",
                        InputNoticeKind::Success,
                        40,
                        true,
                    );
                }
            }
            KeyCode::PageDown if self.focus == FocusArea::Chat => {
                if matches!(
                    self.main_pane_view,
                    MainPaneView::Task(_) | MainPaneView::WorkContext
                ) {
                    self.step_detail_view_scroll((self.height / 2) as i32);
                } else {
                    let half_page = (self.height / 2) as i32;
                    self.chat.reduce(chat::ChatAction::ScrollChat(-half_page));
                }
            }
            KeyCode::PageUp if self.focus == FocusArea::Chat => {
                if matches!(
                    self.main_pane_view,
                    MainPaneView::Task(_) | MainPaneView::WorkContext
                ) {
                    self.step_detail_view_scroll(-((self.height / 2) as i32));
                } else {
                    let half_page = (self.height / 2) as i32;
                    self.chat.reduce(chat::ChatAction::ScrollChat(half_page));
                }
            }
            KeyCode::Esc => {
                if self.dismiss_active_main_pane(FocusArea::Chat) {
                    self.clear_pending_stop();
                    return false;
                }
                if self.assistant_busy() {
                    if self.pending_stop_active() {
                        self.cancelled_thread_id = self.chat.active_thread_id().map(String::from);
                        self.chat.reduce(chat::ChatAction::ForceStopStreaming);
                        self.clear_active_thread_activity();
                        self.status_line = "Stopped stream".to_string();
                        self.show_input_notice(
                            "Stopped stream",
                            InputNoticeKind::Success,
                            100,
                            false,
                        );
                        self.pending_stop = false;
                    } else {
                        self.pending_stop = true;
                        self.pending_stop_tick = self.tick_counter;
                        self.status_line = "Press Esc again to stop stream".to_string();
                        self.show_input_notice(
                            "Press Esc again to stop stream",
                            InputNoticeKind::Warning,
                            100,
                            true,
                        );
                    }
                } else {
                    self.clear_pending_stop();
                    if self.focus == FocusArea::Chat {
                        match &self.main_pane_view {
                            MainPaneView::Collaboration
                            | MainPaneView::Task(_)
                            | MainPaneView::WorkContext
                            | MainPaneView::FilePreview(_)
                            | MainPaneView::GoalComposer => {}
                            MainPaneView::Conversation => {
                                if self.chat.selected_message().is_some() {
                                    self.chat.select_message(None);
                                    let current_scroll = self.chat.scroll_offset() as i32;
                                    if current_scroll > 0 {
                                        self.chat
                                            .reduce(chat::ChatAction::ScrollChat(-current_scroll));
                                    }
                                }
                            }
                        }
                    } else if self.focus == FocusArea::Input {
                        self.focus = FocusArea::Chat;
                    }
                }
            }
            KeyCode::Tab => {
                if self.focus == FocusArea::Input {
                    let completion = self.input.complete_active_at_token_with_agents(
                        &self.known_agent_directive_aliases(),
                    );
                    if let Some(notice) = completion.notice {
                        self.status_line = notice.clone();
                        self.show_input_notice(notice, InputNoticeKind::Warning, 40, true);
                    }
                    if completion.consumed {
                        return false;
                    }
                }
                self.focus_next();
            }
            KeyCode::BackTab => self.focus_prev(),
            KeyCode::Left if self.focus == FocusArea::Input => {
                self.input.reduce(input::InputAction::MoveCursorLeft);
            }
            KeyCode::Right if self.focus == FocusArea::Input => {
                self.input.reduce(input::InputAction::MoveCursorRight);
            }
            KeyCode::Up if self.focus == FocusArea::Input => {
                let wrap_w = self.input_wrap_width();
                self.input
                    .reduce(input::InputAction::MoveCursorUpVisual(wrap_w));
            }
            KeyCode::Down if self.focus == FocusArea::Input => {
                let wrap_w = self.input_wrap_width();
                self.input
                    .reduce(input::InputAction::MoveCursorDownVisual(wrap_w));
            }
            KeyCode::Home if self.focus == FocusArea::Input => {
                self.input.reduce(input::InputAction::MoveCursorHome);
            }
            KeyCode::End if self.focus == FocusArea::Input => {
                self.input.reduce(input::InputAction::MoveCursorEnd);
            }
            KeyCode::Char('z') if ctrl && self.focus == FocusArea::Input => {
                self.input.reduce(input::InputAction::Undo);
            }
            KeyCode::Char('y') if ctrl && self.focus == FocusArea::Input => {
                self.input.reduce(input::InputAction::Redo);
            }
            KeyCode::Home if self.focus == FocusArea::Chat => {
                if matches!(
                    self.main_pane_view,
                    MainPaneView::Task(_) | MainPaneView::WorkContext
                ) {
                    self.scroll_detail_view_to_top();
                } else {
                    self.chat.reduce(chat::ChatAction::ScrollChat(i32::MAX / 2));
                    self.chat.select_message(Some(0));
                }
            }
            KeyCode::End if self.focus == FocusArea::Chat => {
                if matches!(
                    self.main_pane_view,
                    MainPaneView::Task(_) | MainPaneView::WorkContext
                ) {
                    self.scroll_detail_view_to_bottom();
                } else {
                    let offset = self.chat.scroll_offset() as i32;
                    self.chat.reduce(chat::ChatAction::ScrollChat(-offset));
                    self.chat.select_message(None);
                }
            }
            KeyCode::Down if self.focus != FocusArea::Input => match self.focus {
                FocusArea::Chat => {
                    if matches!(self.main_pane_view, MainPaneView::Collaboration)
                        && self.collaboration.focus() == CollaborationPaneFocus::Navigator
                    {
                        self.collaboration.reduce(CollaborationAction::SelectRow(
                            self.collaboration.selected_row_index().saturating_add(1),
                        ));
                    } else if matches!(
                        self.main_pane_view,
                        MainPaneView::Task(sidebar::SidebarItemTarget::GoalRun { .. })
                    ) {
                        match self.goal_workspace.focused_pane() {
                            crate::state::goal_workspace::GoalWorkspacePane::Plan => {
                                self.step_goal_workspace_plan_selection(1);
                            }
                            crate::state::goal_workspace::GoalWorkspacePane::Timeline => {
                                self.step_goal_workspace_timeline_selection(1);
                            }
                            crate::state::goal_workspace::GoalWorkspacePane::Details => {
                                self.step_goal_workspace_detail_selection(1);
                            }
                            crate::state::goal_workspace::GoalWorkspacePane::CommandBar => {}
                        }
                    } else if matches!(self.main_pane_view, MainPaneView::GoalComposer) {
                        if self
                            .goal_mission_control
                            .cycle_selected_runtime_assignment(1)
                        {
                            let role_label = self
                                .goal_mission_control
                                .selected_runtime_row_label()
                                .unwrap_or("assignment");
                            self.status_line = format!("Mission Control selected {role_label}");
                        }
                    } else if matches!(
                        self.main_pane_view,
                        MainPaneView::Task(_) | MainPaneView::WorkContext
                    ) {
                        self.step_detail_view_scroll(1);
                    } else {
                        self.chat.select_next_message()
                    }
                }
                FocusArea::Sidebar => {
                    if self.sidebar_uses_goal_sidebar() {
                        self.navigate_goal_sidebar(1);
                    } else {
                        self.sidebar.navigate(1, self.sidebar_item_count());
                    }
                }
                _ => {}
            },
            KeyCode::Up if self.focus != FocusArea::Input => match self.focus {
                FocusArea::Chat => {
                    if matches!(self.main_pane_view, MainPaneView::Collaboration)
                        && self.collaboration.focus() == CollaborationPaneFocus::Navigator
                    {
                        self.collaboration.reduce(CollaborationAction::SelectRow(
                            self.collaboration.selected_row_index().saturating_sub(1),
                        ));
                    } else if matches!(
                        self.main_pane_view,
                        MainPaneView::Task(sidebar::SidebarItemTarget::GoalRun { .. })
                    ) {
                        match self.goal_workspace.focused_pane() {
                            crate::state::goal_workspace::GoalWorkspacePane::Plan => {
                                self.step_goal_workspace_plan_selection(-1);
                            }
                            crate::state::goal_workspace::GoalWorkspacePane::Timeline => {
                                self.step_goal_workspace_timeline_selection(-1);
                            }
                            crate::state::goal_workspace::GoalWorkspacePane::Details => {
                                self.step_goal_workspace_detail_selection(-1);
                            }
                            crate::state::goal_workspace::GoalWorkspacePane::CommandBar => {}
                        }
                    } else if matches!(self.main_pane_view, MainPaneView::GoalComposer) {
                        if self
                            .goal_mission_control
                            .cycle_selected_runtime_assignment(-1)
                        {
                            let role_label = self
                                .goal_mission_control
                                .selected_runtime_row_label()
                                .unwrap_or("assignment");
                            self.status_line = format!("Mission Control selected {role_label}");
                        }
                    } else if matches!(
                        self.main_pane_view,
                        MainPaneView::Task(_) | MainPaneView::WorkContext
                    ) {
                        self.step_detail_view_scroll(-1);
                    } else {
                        self.chat.select_prev_message()
                    }
                }
                FocusArea::Sidebar => {
                    if self.sidebar_uses_goal_sidebar() {
                        self.navigate_goal_sidebar(-1);
                    } else {
                        self.sidebar.navigate(-1, self.sidebar_item_count());
                    }
                }
                _ => {}
            },
            KeyCode::Left
                if self.focus == FocusArea::Chat
                    && self.goal_workspace.focused_pane()
                        == crate::state::goal_workspace::GoalWorkspacePane::CommandBar
                    && matches!(
                        self.main_pane_view,
                        MainPaneView::Task(sidebar::SidebarItemTarget::GoalRun { .. })
                    ) =>
            {
                self.cycle_goal_workspace_mode(-1);
            }
            KeyCode::Right
                if self.focus == FocusArea::Chat
                    && self.goal_workspace.focused_pane()
                        == crate::state::goal_workspace::GoalWorkspacePane::CommandBar
                    && matches!(
                        self.main_pane_view,
                        MainPaneView::Task(sidebar::SidebarItemTarget::GoalRun { .. })
                    ) =>
            {
                self.cycle_goal_workspace_mode(1);
            }
            KeyCode::Left
                if self.focus == FocusArea::Chat
                    && self.goal_workspace.focused_pane()
                        == crate::state::goal_workspace::GoalWorkspacePane::Plan
                    && matches!(
                        self.main_pane_view,
                        MainPaneView::Task(sidebar::SidebarItemTarget::GoalRun { .. })
                    ) =>
            {
                self.collapse_goal_workspace_selection();
            }
            KeyCode::Right
                if self.focus == FocusArea::Chat
                    && self.goal_workspace.focused_pane()
                        == crate::state::goal_workspace::GoalWorkspacePane::Plan
                    && matches!(
                        self.main_pane_view,
                        MainPaneView::Task(sidebar::SidebarItemTarget::GoalRun { .. })
                    ) =>
            {
                self.expand_selected_goal_workspace_step();
            }
            KeyCode::Left
                if self.focus == FocusArea::Chat
                    && matches!(self.main_pane_view, MainPaneView::Collaboration) =>
            {
                if self.collaboration.focus() == CollaborationPaneFocus::Detail {
                    if self.collaboration.selected_detail_action_index() > 0 {
                        self.collaboration
                            .reduce(CollaborationAction::StepDetailAction(-1));
                    } else {
                        self.collaboration.reduce(CollaborationAction::SetFocus(
                            CollaborationPaneFocus::Navigator,
                        ));
                    }
                }
            }
            KeyCode::Right
                if self.focus == FocusArea::Chat
                    && matches!(self.main_pane_view, MainPaneView::Collaboration) =>
            {
                if self.collaboration.focus() == CollaborationPaneFocus::Navigator {
                    self.collaboration.reduce(CollaborationAction::SetFocus(
                        CollaborationPaneFocus::Detail,
                    ));
                } else {
                    self.collaboration
                        .reduce(CollaborationAction::StepDetailAction(1));
                }
            }
            KeyCode::Left if self.focus == FocusArea::Sidebar => {
                self.step_sidebar_tab(-1);
            }
            KeyCode::Right if self.focus == FocusArea::Sidebar => {
                self.step_sidebar_tab(1);
            }
            KeyCode::Char('[')
                if self.sidebar_visible()
                    && self.focus != FocusArea::Input
                    && !(self.focus == FocusArea::Chat
                        && matches!(
                            self.main_pane_view,
                            MainPaneView::Task(sidebar::SidebarItemTarget::GoalRun { .. })
                        )) =>
            {
                self.step_sidebar_tab(-1);
            }
            KeyCode::Char(']')
                if self.sidebar_visible()
                    && self.focus != FocusArea::Input
                    && !(self.focus == FocusArea::Chat
                        && matches!(
                            self.main_pane_view,
                            MainPaneView::Task(sidebar::SidebarItemTarget::GoalRun { .. })
                        )) =>
            {
                self.step_sidebar_tab(1);
            }
            KeyCode::Char('u')
                if self.focus == FocusArea::Sidebar
                    && !self.sidebar_uses_goal_sidebar()
                    && self.sidebar.active_tab() == sidebar::SidebarTab::Pinned =>
            {
                self.unpin_selected_sidebar_message();
            }
            KeyCode::Char('b')
                if self.focus == FocusArea::Chat
                    && matches!(self.main_pane_view, MainPaneView::Conversation)
                    && (self.mission_control_return_to_thread_id().is_some()
                        || self.mission_control_return_to_goal_target().is_some()) =>
            {
                let _ = self.return_from_mission_control_navigation();
            }
            // Dismiss selected audit entry with 'd' key (BEAT-07)
            KeyCode::Char('d')
                if self.focus == FocusArea::Chat || self.focus == FocusArea::Sidebar =>
            {
                if let Some(entry_id) = self.audit.selected_entry_id().map(String::from) {
                    self.audit
                        .reduce(crate::state::audit::AuditAction::DismissEntry(
                            entry_id.clone(),
                        ));
                    self.send_daemon_command(DaemonCommand::AuditDismiss { entry_id });
                    self.show_input_notice(
                        "Audit entry dismissed",
                        InputNoticeKind::Success,
                        40,
                        true,
                    );
                }
            }
            KeyCode::Char('t')
                if self.focus == FocusArea::Chat
                    && matches!(self.main_pane_view, MainPaneView::Task(_)) =>
            {
                self.task_show_live_todos = !self.task_show_live_todos;
                self.clamp_detail_view_scroll();
            }
            KeyCode::Char('l')
                if self.focus == FocusArea::Chat
                    && matches!(self.main_pane_view, MainPaneView::Task(_)) =>
            {
                self.task_show_timeline = !self.task_show_timeline;
                self.clamp_detail_view_scroll();
            }
            KeyCode::Char('a')
                if self.focus == FocusArea::Chat
                    && matches!(self.main_pane_view, MainPaneView::GoalComposer)
                    && !self.goal_mission_control.runtime_mode() =>
            {
                self.goal_mission_control.append_preflight_assignment();
                let role_label = self
                    .goal_mission_control
                    .selected_runtime_row_label()
                    .unwrap_or("assignment");
                self.status_line = format!("Mission Control added {role_label}");
            }
            KeyCode::Char('a')
                if self.focus == FocusArea::Chat
                    && matches!(self.main_pane_view, MainPaneView::Task(_)) =>
            {
                if self.open_goal_step_action_picker() {
                    self.status_line = "Goal actions".to_string();
                }
            }
            KeyCode::Char('r')
                if self.focus == FocusArea::Chat
                    && !modifiers.contains(KeyModifiers::SHIFT)
                    && matches!(self.main_pane_view, MainPaneView::Task(_)) =>
            {
                if self.request_selected_goal_step_retry_confirmation() {
                    self.status_line = "Retry selected goal step?".to_string();
                }
            }
            KeyCode::Char(ch)
                if self.focus == FocusArea::Chat
                    && Self::matches_shift_char(KeyCode::Char(ch), modifiers, 'r')
                    && matches!(self.main_pane_view, MainPaneView::Task(_)) =>
            {
                if self.request_selected_goal_step_rerun_confirmation() {
                    self.status_line = "Rerun goal from selected step?".to_string();
                }
            }
            KeyCode::Char('m')
                if self.focus == FocusArea::Chat
                    && matches!(
                        self.main_pane_view,
                        MainPaneView::Task(sidebar::SidebarItemTarget::GoalRun { .. })
                    ) =>
            {
                if self.open_mission_control_runtime_editor() {
                    self.status_line = "Opened Mission Control runtime editor".to_string();
                } else {
                    self.status_line = "Mission Control runtime editor is unavailable".to_string();
                }
            }
            KeyCode::Char('p')
                if self.focus == FocusArea::Chat
                    && matches!(self.main_pane_view, MainPaneView::GoalComposer) =>
            {
                if !self.stage_mission_control_assignment_modal_edit(
                    goal_mission_control::RuntimeAssignmentEditField::Provider,
                ) {
                    self.status_line = "Mission Control roster is unavailable".to_string();
                }
            }
            KeyCode::Char('m')
                if self.focus == FocusArea::Chat
                    && matches!(self.main_pane_view, MainPaneView::GoalComposer) =>
            {
                if !self.stage_mission_control_assignment_modal_edit(
                    goal_mission_control::RuntimeAssignmentEditField::Model,
                ) {
                    self.status_line = "Mission Control roster is unavailable".to_string();
                }
            }
            KeyCode::Char('e')
                if self.focus == FocusArea::Chat
                    && matches!(self.main_pane_view, MainPaneView::GoalComposer) =>
            {
                if !self.stage_mission_control_assignment_modal_edit(
                    goal_mission_control::RuntimeAssignmentEditField::ReasoningEffort,
                ) {
                    self.status_line = "Mission Control roster is unavailable".to_string();
                }
            }
            KeyCode::Char('r')
                if self.focus == FocusArea::Chat
                    && matches!(self.main_pane_view, MainPaneView::GoalComposer) =>
            {
                if !self.stage_mission_control_assignment_modal_edit(
                    goal_mission_control::RuntimeAssignmentEditField::Role,
                ) {
                    self.status_line = "Mission Control roster is unavailable".to_string();
                }
            }
            KeyCode::Char('s')
                if self.focus == FocusArea::Chat
                    && matches!(self.main_pane_view, MainPaneView::GoalComposer)
                    && !self.goal_mission_control.runtime_mode() =>
            {
                self.goal_mission_control.toggle_save_as_default_pending();
                self.status_line = if self.goal_mission_control.save_as_default_pending {
                    "Mission Control preflight will be saved as the new default".to_string()
                } else {
                    "Mission Control preflight will not overwrite defaults".to_string()
                };
            }
            KeyCode::Char('[')
                if self.focus == FocusArea::Chat
                    && matches!(
                        self.main_pane_view,
                        MainPaneView::Task(sidebar::SidebarItemTarget::GoalRun { .. })
                    ) =>
            {
                self.step_goal_step_selection(-1);
            }
            KeyCode::Char(']')
                if self.focus == FocusArea::Chat
                    && matches!(
                        self.main_pane_view,
                        MainPaneView::Task(sidebar::SidebarItemTarget::GoalRun { .. })
                    ) =>
            {
                self.step_goal_step_selection(1);
            }
            KeyCode::Char('r') if self.focus == FocusArea::Chat => {
                if let Some(sel) = self.chat.selected_message() {
                    self.chat.toggle_reasoning(sel);
                } else {
                    self.chat.toggle_last_reasoning();
                }
            }
            KeyCode::Char('f')
                if self.focus == FocusArea::Chat
                    && matches!(self.main_pane_view, MainPaneView::Task(_)) =>
            {
                self.task_show_files = !self.task_show_files;
                self.clamp_detail_view_scroll();
            }
            KeyCode::Char('e') if self.focus == FocusArea::Chat => {
                if let Some(sel) = self.chat.selected_message() {
                    let is_tool = self
                        .chat
                        .active_thread()
                        .and_then(|thread| thread.messages.get(sel))
                        .map(|msg| msg.role == chat::MessageRole::Tool)
                        .unwrap_or(false);
                    if is_tool {
                        self.chat.toggle_tool_expansion(sel);
                    }
                }
            }
            KeyCode::Char('j') if ctrl && self.focus == FocusArea::Input => {
                self.input.reduce(input::InputAction::InsertNewline);
            }
            KeyCode::Char('l') if ctrl && self.focus == FocusArea::Input => {
                self.toggle_voice_capture();
            }
            KeyCode::Char('p') if ctrl && self.focus == FocusArea::Chat => {
                self.speak_latest_assistant_message();
            }
            KeyCode::Enter => return self.handle_enter_key(modifiers),
            KeyCode::Backspace if ctrl => {
                if self.focus == FocusArea::Input {
                    self.input.reduce(input::InputAction::DeleteWord);
                }
            }
            KeyCode::Char('h') if ctrl && self.focus == FocusArea::Input => {
                self.input.reduce(input::InputAction::DeleteWord);
            }
            KeyCode::Backspace => {
                if self.focus == FocusArea::Input {
                    self.input.reduce(input::InputAction::Backspace);
                    if self.modal.top() == Some(modal::ModalKind::CommandPalette) {
                        self.modal.reduce(modal::ModalAction::SetQuery(
                            self.input.buffer().to_string(),
                        ));
                    }
                }
            }
            KeyCode::Delete => {
                if self.focus == FocusArea::Chat {
                    if let Some(sel) = self.chat.selected_message() {
                        self.request_delete_message(sel);
                    }
                }
            }
            KeyCode::Char('/') if self.focus != FocusArea::Input => {
                self.input.reduce(input::InputAction::Clear);
                self.input.reduce(input::InputAction::InsertChar('/'));
                self.input.set_mode(input::InputMode::Insert);
                self.focus = FocusArea::Input;
                self.modal
                    .reduce(modal::ModalAction::Push(modal::ModalKind::CommandPalette));
            }
            KeyCode::Char('w') if ctrl && self.focus == FocusArea::Input => {
                self.input.reduce(input::InputAction::DeleteWord);
            }
            KeyCode::Char('v' | 'V') if ctrl => self.paste_from_clipboard(),
            KeyCode::Char('\u{16}') => self.paste_from_clipboard(),
            KeyCode::Insert if modifiers.contains(KeyModifiers::SHIFT) => {
                self.paste_from_clipboard();
            }
            KeyCode::Char('c')
                if self.focus == FocusArea::Chat && self.chat.selected_message().is_some() =>
            {
                if let Some(sel) = self.chat.selected_message() {
                    self.copy_message(sel);
                }
            }
            KeyCode::Char(c) => {
                if self.focus == FocusArea::Input {
                    self.input.reduce(input::InputAction::InsertChar(c));
                    if c == '/'
                        && self.input.buffer() == "/"
                        && self.modal.top() != Some(modal::ModalKind::CommandPalette)
                    {
                        self.modal
                            .reduce(modal::ModalAction::Push(modal::ModalKind::CommandPalette));
                    }
                    if self.modal.top() == Some(modal::ModalKind::CommandPalette) {
                        self.modal.reduce(modal::ModalAction::SetQuery(
                            self.input.buffer().to_string(),
                        ));
                    }
                } else {
                    self.focus = FocusArea::Input;
                    self.input.set_mode(input::InputMode::Insert);
                    self.input.reduce(input::InputAction::InsertChar(c));
                }
            }
            _ => {}
        }

        self.sync_goal_mission_control_prompt_from_input();

        false
    }
}
