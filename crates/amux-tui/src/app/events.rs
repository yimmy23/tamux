use super::*;

impl TuiModel {
    pub fn pump_daemon_events(&mut self) {
        while let Ok(event) = self.daemon_events_rx.try_recv() {
            self.handle_client_event(event);
        }
    }

    pub fn on_tick(&mut self) {
        self.tick_counter = self.tick_counter.saturating_add(1);
        self.chat.clear_expired_copy_feedback(self.tick_counter);
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
                | ClientEvent::ToolResult { thread_id, .. }
                | ClientEvent::RetryStatus { thread_id, .. } => thread_id == cancelled_id,
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
                self.agent_config_loaded = false;
                self.ignore_pending_concierge_welcome = false;
                self.operator_profile.loading = false;
                self.status_line = "Connected to daemon".to_string();
                // Fetch fast data first so UI is responsive immediately.
                // Concierge welcome triggers an LLM call on the daemon which blocks
                // the single-connection response stream — send it LAST so settings,
                // plugins, and session spawn aren't queued behind it.
                self.send_daemon_command(DaemonCommand::Refresh);
                self.send_daemon_command(DaemonCommand::RefreshServices);
                self.send_daemon_command(DaemonCommand::GetProviderAuthStates);
                self.send_daemon_command(DaemonCommand::ListSubAgents);
                self.send_daemon_command(DaemonCommand::GetConciergeConfig);
                self.send_daemon_command(DaemonCommand::PluginList);
                self.send_daemon_command(DaemonCommand::PluginListCommands);
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
                // Request concierge welcome LAST so all other setup commands
                // (settings, plugins, session spawn) are queued ahead of it.
                // The LLM call may block the connection handler, but since all
                // setup is already queued, nothing else is waiting.
                self.send_daemon_command(DaemonCommand::RequestConciergeWelcome);
                self.concierge
                    .reduce(crate::state::ConciergeAction::WelcomeLoading(true));
            }
            ClientEvent::Disconnected => {
                self.connected = false;
                self.agent_config_loaded = false;
                self.last_attention_surface = None;
                self.default_session_id = None;
                self.agent_activity = None;
                self.operator_profile.visible = false;
                self.operator_profile.loading = false;
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
                self.operator_profile.visible = false;
                self.operator_profile.loading = false;
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
            ClientEvent::ApprovalRequired {
                approval_id,
                command,
                risk_level,
                blast_radius,
            } => {
                let task_match = self.tasks.tasks().iter().find(|task| {
                    task.awaiting_approval_id.as_deref() == Some(approval_id.as_str())
                });
                self.approval
                    .reduce(crate::state::ApprovalAction::ApprovalRequired(
                        crate::state::PendingApproval {
                            approval_id: approval_id.clone(),
                            task_id: task_match
                                .map(|task| task.id.clone())
                                .unwrap_or_else(|| approval_id.clone()),
                            task_title: task_match.map(|task| task.title.clone()),
                            command,
                            risk_level: crate::state::RiskLevel::from_str_lossy(&risk_level),
                            blast_radius,
                        },
                    ));
                if self.modal.top() != Some(crate::state::modal::ModalKind::ApprovalOverlay) {
                    self.modal.reduce(crate::state::modal::ModalAction::Push(
                        crate::state::modal::ModalKind::ApprovalOverlay,
                    ));
                }
                self.status_line = "Approval required".to_string();
            }
            ClientEvent::ApprovalResolved {
                approval_id,
                decision,
            } => {
                self.approval.reduce(crate::state::ApprovalAction::Resolve {
                    approval_id: approval_id.clone(),
                    decision,
                });
                if self.approval.current_approval().is_none()
                    && self.modal.top() == Some(crate::state::modal::ModalKind::ApprovalOverlay)
                {
                    self.close_top_modal();
                }
                self.status_line = "Approval resolved".to_string();
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
                if self.chat.active_thread_id() == Some(thread_id.as_str()) {
                    self.clear_chat_drag_selection();
                }
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
                self.apply_config_json(&raw);
                self.agent_config_loaded = true;
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
                if self.modal.top() == Some(crate::state::modal::ModalKind::ModelPicker) {
                    let count = widgets::model_picker::available_models(&self.config).len() + 1;
                    self.modal.set_picker_item_count(count);
                }
            }
            ClientEvent::HeartbeatItems(items) => {
                let items = items
                    .into_iter()
                    .map(conversion::convert_heartbeat)
                    .collect();
                self.tasks
                    .reduce(task::TaskAction::HeartbeatItemsReceived(items));
            }
            ClientEvent::HeartbeatDigest {
                cycle_id,
                actionable,
                digest,
                items,
                checked_at,
                explanation,
            } => {
                let vm_items: Vec<task::HeartbeatDigestItemVm> = items
                    .into_iter()
                    .map(
                        |(priority, check_type, title, suggestion)| task::HeartbeatDigestItemVm {
                            priority,
                            check_type,
                            title,
                            suggestion,
                        },
                    )
                    .collect();
                let item_count = vm_items.len();

                // Extract recent actions BEFORE moving vm_items into the task state
                for item in &vm_items {
                    self.recent_actions.push(super::RecentActionVm {
                        action_type: item.check_type.clone(),
                        summary: item.title.clone(),
                        timestamp: checked_at,
                    });
                }
                // Retain only the 3 most recent actions
                if self.recent_actions.len() > 3 {
                    let start = self.recent_actions.len() - 3;
                    self.recent_actions = self.recent_actions.split_off(start);
                }

                self.tasks.reduce(task::TaskAction::HeartbeatDigestReceived(
                    task::HeartbeatDigestVm {
                        cycle_id,
                        actionable,
                        digest: digest.clone(),
                        items: vm_items,
                        checked_at,
                        explanation,
                    },
                ));
                if actionable && item_count > 0 {
                    self.status_line = format!("\u{2665} Heartbeat: {}", digest);
                }
            }
            ClientEvent::AuditEntry {
                id,
                timestamp,
                action_type,
                summary,
                explanation,
                confidence,
                confidence_band,
                causal_trace_id,
                thread_id,
            } => {
                self.audit
                    .reduce(crate::state::audit::AuditAction::EntryReceived(
                        crate::state::audit::AuditEntryVm {
                            id,
                            timestamp,
                            action_type,
                            summary,
                            explanation,
                            confidence,
                            confidence_band,
                            causal_trace_id,
                            thread_id,
                            dismissed: false,
                        },
                    ));
            }
            ClientEvent::EscalationUpdate {
                thread_id,
                from_level,
                to_level,
                reason,
                attempts,
                audit_id,
            } => {
                self.status_line = format!("Escalating: {}->{} {}", from_level, to_level, reason);
                self.audit
                    .reduce(crate::state::audit::AuditAction::EscalationUpdate(
                        crate::state::audit::EscalationVm {
                            thread_id,
                            from_level,
                            to_level,
                            reason,
                            attempts,
                            audit_id,
                        },
                    ));
            }
            ClientEvent::AnticipatoryItems(items) => {
                self.anticipatory
                    .reduce(crate::state::AnticipatoryAction::Replace(items));
            }
            ClientEvent::GatewayStatus {
                platform,
                status,
                last_error,
                consecutive_failures,
            } => {
                let status_changed = self
                    .gateway_statuses
                    .iter()
                    .find(|g| g.platform == platform)
                    .is_none_or(|existing| {
                        existing.status != status || existing.last_error != last_error
                    });
                let vm = chat::GatewayStatusVm {
                    platform: platform.clone(),
                    status: status.clone(),
                    last_error,
                    consecutive_failures,
                };
                if let Some(existing) = self
                    .gateway_statuses
                    .iter_mut()
                    .find(|g| g.platform == platform)
                {
                    *existing = vm;
                } else {
                    self.gateway_statuses.push(vm);
                }
                if status_changed {
                    self.status_line = format!("\u{1F310} Gateway {}: {}", platform, status);
                }
            }
            ClientEvent::WhatsAppLinkStatus {
                state,
                phone,
                last_error,
            } => {
                tracing::info!(
                    state = %state,
                    phone = phone.as_deref().unwrap_or(""),
                    has_last_error = last_error.is_some(),
                    "tui received whatsapp link status"
                );
                self.modal
                    .set_whatsapp_link_status(&state, phone.clone(), last_error.clone());
                self.status_line = match state.as_str() {
                    "connected" => {
                        format!("WhatsApp linked: {}", phone.as_deref().unwrap_or("device"))
                    }
                    "error" => format!(
                        "WhatsApp link error: {}",
                        last_error.as_deref().unwrap_or("unknown")
                    ),
                    "disconnected" => format!(
                        "WhatsApp link disconnected: {}",
                        last_error.as_deref().unwrap_or("none")
                    ),
                    "qr_ready" | "awaiting_qr" => "WhatsApp link awaiting QR scan".to_string(),
                    "starting" => "WhatsApp link starting".to_string(),
                    _ => "WhatsApp link status updated".to_string(),
                };
            }
            ClientEvent::WhatsAppLinkQr {
                ascii_qr,
                expires_at_ms,
            } => {
                tracing::info!(
                    qr_len = ascii_qr.len(),
                    expires_at_ms,
                    "tui received whatsapp link qr"
                );
                self.modal.set_whatsapp_link_qr(ascii_qr, expires_at_ms);
                if self.modal.top() != Some(crate::state::modal::ModalKind::WhatsAppLink) {
                    self.modal.reduce(crate::state::modal::ModalAction::Push(
                        crate::state::modal::ModalKind::WhatsAppLink,
                    ));
                }
                self.status_line = "WhatsApp QR ready — scan with your phone".to_string();
            }
            ClientEvent::WhatsAppLinked { phone } => {
                tracing::info!(
                    phone = phone.as_deref().unwrap_or(""),
                    "tui received whatsapp linked event"
                );
                self.modal.set_whatsapp_link_connected(phone.clone());
                self.status_line =
                    format!("WhatsApp linked: {}", phone.as_deref().unwrap_or("device"));
            }
            ClientEvent::WhatsAppLinkError { message, .. } => {
                tracing::warn!(message = %message, "tui received whatsapp link error");
                self.modal.set_whatsapp_link_error(message.clone());
                if self.modal.top() != Some(crate::state::modal::ModalKind::WhatsAppLink) {
                    self.modal.reduce(crate::state::modal::ModalAction::Push(
                        crate::state::modal::ModalKind::WhatsAppLink,
                    ));
                }
                self.status_line = format!("WhatsApp link error: {message}");
            }
            ClientEvent::WhatsAppLinkDisconnected { reason } => {
                tracing::info!(
                    reason = reason.as_deref().unwrap_or(""),
                    "tui received whatsapp link disconnected"
                );
                self.modal.set_whatsapp_link_disconnected(reason.clone());
                let display_reason = self
                    .modal
                    .whatsapp_link()
                    .last_error()
                    .map(str::to_string)
                    .or(reason.clone())
                    .unwrap_or_else(|| "none".to_string());
                self.status_line = format!("WhatsApp link disconnected: {}", display_reason);
            }
            ClientEvent::TierChanged { new_tier } => {
                self.tier.on_tier_changed(&new_tier);
                self.status_line = format!("Tier: {}", new_tier);
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
                self.agent_activity = Some(format!("\u{2699}  {}", name));
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
                self.agent_activity = Some(format!("\u{2699}  {} \u{2713}", name));
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
                reasoning,
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
                    reasoning,
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
                if self.ignore_pending_concierge_welcome {
                    self.ignore_pending_concierge_welcome = false;
                    self.concierge
                        .reduce(crate::state::ConciergeAction::WelcomeDismissed);
                    self.chat.reduce(chat::ChatAction::DismissConciergeWelcome);
                    return;
                }
                if self.concierge.is_same_welcome(&content, &actions) {
                    self.concierge
                        .reduce(crate::state::ConciergeAction::WelcomeLoading(false));
                    return;
                }
                self.ignore_pending_concierge_welcome = false;

                // Keep action state for keyboard navigation (left/right arrows)
                self.concierge
                    .reduce(crate::state::ConciergeAction::WelcomeReceived {
                        content: content.clone(),
                        actions: actions.clone(),
                    });

                let concierge_thread_id = "concierge".to_string();
                let existing_thread = self
                    .chat
                    .threads()
                    .iter()
                    .any(|thread| thread.id == concierge_thread_id);
                if !existing_thread {
                    self.chat.reduce(chat::ChatAction::ThreadCreated {
                        thread_id: concierge_thread_id.clone(),
                        title: "Concierge".to_string(),
                    });
                }
                // Clear existing messages in the concierge thread before
                // adding the new welcome — prevents duplicate stacking.
                self.chat.reduce(chat::ChatAction::ClearThread {
                    thread_id: concierge_thread_id.clone(),
                });
                self.chat.reduce(chat::ChatAction::AppendMessage {
                    thread_id: concierge_thread_id.clone(),
                    message: chat::AgentMessage {
                        role: chat::MessageRole::Assistant,
                        content,
                        actions: actions
                            .iter()
                            .map(|action| chat::MessageAction {
                                label: action.label.clone(),
                                action_type: action.action_type.clone(),
                                thread_id: action.thread_id.clone(),
                            })
                            .collect(),
                        is_concierge_welcome: true,
                        ..Default::default()
                    },
                });
                if let Some(thread) = self.chat.active_thread() {
                    if thread.id == concierge_thread_id {
                        let welcome_index = thread.messages.len().saturating_sub(1);
                        self.chat
                            .reduce(chat::ChatAction::PinMessageTop(welcome_index));
                    }
                }

                // Ensure concierge thread is selected and visible
                if self.chat.active_thread_id().is_none()
                    || self.chat.active_thread_id() != Some("concierge")
                {
                    self.chat
                        .reduce(chat::ChatAction::SelectThread("concierge".to_string()));
                    self.send_daemon_command(DaemonCommand::RequestThread("concierge".to_string()));
                }
                if let Some(thread) = self.chat.active_thread() {
                    if thread.id == concierge_thread_id {
                        let welcome_index = thread.messages.len().saturating_sub(1);
                        self.chat
                            .reduce(chat::ChatAction::PinMessageTop(welcome_index));
                    }
                }
                self.main_pane_view = MainPaneView::Conversation;
                self.focus = FocusArea::Chat;
            }
            ClientEvent::ConciergeWelcomeDismissed => {
                self.concierge
                    .reduce(crate::state::ConciergeAction::WelcomeDismissed);
                self.chat.reduce(chat::ChatAction::DismissConciergeWelcome);
                self.send_daemon_command(DaemonCommand::RequestThread("concierge".to_string()));
            }
            ClientEvent::OperatorProfileSessionStarted { session_id, kind } => {
                self.operator_profile.visible = true;
                self.operator_profile.loading = true;
                self.operator_profile.session_id = Some(session_id);
                self.operator_profile.session_kind = Some(kind);
                self.operator_profile.question = None;
                self.operator_profile.warning = None;
                self.set_main_pane_conversation(FocusArea::Input);
                self.status_line = "Operator profile onboarding started".to_string();
                self.send_daemon_command(DaemonCommand::GetOperatorProfileSummary);
            }
            ClientEvent::OperatorProfileQuestion {
                session_id,
                question_id,
                field_key,
                prompt,
                input_kind,
                optional,
            } => {
                self.operator_profile.visible = true;
                self.operator_profile.loading = false;
                self.operator_profile.session_id = Some(session_id.clone());
                self.operator_profile.question = Some(super::OperatorProfileQuestionVm {
                    session_id,
                    question_id,
                    field_key,
                    prompt,
                    input_kind,
                    optional,
                });
                self.operator_profile.warning = None;
                self.set_main_pane_conversation(FocusArea::Input);
                self.input.reduce(input::InputAction::Clear);
                if let Some(options) = self.current_operator_profile_select_options() {
                    if let Some(first) = options.first() {
                        self.input.set_text(first);
                    }
                }
                self.status_line = "Operator profile question ready".to_string();
                self.show_input_notice(
                    "Answer then Enter • Ctrl+S skip • Ctrl+D defer",
                    InputNoticeKind::Success,
                    120,
                    true,
                );
            }
            ClientEvent::OperatorProfileProgress {
                session_id,
                answered,
                remaining,
                completion_ratio,
            } => {
                self.operator_profile.visible = true;
                self.operator_profile.loading = true;
                self.operator_profile.session_id = Some(session_id.clone());
                self.operator_profile.progress = Some(super::OperatorProfileProgressVm {
                    answered,
                    remaining,
                    completion_ratio,
                });
                self.send_daemon_command(DaemonCommand::NextOperatorProfileQuestion { session_id });
                self.status_line = format!(
                    "Operator profile progress: {} answered, {} remaining",
                    answered, remaining
                );
            }
            ClientEvent::OperatorProfileSummary { summary_json } => {
                self.operator_profile.summary_json = Some(summary_json.clone());
                if self.operator_profile.progress.is_none() {
                    if let Ok(summary) = serde_json::from_str::<serde_json::Value>(&summary_json) {
                        let answered = summary
                            .get("field_count")
                            .and_then(|value| value.as_u64())
                            .unwrap_or(0) as u32;
                        self.operator_profile.progress = Some(super::OperatorProfileProgressVm {
                            answered,
                            remaining: self
                                .operator_profile
                                .question
                                .as_ref()
                                .map(|_| 1u32)
                                .unwrap_or(0),
                            completion_ratio: 0.0,
                        });
                    }
                }
            }
            ClientEvent::OperatorProfileSessionCompleted {
                session_id,
                updated_fields,
            } => {
                self.operator_profile.loading = false;
                self.operator_profile.question = None;
                self.operator_profile.warning = None;
                self.operator_profile.visible = false;
                self.operator_profile.session_id = Some(session_id);
                self.operator_profile.progress = Some(super::OperatorProfileProgressVm {
                    answered: updated_fields.len() as u32,
                    remaining: 0,
                    completion_ratio: 1.0,
                });
                self.input.reduce(input::InputAction::Clear);
                self.status_line = "Operator profile onboarding complete".to_string();
                self.show_input_notice(
                    "Operator profile updated",
                    InputNoticeKind::Success,
                    120,
                    true,
                );
                self.send_daemon_command(DaemonCommand::RequestConciergeWelcome);
            }
            // Plugin settings events (Plan 16-03)
            ClientEvent::PluginList(plugins) => {
                self.plugin_settings.plugins = plugins
                    .iter()
                    .map(|p| crate::state::settings::PluginListItem {
                        name: p.name.clone(),
                        version: p.version.clone(),
                        enabled: p.enabled,
                        has_api: p.has_api,
                        has_auth: p.has_auth,
                        settings_count: p.settings_count,
                        description: p.description.clone(),
                        install_source: p.install_source.clone(),
                        auth_status: p.auth_status.clone(),
                    })
                    .collect();
                self.plugin_settings.loading = false;
            }
            ClientEvent::PluginGet {
                plugin: _,
                settings_schema,
            } => {
                if let Some(schema_json) = settings_schema {
                    if let Ok(map) = serde_json::from_str::<
                        serde_json::Map<String, serde_json::Value>,
                    >(&schema_json)
                    {
                        self.plugin_settings.schema_fields = map
                            .into_iter()
                            .map(|(key, val)| crate::state::settings::PluginSchemaField {
                                key,
                                field_type: val
                                    .get("type")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("string")
                                    .to_string(),
                                label: val
                                    .get("label")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("")
                                    .to_string(),
                                required: val
                                    .get("required")
                                    .and_then(|v| v.as_bool())
                                    .unwrap_or(false),
                                secret: val
                                    .get("secret")
                                    .and_then(|v| v.as_bool())
                                    .unwrap_or(false),
                                options: val.get("options").and_then(|v| v.as_array()).map(|arr| {
                                    arr.iter()
                                        .filter_map(|v| v.as_str().map(|s| s.to_string()))
                                        .collect()
                                }),
                                description: val
                                    .get("description")
                                    .and_then(|v| v.as_str())
                                    .map(|s| s.to_string()),
                            })
                            .collect();
                    }
                }
            }
            ClientEvent::PluginSettings {
                plugin_name: _,
                settings,
            } => {
                self.plugin_settings.settings_values = settings;
            }
            ClientEvent::PluginTestConnection {
                plugin_name: _,
                success,
                message,
            } => {
                self.plugin_settings.test_result = Some((success, message));
            }
            ClientEvent::PluginAction { success, message } => {
                if success {
                    // Refresh plugin list on successful action (enable/disable/update)
                    if self.settings.active_tab() == settings::SettingsTab::Plugins {
                        self.send_daemon_command(DaemonCommand::PluginList);
                        // Also refresh settings values so the UI reflects the saved state
                        if let Some(plugin) = self.plugin_settings.selected_plugin() {
                            self.send_daemon_command(DaemonCommand::PluginGetSettings(
                                plugin.name.clone(),
                            ));
                        }
                    }
                } else {
                    self.status_line = format!("Plugin error: {}", message);
                }
            }
            ClientEvent::PluginCommands(commands) => {
                let items: Vec<crate::state::modal::CommandItem> = commands
                    .into_iter()
                    .map(|c| crate::state::modal::CommandItem {
                        command: c.command.trim_start_matches('/').to_string(),
                        description: format!("[{}] {}", c.plugin_name, c.description),
                    })
                    .collect();
                self.modal.set_plugin_commands(items);
            }
            ClientEvent::PluginOAuthUrl { name, url } => {
                if crate::auth::open_external_url(&url).is_ok() {
                    self.status_line = format!(
                        "Opening browser for {} OAuth... Waiting for callback.",
                        name
                    );
                } else {
                    self.status_line = format!(
                        "Could not open browser. Visit: {}",
                        if url.len() > 60 { &url[..60] } else { &url }
                    );
                }
            }
            ClientEvent::PluginOAuthComplete {
                name,
                success,
                error,
            } => {
                if success {
                    self.status_line = format!("{}: OAuth connected successfully.", name);
                    // Refresh plugin list to update auth_status
                    self.send_daemon_command(DaemonCommand::PluginList);
                } else {
                    self.status_line = format!(
                        "{}: OAuth failed -- {}",
                        name,
                        error.as_deref().unwrap_or("unknown error")
                    );
                }
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
            ClientEvent::RetryStatus {
                thread_id,
                phase,
                attempt,
                max_retries,
                delay_ms,
                failure_class,
                message,
            } => {
                if phase == "cleared" {
                    self.chat
                        .reduce(chat::ChatAction::ClearRetryStatus { thread_id });
                    if !self.chat.is_streaming() {
                        self.agent_activity = None;
                    }
                    return;
                }
                self.agent_activity = Some(match phase.as_str() {
                    "waiting" => "retry wait".to_string(),
                    _ => "retrying".to_string(),
                });
                self.chat.reduce(chat::ChatAction::SetRetryStatus {
                    thread_id,
                    phase: if phase == "waiting" {
                        chat::RetryPhase::Waiting
                    } else {
                        chat::RetryPhase::Retrying
                    },
                    attempt,
                    max_retries,
                    delay_ms,
                    failure_class,
                    message,
                    received_at_tick: self.tick_counter,
                });
            }
            ClientEvent::WorkflowNotice {
                kind,
                message,
                details,
            } => {
                let details_ref = details.as_deref();
                if kind == "transport-fallback" {
                    if let Some(details) = details_ref {
                        if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(details) {
                            if let Some(to) = parsed.get("to").and_then(|value| value.as_str()) {
                                self.config.api_transport = to.to_string();
                            }
                        }
                    }
                }
                self.status_line = if let Some(details) = details_ref {
                    format!("{message} ({details})")
                } else {
                    message.clone()
                };
                if kind == "operator-profile-warning" {
                    let warning = if let Some(details) = details_ref {
                        if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(details) {
                            parsed
                                .get("error")
                                .and_then(|value| value.as_str())
                                .map(str::to_string)
                                .unwrap_or_else(|| message.clone())
                        } else {
                            details.to_string()
                        }
                    } else {
                        message.clone()
                    };
                    self.operator_profile.warning = Some(warning);
                    self.operator_profile.loading = false;
                    self.show_input_notice(
                        "operator profile warning (Ctrl+R to retry)",
                        InputNoticeKind::Warning,
                        120,
                        false,
                    );
                }
            }
            ClientEvent::StatusDiagnostics {
                operator_profile_sync_state,
                operator_profile_sync_dirty,
                operator_profile_scheduler_fallback,
            } => {
                if operator_profile_sync_dirty {
                    self.status_line = format!(
                        "Operator profile sync state: {} (retry with Ctrl+R)",
                        operator_profile_sync_state
                    );
                    self.show_input_notice(
                        format!(
                            "operator profile sync={} (Ctrl+R to retry)",
                            operator_profile_sync_state
                        ),
                        InputNoticeKind::Warning,
                        120,
                        false,
                    );
                } else if operator_profile_scheduler_fallback {
                    self.status_line =
                        "Operator profile scheduler fallback active (contextual-only)".to_string();
                    self.show_input_notice(
                        "operator profile scheduler fallback active",
                        InputNoticeKind::Warning,
                        120,
                        false,
                    );
                }
            }
            ClientEvent::AgentExplanation(payload) => {
                let thread_id = self
                    .chat
                    .active_thread_id()
                    .map(str::to_string)
                    .unwrap_or_else(|| "local-explain".to_string());
                let content =
                    serde_json::to_string_pretty(&payload).unwrap_or_else(|_| payload.to_string());
                self.chat.reduce(chat::ChatAction::AppendMessage {
                    thread_id,
                    message: chat::AgentMessage {
                        role: chat::MessageRole::System,
                        content: format!("Explainability\n\n{}", content),
                        ..Default::default()
                    },
                });
                self.status_line = "Explainability result received".to_string();
            }
            ClientEvent::DivergentSessionStarted(payload) => {
                let session_id = payload
                    .get("session_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let thread_id = self
                    .chat
                    .active_thread_id()
                    .map(str::to_string)
                    .unwrap_or_else(|| "local-divergent".to_string());
                self.chat.reduce(chat::ChatAction::AppendMessage {
                    thread_id,
                    message: chat::AgentMessage {
                        role: chat::MessageRole::System,
                        content: if session_id.is_empty() {
                            "Divergent session started".to_string()
                        } else {
                            format!(
                                "Divergent session started: `{}`\nUse `/diverge-get {}` to fetch results.",
                                session_id, session_id
                            )
                        },
                        ..Default::default()
                    },
                });
                self.status_line = "Divergent session started".to_string();
            }
            ClientEvent::DivergentSession(payload) => {
                let thread_id = self
                    .chat
                    .active_thread_id()
                    .map(str::to_string)
                    .unwrap_or_else(|| "local-divergent".to_string());
                let content =
                    serde_json::to_string_pretty(&payload).unwrap_or_else(|_| payload.to_string());
                self.chat.reduce(chat::ChatAction::AppendMessage {
                    thread_id,
                    message: chat::AgentMessage {
                        role: chat::MessageRole::System,
                        content: format!("Divergent session payload\n\n{}", content),
                        ..Default::default()
                    },
                });
                self.status_line = "Divergent session payload received".to_string();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::mpsc::unbounded_channel;

    fn make_model() -> TuiModel {
        let (_event_tx, event_rx) = std::sync::mpsc::channel();
        let (daemon_tx, _daemon_rx) = unbounded_channel();
        TuiModel::new(event_rx, daemon_tx)
    }

    #[test]
    fn whatsapp_qr_event_opens_modal_and_sets_ascii_payload() {
        let mut model = make_model();
        assert!(model.modal.top().is_none());

        model.handle_client_event(ClientEvent::WhatsAppLinkQr {
            ascii_qr: "██\n██".to_string(),
            expires_at_ms: Some(123),
        });

        assert_eq!(
            model.modal.top(),
            Some(crate::state::modal::ModalKind::WhatsAppLink)
        );
        assert_eq!(model.modal.whatsapp_link().ascii_qr(), Some("██\n██"));
        assert_eq!(model.modal.whatsapp_link().expires_at_ms(), Some(123));
    }

    #[test]
    fn whatsapp_status_events_update_modal_state() {
        let mut model = make_model();
        model.handle_client_event(ClientEvent::WhatsAppLinkStatus {
            state: "connected".to_string(),
            phone: Some("+12065550123".to_string()),
            last_error: None,
        });
        assert_eq!(
            model.modal.whatsapp_link().phase(),
            crate::state::modal::WhatsAppLinkPhase::Connected
        );

        model.handle_client_event(ClientEvent::WhatsAppLinkError {
            message: "scan timeout".to_string(),
            recoverable: true,
        });
        assert_eq!(
            model.modal.whatsapp_link().phase(),
            crate::state::modal::WhatsAppLinkPhase::Error
        );
        assert!(model
            .modal
            .whatsapp_link()
            .status_text()
            .contains("scan timeout"));

        model.handle_client_event(ClientEvent::WhatsAppLinkDisconnected {
            reason: Some("socket closed".to_string()),
        });
        assert_eq!(
            model.modal.whatsapp_link().phase(),
            crate::state::modal::WhatsAppLinkPhase::Disconnected
        );
        assert!(model
            .modal
            .whatsapp_link()
            .status_text()
            .contains("socket closed"));
    }

    #[test]
    fn operator_profile_workflow_warning_surfaces_retry_notice() {
        let mut model = make_model();
        model.handle_client_event(ClientEvent::WorkflowNotice {
            kind: "operator-profile-warning".to_string(),
            message: "Operator profile operation failed".to_string(),
            details: Some("{\"retry_action\":\"request_concierge_welcome\"}".to_string()),
        });
        let rendered = model
            .input_notice_style()
            .expect("warning should be visible");
        assert!(
            rendered.0.contains("Ctrl+R"),
            "warning notice should include retry hint"
        );
    }

    #[test]
    fn status_diagnostics_warning_mentions_sync_state() {
        let mut model = make_model();
        model.handle_client_event(ClientEvent::StatusDiagnostics {
            operator_profile_sync_state: "dirty".to_string(),
            operator_profile_sync_dirty: true,
            operator_profile_scheduler_fallback: false,
        });
        assert!(
            model.status_line.contains("sync state: dirty"),
            "status line should expose dirty sync diagnostics"
        );
    }

    #[test]
    fn repeated_gateway_status_does_not_keep_overwriting_status_line() {
        let mut model = make_model();
        model.status_line = "Prompt sent".to_string();

        model.handle_client_event(ClientEvent::GatewayStatus {
            platform: "discord".to_string(),
            status: "disconnected".to_string(),
            last_error: Some("socket closed".to_string()),
            consecutive_failures: 1,
        });
        assert_eq!(model.status_line, "🌐 Gateway discord: disconnected");

        model.status_line = "Prompt sent".to_string();
        model.handle_client_event(ClientEvent::GatewayStatus {
            platform: "discord".to_string(),
            status: "disconnected".to_string(),
            last_error: Some("socket closed".to_string()),
            consecutive_failures: 2,
        });

        assert_eq!(
            model.status_line, "Prompt sent",
            "repeated gateway status should not keep stealing the footer"
        );
    }

    #[test]
    fn operator_profile_question_event_shows_onboarding_notice() {
        let mut model = make_model();
        model.handle_client_event(ClientEvent::OperatorProfileSessionStarted {
            session_id: "sess-1".to_string(),
            kind: "first_run_onboarding".to_string(),
        });
        model.handle_client_event(ClientEvent::OperatorProfileQuestion {
            session_id: "sess-1".to_string(),
            question_id: "name".to_string(),
            field_key: "name".to_string(),
            prompt: "What should I call you?".to_string(),
            input_kind: "text".to_string(),
            optional: false,
        });

        assert!(model.should_show_operator_profile_onboarding());
        assert_eq!(
            model
                .operator_profile
                .question
                .as_ref()
                .map(|q| q.field_key.as_str()),
            Some("name")
        );
    }

    #[test]
    fn operator_profile_progress_requests_next_question() {
        let (_event_tx, event_rx) = std::sync::mpsc::channel();
        let (daemon_tx, mut daemon_rx) = unbounded_channel();
        let mut model = TuiModel::new(event_rx, daemon_tx);
        model.handle_client_event(ClientEvent::OperatorProfileSessionStarted {
            session_id: "sess-1".to_string(),
            kind: "first_run_onboarding".to_string(),
        });

        model.handle_client_event(ClientEvent::OperatorProfileProgress {
            session_id: "sess-1".to_string(),
            answered: 1,
            remaining: 2,
            completion_ratio: 0.33,
        });

        let mut found_next = false;
        while let Ok(command) = daemon_rx.try_recv() {
            if matches!(
                command,
                crate::state::DaemonCommand::NextOperatorProfileQuestion { .. }
            ) {
                found_next = true;
                break;
            }
        }
        assert!(found_next, "progress should trigger next-question command");
    }

    #[test]
    fn models_fetched_updates_picker_count_for_open_model_picker() {
        let mut model = make_model();
        model
            .modal
            .reduce(modal::ModalAction::Push(modal::ModalKind::ModelPicker));
        model.modal.set_picker_item_count(1);

        model.handle_client_event(ClientEvent::ModelsFetched(vec![
            crate::wire::FetchedModel {
                id: "m1".to_string(),
                name: Some("Model One".to_string()),
                context_window: Some(128_000),
            },
            crate::wire::FetchedModel {
                id: "m2".to_string(),
                name: Some("Model Two".to_string()),
                context_window: Some(128_000),
            },
            crate::wire::FetchedModel {
                id: "m3".to_string(),
                name: Some("Model Three".to_string()),
                context_window: Some(128_000),
            },
        ]));

        model.modal.reduce(modal::ModalAction::Navigate(1));
        model.modal.reduce(modal::ModalAction::Navigate(1));

        assert_eq!(model.modal.picker_cursor(), 2);
    }

    #[test]
    fn done_event_persists_final_reasoning_into_chat_message() {
        let mut model = make_model();
        model.chat.reduce(chat::ChatAction::ThreadCreated {
            thread_id: "thread-1".to_string(),
            title: "Thread".to_string(),
        });
        model
            .chat
            .reduce(chat::ChatAction::SelectThread("thread-1".to_string()));
        model.chat.reduce(chat::ChatAction::Delta {
            thread_id: "thread-1".to_string(),
            content: "Answer".to_string(),
        });

        model.handle_client_event(ClientEvent::Done {
            thread_id: "thread-1".to_string(),
            input_tokens: 10,
            output_tokens: 20,
            cost: None,
            provider: Some("github-copilot".to_string()),
            model: Some("gpt-5.4".to_string()),
            tps: None,
            generation_ms: None,
            reasoning: Some("Final reasoning summary".to_string()),
        });

        let thread = model.chat.active_thread().expect("thread should exist");
        let last = thread
            .messages
            .last()
            .expect("assistant message should exist");
        assert_eq!(last.reasoning.as_deref(), Some("Final reasoning summary"));
    }
}
