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
    assert_eq!(
        updated.managed_execution.security_level,
        SecurityLevel::Yolo
    );

    let persisted_items = engine
        .history
        .list_agent_config_items()
        .await
        .expect("persisted config should be readable");
    let rehydrated =
        load_config_from_items(persisted_items).expect("persisted config should deserialize");
    assert_eq!(
        rehydrated.managed_execution.security_level,
        SecurityLevel::Yolo
    );
}

#[tokio::test]
async fn prepare_config_item_json_validates_without_mutating_runtime_config() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

    let (prepared_config, prepared_value) = engine
        .prepare_config_item_json("/managed_execution/security_level", r#""yolo""#)
        .await
        .expect("config item preparation should succeed");

    assert_eq!(prepared_value, serde_json::json!("yolo"));
    assert_eq!(
        prepared_config.managed_execution.security_level,
        SecurityLevel::Yolo
    );

    let current = engine.get_config().await;
    assert_eq!(
        current.managed_execution.security_level,
        SecurityLevel::Lowest
    );
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

#[tokio::test]
async fn prepare_provider_model_json_validates_without_mutating_runtime_config() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

    let mut config = engine.get_config().await;
    config.api_key = "sk-test".to_string();
    engine.set_config(config).await;

    let prepared = engine
        .prepare_provider_model_json("openai", "gpt-5.4-mini")
        .await
        .expect("provider/model preparation should succeed");

    assert_eq!(prepared.provider, "openai");
    assert_eq!(prepared.model, "gpt-5.4-mini");

    let current = engine.get_config().await;
    assert_ne!(current.model, "gpt-5.4-mini");
}

#[tokio::test]
async fn persisted_config_is_visible_while_runtime_reconcile_is_still_in_flight() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

    engine
        .set_test_config_reconcile_delay(Some(std::time::Duration::from_secs(1)))
        .await;

    let (prepared, value) = engine
        .prepare_config_item_json("/managed_execution/security_level", r#""yolo""#)
        .await
        .expect("config item preparation should succeed");

    engine
        .persist_prepared_config_item_json("/managed_execution/security_level", &value, prepared)
        .await
        .expect("persist prepared config item");

    let reconcile = {
        let engine = engine.clone();
        tokio::spawn(async move {
            engine.reconcile_config_runtime_after_commit().await;
        })
    };

    tokio::task::yield_now().await;

    let desired = engine.get_config().await;
    assert_eq!(
        desired.managed_execution.security_level,
        SecurityLevel::Yolo
    );

    let projection = engine.current_config_runtime_projection().await;
    assert_eq!(projection.state, ConfigReconcileState::Reconciling);
    assert!(projection.effective_revision < projection.desired_revision);

    reconcile.await.expect("reconcile join should succeed");

    let settled = engine.current_config_runtime_projection().await;
    assert_eq!(settled.state, ConfigReconcileState::Applied);
    assert_eq!(settled.effective_revision, settled.desired_revision);
}

#[tokio::test]
async fn reconcile_failure_marks_projection_error_without_reverting_desired_config() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

    engine
        .set_test_config_reconcile_failure(Some("forced reconcile failure".to_string()))
        .await;

    let (prepared, value) = engine
        .prepare_config_item_json("/managed_execution/security_level", r#""yolo""#)
        .await
        .expect("config item preparation should succeed");

    engine
        .persist_prepared_config_item_json("/managed_execution/security_level", &value, prepared)
        .await
        .expect("persist prepared config item");

    let result = engine.reconcile_config_runtime_after_commit().await;
    assert!(result.is_err(), "reconcile should fail under the test hook");

    let desired = engine.get_config().await;
    assert_eq!(
        desired.managed_execution.security_level,
        SecurityLevel::Yolo
    );

    let projection = engine.current_config_runtime_projection().await;
    assert_eq!(projection.state, ConfigReconcileState::Error);
    assert_eq!(
        projection.effective_revision + 1,
        projection.desired_revision
    );
    assert_eq!(
        projection.last_error.as_deref(),
        Some("forced reconcile failure")
    );
}

#[tokio::test]
async fn gateway_reconcile_without_connected_runtime_marks_projection_degraded() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

    let mut config = engine.get_config().await;
    config.gateway.enabled = true;
    config.gateway.slack_token = "xoxb-test".to_string();
    engine.set_config(config).await;

    let (prepared, value) = engine
        .prepare_config_item_json("/gateway/command_prefix", r#""!tamux""#)
        .await
        .expect("config item preparation should succeed");

    engine
        .persist_prepared_config_item_json("/gateway/command_prefix", &value, prepared)
        .await
        .expect("persist prepared config item");

    engine
        .reconcile_config_runtime_after_commit()
        .await
        .expect("reconcile should degrade, not fail");

    let desired = engine.get_config().await;
    assert_eq!(desired.gateway.command_prefix, "!tamux");

    let projection = engine.current_config_runtime_projection().await;
    assert_eq!(projection.state, ConfigReconcileState::Degraded);
    assert!(projection.effective_revision < projection.desired_revision);
    assert!(projection
        .last_error
        .as_deref()
        .unwrap_or_default()
        .contains("reload"));
}

#[tokio::test]
async fn desired_and_effective_config_surfaces_diverge_during_reconcile() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

    engine
        .set_test_config_reconcile_delay(Some(std::time::Duration::from_secs(1)))
        .await;

    let (prepared, value) = engine
        .prepare_config_item_json("/managed_execution/security_level", r#""yolo""#)
        .await
        .expect("config item preparation should succeed");

    engine
        .persist_prepared_config_item_json("/managed_execution/security_level", &value, prepared)
        .await
        .expect("persist prepared config item");

    let reconcile = {
        let engine = engine.clone();
        tokio::spawn(async move {
            let _ = engine.reconcile_config_runtime_after_commit().await;
        })
    };

    tokio::task::yield_now().await;

    let desired = engine.current_desired_config_snapshot().await;
    let effective = engine.current_effective_config_runtime_state().await;

    assert_eq!(
        desired.managed_execution.security_level,
        SecurityLevel::Yolo
    );
    assert_eq!(effective.reconcile.state, ConfigReconcileState::Reconciling);
    assert!(effective.reconcile.effective_revision < effective.reconcile.desired_revision);

    reconcile.await.expect("reconcile join should succeed");
}

#[tokio::test]
async fn startup_silently_rederives_degraded_effective_state_from_desired_gateway_config() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.gateway.enabled = true;
    config.gateway.slack_token = "xoxb-test".to_string();

    let engine = AgentEngine::new_test(manager, config, root.path()).await;

    let desired = engine.current_desired_config_snapshot().await;
    let effective = engine.current_effective_config_runtime_state().await;

    assert!(desired.gateway.enabled);
    assert_eq!(effective.reconcile.state, ConfigReconcileState::Degraded);
    assert!(effective.reconcile.effective_revision < effective.reconcile.desired_revision);
    assert!(!effective.gateway_runtime_connected);
    assert!(effective
        .reconcile
        .last_error
        .as_deref()
        .unwrap_or_default()
        .contains("startup"));
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
