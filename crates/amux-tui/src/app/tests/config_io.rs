use super::*;
use std::sync::mpsc;
use tokio::sync::mpsc::unbounded_channel;

fn make_model() -> TuiModel {
    let (_client_tx, client_rx) = mpsc::channel();
    let (daemon_tx, _daemon_rx) = unbounded_channel();
    TuiModel::new(client_rx, daemon_tx)
}

#[test]
fn normalize_provider_auth_source_falls_back_for_invalid_values() {
    assert_eq!(
        normalize_provider_auth_source("openai", "bogus"),
        "api_key".to_string()
    );
    assert_eq!(
        normalize_provider_auth_source("openai", "chatgpt_subscription"),
        "chatgpt_subscription".to_string()
    );
}

#[test]
fn normalize_provider_transport_falls_back_for_invalid_values() {
    assert_eq!(
        normalize_provider_transport("minimax-coding-plan", "bogus"),
        "chat_completions".to_string()
    );
    assert_eq!(
        normalize_provider_transport("openai", "responses"),
        "responses".to_string()
    );
}

#[test]
fn normalize_compliance_mode_falls_back_to_standard() {
    assert_eq!(normalize_compliance_mode("soc2"), "soc2".to_string());
    assert_eq!(normalize_compliance_mode("bogus"), "standard".to_string());
}

#[test]
fn build_config_patch_value_covers_all_daemon_backed_tabs() {
    let mut model = make_model();
    model.config.provider = "openai".to_string();
    model.config.base_url = "https://example.invalid/v1".to_string();
    model.config.model = "gpt-5.4-mini".to_string();
    model.config.api_key = "sk-live".to_string();
    model.config.assistant_id = "asst_123".to_string();
    model.config.auth_source = "api_key".to_string();
    model.config.api_transport = "responses".to_string();
    model.config.reasoning_effort = "high".to_string();
    model.config.tool_bash = false;
    model.config.tool_web_search = true;
    model.config.search_provider = "exa".to_string();
    model.config.search_max_results = 12;
    model.config.search_timeout_secs = 45;
    model.config.enable_conversation_memory = false;
    model.config.operator_model_enabled = true;
    model.config.collaboration_enabled = true;
    model.config.compliance_sign_all_events = true;
    model.config.gateway_enabled = true;
    model.config.gateway_prefix = "/tamux".to_string();
    model.config.slack_channel_filter = "ops,alerts".to_string();
    model.config.telegram_allowed_chats = "1,2".to_string();
    model.config.discord_allowed_users = "alice,bob".to_string();
    model.config.whatsapp_phone_id = "phone-1".to_string();
    model.config.max_context_messages = 123;
    model.config.context_budget_tokens = 222_000;
    model.config.snapshot_max_count = 15;
    model.config.agent_config_raw = Some(serde_json::json!({
        "agent_name": "Tamux",
        "system_prompt": "be sharp",
        "agent_backend": "daemon"
    }));

    let json = model.build_config_patch_value();

    assert_eq!(json["agent_name"], "Tamux");
    assert_eq!(json["system_prompt"], "be sharp");
    assert_eq!(json["provider"], "openai");
    assert_eq!(json["providers"]["openai"]["assistant_id"], "asst_123");
    assert!(json["openai"].get("api_key").is_none());
    assert!(json["providers"]["openai"].get("api_key").is_none());
    assert_eq!(json["tools"]["bash"], false);
    assert_eq!(json["search_provider"], "exa");
    assert_eq!(json["enable_conversation_memory"], false);
    assert_eq!(json["operator_model"]["enabled"], true);
    assert_eq!(json["collaboration"]["enabled"], true);
    assert_eq!(json["compliance"]["sign_all_events"], true);
    assert_eq!(json["gateway"]["slack_channel_filter"], "ops,alerts");
    assert_eq!(json["gateway"]["telegram_allowed_chats"], "1,2");
    assert_eq!(json["gateway"]["discord_allowed_users"], "alice,bob");
    assert_eq!(json["gateway"]["whatsapp_phone_id"], "phone-1");
    assert_eq!(json["max_context_messages"], 123);
    assert_eq!(json["context_budget_tokens"], 222000);
    assert_eq!(json["snapshot_retention"]["max_snapshots"], 15);
}

#[test]
fn apply_config_json_prefers_active_provider_transport_over_stale_root_transport() {
    let mut model = make_model();

    model.apply_config_json(&serde_json::json!({
        "provider": "github-copilot",
        "api_transport": "chat_completions",
        "providers": {
            "github-copilot": {
                "base_url": "https://api.githubcopilot.com",
                "model": "gpt-5.4",
                "api_transport": "responses",
                "auth_source": "github_copilot"
            }
        }
    }));

    assert_eq!(model.config.provider, "github-copilot");
    assert_eq!(model.config.api_transport, "responses");
}
