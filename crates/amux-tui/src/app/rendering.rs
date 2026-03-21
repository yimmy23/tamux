use super::*;

impl TuiModel {
    pub fn render(&self, frame: &mut Frame) {
        let area = frame.area();
        let width = area.width;
        let input_height = self.input_height();
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Min(1),
                Constraint::Length(input_height),
                Constraint::Length(1),
            ])
            .split(area);

        widgets::header::render(frame, chunks[0], &self.config, &self.chat, &self.theme);

        let show_sidebar = self.sidebar_visible();
        if show_sidebar {
            let sidebar_pct = if width >= 120 { 33 } else { 28 };
            let body_chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([
                    Constraint::Percentage(100 - sidebar_pct),
                    Constraint::Percentage(sidebar_pct),
                ])
                .split(chunks[1]);
            match &self.main_pane_view {
                MainPaneView::Conversation => widgets::chat::render(
                    frame,
                    body_chunks[0],
                    &self.chat,
                    &self.theme,
                    self.focus == FocusArea::Chat,
                    self.chat_drag_anchor.zip(self.chat_drag_current),
                ),
                MainPaneView::Task(target) => widgets::task_view::render(
                    frame,
                    body_chunks[0],
                    &self.tasks,
                    target,
                    &self.theme,
                    self.focus == FocusArea::Chat,
                    self.task_view_scroll,
                    self.task_show_live_todos,
                    self.task_show_timeline,
                    self.task_show_files,
                ),
                MainPaneView::WorkContext => widgets::work_context_view::render(
                    frame,
                    body_chunks[0],
                    &self.tasks,
                    self.chat.active_thread_id(),
                    self.sidebar.active_tab(),
                    self.sidebar.selected_item(),
                    &self.theme,
                    self.task_view_scroll,
                ),
                MainPaneView::GoalComposer => {
                    render_helpers::render_goal_composer(frame, body_chunks[0], &self.theme)
                }
            }
            widgets::sidebar::render(
                frame,
                body_chunks[1],
                &self.sidebar,
                &self.tasks,
                self.chat.active_thread_id(),
                &self.theme,
                self.focus == FocusArea::Sidebar,
            );
        } else {
            match &self.main_pane_view {
                MainPaneView::Conversation => widgets::chat::render(
                    frame,
                    chunks[1],
                    &self.chat,
                    &self.theme,
                    self.focus == FocusArea::Chat,
                    self.chat_drag_anchor.zip(self.chat_drag_current),
                ),
                MainPaneView::Task(target) => widgets::task_view::render(
                    frame,
                    chunks[1],
                    &self.tasks,
                    target,
                    &self.theme,
                    self.focus == FocusArea::Chat,
                    self.task_view_scroll,
                    self.task_show_live_todos,
                    self.task_show_timeline,
                    self.task_show_files,
                ),
                MainPaneView::WorkContext => widgets::work_context_view::render(
                    frame,
                    chunks[1],
                    &self.tasks,
                    self.chat.active_thread_id(),
                    self.sidebar.active_tab(),
                    self.sidebar.selected_item(),
                    &self.theme,
                    self.task_view_scroll,
                ),
                MainPaneView::GoalComposer => {
                    render_helpers::render_goal_composer(frame, chunks[1], &self.theme)
                }
            }
        }

        widgets::footer::render_input(
            frame,
            chunks[2],
            &self.input,
            &self.theme,
            self.focus == FocusArea::Input,
            self.modal.top().is_some(),
            &self.attachments,
            self.tick_counter,
            self.agent_activity.as_deref(),
            self.input_notice_style(),
        );
        widgets::footer::render_status_bar(
            frame,
            chunks[3],
            &self.theme,
            self.connected,
            self.last_error.is_some(),
            self.error_active,
            self.tick_counter,
            self.error_tick,
            self.queued_prompts.len(),
            &self.status_line,
        );

        if let Some(modal_kind) = self.modal.top() {
            let overlay_area = match modal_kind {
                modal::ModalKind::Settings => render_helpers::centered_rect(75, 80, area),
                modal::ModalKind::ApprovalOverlay => render_helpers::centered_rect(60, 40, area),
                modal::ModalKind::CommandPalette => render_helpers::centered_rect(50, 40, area),
                modal::ModalKind::ThreadPicker => render_helpers::centered_rect(60, 50, area),
                modal::ModalKind::GoalPicker => render_helpers::centered_rect(60, 50, area),
                modal::ModalKind::ProviderPicker => render_helpers::centered_rect(35, 65, area),
                modal::ModalKind::ModelPicker => render_helpers::centered_rect(45, 50, area),
                modal::ModalKind::OpenAIAuth => render_helpers::centered_rect(70, 35, area),
                modal::ModalKind::ErrorViewer => render_helpers::centered_rect(70, 45, area),
                modal::ModalKind::EffortPicker => render_helpers::centered_rect(35, 30, area),
                modal::ModalKind::ToolsPicker | modal::ModalKind::ViewPicker => {
                    render_helpers::centered_rect(40, 35, area)
                }
                modal::ModalKind::Help => render_helpers::centered_rect(70, 80, area),
            };
            frame.render_widget(Clear, overlay_area);

            match modal_kind {
                modal::ModalKind::CommandPalette => {
                    widgets::command_palette::render(frame, overlay_area, &self.modal, &self.theme);
                }
                modal::ModalKind::ThreadPicker => {
                    widgets::thread_picker::render(
                        frame,
                        overlay_area,
                        &self.chat,
                        &self.modal,
                        &self.theme,
                    );
                }
                modal::ModalKind::GoalPicker => {
                    widgets::goal_picker::render(
                        frame,
                        overlay_area,
                        &self.tasks,
                        &self.modal,
                        &self.theme,
                    );
                }
                modal::ModalKind::ApprovalOverlay => {
                    widgets::approval::render(frame, overlay_area, &self.approval, &self.theme);
                }
                modal::ModalKind::Settings => {
                    widgets::settings::render(
                        frame,
                        overlay_area,
                        &self.settings,
                        &self.config,
                        &self.theme,
                    );
                }
                modal::ModalKind::ProviderPicker => {
                    widgets::provider_picker::render(
                        frame,
                        overlay_area,
                        &self.modal,
                        &self.config,
                        &self.theme,
                    );
                }
                modal::ModalKind::ModelPicker => {
                    widgets::model_picker::render(
                        frame,
                        overlay_area,
                        &self.modal,
                        &self.config,
                        &self.theme,
                    );
                }
                modal::ModalKind::OpenAIAuth => {
                    render_helpers::render_openai_auth_modal(
                        frame,
                        overlay_area,
                        self.openai_auth_url.as_deref(),
                        self.openai_auth_status_text.as_deref(),
                        &self.theme,
                    );
                }
                modal::ModalKind::ErrorViewer => {
                    render_helpers::render_error_modal(
                        frame,
                        overlay_area,
                        self.last_error.as_deref(),
                        &self.theme,
                    );
                }
                modal::ModalKind::EffortPicker => {
                    render_helpers::render_effort_picker(
                        frame,
                        overlay_area,
                        &self.modal,
                        &self.config,
                        &self.theme,
                    );
                }
                modal::ModalKind::ToolsPicker | modal::ModalKind::ViewPicker => {}
                modal::ModalKind::Help => {
                    render_helpers::render_help_modal(frame, overlay_area, &self.theme);
                }
            }
        }
    }

    pub(super) fn current_modal_area(&self) -> Option<(modal::ModalKind, Rect)> {
        let kind = self.modal.top()?;
        let area = Rect::new(0, 0, self.width, self.height);
        let rect = match kind {
            modal::ModalKind::Settings => render_helpers::centered_rect(75, 80, area),
            modal::ModalKind::ApprovalOverlay => render_helpers::centered_rect(60, 40, area),
            modal::ModalKind::CommandPalette => render_helpers::centered_rect(50, 40, area),
            modal::ModalKind::ThreadPicker => render_helpers::centered_rect(60, 50, area),
            modal::ModalKind::GoalPicker => render_helpers::centered_rect(60, 50, area),
            modal::ModalKind::ProviderPicker => render_helpers::centered_rect(35, 65, area),
            modal::ModalKind::ModelPicker => render_helpers::centered_rect(45, 50, area),
            modal::ModalKind::OpenAIAuth => render_helpers::centered_rect(70, 35, area),
            modal::ModalKind::ErrorViewer => render_helpers::centered_rect(70, 45, area),
            modal::ModalKind::EffortPicker => render_helpers::centered_rect(35, 30, area),
            modal::ModalKind::ToolsPicker | modal::ModalKind::ViewPicker => {
                render_helpers::centered_rect(40, 35, area)
            }
            modal::ModalKind::Help => render_helpers::centered_rect(70, 80, area),
        };
        Some((kind, rect))
    }
}
