use super::*;

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

        if code == KeyCode::Char('e') && ctrl {
            if self.last_error.is_some() {
                if self.modal.top() == Some(modal::ModalKind::ErrorViewer) {
                    self.modal.reduce(modal::ModalAction::Pop);
                } else {
                    self.modal
                        .reduce(modal::ModalAction::Push(modal::ModalKind::ErrorViewer));
                    self.error_active = false;
                }
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
        if let Some(modal_kind) = self.modal.top() {
            return self.handle_key_modal(code, modifiers, modal_kind);
        }

        if self.concierge.welcome_visible
            && self.chat.active_thread_id() == Some("concierge")
            && self.focus == FocusArea::Chat
        {
            match code {
                KeyCode::Left | KeyCode::Up => {
                    self.concierge
                        .reduce(crate::state::ConciergeAction::NavigateAction(-1));
                    return false;
                }
                KeyCode::Right | KeyCode::Down => {
                    self.concierge
                        .reduce(crate::state::ConciergeAction::NavigateAction(1));
                    return false;
                }
                KeyCode::Enter | KeyCode::Char(' ') => {
                    self.execute_concierge_action(self.concierge.selected_action);
                    return false;
                }
                _ => {}
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
                            MainPaneView::Task(target) => {
                                if let Some(thread_id) = self.target_thread_id(target) {
                                    if self.tasks.selected_work_path(&thread_id).is_some() {
                                        self.tasks.reduce(task::TaskAction::SelectWorkPath {
                                            thread_id,
                                            path: None,
                                        });
                                        return false;
                                    }
                                }
                                self.main_pane_view = MainPaneView::Conversation;
                                self.task_view_scroll = 0;
                            }
                            MainPaneView::WorkContext => {
                                self.main_pane_view = MainPaneView::Conversation;
                                self.task_view_scroll = 0;
                            }
                            MainPaneView::GoalComposer => {
                                self.main_pane_view = MainPaneView::Conversation;
                            }
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
            KeyCode::Tab => self.focus_next(),
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
                    if matches!(
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
                    if matches!(
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
                if self.focus == FocusArea::Chat
                    || self.focus == FocusArea::Sidebar =>
            {
                if let Some(entry_id) = self.audit.selected_entry_id().map(String::from) {
                    self.audit
                        .reduce(crate::state::audit::AuditAction::DismissEntry(
                            entry_id.clone(),
                        ));
                    self.send_daemon_command(DaemonCommand::AuditDismiss {
                        entry_id,
                    });
                    self.show_input_notice("Audit entry dismissed", InputNoticeKind::Success, 40, true);
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
            KeyCode::Enter => {
                let shift = modifiers.contains(KeyModifiers::SHIFT);
                let alt = modifiers.contains(KeyModifiers::ALT);
                let ctrl_enter = modifiers.contains(KeyModifiers::CONTROL);
                if shift || alt || ctrl_enter {
                    if self.focus != FocusArea::Input {
                        self.focus = FocusArea::Input;
                        self.input.set_mode(input::InputMode::Insert);
                    }
                    self.input.reduce(input::InputAction::InsertNewline);
                    return false;
                }
                if self.focus == FocusArea::Chat {
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
                        let has_reasoning = self
                            .chat
                            .active_thread()
                            .and_then(|thread| thread.messages.get(sel))
                            .map(|msg| {
                                msg.role == chat::MessageRole::Assistant && msg.reasoning.is_some()
                            })
                            .unwrap_or(false);
                        if has_reasoning {
                            self.chat.toggle_reasoning(sel);
                        }
                        return false;
                    }
                }
                if self.focus == FocusArea::Sidebar {
                    self.handle_sidebar_enter();
                    return false;
                }
                if self.focus != FocusArea::Input {
                    self.focus = FocusArea::Input;
                    self.input.set_mode(input::InputMode::Insert);
                    return false;
                }
                self.input.reduce(input::InputAction::Submit);
                if let Some(prompt) = self.input.take_submitted() {
                    if prompt.starts_with('/') {
                        let trimmed = prompt.trim_start_matches('/');
                        let cmd = trimmed.split_whitespace().next().unwrap_or("");
                        let args = trimmed[cmd.len()..].trim();
                        if cmd == "apikey" && !args.is_empty() {
                            self.config.api_key = args.to_string();
                            self.status_line =
                                format!("API key set ({}...)", &args[..args.len().min(8)]);
                            if let Ok(value_json) =
                                serde_json::to_string(&serde_json::Value::String(args.to_string()))
                            {
                                self.send_daemon_command(DaemonCommand::SetConfigItem {
                                    key_path: "/api_key".to_string(),
                                    value_json: value_json.clone(),
                                });
                                self.send_daemon_command(DaemonCommand::SetConfigItem {
                                    key_path: format!(
                                        "/providers/{}/api_key",
                                        self.config.provider
                                    ),
                                    value_json: value_json.clone(),
                                });
                                self.send_daemon_command(DaemonCommand::SetConfigItem {
                                    key_path: format!("/{}/api_key", self.config.provider),
                                    value_json,
                                });
                            }
                        } else if cmd == "attach" && !args.is_empty() {
                            self.attach_file(args);
                        } else {
                            self.execute_command(cmd);
                        }
                    } else {
                        if matches!(self.main_pane_view, MainPaneView::GoalComposer) {
                            self.start_goal_run_from_prompt(prompt);
                        } else {
                            self.submit_prompt(prompt);
                        }
                    }
                }
            }
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
                        self.delete_message(sel);
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
                    if let Some(thread) = self.chat.active_thread() {
                        if let Some(msg) = thread.messages.get(sel) {
                            conversion::copy_to_clipboard(&msg.content);
                            self.status_line = "Copied to clipboard".to_string();
                        }
                    }
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
