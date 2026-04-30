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
            goal_sidebar: goal_sidebar::GoalSidebarState::new(),
            goal_mission_control: goal_mission_control::GoalMissionControlState::new(),
            goal_workspace: goal_workspace::GoalWorkspaceState::new(),
            mission_control_navigation: MissionControlNavigationState::default(),
            goal_sidebar_selection_anchor: None,
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
            workspace: crate::state::workspace::WorkspaceState::new(),
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
            next_spawned_sidebar_task_refresh_tick: 0,
            agent_activity: None,
            thread_agent_activity: std::collections::HashMap::new(),
            bootstrap_pending_activity_threads: std::collections::HashSet::new(),
            pending_prompt_response_threads: std::collections::HashSet::new(),
            deleted_thread_ids: std::collections::HashSet::new(),
            participant_playground_activity: std::collections::HashMap::new(),
            last_error: None,
            error_active: false,
            error_tick: 0,
            openai_auth_url: None,
            openai_auth_status_text: None,
            settings_picker_target: None,
            last_attention_surface: None,
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
            pending_pinned_budget_exceeded: None,
            pending_pinned_jump: None,
            pending_pinned_shortcut_leader: None,
            chat_action_confirm_accept_selected: true,
            retry_wait_start_selected: false,
            auto_response_selection: AutoResponseActionSelection::Yes,
            held_key_modifiers: KeyModifiers::NONE,
            attachments: Vec::new(),
            voice_recording: false,
            voice_capture_path: None,
            voice_capture_stderr_path: None,
            voice_capture_backend_label: None,
            voice_recorder: None,
            voice_player: None,
            queued_prompts: Vec::new(),
            queued_prompt_action: QueuedPromptAction::SendNow,
            hidden_auto_response_suggestion_ids: std::collections::HashSet::new(),
            operator_profile: OperatorProfileOnboardingState::default(),
            cancelled_thread_id: None,
            pending_new_thread_target_agent: None,
            pending_builtin_persona_setup: None,
            thread_loading_id: None,
            missing_runtime_thread_ids: std::collections::HashSet::new(),
            pending_reconnect_restore: None,
            pending_goal_hydration_refreshes: std::collections::HashSet::new(),
            ignore_pending_concierge_welcome: false,
            gateway_statuses: Vec::new(),
            weles_health: None,
            recent_actions: Vec::new(),
            status_modal_snapshot: None,
            status_modal_diagnostics_json: None,
            status_modal_loading: false,
            status_modal_error: None,
            status_modal_scroll: 0,
            statistics_modal_snapshot: None,
            statistics_modal_loading: false,
            statistics_modal_error: None,
            statistics_modal_tab: crate::state::statistics::StatisticsTab::Overview,
            statistics_modal_window: zorai_protocol::AgentStatisticsWindow::All,
            statistics_modal_scroll: 0,
            prompt_modal_snapshot: None,
            prompt_modal_loading: false,
            prompt_modal_error: None,
            prompt_modal_scroll: 0,
            prompt_modal_title_override: None,
            prompt_modal_body_override: None,
            settings_modal_scroll: 0,
            thread_participants_modal_scroll: 0,
            help_modal_scroll: 0,
            chat_drag_anchor: None,
            chat_drag_current: None,
            chat_drag_anchor_point: None,
            chat_drag_current_point: None,
            chat_selection_snapshot: None,
            sidebar_snapshot: None,
            chat_scrollbar_drag_grab_offset: None,
            file_preview_scrollbar_drag_grab_offset: None,
            work_context_drag_anchor: None,
            work_context_drag_current: None,
            work_context_drag_anchor_point: None,
            work_context_drag_current_point: None,
            task_view_drag_anchor: None,
            task_view_drag_current: None,
            task_view_drag_anchor_point: None,
            task_view_drag_current_point: None,
            workspace_drag_task: None,
            workspace_drag_status: None,
            workspace_drag_start_target: None,
            workspace_board_selection: None,
            workspace_board_scroll: widgets::workspace_board::WorkspaceBoardScroll::default(),
            workspace_expanded_task_ids: std::collections::HashSet::new(),
            pending_workspace_create_form: None,
            pending_workspace_review_form: None,
            pending_workspace_edit_form: None,
            workspace_edit_modal_scroll: 0,
            pending_workspace_detail_task_id: None,
            pending_workspace_history_task_id: None,
            pending_workspace_actor_picker: None,
        }
    }

    fn send_daemon_command(&self, command: DaemonCommand) {
        let _ = self.daemon_cmd_tx.send(command);
    }

    pub(crate) fn active_always_auto_response_participant(
        &self,
    ) -> Option<&crate::state::chat::ThreadParticipantState> {
        self.chat
            .active_thread()?
            .thread_participants
            .iter()
            .find(|participant| {
                participant.status.eq_ignore_ascii_case("active")
                    && participant.always_auto_response
            })
    }

    pub(crate) fn queued_active_auto_response_suggestion(
        &self,
    ) -> Option<&crate::state::chat::ThreadParticipantSuggestionVm> {
        let thread = self.chat.active_thread()?;
        thread
            .queued_participant_suggestions
            .iter()
            .find(|suggestion| {
                suggestion
                    .suggestion_kind
                    .eq_ignore_ascii_case("auto_response")
                    && suggestion.status.eq_ignore_ascii_case("queued")
                    && !self
                        .hidden_auto_response_suggestion_ids
                        .contains(&suggestion.id)
            })
    }

    pub(crate) fn active_auto_response_suggestion(
        &self,
    ) -> Option<&crate::state::chat::ThreadParticipantSuggestionVm> {
        if self.assistant_busy() || self.active_always_auto_response_participant().is_some() {
            return None;
        }
        self.queued_active_auto_response_suggestion()
    }

    pub(crate) fn active_auto_response_countdown_secs(&self) -> Option<u64> {
        let suggestion = self.active_auto_response_suggestion()?;
        let due_at = suggestion.auto_send_at?;
        let now = Self::current_unix_ms().max(0) as u64;
        Some(due_at.saturating_sub(now).div_ceil(1000))
    }

    pub(crate) fn execute_active_auto_response_action(
        &mut self,
        selection: AutoResponseActionSelection,
    ) -> bool {
        let Some(suggestion) = self.queued_active_auto_response_suggestion().cloned() else {
            return false;
        };
        let Some(thread_id) = self.chat.active_thread_id().map(str::to_string) else {
            return false;
        };
        self.hidden_auto_response_suggestion_ids
            .insert(suggestion.id.clone());
        self.auto_response_selection = AutoResponseActionSelection::Yes;
        match selection {
            AutoResponseActionSelection::Yes => {
                self.send_daemon_command(DaemonCommand::SendParticipantSuggestion {
                    thread_id,
                    suggestion_id: suggestion.id,
                });
                self.status_line = format!(
                    "Auto response requested from {}",
                    suggestion.target_agent_name
                );
            }
            AutoResponseActionSelection::No => {
                self.send_daemon_command(DaemonCommand::DismissParticipantSuggestion {
                    thread_id,
                    suggestion_id: suggestion.id,
                });
                self.status_line = "Auto response dismissed".to_string();
            }
            AutoResponseActionSelection::Always => {
                self.send_daemon_command(DaemonCommand::ThreadParticipantCommand {
                    thread_id: thread_id.clone(),
                    target_agent_id: suggestion.target_agent_id.clone(),
                    action: "auto_response_always".to_string(),
                    instruction: None,
                    session_id: None,
                });
                self.send_daemon_command(DaemonCommand::SendParticipantSuggestion {
                    thread_id,
                    suggestion_id: suggestion.id,
                });
                self.status_line = format!(
                    "Auto response always enabled for {}",
                    suggestion.target_agent_name
                );
            }
        }
        true
    }

    fn latest_visible_main_agent_message_timestamp_for_auto_response(
        &self,
    ) -> Option<(u64, String)> {
        let thread = self.chat.active_thread()?;
        let latest_message = thread.messages.last()?;
        if latest_message.role != chat::MessageRole::Assistant {
            return None;
        }
        if latest_message.content.trim().is_empty() {
            return None;
        }
        let authored_by_participant =
            latest_message
                .author_agent_id
                .as_ref()
                .is_some_and(|author_id| {
                    thread
                        .thread_participants
                        .iter()
                        .any(|participant| participant.agent_id.eq_ignore_ascii_case(author_id))
                });
        (!authored_by_participant).then(|| {
            (
                latest_message.timestamp,
                latest_message.content.trim().to_string(),
            )
        })
    }

    fn active_auto_response_request_target(
        &self,
    ) -> Option<&crate::state::chat::ThreadParticipantState> {
        let thread = self.chat.active_thread()?;
        if let Some(always_participant) = self.active_always_auto_response_participant() {
            return Some(always_participant);
        }
        thread
            .thread_participants
            .iter()
            .filter(|participant| participant.status.eq_ignore_ascii_case("active"))
            .max_by(|left, right| {
                left.last_contribution_at
                    .unwrap_or(0)
                    .cmp(&right.last_contribution_at.unwrap_or(0))
                    .then_with(|| left.updated_at.cmp(&right.updated_at))
                    .then_with(|| left.created_at.cmp(&right.created_at))
                    .then_with(|| left.agent_id.cmp(&right.agent_id))
            })
    }

    pub(crate) fn maybe_request_auto_response_for_open_thread(&mut self, thread_id: &str) -> bool {
        if self.assistant_busy() || self.chat.active_thread_id() != Some(thread_id) {
            return false;
        }
        let Some((source_message_timestamp, _)) =
            self.latest_visible_main_agent_message_timestamp_for_auto_response()
        else {
            return false;
        };
        let Some(target_participant) = self.active_auto_response_request_target() else {
            return false;
        };
        let Some(thread) = self.chat.active_thread() else {
            return false;
        };
        let has_matching_suggestion =
            thread
                .queued_participant_suggestions
                .iter()
                .any(|suggestion| {
                    suggestion
                        .suggestion_kind
                        .eq_ignore_ascii_case("auto_response")
                        && suggestion.status.eq_ignore_ascii_case("queued")
                        && suggestion
                            .target_agent_id
                            .eq_ignore_ascii_case(&target_participant.agent_id)
                        && suggestion.source_message_timestamp == Some(source_message_timestamp)
                });
        if has_matching_suggestion {
            return false;
        }
        self.send_daemon_command(DaemonCommand::ThreadParticipantCommand {
            thread_id: thread_id.to_string(),
            target_agent_id: target_participant.agent_id.clone(),
            action: "auto_response".to_string(),
            instruction: None,
            session_id: None,
        });
        true
    }

    pub(crate) fn maybe_auto_send_always_auto_response(&mut self) -> bool {
        if self.assistant_busy() || self.active_always_auto_response_participant().is_none() {
            return false;
        }
        self.execute_active_auto_response_action(AutoResponseActionSelection::Yes)
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
        self.prompt_modal_title_override = None;
        self.prompt_modal_body_override = None;
        if self.modal.top() != Some(modal::ModalKind::PromptViewer) {
            self.modal
                .reduce(modal::ModalAction::Push(modal::ModalKind::PromptViewer));
        }
    }

    pub(crate) fn open_statistics_modal_loading(&mut self) {
        self.statistics_modal_loading = true;
        self.statistics_modal_snapshot = None;
        self.statistics_modal_error = None;
        self.statistics_modal_scroll = 0;
        if self.modal.top() != Some(modal::ModalKind::Statistics) {
            self.modal
                .reduce(modal::ModalAction::Push(modal::ModalKind::Statistics));
        }
    }

    pub(super) fn open_pinned_budget_exceeded_modal(
        &mut self,
        payload: PendingPinnedBudgetExceeded,
    ) {
        self.pending_pinned_budget_exceeded = Some(payload);
        if self.modal.top() != Some(modal::ModalKind::PinnedBudgetExceeded) {
            self.modal.reduce(modal::ModalAction::Push(
                modal::ModalKind::PinnedBudgetExceeded,
            ));
        }
    }

    pub(super) fn close_pinned_budget_exceeded_modal(&mut self) {
        self.pending_pinned_budget_exceeded = None;
        if self.modal.top() == Some(modal::ModalKind::PinnedBudgetExceeded) {
            self.close_top_modal();
        }
    }

    pub(crate) fn status_modal_body(&self) -> String {
        if self.status_modal_loading {
            return "Loading zorai status...".to_string();
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

    pub(crate) fn statistics_modal_body(&self) -> String {
        if self.statistics_modal_loading {
            return "Loading historical statistics...".to_string();
        }
        if let Some(error) = &self.statistics_modal_error {
            return format!("Statistics request failed\n=========================\n{error}");
        }
        if let Some(snapshot) = &self.statistics_modal_snapshot {
            return widgets::statistics::format_statistics_body(
                snapshot,
                self.statistics_modal_tab,
            );
        }
        "No statistics available.".to_string()
    }

    pub(crate) fn prompt_modal_body(&self) -> String {
        if let Some(body) = &self.prompt_modal_body_override {
            return body.clone();
        }
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

    pub(crate) fn prompt_modal_title(&self) -> &str {
        self.prompt_modal_title_override
            .as_deref()
            .unwrap_or("PROMPT")
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

    pub(crate) fn statistics_modal_max_scroll(&self) -> usize {
        let body = self.statistics_modal_body();
        let (viewport_lines, inner_width) = self
            .current_modal_area()
            .filter(|(kind, _)| *kind == modal::ModalKind::Statistics)
            .map(|(_, area)| {
                (
                    area.height.saturating_sub(5) as usize,
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

    pub(crate) fn set_statistics_modal_scroll(&mut self, scroll: usize) {
        self.statistics_modal_scroll = scroll.min(self.statistics_modal_max_scroll());
    }

    pub(crate) fn step_status_modal_scroll(&mut self, delta: i32) {
        let current = self.status_modal_scroll as i32;
        let next = (current + delta).max(0) as usize;
        self.set_status_modal_scroll(next);
    }

    pub(crate) fn step_statistics_modal_scroll(&mut self, delta: i32) {
        let current = self.statistics_modal_scroll as i32;
        let next = (current + delta).max(0) as usize;
        self.set_statistics_modal_scroll(next);
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

    pub(crate) fn page_statistics_modal_scroll(&mut self, direction: i32) {
        let page = self
            .current_modal_area()
            .filter(|(kind, _)| *kind == modal::ModalKind::Statistics)
            .map(|(_, area)| area.height.saturating_sub(6) as i32)
            .unwrap_or(10)
            .max(1);
        self.step_statistics_modal_scroll(page * direction);
    }

    pub(crate) fn set_prompt_modal_scroll(&mut self, scroll: usize) {
        self.prompt_modal_scroll = scroll.min(self.prompt_modal_max_scroll());
    }

    pub(crate) fn settings_modal_max_scroll(&self) -> usize {
        self.current_modal_area()
            .filter(|(kind, _)| *kind == modal::ModalKind::Settings)
            .map(|(_, area)| {
                widgets::settings::max_scroll(
                    area,
                    &self.settings,
                    &self.config,
                    &self.modal,
                    &self.auth,
                    &self.subagents,
                    &self.concierge,
                    &self.tier,
                    &self.plugin_settings,
                    &self.theme,
                )
            })
            .unwrap_or(0)
    }

    pub(crate) fn set_settings_modal_scroll(&mut self, scroll: usize) {
        self.settings_modal_scroll = scroll.min(self.settings_modal_max_scroll());
    }

    pub(crate) fn step_settings_modal_scroll(&mut self, delta: i32) {
        let current = self.settings_modal_scroll as i32;
        let next = (current + delta).max(0) as usize;
        self.set_settings_modal_scroll(next);
    }

    pub(crate) fn page_settings_modal_scroll(&mut self, direction: i32) {
        let page = self
            .current_modal_area()
            .filter(|(kind, _)| *kind == modal::ModalKind::Settings)
            .map(|(_, area)| area.height.saturating_sub(4) as i32)
            .unwrap_or(10)
            .max(1);
        self.step_settings_modal_scroll(page * direction);
    }

    pub(crate) fn sync_settings_modal_scroll_to_selection(&mut self) {
        let Some((kind, area)) = self.current_modal_area() else {
            return;
        };
        if kind != modal::ModalKind::Settings {
            return;
        }

        self.settings_modal_scroll = widgets::settings::scroll_for_selected_field(
            area,
            &self.settings,
            &self.config,
            &self.modal,
            &self.auth,
            &self.subagents,
            &self.concierge,
            &self.tier,
            &self.plugin_settings,
            self.settings_modal_scroll,
            &self.theme,
        );
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
            self.modal.reduce(modal::ModalAction::Push(
                modal::ModalKind::ThreadParticipants,
            ));
        }
    }

    pub(crate) fn request_prompt_inspection(&mut self, agent_id: Option<String>) {
        self.open_prompt_modal_loading();
        self.send_daemon_command(DaemonCommand::RequestPromptInspection { agent_id });
        self.status_line = "Requesting assembled agent prompt...".to_string();
    }

    pub(crate) fn request_statistics_window(
        &mut self,
        window: zorai_protocol::AgentStatisticsWindow,
    ) {
        self.statistics_modal_window = window;
        self.open_statistics_modal_loading();
        self.send_daemon_command(DaemonCommand::RequestAgentStatistics { window });
        self.status_line = format!("Loading statistics for {}...", window.as_str());
    }

    pub(crate) fn select_statistics_tab(&mut self, tab: crate::state::statistics::StatisticsTab) {
        self.statistics_modal_tab = tab;
        self.statistics_modal_scroll = 0;
    }

    pub(crate) fn cycle_statistics_tab(&mut self, direction: i32) {
        let next = if direction >= 0 {
            self.statistics_modal_tab.next()
        } else {
            self.statistics_modal_tab.prev()
        };
        self.select_statistics_tab(next);
    }

    pub(crate) fn cycle_statistics_window(&mut self, direction: i32) {
        let windows = [
            zorai_protocol::AgentStatisticsWindow::Today,
            zorai_protocol::AgentStatisticsWindow::Last7Days,
            zorai_protocol::AgentStatisticsWindow::Last30Days,
            zorai_protocol::AgentStatisticsWindow::All,
        ];
        let current_index = windows
            .iter()
            .position(|window| *window == self.statistics_modal_window)
            .unwrap_or(3) as i32;
        let len = windows.len() as i32;
        let next_index = (current_index + direction).rem_euclid(len) as usize;
        self.request_statistics_window(windows[next_index]);
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

    fn current_thread_agent_activity(&self) -> Option<&str> {
        self.chat
            .active_thread_id()
            .and_then(|thread_id| self.thread_agent_activity.get(thread_id))
            .map(String::as_str)
            .or(self.agent_activity.as_deref())
    }

    fn set_agent_activity_for(&mut self, thread_id: Option<String>, activity: impl Into<String>) {
        let activity = activity.into();
        if let Some(thread_id) = thread_id {
            if activity != "thinking" {
                self.clear_pending_prompt_response_thread(thread_id.as_str());
            }
            self.thread_agent_activity.insert(thread_id, activity);
        } else {
            self.agent_activity = Some(activity);
        }
    }

    fn set_active_thread_activity(&mut self, activity: impl Into<String>) {
        self.set_agent_activity_for(self.chat.active_thread_id().map(str::to_string), activity);
    }

    fn mark_bootstrap_pending_activity_thread(&mut self, thread_id: impl Into<String>) {
        self.bootstrap_pending_activity_threads
            .insert(thread_id.into());
    }

    fn mark_pending_prompt_response_thread(&mut self, thread_id: impl Into<String>) {
        self.pending_prompt_response_threads.insert(thread_id.into());
    }

    fn clear_bootstrap_pending_activity_thread(&mut self, thread_id: &str) {
        self.bootstrap_pending_activity_threads.remove(thread_id);
    }

    fn clear_pending_prompt_response_thread(&mut self, thread_id: &str) {
        self.pending_prompt_response_threads.remove(thread_id);
    }

    fn should_preserve_pending_thinking_activity_on_reload(&self, thread_id: &str) -> bool {
        if self
            .thread_agent_activity
            .get(thread_id)
            .map(String::as_str)
            != Some("thinking")
            || self.chat.is_streaming()
        {
            return false;
        }

        if self.pending_prompt_response_threads.contains(thread_id) {
            return true;
        }

        if self.bootstrap_pending_activity_threads.contains(thread_id) {
            return true;
        }

        self.chat.active_thread_id() == Some(thread_id)
            && self
                .chat
                .active_thread()
                .and_then(|thread| {
                    thread
                        .messages
                        .iter()
                        .rev()
                        .find(|message| {
                            !message.content.trim().is_empty()
                                && !matches!(message.role, chat::MessageRole::System)
                        })
                        .map(|message| message.role == chat::MessageRole::User)
                })
                .unwrap_or(false)
    }

    fn clear_agent_activity_for(&mut self, thread_id: Option<&str>) {
        if let Some(thread_id) = thread_id {
            self.thread_agent_activity.remove(thread_id);
        } else {
            self.agent_activity = None;
        }
    }

    fn clear_active_thread_activity(&mut self) {
        let thread_id = self.chat.active_thread_id().map(str::to_string);
        if let Some(thread_id) = thread_id.as_deref() {
            self.clear_pending_prompt_response_thread(thread_id);
        }
        self.clear_agent_activity_for(thread_id.as_deref());
    }

    fn clear_all_agent_activity(&mut self) {
        self.agent_activity = None;
        self.thread_agent_activity.clear();
        self.pending_prompt_response_threads.clear();
    }

    fn clear_matching_agent_activity(&mut self, target: &str) {
        if self.agent_activity.as_deref() == Some(target) {
            self.agent_activity = None;
        }
        self.thread_agent_activity
            .retain(|_, activity| activity != target);
    }

    fn assistant_busy(&self) -> bool {
        self.chat.is_streaming() || self.current_thread_agent_activity().is_some()
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
        if !matches!(self.main_pane_view, MainPaneView::Conversation) {
            return false;
        }
        if self.should_show_local_landing() {
            return false;
        }
        if self.should_show_operator_profile_onboarding() {
            return false;
        }

        self.concierge.loading || !self.chat.active_actions().is_empty()
    }

    fn should_show_daemon_connection_loading(&self) -> bool {
        (!self.connected || !self.agent_config_loaded)
            && matches!(self.main_pane_view, MainPaneView::Conversation)
            && !self.should_show_operator_profile_onboarding()
            && !self.should_show_provider_onboarding()
    }

    fn should_show_local_landing(&self) -> bool {
        self.connected
            && self.agent_config_loaded
            && matches!(self.main_pane_view, MainPaneView::Conversation)
            && self.chat.active_thread().is_none()
            && !self.has_mission_control_return_target()
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
        0
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
        self.settings_modal_scroll = 0;
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
        self.sync_goal_mission_control_prompt_from_input();
    }

    fn close_top_modal(&mut self) {
        if self.modal.top() == Some(modal::ModalKind::OpenAIAuth) {
            self.openai_auth_url = None;
            self.openai_auth_status_text = None;
        }
        if self.modal.top() == Some(modal::ModalKind::PromptViewer) {
            self.prompt_modal_title_override = None;
            self.prompt_modal_body_override = None;
            self.prompt_modal_scroll = 0;
        }
        if self.modal.top() == Some(modal::ModalKind::WhatsAppLink) {
            if self.modal.whatsapp_link().phase() != modal::WhatsAppLinkPhase::Connected {
                self.send_daemon_command(DaemonCommand::WhatsAppLinkStop);
            }
            self.send_daemon_command(DaemonCommand::WhatsAppLinkUnsubscribe);
            self.modal.reset_whatsapp_link();
        }
        if self.modal.top() == Some(modal::ModalKind::PinnedBudgetExceeded) {
            self.pending_pinned_budget_exceeded = None;
        }
        if self.modal.top() == Some(modal::ModalKind::Settings) {
            self.settings_modal_scroll = 0;
        }
        if self.modal.top() == Some(modal::ModalKind::WorkspaceActorPicker) {
            self.pending_workspace_actor_picker = None;
        }
        if self.modal.top() == Some(modal::ModalKind::WorkspaceCreateTask) {
            self.pending_workspace_create_form = None;
        }
        if self.modal.top() == Some(modal::ModalKind::WorkspaceReviewTask) {
            self.pending_workspace_review_form = None;
        }
        if self.modal.top() == Some(modal::ModalKind::WorkspaceEditTask) {
            self.pending_workspace_edit_form = None;
            self.workspace_edit_modal_scroll = 0;
        }
        if self.modal.top() == Some(modal::ModalKind::WorkspaceTaskDetail) {
            self.pending_workspace_detail_task_id = None;
        }
        if self.modal.top() == Some(modal::ModalKind::WorkspaceTaskHistory) {
            self.pending_workspace_history_task_id = None;
        }
        self.modal.reduce(modal::ModalAction::Pop);
    }
}
