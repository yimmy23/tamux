use super::*;

#[path = "keyboard_enter.rs"]
mod enter;

impl TuiModel {
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
                    self.send_daemon_command(DaemonCommand::RetryOperatorProfile);
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
                self.agent_activity = None;
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
                            self.agent_activity = Some("retrying".to_string());
                        } else {
                            self.cancelled_thread_id = Some(thread_id.clone());
                            self.chat.reduce(chat::ChatAction::ForceStopStreaming);
                            self.agent_activity = None;
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

        match code {
            KeyCode::Char('p') if ctrl => self
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
            KeyCode::Char('i') if ctrl => {
                self.toggle_notifications_modal();
            }
            KeyCode::Char('b') if ctrl => {
                let current = self.show_sidebar_override.unwrap_or(self.width >= 80);
                self.show_sidebar_override = Some(!current);
            }
            KeyCode::Char('d') if ctrl => {
                if matches!(
                    self.main_pane_view,
                    MainPaneView::Task(_) | MainPaneView::WorkContext
                ) {
                    self.task_view_scroll = self
                        .task_view_scroll
                        .saturating_add((self.height / 2) as usize);
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
                    self.task_view_scroll = self
                        .task_view_scroll
                        .saturating_sub((self.height / 2) as usize);
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
                    self.send_daemon_command(DaemonCommand::RetryOperatorProfile);
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
                    self.task_view_scroll = self
                        .task_view_scroll
                        .saturating_add((self.height / 2) as usize);
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
                    self.task_view_scroll = self
                        .task_view_scroll
                        .saturating_sub((self.height / 2) as usize);
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
                        self.agent_activity = None;
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
                    let completion = self.input.complete_active_at_token();
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
                    self.task_view_scroll = 0;
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
                    self.task_view_scroll = usize::MAX / 4;
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
                        MainPaneView::Task(_) | MainPaneView::WorkContext
                    ) {
                        self.task_view_scroll = self.task_view_scroll.saturating_add(1);
                    } else {
                        self.chat.select_next_message()
                    }
                }
                FocusArea::Sidebar => self.sidebar.navigate(1, self.sidebar_item_count()),
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
                        MainPaneView::Task(_) | MainPaneView::WorkContext
                    ) {
                        self.task_view_scroll = self.task_view_scroll.saturating_sub(1);
                    } else {
                        self.chat.select_prev_message()
                    }
                }
                FocusArea::Sidebar => self.sidebar.navigate(-1, self.sidebar_item_count()),
                _ => {}
            },
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
                self.sidebar.reduce(sidebar::SidebarAction::SwitchTab(
                    sidebar::SidebarTab::Files,
                ));
            }
            KeyCode::Right if self.focus == FocusArea::Sidebar => {
                self.sidebar.reduce(sidebar::SidebarAction::SwitchTab(
                    sidebar::SidebarTab::Todos,
                ));
            }
            KeyCode::Char('[') if self.sidebar_visible() && self.focus != FocusArea::Input => {
                self.sidebar.reduce(sidebar::SidebarAction::SwitchTab(
                    sidebar::SidebarTab::Files,
                ));
            }
            KeyCode::Char(']') if self.sidebar_visible() && self.focus != FocusArea::Input => {
                self.sidebar.reduce(sidebar::SidebarAction::SwitchTab(
                    sidebar::SidebarTab::Todos,
                ));
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
            KeyCode::Char('r') if self.focus == FocusArea::Chat => {
                if let Some(sel) = self.chat.selected_message() {
                    self.chat.toggle_reasoning(sel);
                } else {
                    self.chat.toggle_last_reasoning();
                }
            }
            KeyCode::Char('t')
                if self.focus == FocusArea::Chat
                    && matches!(self.main_pane_view, MainPaneView::Task(_)) =>
            {
                self.task_show_live_todos = !self.task_show_live_todos;
            }
            KeyCode::Char('l')
                if self.focus == FocusArea::Chat
                    && matches!(self.main_pane_view, MainPaneView::Task(_)) =>
            {
                self.task_show_timeline = !self.task_show_timeline;
            }
            KeyCode::Char('f')
                if self.focus == FocusArea::Chat
                    && matches!(self.main_pane_view, MainPaneView::Task(_)) =>
            {
                self.task_show_files = !self.task_show_files;
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

        false
    }
}
