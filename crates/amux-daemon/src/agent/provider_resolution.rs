use super::types::*;
use anyhow::Result;

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
    if provider_id == "openai" && resolved.auth_source == AuthSource::ChatgptSubscription {
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
        },
        config,
    ))
}

pub(super) fn resolve_active_provider_config(config: &AgentConfig) -> Result<ProviderConfig> {
    resolve_provider_config_for(config, &config.provider, None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn named_provider_inherits_defaults_and_transport_rules() {
        let mut config = AgentConfig::default();
        config.provider = "openai".to_string();
        config.base_url = "https://api.openai.com/v1".to_string();
        config.model = "gpt-5.4".to_string();
        config.reasoning_effort = "high".to_string();
        config.context_window_tokens = 99_999;
        config.assistant_id = "assistant-root".to_string();
        config.providers.insert(
            "alibaba-coding-plan".to_string(),
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
            },
        );

        let resolved = resolve_provider_config_for(
            &config,
            "alibaba-coding-plan",
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
        config.provider = "openai".to_string();
        let err = resolve_provider_config_for(&config, "groq", None).unwrap_err();
        assert!(err.to_string().contains("No credentials configured for provider 'groq'"));
    }
}
