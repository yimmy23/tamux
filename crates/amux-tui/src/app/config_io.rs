use super::*;

fn normalize_provider_auth_source(provider_id: &str, auth_source: &str) -> String {
    if providers::supported_auth_sources_for(provider_id).contains(&auth_source) {
        auth_source.to_string()
    } else {
        providers::default_auth_source_for(provider_id).to_string()
    }
}

fn normalize_provider_transport(provider_id: &str, api_transport: &str) -> String {
    if providers::supported_transports_for(provider_id).contains(&api_transport) {
        api_transport.to_string()
    } else {
        providers::default_transport_for(provider_id).to_string()
    }
}

fn normalize_compliance_mode(mode: &str) -> String {
    match mode {
        "standard" | "soc2" | "hipaa" | "fedramp" => mode.to_string(),
        _ => "standard".to_string(),
    }
}

fn escape_pointer_segment(segment: &str) -> String {
    segment.replace('~', "~0").replace('/', "~1")
}

fn flatten_config_value(
    value: &serde_json::Value,
    pointer: &str,
    items: &mut Vec<(String, serde_json::Value)>,
) {
    match value {
        serde_json::Value::Object(map) if !map.is_empty() => {
            for (key, child) in map {
                let next = format!("{}/{}", pointer, escape_pointer_segment(key));
                flatten_config_value(child, &next, items);
            }
        }
        other => items.push((pointer.to_string(), other.clone())),
    }
}

impl TuiModel {
    pub(super) fn provider_field_str<'a>(
        provider_value: &'a serde_json::Value,
        camel_case: &str,
        snake_case: &str,
    ) -> Option<&'a str> {
        provider_value
            .get(camel_case)
            .and_then(|value| value.as_str())
            .or_else(|| {
                provider_value
                    .get(snake_case)
                    .and_then(|value| value.as_str())
            })
    }

    pub(super) fn provider_field_u64(
        provider_value: &serde_json::Value,
        camel_case: &str,
        snake_case: &str,
    ) -> Option<u64> {
        provider_value
            .get(camel_case)
            .and_then(|value| value.as_u64())
            .or_else(|| {
                provider_value
                    .get(snake_case)
                    .and_then(|value| value.as_u64())
            })
    }

    pub(super) fn refresh_openai_auth_status(&mut self) {
        let status = crate::auth::openai_codex_auth_status();
        self.config.chatgpt_auth_available = status.available;
        self.config.chatgpt_auth_source = status.source;
    }

    pub(super) fn effective_context_window_for_provider_value(
        provider_id: &str,
        provider_value: &serde_json::Value,
    ) -> u32 {
        if provider_id == "custom" {
            return provider_value
                .get("context_window_tokens")
                .and_then(|value| value.as_u64())
                .map(|value| value.max(1000) as u32)
                .unwrap_or(128_000);
        }

        let model = provider_value
            .get("model")
            .and_then(|value| value.as_str())
            .unwrap_or("");
        providers::known_context_window_for(provider_id, model).unwrap_or(128_000)
    }

    fn effective_current_context_window(&self) -> u32 {
        if self.config.provider == "custom" {
            self.config.custom_context_window_tokens.unwrap_or(128_000)
        } else {
            providers::known_context_window_for(&self.config.provider, &self.config.model)
                .unwrap_or(128_000)
        }
    }

    pub(super) fn provider_config_value(&self, provider_id: &str) -> serde_json::Value {
        if provider_id == self.config.provider {
            return serde_json::json!({
                "base_url": &self.config.base_url,
                "model": &self.config.model,
                "custom_model_name": &self.config.custom_model_name,
                "api_key": &self.config.api_key,
                "assistant_id": &self.config.assistant_id,
                "api_transport": &self.config.api_transport,
                "auth_source": &self.config.auth_source,
                "context_window_tokens": self.config.custom_context_window_tokens,
            });
        }

        if let Some(existing) = self
            .config
            .agent_config_raw
            .as_ref()
            .and_then(|raw| raw.get(provider_id))
            .cloned()
        {
            return existing;
        }

        let def = providers::find_by_id(provider_id);
        serde_json::json!({
            "base_url": def.map(|entry| entry.default_base_url).unwrap_or(""),
            "model": def.map(|entry| entry.default_model).unwrap_or(""),
            "custom_model_name": "",
            "api_key": "",
            "assistant_id": "",
            "api_transport": providers::default_transport_for(provider_id),
            "auth_source": providers::default_auth_source_for(provider_id),
            "context_window_tokens": if provider_id == "custom" { serde_json::Value::from(128_000u32) } else { serde_json::Value::Null },
        })
    }

    pub(super) fn provider_wire_config_value(&self, provider_id: &str) -> serde_json::Value {
        let ui_value = self.provider_config_value(provider_id);
        let auth_source = normalize_provider_auth_source(
            provider_id,
            Self::provider_field_str(&ui_value, "auth_source", "auth_source")
                .unwrap_or(providers::default_auth_source_for(provider_id)),
        );
        let api_transport = normalize_provider_transport(
            provider_id,
            Self::provider_field_str(&ui_value, "api_transport", "api_transport")
                .unwrap_or(providers::default_transport_for(provider_id)),
        );
        serde_json::json!({
            "base_url": Self::provider_field_str(&ui_value, "base_url", "base_url").unwrap_or(""),
            "model": Self::provider_field_str(&ui_value, "model", "model").unwrap_or(""),
            "custom_model_name": Self::provider_field_str(&ui_value, "custom_model_name", "custom_model_name").unwrap_or(""),
            "api_key": Self::provider_field_str(&ui_value, "api_key", "api_key").unwrap_or(""),
            "assistant_id": Self::provider_field_str(&ui_value, "assistant_id", "assistant_id").unwrap_or(""),
            "auth_source": auth_source,
            "api_transport": api_transport,
            "reasoning_effort": &self.config.reasoning_effort,
            "context_window_tokens": if provider_id == "custom" {
                Self::provider_field_u64(&ui_value, "context_window_tokens", "context_window_tokens")
                    .unwrap_or(128_000)
            } else {
                providers::known_context_window_for(
                    provider_id,
                    Self::provider_field_str(&ui_value, "model", "model").unwrap_or(""),
                )
                .unwrap_or(128_000) as u64
            },
        })
    }

    fn all_provider_config_values(&self) -> serde_json::Map<String, serde_json::Value> {
        let mut providers_json = serde_json::Map::new();
        for provider in providers::PROVIDERS {
            providers_json.insert(
                provider.id.to_string(),
                self.provider_config_value(provider.id),
            );
        }
        providers_json
    }

    fn all_provider_wire_config_values(&self) -> serde_json::Map<String, serde_json::Value> {
        let mut providers_json = serde_json::Map::new();
        for provider in providers::PROVIDERS {
            providers_json.insert(
                provider.id.to_string(),
                self.provider_wire_config_value(provider.id),
            );
        }
        providers_json
    }

    fn refresh_snapshot_stats(&mut self) {
        let home = std::env::var("HOME")
            .or_else(|_| std::env::var("USERPROFILE"))
            .unwrap_or_default();
        if home.is_empty() {
            return;
        }

        let snapshot_dir = std::path::Path::new(&home).join(".tamux").join("snapshots");
        let Ok(entries) = std::fs::read_dir(snapshot_dir) else {
            self.config.snapshot_count = 0;
            self.config.snapshot_total_size_bytes = 0;
            return;
        };

        let mut count = 0usize;
        let mut total_size_bytes = 0u64;
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let Some(name) = path.file_name().and_then(|v| v.to_str()) else {
                continue;
            };
            if !name.starts_with("snap_") {
                continue;
            }
            if let Ok(metadata) = entry.metadata() {
                count += 1;
                total_size_bytes = total_size_bytes.saturating_add(metadata.len());
            }
        }

        self.config.snapshot_count = count;
        self.config.snapshot_total_size_bytes = total_size_bytes;
    }

    fn build_config_patch_value(&mut self) -> serde_json::Value {
        let mut raw = self
            .config
            .agent_config_raw
            .clone()
            .unwrap_or_else(|| serde_json::json!({}));
        let provider_id = self.config.provider.clone();
        raw["provider"] = serde_json::Value::String(self.config.provider.clone());
        raw["base_url"] = serde_json::Value::String(self.config.base_url.clone());
        raw["assistant_id"] = serde_json::Value::String(self.config.assistant_id.clone());
        raw["auth_source"] = serde_json::Value::String(normalize_provider_auth_source(
            &self.config.provider,
            &self.config.auth_source,
        ));
        raw["model"] = serde_json::Value::String(self.config.model.clone());
        raw["api_transport"] = serde_json::Value::String(normalize_provider_transport(
            &self.config.provider,
            &self.config.api_transport,
        ));
        raw["reasoning_effort"] = serde_json::Value::String(self.config.reasoning_effort.clone());
        raw["context_window_tokens"] =
            serde_json::Value::Number(self.effective_current_context_window().into());
        if raw
            .get("providers")
            .and_then(|value| value.as_object())
            .is_none()
        {
            raw["providers"] = serde_json::json!({});
        }
        raw["providers"][provider_id.as_str()] = serde_json::json!({
            "base_url": &self.config.base_url,
            "model": &self.config.model,
            "custom_model_name": &self.config.custom_model_name,
            "assistant_id": &self.config.assistant_id,
            "api_transport": normalize_provider_transport(&self.config.provider, &self.config.api_transport),
            "auth_source": normalize_provider_auth_source(&self.config.provider, &self.config.auth_source),
            "reasoning_effort": &self.config.reasoning_effort,
            "context_window_tokens": self.effective_current_context_window(),
        });
        raw[provider_id.as_str()] = serde_json::json!({
            "base_url": &self.config.base_url,
            "model": &self.config.model,
            "custom_model_name": &self.config.custom_model_name,
            "assistant_id": &self.config.assistant_id,
            "api_transport": normalize_provider_transport(&self.config.provider, &self.config.api_transport),
            "auth_source": normalize_provider_auth_source(&self.config.provider, &self.config.auth_source),
            "context_window_tokens": self.config.custom_context_window_tokens,
        });
        raw["tools"] = serde_json::json!({
            "bash": self.config.tool_bash,
            "file_operations": self.config.tool_file_ops,
            "web_search": self.config.tool_web_search,
            "web_browse": self.config.tool_web_browse,
            "vision": self.config.tool_vision,
            "system_info": self.config.tool_system_info,
            "gateway_messaging": self.config.tool_gateway,
            "workspace": false,
            "terminal_management": false,
            "managed_commands": false,
        });
        raw["search_provider"] = serde_json::Value::String(self.config.search_provider.clone());
        raw["firecrawl_api_key"] = serde_json::Value::String(self.config.firecrawl_api_key.clone());
        raw["exa_api_key"] = serde_json::Value::String(self.config.exa_api_key.clone());
        raw["tavily_api_key"] = serde_json::Value::String(self.config.tavily_api_key.clone());
        raw["search_max_results"] =
            serde_json::Value::Number(self.config.search_max_results.into());
        raw["search_timeout_secs"] =
            serde_json::Value::Number(self.config.search_timeout_secs.into());
        raw["enable_streaming"] = serde_json::Value::Bool(self.config.enable_streaming);
        raw["enable_conversation_memory"] =
            serde_json::Value::Bool(self.config.enable_conversation_memory);
        raw["enable_honcho_memory"] = serde_json::Value::Bool(self.config.enable_honcho_memory);
        raw["honcho_api_key"] = serde_json::Value::String(self.config.honcho_api_key.clone());
        raw["honcho_base_url"] = serde_json::Value::String(self.config.honcho_base_url.clone());
        raw["honcho_workspace_id"] =
            serde_json::Value::String(self.config.honcho_workspace_id.clone());
        raw["anticipatory"] = serde_json::json!({
            "enabled": self.config.anticipatory_enabled,
            "morning_brief": self.config.anticipatory_morning_brief,
            "predictive_hydration": self.config.anticipatory_predictive_hydration,
            "stuck_detection": self.config.anticipatory_stuck_detection,
        });
        raw["operator_model"] = serde_json::json!({
            "enabled": self.config.operator_model_enabled,
            "allow_message_statistics": self.config.operator_model_allow_message_statistics,
            "allow_approval_learning": self.config.operator_model_allow_approval_learning,
            "allow_attention_tracking": self.config.operator_model_allow_attention_tracking,
            "allow_implicit_feedback": self.config.operator_model_allow_implicit_feedback,
        });
        raw["collaboration"] = serde_json::json!({
            "enabled": self.config.collaboration_enabled,
        });
        raw["compliance"] = serde_json::json!({
            "mode": normalize_compliance_mode(&self.config.compliance_mode),
            "retention_days": self.config.compliance_retention_days,
            "sign_all_events": self.config.compliance_sign_all_events,
        });
        raw["tool_synthesis"] = serde_json::json!({
            "enabled": self.config.tool_synthesis_enabled,
            "require_activation": self.config.tool_synthesis_require_activation,
            "max_generated_tools": self.config.tool_synthesis_max_generated_tools,
        });
        raw["managed_execution"] = serde_json::json!({
            "sandbox_enabled": self.config.managed_sandbox_enabled,
            "security_level": normalize_managed_security_level(&self.config.managed_security_level),
        });
        raw["snapshot_retention"] = serde_json::json!({
            "max_snapshots": self.config.snapshot_max_count,
            "max_total_size_mb": self.config.snapshot_max_size_mb,
            "auto_cleanup": self.config.snapshot_auto_cleanup,
        });
        raw["max_tool_loops"] = serde_json::Value::Number(self.config.max_tool_loops.into());
        raw["max_retries"] = serde_json::Value::Number(self.config.max_retries.into());
        raw["retry_delay_ms"] = serde_json::Value::Number(self.config.retry_delay_ms.into());
        raw["max_context_messages"] =
            serde_json::Value::Number(self.config.max_context_messages.into());
        raw["context_budget_tokens"] =
            serde_json::Value::Number(self.config.context_budget_tokens.into());
        raw["compact_threshold_pct"] =
            serde_json::Value::Number(self.config.compact_threshold_pct.into());
        raw["keep_recent_on_compact"] =
            serde_json::Value::Number(self.config.keep_recent_on_compact.into());
        raw["bash_timeout_seconds"] =
            serde_json::Value::Number(self.config.bash_timeout_secs.into());
        raw["gateway"] = serde_json::json!({
            "enabled": self.config.gateway_enabled,
            "command_prefix": &self.config.gateway_prefix,
            "slack_token": &self.config.slack_token,
            "slack_channel_filter": &self.config.slack_channel_filter,
            "telegram_token": &self.config.telegram_token,
            "telegram_allowed_chats": &self.config.telegram_allowed_chats,
            "discord_token": &self.config.discord_token,
            "discord_channel_filter": &self.config.discord_channel_filter,
            "discord_allowed_users": &self.config.discord_allowed_users,
            "whatsapp_allowed_contacts": &self.config.whatsapp_allowed_contacts,
            "whatsapp_token": &self.config.whatsapp_token,
            "whatsapp_phone_id": &self.config.whatsapp_phone_id,
        });
        self.config.agent_config_raw = Some(raw);
        self.config
            .agent_config_raw
            .clone()
            .unwrap_or_else(|| serde_json::json!({}))
    }

    pub(super) fn sync_config_to_daemon(&mut self) {
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
        let after = self.build_config_patch_value();

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
            .unwrap_or("openai");

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
            let api_transport = json
                .get("api_transport")
                .and_then(|v| v.as_str())
                .filter(|v| !v.is_empty())
                .or_else(|| {
                    Self::provider_field_str(provider_config, "api_transport", "api_transport")
                })
                .unwrap_or_else(|| providers::default_transport_for(provider_id));
            let auth_source = json
                .get("auth_source")
                .and_then(|v| v.as_str())
                .filter(|v| !v.is_empty())
                .or_else(|| Self::provider_field_str(provider_config, "auth_source", "auth_source"))
                .unwrap_or_else(|| providers::default_auth_source_for(provider_id));
            let custom_context_window_tokens = Self::provider_field_u64(
                provider_config,
                "context_window_tokens",
                "context_window_tokens",
            )
            .map(|v| v.max(1000) as u32);

            self.config.provider = provider_id.to_string();
            self.config.base_url = base_url.map(str::to_string).unwrap_or_else(|| {
                providers::find_by_id(provider_id)
                    .map(|def| def.default_base_url.to_string())
                    .unwrap_or_default()
            });
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
            .unwrap_or(false);
        self.config.anticipatory_morning_brief = json
            .get("anticipatory")
            .and_then(|value| value.get("morning_brief"))
            .and_then(|value| value.as_bool())
            .unwrap_or(false);
        self.config.anticipatory_predictive_hydration = json
            .get("anticipatory")
            .and_then(|value| value.get("predictive_hydration"))
            .and_then(|value| value.as_bool())
            .unwrap_or(false);
        self.config.anticipatory_stuck_detection = json
            .get("anticipatory")
            .and_then(|value| value.get("stuck_detection"))
            .and_then(|value| value.as_bool())
            .unwrap_or(false);
        self.config.operator_model_enabled = json
            .get("operator_model")
            .and_then(|value| value.get("enabled"))
            .and_then(|value| value.as_bool())
            .unwrap_or(false);
        self.config.operator_model_allow_message_statistics = json
            .get("operator_model")
            .and_then(|value| value.get("allow_message_statistics"))
            .and_then(|value| value.as_bool())
            .unwrap_or(false);
        self.config.operator_model_allow_approval_learning = json
            .get("operator_model")
            .and_then(|value| value.get("allow_approval_learning"))
            .and_then(|value| value.as_bool())
            .unwrap_or(false);
        self.config.operator_model_allow_attention_tracking = json
            .get("operator_model")
            .and_then(|value| value.get("allow_attention_tracking"))
            .and_then(|value| value.as_bool())
            .unwrap_or(false);
        self.config.operator_model_allow_implicit_feedback = json
            .get("operator_model")
            .and_then(|value| value.get("allow_implicit_feedback"))
            .and_then(|value| value.as_bool())
            .unwrap_or(false);
        self.config.collaboration_enabled = json
            .get("collaboration")
            .and_then(|value| value.get("enabled"))
            .and_then(|value| value.as_bool())
            .unwrap_or(false);
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
            .unwrap_or(false);
        self.config.tool_synthesis_enabled = json
            .get("tool_synthesis")
            .and_then(|value| value.get("enabled"))
            .and_then(|value| value.as_bool())
            .unwrap_or(false);
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
        self.config.max_tool_loops = get_u32("max_tool_loops", "max_tool_loops", 25);
        self.config.max_retries = get_u32("max_retries", "max_retries", 3);
        self.config.retry_delay_ms = get_u32("retry_delay_ms", "retry_delay_ms", 2000);
        self.config.context_budget_tokens =
            get_u32("context_budget_tokens", "context_budget_tokens", 100000);
        self.config.compact_threshold_pct =
            get_u32("compact_threshold_pct", "compact_threshold_pct", 80);
        self.config.keep_recent_on_compact =
            get_u32("keep_recent_on_compact", "keep_recent_on_compact", 10);
        self.config.bash_timeout_secs = get_u32("bash_timeout_seconds", "bash_timeout_seconds", 30);

        self.config.snapshot_max_count = json
            .get("snapshot_retention")
            .and_then(|value| value.get("max_snapshots"))
            .and_then(|value| value.as_u64())
            .unwrap_or(10) as u32;
        self.config.snapshot_max_size_mb = json
            .get("snapshot_retention")
            .and_then(|value| value.get("max_total_size_mb"))
            .and_then(|value| value.as_u64())
            .unwrap_or(51_200) as u32;
        self.config.snapshot_auto_cleanup = json
            .get("snapshot_retention")
            .and_then(|value| value.get("auto_cleanup"))
            .and_then(|value| value.as_bool())
            .unwrap_or(true);

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
            if self.config.gateway_enabled {
                self.config.tool_gateway = true;
            }
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
mod tests {
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
}
