use super::*;
use crate::session_manager::SessionManager;
use amux_protocol::SecurityLevel;
use std::ffi::OsString;
use tempfile::tempdir;

struct EnvGuard {
    saved: Vec<(&'static str, Option<OsString>)>,
}

impl EnvGuard {
    fn new(keys: &[&'static str]) -> Self {
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

#[tokio::test]
async fn merge_config_patch_preserves_whatsapp_link_fallback_flag() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

    let baseline = engine.get_config().await;
    assert!(!baseline.gateway.whatsapp_link_fallback_electron);

    engine
        .merge_config_patch_json(
            r#"{
                "gateway": {
                    "whatsapp_link_fallback_electron": true
                }
            }"#,
        )
        .await
        .unwrap();

    let updated = engine.get_config().await;
    assert!(updated.gateway.whatsapp_link_fallback_electron);
}

#[tokio::test]
async fn whatsapp_link_fallback_flag_persists_across_sqlite_roundtrip() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

    let baseline_items = engine
        .history
        .list_agent_config_items()
        .await
        .expect("baseline config should be readable");
    let baseline_loaded =
        load_config_from_items(baseline_items).expect("baseline config should deserialize");
    assert!(
        !baseline_loaded.gateway.whatsapp_link_fallback_electron,
        "default should remain false when unset"
    );

    engine
        .merge_config_patch_json(
            r#"{
                "gateway": {
                    "whatsapp_link_fallback_electron": true
                }
            }"#,
        )
        .await
        .expect("patch should persist fallback flag");

    let persisted_items = engine
        .history
        .list_agent_config_items()
        .await
        .expect("persisted config should be readable");
    let rehydrated =
        load_config_from_items(persisted_items).expect("persisted config should deserialize");
    assert!(
        rehydrated.gateway.whatsapp_link_fallback_electron,
        "override should survive sqlite write/read roundtrip"
    );
}

#[tokio::test]
async fn set_provider_model_json_updates_provider_and_model_atomically() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

    let mut config = engine.get_config().await;
    config.api_key = "sk-test".to_string();
    engine.set_config(config).await;

    engine
        .set_provider_model_json("openai", "gpt-5.4-mini")
        .await
        .unwrap();

    let updated = engine.get_config().await;
    assert_eq!(updated.provider, "openai");
    assert_eq!(updated.model, "gpt-5.4-mini");
}

#[tokio::test]
async fn set_provider_model_json_restores_canonical_base_url_for_predefined_provider() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

    let mut config = engine.get_config().await;
    config.api_key = "groq-key".to_string();
    config.base_url = "https://stale.example.invalid/v1".to_string();
    engine.set_config(config).await;

    engine
        .set_provider_model_json("groq", "llama-3.3-70b-versatile")
        .await
        .unwrap();

    let updated = engine.get_config().await;
    assert_eq!(updated.provider, "groq");
    assert_eq!(updated.model, "llama-3.3-70b-versatile");
    assert_eq!(updated.base_url, "https://api.groq.com/openai/v1");
}

#[tokio::test]
async fn set_config_item_json_persists_managed_execution_security_level() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

    engine
        .set_config_item_json("/managed_execution/security_level", r#""yolo""#)
        .await
        .expect("managed execution security level should update");

    let updated = engine.get_config().await;
    assert_eq!(updated.managed_execution.security_level, SecurityLevel::Yolo);

    let persisted_items = engine
        .history
        .list_agent_config_items()
        .await
        .expect("persisted config should be readable");
    let rehydrated =
        load_config_from_items(persisted_items).expect("persisted config should deserialize");
    assert_eq!(rehydrated.managed_execution.security_level, SecurityLevel::Yolo);
}

#[tokio::test]
async fn set_provider_model_json_rejects_invalid_model_without_changing_config() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

    let mut config = engine.get_config().await;
    config.api_key = "sk-test".to_string();
    engine.set_config(config).await;

    let before = engine.get_config().await;
    let result = engine
        .set_provider_model_json("openai", "definitely-not-a-real-model")
        .await;
    assert!(result.is_err());

    let after = engine.get_config().await;
    assert_eq!(before.provider, after.provider);
    assert_eq!(before.model, after.model);
    assert_eq!(before.base_url, after.base_url);
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

#[tokio::test]
async fn copilot_auth_states_include_provider_row_when_unconfigured() {
    let _lock = crate::agent::provider_auth_store::provider_auth_test_env_lock();
    let _guard = EnvGuard::new(&[
        "TAMUX_GITHUB_COPILOT_DISABLE_GH_CLI",
        "TAMUX_PROVIDER_AUTH_DB_PATH",
        "COPILOT_GITHUB_TOKEN",
        "GITHUB_TOKEN",
        "GH_TOKEN",
    ]);
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    std::env::set_var("TAMUX_GITHUB_COPILOT_DISABLE_GH_CLI", "1");
    std::env::set_var(
        "TAMUX_PROVIDER_AUTH_DB_PATH",
        root.path().join("provider-auth.db"),
    );
    std::env::remove_var("COPILOT_GITHUB_TOKEN");
    std::env::remove_var("GITHUB_TOKEN");
    std::env::remove_var("GH_TOKEN");

    let states = engine.get_provider_auth_states().await;
    let copilot = states
        .into_iter()
        .find(|state| state.provider_id == "github-copilot")
        .expect("github copilot provider row should be present");

    assert!(!copilot.authenticated);
}
