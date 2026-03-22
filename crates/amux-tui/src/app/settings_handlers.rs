use super::*;

impl TuiModel {
    fn subagent_known_models_for(&self, provider_id: &str) -> Vec<config::FetchedModel> {
        providers::known_models_for_provider_auth(provider_id, "api_key")
    }

    fn open_subagent_editor_new(&mut self) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let default_provider = self.config.provider.clone();
        let default_model = if self.config.model.trim().is_empty() {
            providers::default_model_for_provider_auth(&default_provider, "api_key")
        } else {
            self.config.model.clone()
        };
        let mut editor = crate::state::subagents::SubAgentEditorState::new(
            None,
            now,
            default_provider,
            default_model,
        );
        editor.name = format!("Sub-Agent {}", self.subagents.entries.len() + 1);
        self.subagents.editor = Some(editor);
        self.subagents.actions_focused = false;
    }

    fn open_subagent_editor_existing(&mut self) {
        let Some(entry) = self.subagents.entries.get(self.subagents.selected).cloned() else {
            return;
        };
        let raw = entry
            .raw_json
            .clone()
            .unwrap_or_else(|| serde_json::json!({}));
        let created_at = raw
            .get("created_at")
            .and_then(|value| value.as_u64())
            .unwrap_or(0);
        let mut editor = crate::state::subagents::SubAgentEditorState::new(
            Some(entry.id.clone()),
            created_at,
            entry.provider.clone(),
            entry.model.clone(),
        );
        editor.name = entry.name;
        editor.role = entry.role.unwrap_or_default();
        editor.system_prompt = raw
            .get("system_prompt")
            .and_then(|value| value.as_str())
            .unwrap_or("")
            .to_string();
        editor.enabled = entry.enabled;
        editor.previous_role_preset = editor
            .role_preset_index()
            .and_then(|index| crate::state::subagents::SUBAGENT_ROLE_PRESETS.get(index))
            .map(|preset| preset.id.to_string());
        self.subagents.editor = Some(editor);
        self.subagents.actions_focused = false;
    }

    fn close_subagent_editor(&mut self) {
        self.subagents.editor = None;
        self.settings.reduce(SettingsAction::CancelEdit);
    }

    fn commit_subagent_editor(&mut self) {
        let Some(editor) = self.subagents.editor.clone() else {
            return;
        };
        if editor.name.trim().is_empty() {
            self.status_line = "Sub-agent name is required".to_string();
            return;
        }
        if editor.provider.trim().is_empty() {
            self.status_line = "Sub-agent provider is required".to_string();
            return;
        }
        if editor.model.trim().is_empty() {
            self.status_line = "Sub-agent model is required".to_string();
            return;
        }

        let id = editor.id.unwrap_or_else(|| {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0);
            format!("subagent-{now}")
        });
        let raw = serde_json::json!({
            "id": id,
            "name": editor.name.trim(),
            "provider": editor.provider,
            "model": editor.model,
            "role": if editor.role.trim().is_empty() { serde_json::Value::Null } else { serde_json::Value::String(editor.role.trim().to_string()) },
            "system_prompt": if editor.system_prompt.trim().is_empty() { serde_json::Value::Null } else { serde_json::Value::String(editor.system_prompt.trim().to_string()) },
            "enabled": editor.enabled,
            "created_at": editor.created_at,
        });
        self.send_daemon_command(DaemonCommand::SetSubAgent(raw.to_string()));

        let optimistic = crate::state::SubAgentEntry {
            id: raw
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string(),
            name: raw
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string(),
            provider: raw
                .get("provider")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string(),
            model: raw
                .get("model")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string(),
            role: raw
                .get("role")
                .and_then(|v| v.as_str())
                .map(ToString::to_string),
            enabled: raw.get("enabled").and_then(|v| v.as_bool()).unwrap_or(true),
            raw_json: Some(raw),
        };
        if self
            .subagents
            .entries
            .iter()
            .any(|entry| entry.id == optimistic.id)
        {
            self.subagents
                .reduce(crate::state::subagents::SubAgentsAction::Updated(
                    optimistic,
                ));
            self.status_line = "Updated sub-agent".to_string();
        } else {
            self.subagents
                .reduce(crate::state::subagents::SubAgentsAction::Added(optimistic));
            self.status_line = "Added sub-agent".to_string();
        }
        self.subagents.editor = None;
        self.settings.reduce(SettingsAction::CancelEdit);
    }

    fn cycle_subagent_role(&mut self, delta: i32) {
        let Some(editor) = self.subagents.editor.as_mut() else {
            return;
        };
        let len = crate::state::subagents::SUBAGENT_ROLE_PRESETS.len();
        if len == 0 {
            return;
        }
        let current = editor.role_preset_index().unwrap_or(0);
        let next = if delta >= 0 {
            (current + delta as usize) % len
        } else {
            current.saturating_sub((-delta) as usize)
        };
        editor.apply_role_preset_by_index(next);
    }

    fn open_subagent_provider_picker(&mut self) {
        self.settings_picker_target = Some(SettingsPickerTarget::SubAgentProvider);
        self.modal
            .reduce(modal::ModalAction::Push(modal::ModalKind::ProviderPicker));
        self.modal
            .set_picker_item_count(providers::PROVIDERS.len().max(1));
    }

    fn open_subagent_model_picker(&mut self) {
        let Some(editor) = self.subagents.editor.as_ref() else {
            return;
        };
        let models = self.subagent_known_models_for(&editor.provider);
        self.config
            .reduce(config::ConfigAction::ModelsFetched(models.clone()));
        self.settings_picker_target = Some(SettingsPickerTarget::SubAgentModel);
        let count = widgets::model_picker::available_models(&self.config).len() + 1;
        self.modal
            .reduce(modal::ModalAction::Push(modal::ModalKind::ModelPicker));
        self.modal.set_picker_item_count(count);
    }

    pub(super) fn send_concierge_config(&mut self) {
        let config = serde_json::json!({
            "enabled": self.concierge.enabled,
            "detail_level": self.concierge.detail_level,
            "provider": self.concierge.provider,
            "model": self.concierge.model,
            "auto_cleanup_on_navigate": self.concierge.auto_cleanup_on_navigate,
        });
        self.send_daemon_command(DaemonCommand::SetConciergeConfig(config.to_string()));
    }

    pub(super) fn refresh_provider_models_for_current_auth(&mut self) {
        let models = providers::known_models_for_provider_auth(
            &self.config.provider,
            &self.config.auth_source,
        );
        if !models.is_empty() {
            self.config
                .reduce(config::ConfigAction::ModelsFetched(models.clone()));
            if !models.iter().any(|model| model.id == self.config.model) {
                let fallback = providers::default_model_for_provider_auth(
                    &self.config.provider,
                    &self.config.auth_source,
                );
                self.config.reduce(config::ConfigAction::SetModel(fallback));
            }
        }
    }

    fn show_openai_auth_modal(&mut self, url: String, status_text: &str) {
        self.openai_auth_url = Some(url);
        self.openai_auth_status_text = Some(status_text.to_string());
        self.modal
            .reduce(modal::ModalAction::Push(modal::ModalKind::OpenAIAuth));
    }

    pub(super) fn apply_provider_selection(&mut self, provider_id: &str) {
        let Some(def) = providers::find_by_id(provider_id) else {
            self.status_line = format!("Unknown provider: {provider_id}");
            return;
        };

        self.config.provider = def.id.to_string();
        self.config.base_url = def.default_base_url.to_string();
        self.config.model = def.default_model.to_string();
        self.config.custom_model_name.clear();
        self.config.api_key.clear();
        self.config.auth_source = providers::default_auth_source_for(def.id).to_string();
        self.config.api_transport = def.default_transport.to_string();
        self.config.assistant_id.clear();
        self.config.custom_context_window_tokens = if def.id == "custom" {
            Some(128_000)
        } else {
            None
        };
        self.config.context_window_tokens = if def.id == "custom" {
            self.config.custom_context_window_tokens.unwrap_or(128_000)
        } else {
            providers::known_context_window_for(def.id, def.default_model).unwrap_or(128_000)
        };

        if let Some(raw) = &self.config.agent_config_raw {
            if let Some(provider_config) = raw.get(def.id) {
                if let Some(saved_base_url) =
                    TuiModel::provider_field_str(provider_config, "baseUrl", "base_url")
                {
                    if !saved_base_url.is_empty() {
                        self.config.base_url = saved_base_url.to_string();
                    }
                }
                if let Some(key) =
                    TuiModel::provider_field_str(provider_config, "apiKey", "api_key")
                {
                    self.config.api_key = key.to_string();
                }
                if let Some(saved_model) =
                    TuiModel::provider_field_str(provider_config, "model", "model")
                {
                    if !saved_model.is_empty() {
                        self.config.model = saved_model.to_string();
                    }
                }
                if let Some(saved_custom_model_name) = TuiModel::provider_field_str(
                    provider_config,
                    "customModelName",
                    "custom_model_name",
                ) {
                    self.config.custom_model_name = saved_custom_model_name.to_string();
                }
                if let Some(saved_transport) =
                    TuiModel::provider_field_str(provider_config, "apiTransport", "api_transport")
                {
                    self.config.api_transport =
                        if def.supported_transports.contains(&saved_transport) {
                            saved_transport.to_string()
                        } else {
                            def.default_transport.to_string()
                        };
                }
                if let Some(saved_auth_source) =
                    TuiModel::provider_field_str(provider_config, "authSource", "auth_source")
                {
                    self.config.auth_source =
                        if def.supported_auth_sources.contains(&saved_auth_source) {
                            saved_auth_source.to_string()
                        } else {
                            def.default_auth_source.to_string()
                        };
                }
                if let Some(saved_assistant_id) =
                    TuiModel::provider_field_str(provider_config, "assistantId", "assistant_id")
                {
                    self.config.assistant_id = saved_assistant_id.to_string();
                }
                self.config.custom_context_window_tokens = TuiModel::provider_field_u64(
                    provider_config,
                    "customContextWindowTokens",
                    "context_window_tokens",
                )
                .map(|value| value.max(1000) as u32);
                self.config.context_window_tokens =
                    TuiModel::effective_context_window_for_provider_value(def.id, provider_config);
            }
        }

        if def.id == "openai" && self.config.auth_source == "chatgpt_subscription" {
            self.config.api_transport = "responses".to_string();
            self.refresh_openai_auth_status();
        }

        self.refresh_provider_models_for_current_auth();
        if providers::known_models_for_provider_auth(
            &self.config.provider,
            &self.config.auth_source,
        )
        .iter()
        .any(|model| model.id == self.config.model)
        {
            self.config.custom_model_name.clear();
        } else if self.config.custom_model_name.trim().is_empty()
            && !self.config.model.trim().is_empty()
        {
            self.config.custom_model_name = self.config.model.clone();
        }
        self.status_line = format!("Provider: {}", def.name);
        self.sync_config_to_daemon();
    }

    pub(super) fn run_auth_tab_action(&mut self) {
        let Some(entry) = self.auth.entries.get(self.auth.selected).cloned() else {
            return;
        };

        match self.auth.action_cursor {
            0 => {
                self.apply_provider_selection(&entry.provider_id);
                if entry.authenticated {
                    if entry.provider_id == "openai"
                        && self.config.auth_source == "chatgpt_subscription"
                    {
                        match crate::auth::clear_openai_codex_auth() {
                            Ok(()) => {
                                self.refresh_openai_auth_status();
                                self.send_daemon_command(DaemonCommand::GetProviderAuthStates);
                                self.status_line = "ChatGPT subscription auth cleared".to_string();
                            }
                            Err(err) => {
                                self.status_line = format!("Failed to clear ChatGPT auth: {err}");
                            }
                        }
                    } else {
                        self.config.api_key.clear();
                        self.sync_config_to_daemon();
                        self.send_daemon_command(DaemonCommand::GetProviderAuthStates);
                        self.status_line =
                            format!("Cleared credentials for {}", entry.provider_name);
                    }
                } else if entry.provider_id == "openai"
                    && self.config.auth_source == "chatgpt_subscription"
                {
                    match crate::auth::begin_openai_codex_auth_flow() {
                        Ok(crate::auth::OpenAICodexAuthFlowResult::AlreadyAvailable) => {
                            self.refresh_openai_auth_status();
                            self.send_daemon_command(DaemonCommand::GetProviderAuthStates);
                            self.status_line =
                                "ChatGPT subscription auth already available".to_string();
                        }
                        Ok(crate::auth::OpenAICodexAuthFlowResult::ImportedFromCodexCli) => {
                            self.refresh_openai_auth_status();
                            self.send_daemon_command(DaemonCommand::GetProviderAuthStates);
                            self.status_line =
                                "Imported ChatGPT auth from ~/.codex/auth.json".to_string();
                        }
                        Ok(crate::auth::OpenAICodexAuthFlowResult::Started { url }) => {
                            self.show_openai_auth_modal(
                                url,
                                "Open this URL in your browser to complete ChatGPT authentication.",
                            );
                        }
                        Err(err) => {
                            self.status_line = format!("Failed to start ChatGPT auth: {err}");
                        }
                    }
                } else {
                    self.settings
                        .reduce(SettingsAction::SwitchTab(SettingsTab::Provider));
                    self.settings
                        .start_editing("api_key", &self.config.api_key.clone());
                    self.status_line = format!("Enter API key for {}", entry.provider_name);
                }
            }
            1 => {
                self.apply_provider_selection(&entry.provider_id);
                if entry.provider_id == "openai"
                    && self.config.auth_source == "chatgpt_subscription"
                {
                    self.refresh_openai_auth_status();
                    self.send_daemon_command(DaemonCommand::GetProviderAuthStates);
                    self.status_line = if self.config.chatgpt_auth_available {
                        "ChatGPT subscription auth is available".to_string()
                    } else {
                        "ChatGPT subscription auth is not configured".to_string()
                    };
                } else {
                    self.auth.validating = Some(entry.provider_id.clone());
                    self.send_daemon_command(DaemonCommand::ValidateProvider {
                        provider_id: entry.provider_id,
                        base_url: self.config.base_url.clone(),
                        api_key: self.config.api_key.clone(),
                        auth_source: self.config.auth_source.clone(),
                    });
                }
            }
            _ => {}
        }
    }

    pub(super) fn run_subagent_action(&mut self) {
        match self.subagents.action_cursor {
            0 => {
                self.open_subagent_editor_new();
            }
            1 => {
                self.open_subagent_editor_existing();
            }
            2 => {
                if let Some(entry) = self.subagents.entries.get(self.subagents.selected) {
                    self.send_daemon_command(DaemonCommand::RemoveSubAgent(entry.id.clone()));
                    self.subagents
                        .reduce(crate::state::subagents::SubAgentsAction::Removed(
                            entry.id.clone(),
                        ));
                }
            }
            3 => {
                if let Some(entry) = self.subagents.entries.get(self.subagents.selected) {
                    if let Some(ref raw) = entry.raw_json {
                        let mut updated = raw.clone();
                        if let Some(obj) = updated.as_object_mut() {
                            obj.insert(
                                "enabled".to_string(),
                                serde_json::Value::Bool(!entry.enabled),
                            );
                        }
                        self.send_daemon_command(DaemonCommand::SetSubAgent(updated.to_string()));
                        self.subagents.reduce(
                            crate::state::subagents::SubAgentsAction::ToggleEnabled(
                                entry.id.clone(),
                            ),
                        );
                    }
                }
            }
            _ => {}
        }
    }

    pub(super) fn handle_auth_settings_key(&mut self, code: KeyCode) -> bool {
        match code {
            KeyCode::Up => {
                self.auth.selected = self.auth.selected.saturating_sub(1);
                true
            }
            KeyCode::Down => {
                if self.auth.selected + 1 < self.auth.entries.len() {
                    self.auth.selected += 1;
                }
                true
            }
            KeyCode::Left => {
                if self.auth.actions_focused && self.auth.action_cursor > 0 {
                    self.auth.action_cursor -= 1;
                } else {
                    self.auth.actions_focused = false;
                }
                true
            }
            KeyCode::Right => {
                if self.auth.actions_focused {
                    self.auth.action_cursor = (self.auth.action_cursor + 1).min(1);
                } else {
                    self.auth.actions_focused = true;
                }
                true
            }
            KeyCode::Enter => {
                if !self.auth.actions_focused {
                    self.auth.actions_focused = true;
                } else {
                    self.run_auth_tab_action();
                }
                true
            }
            KeyCode::Char(' ') => {
                self.run_auth_tab_action();
                true
            }
            KeyCode::Char('h') => {
                if self.auth.actions_focused && self.auth.action_cursor > 0 {
                    self.auth.action_cursor -= 1;
                } else {
                    self.auth.actions_focused = false;
                }
                true
            }
            KeyCode::Char('l') => {
                if self.auth.actions_focused {
                    self.auth.action_cursor = (self.auth.action_cursor + 1).min(1);
                } else {
                    self.auth.actions_focused = true;
                }
                true
            }
            _ => false,
        }
    }

    pub(super) fn handle_subagent_settings_key(&mut self, code: KeyCode) -> bool {
        if self.subagents.editor.is_some() {
            match code {
                KeyCode::Esc => {
                    self.close_subagent_editor();
                    true
                }
                KeyCode::Up => {
                    if let Some(editor) = self.subagents.editor.as_mut() {
                        editor.field = editor.field.prev();
                    }
                    true
                }
                KeyCode::Down | KeyCode::Tab => {
                    if let Some(editor) = self.subagents.editor.as_mut() {
                        editor.field = editor.field.next();
                    }
                    true
                }
                KeyCode::BackTab => {
                    if let Some(editor) = self.subagents.editor.as_mut() {
                        editor.field = editor.field.prev();
                    }
                    true
                }
                KeyCode::Left => {
                    if let Some(editor) = self.subagents.editor.as_ref() {
                        if matches!(
                            editor.field,
                            crate::state::subagents::SubAgentEditorField::Role
                        ) {
                            self.cycle_subagent_role(-1);
                            return true;
                        }
                    }
                    false
                }
                KeyCode::Right => {
                    if let Some(editor) = self.subagents.editor.as_ref() {
                        if matches!(
                            editor.field,
                            crate::state::subagents::SubAgentEditorField::Role
                        ) {
                            self.cycle_subagent_role(1);
                            return true;
                        }
                    }
                    false
                }
                KeyCode::Enter => {
                    let Some(field) = self.subagents.editor.as_ref().map(|editor| editor.field)
                    else {
                        return true;
                    };
                    match field {
                        crate::state::subagents::SubAgentEditorField::Name => {
                            let current = self
                                .subagents
                                .editor
                                .as_ref()
                                .map(|editor| editor.name.clone())
                                .unwrap_or_default();
                            self.settings.start_editing("subagent_name", &current);
                        }
                        crate::state::subagents::SubAgentEditorField::Provider => {
                            self.open_subagent_provider_picker();
                        }
                        crate::state::subagents::SubAgentEditorField::Model => {
                            self.open_subagent_model_picker();
                        }
                        crate::state::subagents::SubAgentEditorField::Role => {
                            let current = self
                                .subagents
                                .editor
                                .as_ref()
                                .map(|editor| editor.role.clone())
                                .unwrap_or_default();
                            self.settings.start_editing("subagent_role", &current);
                        }
                        crate::state::subagents::SubAgentEditorField::SystemPrompt => {
                            let current = self
                                .subagents
                                .editor
                                .as_ref()
                                .map(|editor| editor.system_prompt.clone())
                                .unwrap_or_default();
                            self.settings
                                .start_editing("subagent_system_prompt", &current);
                        }
                        crate::state::subagents::SubAgentEditorField::Save => {
                            self.commit_subagent_editor();
                        }
                        crate::state::subagents::SubAgentEditorField::Cancel => {
                            self.close_subagent_editor();
                        }
                    }
                    true
                }
                KeyCode::Char('s') => {
                    self.commit_subagent_editor();
                    true
                }
                _ => false,
            }
        } else {
            match code {
                KeyCode::Up => {
                    self.subagents.selected = self.subagents.selected.saturating_sub(1);
                    true
                }
                KeyCode::Down => {
                    if self.subagents.selected + 1 < self.subagents.entries.len() {
                        self.subagents.selected += 1;
                    }
                    true
                }
                KeyCode::Left => {
                    if !self.subagents.entries.is_empty() {
                        self.subagents.actions_focused = true;
                        self.subagents.action_cursor =
                            self.subagents.action_cursor.max(1).saturating_sub(1).max(1);
                    }
                    true
                }
                KeyCode::Right => {
                    if !self.subagents.entries.is_empty() {
                        self.subagents.actions_focused = true;
                        self.subagents.action_cursor =
                            (self.subagents.action_cursor.max(1) + 1).min(3);
                    }
                    true
                }
                KeyCode::Enter => {
                    if self.subagents.entries.is_empty() {
                        self.subagents.action_cursor = 0;
                        self.run_subagent_action();
                    } else if self.subagents.actions_focused {
                        self.run_subagent_action();
                    } else {
                        self.subagents.action_cursor = 1;
                        self.subagents.actions_focused = true;
                        self.run_subagent_action();
                    }
                    true
                }
                KeyCode::Char(' ') => {
                    if !self.subagents.entries.is_empty() {
                        self.subagents.actions_focused = true;
                        self.subagents.action_cursor = 3;
                        self.run_subagent_action();
                    }
                    true
                }
                KeyCode::Char('a') => {
                    self.subagents.action_cursor = 0;
                    self.run_subagent_action();
                    true
                }
                KeyCode::Char('e') => {
                    if !self.subagents.entries.is_empty() {
                        self.subagents.actions_focused = true;
                        self.subagents.action_cursor = 1;
                        self.run_subagent_action();
                    }
                    true
                }
                KeyCode::Delete | KeyCode::Backspace | KeyCode::Char('d') => {
                    if !self.subagents.entries.is_empty() {
                        self.subagents.actions_focused = true;
                        self.subagents.action_cursor = 2;
                        self.run_subagent_action();
                    }
                    true
                }
                KeyCode::Char('h') => {
                    if !self.subagents.entries.is_empty() {
                        self.subagents.actions_focused = true;
                        self.subagents.action_cursor =
                            self.subagents.action_cursor.max(1).saturating_sub(1).max(1);
                    }
                    true
                }
                KeyCode::Char('l') => {
                    if !self.subagents.entries.is_empty() {
                        self.subagents.actions_focused = true;
                        self.subagents.action_cursor =
                            (self.subagents.action_cursor.max(1) + 1).min(3);
                    }
                    true
                }
                _ => false,
            }
        }
    }

    pub(super) fn activate_settings_field(&mut self) {
        let field = self.settings.current_field_name().to_string();
        match field.as_str() {
            "provider" => {
                self.settings_picker_target = Some(SettingsPickerTarget::Provider);
                self.execute_command("provider");
            }
            "model" => {
                self.settings_picker_target = Some(SettingsPickerTarget::Model);
                self.execute_command("model");
            }
            "auth_source" => {
                let supported = providers::supported_auth_sources_for(&self.config.provider);
                let current_idx = supported
                    .iter()
                    .position(|source| *source == self.config.auth_source)
                    .unwrap_or(0);
                let next_idx = (current_idx + 1) % supported.len().max(1);
                self.config.auth_source = supported
                    .get(next_idx)
                    .copied()
                    .unwrap_or("api_key")
                    .to_string();
                if self.config.provider == "openai"
                    && self.config.auth_source == "chatgpt_subscription"
                {
                    self.refresh_openai_auth_status();
                }
                self.refresh_provider_models_for_current_auth();
                if self.config.provider == "openai"
                    && self.config.auth_source == "chatgpt_subscription"
                {
                    self.config.api_transport = "responses".to_string();
                }
                self.sync_config_to_daemon();
            }
            "api_transport" => {
                let supported = providers::supported_transports_for(&self.config.provider);
                let current_idx = supported
                    .iter()
                    .position(|transport| *transport == self.config.api_transport)
                    .unwrap_or(0);
                let next_idx = (current_idx + 1) % supported.len().max(1);
                self.config.api_transport = supported
                    .get(next_idx)
                    .copied()
                    .unwrap_or("chat_completions")
                    .to_string();
                if self.config.provider == "openai"
                    && self.config.auth_source == "chatgpt_subscription"
                {
                    self.config.api_transport = "responses".to_string();
                }
                self.sync_config_to_daemon();
            }
            "assistant_id" => self
                .settings
                .start_editing("assistant_id", &self.config.assistant_id.clone()),
            "reasoning_effort" => self.execute_command("effort"),
            "base_url" => self
                .settings
                .start_editing("base_url", &self.config.base_url.clone()),
            "api_key" => {
                if self.config.auth_source == "api_key" {
                    self.settings
                        .start_editing("api_key", &self.config.api_key.clone());
                } else if self.config.chatgpt_auth_available {
                    match crate::auth::clear_openai_codex_auth() {
                        Ok(()) => {
                            self.refresh_openai_auth_status();
                            self.status_line = "ChatGPT subscription auth cleared".to_string();
                        }
                        Err(err) => {
                            self.status_line = format!("Failed to clear ChatGPT auth: {err}");
                        }
                    }
                } else {
                    match crate::auth::begin_openai_codex_auth_flow() {
                        Ok(crate::auth::OpenAICodexAuthFlowResult::AlreadyAvailable) => {
                            self.refresh_openai_auth_status();
                            self.status_line =
                                "ChatGPT subscription auth already available".to_string();
                        }
                        Ok(crate::auth::OpenAICodexAuthFlowResult::ImportedFromCodexCli) => {
                            self.refresh_openai_auth_status();
                            self.status_line =
                                "Imported ChatGPT auth from ~/.codex/auth.json".to_string();
                        }
                        Ok(crate::auth::OpenAICodexAuthFlowResult::Started { url }) => {
                            self.show_openai_auth_modal(
                                url,
                                "Open this URL in your browser to complete ChatGPT authentication.",
                            );
                        }
                        Err(err) => {
                            self.status_line = format!("Failed to start ChatGPT auth: {err}");
                        }
                    }
                }
            }
            "gateway_prefix" => {
                self.settings
                    .start_editing("gateway_prefix", &self.config.gateway_prefix.clone());
            }
            "slack_token" => {
                self.settings
                    .start_editing("slack_token", &self.config.slack_token.clone());
            }
            "slack_channel_filter" => {
                self.settings.start_editing(
                    "slack_channel_filter",
                    &self.config.slack_channel_filter.clone(),
                );
            }
            "telegram_token" => {
                self.settings
                    .start_editing("telegram_token", &self.config.telegram_token.clone());
            }
            "telegram_allowed_chats" => {
                self.settings.start_editing(
                    "telegram_allowed_chats",
                    &self.config.telegram_allowed_chats.clone(),
                );
            }
            "discord_token" => {
                self.settings
                    .start_editing("discord_token", &self.config.discord_token.clone());
            }
            "discord_channel_filter" => {
                self.settings.start_editing(
                    "discord_channel_filter",
                    &self.config.discord_channel_filter.clone(),
                );
            }
            "discord_allowed_users" => {
                self.settings.start_editing(
                    "discord_allowed_users",
                    &self.config.discord_allowed_users.clone(),
                );
            }
            "whatsapp_allowed_contacts" => {
                self.settings.start_editing(
                    "whatsapp_allowed_contacts",
                    &self.config.whatsapp_allowed_contacts.clone(),
                );
            }
            "whatsapp_token" => {
                self.settings
                    .start_editing("whatsapp_token", &self.config.whatsapp_token.clone());
            }
            "whatsapp_phone_id" => {
                self.settings
                    .start_editing("whatsapp_phone_id", &self.config.whatsapp_phone_id.clone());
            }
            "search_provider" => {
                let next = match self.config.search_provider.as_str() {
                    "none" | "" => "firecrawl",
                    "firecrawl" => "exa",
                    "exa" => "tavily",
                    _ => "none",
                };
                self.config.search_provider = next.to_string();
                self.sync_config_to_daemon();
            }
            "firecrawl_api_key" => self
                .settings
                .start_editing("firecrawl_api_key", &self.config.firecrawl_api_key.clone()),
            "exa_api_key" => self
                .settings
                .start_editing("exa_api_key", &self.config.exa_api_key.clone()),
            "tavily_api_key" => self
                .settings
                .start_editing("tavily_api_key", &self.config.tavily_api_key.clone()),
            "search_max_results" => self.settings.start_editing(
                "search_max_results",
                &self.config.search_max_results.to_string(),
            ),
            "search_timeout" => self.settings.start_editing(
                "search_timeout",
                &self.config.search_timeout_secs.to_string(),
            ),
            "honcho_api_key" => self
                .settings
                .start_editing("honcho_api_key", &self.config.honcho_api_key.clone()),
            "honcho_base_url" => self
                .settings
                .start_editing("honcho_base_url", &self.config.honcho_base_url.clone()),
            "honcho_workspace_id" => self.settings.start_editing(
                "honcho_workspace_id",
                &self.config.honcho_workspace_id.clone(),
            ),
            "context_window_tokens" if self.config.provider == "custom" => {
                self.settings.start_editing(
                    "context_window_tokens",
                    &self
                        .config
                        .custom_context_window_tokens
                        .unwrap_or(128_000)
                        .to_string(),
                )
            }
            "max_context_messages" => self.settings.start_editing(
                "max_context_messages",
                &self.config.max_context_messages.to_string(),
            ),
            "max_tool_loops" => self
                .settings
                .start_editing("max_tool_loops", &self.config.max_tool_loops.to_string()),
            "max_retries" => self
                .settings
                .start_editing("max_retries", &self.config.max_retries.to_string()),
            "retry_delay_ms" => self
                .settings
                .start_editing("retry_delay_ms", &self.config.retry_delay_ms.to_string()),
            "context_budget_tokens" => self.settings.start_editing(
                "context_budget_tokens",
                &self.config.context_budget_tokens.to_string(),
            ),
            "compact_threshold_pct" => self.settings.start_editing(
                "compact_threshold_pct",
                &self.config.compact_threshold_pct.to_string(),
            ),
            "keep_recent_on_compact" => self.settings.start_editing(
                "keep_recent_on_compact",
                &self.config.keep_recent_on_compact.to_string(),
            ),
            "bash_timeout_secs" => self.settings.start_editing(
                "bash_timeout_secs",
                &self.config.bash_timeout_secs.to_string(),
            ),
            "snapshot_max_count" => self.settings.start_editing(
                "snapshot_max_count",
                &self.config.snapshot_max_count.to_string(),
            ),
            "snapshot_max_size_mb" => self.settings.start_editing(
                "snapshot_max_size_mb",
                &self.config.snapshot_max_size_mb.to_string(),
            ),
            "snapshot_stats" => {
                // Read-only field, no-op on Enter
            }
            "agent_name" => {
                let current = self
                    .config
                    .agent_config_raw
                    .as_ref()
                    .and_then(|raw| raw.get("agent_name"))
                    .and_then(|value| value.as_str())
                    .unwrap_or("Tamux")
                    .to_string();
                self.settings.start_editing("agent_name", &current);
            }
            "system_prompt" => {
                let current = self
                    .config
                    .agent_config_raw
                    .as_ref()
                    .and_then(|raw| raw.get("system_prompt"))
                    .and_then(|value| value.as_str())
                    .unwrap_or("")
                    .to_string();
                self.settings.start_editing("system_prompt", &current);
            }
            // ── Sub-Agents tab ──
            "subagent_list" => {
                self.subagents.actions_focused = false;
            }
            "concierge_enabled" => {
                self.concierge.enabled = !self.concierge.enabled;
                self.send_concierge_config();
            }
            "concierge_detail_level" => {
                let levels = [
                    "minimal",
                    "context_summary",
                    "proactive_triage",
                    "daily_briefing",
                ];
                let current_idx = levels
                    .iter()
                    .position(|level| *level == self.concierge.detail_level)
                    .unwrap_or(0);
                self.concierge.detail_level = levels[(current_idx + 1) % levels.len()].to_string();
                self.send_concierge_config();
            }
            "concierge_provider" => {
                self.settings_picker_target = Some(SettingsPickerTarget::ConciergeProvider);
                self.execute_command("provider");
            }
            "concierge_model" => {
                self.settings_picker_target = Some(SettingsPickerTarget::ConciergeModel);
                let provider_id = self
                    .concierge
                    .provider
                    .clone()
                    .unwrap_or_else(|| self.config.provider.clone());
                let models = providers::known_models_for_provider_auth(&provider_id, "api_key");
                if !models.is_empty() {
                    self.config
                        .reduce(config::ConfigAction::ModelsFetched(models));
                }
                let count = widgets::model_picker::available_models(&self.config).len() + 1;
                self.modal
                    .reduce(modal::ModalAction::Push(modal::ModalKind::ModelPicker));
                self.modal.set_picker_item_count(count);
            }
            _ => {}
        }
    }

    pub(super) fn toggle_settings_field(&mut self) {
        let field = self.settings.current_field_name().to_string();
        match field.as_str() {
            "gateway_enabled" => {
                self.config.gateway_enabled = !self.config.gateway_enabled;
                self.sync_config_to_daemon();
            }
            "web_search_enabled" => {
                self.config.tool_web_search = !self.config.tool_web_search;
                self.sync_config_to_daemon();
            }
            "enable_streaming" => {
                self.config.enable_streaming = !self.config.enable_streaming;
                self.sync_config_to_daemon();
            }
            "enable_conversation_memory" => {
                self.config.enable_conversation_memory = !self.config.enable_conversation_memory;
                self.sync_config_to_daemon();
            }
            "enable_honcho_memory" => {
                self.config.enable_honcho_memory = !self.config.enable_honcho_memory;
                self.sync_config_to_daemon();
            }
            "auto_compact_context" => {
                self.config.auto_compact_context = !self.config.auto_compact_context;
                self.sync_config_to_daemon();
            }
            "snapshot_auto_cleanup" => {
                self.config.snapshot_auto_cleanup = !self.config.snapshot_auto_cleanup;
                self.sync_config_to_daemon();
            }
            "concierge_enabled" => {
                self.concierge.enabled = !self.concierge.enabled;
                self.send_concierge_config();
            }
            "concierge_provider" => {
                self.concierge.provider = None;
                self.send_concierge_config();
            }
            "concierge_model" => {
                self.concierge.model = None;
                self.send_concierge_config();
            }
            field if field.starts_with("tool_") => {
                let tool_name = field.strip_prefix("tool_").unwrap_or(field).to_string();
                self.config
                    .reduce(config::ConfigAction::ToggleTool(tool_name));
                self.sync_config_to_daemon();
            }
            _ => self.settings.reduce(SettingsAction::ToggleCheckbox),
        }
    }

    pub(super) fn settings_field_click_uses_toggle(&self) -> bool {
        matches!(
            self.settings.current_field_name(),
            "gateway_enabled"
                | "web_search_enabled"
                | "enable_streaming"
                | "enable_conversation_memory"
                | "enable_honcho_memory"
                | "auto_compact_context"
                | "snapshot_auto_cleanup"
        ) || self.settings.current_field_name().starts_with("tool_")
    }
}
