use amux_shared::providers::{PROVIDER_ID_CUSTOM, PROVIDER_ID_GITHUB_COPILOT, PROVIDER_ID_OPENAI};

impl TuiModel {
    pub(super) fn audio_catalog_models(
        endpoint: &str,
        provider_id: &str,
    ) -> Vec<crate::state::config::FetchedModel> {
        let model = |id: &str, name: &str, context_window: Option<u32>| crate::state::config::FetchedModel {
            id: id.to_string(),
            name: Some(name.to_string()),
            context_window,
            pricing: None,
            metadata: None,
        };
        match (provider_id, endpoint) {
            ("openai" | "azure-openai", "stt") => vec![
                model("gpt-4o-transcribe", "GPT-4o Transcribe", Some(128_000)),
                model("gpt-4o-mini-transcribe", "GPT-4o Mini Transcribe", Some(128_000)),
                model("whisper-1", "Whisper 1", None),
            ],
            ("openai" | "azure-openai", "tts") => vec![
                model("gpt-4o-mini-tts", "GPT-4o Mini TTS", Some(128_000)),
                model("tts-1", "TTS 1", None),
                model("tts-1-hd", "TTS 1 HD", None),
            ],
            _ => Vec::new(),
        }
    }

    pub(super) fn default_audio_model_for(endpoint: &str, provider_id: &str) -> String {
        Self::audio_catalog_models(endpoint, provider_id)
            .into_iter()
            .next()
            .map(|model| model.id)
            .unwrap_or_else(|| match endpoint {
                "stt" => "whisper-1".to_string(),
                "tts" => "gpt-4o-mini-tts".to_string(),
                _ => String::new(),
            })
    }

    pub(super) fn set_audio_config_string(&mut self, endpoint: &str, field: &str, value: String) {
        self.send_daemon_command(DaemonCommand::SetConfigItem {
            key_path: format!("/audio/{endpoint}/{field}"),
            value_json: serde_json::Value::String(value.clone()).to_string(),
        });
        if let Some(ref mut raw) = self.config.agent_config_raw {
            if raw.get("audio").is_none() {
                raw["audio"] = serde_json::json!({});
            }
            if raw["audio"].get(endpoint).is_none() {
                raw["audio"][endpoint] = serde_json::json!({});
            }
            raw["audio"][endpoint][field] = serde_json::Value::String(value);
        }
    }

    pub(super) fn open_audio_model_picker(&mut self, endpoint: &str) {
        let provider_id = match endpoint {
            "stt" => {
                let provider = self.config.audio_stt_provider();
                if provider.trim().is_empty() {
                    "openai".to_string()
                } else {
                    provider
                }
            }
            "tts" => {
                let provider = self.config.audio_tts_provider();
                if provider.trim().is_empty() {
                    "openai".to_string()
                } else {
                    provider
                }
            }
            _ => return,
        };
        let (base_url, api_key, auth_source) = self.provider_auth_snapshot(&provider_id);
        let models = Self::audio_catalog_models(endpoint, &provider_id);
        self.config
            .reduce(config::ConfigAction::ModelsFetched(models));
        if self.should_fetch_remote_models(&provider_id, &auth_source) {
            self.send_daemon_command(DaemonCommand::FetchModels {
                provider_id,
                base_url,
                api_key,
            });
        }
        self.settings_picker_target = Some(match endpoint {
            "stt" => SettingsPickerTarget::AudioSttModel,
            "tts" => SettingsPickerTarget::AudioTtsModel,
            _ => return,
        });
        self.modal
            .reduce(modal::ModalAction::Push(modal::ModalKind::ModelPicker));
        self.sync_model_picker_item_count();
    }

    fn json_string_has_audio(value: Option<&serde_json::Value>) -> bool {
        value
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| value.to_ascii_lowercase().contains("audio"))
            .unwrap_or(false)
    }

    fn json_array_contains_audio(value: Option<&serde_json::Value>) -> bool {
        value
            .and_then(|value| value.as_array())
            .map(|items| {
                items.iter().any(|item| {
                    item.as_str()
                        .map(str::trim)
                        .map(|value| value.eq_ignore_ascii_case("audio"))
                        .unwrap_or(false)
                })
            })
            .unwrap_or(false)
    }

    fn fetched_model_supports_audio_endpoint(
        model: &crate::state::config::FetchedModel,
        endpoint: &str,
    ) -> bool {
        if model
            .pricing
            .as_ref()
            .and_then(|pricing| pricing.audio.as_deref())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .is_some()
        {
            return true;
        }

        let metadata = model.metadata.as_ref();
        let input_audio = Self::json_array_contains_audio(
            metadata
                .and_then(|value| value.pointer("/architecture/input_modalities"))
                .or_else(|| metadata.and_then(|value| value.pointer("/input_modalities")))
                .or_else(|| metadata.and_then(|value| value.pointer("/modalities"))),
        );
        let output_audio = Self::json_array_contains_audio(
            metadata
                .and_then(|value| value.pointer("/architecture/output_modalities"))
                .or_else(|| metadata.and_then(|value| value.pointer("/output_modalities")))
                .or_else(|| metadata.and_then(|value| value.pointer("/modalities"))),
        );
        let modality_audio = Self::json_string_has_audio(
            metadata
                .and_then(|value| value.pointer("/architecture/modality"))
                .or_else(|| metadata.and_then(|value| value.pointer("/modality"))),
        );

        match endpoint {
            "stt" => input_audio || modality_audio,
            "tts" => output_audio || modality_audio,
            _ => false,
        }
    }

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

    pub(super) fn open_honcho_editor(&mut self) {
        let editor = crate::state::config::HonchoEditorState::from_config(&self.config);
        self.config.honcho_editor = Some(editor);
        self.settings.reduce(SettingsAction::CancelEdit);
        self.status_line = "Editing Honcho memory settings".to_string();
    }

    pub(super) fn close_honcho_editor(&mut self) {
        self.config.honcho_editor = None;
        self.settings.reduce(SettingsAction::CancelEdit);
        self.status_line = "Closed Honcho memory editor".to_string();
    }

    pub(super) fn commit_honcho_editor(&mut self) {
        let Some(editor) = self.config.honcho_editor.take() else {
            return;
        };
        self.config.enable_honcho_memory = editor.enabled;
        self.config.honcho_api_key = editor.api_key;
        self.config.honcho_base_url = editor.base_url;
        self.config.honcho_workspace_id = if editor.workspace_id.trim().is_empty() {
            "tamux".to_string()
        } else {
            editor.workspace_id
        };
        self.settings.reduce(SettingsAction::CancelEdit);
        self.sync_config_to_daemon();
        self.status_line = "Updated Honcho memory settings".to_string();
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

    pub(super) fn normalize_compaction_custom_transport(&mut self) {
        self.config.compaction_custom_api_transport = self.provider_transport_snapshot(
            &self.config.compaction_custom_provider,
            &self.config.compaction_custom_auth_source,
            &self.config.compaction_custom_model,
            &self.config.compaction_custom_api_transport,
        );
    }

    pub(super) fn open_provider_picker(&mut self, target: SettingsPickerTarget) {
        self.settings_picker_target = Some(target);
        self.modal
            .reduce(modal::ModalAction::Push(modal::ModalKind::ProviderPicker));
        self.modal.set_picker_item_count(
            widgets::provider_picker::available_provider_defs(&self.auth).len(),
        );
    }

    pub(super) fn open_compaction_weles_model_picker(&mut self) {
        let provider_id = self.config.compaction_weles_provider.clone();
        let (_, api_key, auth_source) = self.provider_auth_snapshot(&provider_id);
        let models = providers::known_models_for_provider_auth(&provider_id, &auth_source);
        self.config
            .reduce(config::ConfigAction::ModelsFetched(models));
        if self.should_fetch_remote_models(&provider_id, &auth_source) {
            let base_url = providers::find_by_id(&provider_id)
                .map(|provider| provider.default_base_url.to_string())
                .unwrap_or_default();
            self.send_daemon_command(DaemonCommand::FetchModels {
                provider_id,
                base_url,
                api_key,
            });
        }
        self.settings_picker_target = Some(SettingsPickerTarget::CompactionWelesModel);
        self.modal
            .reduce(modal::ModalAction::Push(modal::ModalKind::ModelPicker));
        self.sync_model_picker_item_count();
    }

    pub(super) fn open_compaction_custom_model_picker(&mut self) {
        let provider_id = self.config.compaction_custom_provider.clone();
        let models = providers::known_models_for_provider_auth(
            &provider_id,
            &self.config.compaction_custom_auth_source,
        );
        self.config
            .reduce(config::ConfigAction::ModelsFetched(models));
        if self.should_fetch_remote_models(&provider_id, &self.config.compaction_custom_auth_source)
        {
            self.send_daemon_command(DaemonCommand::FetchModels {
                provider_id,
                base_url: self.config.compaction_custom_base_url.clone(),
                api_key: self.config.compaction_custom_api_key.clone(),
            });
        }
        self.settings_picker_target = Some(SettingsPickerTarget::CompactionCustomModel);
        self.modal
            .reduce(modal::ModalAction::Push(modal::ModalKind::ModelPicker));
        self.sync_model_picker_item_count();
    }

    pub(super) fn model_picker_current_selection(&self) -> (String, Option<String>) {
        match self
            .settings_picker_target
            .unwrap_or(SettingsPickerTarget::Model)
        {
            SettingsPickerTarget::AudioSttModel => (self.config.audio_stt_model(), None),
            SettingsPickerTarget::AudioTtsModel => (self.config.audio_tts_model(), None),
            SettingsPickerTarget::CompactionWelesModel => {
                (self.config.compaction_weles_model.clone(), None)
            }
            SettingsPickerTarget::CompactionCustomModel => {
                (self.config.compaction_custom_model.clone(), None)
            }
            SettingsPickerTarget::SubAgentModel => self
                .subagents
                .editor
                .as_ref()
                .map(|editor| (editor.model.clone(), None))
                .unwrap_or_else(|| (String::new(), None)),
            SettingsPickerTarget::ConciergeModel => {
                (self.concierge.model.clone().unwrap_or_default(), None)
            }
            _ => (
                self.config.model.clone(),
                Some(self.config.custom_model_name.clone()),
            ),
        }
    }

    pub(super) fn available_model_picker_models(&self) -> Vec<crate::state::config::FetchedModel> {
        let (current_model, custom_model_name) = self.model_picker_current_selection();
        match self
            .settings_picker_target
            .unwrap_or(SettingsPickerTarget::Model)
        {
            SettingsPickerTarget::AudioSttModel | SettingsPickerTarget::AudioTtsModel => {
                let (endpoint, provider_id) = match self
                    .settings_picker_target
                    .unwrap_or(SettingsPickerTarget::Model)
                {
                    SettingsPickerTarget::AudioSttModel => ("stt", self.config.audio_stt_provider()),
                    SettingsPickerTarget::AudioTtsModel => ("tts", self.config.audio_tts_provider()),
                    _ => unreachable!(),
                };
                let mut models = Self::audio_catalog_models(endpoint, &provider_id);
                for model in self.config.fetched_models() {
                    if Self::fetched_model_supports_audio_endpoint(model, endpoint)
                        && !models.iter().any(|existing| existing.id == model.id)
                    {
                        models.push(model.clone());
                    }
                }
                widgets::model_picker::merge_models_for_selection(
                    &models,
                    &current_model,
                    custom_model_name.as_deref(),
                )
            }
            _ => widgets::model_picker::available_models_for(
                &self.config,
                &current_model,
                custom_model_name.as_deref(),
            ),
        }
    }

    pub(super) fn sync_model_picker_item_count(&mut self) {
        let count = self.available_model_picker_models().len() + 1;
        self.modal.set_picker_item_count(count);
    }

    pub(super) fn begin_targeted_custom_model_edit(
        &mut self,
        target: Option<SettingsPickerTarget>,
    ) {
        match target.unwrap_or(SettingsPickerTarget::Model) {
            SettingsPickerTarget::AudioSttModel => {
                if self.modal.top() != Some(modal::ModalKind::Settings) {
                    self.modal
                        .reduce(modal::ModalAction::Push(modal::ModalKind::Settings));
                }
                self.settings
                    .reduce(SettingsAction::SwitchTab(SettingsTab::Features));
                self.settings_navigate_to(18);
                self.settings.start_editing(
                    "feat_audio_stt_model",
                    &self.config.audio_stt_model(),
                );
                self.status_line = "Enter STT model ID".to_string();
            }
            SettingsPickerTarget::AudioTtsModel => {
                if self.modal.top() != Some(modal::ModalKind::Settings) {
                    self.modal
                        .reduce(modal::ModalAction::Push(modal::ModalKind::Settings));
                }
                self.settings
                    .reduce(SettingsAction::SwitchTab(SettingsTab::Features));
                self.settings_navigate_to(21);
                self.settings.start_editing(
                    "feat_audio_tts_model",
                    &self.config.audio_tts_model(),
                );
                self.status_line = "Enter TTS model ID".to_string();
            }
            SettingsPickerTarget::BuiltinPersonaModel => {
                self.status_line =
                    "Custom model entry is not available for builtin persona setup".to_string();
            }
            SettingsPickerTarget::CompactionWelesModel => self.settings.start_editing(
                "compaction_weles_model",
                &self.config.compaction_weles_model,
            ),
            SettingsPickerTarget::CompactionCustomModel => self.settings.start_editing(
                "compaction_custom_model",
                &self.config.compaction_custom_model,
            ),
            SettingsPickerTarget::SubAgentModel => {
                let Some(editor) = self.subagents.editor.as_ref() else {
                    self.status_line = "No sub-agent editor is active".to_string();
                    return;
                };
                if self.modal.top() != Some(modal::ModalKind::Settings) {
                    self.modal
                        .reduce(modal::ModalAction::Push(modal::ModalKind::Settings));
                }
                self.settings
                    .reduce(SettingsAction::SwitchTab(SettingsTab::SubAgents));
                self.settings.start_editing("subagent_model", &editor.model);
                self.status_line = "Enter sub-agent model ID".to_string();
            }
            SettingsPickerTarget::ConciergeModel => {
                if self.modal.top() != Some(modal::ModalKind::Settings) {
                    self.modal
                        .reduce(modal::ModalAction::Push(modal::ModalKind::Settings));
                }
                self.settings
                    .reduce(SettingsAction::SwitchTab(SettingsTab::Concierge));
                self.settings.start_editing(
                    "concierge_model",
                    self.concierge.model.as_deref().unwrap_or(""),
                );
                self.status_line = "Enter Rarog model ID".to_string();
            }
            _ => self.begin_custom_model_edit(),
        }
    }

    fn whatsapp_linking_allowed(&self) -> bool {
        amux_protocol::has_whatsapp_allowed_contacts(&self.config.whatsapp_allowed_contacts)
    }

    pub(super) fn provider_auth_snapshot(&self, provider_id: &str) -> (String, String, String) {
        let mut base_url = providers::find_by_id(provider_id)
            .map(|def| def.default_base_url.to_string())
            .unwrap_or_default();
        let mut api_key = String::new();
        let mut auth_source =
            if provider_id == PROVIDER_ID_OPENAI && self.config.chatgpt_auth_available {
                "chatgpt_subscription".to_string()
            } else {
                providers::default_auth_source_for(provider_id).to_string()
            };

        if self.config.provider == provider_id {
            return (
                self.config.base_url.clone(),
                self.config.api_key.clone(),
                self.config.auth_source.clone(),
            );
        }

        if let Some(provider_config) = self.saved_provider_config(provider_id) {
            if let Some(value) =
                TuiModel::provider_field_str(provider_config, "base_url", "base_url")
            {
                if !value.is_empty() {
                    base_url = value.to_string();
                }
            }
            if let Some(value) = TuiModel::provider_field_str(provider_config, "api_key", "api_key")
            {
                api_key = value.to_string();
            }
            if let Some(value) =
                TuiModel::provider_field_str(provider_config, "auth_source", "auth_source")
            {
                auth_source = value.to_string();
            }
        }

        (base_url, api_key, auth_source)
    }

    pub(super) fn provider_transport_snapshot(
        &self,
        provider_id: &str,
        auth_source: &str,
        model: &str,
        fallback_transport: &str,
    ) -> String {
        let mut api_transport = fallback_transport.to_string();

        if self.config.provider == provider_id {
            api_transport = self.config.api_transport.clone();
        } else if let Some(provider_config) = self.saved_provider_config(provider_id) {
            if let Some(value) =
                TuiModel::provider_field_str(provider_config, "api_transport", "api_transport")
            {
                api_transport = value.to_string();
            }
        }

        self.normalize_provider_transport_for_state(provider_id, auth_source, model, &api_transport)
    }

    fn normalize_provider_transport_for_state(
        &self,
        provider_id: &str,
        auth_source: &str,
        model: &str,
        api_transport: &str,
    ) -> String {
        if provider_id == PROVIDER_ID_OPENAI && auth_source == "chatgpt_subscription" {
            return "responses".to_string();
        }
        if providers::uses_fixed_anthropic_messages(provider_id, model) {
            return "chat_completions".to_string();
        }
        if providers::supported_transports_for(provider_id).contains(&api_transport) {
            api_transport.to_string()
        } else {
            providers::default_transport_for(provider_id).to_string()
        }
    }

    fn saved_provider_config(&self, provider_id: &str) -> Option<&serde_json::Value> {
        self.config.agent_config_raw.as_ref().and_then(|raw| {
            raw.get("providers")
                .and_then(|providers| providers.get(provider_id))
                .or_else(|| raw.get(provider_id))
        })
    }

    pub(super) fn should_fetch_remote_models(&self, provider_id: &str, auth_source: &str) -> bool {
        providers::supports_model_fetch_for(provider_id)
            && !(provider_id == PROVIDER_ID_OPENAI && auth_source == "chatgpt_subscription")
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
        self.config.auth_source =
            if def.id == PROVIDER_ID_OPENAI && self.config.chatgpt_auth_available {
                "chatgpt_subscription".to_string()
            } else {
                providers::default_auth_source_for(def.id).to_string()
            };
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
                if providers::provider_uses_configurable_base_url(def.id) {
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
