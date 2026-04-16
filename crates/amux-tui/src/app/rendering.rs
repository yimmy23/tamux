use super::*;

const MIN_HEADER_CONTEXT_TARGET_TOKENS: u32 = 1_024;

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
    fn estimate_header_message_tokens(message: &chat::AgentMessage) -> u64 {
        let content = if message.message_kind == "compaction_artifact" {
            message
                .compaction_payload
                .as_deref()
                .filter(|payload| !payload.trim().is_empty())
                .unwrap_or(message.content.as_str())
        } else {
            message.content.as_str()
        };

        let mut chars = content.chars().count();
        chars += message
            .tool_name
            .as_deref()
            .map(str::chars)
            .map(Iterator::count)
            .unwrap_or(0);
        chars += message
            .tool_arguments
            .as_deref()
            .map(str::chars)
            .map(Iterator::count)
            .unwrap_or(0);

        chars.div_ceil(4) as u64 + 12
    }

    fn active_compaction_window_start(thread: &chat::AgentThread) -> usize {
        thread
            .messages
            .iter()
            .rposition(|message| message.message_kind == "compaction_artifact")
            .unwrap_or(0)
    }

    fn effective_primary_context_window_tokens(&self) -> u32 {
        if let Some(context_window) = providers::resolve_context_window_for_provider_auth(
            &self.config.provider,
            &self.config.auth_source,
            &self.config.model,
            &self.config.custom_model_name,
        ) {
            context_window
        } else if providers::model_uses_context_window_override(
            &self.config.provider,
            &self.config.auth_source,
            &self.config.model,
            &self.config.custom_model_name,
        ) {
            self.config
                .custom_context_window_tokens
                .unwrap_or(providers::default_custom_model_context_window())
        } else {
            providers::known_context_window_for(&self.config.provider, &self.config.model)
                .unwrap_or(self.config.context_window_tokens)
        }
    }

    fn current_header_context_window_tokens(&self) -> u32 {
        let profile = self.current_header_agent_profile();
        let fallback = match self.current_conversation_agent_kind() {
            ConversationAgentKind::Swarog => self.effective_primary_context_window_tokens(),
            ConversationAgentKind::Rarog => self.effective_primary_context_window_tokens(),
            ConversationAgentKind::Weles => match self.config.compaction_strategy.as_str() {
                "custom_model" => self.config.compaction_custom_context_window_tokens,
                _ => self.config.context_window_tokens,
            },
        }
        .max(1);

        providers::known_context_window_for(&profile.provider, &profile.model)
            .unwrap_or(fallback)
            .max(1)
    }

    fn current_header_weles_compaction_window_tokens(&self, primary_window: u32) -> u32 {
        let provider = self.config.compaction_weles_provider.trim();
        let model = self.config.compaction_weles_model.trim();
        if provider.is_empty() || model.is_empty() {
            return primary_window.max(1);
        }

        providers::known_context_window_for(provider, model)
            .unwrap_or(primary_window)
            .max(1)
    }

    fn current_header_context_target_tokens(&self) -> u32 {
        let context_window = self.current_header_context_window_tokens().max(1);
        if !self.config.auto_compact_context {
            return context_window;
        }

        let threshold_pct = self.config.compact_threshold_pct.clamp(1, 100);
        let threshold_target = context_window.saturating_mul(threshold_pct) / 100;
        let strategy_cap = match self.config.compaction_strategy.as_str() {
            "weles" => {
                self.current_header_weles_compaction_window_tokens(context_window)
                    .saturating_mul(threshold_pct)
                    / 100
            }
            "custom_model" => {
                self.config
                    .compaction_custom_context_window_tokens
                    .max(1)
                    .saturating_mul(threshold_pct)
                    / 100
            }
            _ => threshold_target,
        };

        threshold_target
            .max(MIN_HEADER_CONTEXT_TARGET_TOKENS)
            .min(strategy_cap.max(MIN_HEADER_CONTEXT_TARGET_TOKENS))
            .max(1)
    }

    pub(super) fn conversation_participant_summary_area(&self) -> Option<Rect> {
        if !matches!(self.main_pane_view, MainPaneView::Conversation) {
            return None;
        }
        if self.should_show_provider_onboarding()
            || self.should_show_local_landing()
            || self.should_show_concierge_hero_loading()
            || self.should_show_thread_loading()
        {
            return None;
        }

        let thread = self.chat.active_thread()?;
        let has_summary = !thread.thread_participants.is_empty()
            || !thread.queued_participant_suggestions.is_empty();
        if !has_summary {
            return None;
        }

        let chat_area = self.pane_layout().chat;
        let summary_height = if self.active_auto_response_suggestion().is_some()
            || self.active_always_auto_response_participant().is_some()
        {
            4
        } else {
            3
        };
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(summary_height), Constraint::Min(1)])
            .split(chat_area);
        Some(chunks[0])
    }

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

    pub(crate) fn current_header_usage_summary(&self) -> widgets::header::HeaderUsageDisplay {
        let context_window_tokens = self.current_header_context_window_tokens().max(1) as u64;
        let compaction_target_tokens = self.current_header_context_target_tokens().max(1) as u64;
        let (total_thread_tokens, current_tokens, total_cost_usd) = self
            .chat
            .active_thread()
            .map(|thread| {
                let start = Self::active_compaction_window_start(thread);
                let total_thread_tokens = thread.total_input_tokens + thread.total_output_tokens;
                let current_tokens = thread.messages[start..]
                    .iter()
                    .map(Self::estimate_header_message_tokens)
                    .sum::<u64>();
                let total_cost_usd = thread
                    .messages
                    .iter()
                    .filter_map(|message| message.cost)
                    .reduce(|acc, cost| acc + cost);
                (total_thread_tokens, current_tokens, total_cost_usd)
            })
            .unwrap_or((0, 0, None));

        let utilization_pct = current_tokens
            .saturating_mul(100)
            .checked_div(context_window_tokens)
            .unwrap_or(0)
            .min(100) as u8;

        widgets::header::HeaderUsageDisplay {
            total_thread_tokens,
            current_tokens,
            context_window_tokens,
            compaction_target_tokens,
            utilization_pct,
            total_cost_usd,
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

        if self.should_show_thread_loading() {
            let thread_title = self
                .chat
                .active_thread()
                .map(|thread| thread.title.as_str());
            widgets::concierge_loading::render_thread(
                frame,
                area,
                &self.theme,
                self.tick_counter,
                thread_title,
            );
            return;
        }

        let has_auto_response = self.active_auto_response_suggestion().is_some()
            || self.active_always_auto_response_participant().is_some();
        let participant_summary = self.chat.active_thread().and_then(|thread| {
            let active: Vec<&str> = thread
                .thread_participants
                .iter()
                .filter(|participant| participant.status.eq_ignore_ascii_case("active"))
                .map(|participant| participant.agent_name.as_str())
                .collect();
            let inactive_count = thread
                .thread_participants
                .iter()
                .filter(|participant| !participant.status.eq_ignore_ascii_case("active"))
                .count();
            let queued_count = thread
                .queued_participant_suggestions
                .iter()
                .filter(|suggestion| {
                    !suggestion
                        .suggestion_kind
                        .eq_ignore_ascii_case("auto_response")
                })
                .count();
            if active.is_empty() && inactive_count == 0 && queued_count == 0 && !has_auto_response {
                return None;
            }

            let active_summary = if active.is_empty() {
                "active: none".to_string()
            } else {
                let names = active.into_iter().take(3).collect::<Vec<_>>().join(", ");
                format!("active: {names}")
            };

            Some(format!(
                "Participants  •  {}  •  inactive: {}  •  queued: {}  •  /participants",
                active_summary, inactive_count, queued_count
            ))
        });
        let auto_response_countdown_secs = self.active_auto_response_countdown_secs();
        let always_auto_response_participant = self.active_always_auto_response_participant();

        let area = if let Some(summary) = participant_summary.as_deref() {
            let summary_height = if auto_response_countdown_secs.is_some()
                || always_auto_response_participant.is_some()
            {
                4
            } else {
                3
            };
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Length(summary_height), Constraint::Min(1)])
                .split(area);
            let mut lines = vec![ratatui::text::Line::from(ratatui::text::Span::styled(
                summary.to_string(),
                self.theme.fg_dim,
            ))];
            if let Some(countdown_secs) = auto_response_countdown_secs {
                let yes_style = if self.auto_response_selection == AutoResponseActionSelection::Yes
                {
                    self.theme.accent_primary
                } else {
                    self.theme.fg_dim
                };
                let no_style = if self.auto_response_selection == AutoResponseActionSelection::No {
                    self.theme.accent_danger
                } else {
                    self.theme.fg_dim
                };
                let always_style =
                    if self.auto_response_selection == AutoResponseActionSelection::Always {
                        self.theme.accent_secondary
                    } else {
                        self.theme.fg_dim
                    };
                lines.push(ratatui::text::Line::from(vec![
                    ratatui::text::Span::styled("Auto response in ", self.theme.fg_active),
                    ratatui::text::Span::styled(
                        format!("{countdown_secs}s"),
                        self.theme.accent_primary,
                    ),
                    ratatui::text::Span::styled("  ", self.theme.fg_dim),
                    ratatui::text::Span::styled(format!("[Yes {}s]", countdown_secs), yes_style),
                    ratatui::text::Span::styled(" ", self.theme.fg_dim),
                    ratatui::text::Span::styled("[No]", no_style),
                    ratatui::text::Span::styled(" ", self.theme.fg_dim),
                    ratatui::text::Span::styled("[Always for this thread]", always_style),
                ]));
            } else if let Some(participant) = always_auto_response_participant {
                lines.push(ratatui::text::Line::from(vec![
                    ratatui::text::Span::styled("Auto response: ", self.theme.fg_active),
                    ratatui::text::Span::styled("always ", self.theme.accent_secondary),
                    ratatui::text::Span::styled(
                        participant.agent_name.clone(),
                        self.theme.accent_primary,
                    ),
                ]));
            }
            frame.render_widget(
                ratatui::widgets::Paragraph::new(lines)
                    .block(
                        Block::default()
                            .title(" THREAD PARTICIPANTS ")
                            .borders(Borders::ALL)
                            .border_type(BorderType::Rounded)
                            .border_style(self.theme.accent_secondary),
                    )
                    .style(self.theme.fg_dim),
                chunks[0],
            );
            chunks[1]
        } else {
            area
        };

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
        let usage = self.current_header_usage_summary();

        widgets::header::render(
            frame,
            chunks[0],
            &profile.provider,
            &profile.model,
            profile.reasoning_effort.as_deref(),
            &usage,
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
                &self.chat,
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

        let footer_activity = self.footer_activity_text();
        widgets::footer::render_input(
            frame,
            chunks[4],
            &self.input,
            &self.theme,
            self.focus == FocusArea::Input,
            self.modal.top().is_some(),
            &self.attachments,
            self.tick_counter,
            footer_activity.as_deref(),
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
                modal::ModalKind::OperatorQuestionOverlay => {
                    render_helpers::centered_rect(68, 34, area)
                }
                modal::ModalKind::ApprovalCenter => render_helpers::centered_rect(86, 82, area),
                modal::ModalKind::ChatActionConfirm => render_helpers::centered_rect(48, 28, area),
                modal::ModalKind::PinnedBudgetExceeded => {
                    render_helpers::centered_rect(62, 36, area)
                }
                modal::ModalKind::CommandPalette => render_helpers::centered_rect(50, 40, area),
                modal::ModalKind::Status => render_helpers::centered_rect(72, 70, area),
                modal::ModalKind::Statistics => render_helpers::centered_rect(84, 84, area),
                modal::ModalKind::PromptViewer => render_helpers::centered_rect(84, 84, area),
                modal::ModalKind::ThreadParticipants => render_helpers::centered_rect(76, 68, area),
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
                modal::ModalKind::OperatorQuestionOverlay => {}
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
                modal::ModalKind::PinnedBudgetExceeded => {
                    if let Some(payload) = self.pending_pinned_budget_exceeded.as_ref() {
                        render_helpers::render_pinned_budget_exceeded_modal(
                            frame,
                            overlay_area,
                            payload.current_pinned_chars,
                            payload.pinned_budget_chars,
                            payload.candidate_pinned_chars,
                            &self.theme,
                        );
                    }
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
                    let max_scroll = self.status_modal_max_scroll();
                    render_helpers::render_status_modal(
                        frame,
                        overlay_area,
                        "STATUS",
                        &self.status_modal_body(),
                        self.status_modal_scroll,
                        max_scroll > 0,
                        &self.theme,
                    );
                }
                modal::ModalKind::Statistics => {
                    widgets::statistics::render(
                        frame,
                        overlay_area,
                        self.statistics_modal_snapshot.as_ref(),
                        self.statistics_modal_loading,
                        self.statistics_modal_error.as_deref(),
                        self.statistics_modal_tab,
                        self.statistics_modal_window,
                        self.statistics_modal_scroll,
                        &self.theme,
                    );
                }
                modal::ModalKind::PromptViewer => {
                    render_helpers::render_status_modal(
                        frame,
                        overlay_area,
                        self.prompt_modal_title(),
                        &self.prompt_modal_body(),
                        self.prompt_modal_scroll,
                        true,
                        &self.theme,
                    );
                }
                modal::ModalKind::ThreadParticipants => {
                    render_helpers::render_status_modal(
                        frame,
                        overlay_area,
                        "THREAD PARTICIPANTS",
                        &self.thread_participants_modal_body(),
                        self.thread_participants_modal_scroll,
                        true,
                        &self.theme,
                    );
                }
                modal::ModalKind::Help => {
                    render_helpers::render_help_modal(
                        frame,
                        overlay_area,
                        self.help_modal_scroll,
                        &self.theme,
                    );
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
            modal::ModalKind::OperatorQuestionOverlay => {
                render_helpers::centered_rect(68, 34, area)
            }
            modal::ModalKind::ApprovalCenter => render_helpers::centered_rect(86, 82, area),
            modal::ModalKind::ChatActionConfirm => render_helpers::centered_rect(48, 28, area),
            modal::ModalKind::PinnedBudgetExceeded => render_helpers::centered_rect(62, 36, area),
            modal::ModalKind::CommandPalette => render_helpers::centered_rect(50, 40, area),
            modal::ModalKind::Status => render_helpers::centered_rect(72, 70, area),
            modal::ModalKind::Statistics => render_helpers::centered_rect(84, 84, area),
            modal::ModalKind::PromptViewer => render_helpers::centered_rect(84, 84, area),
            modal::ModalKind::ThreadParticipants => render_helpers::centered_rect(76, 68, area),
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
