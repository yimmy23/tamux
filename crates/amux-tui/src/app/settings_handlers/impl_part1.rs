use amux_shared::providers::{
    PROVIDER_ID_CUSTOM, PROVIDER_ID_GITHUB_COPILOT, PROVIDER_ID_OPENAI,
};

impl TuiModel {
    pub(super) fn current_settings_field_name(&self) -> &str {
        self.settings.current_field_name_with_config(&self.config)
    }

    pub(super) fn settings_field_count(&self) -> usize {
        self.settings.field_count_with_config(&self.config)
    }

    pub(super) fn clamp_settings_cursor(&mut self) {
        self.settings
            .clamp_field_cursor(self.settings.field_count_with_config(&self.config));
    }

    pub(super) fn cycle_compaction_strategy(&mut self) {
        self.config.compaction_strategy = match self.config.compaction_strategy.as_str() {
            "heuristic" => "weles".to_string(),
            "weles" => "custom_model".to_string(),
            _ => "heuristic".to_string(),
        };
        self.clamp_settings_cursor();
        self.sync_config_to_daemon();
    }

    pub(super) fn cycle_provider_id(current: &str) -> String {
        let providers = providers::PROVIDERS;
        let current_idx = providers
            .iter()
            .position(|provider| provider.id == current)
            .unwrap_or(0);
        providers[(current_idx + 1) % providers.len()].id.to_string()
    }

    pub(super) fn apply_compaction_custom_provider(&mut self, provider_id: &str) {
        self.config.compaction_custom_provider = provider_id.to_string();
        self.config.compaction_custom_base_url = providers::find_by_id(provider_id)
            .map(|provider| provider.default_base_url.to_string())
            .unwrap_or_default();
        self.config.compaction_custom_auth_source =
            providers::default_auth_source_for(provider_id).to_string();
        self.config.compaction_custom_api_transport =
            providers::default_transport_for(provider_id).to_string();
        self.config.compaction_custom_model =
            providers::default_model_for_provider_auth(provider_id, "api_key");
        self.config.compaction_custom_context_window_tokens = if provider_id == PROVIDER_ID_CUSTOM {
            128_000
        } else {
            providers::known_context_window_for(provider_id, &self.config.compaction_custom_model)
                .unwrap_or(128_000)
        };
        self.config.compaction_custom_api_key.clear();
        self.config.compaction_custom_assistant_id.clear();
    }

    fn whatsapp_linking_allowed(&self) -> bool {
        amux_protocol::has_whatsapp_allowed_contacts(&self.config.whatsapp_allowed_contacts)
    }

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
        self.config.custom_context_window_tokens = if def.id == PROVIDER_ID_CUSTOM {
            Some(128_000)
        } else {
            None
        };
        self.config.context_window_tokens = if def.id == PROVIDER_ID_CUSTOM {
            self.config.custom_context_window_tokens.unwrap_or(128_000)
        } else {
            providers::known_context_window_for(def.id, def.default_model).unwrap_or(128_000)
        };

        if let Some(raw) = &self.config.agent_config_raw {
            if let Some(provider_config) = raw
                .get("providers")
                .and_then(|providers| providers.get(def.id))
                .or_else(|| raw.get(def.id))
            {
                // For predefined providers, always use the canonical base_url
                // from the definition — stale DB values must not override it.
                if def.id == PROVIDER_ID_CUSTOM {
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

        if def.id == PROVIDER_ID_OPENAI && self.config.auth_source == "chatgpt_subscription" {
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
        editor.builtin = entry.builtin;
        editor.immutable_identity = entry.immutable_identity;
        editor.disable_allowed = entry.disable_allowed;
        editor.delete_allowed = entry.delete_allowed;
        editor.protected_reason = entry.protected_reason.clone();
        editor.reasoning_effort = entry.reasoning_effort.clone();
        editor.raw_json = Some(raw);
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
}
