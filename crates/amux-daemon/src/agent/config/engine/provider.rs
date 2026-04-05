use super::super::*;
use amux_shared::providers::{PROVIDER_ID_GITHUB_COPILOT, PROVIDER_ID_OPENAI};
use crate::agent::{copilot_auth, openai_codex_auth, provider_resolution};

impl AgentEngine {
    pub async fn set_provider_model_json(
        &self,
        provider_id: &str,
        model: &str,
    ) -> Result<AgentConfig> {
        let updated = self.prepare_provider_model_json(provider_id, model).await?;
        self.persist_prepared_provider_model_json(updated.clone())
            .await;
        self.reconcile_config_runtime_after_commit().await?;
        Ok(self.get_config().await)
    }

    pub async fn persist_prepared_provider_model_json(&self, merged: AgentConfig) {
        let mut merged = merged;
        let collisions = sanitize_weles_collisions_from_config(&mut merged);
        let _ = self.persist_sanitized_config(merged, collisions).await;
        let mut projection = self.config_runtime_projection.lock().await;
        projection.desired_revision = projection.desired_revision.saturating_add(1);
        projection.state = ConfigReconcileState::Reconciling;
        projection.last_error = None;
    }

    pub async fn prepare_provider_model_json(
        &self,
        provider_id: &str,
        model: &str,
    ) -> Result<AgentConfig> {
        let current = self.get_config().await;
        let selection =
            provider_resolution::resolve_provider_model_switch(&current, provider_id, model)?;

        let mut updated = current;
        updated.provider = selection.provider_id;
        updated.model = selection.model;
        updated.base_url = selection.base_url;
        updated.api_transport = selection.api_transport;
        updated.context_window_tokens = selection.context_window_tokens;

        Ok(updated)
    }

    pub async fn merge_config_patch_json(&self, patch_json: &str) -> Result<AgentConfig> {
        let mut patch_value: Value =
            serde_json::from_str(patch_json).context("invalid config patch JSON")?;
        normalize_config_keys_to_snake_case(&mut patch_value);
        let mut merged_value = serde_json::to_value(self.get_config().await)?;
        normalize_config_keys_to_snake_case(&mut merged_value);
        merge_json_value(&mut merged_value, patch_value);
        sanitize_config_value(&mut merged_value);
        let merged = match serde_json::from_value::<AgentConfig>(merged_value.clone()) {
            Ok(merged) => merged,
            Err(error) => {
                let redacted_patch = serde_json::from_str::<Value>(patch_json)
                    .map(|value| redact_config_value(&value))
                    .unwrap_or_else(|_| Value::String("<invalid-json>".to_string()));
                let redacted_merged = redact_config_value(&merged_value);
                tracing::warn!(
                    error = %error,
                    patch_keys = ?top_level_config_keys(&redacted_patch),
                    merged_keys = ?top_level_config_keys(&redacted_merged),
                    patch = %redacted_patch,
                    merged = %redacted_merged,
                    "agent config patch merge failed"
                );
                return Err(error).context("merged config patch could not be parsed");
            }
        };
        self.set_config(merged.clone()).await;
        let _ = self.reinit_gateway().await;
        Ok(merged)
    }

    /// Build provider auth states by merging persisted config with PROVIDER_DEFINITIONS.
    pub async fn get_provider_auth_states(&self) -> Vec<ProviderAuthState> {
        use crate::agent::types::{ProviderAuthState, PROVIDER_DEFINITIONS};

        let config = self.config.read().await;
        let mut states = Vec::new();
        let use_legacy_top_level_fallback = config.providers.is_empty();

        for def in PROVIDER_DEFINITIONS {
            let (authenticated, auth_source, model, base_url) = if let Some(pc) =
                config.providers.get(def.id)
            {
                if def.id == PROVIDER_ID_GITHUB_COPILOT {
                    let resolved =
                        copilot_auth::resolve_github_copilot_auth(&pc.api_key, pc.auth_source);
                    (
                        resolved.is_some(),
                        resolved
                            .as_ref()
                            .map(|auth| auth.auth_source)
                            .unwrap_or(pc.auth_source),
                        pc.model.clone(),
                        pc.base_url.clone(),
                    )
                } else if def.id == PROVIDER_ID_OPENAI
                    && pc.auth_source == AuthSource::ChatgptSubscription
                {
                    (
                        openai_codex_auth::provider_auth_state_authenticated(),
                        pc.auth_source,
                        pc.model.clone(),
                        pc.base_url.clone(),
                    )
                } else {
                    (
                        !pc.api_key.is_empty(),
                        pc.auth_source,
                        pc.model.clone(),
                        pc.base_url.clone(),
                    )
                }
            } else if use_legacy_top_level_fallback && config.provider == def.id {
                if def.id == PROVIDER_ID_GITHUB_COPILOT {
                    let resolved = copilot_auth::resolve_github_copilot_auth(
                        &config.api_key,
                        config.auth_source,
                    );
                    (
                        resolved.is_some(),
                        resolved
                            .as_ref()
                            .map(|auth| auth.auth_source)
                            .unwrap_or(config.auth_source),
                        config.model.clone(),
                        config.base_url.clone(),
                    )
                } else if def.id == PROVIDER_ID_OPENAI
                    && config.auth_source == AuthSource::ChatgptSubscription
                {
                    (
                        openai_codex_auth::provider_auth_state_authenticated(),
                        config.auth_source,
                        config.model.clone(),
                        config.base_url.clone(),
                    )
                } else {
                    // Fall back to top-level config if this is the active provider.
                    (
                        !config.api_key.is_empty(),
                        config.auth_source,
                        config.model.clone(),
                        config.base_url.clone(),
                    )
                }
            } else if def.id == PROVIDER_ID_GITHUB_COPILOT {
                let resolved = copilot_auth::resolve_github_copilot_auth("", AuthSource::ApiKey);
                (
                    resolved.is_some(),
                    resolved
                        .as_ref()
                        .map(|auth| auth.auth_source)
                        .unwrap_or(AuthSource::ApiKey),
                    def.default_model.to_string(),
                    def.default_base_url.to_string(),
                )
            } else {
                (
                    false,
                    AuthSource::default(),
                    def.default_model.to_string(),
                    def.default_base_url.to_string(),
                )
            };

            states.push(ProviderAuthState {
                provider_id: def.id.to_string(),
                provider_name: def.name.to_string(),
                authenticated,
                auth_source,
                model,
                base_url,
            });
        }

        states
    }
}
