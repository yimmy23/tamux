use super::*;
use amux_shared::providers::PROVIDER_ID_OPENAI;
use serde_json::Value;

use amux_protocol::AGENT_NAME_RAROG;

impl TuiModel {
    pub(in crate::app) fn clear_openai_auth_modal_state(&mut self) {
        self.openai_auth_url = None;
        self.openai_auth_status_text = None;
        self.modal
            .reduce(modal::ModalAction::RemoveAll(modal::ModalKind::OpenAIAuth));
    }

    fn refresh_auth_views_after_openai_event(&mut self) {
        self.send_daemon_command(DaemonCommand::GetProviderAuthStates);
    }

    fn apply_openai_codex_auth_status(&mut self, status: &crate::client::OpenAICodexAuthStatusVm) {
        self.config.chatgpt_auth_available = status.available;
        self.config.chatgpt_auth_source = status.source.clone();

        self.modal
            .reduce(modal::ModalAction::RemoveAll(modal::ModalKind::OpenAIAuth));

        self.openai_auth_url = status.auth_url.clone();
        self.openai_auth_status_text =
            status
                .error
                .clone()
                .or_else(|| match status.status.as_deref() {
                    Some("pending") if status.auth_url.is_some() => Some(
                        "Open this URL in your browser to complete ChatGPT authentication."
                            .to_string(),
                    ),
                    Some("completed") if status.available => {
                        Some("ChatGPT subscription auth is available.".to_string())
                    }
                    Some("error") => Some(
                        "OpenAI authentication failed. Please try signing in again.".to_string(),
                    ),
                    _ => None,
                });

        if status.auth_url.is_some() || status.error.is_some() {
            if self.modal.top() != Some(modal::ModalKind::OpenAIAuth) {
                self.modal
                    .reduce(modal::ModalAction::Push(modal::ModalKind::OpenAIAuth));
            }
        } else {
            self.clear_openai_auth_modal_state();
        }
    }

    pub(in crate::app) fn handle_provider_auth_states_event(
        &mut self,
        mut entries: Vec<crate::state::ProviderAuthEntry>,
    ) {
        if self.config.chatgpt_auth_available {
            if let Some(openai_entry) = entries
                .iter_mut()
                .find(|entry| entry.provider_id == PROVIDER_ID_OPENAI)
            {
                openai_entry.authenticated = true;
                openai_entry.auth_source = "chatgpt_subscription".to_string();
            }
        }
        self.auth
            .reduce(crate::state::auth::AuthAction::Received(entries));
    }

    pub(in crate::app) fn handle_provider_validation_event(
        &mut self,
        provider_id: String,
        valid: bool,
        error: Option<String>,
    ) {
        let provider_name = self
            .auth
            .entries
            .iter()
            .find(|entry| entry.provider_id == provider_id)
            .map(|entry| entry.provider_name.clone())
            .unwrap_or_else(|| provider_id.clone());
        self.status_line = if valid {
            format!("{provider_name} connection OK")
        } else {
            format!(
                "{provider_name} test failed: {}",
                error.clone().unwrap_or_else(|| "unknown".to_string())
            )
        };
        self.auth
            .reduce(crate::state::auth::AuthAction::ValidationResult {
                provider_id,
                valid,
                error,
            });
    }

    pub(in crate::app) fn handle_openai_codex_auth_status_event(
        &mut self,
        status: crate::client::OpenAICodexAuthStatusVm,
    ) {
        self.apply_openai_codex_auth_status(&status);
        self.refresh_auth_views_after_openai_event();
        if let Some(error) = status.error {
            self.status_line = error;
        } else if status.available {
            self.status_line = "ChatGPT subscription auth available".to_string();
        } else if matches!(status.status.as_deref(), Some("pending")) {
            self.status_line = "ChatGPT subscription login pending".to_string();
        }
    }

    pub(in crate::app) fn handle_openai_codex_auth_login_result_event(
        &mut self,
        status: crate::client::OpenAICodexAuthStatusVm,
    ) {
        self.apply_openai_codex_auth_status(&status);
        self.refresh_auth_views_after_openai_event();

        if status.auth_url.is_some() {
            self.status_line = "ChatGPT subscription login started".to_string();
        } else if let Some(error) = status.error {
            self.status_line = error;
        } else if status.available {
            self.status_line = "ChatGPT subscription auth available".to_string();
        }
    }

    pub(in crate::app) fn handle_openai_codex_auth_logout_result_event(
        &mut self,
        ok: bool,
        error: Option<String>,
    ) {
        if ok {
            self.config.chatgpt_auth_available = false;
            self.config.chatgpt_auth_source = None;
            self.clear_openai_auth_modal_state();
            self.refresh_auth_views_after_openai_event();
            self.status_line = "ChatGPT subscription auth cleared".to_string();
        } else {
            self.openai_auth_url = None;
            self.openai_auth_status_text = error.clone();
            if self.modal.top() != Some(modal::ModalKind::OpenAIAuth) {
                self.modal
                    .reduce(modal::ModalAction::Push(modal::ModalKind::OpenAIAuth));
            }
            self.refresh_auth_views_after_openai_event();
            self.status_line = error.unwrap_or_else(|| {
                "OpenAI authentication failed. Please try signing in again.".to_string()
            });
        }
    }

    pub(in crate::app) fn handle_subagent_list_event(
        &mut self,
        entries: Vec<crate::state::SubAgentEntry>,
    ) {
        self.subagents
            .reduce(crate::state::subagents::SubAgentsAction::ListReceived(
                entries,
            ));
    }

    pub(in crate::app) fn handle_subagent_updated_event(
        &mut self,
        entry: crate::state::SubAgentEntry,
    ) {
        let before_profile = self.current_conversation_agent_profile();
        self.subagents
            .reduce(crate::state::subagents::SubAgentsAction::Updated(entry));
        self.invalidate_active_header_runtime_profile_if_profile_changed(&before_profile);
    }

    pub(in crate::app) fn handle_subagent_removed_event(&mut self, sub_agent_id: String) {
        self.subagents
            .reduce(crate::state::subagents::SubAgentsAction::Removed(
                sub_agent_id,
            ));
    }

    pub(in crate::app) fn handle_concierge_config_event(&mut self, raw: Value) {
        let before_profile = self.current_conversation_agent_profile();
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
                reasoning_effort: raw
                    .get("reasoning_effort")
                    .and_then(|value| value.as_str())
                    .map(str::to_string),
                auto_cleanup_on_navigate: raw
                    .get("auto_cleanup_on_navigate")
                    .and_then(|value| value.as_bool())
                    .unwrap_or(true),
            });
        self.invalidate_active_header_runtime_profile_if_profile_changed(&before_profile);
    }

    pub(in crate::app) fn handle_concierge_welcome_event(
        &mut self,
        content: String,
        actions: Vec<crate::state::ConciergeActionVm>,
    ) {
        let welcome_complete = !actions.is_empty();
        if self.ignore_pending_concierge_welcome {
            self.concierge
                .reduce(crate::state::ConciergeAction::WelcomeDismissed);
            self.chat.reduce(chat::ChatAction::DismissConciergeWelcome);
            return;
        }
        if self.concierge.is_same_welcome(&content, &actions) {
            self.concierge
                .reduce(crate::state::ConciergeAction::WelcomeLoading(
                    !welcome_complete,
                ));
            return;
        }
        self.ignore_pending_concierge_welcome = false;

        self.concierge
            .reduce(crate::state::ConciergeAction::WelcomeReceived {
                content: content.clone(),
                actions: actions.clone(),
            });
        self.concierge
            .reduce(crate::state::ConciergeAction::WelcomeLoading(
                !welcome_complete,
            ));

        let concierge_thread_id = "concierge".to_string();
        let existing_thread = self
            .chat
            .threads()
            .iter()
            .any(|thread| thread.id == concierge_thread_id);
        if !existing_thread {
            self.chat.reduce(chat::ChatAction::ThreadCreated {
                thread_id: concierge_thread_id.clone(),
                title: AGENT_NAME_RAROG.to_string(),
            });
        }
        self.chat.reduce(chat::ChatAction::ClearThread {
            thread_id: concierge_thread_id.clone(),
        });
        self.reduce_chat_for_thread(
            Some(concierge_thread_id.as_str()),
            chat::ChatAction::AppendMessage {
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
            },
        );
        if let Some(thread) = self.chat.active_thread() {
            if thread.id == concierge_thread_id {
                let welcome_index = thread.messages.len().saturating_sub(1);
                self.chat
                    .reduce(chat::ChatAction::PinMessageTop(welcome_index));
            }
        }

        if self.chat.active_thread_id().is_none()
            || self.chat.active_thread_id() != Some("concierge")
        {
            self.chat
                .reduce(chat::ChatAction::SelectThread("concierge".to_string()));
            self.send_daemon_command(DaemonCommand::RequestThread {
                thread_id: "concierge".to_string(),
                message_limit: Some(50),
                message_offset: Some(0),
            });
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

    pub(in crate::app) fn handle_concierge_welcome_dismissed_event(&mut self) {
        self.concierge
            .reduce(crate::state::ConciergeAction::WelcomeDismissed);
        self.chat.reduce(chat::ChatAction::DismissConciergeWelcome);
        self.send_daemon_command(DaemonCommand::RequestThread {
            thread_id: "concierge".to_string(),
            message_limit: Some(50),
            message_offset: Some(0),
        });
    }

    pub(in crate::app) fn handle_plugin_list_event(
        &mut self,
        plugins: Vec<amux_protocol::PluginInfo>,
    ) {
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

    pub(in crate::app) fn handle_plugin_get_event(&mut self, settings_schema: Option<String>) {
        if let Some(schema_json) = settings_schema {
            if let Ok(map) =
                serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(&schema_json)
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
                        secret: val.get("secret").and_then(|v| v.as_bool()).unwrap_or(false),
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

    pub(in crate::app) fn handle_plugin_settings_event(
        &mut self,
        settings: Vec<(String, String, bool)>,
    ) {
        self.plugin_settings.settings_values = settings;
    }

    pub(in crate::app) fn handle_plugin_test_connection_event(
        &mut self,
        success: bool,
        message: String,
    ) {
        self.plugin_settings.test_result = Some((success, message));
    }

    pub(in crate::app) fn handle_plugin_action_event(&mut self, success: bool, message: String) {
        if success {
            if self.settings.active_tab() == settings::SettingsTab::Plugins {
                self.send_daemon_command(DaemonCommand::PluginList);
                if let Some(plugin) = self.plugin_settings.selected_plugin() {
                    self.send_daemon_command(DaemonCommand::PluginGetSettings(plugin.name.clone()));
                }
            }
        } else {
            self.status_line = format!("Plugin error: {}", message);
        }
    }

    pub(in crate::app) fn handle_plugin_commands_event(
        &mut self,
        commands: Vec<amux_protocol::PluginCommandInfo>,
    ) {
        let items: Vec<crate::state::modal::CommandItem> = commands
            .into_iter()
            .map(|c| crate::state::modal::CommandItem {
                command: c.command.trim_start_matches('/').to_string(),
                description: format!("[{}] {}", c.plugin_name, c.description),
            })
            .collect();
        self.modal.set_plugin_commands(items);
    }

    pub(in crate::app) fn handle_plugin_oauth_url_event(&mut self, name: String, url: String) {
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

    pub(in crate::app) fn handle_plugin_oauth_complete_event(
        &mut self,
        name: String,
        success: bool,
        error: Option<String>,
    ) {
        if success {
            self.status_line = format!("{}: OAuth connected successfully.", name);
            self.send_daemon_command(DaemonCommand::PluginList);
        } else {
            self.status_line = format!(
                "{}: OAuth failed -- {}",
                name,
                error.as_deref().unwrap_or("unknown error")
            );
        }
    }
}
