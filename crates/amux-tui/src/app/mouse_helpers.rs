use super::*;

impl TuiModel {
    pub(in super::super) fn clear_chat_drag_selection(&mut self) {
        self.chat_drag_anchor = None;
        self.chat_drag_current = None;
        self.chat_drag_anchor_point = None;
        self.chat_drag_current_point = None;
        self.chat_selection_snapshot = None;
        self.chat_scrollbar_drag_grab_offset = None;
    }

    pub(in super::super) fn set_chat_scroll_offset(&mut self, target: usize) {
        let current = self.chat.scroll_offset();
        if target == current {
            return;
        }

        let delta = if target > current {
            (target - current).min(i32::MAX as usize) as i32
        } else {
            -((current - target).min(i32::MAX as usize) as i32)
        };
        self.chat.reduce(chat::ChatAction::ScrollChat(delta));
    }

    pub(in super::super) fn clear_work_context_drag_selection(&mut self) {
        self.work_context_drag_anchor = None;
        self.work_context_drag_current = None;
        self.work_context_drag_anchor_point = None;
        self.work_context_drag_current_point = None;
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

    pub(super) fn input_offset_from_mouse(
        &self,
        input_start_row: u16,
        mouse: MouseEvent,
    ) -> Option<usize> {
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

    pub(super) fn handle_chat_click(&mut self, chat_area: Rect, mouse: Position) {
        match widgets::chat::hit_test(chat_area, &self.chat, &self.theme, self.tick_counter, mouse)
        {
            Some(chat::ChatHitTarget::Message(idx)) => self.chat.toggle_message_selection(idx),
            Some(chat::ChatHitTarget::ReasoningToggle(idx)) => {
                self.chat.select_message(Some(idx));
                self.chat.toggle_reasoning(idx);
            }
            Some(chat::ChatHitTarget::ToolToggle(idx)) => {
                self.chat.select_message(Some(idx));
                self.chat.toggle_tool_expansion(idx);
            }
            Some(chat::ChatHitTarget::ToolFilePath { message_index }) => {
                self.chat.select_message(Some(message_index));
                self.open_chat_tool_file_preview(message_index);
            }
            Some(chat::ChatHitTarget::RetryStartNow) => {
                if let Some(thread_id) = self.chat.active_thread_id().map(str::to_string) {
                    self.retry_wait_start_selected = true;
                    self.chat.reduce(chat::ChatAction::ClearRetryStatus {
                        thread_id: thread_id.clone(),
                    });
                    self.send_daemon_command(DaemonCommand::RetryStreamNow { thread_id });
                    self.status_line = "Retrying now...".to_string();
                    self.agent_activity = Some("retrying".to_string());
                }
            }
            Some(chat::ChatHitTarget::RetryStop) => {
                if let Some(thread_id) = self.chat.active_thread_id().map(str::to_string) {
                    self.retry_wait_start_selected = false;
                    self.cancelled_thread_id = Some(thread_id.clone());
                    self.chat.reduce(chat::ChatAction::ForceStopStreaming);
                    self.agent_activity = None;
                    self.send_daemon_command(DaemonCommand::StopStream { thread_id });
                    self.status_line = "Stopped retry loop".to_string();
                }
            }
            Some(chat::ChatHitTarget::MessageAction {
                message_index,
                action_index,
            }) => {
                self.chat.select_message(Some(message_index));
                self.chat.select_message_action(action_index);
                self.execute_concierge_message_action(message_index, action_index);
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
                self.request_regenerate_message(idx);
            }
            Some(chat::ChatHitTarget::DeleteMessage(idx)) => {
                self.chat.select_message(Some(idx));
                self.request_delete_message(idx);
            }
            None => {}
        }
    }

    pub(super) fn modal_navigate_to(&mut self, target: usize) {
        let current = self.modal.picker_cursor();
        self.modal
            .reduce(modal::ModalAction::Navigate(target as i32 - current as i32));
    }

    pub(in super::super) fn settings_navigate_to(&mut self, target: usize) {
        let current = self.settings.field_cursor();
        self.settings
            .navigate_field(target as i32 - current as i32, self.settings_field_count());
    }

    pub(super) fn handle_modal_mouse(&mut self, mouse: MouseEvent) {
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
                | modal::ModalKind::QueuedPrompts
                | modal::ModalKind::ProviderPicker
                | modal::ModalKind::ModelPicker
                | modal::ModalKind::OpenAIAuth
                | modal::ModalKind::EffortPicker => {
                    self.modal.reduce(modal::ModalAction::Navigate(-1));
                }
                modal::ModalKind::Notifications => {
                    self.notifications
                        .reduce(crate::state::NotificationsAction::FocusHeader(None));
                    self.notifications
                        .reduce(crate::state::NotificationsAction::FocusRowAction(None));
                    self.notifications
                        .reduce(crate::state::NotificationsAction::Navigate(-1));
                }
                modal::ModalKind::ApprovalCenter => {
                    self.step_approval_selection(-1);
                }
                modal::ModalKind::PromptViewer => {
                    self.step_prompt_modal_scroll(-3);
                }
                _ => {}
            },
            MouseEventKind::ScrollDown if inside => match kind {
                modal::ModalKind::CommandPalette
                | modal::ModalKind::ThreadPicker
                | modal::ModalKind::GoalPicker
                | modal::ModalKind::QueuedPrompts
                | modal::ModalKind::ProviderPicker
                | modal::ModalKind::ModelPicker
                | modal::ModalKind::OpenAIAuth
                | modal::ModalKind::EffortPicker => {
                    self.modal.reduce(modal::ModalAction::Navigate(1));
                }
                modal::ModalKind::Notifications => {
                    self.notifications
                        .reduce(crate::state::NotificationsAction::FocusHeader(None));
                    self.notifications
                        .reduce(crate::state::NotificationsAction::FocusRowAction(None));
                    self.notifications
                        .reduce(crate::state::NotificationsAction::Navigate(1));
                }
                modal::ModalKind::ApprovalCenter => {
                    self.step_approval_selection(1);
                }
                modal::ModalKind::PromptViewer => {
                    self.step_prompt_modal_scroll(3);
                }
                _ => {}
            },
            MouseEventKind::Down(MouseButton::Left) if !inside => {
                if matches!(
                    kind,
                    modal::ModalKind::Help
                        | modal::ModalKind::Status
                        | modal::ModalKind::PromptViewer
                        | modal::ModalKind::CommandPalette
                        | modal::ModalKind::ThreadPicker
                        | modal::ModalKind::GoalPicker
                        | modal::ModalKind::QueuedPrompts
                        | modal::ModalKind::ProviderPicker
                        | modal::ModalKind::ModelPicker
                        | modal::ModalKind::OpenAIAuth
                        | modal::ModalKind::ErrorViewer
                        | modal::ModalKind::Notifications
                        | modal::ModalKind::ApprovalCenter
                        | modal::ModalKind::EffortPicker
                        | modal::ModalKind::ChatActionConfirm
                ) {
                    if kind == modal::ModalKind::ChatActionConfirm {
                        self.close_chat_action_confirm();
                    } else {
                        self.close_top_modal();
                    }
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
                            } else if matches!(tab, SettingsTab::Gateway) {
                                self.send_daemon_command(DaemonCommand::WhatsAppLinkStatus);
                            } else if matches!(tab, SettingsTab::Plugins) {
                                self.plugin_settings.list_mode = true;
                                self.send_daemon_command(DaemonCommand::PluginList);
                            }
                        }
                        Some(widgets::settings::SettingsHitTarget::AuthProviderItem(index)) => {
                            if self.settings.is_editing() {
                                return;
                            }
                            self.auth.selected =
                                index.min(self.auth.entries.len().saturating_sub(1));
                            self.auth.actions_focused = false;
                        }
                        Some(widgets::settings::SettingsHitTarget::AuthAction {
                            index,
                            action,
                        }) => {
                            if self.settings.is_editing() {
                                return;
                            }
                            self.auth.selected =
                                index.min(self.auth.entries.len().saturating_sub(1));
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
                modal::ModalKind::Notifications => {
                    if let Some(target) = widgets::notifications::hit_test(
                        overlay_area,
                        &self.notifications,
                        Position::new(mouse.column, mouse.row),
                    ) {
                        match target {
                            widgets::notifications::NotificationsHitTarget::MarkAllRead => {
                                self.notifications.reduce(
                                    crate::state::NotificationsAction::FocusHeader(Some(
                                        crate::state::NotificationsHeaderAction::MarkAllRead,
                                    )),
                                );
                                self.mark_all_notifications_read();
                            }
                            widgets::notifications::NotificationsHitTarget::ArchiveRead => {
                                self.notifications.reduce(
                                    crate::state::NotificationsAction::FocusHeader(Some(
                                        crate::state::NotificationsHeaderAction::ArchiveRead,
                                    )),
                                );
                                self.archive_read_notifications();
                            }
                            widgets::notifications::NotificationsHitTarget::Close => {
                                self.notifications.reduce(
                                    crate::state::NotificationsAction::FocusHeader(Some(
                                        crate::state::NotificationsHeaderAction::Close,
                                    )),
                                );
                                self.close_top_modal();
                            }
                            widgets::notifications::NotificationsHitTarget::Row(index) => {
                                self.notifications
                                    .reduce(crate::state::NotificationsAction::FocusHeader(None));
                                self.notifications.reduce(
                                    crate::state::NotificationsAction::FocusRowAction(None),
                                );
                                self.notifications
                                    .reduce(crate::state::NotificationsAction::Select(index));
                                if let Some(id) = self
                                    .notifications
                                    .selected_item()
                                    .map(|notification| notification.id.clone())
                                {
                                    self.toggle_notification_expand(id);
                                }
                            }
                            widgets::notifications::NotificationsHitTarget::ToggleExpand(id) => {
                                self.notifications.reduce(
                                    crate::state::NotificationsAction::FocusRowAction(Some(0)),
                                );
                                self.execute_notification_row_action(&id, 0);
                            }
                            widgets::notifications::NotificationsHitTarget::MarkRead(id) => {
                                self.notifications.reduce(
                                    crate::state::NotificationsAction::FocusRowAction(Some(1)),
                                );
                                self.execute_notification_row_action(&id, 1);
                            }
                            widgets::notifications::NotificationsHitTarget::Archive(id) => {
                                self.notifications.reduce(
                                    crate::state::NotificationsAction::FocusRowAction(Some(2)),
                                );
                                self.execute_notification_row_action(&id, 2);
                            }
                            widgets::notifications::NotificationsHitTarget::Delete(id) => {
                                self.notifications.reduce(
                                    crate::state::NotificationsAction::FocusRowAction(Some(3)),
                                );
                                self.execute_notification_row_action(&id, 3);
                            }
                            widgets::notifications::NotificationsHitTarget::Action {
                                notification_id,
                                action_index,
                            } => {
                                self.notifications.reduce(
                                    crate::state::NotificationsAction::FocusRowAction(Some(
                                        action_index + 4,
                                    )),
                                );
                                self.execute_notification_row_action(
                                    &notification_id,
                                    action_index + 4,
                                );
                            }
                        }
                    }
                }
                modal::ModalKind::ApprovalCenter => {
                    if let Some(target) = widgets::approval_center::hit_test(
                        overlay_area,
                        &self.approval,
                        self.chat.active_thread_id(),
                        self.current_workspace_id(),
                        Position::new(mouse.column, mouse.row),
                    ) {
                        match target {
                            widgets::approval_center::ApprovalCenterHitTarget::Filter(filter) => {
                                self.approval
                                    .reduce(crate::state::ApprovalAction::SetFilter(filter));
                            }
                            widgets::approval_center::ApprovalCenterHitTarget::Row(index) => {
                                self.select_approval_center_row(index);
                            }
                            widgets::approval_center::ApprovalCenterHitTarget::ThreadJump(
                                thread_id,
                            ) => {
                                self.open_thread_conversation(thread_id);
                                self.close_top_modal();
                            }
                            widgets::approval_center::ApprovalCenterHitTarget::ApproveOnce(
                                approval_id,
                            ) => {
                                self.resolve_approval(approval_id, "allow_once");
                            }
                            widgets::approval_center::ApprovalCenterHitTarget::ApproveSession(
                                approval_id,
                            ) => {
                                self.resolve_approval(approval_id, "allow_session");
                            }
                            widgets::approval_center::ApprovalCenterHitTarget::Deny(
                                approval_id,
                            ) => {
                                self.resolve_approval(approval_id, "reject");
                            }
                            widgets::approval_center::ApprovalCenterHitTarget::Close => {
                                self.close_top_modal();
                            }
                        }
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
                    match widgets::thread_picker::hit_test(
                        overlay_area,
                        &self.chat,
                        &self.modal,
                        Position::new(mouse.column, mouse.row),
                    ) {
                        Some(widgets::thread_picker::ThreadPickerHitTarget::Tab(tab)) => {
                            self.modal.set_thread_picker_tab(tab);
                            self.sync_thread_picker_item_count();
                        }
                        Some(widgets::thread_picker::ThreadPickerHitTarget::Item(idx)) => {
                            self.modal_navigate_to(idx);
                            self.handle_modal_enter(kind);
                        }
                        None => {}
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
                modal::ModalKind::QueuedPrompts => {
                    if let Some(target) = widgets::queued_prompts::hit_test(
                        overlay_area,
                        &self.queued_prompts,
                        self.modal.picker_cursor(),
                        self.tick_counter,
                        Position::new(mouse.column, mouse.row),
                    ) {
                        match target {
                            widgets::queued_prompts::QueuedPromptsHitTarget::Row(index) => {
                                self.modal_navigate_to(index);
                            }
                            widgets::queued_prompts::QueuedPromptsHitTarget::Action {
                                message_index,
                                action,
                            } => {
                                self.modal_navigate_to(message_index);
                                self.queued_prompt_action = action;
                                self.execute_selected_queued_prompt_action();
                            }
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
                modal::ModalKind::ChatActionConfirm => {
                    if let Some((confirm_rect, cancel_rect)) =
                        render_helpers::chat_action_confirm_button_bounds(overlay_area)
                    {
                        if contains_mouse(confirm_rect, mouse) {
                            self.chat_action_confirm_accept_selected = true;
                        } else if contains_mouse(cancel_rect, mouse) {
                            self.chat_action_confirm_accept_selected = false;
                        }
                    }
                }
                modal::ModalKind::Help => {
                    self.close_top_modal();
                }
                _ => {}
            },
            MouseEventKind::Up(MouseButton::Left)
                if kind == modal::ModalKind::ChatActionConfirm =>
            {
                if let Some((confirm_rect, cancel_rect)) =
                    render_helpers::chat_action_confirm_button_bounds(overlay_area)
                {
                    if contains_mouse(confirm_rect, mouse) {
                        self.chat_action_confirm_accept_selected = true;
                        self.confirm_pending_chat_action();
                    } else if contains_mouse(cancel_rect, mouse) {
                        self.chat_action_confirm_accept_selected = false;
                        self.close_chat_action_confirm();
                    }
                }
            }
            _ => {}
        }
    }
}

pub(super) fn contains_mouse(rect: Rect, mouse: MouseEvent) -> bool {
    rect.width > 0
        && rect.height > 0
        && mouse.column >= rect.x
        && mouse.column < rect.x.saturating_add(rect.width)
        && mouse.row >= rect.y
        && mouse.row < rect.y.saturating_add(rect.height)
}
