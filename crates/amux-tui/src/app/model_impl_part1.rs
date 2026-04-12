impl TuiModel {
    pub fn new(
        daemon_events_rx: Receiver<ClientEvent>,
        daemon_cmd_tx: UnboundedSender<DaemonCommand>,
    ) -> Self {
        Self {
            chat: chat::ChatState::new(),
            input: input::InputState::new(),
            modal: modal::ModalState::new(),
            sidebar: sidebar::SidebarState::new(),
            tasks: task::TaskState::new(),
            config: config::ConfigState::new(),
            approval: approval::ApprovalState::new(),
            anticipatory: AnticipatoryState::new(),
            audit: crate::state::audit::AuditState::new(),
            notifications: notifications::NotificationsState::new(),
            settings: settings::SettingsState::new(),
            plugin_settings: settings::PluginSettingsState::new(),
            auth: AuthState::new(),
            subagents: SubAgentsState::new(),
            collaboration: CollaborationState::new(),
            concierge: ConciergeState::new(),
            tier: TierState::default(),
            focus: FocusArea::Input,
            theme: ThemeTokens::default(),
            width: 120,
            height: 40,
            daemon_cmd_tx,
            daemon_events_rx,
            connected: false,
            agent_config_loaded: false,
            status_line: "Starting...".to_string(),
            default_session_id: None,
            tick_counter: 0,
            agent_activity: None,
            last_error: None,
            error_active: false,
            error_tick: 0,
            openai_auth_url: None,
            openai_auth_status_text: None,
            settings_picker_target: None,
            last_attention_surface: None,
            pending_g: false,
            show_sidebar_override: None,
            main_pane_view: MainPaneView::Conversation,
            task_view_scroll: 0,
            task_show_live_todos: true,
            task_show_timeline: true,
            task_show_files: true,
            pending_quit: false,
            pending_stop: false,
            pending_stop_tick: 0,
            input_notice: None,
            pending_chat_action_confirm: None,
            chat_action_confirm_accept_selected: true,
            retry_wait_start_selected: false,
            held_key_modifiers: KeyModifiers::NONE,
            attachments: Vec::new(),
            queued_prompts: Vec::new(),
            queued_prompt_action: QueuedPromptAction::SendNow,
            operator_profile: OperatorProfileOnboardingState::default(),
            cancelled_thread_id: None,
            pending_new_thread_target_agent: None,
            pending_builtin_persona_setup: None,
            thread_loading_id: None,
            ignore_pending_concierge_welcome: false,
            gateway_statuses: Vec::new(),
            weles_health: None,
            recent_actions: Vec::new(),
            status_modal_snapshot: None,
            status_modal_diagnostics_json: None,
            status_modal_loading: false,
            status_modal_error: None,
            status_modal_scroll: 0,
            prompt_modal_snapshot: None,
            prompt_modal_loading: false,
            prompt_modal_error: None,
            prompt_modal_scroll: 0,
            thread_participants_modal_scroll: 0,
            help_modal_scroll: 0,
            chat_drag_anchor: None,
            chat_drag_current: None,
            chat_drag_anchor_point: None,
            chat_drag_current_point: None,
            chat_selection_snapshot: None,
            chat_scrollbar_drag_grab_offset: None,
            work_context_drag_anchor: None,
            work_context_drag_current: None,
            work_context_drag_anchor_point: None,
            work_context_drag_current_point: None,
        }
    }

    fn send_daemon_command(&self, command: DaemonCommand) {
        let _ = self.daemon_cmd_tx.send(command);
    }

    pub(crate) fn open_status_modal_loading(&mut self) {
        self.status_modal_loading = true;
        self.status_modal_snapshot = None;
        self.status_modal_diagnostics_json = None;
        self.status_modal_error = None;
        self.status_modal_scroll = 0;
        if self.modal.top() != Some(modal::ModalKind::Status) {
            self.modal
                .reduce(modal::ModalAction::Push(modal::ModalKind::Status));
        }
    }

    pub(crate) fn open_prompt_modal_loading(&mut self) {
        self.prompt_modal_loading = true;
        self.prompt_modal_snapshot = None;
        self.prompt_modal_error = None;
        self.prompt_modal_scroll = 0;
        if self.modal.top() != Some(modal::ModalKind::PromptViewer) {
            self.modal
                .reduce(modal::ModalAction::Push(modal::ModalKind::PromptViewer));
        }
    }

    pub(crate) fn status_modal_body(&self) -> String {
        if self.status_modal_loading {
            return "Loading tamux status...".to_string();
        }
        if let Some(error) = &self.status_modal_error {
            return format!("Status request failed\n====================\n{error}");
        }
        if let Some(snapshot) = &self.status_modal_snapshot {
            return render_helpers::format_status_modal_text(
                snapshot,
                self.status_modal_diagnostics_json.as_deref(),
            );
        }
        "No status available.".to_string()
    }

    pub(crate) fn prompt_modal_body(&self) -> String {
        if self.prompt_modal_loading {
            return "Loading agent prompt...".to_string();
        }
        if let Some(error) = &self.prompt_modal_error {
            return format!("Prompt request failed\n=====================\n{error}");
        }
        if let Some(prompt) = &self.prompt_modal_snapshot {
            return render_helpers::format_prompt_modal_text(prompt);
        }
        "No prompt available.".to_string()
    }

    pub(crate) fn thread_participants_modal_body(&self) -> String {
        let Some(thread) = self.chat.active_thread() else {
            return "No active thread selected.".to_string();
        };

        let active: Vec<_> = thread
            .thread_participants
            .iter()
            .filter(|participant| participant.status.eq_ignore_ascii_case("active"))
            .collect();
        let inactive: Vec<_> = thread
            .thread_participants
            .iter()
            .filter(|participant| !participant.status.eq_ignore_ascii_case("active"))
            .collect();

        let mut body = String::new();
        body.push_str(&format!("Thread: {}\n", thread.title));
        body.push_str("==============================\n\n");

        body.push_str("Active Participants\n");
        body.push_str("-------------------\n");
        if active.is_empty() {
            body.push_str("- none\n");
        } else {
            for participant in active {
                body.push_str(&format!(
                    "- {} ({})\n  instruction: {}\n",
                    participant.agent_name,
                    participant.agent_id,
                    participant.instruction.trim()
                ));
            }
        }
        body.push('\n');

        body.push_str("Inactive Participants\n");
        body.push_str("---------------------\n");
        if inactive.is_empty() {
            body.push_str("- none\n");
        } else {
            for participant in inactive {
                body.push_str(&format!(
                    "- {} ({})\n  instruction: {}\n",
                    participant.agent_name,
                    participant.agent_id,
                    participant.instruction.trim()
                ));
            }
        }
        body.push('\n');

        body.push_str("Queued Suggestions\n");
        body.push_str("------------------\n");
        if thread.queued_participant_suggestions.is_empty() {
            body.push_str("- none\n");
        } else {
            for suggestion in &thread.queued_participant_suggestions {
                let mut badges = vec![suggestion.status.clone()];
                if suggestion.force_send {
                    badges.push("force_send".to_string());
                }
                body.push_str(&format!(
                    "- {} [{}]\n  message: {}\n",
                    suggestion.target_agent_name,
                    badges.join(", "),
                    suggestion.instruction.trim()
                ));
                if let Some(error) = suggestion.error.as_deref() {
                    if !error.trim().is_empty() {
                        body.push_str(&format!("  error: {}\n", error.trim()));
                    }
                }
            }
        }

        body
    }

    pub(crate) fn prompt_modal_max_scroll(&self) -> usize {
        let body = self.prompt_modal_body();
        let (viewport_lines, inner_width) = self
            .current_modal_area()
            .filter(|(kind, _)| *kind == modal::ModalKind::PromptViewer)
            .map(|(_, area)| {
                (
                    area.height.saturating_sub(3) as usize,
                    area.width.saturating_sub(2) as usize,
                )
            })
            .unwrap_or((1, 1));
        let total_lines = crate::widgets::message::wrap_text(&body, inner_width.max(1))
            .len()
            .max(1);
        let viewport_lines = viewport_lines.max(1);
        total_lines.saturating_sub(viewport_lines)
    }

    pub(crate) fn status_modal_max_scroll(&self) -> usize {
        let body = self.status_modal_body();
        let (viewport_lines, inner_width) = self
            .current_modal_area()
            .filter(|(kind, _)| *kind == modal::ModalKind::Status)
            .map(|(_, area)| {
                (
                    area.height.saturating_sub(3) as usize,
                    area.width.saturating_sub(2) as usize,
                )
            })
            .unwrap_or((1, 1));
        let total_lines = crate::widgets::message::wrap_text(&body, inner_width.max(1))
            .len()
            .max(1);
        let viewport_lines = viewport_lines.max(1);
        total_lines.saturating_sub(viewport_lines)
    }

    pub(crate) fn thread_participants_modal_max_scroll(&self) -> usize {
        let body = self.thread_participants_modal_body();
        let (viewport_lines, inner_width) = self
            .current_modal_area()
            .filter(|(kind, _)| *kind == modal::ModalKind::ThreadParticipants)
            .map(|(_, area)| {
                (
                    area.height.saturating_sub(3) as usize,
                    area.width.saturating_sub(2) as usize,
                )
            })
            .unwrap_or((1, 1));
        let total_lines = crate::widgets::message::wrap_text(&body, inner_width.max(1))
            .len()
            .max(1);
        let viewport_lines = viewport_lines.max(1);
        total_lines.saturating_sub(viewport_lines)
    }

    pub(crate) fn help_modal_max_scroll(&self) -> usize {
        let body = render_helpers::help_modal_text();
        let (viewport_lines, inner_width) = self
            .current_modal_area()
            .filter(|(kind, _)| *kind == modal::ModalKind::Help)
            .map(|(_, area)| {
                (
                    area.height.saturating_sub(3) as usize,
                    area.width.saturating_sub(2) as usize,
                )
            })
            .unwrap_or((1, 1));
        let total_lines = crate::widgets::message::wrap_text(&body, inner_width.max(1))
            .len()
            .max(1);
        let viewport_lines = viewport_lines.max(1);
        total_lines.saturating_sub(viewport_lines)
    }

    pub(crate) fn set_status_modal_scroll(&mut self, scroll: usize) {
        self.status_modal_scroll = scroll.min(self.status_modal_max_scroll());
    }

    pub(crate) fn step_status_modal_scroll(&mut self, delta: i32) {
        let current = self.status_modal_scroll as i32;
        let next = (current + delta).max(0) as usize;
        self.set_status_modal_scroll(next);
    }

    pub(crate) fn page_status_modal_scroll(&mut self, direction: i32) {
        let page = self
            .current_modal_area()
            .filter(|(kind, _)| *kind == modal::ModalKind::Status)
            .map(|(_, area)| area.height.saturating_sub(4) as i32)
            .unwrap_or(10)
            .max(1);
        self.step_status_modal_scroll(page * direction);
    }

    pub(crate) fn set_prompt_modal_scroll(&mut self, scroll: usize) {
        self.prompt_modal_scroll = scroll.min(self.prompt_modal_max_scroll());
    }

    pub(crate) fn set_thread_participants_modal_scroll(&mut self, scroll: usize) {
        self.thread_participants_modal_scroll =
            scroll.min(self.thread_participants_modal_max_scroll());
    }

    pub(crate) fn set_help_modal_scroll(&mut self, scroll: usize) {
        self.help_modal_scroll = scroll.min(self.help_modal_max_scroll());
    }

    pub(crate) fn step_prompt_modal_scroll(&mut self, delta: i32) {
        let current = self.prompt_modal_scroll as i32;
        let next = (current + delta).max(0) as usize;
        self.set_prompt_modal_scroll(next);
    }

    pub(crate) fn page_prompt_modal_scroll(&mut self, direction: i32) {
        let page = self
            .current_modal_area()
            .filter(|(kind, _)| *kind == modal::ModalKind::PromptViewer)
            .map(|(_, area)| area.height.saturating_sub(4) as i32)
            .unwrap_or(10)
            .max(1);
        self.step_prompt_modal_scroll(page * direction);
    }

    pub(crate) fn step_thread_participants_modal_scroll(&mut self, delta: i32) {
        let current = self.thread_participants_modal_scroll as i32;
        let next = (current + delta).max(0) as usize;
        self.set_thread_participants_modal_scroll(next);
    }

    pub(crate) fn step_help_modal_scroll(&mut self, delta: i32) {
        let current = self.help_modal_scroll as i32;
        let next = (current + delta).max(0) as usize;
        self.set_help_modal_scroll(next);
    }

    pub(crate) fn page_thread_participants_modal_scroll(&mut self, direction: i32) {
        let page = self
            .current_modal_area()
            .filter(|(kind, _)| *kind == modal::ModalKind::ThreadParticipants)
            .map(|(_, area)| area.height.saturating_sub(4) as i32)
            .unwrap_or(10)
            .max(1);
        self.step_thread_participants_modal_scroll(page * direction);
    }

    pub(crate) fn page_help_modal_scroll(&mut self, direction: i32) {
        let page = self
            .current_modal_area()
            .filter(|(kind, _)| *kind == modal::ModalKind::Help)
            .map(|(_, area)| area.height.saturating_sub(4) as i32)
            .unwrap_or(10)
            .max(1);
        self.step_help_modal_scroll(page * direction);
    }

    pub(crate) fn open_thread_participants_modal(&mut self) {
        if self.chat.active_thread().is_none() {
            self.status_line = "Open a thread first, then run /participants".to_string();
            return;
        }
        self.thread_participants_modal_scroll = 0;
        if self.modal.top() != Some(modal::ModalKind::ThreadParticipants) {
            self.modal
                .reduce(modal::ModalAction::Push(modal::ModalKind::ThreadParticipants));
        }
    }

    pub(crate) fn request_prompt_inspection(&mut self, agent_id: Option<String>) {
        self.open_prompt_modal_loading();
        self.send_daemon_command(DaemonCommand::RequestPromptInspection { agent_id });
        self.status_line = "Requesting assembled agent prompt...".to_string();
    }

    fn show_input_notice(
        &mut self,
        text: impl Into<String>,
        kind: InputNoticeKind,
        duration_ticks: u64,
        dismiss_on_interaction: bool,
    ) {
        self.input_notice = Some(InputNotice {
            text: text.into(),
            kind,
            expires_at_tick: self.tick_counter.saturating_add(duration_ticks),
            dismiss_on_interaction,
        });
    }

    fn clear_dismissable_input_notice(&mut self) {
        if self
            .input_notice
            .as_ref()
            .is_some_and(|notice| notice.dismiss_on_interaction)
        {
            self.input_notice = None;
        }
    }

    fn begin_thread_loading(&mut self, thread_id: impl Into<String>) {
        let thread_id = thread_id.into();
        self.thread_loading_id = Some(thread_id.clone());
        self.status_line = match self.chat.active_thread() {
            Some(thread) if !thread.title.trim().is_empty() => {
                format!("Loading thread: {}", thread.title.trim())
            }
            _ => format!("Loading thread: {thread_id}"),
        };
    }

    fn finish_thread_loading(&mut self, thread_id: &str) {
        if self.thread_loading_id.as_deref() == Some(thread_id) {
            self.thread_loading_id = None;
        }
    }

    fn should_show_thread_loading(&self) -> bool {
        self.thread_loading_id
            .as_deref()
            .is_some_and(|thread_id| self.chat.active_thread_id() == Some(thread_id))
            && self
                .chat
                .active_thread()
                .is_some_and(|thread| thread.messages.is_empty())
            && !self.chat.is_streaming()
    }

    fn clear_pending_stop(&mut self) {
        self.pending_stop = false;
        self.clear_dismissable_input_notice();
    }

    fn pending_stop_active(&self) -> bool {
        self.pending_stop && self.tick_counter.saturating_sub(self.pending_stop_tick) < 100
    }

    fn assistant_busy(&self) -> bool {
        self.chat.is_streaming() || self.agent_activity.is_some()
    }

    fn queue_barrier_active(&self) -> bool {
        self.chat.has_running_tool_calls()
    }

    fn should_queue_submitted_prompt(&self) -> bool {
        self.chat.is_streaming()
    }

    fn clear_expired_queued_prompt_copy_feedback(&mut self) {
        for prompt in &mut self.queued_prompts {
            prompt.clear_expired_copy_feedback(self.tick_counter);
        }
    }

    fn sync_queued_prompt_modal_state(&mut self) {
        if self.modal.top() != Some(modal::ModalKind::QueuedPrompts) {
            return;
        }

        if self.queued_prompts.is_empty() {
            self.close_top_modal();
            return;
        }

        self.modal.set_picker_item_count(self.queued_prompts.len());
    }

    fn actions_bar_visible(&self) -> bool {
        if self.should_show_local_landing() {
            return false;
        }
        if self.should_show_operator_profile_onboarding() {
            return false;
        }

        self.concierge.loading || !self.chat.active_actions().is_empty()
    }

    fn concierge_banner_visible(&self) -> bool {
        self.actions_bar_visible()
    }

    fn should_show_local_landing(&self) -> bool {
        matches!(self.main_pane_view, MainPaneView::Conversation)
            && self.chat.active_thread().is_none()
            && !self.chat.is_streaming()
            && !self.concierge.loading
            && !self.should_show_operator_profile_onboarding()
            && !self.should_show_provider_onboarding()
    }

    fn should_show_concierge_hero_loading(&self) -> bool {
        self.concierge.loading
            && matches!(self.main_pane_view, MainPaneView::Conversation)
            && self.chat.active_thread().is_none()
            && self.chat.streaming_content().is_empty()
            && !self.should_show_operator_profile_onboarding()
            && !self.concierge.has_active_welcome()
    }

    fn concierge_banner_height(&self) -> u16 {
        if self.should_show_concierge_hero_loading() {
            0
        } else if self.actions_bar_visible() {
            1
        } else {
            0
        }
    }

    fn anticipatory_banner_height(&self) -> u16 {
        if self.anticipatory.has_items()
            && !self.concierge.loading
            && !self.concierge.has_active_welcome()
        {
            8
        } else {
            0
        }
    }

    fn pane_layout_for_area(&self, area: Rect) -> PaneLayout {
        let input_height = self.input_height().min(area.height.saturating_sub(1));
        let remaining_after_input = area.height.saturating_sub(input_height + 1);
        let anticipatory_height = self
            .anticipatory_banner_height()
            .min(remaining_after_input.saturating_sub(1));
        let remaining_after_anticipatory =
            remaining_after_input.saturating_sub(anticipatory_height);
        let concierge_height = self
            .concierge_banner_height()
            .min(remaining_after_anticipatory.saturating_sub(1));
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

        let body = chunks[1];
        let chat = if self.sidebar_visible() {
            let sidebar_pct = if self.width >= 120 { 33 } else { 28 };
            Layout::default()
                .direction(Direction::Horizontal)
                .constraints([
                    Constraint::Percentage(100 - sidebar_pct),
                    Constraint::Percentage(sidebar_pct),
                ])
                .split(body)[0]
        } else {
            body
        };
        let sidebar = if self.sidebar_visible() {
            let sidebar_pct = if self.width >= 120 { 33 } else { 28 };
            Some(
                Layout::default()
                    .direction(Direction::Horizontal)
                    .constraints([
                        Constraint::Percentage(100 - sidebar_pct),
                        Constraint::Percentage(sidebar_pct),
                    ])
                    .split(body)[1],
            )
        } else {
            None
        };

        PaneLayout {
            chat,
            sidebar,
            concierge: chunks[3],
            input: chunks[4],
        }
    }

    fn pane_layout(&self) -> PaneLayout {
        self.pane_layout_for_area(Rect::new(0, 0, self.width, self.height))
    }

    fn has_configured_provider(&self) -> bool {
        self.auth.entries.iter().any(|entry| entry.authenticated)
    }

    fn should_show_provider_onboarding(&self) -> bool {
        self.connected
            && self.auth.loaded
            && !self.has_configured_provider()
            && matches!(self.main_pane_view, MainPaneView::Conversation)
            && self.chat.active_thread().is_none()
            && self.chat.streaming_content().is_empty()
            && !self.should_show_operator_profile_onboarding()
    }

    fn should_show_operator_profile_onboarding(&self) -> bool {
        self.operator_profile.visible
            && matches!(self.main_pane_view, MainPaneView::Conversation)
            && self.chat.streaming_content().is_empty()
    }

    fn operator_profile_select_options(field_key: &str) -> Option<&'static [&'static str]> {
        match field_key {
            "notification_preference" => Some(&["minimal", "balanced", "proactive"]),
            _ => None,
        }
    }

    fn current_operator_profile_select_options(&self) -> Option<&'static [&'static str]> {
        self.operator_profile
            .question
            .as_ref()
            .and_then(|question| Self::operator_profile_select_options(&question.field_key))
    }

    fn submit_operator_profile_answer(&mut self) -> bool {
        let Some(question) = self.operator_profile.question.clone() else {
            return false;
        };
        let answer = self.input.buffer().trim();
        if answer.is_empty() && !question.optional {
            self.show_input_notice(
                "Answer required (Ctrl+S to skip, Ctrl+D to defer)",
                InputNoticeKind::Warning,
                80,
                true,
            );
            return true;
        }

        let answer_json = if answer.is_empty() && question.optional {
            "null".to_string()
        } else {
            match question.input_kind.as_str() {
                "bool" => match answer.to_ascii_lowercase().as_str() {
                    "true" | "t" | "yes" | "y" | "1" => "true".to_string(),
                    "false" | "f" | "no" | "n" | "0" => "false".to_string(),
                    _ => {
                        self.show_input_notice(
                            "Use true/false (or yes/no) for this question",
                            InputNoticeKind::Warning,
                            80,
                            true,
                        );
                        return true;
                    }
                },
                "select" => {
                    let normalized = answer.to_ascii_lowercase();
                    if let Some(options) =
                        Self::operator_profile_select_options(&question.field_key)
                    {
                        if !options.iter().any(|option| *option == normalized) {
                            self.show_input_notice(
                                format!(
                                    "Pick one: {}",
                                    options
                                        .iter()
                                        .map(|option| option.to_string())
                                        .collect::<Vec<_>>()
                                        .join(", ")
                                ),
                                InputNoticeKind::Warning,
                                100,
                                true,
                            );
                            return true;
                        }
                    }
                    match serde_json::to_string(&normalized) {
                        Ok(json) => json,
                        Err(_) => return false,
                    }
                }
                _ => match serde_json::to_string(answer) {
                    Ok(json) => json,
                    Err(_) => return false,
                },
            }
        };

        self.operator_profile.loading = true;
        self.operator_profile.question = None;
        self.operator_profile.warning = None;
        self.send_daemon_command(DaemonCommand::SubmitOperatorProfileAnswer {
            session_id: question.session_id,
            question_id: question.question_id,
            answer_json,
        });
        self.input.reduce(input::InputAction::Clear);
        self.status_line = "Submitting operator profile answer…".to_string();
        true
    }

    fn skip_operator_profile_question(&mut self) -> bool {
        let Some(question) = self.operator_profile.question.clone() else {
            return false;
        };
        self.operator_profile.loading = true;
        self.operator_profile.question = None;
        self.operator_profile.warning = None;
        self.send_daemon_command(DaemonCommand::SkipOperatorProfileQuestion {
            session_id: question.session_id,
            question_id: question.question_id,
            reason: Some("tui_skip_shortcut".to_string()),
        });
        self.input.reduce(input::InputAction::Clear);
        self.status_line = "Skipping operator profile question…".to_string();
        true
    }

    fn defer_operator_profile_question(&mut self) -> bool {
        let Some(question) = self.operator_profile.question.clone() else {
            return false;
        };
        let defer_until_unix_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .ok()
            .map(|duration| duration.as_millis() as u64 + 24 * 60 * 60 * 1_000);
        self.operator_profile.loading = true;
        self.operator_profile.question = None;
        self.operator_profile.warning = None;
        self.send_daemon_command(DaemonCommand::DeferOperatorProfileQuestion {
            session_id: question.session_id,
            question_id: question.question_id,
            defer_until_unix_ms,
        });
        self.input.reduce(input::InputAction::Clear);
        self.status_line = "Deferring operator profile question for 24h…".to_string();
        true
    }

    fn open_settings_tab(&mut self, tab: SettingsTab) {
        if self.modal.top() != Some(modal::ModalKind::Settings) {
            self.modal
                .reduce(modal::ModalAction::Push(modal::ModalKind::Settings));
        }
        self.settings.reduce(SettingsAction::SwitchTab(tab));
        self.send_daemon_command(DaemonCommand::GetProviderAuthStates);
        self.send_daemon_command(DaemonCommand::GetOpenAICodexAuthStatus);
        self.send_daemon_command(DaemonCommand::ListSubAgents);
        self.send_daemon_command(DaemonCommand::GetConciergeConfig);
        if matches!(tab, SettingsTab::Gateway) {
            self.send_daemon_command(DaemonCommand::WhatsAppLinkStatus);
        } else if matches!(tab, SettingsTab::Plugins) {
            self.plugin_settings.list_mode = true;
            self.send_daemon_command(DaemonCommand::PluginList);
        }
    }

    fn open_provider_setup(&mut self) {
        self.open_settings_tab(SettingsTab::Agent);
        self.status_line = "Configure provider credentials to start chatting".to_string();
    }

    fn set_input_text(&mut self, text: &str) {
        self.input.reduce(input::InputAction::Clear);
        for ch in text.chars() {
            self.input.reduce(input::InputAction::InsertChar(ch));
        }
        self.input.set_mode(input::InputMode::Insert);
    }

    fn close_top_modal(&mut self) {
        if self.modal.top() == Some(modal::ModalKind::OpenAIAuth) {
            self.openai_auth_url = None;
            self.openai_auth_status_text = None;
        }
        if self.modal.top() == Some(modal::ModalKind::WhatsAppLink) {
            if self.modal.whatsapp_link().phase() != modal::WhatsAppLinkPhase::Connected {
                self.send_daemon_command(DaemonCommand::WhatsAppLinkStop);
            }
            self.send_daemon_command(DaemonCommand::WhatsAppLinkUnsubscribe);
            self.modal.reset_whatsapp_link();
        }
        self.modal.reduce(modal::ModalAction::Pop);
    }
}
