use super::*;

impl TuiModel {
    pub(super) fn apply_provider_config_json(&mut self, json: &serde_json::Value) {
        let provider_id = json
            .get("provider")
            .and_then(|v| v.as_str())
            .unwrap_or(PROVIDER_ID_OPENAI);

        let provider_config = json
            .get("providers")
            .and_then(|value| value.get(provider_id))
            .or_else(|| json.get(provider_id));

        if let Some(provider_config) = provider_config {
            let base_url = json
                .get("base_url")
                .and_then(|v| v.as_str())
                .filter(|v| !v.is_empty())
                .or_else(|| Self::provider_field_str(provider_config, "base_url", "base_url"));
            let model = json
                .get("model")
                .and_then(|v| v.as_str())
                .filter(|v| !v.is_empty())
                .or_else(|| Self::provider_field_str(provider_config, "model", "model"));
            let custom_model_name =
                Self::provider_field_str(provider_config, "custom_model_name", "custom_model_name")
                    .unwrap_or("");
            let api_key = json
                .get("api_key")
                .and_then(|v| v.as_str())
                .filter(|v| !v.is_empty())
                .or_else(|| Self::provider_field_str(provider_config, "api_key", "api_key"))
                .unwrap_or("");
            let assistant_id = json
                .get("assistant_id")
                .and_then(|v| v.as_str())
                .filter(|v| !v.is_empty())
                .or_else(|| {
                    Self::provider_field_str(provider_config, "assistant_id", "assistant_id")
                })
                .unwrap_or("");
            let api_transport =
                Self::provider_field_str(provider_config, "api_transport", "api_transport")
                    .filter(|v| !v.is_empty())
                    .or_else(|| {
                        json.get("api_transport")
                            .and_then(|v| v.as_str())
                            .filter(|v| !v.is_empty())
                    })
                    .unwrap_or_else(|| providers::default_transport_for(provider_id));
            let auth_source =
                Self::provider_field_str(provider_config, "auth_source", "auth_source")
                    .filter(|v| !v.is_empty())
                    .or_else(|| {
                        json.get("auth_source")
                            .and_then(|v| v.as_str())
                            .filter(|v| !v.is_empty())
                    })
                    .unwrap_or_else(|| providers::default_auth_source_for(provider_id));
            let custom_context_window_tokens = Self::provider_field_u64(
                provider_config,
                "context_window_tokens",
                "context_window_tokens",
            )
            .map(|v| v.max(1000) as u32);

            self.config.provider = provider_id.to_string();
            self.config.base_url = if !providers::provider_uses_configurable_base_url(provider_id) {
                providers::find_by_id(provider_id)
                    .map(|def| def.default_base_url.to_string())
                    .unwrap_or_else(|| base_url.unwrap_or("").to_string())
            } else {
                base_url.map(str::to_string).unwrap_or_default()
            };
            self.config.model = model.map(str::to_string).unwrap_or_else(|| {
                providers::find_by_id(provider_id)
                    .map(|def| def.default_model.to_string())
                    .unwrap_or_default()
            });
            self.config.custom_model_name = custom_model_name.to_string();
            self.config.api_key = api_key.to_string();
            self.config.assistant_id = assistant_id.to_string();
            self.config.auth_source =
                if providers::supported_auth_sources_for(provider_id).contains(&auth_source) {
                    auth_source.to_string()
                } else {
                    providers::default_auth_source_for(provider_id).to_string()
                };
            let supported_models =
                providers::known_models_for_provider_auth(provider_id, &self.config.auth_source);
            if supported_models
                .iter()
                .any(|entry| entry.id == self.config.model)
            {
                self.config.custom_model_name.clear();
            } else if self.config.custom_model_name.trim().is_empty()
                && !self.config.model.trim().is_empty()
            {
                self.config.custom_model_name = self.config.model.clone();
            }
            self.config.api_transport =
                if providers::supported_transports_for(provider_id).contains(&api_transport) {
                    api_transport.to_string()
                } else {
                    providers::default_transport_for(provider_id).to_string()
                };
            self.config.api_transport = self.provider_transport_snapshot(
                provider_id,
                &self.config.auth_source,
                &self.config.model,
                &self.config.api_transport,
            );
            self.config.custom_context_window_tokens = custom_context_window_tokens;
            self.config.openrouter_provider_order =
                openrouter_provider_list_value(provider_config, "openrouter_provider_order");
            self.config.openrouter_provider_ignore =
                openrouter_provider_list_value(provider_config, "openrouter_provider_ignore");
            self.config.openrouter_allow_fallbacks = provider_config
                .get("openrouter_allow_fallbacks")
                .and_then(|value| value.as_bool())
                .unwrap_or(true);
            self.config.openrouter_response_cache_enabled = provider_config
                .get("openrouter_response_cache_enabled")
                .and_then(|value| value.as_bool())
                .unwrap_or(false);
            self.config.huggingface_provider = Self::provider_field_str(
                provider_config,
                "huggingface_provider",
                "huggingface_provider",
            )
            .map(str::trim)
            .unwrap_or("")
            .to_string();
            self.config.context_window_tokens = json
                .get("context_window_tokens")
                .and_then(|v| v.as_u64())
                .map(|v| v.max(1000) as u32)
                .unwrap_or_else(|| {
                    Self::effective_context_window_for_provider_value(provider_id, provider_config)
                });
        }
    }
}
