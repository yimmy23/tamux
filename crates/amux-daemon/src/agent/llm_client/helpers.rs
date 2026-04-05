struct PendingToolCall {
    id: String,
    name: String,
    arguments: String,
}

fn synthesize_tool_call_id(seed: &str, index: usize, name: &str) -> String {
    let clean_name: String = name
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' {
                ch
            } else {
                '_'
            }
        })
        .collect();
    format!("synthetic_tool_call_{seed}_{index}_{clean_name}")
}

fn drain_tool_calls(map: &mut HashMap<u32, PendingToolCall>) -> Vec<ToolCall> {
    let mut entries: Vec<(u32, PendingToolCall)> = map.drain().collect();
    entries.sort_by_key(|(idx, _)| *idx);
    entries
        .into_iter()
        .map(|(idx, tc)| {
            ToolCall::with_default_weles_review(
                if tc.id.trim().is_empty() {
                    synthesize_tool_call_id(
                        amux_shared::providers::PROVIDER_ID_OPENAI,
                        idx as usize,
                        &tc.name,
                    )
                } else {
                    tc.id
                },
                ToolFunction {
                    name: tc.name,
                    arguments: tc.arguments,
                },
            )
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Model fetching
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FetchedModel {
    pub id: String,
    pub name: Option<String>,
    pub context_window: Option<u32>,
}

pub async fn fetch_models(
    provider_id: &str,
    base_url: &str,
    api_key: &str,
) -> Result<Vec<FetchedModel>> {
    if provider_id == amux_shared::providers::PROVIDER_ID_GITHUB_COPILOT {
        return get_provider_definition(provider_id)
            .map(|definition| {
                definition
                    .models
                    .iter()
                    .map(|model| FetchedModel {
                        id: model.id.to_string(),
                        name: Some(model.name.to_string()),
                        context_window: Some(model.context_window),
                    })
                    .collect()
            })
            .ok_or_else(|| anyhow::anyhow!("Unknown provider '{}'", provider_id));
    }

    let def = super::types::get_provider_definition(provider_id)
        .ok_or_else(|| anyhow::anyhow!("Unknown provider '{}'", provider_id))?;

    if !def.supports_model_fetch {
        tracing::warn!(provider_id, "provider does not support remote model fetching");
        return Ok(Vec::new());
    }

    let client = reqwest::Client::new();
    let url = format!("{}/models", base_url.trim_end_matches('/'));

    let mut req = client.get(&url).header("Content-Type", "application/json");

    if !api_key.is_empty() {
        req = def.auth_method.apply(req, api_key);
    }

    let response = req.send().await?;

    if !response.status().is_success() {
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        return Err(anyhow::anyhow!(
            "Failed to fetch models: {} - {}",
            status,
            text
        ));
    }

    let json: serde_json::Value = response.json().await?;

    let models = json
        .get("data")
        .and_then(|d| d.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|m| {
                    let id = m.get("id")?.as_str()?.to_string();
                    let name = m
                        .get("name")
                        .and_then(|n| n.as_str())
                        .map(|s| s.to_string());

                    let context_window = m
                        .get("context_length")
                        .or_else(|| m.get("context_window"))
                        .and_then(|c| c.as_u64())
                        .map(|n| n as u32);

                    Some(FetchedModel {
                        id,
                        name,
                        context_window,
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    Ok(models)
}

pub async fn validate_provider_connection(
    provider_id: &str,
    base_url: &str,
    api_key: &str,
    auth_source: AuthSource,
) -> Result<Option<Vec<FetchedModel>>> {
    let def = get_provider_definition(provider_id)
        .with_context(|| format!("Unknown provider '{}'", provider_id))?;
    let resolved_base_url = if base_url.trim().is_empty() {
        def.default_base_url.to_string()
    } else {
        base_url.trim().to_string()
    };

    if provider_id == amux_shared::providers::PROVIDER_ID_GITHUB_COPILOT {
        let models = super::copilot_auth::list_github_copilot_models(api_key, auth_source)?
            .into_iter()
            .map(|model| FetchedModel {
                id: model.id,
                name: model.name,
                context_window: model.context_window,
            })
            .collect::<Vec<_>>();
        if models.is_empty() {
            anyhow::bail!("GitHub Copilot auth is valid but no models are available");
        }
        return Ok(Some(models));
    }

    if provider_id == amux_shared::providers::PROVIDER_ID_OPENAI
        && auth_source == AuthSource::ChatgptSubscription
    {
        let client = reqwest::Client::new();
        let config = ProviderConfig {
            base_url: resolved_base_url,
            model: def.default_model.to_string(),
            api_key: String::new(),
            assistant_id: String::new(),
            auth_source,
            api_transport: def.default_transport,
            reasoning_effort: "off".to_string(),
            context_window_tokens: def
                .models
                .iter()
                .find(|model| model.id == def.default_model)
                .map(|model| model.context_window)
                .unwrap_or(128_000),
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
        };
        let _ = resolve_openai_codex_request_auth(&client, provider_id, &config).await?;
        return Ok(None);
    }

    // Always validate via a minimal chat completion — this tests both connectivity
    // AND the API key (fetch_models doesn't require auth on some providers like OpenRouter).
    let client = reqwest::Client::new();
    let api_type = get_provider_api_type(provider_id, def.default_model, &resolved_base_url);
    let request = match api_type {
        ApiType::OpenAI => {
            let url = build_chat_completion_url(&resolved_base_url);
            let body = serde_json::json!({
                "model": def.default_model,
                "messages": [{ "role": "user", "content": "ok" }],
                "max_tokens": 1,
                "stream": false,
            });
            let req = client.post(url).header("Content-Type", "application/json");
            let req = if !api_key.is_empty() {
                def.auth_method.apply(req, api_key)
            } else {
                req
            };
            apply_dashscope_coding_plan_sdk_headers(req, provider_id, &resolved_base_url, api_type)
                .json(&body)
        }
        ApiType::Anthropic => {
            let base = resolved_base_url.trim_end_matches('/');
            let url = if base.ends_with("/v1") {
                format!("{}/messages", base)
            } else {
                format!("{}/v1/messages", base)
            };
            let body = serde_json::json!({
                "model": def.default_model,
                "max_tokens": 1,
                "messages": [{ "role": "user", "content": "ok" }],
            });
            let req = client.post(url).header("Content-Type", "application/json");
            let req = if !is_dashscope_coding_plan_anthropic_base_url(&resolved_base_url) {
                req.header("anthropic-version", "2023-06-01")
            } else {
                req
            };
            let req = if !api_key.is_empty() {
                def.auth_method.apply(req, api_key)
            } else {
                req
            };
            apply_dashscope_coding_plan_sdk_headers(req, provider_id, &resolved_base_url, api_type)
                .json(&body)
        }
    };

    let response = request.send().await?;
    if !response.status().is_success() {
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        anyhow::bail!("Validation failed: {} - {}", status, text);
    }

    Ok(None)
}
