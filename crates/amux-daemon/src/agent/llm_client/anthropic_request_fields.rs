fn apply_anthropic_optional_request_fields(
    body: &mut serde_json::Value,
    config: &ProviderConfig,
) {
    if let Some(stop_sequences) = config
        .stop_sequences
        .as_ref()
        .filter(|values| !values.is_empty())
    {
        body["stop_sequences"] = serde_json::json!(stop_sequences);
    }
    if let Some(temperature) = config.temperature {
        body["temperature"] = serde_json::json!(temperature);
    }
    if let Some(top_p) = config.top_p {
        body["top_p"] = serde_json::json!(top_p);
    }
    if let Some(top_k) = config.top_k {
        body["top_k"] = serde_json::json!(top_k);
    }
    if let Some(metadata) = config.metadata.as_ref() {
        body["metadata"] = serde_json::json!(metadata);
    }
    if let Some(service_tier) = config.service_tier.as_ref() {
        body["service_tier"] = serde_json::json!(service_tier);
    }
    if let Some(container) = config.container.as_ref() {
        body["container"] = serde_json::json!(container);
    }
    if let Some(inference_geo) = config.inference_geo.as_ref() {
        body["inference_geo"] = serde_json::json!(inference_geo);
    }
    if let Some(cache_control) = config.cache_control.as_ref() {
        body["cache_control"] = serde_json::json!(cache_control);
    }
}

fn anthropic_tool_choice_json(config: &ProviderConfig) -> Option<serde_json::Value> {
    config
        .anthropic_tool_choice
        .as_ref()
        .map(|choice| serde_json::json!(choice))
}

fn anthropic_request_id(headers: &reqwest::header::HeaderMap) -> Option<String> {
    headers
        .get("request-id")
        .and_then(|value| value.to_str().ok())
        .map(str::to_string)
}
