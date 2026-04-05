use super::types::*;
use amux_shared::providers::{
    PROVIDER_ID_ALIBABA_CODING_PLAN, PROVIDER_ID_CUSTOM, PROVIDER_ID_GITHUB_COPILOT,
    PROVIDER_ID_OPENAI,
};
use anyhow::{bail, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ProviderModelSwitch {
    pub provider_id: String,
    pub model: String,
    pub base_url: String,
    pub api_transport: ApiTransport,
    pub context_window_tokens: u32,
}

fn finalize_resolved_provider(
    provider_id: &str,
    mut resolved: ProviderConfig,
    top_level: &AgentConfig,
) -> ProviderConfig {
    if resolved.reasoning_effort.trim().is_empty() {
        resolved.reasoning_effort = top_level.reasoning_effort.clone();
    }
    if resolved.assistant_id.trim().is_empty() {
        resolved.assistant_id = top_level.assistant_id.clone();
    }
    if resolved.context_window_tokens == 0 {
        resolved.context_window_tokens = top_level.context_window_tokens;
    }
    if !provider_supports_transport(provider_id, resolved.api_transport) {
        resolved.api_transport = default_api_transport_for_provider(provider_id);
    }
    if provider_id == PROVIDER_ID_OPENAI
        && resolved.auth_source == AuthSource::ChatgptSubscription
    {
        resolved.api_transport = ApiTransport::Responses;
    }
    resolved
}

pub(super) fn resolve_provider_config_for(
    config: &AgentConfig,
    provider_id: &str,
    model_override: Option<&str>,
) -> Result<ProviderConfig> {
    let requested_model = model_override.unwrap_or(&config.model);

    if let Some(pc) = config.providers.get(provider_id) {
        let mut resolved = pc.clone();
        if resolved.model.is_empty() {
            if !requested_model.is_empty() {
                resolved.model = requested_model.to_string();
            } else if let Some(def) = get_provider_definition(provider_id) {
                resolved.model = def.default_model.to_string();
            }
        }
        // For predefined providers, always use the canonical base URL from the
        // provider definition so stale values in the DB config cannot override it.
        // Only "custom" providers honour a user-supplied base_url.
        if provider_id != PROVIDER_ID_CUSTOM {
            if let Some(def) = get_provider_definition(provider_id) {
                resolved.base_url =
                    get_provider_base_url(provider_id, &resolved.model, def.default_base_url);
            }
        }
        if resolved.base_url.is_empty() {
            let inherited_base = if provider_id == config.provider {
                config.base_url.as_str()
            } else {
                ""
            };
            resolved.base_url = get_provider_base_url(provider_id, &resolved.model, inherited_base);
        }
        return Ok(finalize_resolved_provider(provider_id, resolved, config));
    }

    if provider_id != config.provider {
        anyhow::bail!(
            "No credentials configured for provider '{}'. Log in via Auth settings.",
            provider_id
        );
    }

    let model = if requested_model.is_empty() {
        get_provider_definition(provider_id)
            .map(|definition| definition.default_model.to_string())
            .unwrap_or_default()
    } else {
        requested_model.to_string()
    };
    let base_url = get_provider_base_url(provider_id, &model, &config.base_url);
    if base_url.is_empty() {
        anyhow::bail!(
            "No base URL configured for provider '{}'. Configure in agent settings.",
            provider_id
        );
    }

    Ok(finalize_resolved_provider(
        provider_id,
        ProviderConfig {
            base_url,
            model,
            api_key: config.api_key.clone(),
            assistant_id: config.assistant_id.clone(),
            auth_source: config.auth_source,
            api_transport: config.api_transport,
            reasoning_effort: config.reasoning_effort.clone(),
            context_window_tokens: config.context_window_tokens,
            response_schema: None,
            stop_sequences: None,
            temperature: None,
            top_p: None,
            top_k: None,
            metadata: None,
            service_tier: None,
            container: None,
            inference_geo: None,
            cache_control: None,
            max_tokens: None,
            anthropic_tool_choice: None,
            output_effort: None,
        },
        config,
    ))
}

pub(super) fn resolve_candidate_provider_config(
    config: &AgentConfig,
    provider_id: &str,
) -> Result<ProviderConfig> {
    let model_override = config
        .providers
        .get(provider_id)
        .filter(|provider| provider.model.trim().is_empty())
        .and_then(|_| {
            get_provider_definition(provider_id).map(|definition| definition.default_model)
        });
    resolve_provider_config_for(config, provider_id, model_override)
}

pub(super) fn resolve_active_provider_config(config: &AgentConfig) -> Result<ProviderConfig> {
    resolve_provider_config_for(config, &config.provider, None)
}

pub(super) fn resolve_provider_model_switch(
    config: &AgentConfig,
    provider_id: &str,
    model: &str,
) -> Result<ProviderModelSwitch> {
    let provider_id = provider_id.trim();
    let model = model.trim();

    if provider_id.is_empty() {
        bail!("provider id cannot be empty");
    }
    if model.is_empty() {
        bail!("model cannot be empty");
    }

    let mut base_url = config.base_url.clone();
    let mut api_transport = config.api_transport;
    let mut context_window_tokens = config.context_window_tokens;

    if provider_id == PROVIDER_ID_CUSTOM {
        if base_url.trim().is_empty() {
            bail!("base URL cannot be empty for provider 'custom'");
        }
    } else {
        let def = get_provider_definition(provider_id)
            .ok_or_else(|| anyhow::anyhow!("unknown provider '{provider_id}'"))?;
        if !def.models.is_empty() && !def.models.iter().any(|entry| entry.id == model) {
            bail!(
                "model '{}' is not available for provider '{}'",
                model,
                provider_id
            );
        }
        base_url = get_provider_base_url(provider_id, model, def.default_base_url);
        api_transport = if provider_supports_transport(provider_id, config.api_transport) {
            config.api_transport
        } else {
            def.default_transport
        };
        if let Some(model_def) = def.models.iter().find(|entry| entry.id == model) {
            context_window_tokens = model_def.context_window;
        }
    }

    match config.auth_source {
        AuthSource::ApiKey => {
            if config.api_key.trim().is_empty() {
                bail!(
                    "API key is required to switch provider '{}' to model '{}'",
                    provider_id,
                    model
                );
            }
        }
        AuthSource::ChatgptSubscription => {
            if provider_id != PROVIDER_ID_OPENAI
                || !super::llm_client::has_openai_chatgpt_subscription_auth()
            {
                bail!(
                    "ChatGPT subscription auth is not available for provider '{}'",
                    provider_id
                );
            }
        }
        AuthSource::GithubCopilot => {
            if provider_id != PROVIDER_ID_GITHUB_COPILOT
                || !super::copilot_auth::github_copilot_has_available_models(
                    &config.api_key,
                    config.auth_source,
                )
            {
                bail!(
                    "GitHub Copilot auth is not available for provider '{}'",
                    provider_id
                );
            }
        }
    }

    Ok(ProviderModelSwitch {
        provider_id: provider_id.to_string(),
        model: model.to_string(),
        base_url,
        api_transport,
        context_window_tokens,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use amux_shared::providers::{
        PROVIDER_ID_ALIBABA_CODING_PLAN, PROVIDER_ID_GROQ, PROVIDER_ID_OPENAI,
    };

    #[test]
    fn named_provider_inherits_defaults_and_transport_rules() {
        let mut config = AgentConfig::default();
        config.provider = PROVIDER_ID_OPENAI.to_string();
        config.base_url = "https://api.openai.com/v1".to_string();
        config.model = "gpt-5.4".to_string();
        config.reasoning_effort = "high".to_string();
        config.context_window_tokens = 99_999;
        config.assistant_id = "assistant-root".to_string();
        config.providers.insert(
            PROVIDER_ID_ALIBABA_CODING_PLAN.to_string(),
            ProviderConfig {
                base_url: String::new(),
                model: String::new(),
                api_key: "dashscope-key".to_string(),
                assistant_id: String::new(),
                auth_source: AuthSource::ApiKey,
                api_transport: ApiTransport::Responses,
                context_window_tokens: 0,
                reasoning_effort: String::new(),
                response_schema: None,
                stop_sequences: None,
                temperature: None,
                top_p: None,
                top_k: None,
                metadata: None,
                service_tier: None,
                container: None,
                inference_geo: None,
                cache_control: None,
                max_tokens: None,
                anthropic_tool_choice: None,
                output_effort: None,
            },
        );

        let resolved =
            resolve_provider_config_for(
                &config,
                PROVIDER_ID_ALIBABA_CODING_PLAN,
                Some("qwen3.5-plus"),
            )
                .expect("provider should resolve");

        assert_eq!(
            resolved.base_url,
            "https://coding-intl.dashscope.aliyuncs.com/v1"
        );
        assert_eq!(resolved.model, "qwen3.5-plus");
        assert_eq!(resolved.api_key, "dashscope-key");
        assert_eq!(resolved.reasoning_effort, "high");
        assert_eq!(resolved.assistant_id, "assistant-root");
        assert_eq!(resolved.context_window_tokens, 99_999);
        assert_eq!(resolved.api_transport, ApiTransport::ChatCompletions);
    }

    #[test]
    fn non_active_provider_without_named_credentials_fails() {
        let mut config = AgentConfig::default();
        config.provider = PROVIDER_ID_OPENAI.to_string();
        let err = resolve_provider_config_for(&config, PROVIDER_ID_GROQ, None).unwrap_err();
        assert!(err
            .to_string()
            .contains("No credentials configured for provider 'groq'"));
    }

    #[test]
    fn candidate_provider_with_empty_model_uses_its_own_default_model() {
        let mut config = AgentConfig::default();
        config.provider = PROVIDER_ID_OPENAI.to_string();
        config.model = "gpt-5.4".to_string();
        config.providers.insert(
            PROVIDER_ID_GROQ.to_string(),
            ProviderConfig {
                base_url: String::new(),
                model: String::new(),
                api_key: "groq-key".to_string(),
                assistant_id: String::new(),
                auth_source: AuthSource::ApiKey,
                api_transport: ApiTransport::Responses,
                context_window_tokens: 0,
                reasoning_effort: String::new(),
                response_schema: None,
                stop_sequences: None,
                temperature: None,
                top_p: None,
                top_k: None,
                metadata: None,
                service_tier: None,
                container: None,
                inference_geo: None,
                cache_control: None,
                max_tokens: None,
                anthropic_tool_choice: None,
                output_effort: None,
            },
        );

        let resolved =
            resolve_candidate_provider_config(&config, PROVIDER_ID_GROQ)
                .expect("candidate should resolve");

        assert_eq!(resolved.model, "llama-3.3-70b-versatile");
        assert_eq!(resolved.base_url, "https://api.groq.com/openai/v1");
    }
}
