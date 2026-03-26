use super::*;

impl TuiModel {
    fn provider_auth_snapshot(&self, provider_id: &str) -> (String, String, String) {
        let mut base_url = providers::find_by_id(provider_id)
            .map(|def| def.default_base_url.to_string())
            .unwrap_or_default();
        let mut api_key = String::new();
        let mut auth_source = providers::default_auth_source_for(provider_id).to_string();

        if self.config.provider == provider_id {
            return (
                self.config.base_url.clone(),
                self.config.api_key.clone(),
                self.config.auth_source.clone(),
            );
        }

        if let Some(raw) = &self.config.agent_config_raw {
            if let Some(provider_config) = raw.get(provider_id) {
                if let Some(value) =
                    TuiModel::provider_field_str(provider_config, "base_url", "base_url")
                {
                    if !value.is_empty() {
                        base_url = value.to_string();
                    }
                }
                if let Some(value) =
                    TuiModel::provider_field_str(provider_config, "api_key", "api_key")
                {
                    api_key = value.to_string();
                }
                if let Some(value) =
                    TuiModel::provider_field_str(provider_config, "auth_source", "auth_source")
                {
                    auth_source = value.to_string();
                }
            }
        }

        (base_url, api_key, auth_source)
    }

    fn upsert_saved_provider_api_key(&mut self, provider_id: &str, api_key: &str) {
        let mut raw = self
            .config
            .agent_config_raw
            .clone()
            .unwrap_or_else(|| serde_json::json!({}));

        if raw
            .get(provider_id)
            .and_then(|value| value.as_object())
            .is_none()
        {
            raw[provider_id] = self.provider_config_value(provider_id);
        }
        raw[provider_id]["api_key"] = serde_json::Value::String(api_key.to_string());

        if raw
            .get("providers")
            .and_then(|value| value.as_object())
            .is_none()
        {
            raw["providers"] = serde_json::json!({});
        }
        if raw["providers"]
            .get(provider_id)
            .and_then(|value| value.as_object())
            .is_none()
        {
            raw["providers"][provider_id] = self.provider_wire_config_value(provider_id);
        }
        raw["providers"][provider_id]["api_key"] = serde_json::Value::String(api_key.to_string());

        self.config.agent_config_raw = Some(raw);
    }

    fn clear_saved_provider_api_key(&mut self, provider_id: &str) {
        self.upsert_saved_provider_api_key(provider_id, "");
    }

    fn apply_provider_selection_internal(&mut self, provider_id: &str, sync: bool) {
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
                // For predefined providers, always use the canonical base_url
                // from the definition — stale DB values must not override it.
                if def.id == "custom" {
                    if let Some(saved_base_url) =
                        TuiModel::provider_field_str(provider_config, "base_url", "base_url")
                    {
                        if !saved_base_url.is_empty() {
                            self.config.base_url = saved_base_url.to_string();
                        }
                    }
                }
                if let Some(key) =
                    TuiModel::provider_field_str(provider_config, "api_key", "api_key")
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
                    "custom_model_name",
                    "custom_model_name",
                ) {
                    self.config.custom_model_name = saved_custom_model_name.to_string();
                }
                if let Some(saved_transport) =
                    TuiModel::provider_field_str(provider_config, "api_transport", "api_transport")
                {
                    self.config.api_transport =
                        if def.supported_transports.contains(&saved_transport) {
                            saved_transport.to_string()
                        } else {
                            def.default_transport.to_string()
                        };
                }
                if let Some(saved_auth_source) =
                    TuiModel::provider_field_str(provider_config, "auth_source", "auth_source")
                {
                    self.config.auth_source =
                        if def.supported_auth_sources.contains(&saved_auth_source) {
                            saved_auth_source.to_string()
                        } else {
                            def.default_auth_source.to_string()
                        };
                }
                if let Some(saved_assistant_id) =
                    TuiModel::provider_field_str(provider_config, "assistant_id", "assistant_id")
                {
                    self.config.assistant_id = saved_assistant_id.to_string();
                }
                self.config.custom_context_window_tokens = TuiModel::provider_field_u64(
                    provider_config,
                    "context_window_tokens",
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
        if sync {
            self.sync_config_to_daemon();
        }
    }

    fn stored_provider_api_key(&self, provider_id: &str) -> String {
        self.config
            .agent_config_raw
            .as_ref()
            .and_then(|raw| raw.get("providers"))
            .and_then(|providers| providers.get(provider_id))
            .and_then(|provider| TuiModel::provider_field_str(provider, "api_key", "api_key"))
            .map(str::to_string)
            .or_else(|| {
                self.config
                    .agent_config_raw
                    .as_ref()
                    .and_then(|raw| raw.get(provider_id))
                    .and_then(|provider| {
                        TuiModel::provider_field_str(provider, "api_key", "api_key")
                    })
                    .map(str::to_string)
            })
            .unwrap_or_default()
    }

    fn start_auth_login(&mut self, provider_id: &str, provider_name: &str) {
        let initial_key = if self.config.provider == provider_id {
            self.config.api_key.clone()
        } else {
            self.stored_provider_api_key(provider_id)
        };
        self.auth.reduce(crate::state::auth::AuthAction::StartLogin(
            provider_id.to_string(),
        ));
        self.auth.login_buffer = initial_key;
        self.auth.login_cursor = self.auth.login_buffer.chars().count();
        self.auth.actions_focused = false;
        self.status_line = format!("Enter API key for {provider_name}");
    }

    fn confirm_auth_login(&mut self) {
        let Some(provider_id) = self.auth.login_target.clone() else {
            return;
        };
        let provider_name = self
            .auth
            .entries
            .iter()
            .find(|entry| entry.provider_id == provider_id)
            .map(|entry| entry.provider_name.clone())
            .or_else(|| providers::find_by_id(&provider_id).map(|def| def.name.to_string()))
            .unwrap_or_else(|| provider_id.clone());
        let api_key = self.auth.login_buffer.trim().to_string();

        if api_key.is_empty() {
            self.status_line = format!("API key required for {provider_name}");
            return;
        }

        let is_active_provider = self.config.provider == provider_id;
        if self.config.provider == provider_id {
            self.config.api_key = api_key.clone();
        }
        self.upsert_saved_provider_api_key(&provider_id, &api_key);

        if let Ok(value_json) = serde_json::to_string(&serde_json::Value::String(api_key.clone())) {
            self.send_daemon_command(DaemonCommand::SetConfigItem {
                key_path: format!("/providers/{provider_id}/api_key"),
                value_json: value_json.clone(),
            });
            self.send_daemon_command(DaemonCommand::SetConfigItem {
                key_path: format!("/{provider_id}/api_key"),
                value_json: value_json.clone(),
            });
            if is_active_provider {
                self.send_daemon_command(DaemonCommand::SetConfigItem {
                    key_path: "/api_key".to_string(),
                    value_json,
                });
            }
        }
        self.send_daemon_command(DaemonCommand::GetProviderAuthStates);
        self.auth
            .reduce(crate::state::auth::AuthAction::ConfirmLogin);
        self.status_line = format!("Saved credentials for {provider_name}");
    }

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
        self.apply_provider_selection_internal(provider_id, true);
    }

    pub(super) fn run_auth_tab_action(&mut self) {
        let Some(entry) = self.auth.entries.get(self.auth.selected).cloned() else {
            return;
        };

        match self.auth.action_cursor {
            0 => {
                if entry.authenticated {
                    if entry.provider_id == "openai" && entry.auth_source == "chatgpt_subscription"
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
                        if self.config.provider == entry.provider_id {
                            self.config.api_key.clear();
                        }
                        self.clear_saved_provider_api_key(&entry.provider_id);
                        if let Ok(value_json) =
                            serde_json::to_string(&serde_json::Value::String(String::new()))
                        {
                            self.send_daemon_command(DaemonCommand::SetConfigItem {
                                key_path: format!("/providers/{}/api_key", entry.provider_id),
                                value_json: value_json.clone(),
                            });
                            self.send_daemon_command(DaemonCommand::SetConfigItem {
                                key_path: format!("/{}/api_key", entry.provider_id),
                                value_json: value_json.clone(),
                            });
                            if self.config.provider == entry.provider_id {
                                self.send_daemon_command(DaemonCommand::SetConfigItem {
                                    key_path: "/api_key".to_string(),
                                    value_json,
                                });
                            }
                        }
                        self.send_daemon_command(DaemonCommand::GetProviderAuthStates);
                        self.status_line =
                            format!("Cleared credentials for {}", entry.provider_name);
                    }
                } else if entry.provider_id == "openai"
                    && entry.auth_source == "chatgpt_subscription"
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
                    self.start_auth_login(&entry.provider_id, &entry.provider_name);
                }
            }
            1 => {
                if !entry.authenticated && entry.provider_id == "openai" {
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
                    let (base_url, api_key, auth_source) =
                        self.provider_auth_snapshot(&entry.provider_id);
                    if entry.provider_id == "openai" && auth_source == "chatgpt_subscription" {
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
                            base_url,
                            api_key,
                            auth_source,
                        });
                    }
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
        if self.auth.login_target.is_some() {
            match code {
                KeyCode::Esc => {
                    self.auth
                        .reduce(crate::state::auth::AuthAction::CancelLogin);
                    self.status_line = "Login cancelled".to_string();
                    true
                }
                KeyCode::Enter => {
                    self.confirm_auth_login();
                    true
                }
                KeyCode::Backspace => {
                    self.auth
                        .reduce(crate::state::auth::AuthAction::LoginKeyBackspace);
                    true
                }
                KeyCode::Left => {
                    self.auth.login_cursor = self.auth.login_cursor.saturating_sub(1);
                    true
                }
                KeyCode::Right => {
                    self.auth.login_cursor =
                        (self.auth.login_cursor + 1).min(self.auth.login_buffer.chars().count());
                    true
                }
                KeyCode::Char(c) => {
                    self.auth
                        .reduce(crate::state::auth::AuthAction::LoginKeyChar(c));
                    true
                }
                _ => true,
            }
        } else {
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
                if providers::uses_fixed_anthropic_messages(
                    &self.config.provider,
                    &self.config.model,
                ) {
                    self.status_line =
                        "This provider uses the Anthropic messages protocol.".to_string();
                    return;
                }
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
            "whatsapp_link_device" => {
                self.modal.set_whatsapp_link_starting();
                self.send_daemon_command(DaemonCommand::WhatsAppLinkSubscribe);
                self.send_daemon_command(DaemonCommand::WhatsAppLinkStart);
                self.modal
                    .reduce(modal::ModalAction::Push(modal::ModalKind::WhatsAppLink));
                self.status_line = "Starting WhatsApp link workflow".to_string();
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
            "browse_provider" => {
                let next = match self.config.browse_provider.as_str() {
                    "auto" | "" => "lightpanda",
                    "lightpanda" => "chrome",
                    "chrome" => "none",
                    _ => "auto",
                };
                self.config.browse_provider = next.to_string();
                self.sync_config_to_daemon();
            }
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
            "compliance_mode" => {
                let next = match self.config.compliance_mode.as_str() {
                    "standard" => "soc2",
                    "soc2" => "hipaa",
                    "hipaa" => "fedramp",
                    _ => "standard",
                };
                self.config.compliance_mode = next.to_string();
                self.sync_config_to_daemon();
            }
            "compliance_retention_days" => self.settings.start_editing(
                "compliance_retention_days",
                &self.config.compliance_retention_days.to_string(),
            ),
            "tool_synthesis_max_generated_tools" => self.settings.start_editing(
                "tool_synthesis_max_generated_tools",
                &self.config.tool_synthesis_max_generated_tools.to_string(),
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
            "auto_retry" => {
                self.config.auto_retry = !self.config.auto_retry;
                self.sync_config_to_daemon();
            }
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
            "managed_security_level" => {
                let levels = ["highest", "moderate", "lowest", "yolo"];
                let current_idx = levels
                    .iter()
                    .position(|level| *level == self.config.managed_security_level)
                    .unwrap_or(2);
                self.config.managed_security_level =
                    levels[(current_idx + 1) % levels.len()].to_string();
                self.sync_config_to_daemon();
            }
            // ── Features tab ──
            "feat_tier_override" => {
                let tiers = ["newcomer", "familiar", "power_user", "expert"];
                let current = self
                    .config
                    .agent_config_raw
                    .as_ref()
                    .and_then(|r| r.get("tier"))
                    .and_then(|t| t.get("user_override"))
                    .and_then(|v| v.as_str())
                    .unwrap_or(&self.tier.current_tier);
                let current_idx = tiers.iter().position(|t| *t == current).unwrap_or(0);
                let next = tiers[(current_idx + 1) % tiers.len()];
                self.send_daemon_command(DaemonCommand::SetConfigItem {
                    key_path: "/tier/user_override".to_string(),
                    value_json: format!("\"{}\"", next),
                });
                // Update local raw config optimistically
                if let Some(ref mut raw) = self.config.agent_config_raw {
                    if raw.get("tier").is_none() {
                        raw["tier"] = serde_json::json!({});
                    }
                    raw["tier"]["user_override"] = serde_json::Value::String(next.to_string());
                }
                self.tier.on_tier_changed(next);
            }
            "feat_security_level" => {
                let levels = ["permissive", "balanced", "strict"];
                let current = self
                    .config
                    .agent_config_raw
                    .as_ref()
                    .and_then(|r| r.get("managed_security_level"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("balanced");
                let current_idx = levels.iter().position(|l| *l == current).unwrap_or(1);
                let next = levels[(current_idx + 1) % levels.len()];
                self.send_daemon_command(DaemonCommand::SetConfigItem {
                    key_path: "/managed_security_level".to_string(),
                    value_json: format!("\"{}\"", next),
                });
                if let Some(ref mut raw) = self.config.agent_config_raw {
                    raw["managed_security_level"] = serde_json::Value::String(next.to_string());
                }
            }
            "feat_heartbeat_cron" => {
                let current = self
                    .config
                    .agent_config_raw
                    .as_ref()
                    .and_then(|r| r.get("heartbeat"))
                    .and_then(|h| h.get("cron"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("*/15 * * * *")
                    .to_string();
                self.settings.start_editing("feat_heartbeat_cron", &current);
            }
            "feat_heartbeat_quiet_start" => {
                let current = self
                    .config
                    .agent_config_raw
                    .as_ref()
                    .and_then(|r| r.get("heartbeat"))
                    .and_then(|h| h.get("quiet_start"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("22:00")
                    .to_string();
                self.settings
                    .start_editing("feat_heartbeat_quiet_start", &current);
            }
            "feat_heartbeat_quiet_end" => {
                let current = self
                    .config
                    .agent_config_raw
                    .as_ref()
                    .and_then(|r| r.get("heartbeat"))
                    .and_then(|h| h.get("quiet_end"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("07:00")
                    .to_string();
                self.settings
                    .start_editing("feat_heartbeat_quiet_end", &current);
            }
            "feat_decay_half_life_hours" => {
                let current = self
                    .config
                    .agent_config_raw
                    .as_ref()
                    .and_then(|r| r.get("consolidation"))
                    .and_then(|c| c.get("decay_half_life_hours"))
                    .and_then(|v| v.as_f64())
                    .map(|v| format!("{:.0}", v))
                    .unwrap_or_else(|| "69".to_string());
                self.settings
                    .start_editing("feat_decay_half_life_hours", &current);
            }
            "feat_heuristic_promotion_threshold" => {
                let current = self
                    .config
                    .agent_config_raw
                    .as_ref()
                    .and_then(|r| r.get("consolidation"))
                    .and_then(|c| c.get("heuristic_promotion_threshold"))
                    .and_then(|v| v.as_u64())
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| "5".to_string());
                self.settings
                    .start_editing("feat_heuristic_promotion_threshold", &current);
            }
            "feat_skill_promotion_threshold" => {
                let current = self
                    .config
                    .agent_config_raw
                    .as_ref()
                    .and_then(|r| r.get("skill_discovery"))
                    .and_then(|s| s.get("promotion_threshold"))
                    .and_then(|v| v.as_u64())
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| "3".to_string());
                self.settings
                    .start_editing("feat_skill_promotion_threshold", &current);
            }
            _ => {}
        }
    }

    pub(super) fn toggle_settings_field(&mut self) {
        let field = self.settings.current_field_name().to_string();
        match field.as_str() {
            "managed_sandbox_enabled" => {
                self.config.managed_sandbox_enabled = !self.config.managed_sandbox_enabled;
                self.sync_config_to_daemon();
            }
            "managed_security_level" => {
                let levels = ["highest", "moderate", "lowest", "yolo"];
                let current_idx = levels
                    .iter()
                    .position(|level| *level == self.config.managed_security_level)
                    .unwrap_or(2);
                self.config.managed_security_level =
                    levels[(current_idx + 1) % levels.len()].to_string();
                self.sync_config_to_daemon();
            }
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
            "auto_retry" => {
                self.config.auto_retry = !self.config.auto_retry;
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
            "anticipatory_enabled" => {
                self.config.anticipatory_enabled = !self.config.anticipatory_enabled;
                self.sync_config_to_daemon();
            }
            "anticipatory_morning_brief" => {
                self.config.anticipatory_morning_brief = !self.config.anticipatory_morning_brief;
                self.sync_config_to_daemon();
            }
            "anticipatory_predictive_hydration" => {
                self.config.anticipatory_predictive_hydration =
                    !self.config.anticipatory_predictive_hydration;
                self.sync_config_to_daemon();
            }
            "anticipatory_stuck_detection" => {
                self.config.anticipatory_stuck_detection =
                    !self.config.anticipatory_stuck_detection;
                self.sync_config_to_daemon();
            }
            "operator_model_enabled" => {
                self.config.operator_model_enabled = !self.config.operator_model_enabled;
                self.sync_config_to_daemon();
            }
            "operator_model_allow_message_statistics" => {
                self.config.operator_model_allow_message_statistics =
                    !self.config.operator_model_allow_message_statistics;
                self.sync_config_to_daemon();
            }
            "operator_model_allow_approval_learning" => {
                self.config.operator_model_allow_approval_learning =
                    !self.config.operator_model_allow_approval_learning;
                self.sync_config_to_daemon();
            }
            "operator_model_allow_attention_tracking" => {
                self.config.operator_model_allow_attention_tracking =
                    !self.config.operator_model_allow_attention_tracking;
                self.sync_config_to_daemon();
            }
            "operator_model_allow_implicit_feedback" => {
                self.config.operator_model_allow_implicit_feedback =
                    !self.config.operator_model_allow_implicit_feedback;
                self.sync_config_to_daemon();
            }
            "collaboration_enabled" => {
                self.config.collaboration_enabled = !self.config.collaboration_enabled;
                self.sync_config_to_daemon();
            }
            "compliance_sign_all_events" => {
                self.config.compliance_sign_all_events = !self.config.compliance_sign_all_events;
                self.sync_config_to_daemon();
            }
            "tool_synthesis_enabled" => {
                self.config.tool_synthesis_enabled = !self.config.tool_synthesis_enabled;
                self.sync_config_to_daemon();
            }
            "tool_synthesis_require_activation" => {
                self.config.tool_synthesis_require_activation =
                    !self.config.tool_synthesis_require_activation;
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
            // ── Features tab toggles ──
            "feat_tier_override" => {
                // Cycle tier on Space (same as Enter)
                self.activate_settings_field();
            }
            "feat_security_level" => {
                self.activate_settings_field();
            }
            "feat_check_stale_todos"
            | "feat_check_stuck_goals"
            | "feat_check_unreplied_messages"
            | "feat_check_repo_changes" => {
                let key = match field.as_str() {
                    "feat_check_stale_todos" => "check_stale_todos",
                    "feat_check_stuck_goals" => "check_stuck_goals",
                    "feat_check_unreplied_messages" => "check_unreplied_messages",
                    "feat_check_repo_changes" => "check_repo_changes",
                    _ => return,
                };
                let current = self
                    .config
                    .agent_config_raw
                    .as_ref()
                    .and_then(|r| r.get("heartbeat"))
                    .and_then(|h| h.get(key))
                    .and_then(|v| v.as_bool())
                    .unwrap_or(true);
                let next = !current;
                self.send_daemon_command(DaemonCommand::SetConfigItem {
                    key_path: format!("/heartbeat/{}", key),
                    value_json: next.to_string(),
                });
                if let Some(ref mut raw) = self.config.agent_config_raw {
                    if raw.get("heartbeat").is_none() {
                        raw["heartbeat"] = serde_json::json!({});
                    }
                    raw["heartbeat"][key] = serde_json::Value::Bool(next);
                }
            }
            "feat_consolidation_enabled" => {
                let current = self
                    .config
                    .agent_config_raw
                    .as_ref()
                    .and_then(|r| r.get("consolidation"))
                    .and_then(|c| c.get("enabled"))
                    .and_then(|v| v.as_bool())
                    .unwrap_or(true);
                let next = !current;
                self.send_daemon_command(DaemonCommand::SetConfigItem {
                    key_path: "/consolidation/enabled".to_string(),
                    value_json: next.to_string(),
                });
                if let Some(ref mut raw) = self.config.agent_config_raw {
                    if raw.get("consolidation").is_none() {
                        raw["consolidation"] = serde_json::json!({});
                    }
                    raw["consolidation"]["enabled"] = serde_json::Value::Bool(next);
                }
            }
            "feat_skill_discovery_enabled" => {
                let current = self
                    .config
                    .agent_config_raw
                    .as_ref()
                    .and_then(|r| r.get("skill_discovery"))
                    .and_then(|s| s.get("enabled"))
                    .and_then(|v| v.as_bool())
                    .unwrap_or(true);
                let next = !current;
                self.send_daemon_command(DaemonCommand::SetConfigItem {
                    key_path: "/skill_discovery/enabled".to_string(),
                    value_json: next.to_string(),
                });
                if let Some(ref mut raw) = self.config.agent_config_raw {
                    if raw.get("skill_discovery").is_none() {
                        raw["skill_discovery"] = serde_json::json!({});
                    }
                    raw["skill_discovery"]["enabled"] = serde_json::Value::Bool(next);
                }
            }
            "whatsapp_link_device" => {
                self.activate_settings_field();
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

    // ── Plugin settings handlers (Plan 16-03) ────────────────────────────────

    pub(super) fn handle_plugins_settings_key(&mut self, code: KeyCode) -> bool {
        if self.plugin_settings.list_mode {
            // List mode navigation
            match code {
                KeyCode::Down => {
                    let count = self.plugin_settings.plugins.len();
                    if count > 0 {
                        self.plugin_settings.selected_index =
                            (self.plugin_settings.selected_index + 1).min(count - 1);
                    }
                    return true;
                }
                KeyCode::Up => {
                    self.plugin_settings.selected_index =
                        self.plugin_settings.selected_index.saturating_sub(1);
                    return true;
                }
                KeyCode::Enter => {
                    // Switch to detail mode for selected plugin
                    if let Some(plugin) = self.plugin_settings.selected_plugin() {
                        let name = plugin.name.clone();
                        self.plugin_settings.list_mode = false;
                        self.plugin_settings.detail_cursor = 0;
                        self.plugin_settings.test_result = None;
                        self.plugin_settings.schema_fields.clear();
                        self.plugin_settings.settings_values.clear();
                        self.send_daemon_command(DaemonCommand::PluginGet(name.clone()));
                        self.send_daemon_command(DaemonCommand::PluginGetSettings(name));
                    }
                    return true;
                }
                KeyCode::Char(' ') => {
                    // Toggle enable/disable
                    if let Some(plugin) = self.plugin_settings.selected_plugin() {
                        let name = plugin.name.clone();
                        if plugin.enabled {
                            self.send_daemon_command(DaemonCommand::PluginDisable(name));
                        } else {
                            self.send_daemon_command(DaemonCommand::PluginEnable(name));
                        }
                    }
                    return true;
                }
                _ => {}
            }
        } else {
            // Detail mode navigation
            if self.settings.is_editing() {
                // Standard editing keys are handled by the base settings reducer
                match code {
                    KeyCode::Enter => {
                        // Confirm edit and save to daemon
                        let value = self.settings.edit_buffer().to_string();
                        if let Some(field_key) = self.settings.editing_field().map(str::to_string) {
                            // Extract plugin name and secret flag before mutating settings_values
                            let plugin_name = self
                                .plugin_settings
                                .selected_plugin()
                                .map(|p| p.name.clone());
                            let is_secret = self
                                .plugin_settings
                                .schema_fields
                                .iter()
                                .find(|f| f.key == field_key)
                                .map_or(false, |f| f.secret);
                            if let Some(pname) = plugin_name {
                                // Optimistic local update so UI reflects change immediately
                                if let Some(entry) = self
                                    .plugin_settings
                                    .settings_values
                                    .iter_mut()
                                    .find(|(k, _, _)| *k == field_key)
                                {
                                    entry.1 = value.clone();
                                } else {
                                    self.plugin_settings.settings_values.push((
                                        field_key.clone(),
                                        value.clone(),
                                        is_secret,
                                    ));
                                }
                                self.send_daemon_command(DaemonCommand::PluginUpdateSetting {
                                    plugin_name: pname,
                                    key: field_key,
                                    value,
                                    is_secret,
                                });
                            }
                        }
                        self.settings.reduce(SettingsAction::ConfirmEdit);
                        return true;
                    }
                    KeyCode::Esc => {
                        self.settings.reduce(SettingsAction::CancelEdit);
                        return true;
                    }
                    _ => return false, // Let base handler deal with InsertChar, Backspace, etc.
                }
            }

            match code {
                KeyCode::Down => {
                    let count = self.plugin_settings.detail_field_count();
                    if count > 0 {
                        self.plugin_settings.detail_cursor =
                            (self.plugin_settings.detail_cursor + 1).min(count - 1);
                    }
                    return true;
                }
                KeyCode::Up => {
                    self.plugin_settings.detail_cursor =
                        self.plugin_settings.detail_cursor.saturating_sub(1);
                    return true;
                }
                KeyCode::Enter => {
                    let cursor = self.plugin_settings.detail_cursor;
                    let field_count = self.plugin_settings.schema_fields.len();
                    if cursor < field_count {
                        // Edit a settings field
                        let field = &self.plugin_settings.schema_fields[cursor];
                        let key = field.key.clone();
                        let current_value = self
                            .plugin_settings
                            .value_for_key(&key)
                            .unwrap_or("")
                            .to_string();
                        if field.field_type == "boolean" {
                            // Toggle boolean fields directly
                            let new_val = if current_value == "true" {
                                "false"
                            } else {
                                "true"
                            };
                            if let Some(plugin) = self.plugin_settings.selected_plugin() {
                                self.send_daemon_command(DaemonCommand::PluginUpdateSetting {
                                    plugin_name: plugin.name.clone(),
                                    key,
                                    value: new_val.to_string(),
                                    is_secret: false,
                                });
                            }
                        } else {
                            // Start editing — clear buffer for secret fields so user
                            // doesn't accidentally save the masked "********" string
                            let edit_value = if field.secret { "" } else { &current_value };
                            self.settings.start_editing(&key, edit_value);
                        }
                    } else {
                        // Action button pressed
                        let action_offset = field_count;
                        let has_api = self
                            .plugin_settings
                            .selected_plugin()
                            .map_or(false, |p| p.has_api);
                        if has_api && cursor == action_offset {
                            // Test Connection
                            let name = self
                                .plugin_settings
                                .selected_plugin()
                                .map(|p| p.name.clone());
                            if let Some(name) = name {
                                self.plugin_settings.test_result = None;
                                self.send_daemon_command(DaemonCommand::PluginTestConnection(name));
                            }
                        }
                        // Connect / Reconnect button: trigger OAuth flow (Plan 18-03)
                        let has_auth = self
                            .plugin_settings
                            .selected_plugin()
                            .map_or(false, |p| p.has_auth);
                        let connect_offset = action_offset + if has_api { 1 } else { 0 };
                        if has_auth && cursor == connect_offset {
                            let name = self
                                .plugin_settings
                                .selected_plugin()
                                .map(|p| p.name.clone());
                            if let Some(name) = name {
                                self.send_daemon_command(DaemonCommand::PluginOAuthStart(name));
                                self.status_line = "Starting OAuth flow...".to_string();
                            }
                        }
                    }
                    return true;
                }
                KeyCode::Esc => {
                    // Return to list mode
                    self.plugin_settings.list_mode = true;
                    self.plugin_settings.detail_cursor = 0;
                    self.settings.reduce(SettingsAction::CancelEdit);
                    return true;
                }
                _ => {}
            }
        }
        false
    }

    pub(super) fn settings_field_click_uses_toggle(&self) -> bool {
        matches!(
            self.settings.current_field_name(),
            "managed_sandbox_enabled"
                | "managed_security_level"
                | "gateway_enabled"
                | "web_search_enabled"
                | "enable_streaming"
                | "auto_retry"
                | "enable_conversation_memory"
                | "enable_honcho_memory"
                | "anticipatory_enabled"
                | "anticipatory_morning_brief"
                | "anticipatory_predictive_hydration"
                | "anticipatory_stuck_detection"
                | "operator_model_enabled"
                | "operator_model_allow_message_statistics"
                | "operator_model_allow_approval_learning"
                | "operator_model_allow_attention_tracking"
                | "operator_model_allow_implicit_feedback"
                | "collaboration_enabled"
                | "compliance_sign_all_events"
                | "tool_synthesis_enabled"
                | "tool_synthesis_require_activation"
                | "auto_compact_context"
                | "snapshot_auto_cleanup"
                | "feat_tier_override"
                | "feat_security_level"
                | "feat_check_stale_todos"
                | "feat_check_stuck_goals"
                | "feat_check_unreplied_messages"
                | "feat_check_repo_changes"
                | "feat_consolidation_enabled"
                | "feat_skill_discovery_enabled"
                | "whatsapp_link_device"
        ) || self.settings.current_field_name().starts_with("tool_")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::mpsc::unbounded_channel;

    fn make_model() -> (
        TuiModel,
        tokio::sync::mpsc::UnboundedReceiver<DaemonCommand>,
    ) {
        let (_event_tx, event_rx) = std::sync::mpsc::channel();
        let (daemon_tx, daemon_rx) = unbounded_channel();
        (TuiModel::new(event_rx, daemon_tx), daemon_rx)
    }

    #[test]
    fn whatsapp_link_device_sends_subscribe_and_start_without_status_probe() {
        let (mut model, mut daemon_rx) = make_model();
        model
            .settings
            .reduce(SettingsAction::SwitchTab(SettingsTab::Gateway));
        model.settings.reduce(SettingsAction::NavigateField(12));
        assert_eq!(model.settings.current_field_name(), "whatsapp_link_device");

        model.activate_settings_field();

        assert!(matches!(
            daemon_rx.try_recv().expect("expected subscribe command"),
            DaemonCommand::WhatsAppLinkSubscribe
        ));
        assert!(matches!(
            daemon_rx.try_recv().expect("expected start command"),
            DaemonCommand::WhatsAppLinkStart
        ));
        assert!(daemon_rx.try_recv().is_err());
        assert_eq!(model.modal.top(), Some(modal::ModalKind::WhatsAppLink));
    }
}
