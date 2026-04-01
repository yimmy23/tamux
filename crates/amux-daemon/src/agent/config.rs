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

const WELES_BUILTIN_ID: &str = super::agent_identity::WELES_BUILTIN_SUBAGENT_ID;
const WELES_BUILTIN_NAME: &str = "WELES";
const WELES_PROTECTED_REASON: &str = "Daemon-owned WELES registry entry";

fn is_reserved_builtin_sub_agent_id(id: &str) -> bool {
    id.eq_ignore_ascii_case(WELES_BUILTIN_ID)
}

fn is_reserved_builtin_sub_agent_name(name: &str) -> bool {
    name.eq_ignore_ascii_case(WELES_BUILTIN_NAME)
}

fn protected_mutation_error(message: impl Into<String>) -> anyhow::Error {
    anyhow::anyhow!("protected mutation: {}", message.into())
}

fn builtin_collision_error(field: &str, value: &str) -> anyhow::Error {
    anyhow::anyhow!(
        "reserved built-in sub-agent {} collision: '{}' is reserved for WELES",
        field,
        value
    )
}

fn now_millis_u64() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn resolve_weles_reasoning_effort(overrides: &WelesBuiltinOverrides) -> String {
    overrides
        .reasoning_effort
        .clone()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "medium".to_string())
}

fn resolve_main_agent_default(value: Option<String>, fallback: &str) -> String {
    value
        .filter(|candidate| !candidate.trim().is_empty())
        .unwrap_or_else(|| fallback.to_string())
}

fn sanitize_weles_operator_system_prompt(
    system_prompt: Option<String>,
    inherited_system_prompt: &str,
) -> Option<String> {
    let sanitized = system_prompt
        .map(|value| crate::agent::weles_governance::strip_weles_internal_payload_markers(&value))
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    sanitized.filter(|value| value != inherited_system_prompt)
}

fn sanitize_weles_builtin_overrides_struct(overrides: &mut WelesBuiltinOverrides, inherited_system_prompt: &str) {
    if overrides
        .role
        .as_deref()
        .is_some_and(|value| !value.trim().is_empty() && value.trim() != "governance")
    {
        overrides.role = None;
    }
    overrides.system_prompt =
        sanitize_weles_operator_system_prompt(overrides.system_prompt.take(), inherited_system_prompt);
}

fn build_effective_weles_definition(config: &AgentConfig) -> SubAgentDefinition {
    let mut overrides = config.builtin_sub_agents.weles.clone();
    sanitize_weles_builtin_overrides_struct(&mut overrides, &config.system_prompt);
    let reasoning_effort = resolve_weles_reasoning_effort(&overrides);
    let system_prompt = resolve_main_agent_default(overrides.system_prompt.take(), &config.system_prompt);
    SubAgentDefinition {
        id: WELES_BUILTIN_ID.to_string(),
        name: WELES_BUILTIN_NAME.to_string(),
        provider: resolve_main_agent_default(overrides.provider.clone(), &config.provider),
        model: resolve_main_agent_default(overrides.model.clone(), &config.model),
        role: overrides
            .role
            .clone()
            .or_else(|| Some("governance".to_string())),
        system_prompt: Some(system_prompt),
        tool_whitelist: overrides.tool_whitelist.clone(),
        tool_blacklist: overrides.tool_blacklist.clone(),
        context_budget_tokens: overrides.context_budget_tokens,
        max_duration_secs: overrides.max_duration_secs,
        supervisor_config: overrides.supervisor_config.clone(),
        enabled: true,
        builtin: true,
        immutable_identity: true,
        disable_allowed: false,
        delete_allowed: false,
        protected_reason: Some(WELES_PROTECTED_REASON.to_string()),
        reasoning_effort: Some(reasoning_effort),
        created_at: 0,
    }
}

fn is_weles_builtin_update_shape(def: &SubAgentDefinition) -> bool {
    def.id == WELES_BUILTIN_ID
        && def.name == WELES_BUILTIN_NAME
        && def.builtin
        && def.immutable_identity
        && !def.disable_allowed
        && !def.delete_allowed
        && def.protected_reason.as_deref() == Some(WELES_PROTECTED_REASON)
}

fn is_weles_builtin_target(def: &SubAgentDefinition) -> bool {
    def.id == WELES_BUILTIN_ID
}

fn is_attempted_weles_builtin_edit(def: &SubAgentDefinition) -> bool {
    def.id == WELES_BUILTIN_ID
        && (def.builtin
            || def.immutable_identity
            || !def.disable_allowed
            || !def.delete_allowed
            || def.protected_reason.is_some())
}

pub(crate) fn canonicalize_weles_client_update(def: &mut SubAgentDefinition) {
    if def.id != WELES_BUILTIN_ID {
        return;
    }
    if def.name.trim().is_empty() {
        def.name = WELES_BUILTIN_NAME.to_string();
    }
    if def.name == WELES_BUILTIN_NAME {
        if def.protected_reason.is_none() {
            def.protected_reason = Some(WELES_PROTECTED_REASON.to_string());
        }
        if !def.builtin && !def.immutable_identity && def.disable_allowed && def.delete_allowed {
            def.builtin = true;
            def.immutable_identity = true;
            def.disable_allowed = false;
            def.delete_allowed = false;
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

fn apply_weles_allowed_overrides(
    config: &mut AgentConfig,
    def: &SubAgentDefinition,
) -> Result<()> {
    if !is_weles_builtin_target(def) {
        return Err(protected_mutation_error("unexpected built-in sub-agent target"));
    }
    if def.name != WELES_BUILTIN_NAME {
        return Err(protected_mutation_error("cannot change WELES name"));
    }
    if !def.enabled {
        return Err(protected_mutation_error("cannot disable daemon-owned WELES"));
    }
    if !def.builtin || !def.immutable_identity || def.disable_allowed || def.delete_allowed {
        return Err(protected_mutation_error(
            "cannot change WELES built-in protection metadata",
        ));
    }
    if def.protected_reason.as_deref() != Some(WELES_PROTECTED_REASON) {
        return Err(protected_mutation_error("cannot change WELES protected reason"));
    }

    let inherited_provider = resolve_main_agent_default(None, &config.provider);
    let inherited_model = resolve_main_agent_default(None, &config.model);
    let inherited_role = Some("governance".to_string());
    let inherited_system_prompt = Some(resolve_main_agent_default(None, &config.system_prompt));
    let inherited_tool_whitelist = None::<Vec<String>>;
    let inherited_tool_blacklist = None::<Vec<String>>;
    let inherited_context_budget_tokens = None::<u32>;
    let inherited_max_duration_secs = None::<u64>;
    let inherited_supervisor_config = None::<SupervisorConfig>;
    let inherited_reasoning_effort = Some("medium".to_string());

    let system_prompt = sanitize_weles_operator_system_prompt(
        if def.system_prompt == inherited_system_prompt {
            None
        } else {
            def.system_prompt.clone()
        },
        &config.system_prompt,
    );

    config.builtin_sub_agents.weles = WelesBuiltinOverrides {
        provider: if def.provider == inherited_provider {
            None
        } else {
            Some(def.provider.clone()).filter(|value| !value.trim().is_empty())
        },
        model: if def.model == inherited_model {
            None
        } else {
            Some(def.model.clone()).filter(|value| !value.trim().is_empty())
        },
        role: if def.role == inherited_role {
            None
        } else {
            def.role.clone()
        },
        system_prompt,
        tool_whitelist: if def.tool_whitelist == inherited_tool_whitelist {
            None
        } else {
            def.tool_whitelist.clone()
        },
        tool_blacklist: if def.tool_blacklist == inherited_tool_blacklist {
            None
        } else {
            def.tool_blacklist.clone()
        },
        context_budget_tokens: if def.context_budget_tokens == inherited_context_budget_tokens {
            None
        } else {
            def.context_budget_tokens
        },
        max_duration_secs: if def.max_duration_secs == inherited_max_duration_secs {
            None
        } else {
            def.max_duration_secs
        },
        supervisor_config: if serde_json::to_value(&def.supervisor_config).ok()
            == serde_json::to_value(&inherited_supervisor_config).ok()
        {
            None
        } else {
            def.supervisor_config.clone()
        },
        reasoning_effort: if def.reasoning_effort == inherited_reasoning_effort {
            None
        } else {
            def.reasoning_effort.clone()
        },
    };
    Ok(())
}

fn filter_user_sub_agents_and_collect_collisions(
    config: &AgentConfig,
) -> (Vec<SubAgentDefinition>, Vec<SubAgentDefinition>) {
    let mut filtered = Vec::new();
    let mut collisions = Vec::new();
    for def in &config.sub_agents {
        if is_reserved_builtin_sub_agent_id(&def.id) || is_reserved_builtin_sub_agent_name(&def.name) {
            collisions.push(def.clone());
        } else {
            filtered.push(def.clone());
        }
    }
    (filtered, collisions)
}

pub(crate) fn effective_sub_agents_from_config(
    config: &AgentConfig,
) -> (Vec<SubAgentDefinition>, Vec<SubAgentDefinition>) {
    let (user_sub_agents, collisions) = filter_user_sub_agents_and_collect_collisions(config);
    let mut effective = user_sub_agents;
    effective.push(build_effective_weles_definition(config));
    effective.sort_by(|left, right| left.name.cmp(&right.name).then(left.id.cmp(&right.id)));
    (effective, collisions)
}

fn sanitize_weles_builtin_overrides(root: &mut serde_json::Map<String, Value>) {
    let Some(Value::Object(builtin_sub_agents)) = root.get_mut("builtin_sub_agents") else {
        return;
    };
    let Some(Value::Object(weles)) = builtin_sub_agents.get_mut("weles") else {
        return;
    };

    for forbidden_key in [
        "enabled",
        "builtin",
        "immutable_identity",
        "disable_allowed",
        "delete_allowed",
        "protected_reason",
        "tool_name",
        "tool_args",
        "security_level",
        "suspicion_reasons",
        "task_metadata",
    ] {
        weles.remove(forbidden_key);
    }

    if weles
        .get("role")
        .and_then(|value| value.as_str())
        .is_some_and(|value| !value.trim().is_empty() && value.trim() != "governance")
    {
        weles.remove("role");
    }

    if let Some(Value::String(system_prompt)) = weles.get_mut("system_prompt") {
        let sanitized = crate::agent::weles_governance::strip_weles_internal_payload_markers(system_prompt);
        *system_prompt = sanitized;
    }
}

pub(in crate::agent) fn sanitize_weles_collisions_from_config(
    config: &mut AgentConfig,
) -> Vec<SubAgentDefinition> {
    let (filtered, collisions) = filter_user_sub_agents_and_collect_collisions(config);
    config.sub_agents = filtered;
    collisions
}

pub(in crate::agent) fn config_to_items(config: &AgentConfig) -> Vec<(String, Value)> {
    let mut value = serde_json::to_value(config).unwrap_or_else(|_| Value::Object(Default::default()));
    normalize_config_keys_to_snake_case(&mut value);
    sanitize_config_value(&mut value);
    let mut items = Vec::new();
    flatten_config_value_to_items(&value, "", &mut items);
    items
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
    let (config, _) = load_config_from_items_with_weles_cleanup(items)?;
    Ok(config)
}

pub(in crate::agent) fn load_config_from_items_with_weles_cleanup(
    items: Vec<(String, Value)>,
) -> Result<(AgentConfig, Vec<SubAgentDefinition>)> {
    if items.is_empty() {
        return Ok((AgentConfig::default(), Vec::new()));
    }
    let mut root = Value::Object(Default::default());
    for (pointer, value) in items {
        set_config_value_at_pointer(&mut root, &pointer, value)?;
    }
    sanitize_config_value(&mut root);
    let mut config = serde_json::from_value(root).unwrap_or_default();
    let collisions = sanitize_weles_collisions_from_config(&mut config);
    Ok((config, collisions))
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
    sanitize_weles_builtin_overrides(root);

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

    pub(in crate::agent) async fn persist_sanitized_config(
        &self,
        config: AgentConfig,
        collisions: Vec<SubAgentDefinition>,
    ) -> AgentConfig {
        let mut config = config;
        sanitize_weles_builtin_overrides_struct(
            &mut config.builtin_sub_agents.weles,
            &config.system_prompt,
        );
        let items = config_to_items(&config);
        if let Err(error) = self.history.replace_agent_config_items(&items).await {
            tracing::warn!("failed to persist agent config to sqlite: {error}");
        }
        *self.config.write().await = config.clone();
        self.config_notify.notify_waiters();
        self.report_weles_collisions_once(&collisions).await;
        config
    }

    pub(in crate::agent) async fn store_config_snapshot(&self, config: AgentConfig) -> AgentConfig {
        let mut config = config;
        let collisions = sanitize_weles_collisions_from_config(&mut config);
        self.persist_sanitized_config(config, collisions).await
    }

    async fn audit_weles_collision(&self, def: &SubAgentDefinition) {
        let audit_entry = crate::history::AuditEntryRow {
            id: format!("audit-subagent-weles-collision-{}", uuid::Uuid::new_v4()),
            timestamp: now_millis_u64() as i64,
            action_type: "subagent".to_string(),
            summary: format!(
                "Excluded legacy WELES collision from effective registry: {} ({})",
                def.name, def.id
            ),
            explanation: Some(
                "A persisted user subagent collided with the daemon-owned WELES registry entry and was excluded."
                    .to_string(),
            ),
            confidence: None,
            confidence_band: None,
            causal_trace_id: None,
            thread_id: None,
            goal_run_id: None,
            task_id: None,
            raw_data_json: Some(
                serde_json::json!({
                    "collision_id": def.id,
                    "collision_name": def.name,
                    "reserved_id": WELES_BUILTIN_ID,
                    "reserved_name": WELES_BUILTIN_NAME,
                })
                .to_string(),
            ),
        };
        if let Err(error) = self.history.insert_action_audit(&audit_entry).await {
            tracing::warn!(error = %error, sub_agent_id = %def.id, "failed to persist WELES collision audit entry");
        }
    }

    async fn report_weles_collisions_once(&self, collisions: &[SubAgentDefinition]) {
        for collision in collisions {
            tracing::warn!(
                sub_agent_id = %collision.id,
                sub_agent_name = %collision.name,
                "excluding legacy WELES collision from effective registry"
            );
            self.audit_weles_collision(collision).await;
        }
    }

    pub(crate) async fn effective_sub_agents(&self) -> Vec<SubAgentDefinition> {
        let config = self.config.read().await;
        let (effective, _) = effective_sub_agents_from_config(&config);
        effective
    }

    pub async fn get_config(&self) -> AgentConfig {
        self.config.read().await.clone()
    }

    pub async fn set_config(&self, config: AgentConfig) {
        self.store_config_snapshot(config).await;
    }

    pub async fn get_sub_agent(&self, id: &str) -> Option<SubAgentDefinition> {
        self.list_sub_agents()
            .await
            .into_iter()
            .find(|entry| entry.id == id)
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
        Ok(self.get_config().await)
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

        let mut merged = merged;
        let collisions = sanitize_weles_collisions_from_config(&mut merged);
        self.persist_sanitized_config(merged, collisions).await;

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
    pub async fn set_sub_agent(&self, def: SubAgentDefinition) -> Result<()> {
        if is_attempted_weles_builtin_edit(&def) || is_weles_builtin_update_shape(&def)
        {
            {
                let mut config = self.config.write().await;
                apply_weles_allowed_overrides(&mut config, &def)?;
            }
            self.persist_config().await;
            return Ok(());
        }

        if is_reserved_builtin_sub_agent_id(&def.id) {
            return Err(builtin_collision_error("id", &def.id));
        }
        if is_reserved_builtin_sub_agent_name(&def.name) {
            return Err(builtin_collision_error("name", &def.name));
        }

        let mut config = self.config.write().await;
        if let Some(existing) = config.sub_agents.iter_mut().find(|s| s.id == def.id) {
            *existing = def;
        } else {
            config.sub_agents.push(def);
        }
        drop(config);
        self.persist_config().await;
        Ok(())
    }

    /// Remove a sub-agent definition by id.
    pub async fn remove_sub_agent(&self, id: &str) -> Result<bool> {
        if is_reserved_builtin_sub_agent_id(id) {
            return Err(protected_mutation_error("cannot remove daemon-owned WELES"));
        }
        let mut config = self.config.write().await;
        let before = config.sub_agents.len();
        config.sub_agents.retain(|s| s.id != id);
        let removed = config.sub_agents.len() < before;
        drop(config);
        if removed {
            self.persist_config().await;
        }
        Ok(removed)
    }

    /// List all sub-agent definitions.
    pub async fn list_sub_agents(&self) -> Vec<SubAgentDefinition> {
        self.effective_sub_agents().await
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
