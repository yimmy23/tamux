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
                self.default_session_id = None;
                self.agent_activity = None;
                self.chat.reduce(chat::ChatAction::ResetStreaming);
                self.clear_pending_stop();
                self.status_line = "Disconnected from daemon".to_string();
            }
            ClientEvent::Reconnecting { delay_secs } => {
                self.connected = false;
                self.default_session_id = None;
                self.agent_activity = None;
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
                self.chat.reduce(chat::ChatAction::ThreadDetailReceived(
                    conversion::convert_thread(thread),
                ));
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
            ClientEvent::Error(message) => {
                if self.assistant_busy() {
                    self.chat.reduce(chat::ChatAction::ForceStopStreaming);
                }
                self.agent_activity = None;
                self.clear_pending_stop();
                self.last_error = Some(message.clone());
                self.error_active = true;
                self.error_tick = self.tick_counter;
                if let Some(thread) = self.chat.active_thread_mut() {
                    thread.messages.push(chat::AgentMessage {
                        role: chat::MessageRole::System,
                        content: format!("Error: {}", message),
                        ..Default::default()
                    });
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
