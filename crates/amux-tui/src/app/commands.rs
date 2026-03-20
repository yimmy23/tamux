use super::*;

impl TuiModel {
    pub(super) fn sidebar_items(&self) -> Vec<SidebarFlatItem> {
        let mut flat_items = Vec::new();

        match self.sidebar.active_tab() {
            sidebar::SidebarTab::Tasks => {
                for run in self.tasks.goal_runs() {
                    flat_items.push(SidebarFlatItem {
                        target: Some(sidebar::SidebarItemTarget::GoalRun {
                            goal_run_id: run.id.clone(),
                            step_id: None,
                        }),
                        title: run.title.clone(),
                    });
                    if self.sidebar.is_expanded(&run.id) {
                        for step in &run.steps {
                            flat_items.push(SidebarFlatItem {
                                target: Some(sidebar::SidebarItemTarget::GoalRun {
                                    goal_run_id: run.id.clone(),
                                    step_id: Some(step.id.clone()),
                                }),
                                title: step.title.clone(),
                            });
                        }
                    }
                }

                for task in self.tasks.tasks() {
                    if task.goal_run_id.is_none() {
                        flat_items.push(SidebarFlatItem {
                            target: Some(sidebar::SidebarItemTarget::Task {
                                task_id: task.id.clone(),
                            }),
                            title: task.title.clone(),
                        });
                    }
                }
            }
            sidebar::SidebarTab::Subagents => {
                for run in self.tasks.goal_runs() {
                    flat_items.push(SidebarFlatItem {
                        target: Some(sidebar::SidebarItemTarget::GoalRun {
                            goal_run_id: run.id.clone(),
                            step_id: None,
                        }),
                        title: run.title.clone(),
                    });
                    if self.sidebar.is_expanded(&run.id) {
                        for task in self
                            .tasks
                            .tasks()
                            .iter()
                            .filter(|task| task.goal_run_id.as_deref() == Some(run.id.as_str()))
                        {
                            flat_items.push(SidebarFlatItem {
                                target: Some(sidebar::SidebarItemTarget::Task {
                                    task_id: task.id.clone(),
                                }),
                                title: task.title.clone(),
                            });
                        }
                    }
                }

                for task in self.tasks.tasks().iter().filter(|task| task.goal_run_id.is_none()) {
                    flat_items.push(SidebarFlatItem {
                        target: Some(sidebar::SidebarItemTarget::Task {
                            task_id: task.id.clone(),
                        }),
                        title: task.title.clone(),
                    });
                }
            }
        }

        flat_items
    }

    pub(super) fn sidebar_item_count(&self) -> usize {
        self.sidebar_items().len()
    }

    pub(super) fn open_sidebar_target(&mut self, target: sidebar::SidebarItemTarget) {
        if let sidebar::SidebarItemTarget::GoalRun { goal_run_id, .. } = &target {
            self.send_daemon_command(DaemonCommand::RequestGoalRunDetail(goal_run_id.clone()));
        }
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

    pub(super) fn execute_command(&mut self, command: &str) {
        tracing::info!("execute_command: {:?}", command);
        match command {
            "provider" => {
                self.modal
                    .reduce(modal::ModalAction::Push(modal::ModalKind::ProviderPicker));
                self.modal.set_picker_item_count(providers::PROVIDERS.len());
            }
            "model" => {
                let models = providers::known_models_for_provider(&self.config.provider);
                if !models.is_empty() {
                    self.config
                        .reduce(config::ConfigAction::ModelsFetched(models));
                }
                self.send_daemon_command(DaemonCommand::FetchModels {
                    provider_id: self.config.provider.clone(),
                    base_url: self.config.base_url.clone(),
                    api_key: self.config.api_key.clone(),
                });
                let count = self.config.fetched_models().len().max(1);
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
            "new" => {
                self.chat.reduce(chat::ChatAction::NewThread);
                self.main_pane_view = MainPaneView::Conversation;
            }
            "settings" => self
                .modal
                .reduce(modal::ModalAction::Push(modal::ModalKind::Settings)),
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
                self.status_line = "Goal runs: type your goal as a message".to_string();
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
        self.focus = match self.focus {
            FocusArea::Chat => FocusArea::Sidebar,
            FocusArea::Sidebar => FocusArea::Input,
            FocusArea::Input => FocusArea::Chat,
        };
        self.input.set_mode(input::InputMode::Insert);
    }

    pub(super) fn focus_prev(&mut self) {
        self.focus = match self.focus {
            FocusArea::Chat => FocusArea::Input,
            FocusArea::Sidebar => FocusArea::Chat,
            FocusArea::Input => FocusArea::Sidebar,
        };
        self.input.set_mode(input::InputMode::Insert);
    }

    pub(super) fn handle_sidebar_enter(&mut self) {
        let selected = self.sidebar.selected_item();
        let flat_items = self.sidebar_items();

        if let Some(item) = flat_items.get(selected) {
            if let Some(target) = item.target.clone() {
                self.open_sidebar_target(target);
                self.status_line = item.title.clone();
            }
        }
    }
}
