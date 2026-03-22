use super::*;

impl TuiModel {
    pub fn pump_daemon_events(&mut self) {
        while let Ok(event) = self.daemon_events_rx.try_recv() {
            self.handle_client_event(event);
        }
    }

    pub fn on_tick(&mut self) {
        self.tick_counter = self.tick_counter.saturating_add(1);
        if self.pending_stop && !self.pending_stop_active() {
            self.pending_stop = false;
        }
        if self
            .input_notice
            .as_ref()
            .is_some_and(|notice| self.tick_counter >= notice.expires_at_tick)
        {
            self.input_notice = None;
        }
        self.publish_attention_surface_if_changed();
    }

    fn handle_client_event(&mut self, event: ClientEvent) {
        if let Some(ref cancelled_id) = self.cancelled_thread_id.clone() {
            let skip = match &event {
                ClientEvent::Delta { thread_id, .. }
                | ClientEvent::Reasoning { thread_id, .. }
                | ClientEvent::ToolCall { thread_id, .. }
                | ClientEvent::ToolResult { thread_id, .. } => thread_id == cancelled_id,
                ClientEvent::Done { thread_id, .. } => {
                    if thread_id == cancelled_id {
                        self.cancelled_thread_id = None;
                        true
                    } else {
                        false
                    }
                }
                _ => false,
            };
            if skip {
                return;
            }
        }

        match event {
            ClientEvent::Connected => {
                self.connected = true;
                self.status_line = "Connected to daemon".to_string();
                self.sync_config_to_daemon();
                self.send_daemon_command(DaemonCommand::Refresh);
                self.send_daemon_command(DaemonCommand::RefreshServices);
                self.send_daemon_command(DaemonCommand::GetProviderAuthStates);
                self.send_daemon_command(DaemonCommand::ListSubAgents);
                self.send_daemon_command(DaemonCommand::GetConciergeConfig);
                self.concierge
                    .reduce(crate::state::ConciergeAction::WelcomeLoading(true));
                let cwd = std::env::current_dir()
                    .ok()
                    .map(|p| p.to_string_lossy().to_string());
                let shell = std::env::var("SHELL").ok();
                self.send_daemon_command(DaemonCommand::SpawnSession {
                    shell,
                    cwd,
                    cols: self.width.max(80),
                    rows: self.height.max(24),
                });
            }
            ClientEvent::Disconnected => {
                self.connected = false;
                self.last_attention_surface = None;
                self.default_session_id = None;
                self.agent_activity = None;
                self.concierge
                    .reduce(crate::state::ConciergeAction::WelcomeLoading(false));
                self.chat.reduce(chat::ChatAction::ResetStreaming);
                self.clear_pending_stop();
                self.status_line = "Disconnected from daemon".to_string();
            }
            ClientEvent::Reconnecting { delay_secs } => {
                self.connected = false;
                self.last_attention_surface = None;
                self.default_session_id = None;
                self.agent_activity = None;
                self.concierge
                    .reduce(crate::state::ConciergeAction::WelcomeLoading(false));
                self.chat.reduce(chat::ChatAction::ResetStreaming);
                self.clear_pending_stop();
                self.status_line = format!("Connection lost. Retrying in {}s", delay_secs);
            }
            ClientEvent::SessionSpawned { session_id } => {
                self.default_session_id = Some(session_id.clone());
                self.status_line = format!("Session: {}", session_id);
            }
            ClientEvent::ThreadList(threads) => {
                let threads = threads
                    .into_iter()
                    .map(conversion::convert_thread)
                    .collect();
                self.chat
                    .reduce(chat::ChatAction::ThreadListReceived(threads));
            }
            ClientEvent::ThreadDetail(Some(thread)) => {
                let thread_id = thread.id.clone();
                self.chat.reduce(chat::ChatAction::ThreadDetailReceived(
                    conversion::convert_thread(thread),
                ));
                self.send_daemon_command(DaemonCommand::RequestThreadTodos(thread_id.clone()));
                self.send_daemon_command(DaemonCommand::RequestThreadWorkContext(thread_id));
            }
            ClientEvent::ThreadDetail(None) => {}
            ClientEvent::ThreadCreated { thread_id, title } => {
                self.chat
                    .reduce(chat::ChatAction::ThreadCreated { thread_id, title });
            }
            ClientEvent::TaskList(tasks) => {
                let tasks = tasks.into_iter().map(conversion::convert_task).collect();
                self.tasks.reduce(task::TaskAction::TaskListReceived(tasks));
            }
            ClientEvent::TaskUpdate(task) => {
                self.tasks
                    .reduce(task::TaskAction::TaskUpdate(conversion::convert_task(task)));
            }
            ClientEvent::GoalRunList(runs) => {
                let runs = runs.into_iter().map(conversion::convert_goal_run).collect();
                self.tasks
                    .reduce(task::TaskAction::GoalRunListReceived(runs));
            }
            ClientEvent::GoalRunStarted(run) => {
                let run = conversion::convert_goal_run(run);
                let target = sidebar::SidebarItemTarget::GoalRun {
                    goal_run_id: run.id.clone(),
                    step_id: None,
                };
                self.tasks.reduce(task::TaskAction::GoalRunUpdate(run));
                self.open_sidebar_target(target);
                self.status_line = "Goal run started".to_string();
            }
            ClientEvent::GoalRunDetail(Some(run)) => {
                self.tasks.reduce(task::TaskAction::GoalRunDetailReceived(
                    conversion::convert_goal_run(run),
                ));
            }
            ClientEvent::GoalRunDetail(None) => {}
            ClientEvent::GoalRunUpdate(run) => {
                self.tasks.reduce(task::TaskAction::GoalRunUpdate(
                    conversion::convert_goal_run(run),
                ));
            }
            ClientEvent::GoalRunCheckpoints {
                goal_run_id,
                checkpoints,
            } => {
                self.tasks
                    .reduce(task::TaskAction::GoalRunCheckpointsReceived {
                        goal_run_id,
                        checkpoints: checkpoints
                            .into_iter()
                            .map(conversion::convert_checkpoint_summary)
                            .collect(),
                    });
            }
            ClientEvent::ThreadTodos { thread_id, items } => {
                self.tasks.reduce(task::TaskAction::ThreadTodosReceived {
                    thread_id,
                    items: items.into_iter().map(conversion::convert_todo).collect(),
                });
            }
            ClientEvent::WorkContext(context) => {
                self.tasks.reduce(task::TaskAction::WorkContextReceived(
                    conversion::convert_work_context(context),
                ));
                self.ensure_task_view_preview();
            }
            ClientEvent::GitDiff {
                repo_path,
                file_path,
                diff,
            } => {
                self.tasks.reduce(task::TaskAction::GitDiffReceived {
                    repo_path,
                    file_path,
                    diff,
                });
            }
            ClientEvent::FilePreview {
                path,
                content,
                truncated,
                is_text,
            } => {
                self.tasks
                    .reduce(task::TaskAction::FilePreviewReceived(task::FilePreview {
                        path,
                        content,
                        truncated,
                        is_text,
                    }));
            }
            ClientEvent::AgentConfig(cfg) => {
                self.config.reduce(config::ConfigAction::ConfigReceived(
                    config::AgentConfigSnapshot {
                        provider: cfg.provider,
                        base_url: cfg.base_url,
                        model: cfg.model,
                        custom_model_name: String::new(),
                        api_key: cfg.api_key,
                        assistant_id: cfg.assistant_id,
                        auth_source: cfg.auth_source,
                        api_transport: cfg.api_transport,
                        reasoning_effort: cfg.reasoning_effort,
                        context_window_tokens: cfg.context_window_tokens,
                    },
                ));
            }
            ClientEvent::AgentConfigRaw(raw) => {
                self.config
                    .reduce(config::ConfigAction::ConfigRawReceived(raw));
            }
            ClientEvent::ModelsFetched(models) => {
                let models = models
                    .into_iter()
                    .map(|model| config::FetchedModel {
                        id: model.id,
                        name: model.name,
                        context_window: model.context_window,
                    })
                    .collect();
                self.config
                    .reduce(config::ConfigAction::ModelsFetched(models));
            }
            ClientEvent::HeartbeatItems(items) => {
                let items = items
                    .into_iter()
                    .map(conversion::convert_heartbeat)
                    .collect();
                self.tasks
                    .reduce(task::TaskAction::HeartbeatItemsReceived(items));
            }
            ClientEvent::AnticipatoryItems(items) => {
                self.anticipatory
                    .reduce(crate::state::AnticipatoryAction::Replace(items));
            }
            ClientEvent::Delta { thread_id, content } => {
                self.agent_activity = Some("writing".to_string());
                self.chat
                    .reduce(chat::ChatAction::Delta { thread_id, content });
            }
            ClientEvent::Reasoning { thread_id, content } => {
                self.agent_activity = Some("reasoning".to_string());
                self.chat
                    .reduce(chat::ChatAction::Reasoning { thread_id, content });
            }
            ClientEvent::ToolCall {
                thread_id,
                call_id,
                name,
                arguments,
            } => {
                self.agent_activity = Some(format!("\u{2699} {}", name));
                self.chat.reduce(chat::ChatAction::ToolCall {
                    thread_id,
                    call_id,
                    name,
                    args: arguments,
                });
            }
            ClientEvent::ToolResult {
                thread_id,
                call_id,
                name,
                content,
                is_error,
            } => {
                self.agent_activity = Some(format!("\u{2699} {} \u{2713}", name));
                self.chat.reduce(chat::ChatAction::ToolResult {
                    thread_id,
                    call_id,
                    name,
                    content,
                    is_error,
                });
            }
            ClientEvent::Done {
                thread_id,
                input_tokens,
                output_tokens,
                cost,
                provider,
                model,
                tps,
                generation_ms,
            } => {
                self.agent_activity = None;
                self.pending_stop = false;
                if self
                    .input_notice
                    .as_ref()
                    .is_some_and(|notice| notice.kind == InputNoticeKind::Warning)
                {
                    self.input_notice = None;
                }
                self.chat.reduce(chat::ChatAction::TurnDone {
                    thread_id,
                    input_tokens,
                    output_tokens,
                    cost,
                    provider,
                    model,
                    tps,
                    generation_ms,
                });

                if !self.queued_prompts.is_empty() {
                    let next_prompt = self.queued_prompts.remove(0);
                    self.submit_prompt(next_prompt);
                }
            }
            ClientEvent::ProviderAuthStates(entries) => {
                self.auth
                    .reduce(crate::state::auth::AuthAction::Received(entries));
            }
            ClientEvent::ProviderValidation {
                provider_id,
                valid,
                error,
            } => {
                self.auth
                    .reduce(crate::state::auth::AuthAction::ValidationResult {
                        provider_id,
                        valid,
                        error,
                    });
            }
            ClientEvent::SubAgentList(entries) => {
                self.subagents
                    .reduce(crate::state::subagents::SubAgentsAction::ListReceived(
                        entries,
                    ));
            }
            ClientEvent::SubAgentUpdated(entry) => {
                self.subagents
                    .reduce(crate::state::subagents::SubAgentsAction::Updated(entry));
            }
            ClientEvent::SubAgentRemoved { sub_agent_id } => {
                self.subagents
                    .reduce(crate::state::subagents::SubAgentsAction::Removed(
                        sub_agent_id,
                    ));
            }
            ClientEvent::ConciergeConfig(raw) => {
                let detail_level = raw
                    .get("detail_level")
                    .and_then(|value| value.as_str())
                    .unwrap_or("proactive_triage")
                    .to_string();
                self.concierge
                    .reduce(crate::state::ConciergeAction::ConfigReceived {
                        enabled: raw
                            .get("enabled")
                            .and_then(|value| value.as_bool())
                            .unwrap_or(true),
                        detail_level,
                        provider: raw
                            .get("provider")
                            .and_then(|value| value.as_str())
                            .map(str::to_string),
                        model: raw
                            .get("model")
                            .and_then(|value| value.as_str())
                            .map(str::to_string),
                        auto_cleanup_on_navigate: raw
                            .get("auto_cleanup_on_navigate")
                            .and_then(|value| value.as_bool())
                            .unwrap_or(true),
                    });
            }
            ClientEvent::ConciergeWelcome { content, actions } => {
                self.concierge
                    .reduce(crate::state::ConciergeAction::WelcomeReceived { content, actions });
                if self.chat.active_thread_id().is_none() {
                    self.chat
                        .reduce(chat::ChatAction::SelectThread("concierge".to_string()));
                    self.send_daemon_command(DaemonCommand::RequestThread("concierge".to_string()));
                    self.main_pane_view = MainPaneView::Conversation;
                    self.focus = FocusArea::Chat;
                } else if self.chat.active_thread_id() == Some("concierge") {
                    self.focus = FocusArea::Chat;
                }
            }
            ClientEvent::ConciergeWelcomeDismissed => {
                self.concierge
                    .reduce(crate::state::ConciergeAction::WelcomeDismissed);
            }
            ClientEvent::Error(message) => {
                let busy = self.assistant_busy();
                if busy {
                    self.chat.reduce(chat::ChatAction::ForceStopStreaming);
                }
                self.agent_activity = None;
                self.clear_pending_stop();
                self.concierge
                    .reduce(crate::state::ConciergeAction::WelcomeLoading(false));
                self.last_error = Some(message.clone());
                self.error_active = true;
                self.error_tick = self.tick_counter;
                if busy && self.modal.top().is_none() {
                    if let Some(thread) = self.chat.active_thread_mut() {
                        thread.messages.push(chat::AgentMessage {
                            role: chat::MessageRole::System,
                            content: format!("Error: {}", message),
                            ..Default::default()
                        });
                    }
                } else {
                    self.status_line = "Error recorded. Press Ctrl+E for details".to_string();
                }
            }
            ClientEvent::WorkflowNotice {
                kind,
                message,
                details,
            } => {
                if kind == "transport-fallback" {
                    if let Some(details) = details.as_deref() {
                        if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(details) {
                            if let Some(to) = parsed.get("to").and_then(|value| value.as_str()) {
                                self.config.api_transport = to.to_string();
                                self.save_settings();
                            }
                        }
                    }
                }
                self.status_line = if let Some(details) = details {
                    format!("{message} ({details})")
                } else {
                    message
                };
            }
        }
    }
}
