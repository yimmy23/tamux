use super::*;
use std::sync::mpsc;
use tokio::sync::mpsc::unbounded_channel;

fn make_model() -> TuiModel {
    let (_client_tx, client_rx) = mpsc::channel();
    let (daemon_tx, _daemon_rx) = unbounded_channel();
    TuiModel::new(client_rx, daemon_tx)
}

fn make_model_with_daemon_rx() -> (
    TuiModel,
    tokio::sync::mpsc::UnboundedReceiver<DaemonCommand>,
) {
    let (_client_tx, client_rx) = mpsc::channel();
    let (daemon_tx, daemon_rx) = unbounded_channel();
    (TuiModel::new(client_rx, daemon_tx), daemon_rx)
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
    model.config.auto_compact_context = false;
    model.config.max_context_messages = 123;
    model.config.max_tool_loops = 44;
    model.config.max_retries = 7;
    model.config.retry_delay_ms = 9_000;
    model.config.message_loop_delay_ms = 250;
    model.config.tool_call_delay_ms = 750;
    model.config.auto_retry = false;
    model.config.context_budget_tokens = 222_000;
    model.config.compact_threshold_pct = 91;
    model.config.keep_recent_on_compact = 17;
    model.config.bash_timeout_secs = 77;
    model.config.compaction_strategy = "custom_model".to_string();
    model.config.compaction_weles_provider = "openai".to_string();
    model.config.compaction_weles_model = "gpt-5.4-mini".to_string();
    model.config.compaction_weles_reasoning_effort = "medium".to_string();
    model.config.compaction_custom_provider = "openrouter".to_string();
    model.config.compaction_custom_base_url = "https://openrouter.ai/api/v1".to_string();
    model.config.compaction_custom_model = "anthropic/claude-sonnet-4".to_string();
    model.config.compaction_custom_api_key = "sk-compaction".to_string();
    model.config.compaction_custom_assistant_id = "assist-compaction".to_string();
    model.config.compaction_custom_auth_source = "api_key".to_string();
    model.config.compaction_custom_api_transport = "chat_completions".to_string();
    model.config.compaction_custom_reasoning_effort = "high".to_string();
    model.config.compaction_custom_context_window_tokens = 333_000;
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
    assert_eq!(json["auto_compact_context"], false);
    assert_eq!(json["max_context_messages"], 123);
    assert_eq!(json["max_tool_loops"], 44);
    assert_eq!(json["max_retries"], 7);
    assert_eq!(json["retry_delay_ms"], 9000);
    assert_eq!(json["message_loop_delay_ms"], 250);
    assert_eq!(json["tool_call_delay_ms"], 750);
    assert_eq!(json["auto_retry"], false);
    assert_eq!(json["context_budget_tokens"], 222000);
    assert_eq!(json["compact_threshold_pct"], 91);
    assert_eq!(json["keep_recent_on_compact"], 17);
    assert_eq!(json["bash_timeout_seconds"], 77);
    assert_eq!(json["compaction"]["strategy"], "custom_model");
    assert_eq!(json["compaction"]["weles"]["model"], "gpt-5.4-mini");
    assert_eq!(
        json["compaction"]["custom_model"]["provider"],
        "openrouter"
    );
    assert_eq!(
        json["compaction"]["custom_model"]["context_window_tokens"],
        333000
    );
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

#[test]
fn apply_config_json_uses_daemon_retry_delay_default_when_missing() {
    let mut model = make_model();

    model.apply_config_json(&serde_json::json!({
        "provider": "openai",
        "providers": {
            "openai": {
                "base_url": "https://api.openai.com/v1",
                "model": "gpt-5.4",
                "auth_source": "api_key",
                "api_transport": "responses"
            }
        }
    }));

    assert_eq!(model.config.retry_delay_ms, 5_000);
    assert_eq!(model.config.message_loop_delay_ms, 500);
    assert_eq!(model.config.tool_call_delay_ms, 500);
}

#[test]
fn apply_config_json_loads_nested_compaction_settings() {
    let mut model = make_model();

    model.apply_config_json(&serde_json::json!({
        "provider": "openai",
        "providers": {
            "openai": {
                "base_url": "https://api.openai.com/v1",
                "model": "gpt-5.4",
                "auth_source": "api_key",
                "api_transport": "responses"
            }
        },
        "builtin_sub_agents": {
            "weles": {
                "provider": "openai",
                "model": "gpt-5.4-mini",
                "reasoning_effort": "medium"
            }
        },
        "compaction": {
            "strategy": "custom_model",
            "weles": {
                "provider": "openai",
                "model": "gpt-5.4-mini",
                "reasoning_effort": "medium"
            },
            "custom_model": {
                "provider": "openrouter",
                "base_url": "https://openrouter.ai/api/v1",
                "model": "anthropic/claude-sonnet-4",
                "api_key": "sk-compaction",
                "assistant_id": "assistant-compaction",
                "auth_source": "api_key",
                "api_transport": "chat_completions",
                "reasoning_effort": "xhigh",
                "context_window_tokens": 222000
            }
        }
    }));

    assert_eq!(model.config.compaction_strategy, "custom_model");
    assert_eq!(model.config.compaction_weles_provider, "openai");
    assert_eq!(model.config.compaction_weles_model, "gpt-5.4-mini");
    assert_eq!(model.config.compaction_custom_provider, "openrouter");
    assert_eq!(
        model.config.compaction_custom_base_url,
        "https://openrouter.ai/api/v1"
    );
    assert_eq!(
        model.config.compaction_custom_model,
        "anthropic/claude-sonnet-4"
    );
    assert_eq!(model.config.compaction_custom_api_key, "sk-compaction");
    assert_eq!(
        model.config.compaction_custom_assistant_id,
        "assistant-compaction"
    );
    assert_eq!(model.config.compaction_custom_api_transport, "chat_completions");
    assert_eq!(model.config.compaction_custom_reasoning_effort, "xhigh");
    assert_eq!(model.config.compaction_custom_context_window_tokens, 222000);
}

#[test]
fn load_saved_settings_requests_daemon_openai_auth_status() {
    let (mut model, mut daemon_rx) = make_model_with_daemon_rx();

    model.load_saved_settings();

    let mut saw_auth_status = false;
    while let Ok(command) = daemon_rx.try_recv() {
        if matches!(command, DaemonCommand::GetOpenAICodexAuthStatus) {
            saw_auth_status = true;
            break;
        }
    }

    assert!(saw_auth_status, "expected daemon auth status refresh");
}

#[test]
fn apply_config_json_does_not_consult_local_openai_auth_state() {
    let (mut model, mut daemon_rx) = make_model_with_daemon_rx();
    model.config.chatgpt_auth_available = true;
    model.config.chatgpt_auth_source = Some("stale-local".to_string());

    model.apply_config_json(&serde_json::json!({
        "provider": "openai",
        "providers": {
            "openai": {
                "base_url": "https://api.openai.com/v1",
                "model": "gpt-5.4",
                "auth_source": "chatgpt_subscription",
                "api_transport": "responses"
            }
        }
    }));

    let mut saw_auth_status = false;
    while let Ok(command) = daemon_rx.try_recv() {
        if matches!(command, DaemonCommand::GetOpenAICodexAuthStatus) {
            saw_auth_status = true;
            break;
        }
    }

    assert!(saw_auth_status, "expected daemon auth status refresh");
    assert!(model.config.chatgpt_auth_available);
    assert_eq!(
        model.config.chatgpt_auth_source.as_deref(),
        Some("stale-local")
    );
}

#[test]
fn sync_config_to_daemon_does_not_emit_null_api_key_for_advanced_edits() {
    let (mut model, mut daemon_rx) = make_model_with_daemon_rx();
    model.connected = true;
    model.agent_config_loaded = true;
    model.config.provider = "openai".to_string();
    model.config.base_url = "https://api.openai.com/v1".to_string();
    model.config.model = "gpt-5.4".to_string();
    model.config.api_key = "sk-live".to_string();
    model.config.auth_source = "api_key".to_string();
    model.config.api_transport = "responses".to_string();
    model.config.max_context_messages = 123;
    model.config.agent_config_raw = Some(serde_json::json!({
        "provider": "openai",
        "base_url": "https://api.openai.com/v1",
        "model": "gpt-5.4",
        "api_key": "sk-live",
        "auth_source": "api_key",
        "api_transport": "responses",
        "max_context_messages": 100,
        "providers": {
            "openai": {
                "base_url": "https://api.openai.com/v1",
                "model": "gpt-5.4",
                "api_key": "sk-live",
                "auth_source": "api_key",
                "api_transport": "responses",
                "reasoning_effort": ""
            }
        }
    }));

    model.sync_config_to_daemon();

    let mut emitted_max_context_messages = false;
    while let Ok(command) = daemon_rx.try_recv() {
        match command {
            DaemonCommand::SetConfigItem {
                key_path,
                value_json,
            } => {
                assert_ne!(
                    key_path, "/api_key",
                    "advanced edits must not null out api_key"
                );
                if key_path == "/max_context_messages" {
                    emitted_max_context_messages = true;
                    assert_eq!(value_json, "123");
                }
            }
            DaemonCommand::SetProviderModel { .. } => {}
            other => panic!("unexpected daemon command: {other:?}"),
        }
    }

    assert!(
        emitted_max_context_messages,
        "expected max_context_messages update to be emitted"
    );
}

#[test]
fn sync_config_to_daemon_emits_managed_execution_security_level_for_advanced_toggle() {
    let (mut model, mut daemon_rx) = make_model_with_daemon_rx();
    model.connected = true;
    model.agent_config_loaded = true;
    model.config.provider = "openai".to_string();
    model.config.base_url = "https://api.openai.com/v1".to_string();
    model.config.model = "gpt-5.4".to_string();
    model.config.api_key = "sk-live".to_string();
    model.config.auth_source = "api_key".to_string();
    model.config.api_transport = "responses".to_string();
    model.config.managed_security_level = "yolo".to_string();
    model.config.agent_config_raw = Some(serde_json::json!({
        "provider": "openai",
        "base_url": "https://api.openai.com/v1",
        "model": "gpt-5.4",
        "api_key": "sk-live",
        "auth_source": "api_key",
        "api_transport": "responses",
        "managed_execution": {
            "sandbox_enabled": false,
            "security_level": "highest"
        },
        "providers": {
            "openai": {
                "base_url": "https://api.openai.com/v1",
                "model": "gpt-5.4",
                "api_key": "sk-live",
                "auth_source": "api_key",
                "api_transport": "responses",
                "reasoning_effort": ""
            }
        }
    }));

    model.sync_config_to_daemon();

    let mut emitted_security_level = false;
    while let Ok(command) = daemon_rx.try_recv() {
        match command {
            DaemonCommand::SetConfigItem {
                key_path,
                value_json,
            } => {
                assert_ne!(
                    key_path, "/managed_security_level",
                    "advanced tab should use canonical managed_execution path"
                );
                if key_path == "/managed_execution/security_level" {
                    emitted_security_level = true;
                    assert_eq!(value_json, "\"yolo\"");
                }
            }
            DaemonCommand::SetProviderModel { .. } => {}
            other => panic!("unexpected daemon command: {other:?}"),
        }
    }

    assert!(
        emitted_security_level,
        "expected managed_execution security level update to be emitted"
    );
}
