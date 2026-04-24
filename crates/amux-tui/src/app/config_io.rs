use super::*;
use amux_shared::providers::{PROVIDER_ID_CUSTOM, PROVIDER_ID_OPENAI};

#[path = "config_io_helpers.rs"]
mod helpers;

use helpers::{
    flatten_config_value, normalize_compliance_mode, normalize_provider_auth_source,
    normalize_provider_transport,
};

impl TuiModel {
    pub(super) fn sync_config_to_daemon(&mut self) {
        self.chat
            .set_history_page_size(self.config.tui_chat_history_page_size as usize);
        if !self.connected {
            self.status_line = "Config change not saved: daemon is disconnected".to_string();
            return;
        }
        if !self.agent_config_loaded {
            self.status_line = "Config change not saved yet: waiting for daemon config".to_string();
            return;
        }
        let before = self
            .config
            .agent_config_raw
            .clone()
            .unwrap_or_else(|| serde_json::json!({}));
        let mut after = self.build_config_patch_value();

        let mut before_items = Vec::new();
        flatten_config_value(&before, "", &mut before_items);
        let before_map = before_items
            .into_iter()
            .collect::<std::collections::BTreeMap<_, _>>();

        let mut after_items = Vec::new();
        flatten_config_value(&after, "", &mut after_items);
        let mut changed = 0usize;

        for (key_path, value) in after_items {
            if before_map.get(&key_path) == Some(&value) {
                continue;
            }
            if let Ok(value_json) = serde_json::to_string(&value) {
                self.send_daemon_command(DaemonCommand::SetConfigItem {
                    key_path,
                    value_json,
                });
                changed += 1;
            }
        }

        if let (Some(before_providers), Some(after_providers)) = (
            before.get("providers").and_then(|value| value.as_object()),
            after
                .get_mut("providers")
                .and_then(|value| value.as_object_mut()),
        ) {
            for (provider_id, before_provider) in before_providers {
                if let Some(api_key) = before_provider.get("api_key").cloned() {
                    if let Some(after_provider) = after_providers
                        .get_mut(provider_id)
                        .and_then(|value| value.as_object_mut())
                    {
                        after_provider.insert("api_key".to_string(), api_key);
                    }
                }
            }
        }

        for provider in providers::PROVIDERS {
            if let Some(api_key) = before
                .get(provider.id)
                .and_then(|value| value.get("api_key"))
                .cloned()
            {
                if let Some(after_provider) = after
                    .get_mut(provider.id)
                    .and_then(|value| value.as_object_mut())
                {
                    after_provider.insert("api_key".to_string(), api_key);
                }
            }
        }

        self.config.agent_config_raw = Some(after);

        if changed == 0 {
            self.status_line = "No config changes to save".to_string();
        }
    }

    pub fn load_saved_settings(&mut self) {
        self.refresh_openai_auth_status();
        self.refresh_snapshot_stats();
    }

    pub(super) fn apply_config_json(&mut self, json: &serde_json::Value) {
        let provider_id = json
            .get("provider")
            .and_then(|v| v.as_str())
            .unwrap_or(PROVIDER_ID_OPENAI);

        let provider_config = json
            .get("providers")
            .and_then(|value| value.get(provider_id))
            .or_else(|| json.get(provider_id));

        if let Some(provider_config) = provider_config {
            let base_url = json
                .get("base_url")
                .and_then(|v| v.as_str())
                .filter(|v| !v.is_empty())
                .or_else(|| Self::provider_field_str(provider_config, "base_url", "base_url"));
            let model = json
                .get("model")
                .and_then(|v| v.as_str())
                .filter(|v| !v.is_empty())
                .or_else(|| Self::provider_field_str(provider_config, "model", "model"));
            let custom_model_name =
                Self::provider_field_str(provider_config, "custom_model_name", "custom_model_name")
                    .unwrap_or("");
            let api_key = json
                .get("api_key")
                .and_then(|v| v.as_str())
                .filter(|v| !v.is_empty())
                .or_else(|| Self::provider_field_str(provider_config, "api_key", "api_key"))
                .unwrap_or("");
            let assistant_id = json
                .get("assistant_id")
                .and_then(|v| v.as_str())
                .filter(|v| !v.is_empty())
                .or_else(|| {
                    Self::provider_field_str(provider_config, "assistant_id", "assistant_id")
                })
                .unwrap_or("");
            let api_transport =
                Self::provider_field_str(provider_config, "api_transport", "api_transport")
                    .filter(|v| !v.is_empty())
                    .or_else(|| {
                        json.get("api_transport")
                            .and_then(|v| v.as_str())
                            .filter(|v| !v.is_empty())
                    })
                    .unwrap_or_else(|| providers::default_transport_for(provider_id));
            let auth_source =
                Self::provider_field_str(provider_config, "auth_source", "auth_source")
                    .filter(|v| !v.is_empty())
                    .or_else(|| {
                        json.get("auth_source")
                            .and_then(|v| v.as_str())
                            .filter(|v| !v.is_empty())
                    })
                    .unwrap_or_else(|| providers::default_auth_source_for(provider_id));
            let custom_context_window_tokens = Self::provider_field_u64(
                provider_config,
                "context_window_tokens",
                "context_window_tokens",
            )
            .map(|v| v.max(1000) as u32);

            self.config.provider = provider_id.to_string();
            self.config.base_url = if !providers::provider_uses_configurable_base_url(provider_id) {
                providers::find_by_id(provider_id)
                    .map(|def| def.default_base_url.to_string())
                    .unwrap_or_else(|| base_url.unwrap_or("").to_string())
            } else {
                base_url.map(str::to_string).unwrap_or_default()
            };
            self.config.model = model.map(str::to_string).unwrap_or_else(|| {
                providers::find_by_id(provider_id)
                    .map(|def| def.default_model.to_string())
                    .unwrap_or_default()
            });
            self.config.custom_model_name = custom_model_name.to_string();
            self.config.api_key = api_key.to_string();
            self.config.assistant_id = assistant_id.to_string();
            self.config.auth_source =
                if providers::supported_auth_sources_for(provider_id).contains(&auth_source) {
                    auth_source.to_string()
                } else {
                    providers::default_auth_source_for(provider_id).to_string()
                };
            let supported_models =
                providers::known_models_for_provider_auth(provider_id, &self.config.auth_source);
            if supported_models
                .iter()
                .any(|entry| entry.id == self.config.model)
            {
                self.config.custom_model_name.clear();
            } else if self.config.custom_model_name.trim().is_empty()
                && !self.config.model.trim().is_empty()
            {
                self.config.custom_model_name = self.config.model.clone();
            }
            self.config.api_transport =
                if providers::supported_transports_for(provider_id).contains(&api_transport) {
                    api_transport.to_string()
                } else {
                    providers::default_transport_for(provider_id).to_string()
                };
            self.config.api_transport = self.provider_transport_snapshot(
                provider_id,
                &self.config.auth_source,
                &self.config.model,
                &self.config.api_transport,
            );
            self.config.custom_context_window_tokens = custom_context_window_tokens;
            self.config.context_window_tokens = json
                .get("context_window_tokens")
                .and_then(|v| v.as_u64())
                .map(|v| v.max(1000) as u32)
                .unwrap_or_else(|| {
                    Self::effective_context_window_for_provider_value(provider_id, provider_config)
                });
        }

        let get_bool = |camel_key: &str, snake_key: &str, default: bool| {
            json.get(snake_key)
                .and_then(|v| v.as_bool())
                .or_else(|| json.get(camel_key).and_then(|v| v.as_bool()))
                .unwrap_or(default)
        };
        let get_u32 = |camel_key: &str, snake_key: &str, default: u32| {
            json.get(snake_key)
                .and_then(|v| v.as_u64())
                .or_else(|| json.get(camel_key).and_then(|v| v.as_u64()))
                .map(|v| v as u32)
                .unwrap_or(default)
        };
        let get_str = |camel_key: &str, snake_key: &str| {
            json.get(snake_key)
                .and_then(|v| v.as_str())
                .or_else(|| json.get(camel_key).and_then(|v| v.as_str()))
                .unwrap_or("")
                .to_string()
        };

        self.config.tool_bash = json
            .get("tools")
            .and_then(|v| v.get("bash"))
            .and_then(|v| v.as_bool())
            .unwrap_or_else(|| get_bool("enableBashTool", "enable_bash_tool", true));
        self.config.tool_file_ops = json
            .get("tools")
            .and_then(|v| v.get("file_operations"))
            .and_then(|v| v.as_bool())
            .unwrap_or(self.config.tool_file_ops);
        self.config.tool_web_search = json
            .get("tools")
            .and_then(|v| v.get("web_search"))
            .and_then(|v| v.as_bool())
            .unwrap_or_else(|| get_bool("enableWebSearchTool", "enable_web_search_tool", false));
        self.config.tool_web_browse = json
            .get("tools")
            .and_then(|v| v.get("web_browse"))
            .and_then(|v| v.as_bool())
            .unwrap_or_else(|| {
                get_bool("enableWebBrowsingTool", "enable_web_browsing_tool", false)
            });
        self.config.tool_vision = json
            .get("tools")
            .and_then(|v| v.get("vision"))
            .and_then(|v| v.as_bool())
            .unwrap_or_else(|| get_bool("enableVisionTool", "enable_vision_tool", false));
        self.config.tool_system_info = json
            .get("tools")
            .and_then(|v| v.get("system_info"))
            .and_then(|v| v.as_bool())
            .unwrap_or(self.config.tool_system_info);
        self.config.tool_gateway = json
            .get("tools")
            .and_then(|v| v.get("gateway_messaging"))
            .and_then(|v| v.as_bool())
            .unwrap_or(self.config.gateway_enabled);

        self.config.reasoning_effort = {
            let effort = get_str("reasoning_effort", "reasoning_effort");
            if effort.is_empty() {
                self.config.reasoning_effort.clone()
            } else {
                effort
            }
        };

        self.config.search_provider = {
            let provider = get_str("search_provider", "search_provider");
            if provider.is_empty() {
                self.config.search_provider.clone()
            } else {
                provider
            }
        };
        self.config.firecrawl_api_key = get_str("firecrawl_api_key", "firecrawl_api_key");
        self.config.exa_api_key = get_str("exa_api_key", "exa_api_key");
        self.config.tavily_api_key = get_str("tavily_api_key", "tavily_api_key");
        self.config.search_max_results = get_u32("search_max_results", "search_max_results", 8);
        self.config.search_timeout_secs = get_u32("search_timeout_secs", "search_timeout_secs", 20);
        self.config.browse_provider = {
            let provider = get_str("browse_provider", "browse_provider");
            if provider.is_empty() {
                "auto".to_string()
            } else {
                provider
            }
        };
        self.config.enable_streaming = get_bool("enable_streaming", "enable_streaming", true);
        self.config.enable_conversation_memory = get_bool(
            "enable_conversation_memory",
            "enable_conversation_memory",
            true,
        );
        self.config.enable_honcho_memory =
            get_bool("enable_honcho_memory", "enable_honcho_memory", false);
        self.config.honcho_api_key = get_str("honcho_api_key", "honcho_api_key");
        self.config.honcho_base_url = get_str("honcho_base_url", "honcho_base_url");
        self.config.honcho_workspace_id = {
            let ws = get_str("honcho_workspace_id", "honcho_workspace_id");
            if ws.is_empty() {
                "tamux".to_string()
            } else {
                ws
            }
        };
        self.config.anticipatory_enabled = json
            .get("anticipatory")
            .and_then(|value| value.get("enabled"))
            .and_then(|value| value.as_bool())
            .unwrap_or(true);
        self.config.anticipatory_morning_brief = json
            .get("anticipatory")
            .and_then(|value| value.get("morning_brief"))
            .and_then(|value| value.as_bool())
            .unwrap_or(true);
        self.config.anticipatory_predictive_hydration = json
            .get("anticipatory")
            .and_then(|value| value.get("predictive_hydration"))
            .and_then(|value| value.as_bool())
            .unwrap_or(true);
        self.config.anticipatory_stuck_detection = json
            .get("anticipatory")
            .and_then(|value| value.get("stuck_detection"))
            .and_then(|value| value.as_bool())
            .unwrap_or(true);
        self.config.operator_model_enabled = json
            .get("operator_model")
            .and_then(|value| value.get("enabled"))
            .and_then(|value| value.as_bool())
            .unwrap_or(true);
        self.config.operator_model_allow_message_statistics = json
            .get("operator_model")
            .and_then(|value| value.get("allow_message_statistics"))
            .and_then(|value| value.as_bool())
            .unwrap_or(true);
        self.config.operator_model_allow_approval_learning = json
            .get("operator_model")
            .and_then(|value| value.get("allow_approval_learning"))
            .and_then(|value| value.as_bool())
            .unwrap_or(true);
        self.config.operator_model_allow_attention_tracking = json
            .get("operator_model")
            .and_then(|value| value.get("allow_attention_tracking"))
            .and_then(|value| value.as_bool())
            .unwrap_or(true);
        self.config.operator_model_allow_implicit_feedback = json
            .get("operator_model")
            .and_then(|value| value.get("allow_implicit_feedback"))
            .and_then(|value| value.as_bool())
            .unwrap_or(true);
        self.config.collaboration_enabled = json
            .get("collaboration")
            .and_then(|value| value.get("enabled"))
            .and_then(|value| value.as_bool())
            .unwrap_or(true);
        self.config.compliance_mode = json
            .get("compliance")
            .and_then(|value| value.get("mode"))
            .and_then(|value| value.as_str())
            .map(normalize_compliance_mode)
            .unwrap_or_else(|| "standard".to_string());
        self.config.compliance_retention_days = json
            .get("compliance")
            .and_then(|value| value.get("retention_days"))
            .and_then(|value| value.as_u64())
            .unwrap_or(30) as u32;
        self.config.compliance_sign_all_events = json
            .get("compliance")
            .and_then(|value| value.get("sign_all_events"))
            .and_then(|value| value.as_bool())
            .unwrap_or(true);
        self.config.tool_synthesis_enabled = json
            .get("tool_synthesis")
            .and_then(|value| value.get("enabled"))
            .and_then(|value| value.as_bool())
            .unwrap_or(true);
        self.config.tool_synthesis_require_activation = json
            .get("tool_synthesis")
            .and_then(|value| value.get("require_activation"))
            .and_then(|value| value.as_bool())
            .unwrap_or(true);
        self.config.tool_synthesis_max_generated_tools = json
            .get("tool_synthesis")
            .and_then(|value| value.get("max_generated_tools"))
            .and_then(|value| value.as_u64())
            .unwrap_or(24) as u32;
        self.config.managed_sandbox_enabled = json
            .get("managed_execution")
            .and_then(|value| value.get("sandbox_enabled"))
            .and_then(|value| value.as_bool())
            .unwrap_or(false);
        self.config.managed_security_level = json
            .get("managed_execution")
            .and_then(|value| value.get("security_level"))
            .and_then(|value| value.as_str())
            .map(normalize_managed_security_level)
            .unwrap_or_else(|| "lowest".to_string());

        self.config.auto_compact_context =
            get_bool("auto_compact_context", "auto_compact_context", true);
        self.config.max_context_messages =
            get_u32("max_context_messages", "max_context_messages", 100);
        self.config.tui_chat_history_page_size = get_u32(
            "tui_chat_history_page_size",
            "tui_chat_history_page_size",
            20,
        )
        .clamp(20, 500);
        self.config.max_tool_loops = get_u32("max_tool_loops", "max_tool_loops", 25);
        self.config.max_retries = get_u32("max_retries", "max_retries", 3);
        self.config.retry_delay_ms = get_u32("retry_delay_ms", "retry_delay_ms", 5_000);
        self.config.message_loop_delay_ms =
            get_u32("message_loop_delay_ms", "message_loop_delay_ms", 500);
        self.config.tool_call_delay_ms = get_u32("tool_call_delay_ms", "tool_call_delay_ms", 500);
        self.config.llm_stream_chunk_timeout_secs = get_u32(
            "llm_stream_chunk_timeout_secs",
            "llm_stream_chunk_timeout_secs",
            300,
        );
        self.config.auto_retry = get_bool("auto_retry", "auto_retry", true);
        self.config.compact_threshold_pct =
            get_u32("compact_threshold_pct", "compact_threshold_pct", 80);
        self.config.keep_recent_on_compact =
            get_u32("keep_recent_on_compact", "keep_recent_on_compact", 10);
        self.config.bash_timeout_secs = get_u32("bash_timeout_seconds", "bash_timeout_seconds", 30);
        self.config.weles_max_concurrent_reviews = json
            .get("builtin_sub_agents")
            .and_then(|value| value.get("weles"))
            .and_then(|value| value.get("max_concurrent_reviews"))
            .and_then(|value| value.as_u64())
            .map(|value| value.clamp(1, 16) as u32)
            .unwrap_or(2);

        let compaction = json.get("compaction");
        let builtin_weles = json
            .get("builtin_sub_agents")
            .and_then(|value| value.get("weles"));
        let compaction_strategy = compaction
            .and_then(|value| value.get("strategy"))
            .and_then(|value| value.as_str())
            .unwrap_or("heuristic");
        self.config.compaction_strategy = match compaction_strategy {
            "weles" => "weles".to_string(),
            "custom_model" => "custom_model".to_string(),
            _ => "heuristic".to_string(),
        };

        let weles_provider = compaction
            .and_then(|value| value.get("weles"))
            .and_then(|value| value.get("provider"))
            .and_then(|value| value.as_str())
            .filter(|value| !value.is_empty())
            .or_else(|| {
                builtin_weles
                    .and_then(|value| value.get("provider"))
                    .and_then(|value| value.as_str())
                    .filter(|value| !value.is_empty())
            })
            .unwrap_or(PROVIDER_ID_OPENAI);
        self.config.compaction_weles_provider = weles_provider.to_string();
        self.config.compaction_weles_model = compaction
            .and_then(|value| value.get("weles"))
            .and_then(|value| value.get("model"))
            .and_then(|value| value.as_str())
            .filter(|value| !value.is_empty())
            .or_else(|| {
                builtin_weles
                    .and_then(|value| value.get("model"))
                    .and_then(|value| value.as_str())
                    .filter(|value| !value.is_empty())
            })
            .map(str::to_string)
            .unwrap_or_else(|| {
                providers::default_model_for_provider_auth(weles_provider, "api_key")
            });
        self.config.compaction_weles_reasoning_effort = compaction
            .and_then(|value| value.get("weles"))
            .and_then(|value| value.get("reasoning_effort"))
            .and_then(|value| value.as_str())
            .filter(|value| !value.is_empty())
            .or_else(|| {
                builtin_weles
                    .and_then(|value| value.get("reasoning_effort"))
                    .and_then(|value| value.as_str())
                    .filter(|value| !value.is_empty())
            })
            .unwrap_or("medium")
            .to_string();

        let custom_provider = compaction
            .and_then(|value| value.get("custom_model"))
            .and_then(|value| value.get("provider"))
            .and_then(|value| value.as_str())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| self.config.provider.as_str());
        let custom_auth_source = normalize_provider_auth_source(
            custom_provider,
            compaction
                .and_then(|value| value.get("custom_model"))
                .and_then(|value| value.get("auth_source"))
                .and_then(|value| value.as_str())
                .filter(|value| !value.is_empty())
                .unwrap_or_else(|| providers::default_auth_source_for(custom_provider)),
        );
        let custom_api_transport = normalize_provider_transport(
            custom_provider,
            compaction
                .and_then(|value| value.get("custom_model"))
                .and_then(|value| value.get("api_transport"))
                .and_then(|value| value.as_str())
                .filter(|value| !value.is_empty())
                .unwrap_or_else(|| providers::default_transport_for(custom_provider)),
        );
        let default_custom_base_url = if custom_provider == self.config.provider {
            self.config.base_url.clone()
        } else {
            providers::find_by_id(custom_provider)
                .map(|def| def.default_base_url.to_string())
                .unwrap_or_default()
        };
        let default_custom_model = if custom_provider == self.config.provider {
            self.config.model.clone()
        } else {
            providers::default_model_for_provider_auth(custom_provider, &custom_auth_source)
        };
        let default_custom_context_window = if custom_provider == PROVIDER_ID_CUSTOM {
            128_000
        } else {
            providers::known_context_window_for(custom_provider, &default_custom_model)
                .unwrap_or(128_000)
        };
        self.config.compaction_custom_provider = custom_provider.to_string();
        self.config.compaction_custom_base_url = compaction
            .and_then(|value| value.get("custom_model"))
            .and_then(|value| value.get("base_url"))
            .and_then(|value| value.as_str())
            .map(str::to_string)
            .unwrap_or(default_custom_base_url);
        self.config.compaction_custom_model = compaction
            .and_then(|value| value.get("custom_model"))
            .and_then(|value| value.get("model"))
            .and_then(|value| value.as_str())
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .unwrap_or(default_custom_model);
        self.config.compaction_custom_api_key = compaction
            .and_then(|value| value.get("custom_model"))
            .and_then(|value| value.get("api_key"))
            .and_then(|value| value.as_str())
            .unwrap_or("")
            .to_string();
        self.config.compaction_custom_assistant_id = compaction
            .and_then(|value| value.get("custom_model"))
            .and_then(|value| value.get("assistant_id"))
            .and_then(|value| value.as_str())
            .unwrap_or("")
            .to_string();
        self.config.compaction_custom_auth_source = custom_auth_source;
        self.config.compaction_custom_api_transport = custom_api_transport;
        self.config.compaction_custom_reasoning_effort = compaction
            .and_then(|value| value.get("custom_model"))
            .and_then(|value| value.get("reasoning_effort"))
            .and_then(|value| value.as_str())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| self.config.reasoning_effort.as_str())
            .to_string();
        self.config.compaction_custom_context_window_tokens = compaction
            .and_then(|value| value.get("custom_model"))
            .and_then(|value| value.get("context_window_tokens"))
            .and_then(|value| value.as_u64())
            .map(|value| value.max(1000) as u32)
            .unwrap_or(default_custom_context_window);
        self.settings
            .clamp_field_cursor(self.settings.field_count_with_config(&self.config));

        self.config.snapshot_max_count = json
            .get("snapshot_retention")
            .and_then(|value| value.get("max_snapshots"))
            .and_then(|value| value.as_u64())
            .unwrap_or(0) as u32;
        self.config.snapshot_max_size_mb = json
            .get("snapshot_retention")
            .and_then(|value| value.get("max_total_size_mb"))
            .and_then(|value| value.as_u64())
            .unwrap_or(10_240) as u32;
        self.config.snapshot_auto_cleanup = json
            .get("snapshot_retention")
            .and_then(|value| value.get("auto_cleanup"))
            .and_then(|value| value.as_bool())
            .unwrap_or(false);

        if let Some(gateway) = json.get("gateway") {
            self.config.gateway_enabled = gateway
                .get("enabled")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            self.config.gateway_prefix = gateway
                .get("command_prefix")
                .and_then(|v| v.as_str())
                .unwrap_or("!tamux")
                .to_string();
            self.config.slack_token = gateway
                .get("slack_token")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            self.config.slack_channel_filter = gateway
                .get("slack_channel_filter")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            self.config.telegram_token = gateway
                .get("telegram_token")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            self.config.telegram_allowed_chats = gateway
                .get("telegram_allowed_chats")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            self.config.discord_token = gateway
                .get("discord_token")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            self.config.discord_channel_filter = gateway
                .get("discord_channel_filter")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            self.config.discord_allowed_users = gateway
                .get("discord_allowed_users")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            self.config.whatsapp_allowed_contacts = gateway
                .get("whatsapp_allowed_contacts")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            self.config.whatsapp_token = gateway
                .get("whatsapp_token")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            self.config.whatsapp_phone_id = gateway
                .get("whatsapp_phone_id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
        }

        // Hydrate tier from daemon config (override > self_assessment > "newcomer")
        if let Some(tier_config) = json.get("tier") {
            let tier_str = tier_config
                .get("user_override")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .or_else(|| {
                    tier_config
                        .get("user_self_assessment")
                        .and_then(|v| v.as_str())
                        .filter(|s| !s.is_empty())
                })
                .unwrap_or("newcomer");
            self.tier.on_tier_changed(tier_str);
        }

        self.config.agent_config_raw = Some(json.clone());
        self.config
            .reduce(config::ConfigAction::ConfigRawReceived(json.clone()));
        self.refresh_openai_auth_status();
        self.refresh_snapshot_stats();
    }

    pub(super) fn save_settings(&self) {}
}

fn normalize_managed_security_level(value: &str) -> String {
    match value.trim().to_ascii_lowercase().as_str() {
        "highest" => "highest".to_string(),
        "moderate" => "moderate".to_string(),
        "yolo" => "yolo".to_string(),
        _ => "lowest".to_string(),
    }
}

#[cfg(test)]
#[path = "tests/config_io.rs"]
mod tests;
