use super::*;
use amux_shared::providers::{PROVIDER_ID_CHATGPT_SUBSCRIPTION, PROVIDER_ID_OPENAI};
use std::path::{Path, PathBuf};

#[path = "commands_goal_targets.rs"]
mod goal_targets;

impl TuiModel {
    pub(super) fn known_agent_directive_aliases(&self) -> Vec<String> {
        let mut aliases = vec![
            "main".to_string(),
            "svarog".to_string(),
            "swarog".to_string(),
            "weles".to_string(),
            amux_protocol::AGENT_ID_RAROG.to_string(),
            amux_protocol::AGENT_NAME_RAROG.to_string(),
            "swarozyc".to_string(),
            "Swarozyc".to_string(),
            "radogost".to_string(),
            "Radogost".to_string(),
            "domowoj".to_string(),
            "Domowoj".to_string(),
            "swietowit".to_string(),
            "Swietowit".to_string(),
        ];
        for entry in &self.subagents.entries {
            aliases.push(entry.id.clone());
            aliases.push(entry.name.clone());
        }
        aliases.sort();
        aliases.dedup_by(|left, right| left.eq_ignore_ascii_case(right));
        aliases
    }

    fn participant_display_name(&self, agent_alias: &str) -> String {
        if agent_alias.eq_ignore_ascii_case(amux_protocol::AGENT_ID_RAROG)
            || agent_alias.eq_ignore_ascii_case(amux_protocol::AGENT_NAME_RAROG)
        {
            return amux_protocol::AGENT_NAME_RAROG.to_string();
        }
        if agent_alias.eq_ignore_ascii_case("weles") {
            return "Weles".to_string();
        }
        if agent_alias.eq_ignore_ascii_case("swarozyc") {
            return "Swarozyc".to_string();
        }
        if agent_alias.eq_ignore_ascii_case("radogost") {
            return "Radogost".to_string();
        }
        if agent_alias.eq_ignore_ascii_case("domowoj") {
            return "Domowoj".to_string();
        }
        if agent_alias.eq_ignore_ascii_case("swietowit") {
            return "Swietowit".to_string();
        }
        if let Some(entry) = self.subagents.entries.iter().find(|entry| {
            entry.id.eq_ignore_ascii_case(agent_alias)
                || entry.name.eq_ignore_ascii_case(agent_alias)
        }) {
            return entry.name.clone();
        }
        agent_alias.to_string()
    }

    fn builtin_persona_configured(&self, agent_alias: &str) -> bool {
        let Some(raw) = self.config.agent_config_raw.as_ref() else {
            return false;
        };
        let key = match agent_alias.to_ascii_lowercase().as_str() {
            "swarozyc" => "swarozyc",
            "radogost" => "radogost",
            "domowoj" => "domowoj",
            "swietowit" => "swietowit",
            _ => return true,
        };
        let Some(entry) = raw
            .get("builtin_sub_agents")
            .and_then(|value| value.get(key))
            .and_then(|value| value.as_object())
        else {
            return false;
        };
        let provider = entry
            .get("provider")
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty());
        let model = entry
            .get("model")
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty());
        provider.is_some() && model.is_some()
    }

    fn open_builtin_persona_setup_flow(&mut self, agent_alias: &str, prompt: String) {
        let target_agent_id = agent_alias.trim().to_ascii_lowercase();
        let target_agent_name = self.participant_display_name(agent_alias);
        let config_snapshot = BuiltinPersonaSetupConfigSnapshot {
            provider: self.config.provider.clone(),
            base_url: self.config.base_url.clone(),
            model: self.config.model.clone(),
            custom_model_name: self.config.custom_model_name.clone(),
            api_key: self.config.api_key.clone(),
            assistant_id: self.config.assistant_id.clone(),
            auth_source: self.config.auth_source.clone(),
            api_transport: self.config.api_transport.clone(),
            custom_context_window_tokens: self.config.custom_context_window_tokens,
            context_window_tokens: self.config.context_window_tokens,
            fetched_models: self.config.fetched_models().to_vec(),
        };
        self.pending_builtin_persona_setup = Some(PendingBuiltinPersonaSetup {
            target_agent_id,
            target_agent_name: target_agent_name.clone(),
            prompt,
            config_snapshot,
        });
        self.settings_picker_target = Some(SettingsPickerTarget::BuiltinPersonaProvider);
        self.modal
            .reduce(modal::ModalAction::Push(modal::ModalKind::ProviderPicker));
        self.modal.set_picker_item_count(
            widgets::provider_picker::available_provider_defs(&self.auth).len(),
        );
        self.status_line = format!("Configure {} provider", target_agent_name);
    }

    pub(super) fn restore_builtin_persona_setup_config_snapshot(&mut self) {
        let Some(setup) = self.pending_builtin_persona_setup.as_ref() else {
            return;
        };
        let snapshot = &setup.config_snapshot;
        self.config.provider = snapshot.provider.clone();
        self.config.base_url = snapshot.base_url.clone();
        self.config.model = snapshot.model.clone();
        self.config.custom_model_name = snapshot.custom_model_name.clone();
        self.config.api_key = snapshot.api_key.clone();
        self.config.assistant_id = snapshot.assistant_id.clone();
        self.config.auth_source = snapshot.auth_source.clone();
        self.config.api_transport = snapshot.api_transport.clone();
        self.config.custom_context_window_tokens = snapshot.custom_context_window_tokens;
        self.config.context_window_tokens = snapshot.context_window_tokens;
        self.config.reduce(config::ConfigAction::ModelsFetched(
            snapshot.fetched_models.clone(),
        ));
    }

    fn resolve_preview_path(path: &str) -> PathBuf {
        let raw = PathBuf::from(path);
        if raw.is_absolute() {
            raw
        } else {
            std::env::current_dir()
                .unwrap_or_else(|_| PathBuf::from("."))
                .join(raw)
        }
    }

    fn find_repo_root(path: &Path) -> Option<PathBuf> {
        let mut current = path.parent().or_else(|| Some(path));
        while let Some(candidate) = current {
            if candidate.join(".git").exists() {
                return Some(candidate.to_path_buf());
            }
            current = candidate.parent();
        }
        None
    }

    pub(super) fn open_chat_tool_file_preview(&mut self, message_index: usize) {
        let Some(message) = self
            .chat
            .active_thread()
            .and_then(|thread| thread.messages.get(message_index))
        else {
            return;
        };
        let Some(chip) = widgets::chat::tool_file_chip(message) else {
            return;
        };

        let resolved_path = Self::resolve_preview_path(&chip.path);
        let show_plain_preview = chip.tool_name == "read_file";
        let repo_root = if show_plain_preview {
            None
        } else {
            Self::find_repo_root(&resolved_path)
        };
        let repo_relative_path = repo_root.as_ref().and_then(|root| {
            resolved_path
                .strip_prefix(root)
                .ok()
                .map(|path| path.to_string_lossy().to_string())
        });
        let target = ChatFilePreviewTarget {
            path: resolved_path.to_string_lossy().to_string(),
            repo_root: repo_root
                .as_ref()
                .map(|path| path.to_string_lossy().to_string()),
            repo_relative_path,
        };

        if let Some(repo_root) = target.repo_root.as_ref() {
            self.send_daemon_command(DaemonCommand::RequestGitDiff {
                repo_path: repo_root.clone(),
                file_path: target.repo_relative_path.clone(),
            });
        } else {
            self.send_daemon_command(DaemonCommand::RequestFilePreview {
                path: target.path.clone(),
                max_bytes: Some(65_536),
            });
        }

        self.main_pane_view = MainPaneView::FilePreview(target);
        self.task_view_scroll = 0;
        self.focus = FocusArea::Chat;
    }

    pub(super) fn filtered_goal_runs(&self) -> Vec<&task::GoalRun> {
        let query = self.modal.command_query().to_lowercase();
        self.tasks
            .goal_runs()
            .iter()
            .filter(|run| {
                query.is_empty()
                    || run.title.to_lowercase().contains(&query)
                    || run.goal.to_lowercase().contains(&query)
            })
            .collect()
    }

    pub(super) fn request_preview_for_selected_path(&mut self, thread_id: &str) {
        let Some(context) = self.tasks.work_context_for_thread(thread_id) else {
            return;
        };
        let Some(selected_path) = self.tasks.selected_work_path(thread_id) else {
            return;
        };
        let Some(entry) = context
            .entries
            .iter()
            .find(|entry| entry.path == selected_path)
        else {
            return;
        };
        if let Some(repo_root) = entry.repo_root.as_deref() {
            self.send_daemon_command(DaemonCommand::RequestGitDiff {
                repo_path: repo_root.to_string(),
                file_path: Some(entry.path.clone()),
            });
        } else {
            self.send_daemon_command(DaemonCommand::RequestFilePreview {
                path: entry.path.clone(),
                max_bytes: Some(65_536),
            });
        }
    }

    pub(super) fn ensure_task_view_preview(&mut self) {
        let MainPaneView::Task(target) = &self.main_pane_view else {
            return;
        };
        let Some(thread_id) = self.target_thread_id(target) else {
            return;
        };
        if self.tasks.selected_work_path(&thread_id).is_none() {
            if let Some(context) = self.tasks.work_context_for_thread(&thread_id) {
                if let Some(first) = context.entries.first() {
                    self.tasks.reduce(task::TaskAction::SelectWorkPath {
                        thread_id: thread_id.clone(),
                        path: Some(first.path.clone()),
                    });
                }
            }
        }
        self.request_preview_for_selected_path(&thread_id);
    }

    fn request_task_view_context(&mut self, target: &sidebar::SidebarItemTarget) {
        if let Some(thread_id) = self.target_thread_id(target) {
            self.send_daemon_command(DaemonCommand::RequestThreadTodos(thread_id.clone()));
            self.send_daemon_command(DaemonCommand::RequestThreadWorkContext(thread_id));
        }
    }

    pub(super) fn sidebar_item_count(&self) -> usize {
        widgets::sidebar::body_item_count(&self.tasks, &self.sidebar, self.chat.active_thread_id())
    }

    pub(super) fn open_sidebar_target(&mut self, target: sidebar::SidebarItemTarget) {
        self.cleanup_concierge_on_navigate();
        if let sidebar::SidebarItemTarget::GoalRun { goal_run_id, .. } = &target {
            self.send_daemon_command(DaemonCommand::RequestGoalRunDetail(goal_run_id.clone()));
            self.send_daemon_command(DaemonCommand::RequestGoalRunCheckpoints(
                goal_run_id.clone(),
            ));
        }
        self.request_task_view_context(&target);
        self.main_pane_view = MainPaneView::Task(target);
        self.task_view_scroll = 0;
    }

    pub(super) fn sync_thread_picker_item_count(&mut self) {
        let count = widgets::thread_picker::filtered_threads(&self.chat, &self.modal).len() + 1;
        self.modal.set_picker_item_count(count);
    }

    pub(super) fn sync_goal_picker_item_count(&mut self) {
        self.modal
            .set_picker_item_count(self.filtered_goal_runs().len() + 1);
    }

    pub(crate) fn open_queued_prompts_modal(&mut self) {
        if self.queued_prompts.is_empty() {
            self.status_line = "No queued messages".to_string();
            return;
        }
        if self.modal.top() != Some(modal::ModalKind::QueuedPrompts) {
            self.modal
                .reduce(modal::ModalAction::Push(modal::ModalKind::QueuedPrompts));
        }
        self.modal.set_picker_item_count(self.queued_prompts.len());
        self.queued_prompt_action = QueuedPromptAction::SendNow;
    }

    fn queue_prompt(&mut self, prompt: String) {
        self.queued_prompts.push(QueuedPrompt::new(prompt));
        self.status_line = format!("QUEUED ({})", self.queued_prompts.len());
        self.sync_queued_prompt_modal_state();
    }

    pub(super) fn queue_participant_suggestion(
        &mut self,
        thread_id: String,
        suggestion_id: String,
        target_agent_id: String,
        target_agent_name: String,
        prompt: String,
        force_send: bool,
    ) {
        self.queued_prompts.push(QueuedPrompt::new_with_agent(
            prompt,
            thread_id,
            suggestion_id,
            target_agent_id,
            target_agent_name,
            force_send,
        ));
        self.status_line = format!("QUEUED ({})", self.queued_prompts.len());
        self.sync_queued_prompt_modal_state();
    }

    fn pop_next_queued_prompt(&mut self) -> Option<QueuedPrompt> {
        if self.queued_prompts.is_empty() {
            return None;
        }
        let prompt = self.queued_prompts.remove(0);
        self.sync_queued_prompt_modal_state();
        Some(prompt)
    }

    fn remove_queued_prompt_at(&mut self, index: usize) -> Option<QueuedPrompt> {
        if index >= self.queued_prompts.len() {
            return None;
        }
        let prompt = self.queued_prompts.remove(index);
        self.sync_queued_prompt_modal_state();
        Some(prompt)
    }

    pub(super) fn dispatch_next_queued_prompt_if_ready(&mut self) {
        if self.queue_barrier_active() {
            return;
        }
        if let Some(prompt) = self.pop_next_queued_prompt() {
            self.submit_prompt(prompt.text);
        }
    }

    fn interrupt_current_stream(&mut self) {
        let Some(thread_id) = self.chat.active_thread_id().map(str::to_string) else {
            return;
        };
        self.cancelled_thread_id = Some(thread_id.clone());
        self.chat.reduce(chat::ChatAction::ForceStopStreaming);
        self.agent_activity = None;
        self.pending_stop = false;
        self.send_daemon_command(DaemonCommand::StopStream { thread_id });
    }

    pub(super) fn execute_selected_queued_prompt_action(&mut self) {
        let index = self.modal.picker_cursor();
        let action = self.queued_prompt_action;
        match action {
            QueuedPromptAction::SendNow => {
                let Some(prompt) = self.remove_queued_prompt_at(index) else {
                    return;
                };
                if self.assistant_busy() && prompt.force_send {
                    self.interrupt_current_stream();
                }
                if let (Some(thread_id), Some(suggestion_id)) =
                    (prompt.thread_id.clone(), prompt.suggestion_id.clone())
                {
                    self.send_daemon_command(DaemonCommand::SendParticipantSuggestion {
                        thread_id,
                        suggestion_id,
                    });
                } else {
                    self.submit_prompt(prompt.text);
                }
            }
            QueuedPromptAction::Copy => {
                let Some(prompt) = self.queued_prompts.get_mut(index) else {
                    return;
                };
                conversion::copy_to_clipboard(&prompt.text);
                prompt.mark_copied(self.tick_counter.saturating_add(100));
                self.status_line = "Copied queued message".to_string();
            }
            QueuedPromptAction::Delete => {
                if let Some(prompt) = self.remove_queued_prompt_at(index) {
                    if let (Some(thread_id), Some(suggestion_id)) =
                        (prompt.thread_id, prompt.suggestion_id)
                    {
                        self.send_daemon_command(DaemonCommand::DismissParticipantSuggestion {
                            thread_id,
                            suggestion_id,
                        });
                    }
                    self.status_line = "Removed queued message".to_string();
                }
            }
        }
    }

    pub(super) fn open_new_goal_view(&mut self) {
        self.cleanup_concierge_on_navigate();
        self.main_pane_view = MainPaneView::GoalComposer;
        self.task_view_scroll = 0;
        self.focus = FocusArea::Input;
        self.input.reduce(input::InputAction::Clear);
        self.attachments.clear();
        self.status_line = "Describe the goal in the input and press Enter".to_string();
    }

    pub(super) fn start_goal_run_from_prompt(&mut self, goal: String) {
        if !self.connected {
            self.status_line = "Not connected to daemon".to_string();
            return;
        }
        self.cleanup_concierge_on_navigate();
        self.send_daemon_command(DaemonCommand::StartGoalRun {
            goal,
            thread_id: None,
            session_id: None,
        });
        self.status_line = "Starting goal run...".to_string();
    }

    pub(super) fn is_builtin_command(&self, command: &str) -> bool {
        matches!(
            command,
            "provider"
                | "model"
                | "tools"
                | "effort"
                | "thread"
                | "new"
                | "goals"
                | "tasks"
                | "conversation"
                | "chat"
                | "settings"
                | "view"
                | "status"
                | "notifications"
                | "approvals"
                | "participants"
                | "quit"
                | "prompt"
                | "goal"
                | "attach"
                | "plugins install"
                | "skills install"
                | "help"
                | "explain"
                | "diverge"
        )
    }

    pub(super) fn execute_command(&mut self, command: &str) {
        tracing::info!("execute_command: {:?}", command);
        match command {
            "provider" => {
                self.modal
                    .reduce(modal::ModalAction::Push(modal::ModalKind::ProviderPicker));
                self.modal.set_picker_item_count(
                    widgets::provider_picker::available_provider_defs(&self.auth).len(),
                );
            }
            "model" => {
                let models = providers::known_models_for_provider_auth(
                    &self.config.provider,
                    &self.config.auth_source,
                );
                if !models.is_empty() {
                    self.config
                        .reduce(config::ConfigAction::ModelsFetched(models));
                }
                if !(self.config.provider == PROVIDER_ID_OPENAI
                    && self.config.auth_source == PROVIDER_ID_CHATGPT_SUBSCRIPTION)
                    && providers::supports_model_fetch_for(&self.config.provider)
                {
                    self.send_daemon_command(DaemonCommand::FetchModels {
                        provider_id: self.config.provider.clone(),
                        base_url: self.config.base_url.clone(),
                        api_key: self.config.api_key.clone(),
                    });
                }
                let count = widgets::model_picker::available_models(&self.config).len() + 1;
                self.modal
                    .reduce(modal::ModalAction::Push(modal::ModalKind::ModelPicker));
                self.modal.set_picker_item_count(count);
            }
            "tools" => {
                self.open_settings_tab(SettingsTab::Tools);
            }
            "effort" => {
                self.modal
                    .reduce(modal::ModalAction::Push(modal::ModalKind::EffortPicker));
                self.modal.set_picker_item_count(6);
            }
            "thread" => {
                self.modal
                    .reduce(modal::ModalAction::Push(modal::ModalKind::ThreadPicker));
                self.sync_thread_picker_item_count();
            }
            "goals" => {
                self.modal
                    .reduce(modal::ModalAction::Push(modal::ModalKind::GoalPicker));
                self.sync_goal_picker_item_count();
            }
            "new" => {
                self.start_new_thread_view();
            }
            "tasks" => {
                self.modal
                    .reduce(modal::ModalAction::Push(modal::ModalKind::GoalPicker));
                self.sync_goal_picker_item_count();
            }
            "conversation" | "chat" => {
                self.main_pane_view = MainPaneView::Conversation;
            }
            "settings" => {
                self.open_settings_tab(SettingsTab::Auth);
            }
            "view" => {
                let next = match self.chat.transcript_mode() {
                    chat::TranscriptMode::Compact => chat::TranscriptMode::Tools,
                    chat::TranscriptMode::Tools => chat::TranscriptMode::Full,
                    chat::TranscriptMode::Full => chat::TranscriptMode::Compact,
                };
                self.chat.reduce(chat::ChatAction::SetTranscriptMode(next));
                self.status_line = format!("View: {:?}", next);
            }
            "status" => {
                self.open_status_modal_loading();
                self.send_daemon_command(DaemonCommand::RequestAgentStatus);
                self.status_line = "Requesting tamux status...".to_string();
            }
            "notifications" => {
                self.toggle_notifications_modal();
                self.status_line = "Viewing notifications".to_string();
            }
            "approvals" => {
                self.toggle_approval_center();
                self.status_line = "Viewing approvals".to_string();
            }
            "participants" => {
                self.open_thread_participants_modal();
                self.status_line = "Viewing thread participants".to_string();
            }
            "quit" => self.pending_quit = true,
            "prompt" => {
                self.request_prompt_inspection(None);
            }
            "goal" => {
                self.open_new_goal_view();
            }
            "attach" => {
                self.status_line =
                    "Usage: /attach <path>  — attach a file to the next message".to_string();
            }
            "plugins install" => {
                self.input.set_text("tamux install plugin ");
                self.focus = FocusArea::Input;
                self.status_line = "Edit the plugin source and run it in the terminal".to_string();
            }
            "skills install" => {
                self.input.set_text("tamux skill import ");
                self.focus = FocusArea::Input;
                self.status_line = "Edit the skill source and run it in the terminal".to_string();
            }
            "help" => {
                self.help_modal_scroll = 0;
                self.modal
                    .reduce(modal::ModalAction::Push(modal::ModalKind::Help));
                self.modal.set_picker_item_count(100);
            }
            "explain" => {
                let action_id = self
                    .tasks
                    .goal_runs()
                    .iter()
                    .max_by_key(|run| run.updated_at)
                    .map(|run| run.id.clone());
                if let Some(action_id) = action_id {
                    self.send_daemon_command(DaemonCommand::ExplainAction {
                        action_id,
                        step_index: None,
                    });
                    self.status_line = "Requesting explainability report...".to_string();
                } else {
                    self.status_line = "No goal run available to explain".to_string();
                }
            }
            "diverge" => {
                if let Some(thread_id) = self.chat.active_thread_id().map(str::to_string) {
                    self.input.set_text(&format!(
                        "/diverge-start {thread_id} Compare two implementation approaches for the current task"
                    ));
                    self.focus = FocusArea::Input;
                    self.status_line = "Edit /diverge-start prompt and press Enter".to_string();
                } else {
                    self.status_line = "Open a thread first, then run /diverge".to_string();
                }
            }
            _ => {
                // Unrecognized commands — insert into input so user can add
                // context before sending to the agent (plugin commands, etc.)
                self.input.set_text(&format!("/{command} "));
                self.focus = FocusArea::Chat;
            }
        }
    }

    pub(super) fn submit_prompt(&mut self, prompt: String) {
        if !self.connected {
            self.status_line = "Not connected to daemon".to_string();
            return;
        }
        if self.should_queue_submitted_prompt() {
            self.queue_prompt(prompt);
            return;
        }

        self.cleanup_concierge_on_navigate();

        let content_with_attachments = if self.attachments.is_empty() {
            prompt.clone()
        } else {
            let mut parts: Vec<String> = self
                .attachments
                .drain(..)
                .map(|att| {
                    format!(
                        "<attached_file name=\"{}\">\n{}\n</attached_file>",
                        att.filename, att.content
                    )
                })
                .collect();
            parts.push(prompt.clone());
            parts.join("\n\n")
        };
        let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
        let known_agent_aliases = self.known_agent_directive_aliases();
        if let Some(directive) = input_refs::parse_leading_agent_directive(
            &content_with_attachments,
            &known_agent_aliases,
        ) {
            if matches!(
                directive.agent_alias.to_ascii_lowercase().as_str(),
                "swarozyc" | "radogost" | "domowoj" | "swietowit"
            ) && !self.builtin_persona_configured(&directive.agent_alias)
            {
                self.open_builtin_persona_setup_flow(
                    &directive.agent_alias,
                    content_with_attachments.clone(),
                );
                return;
            }
            let directive_content =
                input_refs::append_referenced_files_footer(&directive.body, &cwd);
            match directive.kind {
                input_refs::LeadingAgentDirectiveKind::InternalDelegate => {
                    self.send_daemon_command(DaemonCommand::InternalDelegate {
                        thread_id: self.chat.active_thread_id().map(String::from),
                        target_agent_id: directive.agent_alias.clone(),
                        content: directive_content,
                        session_id: None,
                    });
                    self.main_pane_view = MainPaneView::Conversation;
                    self.focus = FocusArea::Chat;
                    self.input.set_mode(input::InputMode::Insert);
                    self.status_line = format!("Delegated internally to {}", directive.agent_alias);
                    self.agent_activity = None;
                    self.error_active = false;
                    return;
                }
                input_refs::LeadingAgentDirectiveKind::ParticipantUpsert => {
                    let participant_name = self.participant_display_name(&directive.agent_alias);
                    let Some(thread_id) = self.chat.active_thread_id().map(String::from) else {
                        self.status_line =
                            "Participant commands require an active thread".to_string();
                        self.show_input_notice(
                            format!(
                                "Open a thread before adding {participant_name} as a participant"
                            ),
                            InputNoticeKind::Warning,
                            120,
                            false,
                        );
                        return;
                    };
                    self.send_daemon_command(DaemonCommand::ThreadParticipantCommand {
                        thread_id,
                        target_agent_id: directive.agent_alias.clone(),
                        action: "upsert".to_string(),
                        instruction: Some(directive_content),
                        session_id: None,
                    });
                    self.main_pane_view = MainPaneView::Conversation;
                    self.focus = FocusArea::Chat;
                    self.input.set_mode(input::InputMode::Insert);
                    self.status_line = format!("Participant {} updated", directive.agent_alias);
                    self.show_input_notice(
                        format!("Participant {participant_name} updated for this thread"),
                        InputNoticeKind::Success,
                        120,
                        false,
                    );
                    self.agent_activity = None;
                    self.error_active = false;
                    return;
                }
                input_refs::LeadingAgentDirectiveKind::ParticipantDeactivate => {
                    let participant_name = self.participant_display_name(&directive.agent_alias);
                    let Some(thread_id) = self.chat.active_thread_id().map(String::from) else {
                        self.status_line =
                            "Participant commands require an active thread".to_string();
                        self.show_input_notice(
                            format!(
                                "Open a thread before removing {participant_name} as a participant"
                            ),
                            InputNoticeKind::Warning,
                            120,
                            false,
                        );
                        return;
                    };
                    self.send_daemon_command(DaemonCommand::ThreadParticipantCommand {
                        thread_id,
                        target_agent_id: directive.agent_alias.clone(),
                        action: "deactivate".to_string(),
                        instruction: None,
                        session_id: None,
                    });
                    self.main_pane_view = MainPaneView::Conversation;
                    self.focus = FocusArea::Chat;
                    self.input.set_mode(input::InputMode::Insert);
                    self.status_line = format!("Participant {} stopped", directive.agent_alias);
                    self.show_input_notice(
                        format!("Participant {participant_name} removed from this thread"),
                        InputNoticeKind::Success,
                        120,
                        false,
                    );
                    self.agent_activity = None;
                    self.error_active = false;
                    return;
                }
            }
        }

        let final_content =
            input_refs::append_referenced_files_footer(&content_with_attachments, &cwd);

        let thread_id = self.chat.active_thread_id().map(String::from);
        let target_agent_id = if thread_id.is_none() {
            self.pending_new_thread_target_agent.take()
        } else {
            None
        };
        let local_target_agent_name =
            target_agent_id
                .as_deref()
                .and_then(|agent_id| match agent_id {
                    amux_protocol::AGENT_ID_RAROG => {
                        Some(amux_protocol::AGENT_NAME_RAROG.to_string())
                    }
                    "weles" => Some("Weles".to_string()),
                    _ => None,
                });
        if thread_id.as_deref() == self.cancelled_thread_id.as_deref() {
            self.cancelled_thread_id = None;
        }
        if thread_id.is_none() {
            let local_thread_id = format!("local-{}", self.tick_counter);
            let local_title = if prompt.len() > 40 {
                format!("{}...", &prompt[..40])
            } else {
                prompt.clone()
            };
            self.chat.reduce(chat::ChatAction::ThreadCreated {
                thread_id: local_thread_id.clone(),
                title: local_title.clone(),
            });
            if let Some(agent_name) = local_target_agent_name {
                self.chat
                    .reduce(chat::ChatAction::ThreadDetailReceived(chat::AgentThread {
                        id: local_thread_id,
                        agent_name: Some(agent_name),
                        title: local_title,
                        ..Default::default()
                    }));
            }
        }

        if let Some(thread) = self.chat.active_thread_mut() {
            thread.messages.push(chat::AgentMessage {
                role: chat::MessageRole::User,
                content: final_content.clone(),
                timestamp: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_millis() as u64)
                    .unwrap_or(0),
                ..Default::default()
            });
        }

        self.send_daemon_command(DaemonCommand::SendMessage {
            thread_id,
            content: final_content,
            session_id: None,
            target_agent_id,
        });

        self.main_pane_view = MainPaneView::Conversation;
        self.focus = FocusArea::Chat;
        self.input.set_mode(input::InputMode::Insert);
        self.status_line = "Prompt sent".to_string();
        self.agent_activity = Some("thinking".to_string());
        self.error_active = false;
    }

    pub(super) fn focus_next(&mut self) {
        if matches!(self.main_pane_view, MainPaneView::Collaboration) {
            match self.focus {
                FocusArea::Chat => match self.collaboration.focus() {
                    CollaborationPaneFocus::Navigator => self.collaboration.reduce(
                        CollaborationAction::SetFocus(CollaborationPaneFocus::Detail),
                    ),
                    CollaborationPaneFocus::Detail => self.focus = FocusArea::Input,
                },
                FocusArea::Input => {
                    self.focus = FocusArea::Chat;
                    self.collaboration.reduce(CollaborationAction::SetFocus(
                        CollaborationPaneFocus::Navigator,
                    ));
                }
                FocusArea::Sidebar => self.focus = FocusArea::Input,
            }
            self.input.set_mode(input::InputMode::Insert);
            return;
        }

        self.focus = if self.sidebar_visible() {
            match self.focus {
                FocusArea::Chat => FocusArea::Sidebar,
                FocusArea::Sidebar => FocusArea::Input,
                FocusArea::Input => FocusArea::Chat,
            }
        } else {
            match self.focus {
                FocusArea::Chat | FocusArea::Sidebar => FocusArea::Input,
                FocusArea::Input => FocusArea::Chat,
            }
        };
        self.input.set_mode(input::InputMode::Insert);
    }

    pub(super) fn focus_prev(&mut self) {
        if matches!(self.main_pane_view, MainPaneView::Collaboration) {
            match self.focus {
                FocusArea::Input => {
                    self.focus = FocusArea::Chat;
                    self.collaboration.reduce(CollaborationAction::SetFocus(
                        CollaborationPaneFocus::Detail,
                    ));
                }
                FocusArea::Chat => match self.collaboration.focus() {
                    CollaborationPaneFocus::Detail => self.collaboration.reduce(
                        CollaborationAction::SetFocus(CollaborationPaneFocus::Navigator),
                    ),
                    CollaborationPaneFocus::Navigator => self.focus = FocusArea::Input,
                },
                FocusArea::Sidebar => {
                    self.focus = FocusArea::Chat;
                    self.collaboration.reduce(CollaborationAction::SetFocus(
                        CollaborationPaneFocus::Navigator,
                    ));
                }
            }
            self.input.set_mode(input::InputMode::Insert);
            return;
        }

        self.focus = if self.sidebar_visible() {
            match self.focus {
                FocusArea::Chat => FocusArea::Input,
                FocusArea::Sidebar => FocusArea::Chat,
                FocusArea::Input => FocusArea::Sidebar,
            }
        } else {
            match self.focus {
                FocusArea::Chat | FocusArea::Sidebar => FocusArea::Input,
                FocusArea::Input => FocusArea::Chat,
            }
        };
        self.input.set_mode(input::InputMode::Insert);
    }

    pub(super) fn handle_sidebar_enter(&mut self) {
        let Some(thread_id) = self.chat.active_thread_id().map(str::to_string) else {
            return;
        };

        if self.should_toggle_work_context_from_sidebar(&thread_id) {
            self.set_main_pane_conversation(FocusArea::Sidebar);
            self.status_line = "Closed preview".to_string();
            return;
        }

        match self.sidebar.active_tab() {
            sidebar::SidebarTab::Files => {
                let Some(path) = self
                    .tasks
                    .work_context_for_thread(&thread_id)
                    .and_then(|context| context.entries.get(self.sidebar.selected_item()))
                    .map(|entry| entry.path.clone())
                else {
                    return;
                };
                self.main_pane_view = MainPaneView::WorkContext;
                self.task_view_scroll = 0;
                self.focus = FocusArea::Chat;
                self.tasks.reduce(task::TaskAction::SelectWorkPath {
                    thread_id: thread_id.clone(),
                    path: Some(path.clone()),
                });
                self.request_preview_for_selected_path(&thread_id);
                self.status_line = path;
            }
            sidebar::SidebarTab::Todos => {
                self.main_pane_view = MainPaneView::WorkContext;
                self.task_view_scroll = 0;
                self.focus = FocusArea::Chat;
                self.status_line = "Todo details".to_string();
            }
        }
    }

    pub(super) fn submit_selected_collaboration_vote(&mut self) {
        if let (Some(session), Some(disagreement), Some(position)) = (
            self.collaboration.selected_session(),
            self.collaboration.selected_disagreement(),
            self.collaboration.selected_position(),
        ) {
            if let Some(parent_task_id) = session.parent_task_id.clone() {
                self.send_daemon_command(DaemonCommand::VoteOnCollaborationDisagreement {
                    parent_task_id,
                    disagreement_id: disagreement.id.clone(),
                    task_id: "operator".to_string(),
                    position: position.to_string(),
                    confidence: Some(1.0),
                });
                self.status_line = format!("Casting vote: {position}");
            }
        }
    }

    pub(super) fn copy_message(&mut self, index: usize) {
        let Some(thread) = self.chat.active_thread() else {
            return;
        };
        let Some(message) = thread.messages.get(index) else {
            return;
        };
        let mut text = String::new();
        if let Some(reasoning) = message
            .reasoning
            .as_deref()
            .filter(|value| !value.is_empty())
        {
            text.push_str("Reasoning:\n");
            text.push_str(reasoning);
            if !message.content.is_empty() {
                text.push_str("\n\n-------\n\n");
            }
        }
        if !message.content.is_empty() {
            if !text.is_empty() {
                text.push_str("Content:\n");
            }
            text.push_str(&message.content);
        }
        if text.trim().is_empty() {
            return;
        }
        conversion::copy_to_clipboard(&text);
        self.chat
            .mark_message_copied(index, self.tick_counter.saturating_add(100));
        self.status_line = "Copied to clipboard".to_string();
    }

    pub(super) fn copy_work_context_content(&mut self) {
        let Some(thread_id) = self.chat.active_thread_id().map(str::to_string) else {
            return;
        };

        let text = match self.sidebar.active_tab() {
            sidebar::SidebarTab::Files => {
                let Some(context) = self.tasks.work_context_for_thread(&thread_id) else {
                    return;
                };
                let Some(entry) = context.entries.get(self.sidebar.selected_item()) else {
                    return;
                };
                if let Some(repo_root) = entry.repo_root.as_deref() {
                    self.tasks
                        .diff_for_path(repo_root, &entry.path)
                        .map(str::to_string)
                        .filter(|value| !value.trim().is_empty())
                } else {
                    self.tasks
                        .preview_for_path(&entry.path)
                        .filter(|preview| preview.is_text)
                        .map(|preview| preview.content.clone())
                        .filter(|value| !value.trim().is_empty())
                }
            }
            sidebar::SidebarTab::Todos => self
                .tasks
                .todos_for_thread(&thread_id)
                .get(self.sidebar.selected_item())
                .map(|todo| todo.content.clone())
                .filter(|value| !value.trim().is_empty()),
        };

        if let Some(text) = text {
            conversion::copy_to_clipboard(&text);
            self.status_line = "Copied to clipboard".to_string();
        }
    }

    pub(super) fn resend_message(&mut self, index: usize) {
        let content = self
            .chat
            .active_thread()
            .and_then(|thread| thread.messages.get(index))
            .map(|message| message.content.clone());
        if let Some(content) = content.filter(|value| !value.trim().is_empty()) {
            self.submit_prompt(content);
        }
    }

    pub(super) fn delete_message(&mut self, index: usize) {
        let (thread_id, msg_id) = {
            let Some(thread) = self.chat.active_thread() else {
                return;
            };
            if index >= thread.messages.len() {
                return;
            }
            let mid = thread.messages[index]
                .id
                .clone()
                .unwrap_or_else(|| format!("{}:{}", thread.id, index));
            (thread.id.clone(), mid)
        };

        self.send_daemon_command(DaemonCommand::DeleteMessages {
            thread_id,
            message_ids: vec![msg_id],
        });

        // Remove locally.
        if let Some(thread) = self.chat.active_thread_mut() {
            if index < thread.messages.len() {
                thread.messages.remove(index);
            }
        }
        self.chat.select_message(None);
        self.status_line = format!("Deleted message {}", index + 1);
    }

    pub(super) fn regenerate_from_message(&mut self, index: usize) {
        let prompt = self.chat.active_thread().and_then(|thread| {
            thread
                .messages
                .iter()
                .take(index)
                .rev()
                .find(|message| {
                    message.role == chat::MessageRole::User && !message.content.trim().is_empty()
                })
                .map(|message| message.content.clone())
        });
        if let Some(prompt) = prompt {
            self.submit_prompt(prompt);
        }
    }
}
