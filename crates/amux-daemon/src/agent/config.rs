//! Agent configuration get/set.

use super::*;
use serde_json::Value;

fn is_sensitive_config_key(key: &str) -> bool {
    let normalized = camel_to_snake_key(key);
    let lower = normalized.to_ascii_lowercase();
    matches!(
        normalized.as_str(),
        "api_key"
            | "slack_token"
            | "telegram_token"
            | "discord_token"
            | "whatsapp_token"
            | "firecrawl_api_key"
            | "exa_api_key"
            | "tavily_api_key"
            | "honcho_api_key"
            | "client_secret"
            | "access_token"
            | "refresh_token"
    ) || key.ends_with("_token")
        || key.ends_with("_api_key")
        || lower.contains("oauth")
        || lower.contains("credential")
}

fn redact_config_value(value: &Value) -> Value {
    match value {
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(key, value)| {
                    let redacted = if is_sensitive_config_key(key) {
                        Value::String("<redacted>".to_string())
                    } else {
                        redact_config_value(value)
                    };
                    (key.clone(), redacted)
                })
                .collect(),
        ),
        Value::Array(items) => Value::Array(items.iter().map(redact_config_value).collect()),
        other => other.clone(),
    }
}

fn top_level_config_keys(value: &Value) -> Vec<String> {
    value
        .as_object()
        .map(|map| map.keys().cloned().collect())
        .unwrap_or_default()
}

fn camel_to_snake_key(key: &str) -> String {
    let mut result = String::with_capacity(key.len() + 4);
    let mut prev_is_lower_or_digit = false;
    for ch in key.chars() {
        if ch.is_ascii_uppercase() {
            if prev_is_lower_or_digit {
                result.push('_');
            }
            result.push(ch.to_ascii_lowercase());
            prev_is_lower_or_digit = false;
        } else {
            prev_is_lower_or_digit = ch.is_ascii_lowercase() || ch.is_ascii_digit();
            result.push(ch);
        }
    }
    result
}

pub(crate) fn normalize_config_keys_to_snake_case(value: &mut Value) {
    match value {
        Value::Object(map) => {
            let entries: Vec<(String, Value)> = map.clone().into_iter().collect();
            map.clear();
            for (key, mut child) in entries {
                normalize_config_keys_to_snake_case(&mut child);
                map.insert(camel_to_snake_key(&key), child);
            }
        }
        Value::Array(items) => {
            for item in items {
                normalize_config_keys_to_snake_case(item);
            }
        }
        _ => {}
    }
}

fn merge_json_value(base: &mut Value, patch: Value) {
    match (base, patch) {
        (Value::Object(base_map), Value::Object(patch_map)) => {
            for (key, patch_value) in patch_map {
                match base_map.get_mut(&key) {
                    Some(base_value) => merge_json_value(base_value, patch_value),
                    None => {
                        base_map.insert(key, patch_value);
                    }
                }
            }
        }
        (base_slot, patch_value) => {
            *base_slot = patch_value;
        }
    }
}

fn escape_pointer_segment(segment: &str) -> String {
    segment.replace('~', "~0").replace('/', "~1")
}

fn unescape_pointer_segment(segment: &str) -> String {
    segment.replace("~1", "/").replace("~0", "~")
}

pub(crate) fn flatten_config_value_to_items(
    value: &Value,
    pointer: &str,
    items: &mut Vec<(String, Value)>,
) {
    match value {
        Value::Object(map) if !map.is_empty() => {
            for (key, child) in map {
                let next = format!("{}/{}", pointer, escape_pointer_segment(key));
                flatten_config_value_to_items(child, &next, items);
            }
        }
        other => items.push((pointer.to_string(), other.clone())),
    }
}

fn set_config_value_at_pointer(root: &mut Value, pointer: &str, value: Value) -> Result<()> {
    if !pointer.starts_with('/') {
        anyhow::bail!("config key path must be a JSON pointer starting with '/'");
    }
    let mut current = root;
    let mut segments = pointer
        .trim_start_matches('/')
        .split('/')
        .filter(|segment| !segment.is_empty())
        .peekable();
    while let Some(segment) = segments.next() {
        let key = unescape_pointer_segment(segment);
        if segments.peek().is_none() {
            let object = current
                .as_object_mut()
                .context("config pointer parent is not an object")?;
            object.insert(key, value);
            return Ok(());
        }
        let object = current
            .as_object_mut()
            .context("config pointer parent is not an object")?;
        current = object
            .entry(key)
            .or_insert_with(|| Value::Object(Default::default()));
        if !current.is_object() {
            *current = Value::Object(Default::default());
        }
    }
    anyhow::bail!("config key path must not point to the root object")
}

pub(crate) fn load_config_from_items(items: Vec<(String, Value)>) -> Result<AgentConfig> {
    if items.is_empty() {
        return Ok(AgentConfig::default());
    }
    let mut root = Value::Object(Default::default());
    for (pointer, value) in items {
        set_config_value_at_pointer(&mut root, &pointer, value)?;
    }
    sanitize_config_value(&mut root);
    Ok(serde_json::from_value(root).unwrap_or_default())
}

fn canonical_enum_value(value: &str, default: &str, aliases: &[(&str, &str)]) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return default.to_string();
    }
    let lowercase = trimmed.to_ascii_lowercase();
    for (alias, canonical) in aliases {
        if lowercase == *alias {
            return (*canonical).to_string();
        }
    }
    lowercase
}

fn normalize_string_enum_field(
    object: &mut serde_json::Map<String, Value>,
    key: &str,
    default: &str,
    aliases: &[(&str, &str)],
) {
    let normalized = match object.get(key) {
        Some(Value::String(value)) => canonical_enum_value(value, default, aliases),
        Some(Value::Null) | None => default.to_string(),
        Some(other) => canonical_enum_value(&other.to_string(), default, aliases),
    };
    object.insert(key.to_string(), Value::String(normalized));
}

pub(crate) fn sanitize_config_value(config: &mut Value) {
    normalize_config_keys_to_snake_case(config);
    let Some(root) = config.as_object_mut() else {
        return;
    };

    normalize_string_enum_field(
        root,
        "auth_source",
        "api_key",
        &[
            ("apikey", "api_key"),
            ("api-key", "api_key"),
            ("chatgptsubscription", "chatgpt_subscription"),
            ("chatgpt-subscription", "chatgpt_subscription"),
            ("chatgpt subscription", "chatgpt_subscription"),
        ],
    );
    normalize_string_enum_field(
        root,
        "api_transport",
        "responses",
        &[
            ("responses", "responses"),
            ("chatcompletions", "chat_completions"),
            ("chat-completions", "chat_completions"),
            ("chat completions", "chat_completions"),
            ("nativeassistant", "native_assistant"),
            ("native-assistant", "native_assistant"),
            ("native assistant", "native_assistant"),
        ],
    );
    normalize_string_enum_field(
        root,
        "agent_backend",
        "daemon",
        &[
            ("daemon", "daemon"),
            ("openclaw", "openclaw"),
            ("hermes", "hermes"),
            ("legacy", "legacy"),
        ],
    );

    if let Some(Value::Object(compliance)) = root.get_mut("compliance") {
        normalize_string_enum_field(
            compliance,
            "mode",
            "standard",
            &[
                ("standard", "standard"),
                ("soc2", "soc2"),
                ("hipaa", "hipaa"),
                ("fedramp", "fedramp"),
            ],
        );
    }

    if let Some(Value::Object(concierge)) = root.get_mut("concierge") {
        normalize_string_enum_field(
            concierge,
            "detail_level",
            "proactive_triage",
            &[
                ("minimal", "minimal"),
                ("contextsummary", "context_summary"),
                ("context-summary", "context_summary"),
                ("context summary", "context_summary"),
                ("proactivetriage", "proactive_triage"),
                ("proactive-triage", "proactive_triage"),
                ("proactive triage", "proactive_triage"),
                ("dailybriefing", "daily_briefing"),
                ("daily-briefing", "daily_briefing"),
                ("daily briefing", "daily_briefing"),
            ],
        );
    }

    if let Some(Value::Object(providers)) = root.get_mut("providers") {
        for provider in providers.values_mut() {
            if let Value::Object(provider_obj) = provider {
                for required_field in ["base_url", "model", "api_key"] {
                    if !provider_obj.contains_key(required_field) {
                        provider_obj
                            .insert(required_field.to_string(), Value::String(String::new()));
                    }
                }
                normalize_string_enum_field(
                    provider_obj,
                    "auth_source",
                    "api_key",
                    &[
                        ("apikey", "api_key"),
                        ("api-key", "api_key"),
                        ("chatgptsubscription", "chatgpt_subscription"),
                        ("chatgpt-subscription", "chatgpt_subscription"),
                        ("chatgpt subscription", "chatgpt_subscription"),
                    ],
                );
                normalize_string_enum_field(
                    provider_obj,
                    "api_transport",
                    "responses",
                    &[
                        ("responses", "responses"),
                        ("chatcompletions", "chat_completions"),
                        ("chat-completions", "chat_completions"),
                        ("chat completions", "chat_completions"),
                        ("nativeassistant", "native_assistant"),
                        ("native-assistant", "native_assistant"),
                        ("native assistant", "native_assistant"),
                    ],
                );
            }
        }
    }
}

impl AgentEngine {
    pub async fn get_config(&self) -> AgentConfig {
        self.config.read().await.clone()
    }

    pub async fn set_config(&self, config: AgentConfig) {
        let mut value =
            serde_json::to_value(&config).unwrap_or_else(|_| Value::Object(Default::default()));
        normalize_config_keys_to_snake_case(&mut value);
        sanitize_config_value(&mut value);
        let mut items = Vec::new();
        flatten_config_value_to_items(&value, "", &mut items);
        if let Err(error) = self.history.replace_agent_config_items(&items).await {
            tracing::warn!("failed to persist agent config to sqlite: {error}");
        }
        *self.config.write().await = config;
        self.config_notify.notify_waiters();
    }

    pub async fn set_config_item_json(
        &self,
        key_path: &str,
        value_json: &str,
    ) -> Result<AgentConfig> {
        let value =
            serde_json::from_str::<Value>(value_json).context("invalid config item JSON")?;
        let mut merged_value = serde_json::to_value(self.get_config().await)?;
        normalize_config_keys_to_snake_case(&mut merged_value);
        set_config_value_at_pointer(&mut merged_value, key_path, value.clone())?;
        sanitize_config_value(&mut merged_value);
        let merged = serde_json::from_value::<AgentConfig>(merged_value)
            .context("updated config item could not be parsed")?;
        self.history
            .upsert_agent_config_item(key_path, &value)
            .await
            .context("failed to persist config item update")?;
        *self.config.write().await = merged.clone();
        self.config_notify.notify_waiters();
        self.reinit_gateway().await;
        Ok(merged)
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
        self.reinit_gateway().await;
        Ok(merged)
    }

    /// Build provider auth states by merging persisted config with PROVIDER_DEFINITIONS.
    pub async fn get_provider_auth_states(&self) -> Vec<ProviderAuthState> {
        use crate::agent::types::{ProviderAuthState, PROVIDER_DEFINITIONS};

        let config = self.config.read().await;
        let mut states = Vec::new();
        let use_legacy_top_level_fallback = config.providers.is_empty();

        for def in PROVIDER_DEFINITIONS {
            let (authenticated, auth_source, model, base_url) =
                if let Some(pc) = config.providers.get(def.id) {
                    (
                        !pc.api_key.is_empty(),
                        pc.auth_source,
                        pc.model.clone(),
                        pc.base_url.clone(),
                    )
                } else if use_legacy_top_level_fallback && config.provider == def.id {
                    // Fall back to top-level config if this is the active provider.
                    (
                        !config.api_key.is_empty(),
                        config.auth_source,
                        config.model.clone(),
                        config.base_url.clone(),
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

    /// Upsert a sub-agent definition (matched by id).
    pub async fn set_sub_agent(&self, def: SubAgentDefinition) {
        let mut config = self.config.write().await;
        if let Some(existing) = config.sub_agents.iter_mut().find(|s| s.id == def.id) {
            *existing = def;
        } else {
            config.sub_agents.push(def);
        }
        drop(config);
        self.persist_config().await;
    }

    /// Remove a sub-agent definition by id.
    pub async fn remove_sub_agent(&self, id: &str) -> bool {
        let mut config = self.config.write().await;
        let before = config.sub_agents.len();
        config.sub_agents.retain(|s| s.id != id);
        let removed = config.sub_agents.len() < before;
        drop(config);
        if removed {
            self.persist_config().await;
        }
        removed
    }

    /// List all sub-agent definitions.
    pub async fn list_sub_agents(&self) -> Vec<SubAgentDefinition> {
        self.config.read().await.sub_agents.clone()
    }

    /// Get the concierge configuration.
    pub async fn get_concierge_config(&self) -> ConciergeConfig {
        self.config.read().await.concierge.clone()
    }

    /// Update the concierge configuration and persist.
    pub async fn set_concierge_config(&self, concierge: ConciergeConfig) {
        self.config.write().await.concierge = concierge;
        self.persist_config().await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session_manager::SessionManager;
    use tempfile::tempdir;

    #[tokio::test]
    async fn merge_config_patch_preserves_existing_provider_state() {
        let root = tempdir().unwrap();
        let manager = SessionManager::new_test(root.path()).await;
        let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
        let mut config = engine.get_config().await;
        config.provider = "openai".to_string();
        config.base_url = "https://api.openai.com/v1".to_string();
        config.model = "gpt-5.4".to_string();
        config.api_key = "root-key".to_string();
        config.providers.insert(
            "openai".to_string(),
            ProviderConfig {
                base_url: "https://api.openai.com/v1".to_string(),
                model: "gpt-5.4".to_string(),
                api_key: "openai-key".to_string(),
                assistant_id: "asst_openai".to_string(),
                auth_source: AuthSource::ApiKey,
                api_transport: ApiTransport::Responses,
                context_window_tokens: 128_000,
                reasoning_effort: "high".to_string(),
                response_schema: None,
            },
        );
        config.providers.insert(
            "groq".to_string(),
            ProviderConfig {
                base_url: "https://api.groq.com/openai/v1".to_string(),
                model: "llama-3.3-70b-versatile".to_string(),
                api_key: "groq-key".to_string(),
                assistant_id: String::new(),
                auth_source: AuthSource::ApiKey,
                api_transport: ApiTransport::Responses,
                context_window_tokens: 128_000,
                reasoning_effort: "high".to_string(),
                response_schema: None,
            },
        );
        engine.set_config(config).await;

        engine
            .merge_config_patch_json(r#"{"model":"gpt-5.4-mini"}"#)
            .await
            .unwrap();

        let updated = engine.get_config().await;
        assert_eq!(updated.model, "gpt-5.4-mini");
        assert_eq!(
            updated
                .providers
                .get("openai")
                .map(|provider| provider.api_key.as_str()),
            Some("openai-key")
        );
        assert_eq!(
            updated
                .providers
                .get("groq")
                .map(|provider| provider.api_key.as_str()),
            Some("groq-key")
        );
    }

    #[tokio::test]
    async fn merge_config_patch_sanitizes_stale_enum_strings() {
        let root = tempdir().unwrap();
        let manager = SessionManager::new_test(root.path()).await;
        let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

        engine
            .merge_config_patch_json(
                r#"{
                    "agent_backend":"OpenClaw",
                    "auth_source":"API-KEY",
                    "api_transport":"chat completions",
                    "concierge":{"detail_level":"daily briefing"},
                    "compliance":{"mode":"SOC2"},
                    "providers":{
                        "openai":{"auth_source":"chatgpt-subscription","api_transport":"native assistant"}
                    }
                }"#,
            )
            .await
            .unwrap();

        let updated = engine.get_config().await;
        assert_eq!(updated.agent_backend, AgentBackend::Openclaw);
        assert_eq!(updated.auth_source, AuthSource::ApiKey);
        assert_eq!(updated.api_transport, ApiTransport::ChatCompletions);
        assert_eq!(
            updated.concierge.detail_level,
            ConciergeDetailLevel::DailyBriefing
        );
        assert_eq!(updated.compliance.mode, ComplianceMode::Soc2);
        let provider = updated.providers.get("openai").unwrap();
        assert_eq!(provider.auth_source, AuthSource::ChatgptSubscription);
        assert_eq!(provider.api_transport, ApiTransport::NativeAssistant);
    }

    #[tokio::test]
    async fn merge_config_patch_preserves_extended_gateway_fields() {
        let root = tempdir().unwrap();
        let manager = SessionManager::new_test(root.path()).await;
        let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

        engine
            .merge_config_patch_json(
                r#"{
                    "gateway": {
                        "enabled": true,
                        "command_prefix": "!tamux",
                        "slack_token": "xoxb-test",
                        "slack_channel_filter": "ops,alerts",
                        "telegram_token": "tg-token",
                        "telegram_allowed_chats": "1,2",
                        "discord_token": "discord-token",
                        "discord_channel_filter": "deployments",
                        "discord_allowed_users": "alice,bob",
                        "whatsapp_allowed_contacts": "+48123456789",
                        "whatsapp_token": "wa-token",
                        "whatsapp_phone_id": "phone-id"
                    }
                }"#,
            )
            .await
            .unwrap();

        let updated = engine.get_config().await;
        assert!(updated.gateway.enabled);
        assert_eq!(updated.gateway.command_prefix, "!tamux");
        assert_eq!(updated.gateway.slack_channel_filter, "ops,alerts");
        assert_eq!(updated.gateway.telegram_allowed_chats, "1,2");
        assert_eq!(updated.gateway.discord_channel_filter, "deployments");
        assert_eq!(updated.gateway.discord_allowed_users, "alice,bob");
        assert_eq!(updated.gateway.whatsapp_allowed_contacts, "+48123456789");
        assert_eq!(updated.gateway.whatsapp_token, "wa-token");
        assert_eq!(updated.gateway.whatsapp_phone_id, "phone-id");
    }

    #[test]
    fn agent_config_serializes_honcho_fields_in_snake_case() {
        let config = AgentConfig {
            enable_honcho_memory: true,
            honcho_api_key: "key".to_string(),
            honcho_base_url: "https://honcho.example".to_string(),
            honcho_workspace_id: "workspace".to_string(),
            ..AgentConfig::default()
        };

        let json = serde_json::to_value(config).unwrap();
        assert_eq!(json["enable_honcho_memory"], true);
        assert_eq!(json["honcho_api_key"], "key");
        assert_eq!(json["honcho_base_url"], "https://honcho.example");
        assert_eq!(json["honcho_workspace_id"], "workspace");
    }
}
