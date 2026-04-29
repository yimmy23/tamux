use zorai_shared::providers::{
    AudioToolKind, PROVIDER_ID_AZURE_OPENAI, PROVIDER_ID_CUSTOM, PROVIDER_ID_GITHUB_COPILOT,
    PROVIDER_ID_GROQ, PROVIDER_ID_MINIMAX, PROVIDER_ID_MINIMAX_CODING_PLAN,
    PROVIDER_ID_OPENAI, PROVIDER_ID_OPENROUTER, PROVIDER_ID_XAI,
    PROVIDER_ID_XIAOMI_MIMO_TOKEN_PLAN,
};

impl TuiModel {
    fn image_remote_model_fetch_output_modalities(provider_id: &str) -> Option<String> {
        if provider_id == PROVIDER_ID_OPENROUTER {
            Some("image".to_string())
        } else {
            None
        }
    }

    fn audio_remote_model_fetch_output_modalities(
        endpoint: &str,
        provider_id: &str,
    ) -> Option<String> {
        if provider_id != PROVIDER_ID_OPENROUTER {
            return None;
        }

        match endpoint {
            "tts" => Some("audio".to_string()),
            _ => None,
        }
    }

    pub(super) fn audio_catalog_models(
        endpoint: &str,
        provider_id: &str,
    ) -> Vec<crate::state::config::FetchedModel> {
        let model = |id: &str, name: &str, context_window: Option<u32>| {
            crate::state::config::FetchedModel {
                id: id.to_string(),
                name: Some(name.to_string()),
                context_window,
                pricing: None,
                metadata: None,
            }
        };
        match (provider_id, endpoint) {
            (PROVIDER_ID_OPENAI | PROVIDER_ID_AZURE_OPENAI, "stt") => vec![
                model("gpt-4o-transcribe", "GPT-4o Transcribe", Some(128_000)),
                model(
                    "gpt-4o-mini-transcribe",
                    "GPT-4o Mini Transcribe",
                    Some(128_000),
                ),
                model(
                    "gpt-4o-transcribe-diarize",
                    "GPT-4o Transcribe Diarize",
                    Some(16_000),
                ),
                model("whisper-1", "Whisper 1", None),
            ],
            (PROVIDER_ID_GROQ, "stt") => vec![
                model("whisper-large-v3-turbo", "Whisper Large V3 Turbo", None),
                model("whisper-large-v3", "Whisper Large V3", None),
            ],
            (PROVIDER_ID_GROQ, "tts") => vec![
                model(
                    "canopylabs/orpheus-v1-english",
                    "CanopyLabs Orpheus V1 English",
                    None,
                ),
                model(
                    "canopylabs/orpheus-arabic-saudi",
                    "CanopyLabs Orpheus Arabic Saudi",
                    None,
                ),
            ],
            (PROVIDER_ID_OPENAI | PROVIDER_ID_AZURE_OPENAI, "tts") => vec![
                model("gpt-4o-mini-tts", "GPT-4o Mini TTS", Some(128_000)),
                model("tts-1", "TTS 1", None),
                model("tts-1-hd", "TTS 1 HD", None),
            ],
            (PROVIDER_ID_XAI, "stt" | "tts") => {
                vec![model("grok-4", "Grok 4", Some(262_144))]
            }
            (PROVIDER_ID_XIAOMI_MIMO_TOKEN_PLAN, "tts") => vec![
                model("mimo-v2.5-tts", "MiMo V2.5 TTS", Some(128_000)),
                model(
                    "mimo-v2.5-tts-voiceclone",
                    "MiMo V2.5 TTS VoiceClone",
                    Some(128_000),
                ),
                model(
                    "mimo-v2.5-tts-voicedesign",
                    "MiMo V2.5 TTS VoiceDesign",
                    Some(128_000),
                ),
            ],
            (PROVIDER_ID_MINIMAX | PROVIDER_ID_MINIMAX_CODING_PLAN, "tts") => vec![
                model("speech-2.8-hd", "MiniMax Speech 2.8 HD", None),
                model("speech-2.8-turbo", "MiniMax Speech 2.8 Turbo", None),
                model("speech-2.6-hd", "MiniMax Speech 2.6 HD", None),
                model("speech-2.6-turbo", "MiniMax Speech 2.6 Turbo", None),
                model("speech-02-hd", "MiniMax Speech 02 HD", None),
                model("speech-02-turbo", "MiniMax Speech 02 Turbo", None),
                model("speech-01-hd", "MiniMax Speech 01 HD", None),
                model("speech-01-turbo", "MiniMax Speech 01 Turbo", None),
            ],
            _ => Vec::new(),
        }
    }

    pub(super) fn default_audio_model_for(endpoint: &str, provider_id: &str) -> String {
        Self::audio_catalog_models(endpoint, provider_id)
            .into_iter()
            .next()
            .map(|model| model.id)
            .unwrap_or_default()
    }

    pub(super) fn image_generation_catalog_models(
        provider_id: &str,
    ) -> Vec<crate::state::config::FetchedModel> {
        let model = |id: &str, name: &str, context_window: Option<u32>| {
            crate::state::config::FetchedModel {
                id: id.to_string(),
                name: Some(name.to_string()),
                context_window,
                pricing: None,
                metadata: None,
            }
        };
        match provider_id {
            PROVIDER_ID_OPENAI | PROVIDER_ID_AZURE_OPENAI | PROVIDER_ID_CUSTOM => {
                vec![
                    model("gpt-image-1", "GPT Image 1", None),
                    model("gpt-image-2", "GPT Image 2", None),
                ]
            }
            PROVIDER_ID_OPENROUTER => {
                vec![
                    model("openai/gpt-image-1", "OpenAI GPT Image 1", None),
                    model("openai/gpt-image-2", "OpenAI GPT Image 2", None),
                ]
            }
            PROVIDER_ID_MINIMAX | PROVIDER_ID_MINIMAX_CODING_PLAN => {
                vec![model("image-01", "MiniMax Image 01", None)]
            }
            _ => Vec::new(),
        }
    }

    pub(super) fn default_image_generation_model_for(provider_id: &str) -> String {
        Self::image_generation_catalog_models(provider_id)
            .into_iter()
            .next()
            .map(|model| model.id)
            .unwrap_or_default()
    }

    pub(super) fn embedding_catalog_models(
        provider_id: &str,
    ) -> Vec<crate::state::config::FetchedModel> {
        let model = |id: &str, name: &str, context_window: Option<u32>| {
            crate::state::config::FetchedModel {
                id: id.to_string(),
                name: Some(name.to_string()),
                context_window,
                pricing: None,
                metadata: None,
            }
        };
        match provider_id {
            PROVIDER_ID_OPENAI | PROVIDER_ID_AZURE_OPENAI | PROVIDER_ID_CUSTOM => {
                vec![
                    model("text-embedding-3-small", "Text Embedding 3 Small", Some(8192)),
                    model("text-embedding-3-large", "Text Embedding 3 Large", Some(8192)),
                ]
            }
            _ => Vec::new(),
        }
    }

    pub(super) fn default_embedding_model_for(provider_id: &str) -> String {
        Self::embedding_catalog_models(provider_id)
            .into_iter()
            .next()
            .map(|model| model.id)
            .unwrap_or_default()
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

    pub(super) fn set_image_generation_config_string(&mut self, field: &str, value: String) {
        self.send_daemon_command(DaemonCommand::SetConfigItem {
            key_path: format!("/image/generation/{field}"),
            value_json: serde_json::Value::String(value.clone()).to_string(),
        });
        if let Some(ref mut raw) = self.config.agent_config_raw {
            if raw.get("image").is_none() {
                raw["image"] = serde_json::json!({});
            }
            if raw["image"].get("generation").is_none() {
                raw["image"]["generation"] = serde_json::json!({});
            }
            raw["image"]["generation"][field] = serde_json::Value::String(value);
        }
    }

    pub(super) fn set_embedding_config_string(&mut self, field: &str, value: String) {
        self.send_daemon_command(DaemonCommand::SetConfigItem {
            key_path: format!("/semantic/embedding/{field}"),
            value_json: serde_json::Value::String(value.clone()).to_string(),
        });
        if let Some(ref mut raw) = self.config.agent_config_raw {
            if raw.get("semantic").is_none() {
                raw["semantic"] = serde_json::json!({});
            }
            if raw["semantic"].get("embedding").is_none() {
                raw["semantic"]["embedding"] = serde_json::json!({});
            }
            raw["semantic"]["embedding"][field] = serde_json::Value::String(value);
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
            let output_modalities =
                Self::audio_remote_model_fetch_output_modalities(endpoint, &provider_id);
            self.send_daemon_command(DaemonCommand::FetchModels {
                provider_id,
                base_url,
                api_key,
                output_modalities,
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

    pub(super) fn open_image_generation_model_picker(&mut self) {
        let provider_id = {
            let provider = self.config.image_generation_provider();
            if provider.trim().is_empty() {
                "openai".to_string()
            } else {
                provider
            }
        };
        let (base_url, api_key, auth_source) = self.provider_auth_snapshot(&provider_id);
        let models = Self::image_generation_catalog_models(&provider_id);
        self.config
            .reduce(config::ConfigAction::ModelsFetched(models));
        if self.should_fetch_remote_models(&provider_id, &auth_source) {
            let output_modalities =
                Self::image_remote_model_fetch_output_modalities(&provider_id);
            self.send_daemon_command(DaemonCommand::FetchModels {
                provider_id,
                base_url,
                api_key,
                output_modalities,
            });
        }
        self.settings_picker_target = Some(SettingsPickerTarget::ImageGenerationModel);
        self.modal
            .reduce(modal::ModalAction::Push(modal::ModalKind::ModelPicker));
        self.sync_model_picker_item_count();
    }

    pub(super) fn open_embedding_model_picker(&mut self) {
        let provider_id = {
            let provider = self.config.semantic_embedding_provider();
            if provider.trim().is_empty() {
                "openai".to_string()
            } else {
                provider
            }
        };
        let (base_url, api_key, auth_source) = self.provider_auth_snapshot(&provider_id);
        let models = Self::embedding_catalog_models(&provider_id);
        self.config
            .reduce(config::ConfigAction::ModelsFetched(models));
        if self.should_fetch_remote_models(&provider_id, &auth_source) {
            self.send_daemon_command(DaemonCommand::FetchModels {
                provider_id,
                base_url,
                api_key,
                output_modalities: Some("embedding".to_string()),
            });
        }
        self.settings_picker_target = Some(SettingsPickerTarget::EmbeddingModel);
        self.modal
            .reduce(modal::ModalAction::Push(modal::ModalKind::ModelPicker));
        self.sync_model_picker_item_count();
    }

    pub(super) fn open_provider_backed_model_picker(
        &mut self,
        target: SettingsPickerTarget,
        provider_id: String,
        base_url: String,
        api_key: String,
        auth_source: String,
    ) {
        let models = providers::known_models_for_provider_auth(&provider_id, &auth_source);
        self.config.reduce(config::ConfigAction::ModelsFetched(models));
        if self.should_fetch_remote_models(&provider_id, &auth_source) {
            self.send_daemon_command(DaemonCommand::FetchModels {
                provider_id,
                base_url,
                api_key,
                output_modalities: None,
            });
        }
        self.settings_picker_target = Some(target);
        self.modal
            .reduce(modal::ModalAction::Push(modal::ModalKind::ModelPicker));
        self.sync_model_picker_item_count();
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

    fn modality_side_has_audio(modality: &str, side: &str) -> bool {
        let trimmed = modality.trim().to_ascii_lowercase();
        if trimmed.is_empty() {
            return false;
        }

        let Some((input, output)) = trimmed.split_once("->") else {
            return false;
        };
        let directional = match side {
            "input" => input,
            "output" => output,
            _ => return false,
        };

        directional
            .split(|ch: char| matches!(ch, '+' | ',' | '|' | '/' | ' '))
            .any(|token| token.trim() == "audio")
    }

    fn json_string_has_directional_audio(
        value: Option<&serde_json::Value>,
        side: &str,
    ) -> bool {
        value
            .and_then(|value| value.as_str())
            .map(|value| Self::modality_side_has_audio(value, side))
            .unwrap_or(false)
    }

    fn fetched_model_audio_direction_override(
        model: &crate::state::config::FetchedModel,
        endpoint: &str,
    ) -> Option<bool> {
        let provider_prefix_sensitive =
            model.id.starts_with("xai/") || model.id.starts_with("openai/")
                || model.id.starts_with(&format!("{PROVIDER_ID_XAI}/"))
                || model.id.starts_with(&format!("{PROVIDER_ID_OPENROUTER}/"));
        let name = model
            .name
            .as_deref()
            .map(str::to_ascii_lowercase)
            .unwrap_or_default();
        let id = model.id.to_ascii_lowercase();
        let haystack = format!("{id} {name}");

        let looks_like_stt = haystack.contains("transcribe")
            || haystack.contains("transcription")
            || haystack.contains("speech-to-text")
            || haystack.contains("speech to text")
            || haystack.contains("whisper")
            || (provider_prefix_sensitive && haystack.contains("listen"));
        let looks_like_tts = haystack.contains("text-to-speech")
            || haystack.contains("text to speech")
            || haystack.contains("-tts")
            || haystack.contains(" tts")
            || (provider_prefix_sensitive && haystack.contains("speak"));

        match endpoint {
            "stt" if looks_like_stt && !looks_like_tts => Some(true),
            "stt" if looks_like_tts && !looks_like_stt => Some(false),
            "tts" if looks_like_tts && !looks_like_stt => Some(true),
            "tts" if looks_like_stt && !looks_like_tts => Some(false),
            _ => None,
        }
    }

    fn fetched_model_supports_audio_endpoint(
        model: &crate::state::config::FetchedModel,
        endpoint: &str,
    ) -> bool {
        let metadata = model.metadata.as_ref();
        let input_audio = Self::json_array_contains_audio(
            metadata
                .and_then(|value| value.pointer("/architecture/input_modalities"))
                .or_else(|| metadata.and_then(|value| value.pointer("/input_modalities"))),
        );
        let output_audio = Self::json_array_contains_audio(
            metadata
                .and_then(|value| value.pointer("/architecture/output_modalities"))
                .or_else(|| metadata.and_then(|value| value.pointer("/output_modalities"))),
        );
        let modality_input_audio = Self::json_string_has_directional_audio(
            metadata
                .and_then(|value| value.pointer("/architecture/modality"))
                .or_else(|| metadata.and_then(|value| value.pointer("/modality"))),
            "input",
        );
        let modality_output_audio = Self::json_string_has_directional_audio(
            metadata
                .and_then(|value| value.pointer("/architecture/modality"))
                .or_else(|| metadata.and_then(|value| value.pointer("/modality"))),
            "output",
        );

        let directional_match = match endpoint {
            "stt" => input_audio || modality_input_audio,
            "tts" => output_audio || modality_output_audio,
            _ => false,
        };
        if directional_match {
            return true;
        }

        if let Some(override_result) = Self::fetched_model_audio_direction_override(model, endpoint)
        {
            return override_result;
        }

        false
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
            "zorai".to_string()
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
        let item_count = match target {
            SettingsPickerTarget::AudioSttProvider => {
                widgets::provider_picker::available_audio_provider_defs(
                    &self.auth,
                    AudioToolKind::SpeechToText,
                )
                .len()
            }
            SettingsPickerTarget::AudioTtsProvider => {
                widgets::provider_picker::available_audio_provider_defs(
                    &self.auth,
                    AudioToolKind::TextToSpeech,
                )
                .len()
            }
            SettingsPickerTarget::ImageGenerationProvider => {
                widgets::provider_picker::available_provider_defs(&self.auth).len()
            }
            SettingsPickerTarget::EmbeddingProvider => {
                widgets::provider_picker::available_embedding_provider_defs(&self.auth).len()
            }
            _ => widgets::provider_picker::available_provider_defs(&self.auth).len(),
        };
        self.modal.set_picker_item_count(item_count);
    }

    pub(super) fn open_compaction_weles_model_picker(&mut self) {
        let provider_id = self.config.compaction_weles_provider.clone();
        let (base_url, api_key, auth_source) = self.provider_auth_snapshot(&provider_id);
        self.open_provider_backed_model_picker(
            SettingsPickerTarget::CompactionWelesModel,
            provider_id,
            base_url,
            api_key,
            auth_source,
        );
    }

    pub(super) fn open_compaction_custom_model_picker(&mut self) {
        let provider_id = self.config.compaction_custom_provider.clone();
        self.open_provider_backed_model_picker(
            SettingsPickerTarget::CompactionCustomModel,
            provider_id,
            self.config.compaction_custom_base_url.clone(),
            self.config.compaction_custom_api_key.clone(),
            self.config.compaction_custom_auth_source.clone(),
        );
    }

    pub(super) fn model_picker_current_selection(&self) -> (String, Option<String>) {
        match self
            .settings_picker_target
            .unwrap_or(SettingsPickerTarget::Model)
        {
            SettingsPickerTarget::AudioSttModel => (self.config.audio_stt_model(), None),
            SettingsPickerTarget::AudioTtsModel => (self.config.audio_tts_model(), None),
            SettingsPickerTarget::ImageGenerationModel => {
                (self.config.image_generation_model(), None)
            }
            SettingsPickerTarget::EmbeddingModel => (self.config.semantic_embedding_model(), None),
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
            SettingsPickerTarget::AudioSttModel
            | SettingsPickerTarget::AudioTtsModel
            | SettingsPickerTarget::ImageGenerationModel
            | SettingsPickerTarget::EmbeddingModel => {
                let (endpoint, provider_id) = match self
                    .settings_picker_target
                    .unwrap_or(SettingsPickerTarget::Model)
                {
                    SettingsPickerTarget::AudioSttModel => {
                        ("stt", self.config.audio_stt_provider())
                    }
                    SettingsPickerTarget::AudioTtsModel => {
                        ("tts", self.config.audio_tts_provider())
                    }
                    SettingsPickerTarget::ImageGenerationModel => {
                        ("image_generation", self.config.image_generation_provider())
                    }
                    SettingsPickerTarget::EmbeddingModel => {
                        ("embedding", self.config.semantic_embedding_provider())
                    }
                    _ => unreachable!(),
                };
                let mut models = match endpoint {
                    "image_generation" => Self::image_generation_catalog_models(&provider_id),
                    "embedding" => Self::embedding_catalog_models(&provider_id),
                    _ => Self::audio_catalog_models(endpoint, &provider_id),
                };
                for model in self.config.fetched_models() {
                    let include = match endpoint {
                        "image_generation" => {
                            let pricing_image = model
                                .pricing
                                .as_ref()
                                .and_then(|pricing| pricing.image.as_deref())
                                .is_some();
                            zorai_shared::providers::derive_model_feature_capabilities(
                                &provider_id,
                                &model.id,
                                model.metadata.as_ref(),
                                pricing_image,
                            )
                            .image_generation
                        }
                        "embedding" => model.id.to_ascii_lowercase().contains("embedding")
                            || model.id.to_ascii_lowercase().contains("embed")
                            || model
                                .metadata
                                .as_ref()
                                .map(|metadata| {
                                    metadata.to_string().to_ascii_lowercase().contains("embedding")
                                })
                                .unwrap_or(false),
                        _ => Self::fetched_model_supports_audio_endpoint(model, endpoint),
                    };
                    if include
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
                self.settings
                    .start_editing("feat_audio_stt_model", &self.config.audio_stt_model());
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
                self.settings
                    .start_editing("feat_audio_tts_model", &self.config.audio_tts_model());
                self.status_line = "Enter TTS model ID".to_string();
            }
            SettingsPickerTarget::ImageGenerationModel => {
                if self.modal.top() != Some(modal::ModalKind::Settings) {
                    self.modal
                        .reduce(modal::ModalAction::Push(modal::ModalKind::Settings));
                }
                self.settings
                    .reduce(SettingsAction::SwitchTab(SettingsTab::Features));
                self.settings_navigate_to(24);
                self.settings.start_editing(
                    "feat_image_generation_model",
                    &self.config.image_generation_model(),
                );
                self.status_line = "Enter image generation model ID".to_string();
            }
            SettingsPickerTarget::EmbeddingModel => {
                if self.modal.top() != Some(modal::ModalKind::Settings) {
                    self.modal
                        .reduce(modal::ModalAction::Push(modal::ModalKind::Settings));
                }
                self.settings
                    .reduce(SettingsAction::SwitchTab(SettingsTab::Features));
                self.settings_navigate_to(27);
                self.settings.start_editing(
                    "feat_embedding_model",
                    &self.config.semantic_embedding_model(),
                );
                self.status_line = "Enter embedding model ID".to_string();
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
        zorai_protocol::has_whatsapp_allowed_contacts(&self.config.whatsapp_allowed_contacts)
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

        if let Some(entry) = self
            .auth
            .entries
            .iter()
            .find(|entry| entry.provider_id == provider_id)
        {
            if !entry.auth_source.trim().is_empty() {
                auth_source = entry.auth_source.clone();
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
        if let Some(fixed_transport) = providers::fixed_transport_for_model(provider_id, model) {
            return fixed_transport.to_string();
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
            let Some(entry) = self
                .auth
                .entries
                .iter()
                .find(|entry| entry.provider_id == provider_id)
                .cloned()
            else {
                self.status_line = format!("Unknown provider: {provider_id}");
                return;
            };

            self.config.provider = entry.provider_id.clone();
            self.config.base_url.clear();
            self.config.model = entry.model;
            self.config.custom_model_name.clear();
            self.config.api_key.clear();
            self.config.auth_source = entry.auth_source;
            self.config.api_transport = "chat_completions".to_string();
            self.config.assistant_id.clear();
            self.config.custom_context_window_tokens = None;
            self.config.context_window_tokens = 128_000;
            self.status_line = format!("Provider: {}", entry.provider_name);
            if sync {
                self.sync_config_to_daemon();
            }
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

        self.config.api_transport = self.normalize_provider_transport_for_state(
            &self.config.provider,
            &self.config.auth_source,
            &self.config.model,
            &self.config.api_transport,
        );

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
