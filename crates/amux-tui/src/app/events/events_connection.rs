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
        risk_level: String,
        blast_radius: String,
    ) {
        let task_match = self
            .tasks
            .tasks()
            .iter()
            .find(|task| task.awaiting_approval_id.as_deref() == Some(approval_id.as_str()));
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

    pub(in crate::app) fn handle_approval_resolved_event(
        &mut self,
        approval_id: String,
        decision: String,
    ) {
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
}
