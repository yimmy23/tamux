// Local wire type copies (will be replaced by crate::wire imports in Task 9)
#![allow(dead_code)]

#[derive(Debug, Clone, Default)]
pub struct FetchedModel {
    pub id: String,
    pub name: Option<String>,
    pub context_window: Option<u32>,
}

#[derive(Debug, Clone, Default)]
pub struct AgentConfigSnapshot {
    pub provider: String,
    pub base_url: String,
    pub model: String,
    pub custom_model_name: String,
    pub api_key: String,
    pub assistant_id: String,
    pub auth_source: String,
    pub api_transport: String,
    pub reasoning_effort: String,
    pub context_window_tokens: u32,
}

// ── ConfigAction ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum ConfigAction {
    ConfigReceived(AgentConfigSnapshot),
    ConfigRawReceived(serde_json::Value),
    ModelsFetched(Vec<FetchedModel>),
    SetProvider(String),
    SetModel(String),
    SetReasoningEffort(String),
    ToggleTool(String), // toggle tool by name: "bash", "file_ops", "web_search", etc.
}

// ── ConfigState ───────────────────────────────────────────────────────────────

pub struct ConfigState {
    pub provider: String,
    pub base_url: String,
    pub model: String,
    pub custom_model_name: String,
    pub api_key: String,
    pub assistant_id: String,
    pub auth_source: String,
    pub api_transport: String,
    pub reasoning_effort: String,
    pub custom_context_window_tokens: Option<u32>,
    pub chatgpt_auth_available: bool,
    pub chatgpt_auth_source: Option<String>,
    pub fetched_models: Vec<FetchedModel>,
    pub agent_config_raw: Option<serde_json::Value>,

    // Tool toggles
    pub tool_bash: bool,
    pub tool_file_ops: bool,
    pub tool_web_search: bool,
    pub tool_web_browse: bool,
    pub tool_vision: bool,
    pub tool_system_info: bool,
    pub tool_gateway: bool,

    // Web search config
    pub search_provider: String, // "none", "firecrawl", "exa", "tavily"
    pub firecrawl_api_key: String,
    pub exa_api_key: String,
    pub tavily_api_key: String,
    pub search_max_results: u32,
    pub search_timeout_secs: u32,

    // Gateway config
    pub gateway_enabled: bool,
    pub gateway_prefix: String,
    pub slack_token: String,
    pub slack_channel_filter: String,
    pub telegram_token: String,
    pub telegram_allowed_chats: String,
    pub discord_token: String,
    pub discord_channel_filter: String,
    pub discord_allowed_users: String,
    pub whatsapp_allowed_contacts: String,
    pub whatsapp_token: String,
    pub whatsapp_phone_id: String,

    // Chat settings
    pub enable_streaming: bool,
    pub enable_conversation_memory: bool,
    pub enable_honcho_memory: bool,
    pub honcho_api_key: String,
    pub honcho_base_url: String,
    pub honcho_workspace_id: String,
    pub anticipatory_enabled: bool,
    pub anticipatory_morning_brief: bool,
    pub anticipatory_predictive_hydration: bool,
    pub anticipatory_stuck_detection: bool,
    pub operator_model_enabled: bool,
    pub operator_model_allow_message_statistics: bool,
    pub operator_model_allow_approval_learning: bool,
    pub operator_model_allow_attention_tracking: bool,
    pub operator_model_allow_implicit_feedback: bool,
    pub collaboration_enabled: bool,
    pub compliance_mode: String,
    pub compliance_retention_days: u32,
    pub compliance_sign_all_events: bool,
    pub tool_synthesis_enabled: bool,
    pub tool_synthesis_require_activation: bool,
    pub tool_synthesis_max_generated_tools: u32,
    pub managed_sandbox_enabled: bool,
    pub managed_security_level: String,

    // Advanced settings
    pub auto_compact_context: bool,
    pub max_context_messages: u32,
    pub max_tool_loops: u32,
    pub max_retries: u32,
    pub retry_delay_ms: u32,
    pub context_window_tokens: u32,
    pub context_budget_tokens: u32,
    pub compact_threshold_pct: u32,
    pub keep_recent_on_compact: u32,
    pub bash_timeout_secs: u32,

    // Snapshot retention settings
    pub snapshot_max_count: u32,
    pub snapshot_max_size_mb: u32,
    pub snapshot_auto_cleanup: bool,

    // Snapshot stats (read-only, refreshed periodically)
    pub snapshot_count: usize,
    pub snapshot_total_size_bytes: u64,
}

impl ConfigState {
    pub fn new() -> Self {
        Self {
            provider: "openai".to_string(),
            base_url: "https://api.openai.com/v1".to_string(),
            model: "gpt-5.4".to_string(),
            custom_model_name: String::new(),
            api_key: String::new(),
            assistant_id: String::new(),
            auth_source: "api_key".to_string(),
            api_transport: "responses".to_string(),
            reasoning_effort: String::new(),
            custom_context_window_tokens: None,
            chatgpt_auth_available: false,
            chatgpt_auth_source: None,
            fetched_models: Vec::new(),
            agent_config_raw: None,
            tool_bash: true,
            tool_file_ops: true,
            tool_web_search: true,
            tool_web_browse: false,
            tool_vision: false,
            tool_system_info: true,
            tool_gateway: false,
            search_provider: "none".to_string(),
            firecrawl_api_key: String::new(),
            exa_api_key: String::new(),
            tavily_api_key: String::new(),
            search_max_results: 8,
            search_timeout_secs: 20,
            gateway_enabled: false,
            gateway_prefix: "!tamux".to_string(),
            slack_token: String::new(),
            slack_channel_filter: String::new(),
            telegram_token: String::new(),
            telegram_allowed_chats: String::new(),
            discord_token: String::new(),
            discord_channel_filter: String::new(),
            discord_allowed_users: String::new(),
            whatsapp_allowed_contacts: String::new(),
            whatsapp_token: String::new(),
            whatsapp_phone_id: String::new(),
            enable_streaming: true,
            enable_conversation_memory: true,
            enable_honcho_memory: false,
            honcho_api_key: String::new(),
            honcho_base_url: String::new(),
            honcho_workspace_id: "tamux".to_string(),
            anticipatory_enabled: false,
            anticipatory_morning_brief: false,
            anticipatory_predictive_hydration: false,
            anticipatory_stuck_detection: false,
            operator_model_enabled: false,
            operator_model_allow_message_statistics: false,
            operator_model_allow_approval_learning: false,
            operator_model_allow_attention_tracking: false,
            operator_model_allow_implicit_feedback: false,
            collaboration_enabled: false,
            compliance_mode: "standard".to_string(),
            compliance_retention_days: 30,
            compliance_sign_all_events: false,
            tool_synthesis_enabled: false,
            tool_synthesis_require_activation: true,
            tool_synthesis_max_generated_tools: 24,
            managed_sandbox_enabled: false,
            managed_security_level: "lowest".to_string(),
            auto_compact_context: true,
            max_context_messages: 100,
            max_tool_loops: 25,
            max_retries: 3,
            retry_delay_ms: 2000,
            context_window_tokens: 128_000,
            context_budget_tokens: 100000,
            compact_threshold_pct: 80,
            keep_recent_on_compact: 10,
            bash_timeout_secs: 30,
            snapshot_max_count: 10,
            snapshot_max_size_mb: 51_200,
            snapshot_auto_cleanup: true,
            snapshot_count: 0,
            snapshot_total_size_bytes: 0,
        }
    }

    pub fn provider(&self) -> &str {
        &self.provider
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    pub fn model(&self) -> &str {
        &self.model
    }

    pub fn api_key(&self) -> &str {
        &self.api_key
    }

    pub fn reasoning_effort(&self) -> &str {
        &self.reasoning_effort
    }

    pub fn api_transport(&self) -> &str {
        &self.api_transport
    }

    pub fn fetched_models(&self) -> &[FetchedModel] {
        &self.fetched_models
    }

    pub fn agent_config_raw(&self) -> Option<&serde_json::Value> {
        self.agent_config_raw.as_ref()
    }

    pub fn reduce(&mut self, action: ConfigAction) {
        match action {
            ConfigAction::ConfigReceived(snapshot) => {
                self.provider = snapshot.provider;
                self.base_url = snapshot.base_url;
                self.model = snapshot.model;
                self.custom_model_name.clear();
                self.api_key = snapshot.api_key;
                self.assistant_id = snapshot.assistant_id;
                self.auth_source = snapshot.auth_source;
                self.api_transport = snapshot.api_transport;
                self.reasoning_effort = snapshot.reasoning_effort;
                self.context_window_tokens = snapshot.context_window_tokens;
            }

            ConfigAction::ConfigRawReceived(raw) => {
                self.agent_config_raw = Some(raw);
            }

            ConfigAction::ModelsFetched(models) => {
                self.fetched_models = models;
            }

            ConfigAction::SetProvider(provider) => {
                self.provider = provider;
            }

            ConfigAction::SetModel(model) => {
                self.model = model;
            }

            ConfigAction::SetReasoningEffort(effort) => {
                self.reasoning_effort = effort;
            }
            ConfigAction::ToggleTool(name) => match name.as_str() {
                "bash" => self.tool_bash = !self.tool_bash,
                "file_ops" => self.tool_file_ops = !self.tool_file_ops,
                "web_search" => self.tool_web_search = !self.tool_web_search,
                "web_browse" => self.tool_web_browse = !self.tool_web_browse,
                "vision" => self.tool_vision = !self.tool_vision,
                "system_info" => self.tool_system_info = !self.tool_system_info,
                "gateway" => self.tool_gateway = !self.tool_gateway,
                _ => {}
            },
        }
    }
}

impl Default for ConfigState {
    fn default() -> Self {
        Self::new()
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_snapshot(provider: &str, model: &str) -> AgentConfigSnapshot {
        AgentConfigSnapshot {
            provider: provider.into(),
            model: model.into(),
            custom_model_name: String::new(),
            base_url: "https://api.example.com".into(),
            api_key: "sk-test".into(),
            assistant_id: "asst_test".into(),
            auth_source: "api_key".into(),
            reasoning_effort: "high".into(),
            api_transport: "responses".into(),
            context_window_tokens: 128_000,
        }
    }

    #[test]
    fn config_received_populates_all_fields() {
        let mut state = ConfigState::new();
        state.reduce(ConfigAction::ConfigReceived(make_snapshot(
            "openai", "gpt-4o",
        )));
        assert_eq!(state.provider(), "openai");
        assert_eq!(state.model(), "gpt-4o");
        assert_eq!(state.base_url(), "https://api.example.com");
        assert_eq!(state.api_key(), "sk-test");
        assert_eq!(state.reasoning_effort(), "high");
    }

    #[test]
    fn models_fetched_replaces_list() {
        let mut state = ConfigState::new();
        state.reduce(ConfigAction::ModelsFetched(vec![
            FetchedModel {
                id: "m1".into(),
                name: Some("Model One".into()),
                context_window: Some(128_000),
            },
            FetchedModel {
                id: "m2".into(),
                name: None,
                context_window: None,
            },
        ]));
        assert_eq!(state.fetched_models().len(), 2);
        assert_eq!(state.fetched_models()[0].id, "m1");

        state.reduce(ConfigAction::ModelsFetched(vec![]));
        assert_eq!(state.fetched_models().len(), 0);
    }

    #[test]
    fn set_provider_updates_only_provider() {
        let mut state = ConfigState::new();
        state.reduce(ConfigAction::ConfigReceived(make_snapshot(
            "openai", "gpt-4o",
        )));
        state.reduce(ConfigAction::SetProvider("anthropic".into()));
        assert_eq!(state.provider(), "anthropic");
        // Other fields unchanged
        assert_eq!(state.model(), "gpt-4o");
        assert_eq!(state.api_key(), "sk-test");
    }

    #[test]
    fn set_model_updates_only_model() {
        let mut state = ConfigState::new();
        state.reduce(ConfigAction::ConfigReceived(make_snapshot(
            "openai", "gpt-4o",
        )));
        state.reduce(ConfigAction::SetModel("gpt-4o-mini".into()));
        assert_eq!(state.model(), "gpt-4o-mini");
        assert_eq!(state.provider(), "openai");
    }

    #[test]
    fn config_raw_received_stores_json() {
        let mut state = ConfigState::new();
        assert!(state.agent_config_raw().is_none());

        let raw = serde_json::json!({ "key": "value", "number": 42 });
        state.reduce(ConfigAction::ConfigRawReceived(raw.clone()));
        assert!(state.agent_config_raw().is_some());
        assert_eq!(state.agent_config_raw().unwrap()["key"], "value");
    }

    #[test]
    fn config_received_twice_overwrites() {
        let mut state = ConfigState::new();
        state.reduce(ConfigAction::ConfigReceived(make_snapshot(
            "openai", "gpt-4o",
        )));
        state.reduce(ConfigAction::ConfigReceived(make_snapshot(
            "anthropic",
            "claude-3-5-sonnet",
        )));
        assert_eq!(state.provider(), "anthropic");
        assert_eq!(state.model(), "claude-3-5-sonnet");
    }
}
