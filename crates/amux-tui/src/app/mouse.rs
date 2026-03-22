use super::*;

impl TuiModel {
    pub fn handle_resize(&mut self, width: u16, height: u16) {
        self.width = width;
        self.height = height;
        self.show_sidebar_override = None;
    }

    pub fn handle_mouse(&mut self, mouse: MouseEvent) {
        if matches!(
            mouse.kind,
            MouseEventKind::Down(_) | MouseEventKind::Up(_) | MouseEventKind::Drag(_)
        ) {
            self.clear_dismissable_input_notice();
        }

        if self.modal.top().is_some() {
            self.handle_modal_mouse(mouse);
            self.input.set_mode(input::InputMode::Insert);
            return;
        }

        let body_start_row: u16 = 3;
        let actual_input_height = self.input_height();
        let concierge_height = self.concierge_banner_height();
        let input_start_row: u16 =
            self.height.saturating_sub(actual_input_height + 1);
        let concierge_start_row = input_start_row.saturating_sub(concierge_height);
        let show_sidebar = self.sidebar_visible();
        let sidebar_pct: u16 = if self.width >= 120 { 33 } else { 28 };
        let sidebar_start_col: u16 = if show_sidebar {
            self.width * (100 - sidebar_pct) / 100
        } else {
            self.width
        };
        let chat_area = Rect::new(
            0,
            body_start_row,
            sidebar_start_col,
            input_start_row.saturating_sub(body_start_row),
        );
        let sidebar_area = if show_sidebar {
            Rect::new(
                sidebar_start_col,
                body_start_row,
                self.width.saturating_sub(sidebar_start_col),
                input_start_row.saturating_sub(body_start_row),
            )
        } else {
            Rect::default()
        };

        let cursor_in_body = mouse.row >= body_start_row && mouse.row < input_start_row;
        let cursor_in_concierge =
            concierge_height > 0 && mouse.row >= concierge_start_row && mouse.row < input_start_row;
        let cursor_in_sidebar = show_sidebar && cursor_in_body && mouse.column >= sidebar_start_col;
        let cursor_in_chat =
            cursor_in_body && mouse.row < concierge_start_row && mouse.column < sidebar_start_col;
        let cursor_in_input =
            mouse.row >= input_start_row && mouse.row < self.height.saturating_sub(1);
        let concierge_area = Rect::new(0, concierge_start_row, self.width, concierge_height);

        match mouse.kind {
            MouseEventKind::ScrollUp => {
                if cursor_in_chat {
                    if matches!(
                        self.main_pane_view,
                        MainPaneView::Task(_) | MainPaneView::WorkContext
                    ) {
                        self.task_view_scroll = self.task_view_scroll.saturating_sub(3);
                    } else {
                        self.chat.reduce(chat::ChatAction::ScrollChat(3));
                        if self.chat_drag_anchor.is_some() {
                            self.chat_drag_current = Some(Position::new(mouse.column, mouse.row));
                        }
                    }
                } else if cursor_in_sidebar {
                    self.sidebar.reduce(sidebar::SidebarAction::Scroll(3));
                } else if cursor_in_input {
                    for _ in 0..3 {
                        self.input.reduce(input::InputAction::MoveCursorUp);
                    }
                }
            }
            MouseEventKind::ScrollDown => {
                if cursor_in_chat {
                    if matches!(
                        self.main_pane_view,
                        MainPaneView::Task(_) | MainPaneView::WorkContext
                    ) {
                        self.task_view_scroll = self.task_view_scroll.saturating_add(3);
                    } else {
                        self.chat.reduce(chat::ChatAction::ScrollChat(-3));
                        if self.chat_drag_anchor.is_some() {
                            self.chat_drag_current = Some(Position::new(mouse.column, mouse.row));
                        }
                    }
                } else if cursor_in_sidebar {
                    self.sidebar.reduce(sidebar::SidebarAction::Scroll(-3));
                } else if cursor_in_input {
                    for _ in 0..3 {
                        self.input.reduce(input::InputAction::MoveCursorDown);
                    }
                }
            }
            MouseEventKind::Down(MouseButton::Left) => {
                if cursor_in_concierge {
                    if let Some(widgets::concierge::ConciergeHitTarget::Action(index)) =
                        widgets::concierge::hit_test(
                            concierge_area,
                            &self.concierge,
                            Position::new(mouse.column, mouse.row),
                        )
                    {
                        self.concierge
                            .reduce(crate::state::ConciergeAction::SelectAction(index));
                        self.execute_concierge_action(index);
                    } else if self.chat.active_thread_id() == Some("concierge") {
                        self.focus = FocusArea::Chat;
                    }
                } else if cursor_in_chat {
                    self.focus = FocusArea::Chat;
                    if matches!(self.main_pane_view, MainPaneView::Conversation) {
                        let pos = Position::new(mouse.column, mouse.row);
                        self.chat_drag_anchor = Some(pos);
                        self.chat_drag_current = Some(pos);
                    } else if matches!(self.main_pane_view, MainPaneView::WorkContext) {
                        let pos = Position::new(mouse.column, mouse.row);
                        self.work_context_drag_anchor = Some(pos);
                        self.work_context_drag_current = Some(pos);
                    } else if let MainPaneView::Task(target) = &self.main_pane_view {
                        if let Some(hit) = widgets::task_view::hit_test(
                            chat_area,
                            &self.tasks,
                            target,
                            &self.theme,
                            self.task_view_scroll,
                            self.task_show_live_todos,
                            self.task_show_timeline,
                            self.task_show_files,
                            Position::new(mouse.column, mouse.row),
                        ) {
                            if let Some(thread_id) = self.target_thread_id(target) {
                                match hit {
                                    widgets::task_view::TaskViewHitTarget::WorkPath(path) => {
                                        self.tasks.reduce(task::TaskAction::SelectWorkPath {
                                            thread_id: thread_id.clone(),
                                            path: Some(path),
                                        });
                                        self.request_preview_for_selected_path(&thread_id);
                                    }
                                    widgets::task_view::TaskViewHitTarget::ClosePreview => {
                                        self.tasks.reduce(task::TaskAction::SelectWorkPath {
                                            thread_id,
                                            path: None,
                                        });
                                    }
                                }
                            }
                        }
                    }
                } else if cursor_in_sidebar {
                    self.clear_chat_drag_selection();
                    self.clear_work_context_drag_selection();
                    self.focus = FocusArea::Sidebar;
                    match widgets::sidebar::hit_test(
                        sidebar_area,
                        &self.sidebar,
                        &self.tasks,
                        self.chat.active_thread_id(),
                        Position::new(mouse.column, mouse.row),
                    ) {
                        Some(widgets::sidebar::SidebarHitTarget::Tab(tab)) => {
                            self.sidebar.reduce(sidebar::SidebarAction::SwitchTab(tab));
                        }
                        Some(widgets::sidebar::SidebarHitTarget::File(path)) => {
                            if let Some(thread_id) = self.chat.active_thread_id().map(str::to_string)
                            {
                                let index = self
                                    .tasks
                                    .work_context_for_thread(&thread_id)
                                    .and_then(|context| {
                                        context.entries.iter().position(|entry| entry.path == path)
                                    })
                                    .unwrap_or(0);
                                self.sidebar.navigate(
                                    index as i32 - self.sidebar.selected_item() as i32,
                                    self.sidebar_item_count(),
                                );
                                self.handle_sidebar_enter();
                            }
                        }
                        Some(widgets::sidebar::SidebarHitTarget::Todo(index)) => {
                            self.sidebar.navigate(
                                index as i32 - self.sidebar.selected_item() as i32,
                                self.sidebar_item_count(),
                            );
                            self.handle_sidebar_enter();
                        }
                        None => {}
                    }
                } else if cursor_in_input {
                    self.clear_chat_drag_selection();
                    self.clear_work_context_drag_selection();
                    self.focus = FocusArea::Input;
                    if let Some(offset) = self.input_offset_from_mouse(input_start_row, mouse) {
                        self.input
                            .reduce(input::InputAction::MoveCursorToPos(offset));
                    }
                }
                self.input.set_mode(input::InputMode::Insert);
            }
            MouseEventKind::Drag(MouseButton::Left) => {
                if self.chat_drag_anchor.is_some()
                    && matches!(self.main_pane_view, MainPaneView::Conversation)
                {
                    if mouse.row <= chat_area.y.saturating_add(1) {
                        self.chat.reduce(chat::ChatAction::ScrollChat(1));
                    } else if mouse.row
                        >= chat_area
                            .y
                            .saturating_add(chat_area.height)
                            .saturating_sub(2)
                    {
                        self.chat.reduce(chat::ChatAction::ScrollChat(-1));
                    }
                    self.chat_drag_current = Some(Position::new(mouse.column, mouse.row));
                } else if self.work_context_drag_anchor.is_some()
                    && matches!(self.main_pane_view, MainPaneView::WorkContext)
                {
                    if mouse.row <= chat_area.y.saturating_add(1) {
                        self.task_view_scroll = self.task_view_scroll.saturating_sub(1);
                    } else if mouse.row
                        >= chat_area
                            .y
                            .saturating_add(chat_area.height)
                            .saturating_sub(2)
                    {
                        self.task_view_scroll = self.task_view_scroll.saturating_add(1);
                    }
                    self.work_context_drag_current = Some(Position::new(mouse.column, mouse.row));
                }
            }
            MouseEventKind::Up(MouseButton::Left) => {
                if let Some(anchor) = self.chat_drag_anchor.take() {
                    let current = self
                        .chat_drag_current
                        .take()
                        .unwrap_or(Position::new(mouse.column, mouse.row));
                    let Some((anchor_point, current_point)) =
                        widgets::chat::selection_points_from_mouse(
                            chat_area,
                            &self.chat,
                            &self.theme,
                            anchor,
                            current,
                        )
                    else {
                        if cursor_in_chat {
                            self.handle_chat_click(
                                chat_area,
                                Position::new(mouse.column, mouse.row),
                            );
                        }
                        return;
                    };

                    if anchor_point != current_point {
                        if let Some(text) = widgets::chat::selected_text(
                            chat_area,
                            &self.chat,
                            &self.theme,
                            anchor_point,
                            current_point,
                        ) {
                            conversion::copy_to_clipboard(&text);
                            self.status_line = "Copied selection to clipboard".to_string();
                        }
                    } else if cursor_in_chat {
                        self.handle_chat_click(chat_area, Position::new(mouse.column, mouse.row));
                    }
                } else if let Some(anchor) = self.work_context_drag_anchor.take() {
                    let current = self
                        .work_context_drag_current
                        .take()
                        .unwrap_or(Position::new(mouse.column, mouse.row));
                    let Some((anchor_point, current_point)) =
                        widgets::work_context_view::selection_points_from_mouse(
                            chat_area,
                            &self.tasks,
                            self.chat.active_thread_id(),
                            self.sidebar.active_tab(),
                            self.sidebar.selected_item(),
                            &self.theme,
                            self.task_view_scroll,
                            anchor,
                            current,
                        )
                    else {
                        return;
                    };

                    if anchor_point != current_point {
                        if let Some(text) = widgets::work_context_view::selected_text(
                            chat_area,
                            &self.tasks,
                            self.chat.active_thread_id(),
                            self.sidebar.active_tab(),
                            self.sidebar.selected_item(),
                            &self.theme,
                            self.task_view_scroll,
                            anchor_point,
                            current_point,
                        ) {
                            conversion::copy_to_clipboard(&text);
                            self.status_line = "Copied selection to clipboard".to_string();
                        }
                    }
                }
            }
            MouseEventKind::Down(MouseButton::Right) => {
                if let Ok(text) = arboard::Clipboard::new().and_then(|mut cb| cb.get_text()) {
                    if !text.is_empty() {
                        self.handle_paste(text);
                    }
                }
            }
            _ => {}
        }
    }

    fn clear_chat_drag_selection(&mut self) {
        self.chat_drag_anchor = None;
        self.chat_drag_current = None;
    }

    fn clear_work_context_drag_selection(&mut self) {
        self.work_context_drag_anchor = None;
        self.work_context_drag_current = None;
    }

    fn byte_offset_for_display_col(text: &str, target_col: usize) -> usize {
        use unicode_width::UnicodeWidthChar;

        let mut used = 0usize;
        for (idx, ch) in text.char_indices() {
            let width = UnicodeWidthChar::width(ch).unwrap_or(0);
            if target_col <= used || target_col < used + width {
                return idx;
            }
            used += width;
        }
        text.len()
    }

    fn input_offset_from_mouse(&self, input_start_row: u16, mouse: MouseEvent) -> Option<usize> {
        let inner_width = self.input_wrap_width();
        if inner_width == 0 {
            return Some(0);
        }

        let inner_row = mouse.row.saturating_sub(input_start_row + 1) as usize;
        let inner_col = mouse.column.saturating_sub(2) as usize;
        let attachment_rows = self.attachments.len();
        if inner_row < attachment_rows {
            return None;
        }

        let target_visual_row = inner_row - attachment_rows;
        let wrapped = self.input.wrapped_display_buffer(inner_width);
        if wrapped.is_empty() {
            return Some(0);
        }

        let mut wrapped_offset = 0usize;
        for (row_idx, line) in wrapped.split('\n').enumerate() {
            if row_idx == target_visual_row {
                let capped_col = inner_col.min(inner_width);
                let byte_in_line = Self::byte_offset_for_display_col(line, capped_col);
                return Some(self.input.wrapped_display_offset_to_buffer_offset(
                    wrapped_offset + byte_in_line,
                    inner_width,
                ));
            }
            wrapped_offset += line.len() + 1;
        }

        Some(self.input.buffer().len())
    }

    fn handle_chat_click(&mut self, chat_area: Rect, mouse: Position) {
        match widgets::chat::hit_test(chat_area, &self.chat, &self.theme, mouse) {
            Some(chat::ChatHitTarget::Message(idx)) => self.chat.toggle_message_selection(idx),
            Some(chat::ChatHitTarget::ReasoningToggle(idx)) => {
                self.chat.select_message(Some(idx));
                self.chat.toggle_reasoning(idx);
            }
            Some(chat::ChatHitTarget::ToolToggle(idx)) => {
                self.chat.select_message(Some(idx));
                self.chat.toggle_tool_expansion(idx);
            }
            Some(chat::ChatHitTarget::CopyMessage(idx)) => {
                self.chat.select_message(Some(idx));
                self.copy_message(idx);
            }
            Some(chat::ChatHitTarget::ResendMessage(idx)) => {
                self.chat.select_message(Some(idx));
                self.resend_message(idx);
            }
            Some(chat::ChatHitTarget::RegenerateMessage(idx)) => {
                self.chat.select_message(Some(idx));
                self.regenerate_from_message(idx);
            }
            None => {}
        }
    }

    fn modal_navigate_to(&mut self, target: usize) {
        let current = self.modal.picker_cursor();
        self.modal
            .reduce(modal::ModalAction::Navigate(target as i32 - current as i32));
    }

    pub(super) fn settings_navigate_to(&mut self, target: usize) {
        let current = self.settings.field_cursor();
        self.settings.reduce(SettingsAction::NavigateField(
            target as i32 - current as i32,
        ));
    }

    fn handle_modal_mouse(&mut self, mouse: MouseEvent) {
        let Some((kind, overlay_area)) = self.current_modal_area() else {
            return;
        };

        let inside = mouse.column >= overlay_area.x
            && mouse.column < overlay_area.x.saturating_add(overlay_area.width)
            && mouse.row >= overlay_area.y
            && mouse.row < overlay_area.y.saturating_add(overlay_area.height);

        match mouse.kind {
            MouseEventKind::ScrollUp if inside => match kind {
                modal::ModalKind::CommandPalette
                | modal::ModalKind::ThreadPicker
                | modal::ModalKind::GoalPicker
                | modal::ModalKind::ProviderPicker
                | modal::ModalKind::ModelPicker
                | modal::ModalKind::OpenAIAuth
                | modal::ModalKind::EffortPicker => {
                    self.modal.reduce(modal::ModalAction::Navigate(-1));
                }
                _ => {}
            },
            MouseEventKind::ScrollDown if inside => match kind {
                modal::ModalKind::CommandPalette
                | modal::ModalKind::ThreadPicker
                | modal::ModalKind::GoalPicker
                | modal::ModalKind::ProviderPicker
                | modal::ModalKind::ModelPicker
                | modal::ModalKind::OpenAIAuth
                | modal::ModalKind::EffortPicker => {
                    self.modal.reduce(modal::ModalAction::Navigate(1));
                }
                _ => {}
            },
            MouseEventKind::Down(MouseButton::Left) if !inside => {
                if matches!(
                    kind,
                    modal::ModalKind::Help
                        | modal::ModalKind::CommandPalette
                        | modal::ModalKind::ThreadPicker
                        | modal::ModalKind::GoalPicker
                        | modal::ModalKind::ProviderPicker
                        | modal::ModalKind::ModelPicker
                        | modal::ModalKind::OpenAIAuth
                        | modal::ModalKind::ErrorViewer
                        | modal::ModalKind::EffortPicker
                ) {
                    self.modal.reduce(modal::ModalAction::Pop);
                }
            }
            MouseEventKind::Down(MouseButton::Right) if inside => {
                if let Ok(text) = arboard::Clipboard::new().and_then(|mut cb| cb.get_text()) {
                    if !text.is_empty() {
                        self.handle_paste(text);
                    }
                }
            }
            MouseEventKind::Down(MouseButton::Left) => match kind {
                modal::ModalKind::Settings => {
                    match widgets::settings::hit_test(
                        overlay_area,
                        &self.settings,
                        &self.config,
                        &self.auth,
                        &self.subagents,
                        Position::new(mouse.column, mouse.row),
                    ) {
                        Some(widgets::settings::SettingsHitTarget::EditCursor { line, col }) => {
                            self.settings
                                .reduce(SettingsAction::SetCursorLineCol(line, col));
                        }
                        Some(widgets::settings::SettingsHitTarget::Tab(tab)) => {
                            if self.settings.is_editing() {
                                return;
                            }
                            self.settings.reduce(SettingsAction::SwitchTab(tab));
                            if matches!(tab, SettingsTab::SubAgents) {
                                self.send_daemon_command(DaemonCommand::ListSubAgents);
                            } else if matches!(tab, SettingsTab::Concierge) {
                                self.send_daemon_command(DaemonCommand::GetConciergeConfig);
                            }
                        }
                        Some(widgets::settings::SettingsHitTarget::AuthProviderItem(index)) => {
                            if self.settings.is_editing() {
                                return;
                            }
                            self.auth.selected = index.min(self.auth.entries.len().saturating_sub(1));
                            self.auth.actions_focused = false;
                        }
                        Some(widgets::settings::SettingsHitTarget::AuthAction { index, action }) => {
                            if self.settings.is_editing() {
                                return;
                            }
                            self.auth.selected = index.min(self.auth.entries.len().saturating_sub(1));
                            self.auth.actions_focused = true;
                            self.auth.action_cursor = match action {
                                widgets::settings::AuthTabAction::Primary => 0,
                                widgets::settings::AuthTabAction::Test => 1,
                            };
                            self.run_auth_tab_action();
                        }
                        Some(widgets::settings::SettingsHitTarget::SubAgentListItem(index)) => {
                            if self.settings.is_editing() {
                                return;
                            }
                            self.subagents
                                .reduce(crate::state::subagents::SubAgentsAction::Select(index));
                            self.subagents.actions_focused = false;
                        }
                        Some(widgets::settings::SettingsHitTarget::SubAgentAction(action)) => {
                            if self.settings.is_editing() {
                                return;
                            }
                            self.subagents.actions_focused = true;
                            self.subagents.action_cursor = match action {
                                widgets::settings::SubAgentTabAction::Add => 0,
                                widgets::settings::SubAgentTabAction::Edit => 1,
                                widgets::settings::SubAgentTabAction::Delete => 2,
                                widgets::settings::SubAgentTabAction::Toggle => 3,
                            };
                            self.run_subagent_action();
                        }
                        Some(widgets::settings::SettingsHitTarget::SubAgentRowAction {
                            index,
                            action,
                        }) => {
                            if self.settings.is_editing() {
                                return;
                            }
                            self.subagents
                                .reduce(crate::state::subagents::SubAgentsAction::Select(index));
                            self.subagents.actions_focused = true;
                            self.subagents.action_cursor = match action {
                                widgets::settings::SubAgentTabAction::Add => 0,
                                widgets::settings::SubAgentTabAction::Edit => 1,
                                widgets::settings::SubAgentTabAction::Delete => 2,
                                widgets::settings::SubAgentTabAction::Toggle => 3,
                            };
                            self.run_subagent_action();
                        }
                        Some(widgets::settings::SettingsHitTarget::Field(field)) => {
                            if self.settings.is_editing() {
                                return;
                            }
                            self.settings_navigate_to(field);
                            if self.settings_field_click_uses_toggle() {
                                self.toggle_settings_field();
                            } else {
                                self.activate_settings_field();
                            }
                        }
                        None => {}
                    }
                }
                modal::ModalKind::CommandPalette => {
                    let inner = Block::default()
                        .borders(Borders::ALL)
                        .border_type(BorderType::Double)
                        .inner(overlay_area);
                    let chunks = Layout::default()
                        .direction(Direction::Vertical)
                        .constraints([
                            Constraint::Length(1),
                            Constraint::Length(1),
                            Constraint::Min(1),
                            Constraint::Length(1),
                        ])
                        .split(inner);
                    if mouse.row >= chunks[2].y
                        && mouse.row < chunks[2].y.saturating_add(chunks[2].height)
                    {
                        let idx = mouse.row.saturating_sub(chunks[2].y) as usize;
                        if idx < self.modal.filtered_items().len() {
                            self.modal_navigate_to(idx);
                            self.handle_modal_enter(kind);
                        }
                    }
                }
                modal::ModalKind::ThreadPicker => {
                    let inner = Block::default()
                        .borders(Borders::ALL)
                        .border_type(BorderType::Double)
                        .inner(overlay_area);
                    let chunks = Layout::default()
                        .direction(Direction::Vertical)
                        .constraints([
                            Constraint::Length(1),
                            Constraint::Length(1),
                            Constraint::Min(1),
                            Constraint::Length(1),
                        ])
                        .split(inner);
                    if mouse.row >= chunks[2].y
                        && mouse.row < chunks[2].y.saturating_add(chunks[2].height)
                    {
                        let row_idx = mouse.row.saturating_sub(chunks[2].y) as usize;
                        let query = self.modal.command_query().to_lowercase();
                        let filtered_threads = self
                            .chat
                            .threads()
                            .iter()
                            .filter(|thread| {
                                query.is_empty() || thread.title.to_lowercase().contains(&query)
                            })
                            .count();
                        let total_items = filtered_threads + 1;
                        let (visible_start, visible_len) = widgets::thread_picker::visible_window(
                            self.modal.picker_cursor(),
                            total_items,
                            chunks[2].height as usize,
                        );
                        if row_idx < visible_len {
                            let idx = visible_start + row_idx;
                            self.modal_navigate_to(idx);
                            self.handle_modal_enter(kind);
                        }
                    }
                }
                modal::ModalKind::GoalPicker => {
                    let inner = Block::default()
                        .borders(Borders::ALL)
                        .border_type(BorderType::Double)
                        .inner(overlay_area);
                    let chunks = Layout::default()
                        .direction(Direction::Vertical)
                        .constraints([
                            Constraint::Length(1),
                            Constraint::Length(1),
                            Constraint::Min(1),
                            Constraint::Length(1),
                        ])
                        .split(inner);
                    if mouse.row >= chunks[2].y
                        && mouse.row < chunks[2].y.saturating_add(chunks[2].height)
                    {
                        let row_idx = mouse.row.saturating_sub(chunks[2].y) as usize;
                        let total_items = self.filtered_goal_runs().len() + 1;
                        let (visible_start, visible_len) = widgets::thread_picker::visible_window(
                            self.modal.picker_cursor(),
                            total_items,
                            chunks[2].height as usize,
                        );
                        if row_idx < visible_len {
                            self.modal_navigate_to(visible_start + row_idx);
                            self.handle_modal_enter(kind);
                        }
                    }
                }
                modal::ModalKind::ProviderPicker => {
                    let inner = Block::default()
                        .borders(Borders::ALL)
                        .border_type(BorderType::Double)
                        .inner(overlay_area);
                    if mouse.row >= inner.y
                        && mouse.row < inner.y.saturating_add(inner.height.saturating_sub(1))
                    {
                        let idx = mouse.row.saturating_sub(inner.y) as usize;
                        if idx < providers::PROVIDERS.len() {
                            self.modal_navigate_to(idx);
                            self.handle_modal_enter(kind);
                        }
                    }
                }
                modal::ModalKind::ModelPicker => {
                    let inner = Block::default()
                        .borders(Borders::ALL)
                        .border_type(BorderType::Double)
                        .inner(overlay_area);
                    if mouse.row >= inner.y
                        && mouse.row < inner.y.saturating_add(inner.height.saturating_sub(1))
                    {
                        let idx = mouse.row.saturating_sub(inner.y) as usize;
                        if idx <= widgets::model_picker::available_models(&self.config).len() {
                            self.modal_navigate_to(idx);
                            self.handle_modal_enter(kind);
                        }
                    }
                }
                modal::ModalKind::OpenAIAuth => {}
                modal::ModalKind::EffortPicker => {
                    let inner = Block::default()
                        .borders(Borders::ALL)
                        .border_type(BorderType::Double)
                        .inner(overlay_area);
                    if mouse.row >= inner.y
                        && mouse.row < inner.y.saturating_add(inner.height.saturating_sub(1))
                    {
                        let idx = mouse.row.saturating_sub(inner.y) as usize;
                        if idx < 5 {
                            self.modal_navigate_to(idx);
                            self.handle_modal_enter(kind);
                        }
                    }
                }
                modal::ModalKind::ApprovalOverlay => {
                    let inner = Block::default()
                        .borders(Borders::ALL)
                        .border_type(BorderType::Double)
                        .inner(overlay_area);
                    let action_row = inner.y.saturating_add(inner.height.saturating_sub(1));
                    if mouse.row == action_row {
                        let third = inner.width / 3;
                        let rel_x = mouse.column.saturating_sub(inner.x);
                        let key = if rel_x < third {
                            KeyCode::Char('y')
                        } else if rel_x < third.saturating_mul(2) {
                            KeyCode::Char('a')
                        } else {
                            KeyCode::Char('n')
                        };
                        let _ = self.handle_key_modal(key, KeyModifiers::NONE, kind);
                    }
                }
                modal::ModalKind::Help => {
                    self.modal.reduce(modal::ModalAction::Pop);
                }
                _ => {}
            },
            _ => {}
        }
    }
}
