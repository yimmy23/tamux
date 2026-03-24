use super::*;

impl TuiModel {
    pub(super) fn target_thread_id(&self, target: &sidebar::SidebarItemTarget) -> Option<String> {
        match target {
            sidebar::SidebarItemTarget::GoalRun { goal_run_id, .. } => self
                .tasks
                .goal_run_by_id(goal_run_id)
                .and_then(|run| run.thread_id.clone()),
            sidebar::SidebarItemTarget::Task { task_id } => {
                self.tasks.task_by_id(task_id).and_then(|task| {
                    task.thread_id.clone().or_else(|| {
                        task.goal_run_id
                            .as_deref()
                            .and_then(|goal_run_id| self.tasks.goal_run_by_id(goal_run_id))
                            .and_then(|run| run.thread_id.clone())
                    })
                })
            }
        }
        .or_else(|| self.chat.active_thread_id().map(str::to_string))
    }

    fn preferred_task_target(&self) -> Option<sidebar::SidebarItemTarget> {
        if let MainPaneView::Task(target) = &self.main_pane_view {
            return Some(target.clone());
        }

        if let Some(active_thread_id) = self.chat.active_thread_id() {
            if let Some(run) = self
                .tasks
                .goal_runs()
                .iter()
                .filter(|run| run.thread_id.as_deref() == Some(active_thread_id))
                .max_by_key(|run| run.updated_at)
            {
                return Some(sidebar::SidebarItemTarget::GoalRun {
                    goal_run_id: run.id.clone(),
                    step_id: None,
                });
            }

            if let Some(task) = self
                .tasks
                .tasks()
                .iter()
                .rev()
                .find(|task| task.thread_id.as_deref() == Some(active_thread_id))
            {
                return Some(sidebar::SidebarItemTarget::Task {
                    task_id: task.id.clone(),
                });
            }
        }

        if let Some(run) = self
            .tasks
            .goal_runs()
            .iter()
            .max_by_key(|run| run.updated_at)
        {
            return Some(sidebar::SidebarItemTarget::GoalRun {
                goal_run_id: run.id.clone(),
                step_id: None,
            });
        }

        self.tasks
            .tasks()
            .last()
            .map(|task| sidebar::SidebarItemTarget::Task {
                task_id: task.id.clone(),
            })
    }

    pub(super) fn open_goal_runner_view(
        &mut self,
        target: Option<sidebar::SidebarItemTarget>,
    ) -> bool {
        let Some(target) = target.or_else(|| self.preferred_task_target()) else {
            self.status_line = "No goal/task activity yet".to_string();
            return false;
        };
        self.open_sidebar_target(target);
        true
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
        let query = self.modal.command_query().to_lowercase();
        let count = self
            .chat
            .threads()
            .iter()
            .filter(|thread| query.is_empty() || thread.title.to_lowercase().contains(&query))
            .count()
            + 1;
        self.modal.set_picker_item_count(count);
    }

    pub(super) fn sync_goal_picker_item_count(&mut self) {
        self.modal
            .set_picker_item_count(self.filtered_goal_runs().len() + 1);
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
        self.send_daemon_command(DaemonCommand::StartGoalRun {
            goal,
            thread_id: None,
            session_id: self.default_session_id.clone(),
        });
        self.status_line = "Starting goal run...".to_string();
    }

    pub(super) fn execute_command(&mut self, command: &str) {
        tracing::info!("execute_command: {:?}", command);
        match command {
            "provider" => {
                self.modal
                    .reduce(modal::ModalAction::Push(modal::ModalKind::ProviderPicker));
                self.modal.set_picker_item_count(providers::PROVIDERS.len());
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
                if !(self.config.provider == "openai"
                    && self.config.auth_source == "chatgpt_subscription")
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
                self.status_line = "Tools config: use /settings -> Tools tab".to_string();
            }
            "effort" => {
                self.modal
                    .reduce(modal::ModalAction::Push(modal::ModalKind::EffortPicker));
                self.modal.set_picker_item_count(5);
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
            "quit" => self.pending_quit = true,
            "prompt" => {
                self.status_line = "System prompt: use /settings -> Agent tab".to_string();
            }
            "goal" => {
                self.open_new_goal_view();
            }
            "attach" => {
                self.status_line =
                    "Usage: /attach <path>  — attach a file to the next message".to_string();
            }
            "help" => {
                self.modal
                    .reduce(modal::ModalAction::Push(modal::ModalKind::Help));
                self.modal.set_picker_item_count(100);
            }
            _ => self.status_line = format!("Unknown command: {}", command),
        }
    }

    pub(super) fn submit_prompt(&mut self, prompt: String) {
        if !self.connected {
            self.status_line = "Not connected to daemon".to_string();
            return;
        }
        if self.assistant_busy() {
            self.queued_prompts.push(prompt);
            self.status_line = format!("QUEUED ({})", self.queued_prompts.len());
            return;
        }

        let final_content = if self.attachments.is_empty() {
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

        let thread_id = self.chat.active_thread_id().map(String::from);
        if thread_id.is_none() {
            self.chat.reduce(chat::ChatAction::ThreadCreated {
                thread_id: format!("local-{}", self.tick_counter),
                title: if prompt.len() > 40 {
                    format!("{}...", &prompt[..40])
                } else {
                    prompt.clone()
                },
            });
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
            session_id: self.default_session_id.clone(),
        });

        self.main_pane_view = MainPaneView::Conversation;
        self.focus = FocusArea::Chat;
        self.input.set_mode(input::InputMode::Insert);
        self.status_line = "Prompt sent".to_string();
        self.agent_activity = Some("thinking".to_string());
        self.error_active = false;
    }

    pub(super) fn focus_next(&mut self) {
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
                self.status_line = "Todo details".to_string();
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
            text.push_str(reasoning);
            if !message.content.is_empty() {
                text.push_str("\n\n");
            }
        }
        text.push_str(&message.content);
        if text.trim().is_empty() {
            return;
        }
        conversion::copy_to_clipboard(&text);
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
        let Some(thread) = self.chat.active_thread_mut() else {
            return;
        };
        if index >= thread.messages.len() {
            return;
        }
        let thread_id = thread.id.clone();
        let msg_id = thread.messages[index]
            .id
            .clone()
            .unwrap_or_else(|| format!("{}:{}", thread_id, index));
        thread.messages.remove(index);

        // Deselect after removal
        self.chat.select_message(None);
        self.status_line = format!("Deleted message {}", index + 1);

        // Persist to daemon
        self.send_daemon_command(DaemonCommand::DeleteMessages {
            thread_id,
            message_ids: vec![msg_id],
        });
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
