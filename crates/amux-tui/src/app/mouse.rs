use super::*;

#[path = "mouse_helpers.rs"]
mod helpers;

use helpers::contains_mouse;

impl TuiModel {
    pub fn handle_resize(&mut self, width: u16, height: u16) {
        self.width = width;
        self.height = height;
        self.show_sidebar_override = None;
        self.clear_chat_drag_selection();
        self.clear_work_context_drag_selection();
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

        let header_area = Rect::new(0, 0, self.width, 3);
        if matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left)) {
            if let Some(widgets::header::HeaderHitTarget::NotificationBell) =
                widgets::header::hit_test(
                    header_area,
                    self.notifications.unread_count(),
                    Position::new(mouse.column, mouse.row),
                )
            {
                self.toggle_notifications_modal();
                self.input.set_mode(input::InputMode::Insert);
                return;
            }
        }

        let status_area = Rect::new(0, self.height.saturating_sub(1), self.width, 1);
        if matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left)) {
            if widgets::footer::status_bar_hit_test(
                status_area,
                self.connected,
                self.last_error.is_some(),
                self.queued_prompts.len(),
                Position::new(mouse.column, mouse.row),
            ) == Some(widgets::footer::StatusBarHitTarget::QueuedPrompts)
            {
                self.open_queued_prompts_modal();
                self.input.set_mode(input::InputMode::Insert);
                return;
            }
        }

        let layout = self.pane_layout();
        let chat_area = layout.chat;
        let sidebar_area = layout.sidebar.unwrap_or_default();
        let cursor_in_concierge =
            layout.concierge.height > 0 && contains_mouse(layout.concierge, mouse);
        let cursor_in_sidebar = layout
            .sidebar
            .is_some_and(|rect| contains_mouse(rect, mouse));
        let cursor_in_chat = contains_mouse(chat_area, mouse);
        let cursor_in_input = contains_mouse(layout.input, mouse);
        let concierge_area = layout.concierge;

        match mouse.kind {
            MouseEventKind::ScrollUp => {
                if cursor_in_chat {
                    if matches!(
                        self.main_pane_view,
                        MainPaneView::Task(_)
                            | MainPaneView::WorkContext
                            | MainPaneView::FilePreview(_)
                    ) {
                        self.task_view_scroll = self.task_view_scroll.saturating_sub(3);
                        if self.work_context_drag_anchor.is_some()
                            && matches!(self.main_pane_view, MainPaneView::WorkContext)
                        {
                            let pos = Position::new(mouse.column, mouse.row);
                            self.work_context_drag_current = Some(pos);
                            self.work_context_drag_current_point =
                                widgets::work_context_view::selection_point_from_mouse(
                                    chat_area,
                                    &self.tasks,
                                    self.chat.active_thread_id(),
                                    self.sidebar.active_tab(),
                                    self.sidebar.selected_item(),
                                    &self.theme,
                                    self.task_view_scroll,
                                    pos,
                                );
                        }
                    } else {
                        self.chat.reduce(chat::ChatAction::ScrollChat(3));
                        if self.chat_drag_anchor.is_some() {
                            self.chat_selection_snapshot = widgets::chat::build_selection_snapshot(
                                chat_area,
                                &self.chat,
                                &self.theme,
                                self.tick_counter,
                                self.retry_wait_start_selected,
                            );
                            let pos = Position::new(mouse.column, mouse.row);
                            self.chat_drag_current = Some(pos);
                            self.chat_drag_current_point =
                                self.chat_selection_snapshot.as_ref().and_then(|snapshot| {
                                    widgets::chat::selection_point_from_cached_snapshot(
                                        snapshot, pos,
                                    )
                                });
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
                        MainPaneView::Task(_)
                            | MainPaneView::WorkContext
                            | MainPaneView::FilePreview(_)
                    ) {
                        self.task_view_scroll = self.task_view_scroll.saturating_add(3);
                        if self.work_context_drag_anchor.is_some()
                            && matches!(self.main_pane_view, MainPaneView::WorkContext)
                        {
                            let pos = Position::new(mouse.column, mouse.row);
                            self.work_context_drag_current = Some(pos);
                            self.work_context_drag_current_point =
                                widgets::work_context_view::selection_point_from_mouse(
                                    chat_area,
                                    &self.tasks,
                                    self.chat.active_thread_id(),
                                    self.sidebar.active_tab(),
                                    self.sidebar.selected_item(),
                                    &self.theme,
                                    self.task_view_scroll,
                                    pos,
                                );
                        }
                    } else {
                        self.chat.reduce(chat::ChatAction::ScrollChat(-3));
                        if self.chat_drag_anchor.is_some() {
                            self.chat_selection_snapshot = widgets::chat::build_selection_snapshot(
                                chat_area,
                                &self.chat,
                                &self.theme,
                                self.tick_counter,
                                self.retry_wait_start_selected,
                            );
                            let pos = Position::new(mouse.column, mouse.row);
                            self.chat_drag_current = Some(pos);
                            self.chat_drag_current_point =
                                self.chat_selection_snapshot.as_ref().and_then(|snapshot| {
                                    widgets::chat::selection_point_from_cached_snapshot(
                                        snapshot, pos,
                                    )
                                });
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
                            self.chat.active_actions(),
                            self.concierge.selected_action,
                            Position::new(mouse.column, mouse.row),
                        )
                    {
                        self.select_visible_concierge_action(index);
                        self.execute_concierge_action(index);
                    } else if self.chat.active_thread_id() == Some("concierge") {
                        self.focus = FocusArea::Chat;
                    }
                } else if cursor_in_chat {
                    self.focus = FocusArea::Chat;
                    if matches!(self.main_pane_view, MainPaneView::Conversation) {
                        let pos = Position::new(mouse.column, mouse.row);
                        if matches!(
                            widgets::chat::hit_test(
                                chat_area,
                                &self.chat,
                                &self.theme,
                                self.tick_counter,
                                pos,
                            ),
                            Some(chat::ChatHitTarget::RetryStartNow | chat::ChatHitTarget::RetryStop)
                        ) {
                            self.clear_chat_drag_selection();
                            self.handle_chat_click(chat_area, pos);
                            self.input.set_mode(input::InputMode::Insert);
                            return;
                        }
                        self.chat_selection_snapshot = widgets::chat::build_selection_snapshot(
                            chat_area,
                            &self.chat,
                            &self.theme,
                            self.tick_counter,
                            self.retry_wait_start_selected,
                        );
                        self.chat_drag_anchor = Some(pos);
                        self.chat_drag_current = Some(pos);
                        let point = self.chat_selection_snapshot.as_ref().and_then(|snapshot| {
                            widgets::chat::selection_point_from_cached_snapshot(snapshot, pos)
                        });
                        self.chat_drag_anchor_point = point;
                        self.chat_drag_current_point = point;
                    } else if matches!(self.main_pane_view, MainPaneView::WorkContext) {
                        if let Some(
                            widgets::work_context_view::WorkContextHitTarget::ClosePreview,
                        ) = widgets::work_context_view::hit_test(
                            chat_area,
                            &self.tasks,
                            self.chat.active_thread_id(),
                            self.sidebar.active_tab(),
                            self.sidebar.selected_item(),
                            Position::new(mouse.column, mouse.row),
                            &self.theme,
                        ) {
                            self.set_main_pane_conversation(FocusArea::Chat);
                            self.status_line = "Closed preview".to_string();
                            return;
                        }
                        let pos = Position::new(mouse.column, mouse.row);
                        self.work_context_drag_anchor = Some(pos);
                        self.work_context_drag_current = Some(pos);
                        let point = widgets::work_context_view::selection_point_from_mouse(
                            chat_area,
                            &self.tasks,
                            self.chat.active_thread_id(),
                            self.sidebar.active_tab(),
                            self.sidebar.selected_item(),
                            &self.theme,
                            self.task_view_scroll,
                            pos,
                        );
                        self.work_context_drag_anchor_point = point;
                        self.work_context_drag_current_point = point;
                    } else if let MainPaneView::FilePreview(target) = &self.main_pane_view {
                        if let Some(widgets::file_preview::FilePreviewHitTarget::ClosePreview) =
                            widgets::file_preview::hit_test(
                                chat_area,
                                &self.tasks,
                                target,
                                Position::new(mouse.column, mouse.row),
                                &self.theme,
                            )
                        {
                            self.set_main_pane_conversation(FocusArea::Chat);
                            self.status_line = "Closed preview".to_string();
                            return;
                        }
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
                            if let Some(thread_id) =
                                self.chat.active_thread_id().map(str::to_string)
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
                    if let Some(offset) = self.input_offset_from_mouse(layout.input.y, mouse) {
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
                    let mut scrolled = false;
                    if mouse.row <= chat_area.y.saturating_add(1) {
                        self.chat.reduce(chat::ChatAction::ScrollChat(1));
                        scrolled = true;
                    } else if mouse.row
                        >= chat_area
                            .y
                            .saturating_add(chat_area.height)
                            .saturating_sub(2)
                    {
                        self.chat.reduce(chat::ChatAction::ScrollChat(-1));
                        scrolled = true;
                    }
                    if scrolled || self.chat_selection_snapshot.is_none() {
                        self.chat_selection_snapshot = widgets::chat::build_selection_snapshot(
                            chat_area,
                            &self.chat,
                            &self.theme,
                            self.tick_counter,
                            self.retry_wait_start_selected,
                        );
                    }
                    let pos = Position::new(mouse.column, mouse.row);
                    self.chat_drag_current = Some(pos);
                    self.chat_drag_current_point =
                        self.chat_selection_snapshot.as_ref().and_then(|snapshot| {
                            widgets::chat::selection_point_from_cached_snapshot(snapshot, pos)
                        });
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
                    let pos = Position::new(mouse.column, mouse.row);
                    self.work_context_drag_current = Some(pos);
                    self.work_context_drag_current_point =
                        widgets::work_context_view::selection_point_from_mouse(
                            chat_area,
                            &self.tasks,
                            self.chat.active_thread_id(),
                            self.sidebar.active_tab(),
                            self.sidebar.selected_item(),
                            &self.theme,
                            self.task_view_scroll,
                            pos,
                        );
                }
            }
            MouseEventKind::Up(MouseButton::Left) => {
                if let Some(anchor) = self.chat_drag_anchor.take() {
                    let current = self.chat_drag_current.take().unwrap_or(anchor);
                    let snapshot = self.chat_selection_snapshot.take();
                    let anchor_point = self.chat_drag_anchor_point.take().or_else(|| {
                        snapshot.as_ref().and_then(|snapshot| {
                            widgets::chat::selection_point_from_cached_snapshot(snapshot, anchor)
                        })
                    });
                    let current_point = self.chat_drag_current_point.take().or_else(|| {
                        snapshot.as_ref().and_then(|snapshot| {
                            widgets::chat::selection_point_from_cached_snapshot(snapshot, current)
                        })
                    });
                    let Some((anchor_point, current_point)) = anchor_point.zip(current_point)
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
                        if let Some(text) = snapshot.as_ref().and_then(|snapshot| {
                            widgets::chat::selected_text_from_cached_snapshot(
                                snapshot,
                                anchor_point,
                                current_point,
                            )
                        }) {
                            conversion::copy_to_clipboard(&text);
                            self.status_line = "Copied selection to clipboard".to_string();
                        }
                    } else if cursor_in_chat {
                        self.handle_chat_click(chat_area, anchor);
                    }
                } else if let Some(anchor) = self.work_context_drag_anchor.take() {
                    let current = self
                        .work_context_drag_current
                        .take()
                        .unwrap_or(Position::new(mouse.column, mouse.row));
                    let anchor_point = self.work_context_drag_anchor_point.take().or_else(|| {
                        widgets::work_context_view::selection_point_from_mouse(
                            chat_area,
                            &self.tasks,
                            self.chat.active_thread_id(),
                            self.sidebar.active_tab(),
                            self.sidebar.selected_item(),
                            &self.theme,
                            self.task_view_scroll,
                            anchor,
                        )
                    });
                    let current_point = self.work_context_drag_current_point.take().or_else(|| {
                        widgets::work_context_view::selection_point_from_mouse(
                            chat_area,
                            &self.tasks,
                            self.chat.active_thread_id(),
                            self.sidebar.active_tab(),
                            self.sidebar.selected_item(),
                            &self.theme,
                            self.task_view_scroll,
                            current,
                        )
                    });
                    let Some((anchor_point, current_point)) = anchor_point.zip(current_point)
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
}
