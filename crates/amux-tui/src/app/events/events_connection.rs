use super::*;

impl TuiModel {
    pub(in crate::app) fn handle_connected_event(&mut self) {
        self.connected = true;
        self.agent_config_loaded = false;
        self.ignore_pending_concierge_welcome = false;
        self.operator_profile.loading = false;
        self.status_line = "Connected to daemon".to_string();
        self.send_daemon_command(DaemonCommand::Refresh);
        self.send_daemon_command(DaemonCommand::RefreshServices);
        self.send_daemon_command(DaemonCommand::GetProviderAuthStates);
        self.send_daemon_command(DaemonCommand::GetOpenAICodexAuthStatus);
        self.send_daemon_command(DaemonCommand::ListSubAgents);
        self.send_daemon_command(DaemonCommand::GetConciergeConfig);
        self.send_daemon_command(DaemonCommand::ListNotifications);
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
    }

    pub(in crate::app) fn handle_disconnected_event(&mut self) {
        self.connected = false;
        self.agent_config_loaded = false;
        self.last_attention_surface = None;
        self.default_session_id = None;
        self.agent_activity = None;
        self.thread_loading_id = None;
        self.operator_profile.visible = false;
        self.operator_profile.loading = false;
        self.concierge
            .reduce(crate::state::ConciergeAction::WelcomeLoading(false));
        self.chat.reduce(chat::ChatAction::ResetStreaming);
        self.clear_pending_stop();
        self.clear_openai_auth_modal_state();
        self.status_line = "Disconnected from daemon".to_string();
    }

    pub(in crate::app) fn handle_reconnecting_event(&mut self, delay_secs: u64) {
        self.connected = false;
        self.last_attention_surface = None;
        self.default_session_id = None;
        self.agent_activity = None;
        self.thread_loading_id = None;
        self.operator_profile.visible = false;
        self.operator_profile.loading = false;
        self.concierge
            .reduce(crate::state::ConciergeAction::WelcomeLoading(false));
        self.chat.reduce(chat::ChatAction::ResetStreaming);
        self.clear_pending_stop();
        self.clear_openai_auth_modal_state();
        self.status_line = format!("Connection lost. Retrying in {}s", delay_secs);
    }

    pub(in crate::app) fn handle_session_spawned_event(&mut self, session_id: String) {
        self.default_session_id = Some(session_id.clone());
        self.status_line = format!("Session: {}", session_id);
    }

    pub(in crate::app) fn handle_approval_required_event(
        &mut self,
        approval_id: String,
        command: String,
        rationale: Option<String>,
        reasons: Vec<String>,
        risk_level: String,
        blast_radius: String,
    ) {
        let task_match = self
            .tasks
            .tasks()
            .iter()
            .find(|task| task.awaiting_approval_id.as_deref() == Some(approval_id.as_str()));
        let thread_id = task_match.and_then(|task| task.thread_id.clone());
        let thread_title = self.thread_title_for_id(thread_id.as_deref());
        let is_current_thread = match thread_id.as_deref() {
            Some(thread_id) => Some(thread_id) == self.chat.active_thread_id(),
            None => true,
        };
        self.approval
            .reduce(crate::state::ApprovalAction::ApprovalRequired(
                crate::state::PendingApproval {
                    approval_id: approval_id.clone(),
                    task_id: task_match
                        .map(|task| task.id.clone())
                        .unwrap_or_else(|| approval_id.clone()),
                    task_title: task_match.map(|task| task.title.clone()),
                    thread_id: thread_id.clone(),
                    thread_title: thread_title.clone(),
                    workspace_id: self.current_workspace_id().map(str::to_string),
                    rationale,
                    reasons,
                    command,
                    risk_level: crate::state::RiskLevel::from_str_lossy(&risk_level),
                    blast_radius,
                    received_at: Self::current_unix_ms().max(0) as u64,
                    seen_at: None,
                },
            ));
        self.approval
            .reduce(crate::state::ApprovalAction::SelectApproval(
                approval_id.clone(),
            ));
        if is_current_thread
            && self.modal.top() != Some(crate::state::modal::ModalKind::ApprovalOverlay)
        {
            self.modal.reduce(crate::state::modal::ModalAction::Push(
                crate::state::modal::ModalKind::ApprovalOverlay,
            ));
        } else if !is_current_thread {
            let thread_label = thread_title
                .clone()
                .filter(|title: &String| !title.trim().is_empty())
                .unwrap_or_else(|| {
                    thread_id
                        .clone()
                        .unwrap_or_else(|| "another thread".to_string())
                });
            self.show_input_notice(
                format!("Approval pending in {thread_label}. Press Ctrl+A."),
                InputNoticeKind::Warning,
                160,
                true,
            );
        }
        self.status_line = if is_current_thread {
            "Approval required in current thread".to_string()
        } else {
            format!(
                "Approval required in {}",
                thread_title.unwrap_or_else(|| "background thread".to_string())
            )
        };
    }

    pub(in crate::app) fn handle_approval_resolved_event(
        &mut self,
        approval_id: String,
        decision: String,
    ) {
        self.approval.reduce(crate::state::ApprovalAction::Resolve {
            approval_id: approval_id.clone(),
            decision,
        });
        if self.next_current_thread_approval_id().is_none()
            && self.modal.top() == Some(crate::state::modal::ModalKind::ApprovalOverlay)
        {
            self.close_top_modal();
        }
        self.status_line = "Approval resolved".to_string();
    }
}
