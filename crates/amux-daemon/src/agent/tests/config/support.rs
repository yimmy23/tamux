use super::*;
use amux_shared::providers::PROVIDER_ID_OPENAI;
use std::ffi::OsString;

pub(super) struct EnvGuard {
    saved: Vec<(&'static str, Option<OsString>)>,
}

impl EnvGuard {
    pub(super) fn new(keys: &[&'static str]) -> Self {
        Self {
            saved: keys
                .iter()
                .map(|key| (*key, std::env::var_os(key)))
                .collect(),
        }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        for (key, value) in &self.saved {
            match value {
                Some(value) => std::env::set_var(key, value),
                None => std::env::remove_var(key),
            }
        }
    }
}

pub(super) fn test_user_sub_agent(id: &str, name: &str) -> SubAgentDefinition {
    SubAgentDefinition {
        id: id.to_string(),
        name: name.to_string(),
        provider: PROVIDER_ID_OPENAI.to_string(),
        model: "gpt-5.4-mini".to_string(),
        role: Some("specialist".to_string()),
        system_prompt: Some("Handle delegated work.".to_string()),
        tool_whitelist: None,
        tool_blacklist: None,
        context_budget_tokens: None,
        max_duration_secs: None,
        supervisor_config: None,
        enabled: true,
        builtin: false,
        immutable_identity: false,
        disable_allowed: true,
        delete_allowed: true,
        protected_reason: None,
        reasoning_effort: None,
        created_at: 1_712_000_010,
    }
}

pub(super) fn stale_weles_collision_config() -> AgentConfig {
    let mut config = AgentConfig::default();
    config.sub_agents = vec![
        test_user_sub_agent("weles_builtin", "Legacy WELES"),
        test_user_sub_agent("legacy-shadow", "WELES"),
        test_user_sub_agent("reviewer", "Reviewer"),
    ];
    config
}

pub(super) async fn replace_raw_config_items(engine: &Arc<AgentEngine>, config: &AgentConfig) {
    let mut value = serde_json::to_value(config).expect("config should serialize");
    normalize_config_keys_to_snake_case(&mut value);
    sanitize_config_value(&mut value);
    let mut items = Vec::new();
    flatten_config_value_to_items(&value, "", &mut items);
    engine
        .history
        .replace_agent_config_items(&items)
        .await
        .expect("raw config items should persist");
}

pub(super) async fn persisted_sub_agent_ids(engine: &Arc<AgentEngine>) -> Vec<String> {
    let mut ids = engine
        .history
        .list_agent_config_items()
        .await
        .expect("persisted config items should be readable")
        .into_iter()
        .find_map(|(key_path, value)| {
            (key_path == "/sub_agents").then(|| {
                value
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|entry| entry.get("id").and_then(Value::as_str).map(str::to_string))
                    .collect::<Vec<_>>()
            })
        })
        .unwrap_or_default();
    ids.sort();
    ids
}

pub(super) async fn weles_collision_audit_count(engine: &Arc<AgentEngine>) -> usize {
    engine
        .history
        .list_action_audit(None, None, 50)
        .await
        .expect("audit query should succeed")
        .iter()
        .filter(|entry| entry.action_type == "subagent" && entry.summary.contains("collision"))
        .count()
}
