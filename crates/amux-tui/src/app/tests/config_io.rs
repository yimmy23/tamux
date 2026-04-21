use super::*;
use crate::test_support::{env_var_lock, EnvVarGuard, TAMUX_DATA_DIR_ENV};
use amux_shared::providers::{
    PROVIDER_ID_ALIBABA_CODING_PLAN, PROVIDER_ID_AZURE_OPENAI, PROVIDER_ID_CUSTOM,
    PROVIDER_ID_GITHUB_COPILOT, PROVIDER_ID_MINIMAX_CODING_PLAN, PROVIDER_ID_OPENAI,
    PROVIDER_ID_OPENROUTER,
};
use rusqlite::Connection;
use std::sync::mpsc;
use tempfile::tempdir;
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
        normalize_provider_auth_source(PROVIDER_ID_OPENAI, "bogus"),
        "api_key".to_string()
    );
    assert_eq!(
        normalize_provider_auth_source(PROVIDER_ID_OPENAI, "chatgpt_subscription"),
        "chatgpt_subscription".to_string()
    );
}

#[test]
fn normalize_provider_transport_falls_back_for_invalid_values() {
    assert_eq!(
        normalize_provider_transport(PROVIDER_ID_MINIMAX_CODING_PLAN, "bogus"),
        "chat_completions".to_string()
    );
    assert_eq!(
        normalize_provider_transport(PROVIDER_ID_OPENAI, "responses"),
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
    model.config.provider = PROVIDER_ID_OPENAI.to_string();
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
    model.config.slack_token = "slack-secret".to_string();
    model.config.slack_channel_filter = "ops,alerts".to_string();
    model.config.telegram_token = "telegram-secret".to_string();
    model.config.telegram_allowed_chats = "1,2".to_string();
    model.config.discord_token = "discord-secret".to_string();
    model.config.discord_channel_filter = "1477397308600619194".to_string();
    model.config.discord_allowed_users = "alice,bob".to_string();
    model.config.whatsapp_phone_id = "phone-1".to_string();
    model.config.whatsapp_token = "whatsapp-secret".to_string();
    model.config.auto_compact_context = false;
    model.config.max_context_messages = 123;
    model.config.tui_chat_history_page_size = 222;
    model.config.max_tool_loops = 44;
    model.config.max_retries = 7;
    model.config.retry_delay_ms = 9_000;
    model.config.message_loop_delay_ms = 250;
    model.config.tool_call_delay_ms = 750;
    model.config.llm_stream_chunk_timeout_secs = 420;
    model.config.auto_retry = false;
    model.config.compact_threshold_pct = 91;
    model.config.keep_recent_on_compact = 17;
    model.config.bash_timeout_secs = 77;
    model.config.weles_max_concurrent_reviews = 4;
    model.config.compaction_strategy = "custom_model".to_string();
    model.config.compaction_weles_provider = PROVIDER_ID_OPENAI.to_string();
    model.config.compaction_weles_model = "gpt-5.4-mini".to_string();
    model.config.compaction_weles_reasoning_effort = "medium".to_string();
    model.config.compaction_custom_provider = PROVIDER_ID_OPENROUTER.to_string();
    model.config.compaction_custom_base_url = "https://openrouter.ai/api/v1".to_string();
    model.config.compaction_custom_model = "arcee-ai/trinity-large-thinking".to_string();
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
    assert_eq!(json["provider"], PROVIDER_ID_OPENAI);
    assert_eq!(
        json["providers"][PROVIDER_ID_OPENAI]["assistant_id"],
        "asst_123"
    );
    assert!(json[PROVIDER_ID_OPENAI].get("api_key").is_none());
    assert!(json["providers"][PROVIDER_ID_OPENAI]
        .get("api_key")
        .is_none());
    assert_eq!(json["tools"]["bash"], false);
    assert_eq!(json["search_provider"], "exa");
    assert_eq!(json["enable_conversation_memory"], false);
    assert_eq!(json["operator_model"]["enabled"], true);
    assert_eq!(json["collaboration"]["enabled"], true);
    assert_eq!(json["compliance"]["sign_all_events"], true);
    assert_eq!(json["gateway"]["command_prefix"], "/tamux");
    assert_eq!(json["gateway"]["slack_token"], "slack-secret");
    assert_eq!(json["gateway"]["slack_channel_filter"], "ops,alerts");
    assert_eq!(json["gateway"]["telegram_token"], "telegram-secret");
    assert_eq!(json["gateway"]["telegram_allowed_chats"], "1,2");
    assert_eq!(json["gateway"]["discord_token"], "discord-secret");
    assert_eq!(
        json["gateway"]["discord_channel_filter"],
        "1477397308600619194"
    );
    assert_eq!(json["gateway"]["discord_allowed_users"], "alice,bob");
    assert_eq!(json["gateway"]["whatsapp_token"], "whatsapp-secret");
    assert_eq!(json["gateway"]["whatsapp_phone_id"], "phone-1");
    assert_eq!(json["auto_compact_context"], false);
    assert_eq!(json["max_context_messages"], 123);
    assert_eq!(json["tui_chat_history_page_size"], 222);
    assert_eq!(json["max_tool_loops"], 44);
    assert_eq!(json["max_retries"], 7);
    assert_eq!(json["retry_delay_ms"], 9000);
    assert_eq!(json["message_loop_delay_ms"], 250);
    assert_eq!(json["tool_call_delay_ms"], 750);
    assert_eq!(json["llm_stream_chunk_timeout_secs"], 420);
    assert_eq!(json["auto_retry"], false);
    assert!(
        json.get("context_budget_tokens").is_none(),
        "config patch should not serialize removed context budget settings"
    );
    assert_eq!(json["compact_threshold_pct"], 91);
    assert_eq!(json["keep_recent_on_compact"], 17);
    assert_eq!(json["bash_timeout_seconds"], 77);
    assert_eq!(
        json["builtin_sub_agents"]["weles"]["max_concurrent_reviews"],
        4
    );
    assert_eq!(json["compaction"]["strategy"], "custom_model");
    assert_eq!(json["compaction"]["weles"]["model"], "gpt-5.4-mini");
    assert_eq!(
        json["compaction"]["custom_model"]["provider"],
        PROVIDER_ID_OPENROUTER
    );
    assert_eq!(
        json["compaction"]["custom_model"]["context_window_tokens"],
        333000
    );
    assert_eq!(json["snapshot_retention"]["max_snapshots"], 15);
}

#[test]
fn build_config_patch_value_preserves_custom_model_context_override_for_non_custom_provider() {
    let mut model = make_model();
    model.config.provider = PROVIDER_ID_OPENROUTER.to_string();
    model.config.auth_source = "api_key".to_string();
    model.config.base_url = "https://openrouter.ai/api/v1".to_string();
    model.config.model = "openrouter/custom-preview".to_string();
    model.config.custom_model_name = "Custom Preview".to_string();
    model.config.context_window_tokens = 333_000;
    model.config.custom_context_window_tokens = Some(333_000);
    model.config.agent_config_raw = Some(serde_json::json!({
        "provider": "openrouter",
        "providers": {
            "openrouter": {
                "base_url": "https://openrouter.ai/api/v1",
                "model": "openrouter/custom-preview",
                "custom_model_name": "Custom Preview",
                "auth_source": "api_key",
                "api_transport": "chat_completions",
                "context_window_tokens": 333000
            }
        }
    }));

    let json = model.build_config_patch_value();

    assert_eq!(json["context_window_tokens"], serde_json::json!(333000));
    assert_eq!(
        json["providers"]["openrouter"]["context_window_tokens"],
        serde_json::json!(333000)
    );
    assert_eq!(
        json["openrouter"]["context_window_tokens"],
        serde_json::json!(333000)
    );
}

#[test]
fn apply_config_json_preserves_azure_openai_base_url() {
    let mut model = make_model();

    model.apply_config_json(&serde_json::json!({
        "provider": PROVIDER_ID_AZURE_OPENAI,
        "base_url": "https://my-resource.openai.azure.com/openai/v1",
        "model": "my-deployment",
        "providers": {
            PROVIDER_ID_AZURE_OPENAI: {
                "base_url": "https://my-resource.openai.azure.com/openai/v1",
                "model": "my-deployment",
                "auth_source": "api_key",
                "api_transport": "responses"
            }
        }
    }));

    assert_eq!(model.config.provider, PROVIDER_ID_AZURE_OPENAI);
    assert_eq!(
        model.config.base_url,
        "https://my-resource.openai.azure.com/openai/v1"
    );
    assert_eq!(model.config.model, "my-deployment");
    assert_eq!(model.config.api_transport, "responses");
}

#[test]
fn build_config_patch_value_round_trips_daemon_backed_settings() {
    let mut model = make_model();
    model.config.provider = PROVIDER_ID_OPENAI.to_string();
    model.config.base_url = "https://example.invalid/v1".to_string();
    model.config.model = "gpt-5.4-mini".to_string();
    model.config.reasoning_effort = "high".to_string();
    model.config.tool_bash = false;
    model.config.tool_file_ops = false;
    model.config.tool_web_search = false;
    model.config.tool_web_browse = true;
    model.config.tool_vision = true;
    model.config.tool_system_info = false;
    model.config.tool_gateway = false;
    model.config.search_provider = "exa".to_string();
    model.config.firecrawl_api_key = "fc-secret".to_string();
    model.config.exa_api_key = "exa-secret".to_string();
    model.config.tavily_api_key = "tavily-secret".to_string();
    model.config.search_max_results = 12;
    model.config.search_timeout_secs = 45;
    model.config.browse_provider = "chrome".to_string();
    model.config.enable_streaming = false;
    model.config.enable_conversation_memory = false;
    model.config.enable_honcho_memory = true;
    model.config.honcho_api_key = "honcho-secret".to_string();
    model.config.honcho_base_url = "https://honcho.example".to_string();
    model.config.honcho_workspace_id = "workspace-123".to_string();
    model.config.anticipatory_enabled = true;
    model.config.anticipatory_morning_brief = true;
    model.config.anticipatory_predictive_hydration = true;
    model.config.anticipatory_stuck_detection = true;
    model.config.operator_model_enabled = true;
    model.config.operator_model_allow_message_statistics = true;
    model.config.operator_model_allow_approval_learning = true;
    model.config.operator_model_allow_attention_tracking = true;
    model.config.operator_model_allow_implicit_feedback = true;
    model.config.collaboration_enabled = true;
    model.config.compliance_mode = "soc2".to_string();
    model.config.compliance_retention_days = 365;
    model.config.compliance_sign_all_events = true;
    model.config.tool_synthesis_enabled = true;
    model.config.tool_synthesis_require_activation = false;
    model.config.tool_synthesis_max_generated_tools = 77;
    model.config.managed_sandbox_enabled = true;
    model.config.managed_security_level = "moderate".to_string();
    model.config.gateway_enabled = true;
    model.config.gateway_prefix = "/tamux".to_string();
    model.config.slack_token = "slack-secret".to_string();
    model.config.slack_channel_filter = "ops,alerts".to_string();
    model.config.telegram_token = "telegram-secret".to_string();
    model.config.telegram_allowed_chats = "1,2".to_string();
    model.config.discord_token = "discord-secret".to_string();
    model.config.discord_channel_filter = "1477397308600619194".to_string();
    model.config.discord_allowed_users = "alice,bob".to_string();
    model.config.whatsapp_allowed_contacts = "15551234567".to_string();
    model.config.whatsapp_token = "whatsapp-secret".to_string();
    model.config.whatsapp_phone_id = "phone-1".to_string();
    model.config.auto_compact_context = false;
    model.config.max_context_messages = 123;
    model.config.tui_chat_history_page_size = 222;
    model.config.max_tool_loops = 44;
    model.config.max_retries = 7;
    model.config.retry_delay_ms = 9_000;
    model.config.message_loop_delay_ms = 250;
    model.config.tool_call_delay_ms = 750;
    model.config.llm_stream_chunk_timeout_secs = 420;
    model.config.auto_retry = false;
    model.config.compact_threshold_pct = 91;
    model.config.keep_recent_on_compact = 17;
    model.config.bash_timeout_secs = 77;
    model.config.weles_max_concurrent_reviews = 4;
    model.config.compaction_strategy = "custom_model".to_string();
    model.config.compaction_weles_provider = PROVIDER_ID_OPENAI.to_string();
    model.config.compaction_weles_model = "gpt-5.4-mini".to_string();
    model.config.compaction_weles_reasoning_effort = "medium".to_string();
    model.config.compaction_custom_provider = PROVIDER_ID_OPENROUTER.to_string();
    model.config.compaction_custom_base_url = "https://openrouter.ai/api/v1".to_string();
    model.config.compaction_custom_model = "arcee-ai/trinity-large-thinking".to_string();
    model.config.compaction_custom_api_key = "sk-compaction".to_string();
    model.config.compaction_custom_assistant_id = "assist-compaction".to_string();
    model.config.compaction_custom_auth_source = "api_key".to_string();
    model.config.compaction_custom_api_transport = "chat_completions".to_string();
    model.config.compaction_custom_reasoning_effort = "high".to_string();
    model.config.compaction_custom_context_window_tokens = 333_000;
    model.config.snapshot_max_count = 15;
    model.config.snapshot_max_size_mb = 2_048;
    model.config.snapshot_auto_cleanup = false;

    let json = model.build_config_patch_value();

    let mut reloaded = make_model();
    reloaded.apply_config_json(&json);

    assert_eq!(reloaded.config.provider, PROVIDER_ID_OPENAI);
    assert_eq!(reloaded.config.base_url, "https://api.openai.com/v1");
    assert_eq!(reloaded.config.model, "gpt-5.4-mini");
    assert_eq!(reloaded.config.reasoning_effort, "high");
    assert_eq!(reloaded.config.tool_bash, false);
    assert_eq!(reloaded.config.tool_file_ops, false);
    assert_eq!(reloaded.config.tool_web_search, false);
    assert_eq!(reloaded.config.tool_web_browse, true);
    assert_eq!(reloaded.config.tool_vision, true);
    assert_eq!(reloaded.config.tool_system_info, false);
    assert_eq!(reloaded.config.tool_gateway, false);
    assert_eq!(reloaded.config.search_provider, "exa");
    assert_eq!(reloaded.config.firecrawl_api_key, "fc-secret");
    assert_eq!(reloaded.config.exa_api_key, "exa-secret");
    assert_eq!(reloaded.config.tavily_api_key, "tavily-secret");
    assert_eq!(reloaded.config.search_max_results, 12);
    assert_eq!(reloaded.config.search_timeout_secs, 45);
    assert_eq!(reloaded.config.browse_provider, "chrome");
    assert_eq!(reloaded.config.enable_streaming, false);
    assert_eq!(reloaded.config.enable_conversation_memory, false);
    assert_eq!(reloaded.config.enable_honcho_memory, true);
    assert_eq!(reloaded.config.honcho_api_key, "honcho-secret");
    assert_eq!(reloaded.config.honcho_base_url, "https://honcho.example");
    assert_eq!(reloaded.config.honcho_workspace_id, "workspace-123");
    assert_eq!(reloaded.config.anticipatory_enabled, true);
    assert_eq!(reloaded.config.anticipatory_morning_brief, true);
    assert_eq!(reloaded.config.anticipatory_predictive_hydration, true);
    assert_eq!(reloaded.config.anticipatory_stuck_detection, true);
    assert_eq!(reloaded.config.operator_model_enabled, true);
    assert_eq!(
        reloaded.config.operator_model_allow_message_statistics,
        true
    );
    assert_eq!(reloaded.config.operator_model_allow_approval_learning, true);
    assert_eq!(
        reloaded.config.operator_model_allow_attention_tracking,
        true
    );
    assert_eq!(reloaded.config.operator_model_allow_implicit_feedback, true);
    assert_eq!(reloaded.config.collaboration_enabled, true);
    assert_eq!(reloaded.config.compliance_mode, "soc2");
    assert_eq!(reloaded.config.compliance_retention_days, 365);
    assert_eq!(reloaded.config.compliance_sign_all_events, true);
    assert_eq!(reloaded.config.tool_synthesis_enabled, true);
    assert_eq!(reloaded.config.tool_synthesis_require_activation, false);
    assert_eq!(reloaded.config.tool_synthesis_max_generated_tools, 77);
    assert_eq!(reloaded.config.managed_sandbox_enabled, true);
    assert_eq!(reloaded.config.managed_security_level, "moderate");
    assert_eq!(reloaded.config.gateway_enabled, true);
    assert_eq!(reloaded.config.gateway_prefix, "/tamux");
    assert_eq!(reloaded.config.slack_token, "slack-secret");
    assert_eq!(reloaded.config.slack_channel_filter, "ops,alerts");
    assert_eq!(reloaded.config.telegram_token, "telegram-secret");
    assert_eq!(reloaded.config.telegram_allowed_chats, "1,2");
    assert_eq!(reloaded.config.discord_token, "discord-secret");
    assert_eq!(
        reloaded.config.discord_channel_filter,
        "1477397308600619194"
    );
    assert_eq!(reloaded.config.discord_allowed_users, "alice,bob");
    assert_eq!(reloaded.config.whatsapp_allowed_contacts, "15551234567");
    assert_eq!(reloaded.config.whatsapp_token, "whatsapp-secret");
    assert_eq!(reloaded.config.whatsapp_phone_id, "phone-1");
    assert_eq!(reloaded.config.auto_compact_context, false);
    assert_eq!(reloaded.config.max_context_messages, 123);
    assert_eq!(reloaded.config.tui_chat_history_page_size, 222);
    assert_eq!(reloaded.config.max_tool_loops, 44);
    assert_eq!(reloaded.config.max_retries, 7);
    assert_eq!(reloaded.config.retry_delay_ms, 9_000);
    assert_eq!(reloaded.config.message_loop_delay_ms, 250);
    assert_eq!(reloaded.config.tool_call_delay_ms, 750);
    assert_eq!(reloaded.config.llm_stream_chunk_timeout_secs, 420);
    assert_eq!(reloaded.config.auto_retry, false);
    assert_eq!(reloaded.config.compact_threshold_pct, 91);
    assert_eq!(reloaded.config.keep_recent_on_compact, 17);
    assert_eq!(reloaded.config.bash_timeout_secs, 77);
    assert_eq!(reloaded.config.weles_max_concurrent_reviews, 4);
    assert_eq!(reloaded.config.compaction_strategy, "custom_model");
    assert_eq!(
        reloaded.config.compaction_weles_provider,
        PROVIDER_ID_OPENAI
    );
    assert_eq!(reloaded.config.compaction_weles_model, "gpt-5.4-mini");
    assert_eq!(reloaded.config.compaction_weles_reasoning_effort, "medium");
    assert_eq!(
        reloaded.config.compaction_custom_provider,
        PROVIDER_ID_OPENROUTER
    );
    assert_eq!(
        reloaded.config.compaction_custom_base_url,
        "https://openrouter.ai/api/v1"
    );
    assert_eq!(
        reloaded.config.compaction_custom_model,
        "arcee-ai/trinity-large-thinking"
    );
    assert_eq!(reloaded.config.compaction_custom_api_key, "sk-compaction");
    assert_eq!(
        reloaded.config.compaction_custom_assistant_id,
        "assist-compaction"
    );
    assert_eq!(reloaded.config.compaction_custom_auth_source, "api_key");
    assert_eq!(
        reloaded.config.compaction_custom_api_transport,
        "chat_completions"
    );
    assert_eq!(reloaded.config.compaction_custom_reasoning_effort, "high");
    assert_eq!(
        reloaded.config.compaction_custom_context_window_tokens,
        333_000
    );
    assert_eq!(reloaded.config.snapshot_max_count, 15);
    assert_eq!(reloaded.config.snapshot_max_size_mb, 2_048);
    assert_eq!(reloaded.config.snapshot_auto_cleanup, false);
}

#[test]
fn build_config_patch_value_round_trips_disabled_snapshot_retention() {
    let mut model = make_model();
    model.config.snapshot_max_count = 0;
    model.config.snapshot_max_size_mb = 10_240;
    model.config.snapshot_auto_cleanup = false;

    let json = model.build_config_patch_value();

    assert_eq!(json["snapshot_retention"]["max_snapshots"], 0);
    assert_eq!(json["snapshot_retention"]["auto_cleanup"], false);

    let mut reloaded = make_model();
    reloaded.apply_config_json(&json);

    assert_eq!(reloaded.config.snapshot_max_count, 0);
    assert_eq!(reloaded.config.snapshot_max_size_mb, 10_240);
    assert!(!reloaded.config.snapshot_auto_cleanup);
}

#[test]
fn refresh_snapshot_stats_reads_daemon_snapshot_index_db() {
    let _lock = env_var_lock();
    let home = tempdir().expect("tempdir");
    let tamux_dir = home.path().join("tamux-data");
    let history_dir = tamux_dir.join("history");
    std::fs::create_dir_all(&history_dir).expect("create history dir");

    let snap_a = tamux_dir.join("snap-a.tar.gz");
    let snap_b = tamux_dir.join("snap-b.tar.gz");
    std::fs::write(&snap_a, vec![b'a'; 128]).expect("write snapshot a");
    std::fs::write(&snap_b, vec![b'b'; 256]).expect("write snapshot b");

    let db_path = history_dir.join("command-history.db");
    let conn = Connection::open(&db_path).expect("open sqlite db");
    conn.execute(
        "CREATE TABLE snapshot_index (snapshot_id TEXT PRIMARY KEY, path TEXT NOT NULL)",
        [],
    )
    .expect("create snapshot index table");
    conn.execute(
        "INSERT INTO snapshot_index (snapshot_id, path) VALUES (?1, ?2)",
        rusqlite::params!["snap-a", snap_a.to_string_lossy().to_string()],
    )
    .expect("insert snapshot a");
    conn.execute(
        "INSERT INTO snapshot_index (snapshot_id, path) VALUES (?1, ?2)",
        rusqlite::params!["snap-b", snap_b.to_string_lossy().to_string()],
    )
    .expect("insert snapshot b");

    let _data_dir = EnvVarGuard::set(TAMUX_DATA_DIR_ENV, &tamux_dir);

    let mut model = make_model();
    model.load_saved_settings();

    assert_eq!(model.config.snapshot_count, 2);
    assert_eq!(model.config.snapshot_total_size_bytes, 384);
}

#[test]
fn refresh_snapshot_stats_ignores_stale_snapshot_index_rows() {
    let _lock = env_var_lock();
    let home = tempdir().expect("tempdir");
    let tamux_dir = home.path().join("tamux-data");
    let history_dir = tamux_dir.join("history");
    std::fs::create_dir_all(&history_dir).expect("create history dir");

    let snap_a = tamux_dir.join("snap-a.tar.gz");
    std::fs::write(&snap_a, vec![b'a'; 128]).expect("write snapshot a");
    let missing = tamux_dir.join("missing.tar.gz");

    let db_path = history_dir.join("command-history.db");
    let conn = Connection::open(&db_path).expect("open sqlite db");
    conn.execute(
        "CREATE TABLE snapshot_index (snapshot_id TEXT PRIMARY KEY, path TEXT NOT NULL)",
        [],
    )
    .expect("create snapshot index table");
    conn.execute(
        "INSERT INTO snapshot_index (snapshot_id, path) VALUES (?1, ?2)",
        rusqlite::params!["snap-a", snap_a.to_string_lossy().to_string()],
    )
    .expect("insert snapshot a");
    conn.execute(
        "INSERT INTO snapshot_index (snapshot_id, path) VALUES (?1, ?2)",
        rusqlite::params!["missing", missing.to_string_lossy().to_string()],
    )
    .expect("insert missing snapshot row");

    let _data_dir = EnvVarGuard::set(TAMUX_DATA_DIR_ENV, &tamux_dir);

    let mut model = make_model();
    model.load_saved_settings();

    assert_eq!(model.config.snapshot_count, 1);
    assert_eq!(model.config.snapshot_total_size_bytes, 128);
}

#[test]
fn apply_config_json_prefers_active_provider_transport_over_stale_root_transport() {
    let mut model = make_model();

    model.apply_config_json(&serde_json::json!({
        "provider": PROVIDER_ID_GITHUB_COPILOT,
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

    assert_eq!(model.config.provider, PROVIDER_ID_GITHUB_COPILOT);
    assert_eq!(model.config.api_transport, "responses");
}

#[test]
fn apply_config_json_prefers_active_provider_auth_source_over_stale_root_auth_source() {
    let mut model = make_model();

    model.apply_config_json(&serde_json::json!({
        "provider": PROVIDER_ID_GITHUB_COPILOT,
        "auth_source": "api_key",
        "providers": {
            "github-copilot": {
                "base_url": "https://api.githubcopilot.com",
                "model": "gpt-5.4",
                "api_transport": "responses",
                "auth_source": "github_copilot"
            }
        }
    }));

    assert_eq!(model.config.provider, PROVIDER_ID_GITHUB_COPILOT);
    assert_eq!(model.config.auth_source, "github_copilot");
}

#[test]
fn apply_config_json_uses_daemon_retry_delay_default_when_missing() {
    let mut model = make_model();

    model.apply_config_json(&serde_json::json!({
        "provider": PROVIDER_ID_OPENAI,
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
    assert_eq!(model.config.tui_chat_history_page_size, 100);
    assert_eq!(model.config.message_loop_delay_ms, 500);
    assert_eq!(model.config.tool_call_delay_ms, 500);
}

#[test]
fn apply_provider_selection_reads_nested_provider_config_values() {
    let mut model = make_model();

    model.config.agent_config_raw = Some(serde_json::json!({
        "provider": PROVIDER_ID_OPENAI,
        "providers": {
            "alibaba-coding-plan": {
                "base_url": "https://coding-intl.dashscope.aliyuncs.com/v1",
                "model": "qwen3-coder-next",
                "api_key": "dashscope-key",
                "assistant_id": "",
                "auth_source": "api_key",
                "api_transport": "chat_completions",
                "context_window_tokens": 204800
            }
        }
    }));

    model.apply_provider_selection(PROVIDER_ID_ALIBABA_CODING_PLAN);

    assert_eq!(model.config.provider, PROVIDER_ID_ALIBABA_CODING_PLAN);
    assert_eq!(model.config.api_key, "dashscope-key");
    assert_eq!(model.config.model, "qwen3-coder-next");
    assert_eq!(
        model.config.base_url,
        "https://coding-intl.dashscope.aliyuncs.com/v1"
    );
}

#[test]
fn build_config_patch_value_preserves_nested_inactive_provider_auth_modes() {
    let mut model = make_model();
    model.config.provider = PROVIDER_ID_GITHUB_COPILOT.to_string();
    model.config.base_url = "https://models.github.ai/inference".to_string();
    model.config.model = "gpt-5.4".to_string();
    model.config.auth_source = "github_copilot".to_string();
    model.config.api_transport = "responses".to_string();
    model.config.agent_config_raw = Some(serde_json::json!({
        "provider": PROVIDER_ID_GITHUB_COPILOT,
        "providers": {
            "openai": {
                "base_url": "https://api.openai.com/v1",
                "model": "gpt-5.4",
                "auth_source": "chatgpt_subscription",
                "api_transport": "responses"
            },
            "github-copilot": {
                "base_url": "https://models.github.ai/inference",
                "model": "gpt-5.4",
                "auth_source": "github_copilot",
                "api_transport": "responses"
            }
        }
    }));

    let json = model.build_config_patch_value();

    assert_eq!(
        json["providers"][PROVIDER_ID_OPENAI]["auth_source"],
        "chatgpt_subscription"
    );
    assert_eq!(
        json["providers"][PROVIDER_ID_OPENAI]["api_transport"],
        "responses"
    );
    assert_eq!(
        json["providers"][PROVIDER_ID_GITHUB_COPILOT]["auth_source"],
        "github_copilot"
    );
}

#[test]
fn apply_config_json_loads_nested_compaction_settings() {
    let mut model = make_model();

    model.apply_config_json(&serde_json::json!({
        "provider": PROVIDER_ID_OPENAI,
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
                "reasoning_effort": "medium",
                "max_concurrent_reviews": 5
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
                "model": "arcee-ai/trinity-large-thinking",
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
    assert_eq!(model.config.weles_max_concurrent_reviews, 5);
    assert_eq!(model.config.compaction_weles_provider, PROVIDER_ID_OPENAI);
    assert_eq!(model.config.compaction_weles_model, "gpt-5.4-mini");
    assert_eq!(
        model.config.compaction_custom_provider,
        PROVIDER_ID_OPENROUTER
    );
    assert_eq!(
        model.config.compaction_custom_base_url,
        "https://openrouter.ai/api/v1"
    );
    assert_eq!(
        model.config.compaction_custom_model,
        "arcee-ai/trinity-large-thinking"
    );
    assert_eq!(model.config.compaction_custom_api_key, "sk-compaction");
    assert_eq!(
        model.config.compaction_custom_assistant_id,
        "assistant-compaction"
    );
    assert_eq!(
        model.config.compaction_custom_api_transport,
        "chat_completions"
    );
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
        "provider": PROVIDER_ID_OPENAI,
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
    model.config.provider = PROVIDER_ID_OPENAI.to_string();
    model.config.base_url = "https://api.openai.com/v1".to_string();
    model.config.model = "gpt-5.4".to_string();
    model.config.api_key = "sk-live".to_string();
    model.config.auth_source = "api_key".to_string();
    model.config.api_transport = "responses".to_string();
    model.config.max_context_messages = 123;
    model.config.tui_chat_history_page_size = 222;
    model.config.agent_config_raw = Some(serde_json::json!({
        "provider": "openai",
        "base_url": "https://api.openai.com/v1",
        "model": "gpt-5.4",
        "api_key": "sk-live",
        "auth_source": "api_key",
        "api_transport": "responses",
        "max_context_messages": 100,
        "tui_chat_history_page_size": 100,
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
    let mut emitted_tui_chat_history_page_size = false;
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
                if key_path == "/tui_chat_history_page_size" {
                    emitted_tui_chat_history_page_size = true;
                    assert_eq!(value_json, "222");
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
    assert!(
        emitted_tui_chat_history_page_size,
        "expected tui_chat_history_page_size update to be emitted"
    );
}

#[test]
fn sync_config_to_daemon_emits_managed_execution_security_level_for_advanced_toggle() {
    let (mut model, mut daemon_rx) = make_model_with_daemon_rx();
    model.connected = true;
    model.agent_config_loaded = true;
    model.config.provider = PROVIDER_ID_OPENAI.to_string();
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

#[test]
fn sync_config_to_daemon_emits_provider_model_and_base_url_items_for_provider_switch() {
    let (mut model, mut daemon_rx) = make_model_with_daemon_rx();
    model.connected = true;
    model.agent_config_loaded = true;
    model.config.provider = PROVIDER_ID_OPENROUTER.to_string();
    model.config.base_url = "https://openrouter.ai/api/v1".to_string();
    model.config.model = "arcee-ai/trinity-large-preview:free".to_string();
    model.config.auth_source = "api_key".to_string();
    model.config.api_transport = "chat_completions".to_string();
    model.config.reasoning_effort = "high".to_string();
    model.config.context_window_tokens = 1_000_000;
    model.config.agent_config_raw = Some(serde_json::json!({
        "provider": "minimax-coding-plan",
        "base_url": "https://api.minimax.io/anthropic",
        "model": "MiniMax-M2.7",
        "api_key": "sk-cp-test",
        "auth_source": "api_key",
        "api_transport": "chat_completions",
        "providers": {
            "minimax-coding-plan": {
                "base_url": "https://api.minimax.io/anthropic",
                "model": "MiniMax-M2.7",
                "api_key": "sk-cp-test",
                "auth_source": "api_key",
                "api_transport": "chat_completions",
                "reasoning_effort": "high",
                "context_window_tokens": 205000
            },
            "openrouter": {
                "base_url": "https://openrouter.ai/api/v1",
                "model": "arcee-ai/trinity-large-preview:free",
                "api_key": "sk-or-test",
                "auth_source": "api_key",
                "api_transport": "chat_completions",
                "reasoning_effort": "high",
                "context_window_tokens": 1000000
            }
        }
    }));

    model.sync_config_to_daemon();

    let mut saw_provider = false;
    let mut saw_model = false;
    let mut saw_base_url = false;
    while let Ok(command) = daemon_rx.try_recv() {
        match command {
            DaemonCommand::SetConfigItem {
                key_path,
                value_json,
            } => match key_path.as_str() {
                "/provider" => {
                    saw_provider = true;
                    assert_eq!(value_json, format!("\"{}\"", PROVIDER_ID_OPENROUTER));
                }
                "/model" => {
                    saw_model = true;
                    assert_eq!(value_json, "\"arcee-ai/trinity-large-preview:free\"");
                }
                "/base_url" => {
                    saw_base_url = true;
                    assert_eq!(value_json, "\"https://openrouter.ai/api/v1\"");
                }
                _ => {}
            },
            DaemonCommand::SetProviderModel { .. } => {
                panic!("provider switches should persist through config items")
            }
            other => panic!("unexpected daemon command: {other:?}"),
        }
    }

    assert!(saw_provider, "expected /provider update to be emitted");
    assert!(saw_model, "expected /model update to be emitted");
    assert!(saw_base_url, "expected /base_url update to be emitted");
}

#[test]
fn sync_config_to_daemon_emits_second_provider_switch_after_returning_from_custom() {
    let (mut model, mut daemon_rx) = make_model_with_daemon_rx();
    model.connected = true;
    model.agent_config_loaded = true;
    model.config.provider = PROVIDER_ID_MINIMAX_CODING_PLAN.to_string();
    model.config.base_url = "https://api.minimax.io/anthropic".to_string();
    model.config.model = "MiniMax-M2.7".to_string();
    model.config.auth_source = "api_key".to_string();
    model.config.api_transport = "chat_completions".to_string();
    model.config.reasoning_effort = "high".to_string();
    model.config.context_window_tokens = 205_000;
    model.config.agent_config_raw = Some(serde_json::json!({
        "provider": "minimax-coding-plan",
        "base_url": "https://api.minimax.io/anthropic",
        "model": "MiniMax-M2.7",
        "auth_source": "api_key",
        "api_transport": "chat_completions",
        "providers": {
            "minimax-coding-plan": {
                "base_url": "https://api.minimax.io/anthropic",
                "model": "MiniMax-M2.7",
                "auth_source": "api_key",
                "api_transport": "chat_completions",
                "reasoning_effort": "high",
                "context_window_tokens": 205000
            },
            "custom": {
                "base_url": "",
                "model": "",
                "auth_source": "api_key",
                "api_transport": "responses",
                "reasoning_effort": "high",
                "context_window_tokens": 128000
            }
        }
    }));

    model.config.provider = PROVIDER_ID_CUSTOM.to_string();
    model.config.base_url = "http://localhost:11434/v1".to_string();
    model.config.model = "llama3.1".to_string();
    model.config.api_transport = "chat_completions".to_string();
    model.config.context_window_tokens = 128_000;

    model.sync_config_to_daemon();

    while daemon_rx.try_recv().is_ok() {}

    model.config.provider = PROVIDER_ID_MINIMAX_CODING_PLAN.to_string();
    model.config.base_url = "https://api.minimax.io/anthropic".to_string();
    model.config.model = "MiniMax-M2.7".to_string();
    model.config.auth_source = "api_key".to_string();
    model.config.api_transport = "chat_completions".to_string();
    model.config.context_window_tokens = 205_000;

    model.sync_config_to_daemon();

    let mut saw_provider = false;
    let mut saw_model = false;
    let mut saw_base_url = false;
    while let Ok(command) = daemon_rx.try_recv() {
        match command {
            DaemonCommand::SetConfigItem {
                key_path,
                value_json,
            } => match key_path.as_str() {
                "/provider" => {
                    saw_provider = true;
                    assert_eq!(
                        value_json,
                        format!("\"{}\"", PROVIDER_ID_MINIMAX_CODING_PLAN)
                    );
                }
                "/model" => {
                    saw_model = true;
                    assert_eq!(value_json, "\"MiniMax-M2.7\"");
                }
                "/base_url" => {
                    saw_base_url = true;
                    assert_eq!(value_json, "\"https://api.minimax.io/anthropic\"");
                }
                _ => {}
            },
            DaemonCommand::SetProviderModel { .. } => {
                panic!("provider switches should persist through config items")
            }
            other => panic!("unexpected daemon command: {other:?}"),
        }
    }

    assert!(
        saw_provider,
        "expected second provider switch to emit /provider update"
    );
    assert!(
        saw_model,
        "expected second provider switch to emit /model update"
    );
    assert!(
        saw_base_url,
        "expected second provider switch to emit /base_url update"
    );
}

#[test]
fn build_config_patch_value_pins_inherited_rarog_and_weles_when_main_provider_changes_after_setup()
{
    let mut model = make_model();
    model.config.provider = PROVIDER_ID_GITHUB_COPILOT.to_string();
    model.config.base_url = "https://models.github.ai/inference".to_string();
    model.config.model = "gpt-5.4".to_string();
    model.config.auth_source = "github_copilot".to_string();
    model.config.api_transport = "chat_completions".to_string();
    model.config.agent_config_raw = Some(serde_json::json!({
        "provider": "openai",
        "base_url": "https://api.openai.com/v1",
        "model": "gpt-4.1",
        "auth_source": "api_key",
        "api_transport": "responses",
        "providers": {
            "openai": {
                "base_url": "https://api.openai.com/v1",
                "model": "gpt-4.1",
                "auth_source": "api_key",
                "api_transport": "responses"
            },
            "github-copilot": {
                "base_url": "https://models.github.ai/inference",
                "model": "gpt-5.4",
                "auth_source": "github_copilot",
                "api_transport": "chat_completions"
            }
        },
        "concierge": {
            "enabled": true,
            "detail_level": "context_summary"
        },
        "builtin_sub_agents": {
            "weles": {
                "enabled": true
            }
        }
    }));

    let json = model.build_config_patch_value();

    assert_eq!(json["provider"], PROVIDER_ID_GITHUB_COPILOT);
    assert_eq!(json["model"], "gpt-5.4");
    assert_eq!(json["concierge"]["provider"], "openai");
    assert_eq!(json["concierge"]["model"], "gpt-4.1");
    assert_eq!(json["builtin_sub_agents"]["weles"]["provider"], "openai");
    assert_eq!(json["builtin_sub_agents"]["weles"]["model"], "gpt-4.1");
}

#[test]
fn build_config_patch_value_keeps_inheritance_during_first_time_main_provider_setup() {
    let mut model = make_model();
    model.config.provider = PROVIDER_ID_GITHUB_COPILOT.to_string();
    model.config.base_url = "https://models.github.ai/inference".to_string();
    model.config.model = "gpt-5.4".to_string();
    model.config.auth_source = "github_copilot".to_string();
    model.config.api_transport = "chat_completions".to_string();
    model.config.agent_config_raw = Some(serde_json::json!({
        "providers": {
            "github-copilot": {
                "base_url": "https://models.github.ai/inference",
                "model": "gpt-5.4",
                "auth_source": "github_copilot",
                "api_transport": "chat_completions"
            }
        },
        "concierge": {
            "enabled": true,
            "detail_level": "context_summary"
        },
        "builtin_sub_agents": {
            "weles": {
                "enabled": true
            }
        }
    }));

    let json = model.build_config_patch_value();

    assert_eq!(json["provider"], PROVIDER_ID_GITHUB_COPILOT);
    assert_eq!(json["model"], "gpt-5.4");
    assert!(json["concierge"].get("provider").is_none());
    assert!(json["concierge"].get("model").is_none());
    assert!(json["builtin_sub_agents"]["weles"]
        .get("provider")
        .is_none());
    assert!(json["builtin_sub_agents"]["weles"].get("model").is_none());
}

#[test]
fn build_config_patch_value_preserves_explicit_rarog_and_weles_overrides_on_main_switch() {
    let mut model = make_model();
    model.config.provider = PROVIDER_ID_GITHUB_COPILOT.to_string();
    model.config.base_url = "https://models.github.ai/inference".to_string();
    model.config.model = "gpt-5.4".to_string();
    model.config.auth_source = "github_copilot".to_string();
    model.config.api_transport = "chat_completions".to_string();
    model.config.agent_config_raw = Some(serde_json::json!({
        "provider": "openai",
        "base_url": "https://api.openai.com/v1",
        "model": "gpt-4.1",
        "auth_source": "api_key",
        "api_transport": "responses",
        "providers": {
            "openai": {
                "base_url": "https://api.openai.com/v1",
                "model": "gpt-4.1",
                "auth_source": "api_key",
                "api_transport": "responses"
            },
            "github-copilot": {
                "base_url": "https://models.github.ai/inference",
                "model": "gpt-5.4",
                "auth_source": "github_copilot",
                "api_transport": "chat_completions"
            },
            "custom-rarog": {
                "base_url": "https://rarog.example/v1",
                "model": "rarog-model",
                "auth_source": "api_key",
                "api_transport": "chat_completions"
            },
            "custom-weles": {
                "base_url": "https://weles.example/v1",
                "model": "weles-model",
                "auth_source": "api_key",
                "api_transport": "chat_completions"
            }
        },
        "concierge": {
            "enabled": true,
            "detail_level": "context_summary",
            "provider": "custom-rarog",
            "model": "rarog-model"
        },
        "builtin_sub_agents": {
            "weles": {
                "enabled": true,
                "provider": "custom-weles",
                "model": "weles-model"
            }
        }
    }));

    let json = model.build_config_patch_value();

    assert_eq!(json["concierge"]["provider"], "custom-rarog");
    assert_eq!(json["concierge"]["model"], "rarog-model");
    assert_eq!(
        json["builtin_sub_agents"]["weles"]["provider"],
        "custom-weles"
    );
    assert_eq!(json["builtin_sub_agents"]["weles"]["model"], "weles-model");
}
