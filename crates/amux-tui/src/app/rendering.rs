use super::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ConversationAgentProfile {
    pub(crate) agent_label: String,
    pub(crate) provider: String,
    pub(crate) model: String,
    pub(crate) reasoning_effort: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ConversationAgentKind {
    Swarog,
    Rarog,
    Weles,
}

impl TuiModel {
    fn configured_model_label(model: &str, custom_model_name: &str) -> String {
        let custom = custom_model_name.trim();
        if !custom.is_empty() && custom != model {
            custom.to_string()
        } else if model.trim().is_empty() {
            "no model".to_string()
        } else {
            model.to_string()
        }
    }

    fn current_conversation_agent_kind(&self) -> ConversationAgentKind {
        if let Some(thread) = self.chat.active_thread() {
            if thread.id == "concierge"
                || thread
                    .agent_name
                    .as_deref()
                    .is_some_and(|name| name.eq_ignore_ascii_case(amux_protocol::AGENT_NAME_RAROG))
                || widgets::thread_picker::is_rarog_thread(thread)
            {
                return ConversationAgentKind::Rarog;
            }

            if thread
                .agent_name
                .as_deref()
                .is_some_and(|name| name.eq_ignore_ascii_case("weles"))
                || widgets::thread_picker::is_weles_thread(thread)
            {
                return ConversationAgentKind::Weles;
            }

            return ConversationAgentKind::Swarog;
        }

        match self.pending_new_thread_target_agent.as_deref() {
            Some(agent_id) if agent_id == amux_protocol::AGENT_ID_RAROG => {
                ConversationAgentKind::Rarog
            }
            Some("weles") => ConversationAgentKind::Weles,
            _ => ConversationAgentKind::Swarog,
        }
    }

    fn weles_profile(&self) -> ConversationAgentProfile {
        if let Some(entry) = self.subagents.entries.iter().find(|entry| {
            entry.id.eq_ignore_ascii_case("weles_builtin")
                || entry.name.eq_ignore_ascii_case("weles")
        }) {
            return ConversationAgentProfile {
                agent_label: "Weles".to_string(),
                provider: entry.provider.clone(),
                model: entry.model.clone(),
                reasoning_effort: entry
                    .reasoning_effort
                    .clone()
                    .filter(|value| !value.is_empty()),
            };
        }

        let raw_weles = self
            .config
            .agent_config_raw
            .as_ref()
            .and_then(|raw| raw.get("builtin_sub_agents"))
            .and_then(|value| value.get("weles"));

        let provider = raw_weles
            .and_then(|value| value.get("provider"))
            .and_then(|value| value.as_str())
            .filter(|value| !value.is_empty())
            .unwrap_or(self.config.compaction_weles_provider.as_str())
            .to_string();
        let model = raw_weles
            .and_then(|value| value.get("model"))
            .and_then(|value| value.as_str())
            .filter(|value| !value.is_empty())
            .unwrap_or(self.config.compaction_weles_model.as_str())
            .to_string();
        let reasoning_effort = raw_weles
            .and_then(|value| value.get("reasoning_effort"))
            .and_then(|value| value.as_str())
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .or_else(|| {
                (!self
                    .config
                    .compaction_weles_reasoning_effort
                    .trim()
                    .is_empty())
                .then(|| self.config.compaction_weles_reasoning_effort.clone())
            });

        ConversationAgentProfile {
            agent_label: "Weles".to_string(),
            provider,
            model: if model.trim().is_empty() {
                "no model".to_string()
            } else {
                model
            },
            reasoning_effort,
        }
    }

    pub(crate) fn current_conversation_agent_profile(&self) -> ConversationAgentProfile {
        match self.current_conversation_agent_kind() {
            ConversationAgentKind::Swarog => ConversationAgentProfile {
                agent_label: amux_protocol::AGENT_NAME_SWAROG.to_string(),
                provider: self.config.provider.clone(),
                model: Self::configured_model_label(
                    &self.config.model,
                    &self.config.custom_model_name,
                ),
                reasoning_effort: (!self.config.reasoning_effort.trim().is_empty())
                    .then(|| self.config.reasoning_effort.clone()),
            },
            ConversationAgentKind::Rarog => {
                let provider = self
                    .concierge
                    .provider
                    .as_deref()
                    .filter(|value| !value.is_empty())
                    .zip(
                        self.concierge
                            .model
                            .as_deref()
                            .filter(|value| !value.is_empty()),
                    );

                if let Some((provider, model)) = provider {
                    ConversationAgentProfile {
                        agent_label: amux_protocol::AGENT_NAME_RAROG.to_string(),
                        provider: provider.to_string(),
                        model: model.to_string(),
                        reasoning_effort: self
                            .concierge
                            .reasoning_effort
                            .clone()
                            .filter(|value| !value.is_empty()),
                    }
                } else {
                    ConversationAgentProfile {
                        agent_label: amux_protocol::AGENT_NAME_RAROG.to_string(),
                        provider: self.config.provider.clone(),
                        model: Self::configured_model_label(
                            &self.config.model,
                            &self.config.custom_model_name,
                        ),
                        reasoning_effort: (!self.config.reasoning_effort.trim().is_empty())
                            .then(|| self.config.reasoning_effort.clone()),
                    }
                }
            }
            ConversationAgentKind::Weles => self.weles_profile(),
        }
    }

    pub(crate) fn current_header_agent_profile(&self) -> ConversationAgentProfile {
        let fallback = self.current_conversation_agent_profile();
        let Some(runtime) = self.chat.active_thread_runtime_metadata() else {
            return fallback;
        };

        ConversationAgentProfile {
            agent_label: fallback.agent_label,
            provider: runtime.provider.unwrap_or(fallback.provider),
            model: runtime.model.unwrap_or(fallback.model),
            reasoning_effort: runtime.reasoning_effort.or(fallback.reasoning_effort),
        }
    }

    fn render_conversation_panel(&mut self, frame: &mut Frame, area: Rect) {
        if self.should_show_operator_profile_onboarding() {
            let question = self.operator_profile.question.as_ref().map(|question| {
                widgets::operator_profile_onboarding::OperatorProfileQuestionView {
                    field_key: question.field_key.as_str(),
                    prompt: question.prompt.as_str(),
                    input_kind: question.input_kind.as_str(),
                    optional: question.optional,
                }
            });
            let progress = self.operator_profile.progress.as_ref().map(|progress| {
                widgets::operator_profile_onboarding::OperatorProfileProgressView {
                    answered: progress.answered,
                    remaining: progress.remaining,
                    completion_ratio: progress.completion_ratio,
                }
            });
            let view = widgets::operator_profile_onboarding::OperatorProfileOnboardingView {
                session_kind: self.operator_profile.session_kind.as_deref(),
                question,
                progress,
                loading: self.operator_profile.loading,
                warning: self.operator_profile.warning.as_deref(),
                input_value: self.input.buffer(),
                select_options: self.current_operator_profile_select_options(),
            };
            widgets::operator_profile_onboarding::render(frame, area, &view, &self.theme);
            return;
        }

        if self.should_show_provider_onboarding() {
            widgets::onboarding::render(frame, area, &self.config, &self.theme);
            return;
        }

        if self.should_show_local_landing() {
            let profile = self.current_conversation_agent_profile();
            widgets::landing::render(frame, area, &self.theme, &profile.agent_label);
            return;
        }

        if self.should_show_concierge_hero_loading() {
            widgets::concierge_loading::render(frame, area, &self.theme, self.tick_counter);
            return;
        }

        let mouse_selection = self
            .chat_drag_anchor_point
            .zip(self.chat_drag_current_point)
            .or_else(|| {
                let cached_snapshot = self
                    .chat_selection_snapshot
                    .as_ref()
                    .filter(|snapshot| widgets::chat::cached_snapshot_matches_area(snapshot, area));
                self.chat_drag_anchor.and_then(|anchor| {
                    self.chat_drag_current.and_then(|current| {
                        if let Some(snapshot) = cached_snapshot {
                            widgets::chat::selection_point_from_cached_snapshot(snapshot, anchor)
                                .zip(widgets::chat::selection_point_from_cached_snapshot(
                                    snapshot, current,
                                ))
                        } else {
                            widgets::chat::selection_points_from_mouse(
                                area,
                                &self.chat,
                                &self.theme,
                                self.tick_counter,
                                anchor,
                                current,
                                self.retry_wait_start_selected,
                            )
                        }
                    })
                })
            })
            .filter(|(start, end)| start != end);

        let active_drag_selection = self.chat_drag_anchor.is_some() && mouse_selection.is_some();
        if active_drag_selection {
            if let Some(snapshot) = self
                .chat_selection_snapshot
                .as_ref()
                .filter(|snapshot| widgets::chat::cached_snapshot_matches_area(snapshot, area))
            {
                widgets::chat::render_cached(
                    frame,
                    area,
                    &self.chat,
                    &self.theme,
                    snapshot,
                    mouse_selection,
                );
                return;
            }
        }

        if let Some(snapshot) = self.chat_selection_snapshot.as_ref().filter(|snapshot| {
            widgets::chat::cached_snapshot_matches_render(
                snapshot,
                area,
                &self.chat,
                self.tick_counter,
                self.retry_wait_start_selected,
            )
        }) {
            widgets::chat::render_cached(
                frame,
                area,
                &self.chat,
                &self.theme,
                snapshot,
                mouse_selection,
            );
            return;
        }

        self.chat_selection_snapshot = widgets::chat::build_selection_snapshot(
            area,
            &self.chat,
            &self.theme,
            self.tick_counter,
            self.retry_wait_start_selected,
        );

        if let Some(snapshot) = self.chat_selection_snapshot.as_ref() {
            widgets::chat::render_cached(
                frame,
                area,
                &self.chat,
                &self.theme,
                snapshot,
                mouse_selection,
            );
            return;
        }

        widgets::chat::render(
            frame,
            area,
            &self.chat,
            &self.theme,
            self.tick_counter,
            self.retry_wait_start_selected,
            self.focus == FocusArea::Chat,
            mouse_selection,
        );
    }

    pub fn render(&mut self, frame: &mut Frame) {
        let area = frame.area();
        self.width = area.width;
        self.height = area.height;
        let layout = self.pane_layout_for_area(area);
        let input_height = self.input_height();
        let anticipatory_height = self.anticipatory_banner_height();
        let concierge_height = self.concierge_banner_height();
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Min(1),
                Constraint::Length(anticipatory_height),
                Constraint::Length(concierge_height),
                Constraint::Length(input_height),
                Constraint::Length(1),
            ])
            .split(area);

        let profile = self.current_header_agent_profile();

        widgets::header::render(
            frame,
            chunks[0],
            &self.chat,
            &profile.provider,
            &profile.model,
            profile.reasoning_effort.as_deref(),
            &self.theme,
            self.approval.pending_approvals().len(),
            self.modal.top() == Some(modal::ModalKind::ApprovalCenter),
            self.notifications.unread_count(),
            self.modal.top() == Some(modal::ModalKind::Notifications),
        );

        if let Some(sidebar_area) = layout.sidebar {
            match &self.main_pane_view {
                MainPaneView::Conversation => {
                    self.render_conversation_panel(frame, layout.chat);
                }
                MainPaneView::Collaboration => widgets::collaboration_view::render(
                    frame,
                    layout.chat,
                    &self.collaboration,
                    &self.theme,
                    self.focus == FocusArea::Chat,
                ),
                MainPaneView::Task(target) => widgets::task_view::render(
                    frame,
                    layout.chat,
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
                    layout.chat,
                    &self.tasks,
                    self.chat.active_thread_id(),
                    self.sidebar.active_tab(),
                    self.sidebar.selected_item(),
                    &self.theme,
                    self.task_view_scroll,
                    self.work_context_drag_anchor_point
                        .zip(self.work_context_drag_current_point)
                        .or_else(|| {
                            self.work_context_drag_anchor.and_then(|anchor| {
                                self.work_context_drag_current.and_then(|current| {
                                    widgets::work_context_view::selection_points_from_mouse(
                                        layout.chat,
                                        &self.tasks,
                                        self.chat.active_thread_id(),
                                        self.sidebar.active_tab(),
                                        self.sidebar.selected_item(),
                                        &self.theme,
                                        self.task_view_scroll,
                                        anchor,
                                        current,
                                    )
                                })
                            })
                        }),
                ),
                MainPaneView::FilePreview(target) => widgets::file_preview::render(
                    frame,
                    layout.chat,
                    &self.tasks,
                    target,
                    &self.theme,
                    self.task_view_scroll,
                ),
                MainPaneView::GoalComposer => {
                    render_helpers::render_goal_composer(frame, layout.chat, &self.theme)
                }
            }
            widgets::sidebar::render(
                frame,
                sidebar_area,
                &self.sidebar,
                &self.tasks,
                self.chat.active_thread_id(),
                &self.theme,
                self.focus == FocusArea::Sidebar,
                &self.gateway_statuses,
                &self.tier,
                self.agent_activity.as_deref(),
                self.weles_health.as_ref(),
                &self.recent_actions,
            );
        } else {
            match &self.main_pane_view {
                MainPaneView::Conversation => self.render_conversation_panel(frame, layout.chat),
                MainPaneView::Collaboration => widgets::collaboration_view::render(
                    frame,
                    layout.chat,
                    &self.collaboration,
                    &self.theme,
                    self.focus == FocusArea::Chat,
                ),
                MainPaneView::Task(target) => widgets::task_view::render(
                    frame,
                    layout.chat,
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
                    layout.chat,
                    &self.tasks,
                    self.chat.active_thread_id(),
                    self.sidebar.active_tab(),
                    self.sidebar.selected_item(),
                    &self.theme,
                    self.task_view_scroll,
                    self.work_context_drag_anchor_point
                        .zip(self.work_context_drag_current_point)
                        .or_else(|| {
                            self.work_context_drag_anchor.and_then(|anchor| {
                                self.work_context_drag_current.and_then(|current| {
                                    widgets::work_context_view::selection_points_from_mouse(
                                        layout.chat,
                                        &self.tasks,
                                        self.chat.active_thread_id(),
                                        self.sidebar.active_tab(),
                                        self.sidebar.selected_item(),
                                        &self.theme,
                                        self.task_view_scroll,
                                        anchor,
                                        current,
                                    )
                                })
                            })
                        }),
                ),
                MainPaneView::FilePreview(target) => widgets::file_preview::render(
                    frame,
                    layout.chat,
                    &self.tasks,
                    target,
                    &self.theme,
                    self.task_view_scroll,
                ),
                MainPaneView::GoalComposer => {
                    render_helpers::render_goal_composer(frame, layout.chat, &self.theme)
                }
            }
        }

        if anticipatory_height > 0 {
            widgets::anticipatory::render(frame, chunks[2], &self.anticipatory, &self.theme);
        }

        if concierge_height > 0 {
            widgets::concierge::render(
                frame,
                chunks[3],
                &self.concierge,
                &self.chat,
                &self.theme,
                self.focus == FocusArea::Chat,
            );
        }

        widgets::footer::render_input(
            frame,
            chunks[4],
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
            chunks[5],
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
                modal::ModalKind::Settings => render_helpers::centered_rect(90, 88, area),
                modal::ModalKind::ApprovalOverlay => render_helpers::centered_rect(60, 40, area),
                modal::ModalKind::ApprovalCenter => render_helpers::centered_rect(86, 82, area),
                modal::ModalKind::ChatActionConfirm => render_helpers::centered_rect(48, 28, area),
                modal::ModalKind::CommandPalette => render_helpers::centered_rect(50, 40, area),
                modal::ModalKind::Status => render_helpers::centered_rect(72, 70, area),
                modal::ModalKind::PromptViewer => render_helpers::centered_rect(84, 84, area),
                modal::ModalKind::ThreadPicker => render_helpers::centered_rect(60, 50, area),
                modal::ModalKind::GoalPicker => render_helpers::centered_rect(60, 50, area),
                modal::ModalKind::QueuedPrompts => render_helpers::centered_rect(72, 42, area),
                modal::ModalKind::ProviderPicker => render_helpers::centered_rect(35, 65, area),
                modal::ModalKind::ModelPicker => render_helpers::centered_rect(45, 50, area),
                modal::ModalKind::OpenAIAuth => render_helpers::centered_rect(70, 35, area),
                modal::ModalKind::ErrorViewer => render_helpers::centered_rect(70, 45, area),
                modal::ModalKind::EffortPicker => render_helpers::centered_rect(35, 30, area),
                modal::ModalKind::Notifications => render_helpers::centered_rect(78, 78, area),
                modal::ModalKind::WhatsAppLink => render_helpers::centered_rect(70, 80, area),
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
                modal::ModalKind::QueuedPrompts => {
                    widgets::queued_prompts::render(
                        frame,
                        overlay_area,
                        &self.queued_prompts,
                        self.modal.picker_cursor(),
                        self.queued_prompt_action,
                        self.tick_counter,
                        &self.theme,
                    );
                }
                modal::ModalKind::ApprovalOverlay => {
                    widgets::approval::render(frame, overlay_area, &self.approval, &self.theme);
                }
                modal::ModalKind::ApprovalCenter => {
                    widgets::approval_center::render(
                        frame,
                        overlay_area,
                        &self.approval,
                        self.chat.active_thread_id(),
                        self.current_workspace_id(),
                        &self.theme,
                    );
                }
                modal::ModalKind::ChatActionConfirm => {
                    let pending = self.pending_chat_action_confirm.as_ref().map(|pending| {
                        let action = match pending.action {
                            PendingChatActionKind::Regenerate => "regenerate",
                            PendingChatActionKind::Delete => "delete",
                        };
                        (action, pending.message_index + 1)
                    });
                    render_helpers::render_chat_action_confirm_modal(
                        frame,
                        overlay_area,
                        pending,
                        self.chat_action_confirm_accept_selected,
                        &self.theme,
                    );
                }
                modal::ModalKind::Settings => {
                    widgets::settings::render(
                        frame,
                        overlay_area,
                        &self.settings,
                        &self.config,
                        &self.modal,
                        &self.auth,
                        &self.subagents,
                        &self.concierge,
                        &self.tier,
                        &self.plugin_settings,
                        &self.theme,
                    );
                }
                modal::ModalKind::ProviderPicker => {
                    widgets::provider_picker::render(
                        frame,
                        overlay_area,
                        &self.modal,
                        &self.config,
                        &self.auth,
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
                modal::ModalKind::Notifications => {
                    widgets::notifications::render(
                        frame,
                        overlay_area,
                        &self.notifications,
                        &self.theme,
                    );
                }
                modal::ModalKind::ToolsPicker | modal::ModalKind::ViewPicker => {}
                modal::ModalKind::Status => {
                    render_helpers::render_status_modal(
                        frame,
                        overlay_area,
                        "STATUS",
                        &self.status_modal_body(),
                        0,
                        &self.theme,
                    );
                }
                modal::ModalKind::PromptViewer => {
                    render_helpers::render_status_modal(
                        frame,
                        overlay_area,
                        "PROMPT",
                        &self.prompt_modal_body(),
                        self.prompt_modal_scroll,
                        &self.theme,
                    );
                }
                modal::ModalKind::Help => {
                    render_helpers::render_help_modal(frame, overlay_area, &self.theme);
                }
                modal::ModalKind::WhatsAppLink => {
                    widgets::whatsapp_link::render(frame, overlay_area, &self.modal, &self.theme);
                }
            }
        }
    }

    pub(super) fn current_modal_area(&self) -> Option<(modal::ModalKind, Rect)> {
        let kind = self.modal.top()?;
        let area = Rect::new(0, 0, self.width, self.height);
        let rect = match kind {
            modal::ModalKind::Settings => render_helpers::centered_rect(90, 88, area),
            modal::ModalKind::ApprovalOverlay => render_helpers::centered_rect(60, 40, area),
            modal::ModalKind::ApprovalCenter => render_helpers::centered_rect(86, 82, area),
            modal::ModalKind::ChatActionConfirm => render_helpers::centered_rect(48, 28, area),
            modal::ModalKind::CommandPalette => render_helpers::centered_rect(50, 40, area),
            modal::ModalKind::Status => render_helpers::centered_rect(72, 70, area),
            modal::ModalKind::PromptViewer => render_helpers::centered_rect(84, 84, area),
            modal::ModalKind::ThreadPicker => render_helpers::centered_rect(60, 50, area),
            modal::ModalKind::GoalPicker => render_helpers::centered_rect(60, 50, area),
            modal::ModalKind::QueuedPrompts => render_helpers::centered_rect(72, 42, area),
            modal::ModalKind::ProviderPicker => render_helpers::centered_rect(35, 65, area),
            modal::ModalKind::ModelPicker => render_helpers::centered_rect(45, 50, area),
            modal::ModalKind::OpenAIAuth => render_helpers::centered_rect(70, 35, area),
            modal::ModalKind::ErrorViewer => render_helpers::centered_rect(70, 45, area),
            modal::ModalKind::EffortPicker => render_helpers::centered_rect(35, 30, area),
            modal::ModalKind::Notifications => render_helpers::centered_rect(78, 78, area),
            modal::ModalKind::WhatsAppLink => render_helpers::centered_rect(70, 80, area),
            modal::ModalKind::ToolsPicker | modal::ModalKind::ViewPicker => {
                render_helpers::centered_rect(40, 35, area)
            }
            modal::ModalKind::Help => render_helpers::centered_rect(70, 80, area),
        };
        Some((kind, rect))
    }
}
