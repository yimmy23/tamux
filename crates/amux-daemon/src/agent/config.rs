//! Agent configuration get/set.

use super::*;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum ConfigReconcileState {
    Applied,
    Reconciling,
    Degraded,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct ConfigRuntimeProjection {
    pub state: ConfigReconcileState,
    pub desired_revision: u64,
    pub effective_revision: u64,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct ConfigEffectiveRuntimeState {
    pub reconcile: ConfigRuntimeProjection,
    pub gateway_runtime_connected: bool,
}

impl Default for ConfigRuntimeProjection {
    fn default() -> Self {
        Self {
            state: ConfigReconcileState::Applied,
            desired_revision: 0,
            effective_revision: 0,
            last_error: None,
        }
    }
}

fn gateway_runtime_is_desired_at_startup(config: &AgentConfig) -> bool {
    config.gateway.enabled
        && (!config.gateway.slack_token.trim().is_empty()
            || !config.gateway.telegram_token.trim().is_empty()
            || !config.gateway.discord_token.trim().is_empty()
            || !config.gateway.whatsapp_token.trim().is_empty()
            || config.gateway.whatsapp_link_fallback_electron)
}

pub(crate) fn derive_startup_config_runtime_projection(
    config: &AgentConfig,
) -> ConfigRuntimeProjection {
    if gateway_runtime_is_desired_at_startup(config) {
        ConfigRuntimeProjection {
            state: ConfigReconcileState::Degraded,
            desired_revision: 1,
            effective_revision: 0,
            last_error: Some(
                "startup: gateway runtime is not connected; effective state must be re-derived"
                    .to_string(),
            ),
        }
    } else {
        ConfigRuntimeProjection::default()
    }
}

#[cfg(test)]
fn config_reconcile_delay_map(
) -> &'static std::sync::Mutex<std::collections::HashMap<usize, std::time::Duration>> {
    static MAP: std::sync::OnceLock<
        std::sync::Mutex<std::collections::HashMap<usize, std::time::Duration>>,
    > = std::sync::OnceLock::new();
    MAP.get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()))
}

#[cfg(test)]
fn config_reconcile_failure_map(
) -> &'static std::sync::Mutex<std::collections::HashMap<usize, String>> {
    static MAP: std::sync::OnceLock<std::sync::Mutex<std::collections::HashMap<usize, String>>> =
        std::sync::OnceLock::new();
    MAP.get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()))
}

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
            ("githubcopilot", "github_copilot"),
            ("github-copilot", "github_copilot"),
            ("github copilot", "github_copilot"),
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
                        ("githubcopilot", "github_copilot"),
                        ("github-copilot", "github_copilot"),
                        ("github copilot", "github_copilot"),
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
    pub(crate) async fn current_desired_config_snapshot(&self) -> AgentConfig {
        self.get_config().await
    }

    pub(crate) async fn current_effective_config_runtime_state(
        &self,
    ) -> ConfigEffectiveRuntimeState {
        ConfigEffectiveRuntimeState {
            reconcile: self.current_config_runtime_projection().await,
            gateway_runtime_connected: self.gateway_ipc_sender.lock().await.is_some(),
        }
    }

    pub(crate) async fn current_config_runtime_projection(&self) -> ConfigRuntimeProjection {
        self.config_runtime_projection.lock().await.clone()
    }

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
        let (merged, value) = self.prepare_config_item_json(key_path, value_json).await?;
        self.persist_prepared_config_item_json(key_path, &value, merged.clone())
            .await?;
        self.reconcile_config_runtime_after_commit().await?;
        Ok(merged)
    }

    pub async fn prepare_config_item_json(
        &self,
        key_path: &str,
        value_json: &str,
    ) -> Result<(AgentConfig, Value)> {
        let value =
            serde_json::from_str::<Value>(value_json).context("invalid config item JSON")?;
        let mut merged_value = serde_json::to_value(self.get_config().await)?;
        normalize_config_keys_to_snake_case(&mut merged_value);
        set_config_value_at_pointer(&mut merged_value, key_path, value.clone())?;
        sanitize_config_value(&mut merged_value);
        let merged = serde_json::from_value::<AgentConfig>(merged_value)
            .context("updated config item could not be parsed")?;
        Ok((merged, value))
    }

    pub async fn persist_prepared_config_item_json(
        &self,
        key_path: &str,
        value: &Value,
        merged: AgentConfig,
    ) -> Result<()> {
        self.history
            .upsert_agent_config_item(key_path, &value)
            .await
            .context("failed to persist config item update")?;
        *self.config.write().await = merged.clone();
        self.config_notify.notify_waiters();
        let mut projection = self.config_runtime_projection.lock().await;
        projection.desired_revision = projection.desired_revision.saturating_add(1);
        projection.state = ConfigReconcileState::Reconciling;
        projection.last_error = None;
        Ok(())
    }

    pub async fn reconcile_config_runtime_after_commit(&self) -> Result<()> {
        #[cfg(test)]
        if let Some(delay) = self.take_test_config_reconcile_delay().await {
            tokio::time::sleep(delay).await;
        }

        #[cfg(test)]
        if let Some(error) = self.take_test_config_reconcile_failure().await {
            let mut projection = self.config_runtime_projection.lock().await;
            projection.state = ConfigReconcileState::Error;
            projection.last_error = Some(error.clone());
            return Err(anyhow::anyhow!(error));
        }

        let degraded_reason = self.reinit_gateway().await;
        let mut projection = self.config_runtime_projection.lock().await;
        if let Some(reason) = degraded_reason {
            projection.state = ConfigReconcileState::Degraded;
            projection.last_error = Some(reason);
        } else {
            projection.effective_revision = projection.desired_revision;
            projection.state = ConfigReconcileState::Applied;
            projection.last_error = None;
        }
        Ok(())
    }

    #[cfg(test)]
    pub async fn set_test_config_reconcile_delay(&self, delay: Option<std::time::Duration>) {
        let key = self as *const _ as usize;
        let mut delays = config_reconcile_delay_map()
            .lock()
            .expect("config reconcile delay map mutex poisoned");
        if let Some(delay) = delay {
            delays.insert(key, delay);
        } else {
            delays.remove(&key);
        }
    }

    #[cfg(test)]
    pub async fn set_test_config_reconcile_failure(&self, error: Option<String>) {
        let key = self as *const _ as usize;
        let mut failures = config_reconcile_failure_map()
            .lock()
            .expect("config reconcile failure map mutex poisoned");
        if let Some(error) = error {
            failures.insert(key, error);
        } else {
            failures.remove(&key);
        }
    }

    #[cfg(test)]
    async fn take_test_config_reconcile_delay(&self) -> Option<std::time::Duration> {
        let key = self as *const _ as usize;
        config_reconcile_delay_map()
            .lock()
            .expect("config reconcile delay map mutex poisoned")
            .remove(&key)
    }

    #[cfg(test)]
    async fn take_test_config_reconcile_failure(&self) -> Option<String> {
        let key = self as *const _ as usize;
        config_reconcile_failure_map()
            .lock()
            .expect("config reconcile failure map mutex poisoned")
            .remove(&key)
    }

    pub async fn set_provider_model_json(
        &self,
        provider_id: &str,
        model: &str,
    ) -> Result<AgentConfig> {
        let updated = self.prepare_provider_model_json(provider_id, model).await?;
        self.persist_prepared_provider_model_json(updated.clone())
            .await;
        self.reconcile_config_runtime_after_commit().await?;
        Ok(updated)
    }

    pub async fn persist_prepared_provider_model_json(&self, merged: AgentConfig) {
        let mut value =
            serde_json::to_value(&merged).unwrap_or_else(|_| Value::Object(Default::default()));
        normalize_config_keys_to_snake_case(&mut value);
        sanitize_config_value(&mut value);
        let mut items = Vec::new();
        flatten_config_value_to_items(&value, "", &mut items);
        if let Err(error) = self.history.replace_agent_config_items(&items).await {
            tracing::warn!("failed to persist agent config to sqlite: {error}");
        }
        *self.config.write().await = merged;
        self.config_notify.notify_waiters();
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
        let selection = super::provider_resolution::resolve_provider_model_switch(
            &current,
            provider_id,
            model,
        )?;

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
                if def.id == "github-copilot" {
                    let resolved = super::copilot_auth::resolve_github_copilot_auth(
                        &pc.api_key,
                        pc.auth_source,
                    );
                    (
                        resolved.is_some(),
                        resolved
                            .as_ref()
                            .map(|auth| auth.auth_source)
                            .unwrap_or(pc.auth_source),
                        pc.model.clone(),
                        pc.base_url.clone(),
                    )
                } else if def.id == "openai" && pc.auth_source == AuthSource::ChatgptSubscription {
                    (
                        super::llm_client::has_openai_chatgpt_subscription_auth(),
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
                if def.id == "github-copilot" {
                    let resolved = super::copilot_auth::resolve_github_copilot_auth(
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
                } else if def.id == "openai"
                    && config.auth_source == AuthSource::ChatgptSubscription
                {
                    (
                        super::llm_client::has_openai_chatgpt_subscription_auth(),
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
            } else if def.id == "github-copilot" {
                let resolved =
                    super::copilot_auth::resolve_github_copilot_auth("", AuthSource::ApiKey);
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
#[path = "tests/config.rs"]
mod tests;
