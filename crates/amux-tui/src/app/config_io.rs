use super::*;

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
                .get("customContextWindowTokens")
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

    fn provider_config_value(&self, provider_id: &str) -> serde_json::Value {
        if provider_id == self.config.provider {
            return serde_json::json!({
                "baseUrl": &self.config.base_url,
                "model": &self.config.model,
                "apiKey": &self.config.api_key,
                "assistantId": &self.config.assistant_id,
                "apiTransport": &self.config.api_transport,
                "authSource": &self.config.auth_source,
                "customContextWindowTokens": self.config.custom_context_window_tokens,
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
            "baseUrl": def.map(|entry| entry.default_base_url).unwrap_or(""),
            "model": def.map(|entry| entry.default_model).unwrap_or(""),
            "apiKey": "",
            "assistantId": "",
            "apiTransport": providers::default_transport_for(provider_id),
            "authSource": providers::default_auth_source_for(provider_id),
            "customContextWindowTokens": if provider_id == "custom" { serde_json::Value::from(128_000u32) } else { serde_json::Value::Null },
        })
    }

    fn provider_wire_config_value(&self, provider_id: &str) -> serde_json::Value {
        let ui_value = self.provider_config_value(provider_id);
        serde_json::json!({
            "base_url": Self::provider_field_str(&ui_value, "baseUrl", "base_url").unwrap_or(""),
            "model": Self::provider_field_str(&ui_value, "model", "model").unwrap_or(""),
            "api_key": Self::provider_field_str(&ui_value, "apiKey", "api_key").unwrap_or(""),
            "assistant_id": Self::provider_field_str(&ui_value, "assistantId", "assistant_id").unwrap_or(""),
            "auth_source": Self::provider_field_str(&ui_value, "authSource", "auth_source").unwrap_or(providers::default_auth_source_for(provider_id)),
            "api_transport": Self::provider_field_str(&ui_value, "apiTransport", "api_transport").unwrap_or(providers::default_transport_for(provider_id)),
            "reasoning_effort": &self.config.reasoning_effort,
            "context_window_tokens": if provider_id == "custom" {
                Self::provider_field_u64(&ui_value, "customContextWindowTokens", "context_window_tokens")
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

    pub(super) fn sync_config_to_daemon(&mut self) {
        let ui_providers_json = self.all_provider_config_values();
        let daemon_providers_json = self.all_provider_wire_config_values();
        let mut raw = self
            .config
            .agent_config_raw
            .clone()
            .unwrap_or_else(|| serde_json::json!({}));
        raw["activeProvider"] = serde_json::Value::String(self.config.provider.clone());
        raw["reasoningEffort"] = serde_json::Value::String(self.config.reasoning_effort.clone());
        raw["contextWindowTokens"] =
            serde_json::Value::Number(self.effective_current_context_window().into());
        for (provider_id, provider_value) in &ui_providers_json {
            raw[provider_id] = provider_value.clone();
        }
        self.config.agent_config_raw = Some(raw);
        if let Ok(json) = serde_json::to_string(&serde_json::json!({
            "provider": &self.config.provider,
            "base_url": &self.config.base_url,
            "api_key": &self.config.api_key,
            "assistant_id": &self.config.assistant_id,
            "auth_source": &self.config.auth_source,
            "model": &self.config.model,
            "api_transport": &self.config.api_transport,
            "reasoning_effort": &self.config.reasoning_effort,
            "context_window_tokens": self.effective_current_context_window(),
            "providers": daemon_providers_json,
            "tools": {
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
            },
            "search_provider": &self.config.search_provider,
            "firecrawl_api_key": &self.config.firecrawl_api_key,
            "exa_api_key": &self.config.exa_api_key,
            "tavily_api_key": &self.config.tavily_api_key,
            "search_max_results": self.config.search_max_results,
            "search_timeout_secs": self.config.search_timeout_secs,
            "enable_streaming": self.config.enable_streaming,
            "snapshot_retention": {
                "max_snapshots": self.config.snapshot_max_count,
                "max_total_size_mb": self.config.snapshot_max_size_mb,
                "auto_cleanup": self.config.snapshot_auto_cleanup,
            },
            "max_tool_loops": self.config.max_tool_loops,
            "max_retries": self.config.max_retries,
            "retry_delay_ms": self.config.retry_delay_ms,
            "context_budget_tokens": self.config.context_budget_tokens,
            "gateway": {
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
            },
        })) {
            self.send_daemon_command(DaemonCommand::SetConfigJson(json));
        }
        self.save_settings();
    }

    pub fn load_saved_settings(&mut self) {
        let home = std::env::var("HOME")
            .or_else(|_| std::env::var("USERPROFILE"))
            .unwrap_or_default();
        let path = format!("{}/.tamux/agent-settings.json", home);
        tracing::info!("Loading settings from: {}", path);
        let Ok(data) = std::fs::read_to_string(&path) else {
            return;
        };
        let Ok(json): Result<serde_json::Value, _> = serde_json::from_str(&data) else {
            return;
        };

        let provider_id = json
            .get("activeProvider")
            .and_then(|v| v.as_str())
            .unwrap_or("openai");

        if let Some(provider_config) = json.get(provider_id) {
            let base_url =
                Self::provider_field_str(provider_config, "baseUrl", "base_url").unwrap_or("");
            let model = Self::provider_field_str(provider_config, "model", "model").unwrap_or("");
            let api_key =
                Self::provider_field_str(provider_config, "apiKey", "api_key").unwrap_or("");
            let assistant_id =
                Self::provider_field_str(provider_config, "assistantId", "assistant_id")
                    .unwrap_or("");
            let api_transport =
                Self::provider_field_str(provider_config, "apiTransport", "api_transport")
                    .unwrap_or_else(|| providers::default_transport_for(provider_id));
            let auth_source =
                Self::provider_field_str(provider_config, "authSource", "auth_source")
                    .unwrap_or_else(|| providers::default_auth_source_for(provider_id));
            let custom_context_window_tokens = Self::provider_field_u64(
                provider_config,
                "customContextWindowTokens",
                "context_window_tokens",
            )
            .map(|v| v.max(1000) as u32);

            self.config.provider = provider_id.to_string();
            self.config.base_url = if !base_url.is_empty() {
                base_url.to_string()
            } else {
                providers::find_by_id(provider_id)
                    .map(|def| def.default_base_url.to_string())
                    .unwrap_or_default()
            };
            self.config.model = if !model.is_empty() {
                model.to_string()
            } else {
                providers::find_by_id(provider_id)
                    .map(|def| def.default_model.to_string())
                    .unwrap_or_default()
            };
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
            if !supported_models.is_empty()
                && !supported_models
                    .iter()
                    .any(|entry| entry.id == self.config.model)
            {
                self.config.model =
                    providers::default_model_for_provider_auth(provider_id, &self.config.auth_source);
            }
            self.config.api_transport =
                if providers::supported_transports_for(provider_id).contains(&api_transport) {
                    api_transport.to_string()
                } else {
                    providers::default_transport_for(provider_id).to_string()
                };
            self.config.custom_context_window_tokens = custom_context_window_tokens;
            self.config.context_window_tokens =
                Self::effective_context_window_for_provider_value(provider_id, provider_config);
        }

        let get_bool = |key: &str| json.get(key).and_then(|v| v.as_bool()).unwrap_or(false);
        let get_str = |key: &str| {
            json.get(key)
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string()
        };

        self.config.tool_bash = json
            .get("enableBashTool")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);
        self.config.tool_web_search = get_bool("enableWebSearchTool");
        self.config.tool_web_browse = get_bool("enableWebBrowsingTool");
        self.config.tool_vision = get_bool("enableVisionTool");

        if let Some(effort) = json.get("reasoningEffort").and_then(|v| v.as_str()) {
            self.config.reasoning_effort = effort.to_string();
        }

        self.config.search_provider = get_str("searchToolProvider");
        self.config.firecrawl_api_key = get_str("firecrawlApiKey");
        self.config.exa_api_key = get_str("exaApiKey");
        self.config.tavily_api_key = get_str("tavilyApiKey");
        self.config.search_max_results = json
            .get("searchMaxResults")
            .and_then(|v| v.as_u64())
            .unwrap_or(8) as u32;
        self.config.search_timeout_secs = json
            .get("searchTimeoutSeconds")
            .and_then(|v| v.as_u64())
            .unwrap_or(20) as u32;
        self.config.enable_streaming = json
            .get("enableStreaming")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);
        self.config.enable_conversation_memory = json
            .get("enableConversationMemory")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);
        self.config.enable_honcho_memory = get_bool("enableHonchoMemory");
        self.config.honcho_api_key = get_str("honchoApiKey");
        self.config.honcho_base_url = get_str("honchoBaseUrl");
        self.config.honcho_workspace_id = {
            let ws = get_str("honchoWorkspaceId");
            if ws.is_empty() {
                "tamux".to_string()
            } else {
                ws
            }
        };

        self.config.auto_compact_context = json
            .get("autoCompactContext")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);
        self.config.max_context_messages = json
            .get("maxContextMessages")
            .and_then(|v| v.as_u64())
            .unwrap_or(100) as u32;
        self.config.max_tool_loops = json
            .get("maxToolLoops")
            .and_then(|v| v.as_u64())
            .unwrap_or(25) as u32;
        self.config.max_retries =
            json.get("maxRetries").and_then(|v| v.as_u64()).unwrap_or(3) as u32;
        self.config.retry_delay_ms = json
            .get("retryDelayMs")
            .and_then(|v| v.as_u64())
            .unwrap_or(2000) as u32;
        self.config.context_budget_tokens = json
            .get("contextBudgetTokens")
            .and_then(|v| v.as_u64())
            .unwrap_or(100000) as u32;
        self.config.compact_threshold_pct = json
            .get("compactThresholdPercent")
            .and_then(|v| v.as_u64())
            .unwrap_or(80) as u32;
        self.config.keep_recent_on_compact = json
            .get("keepRecentOnCompaction")
            .and_then(|v| v.as_u64())
            .unwrap_or(10) as u32;
        self.config.bash_timeout_secs = json
            .get("bashTimeoutSeconds")
            .and_then(|v| v.as_u64())
            .unwrap_or(30) as u32;

        self.config.snapshot_max_count = json
            .get("snapshotMaxCount")
            .and_then(|v| v.as_u64())
            .unwrap_or(10) as u32;
        self.config.snapshot_max_size_mb = json
            .get("snapshotMaxSizeMb")
            .and_then(|v| v.as_u64())
            .unwrap_or(51_200) as u32;
        self.config.snapshot_auto_cleanup = json
            .get("snapshotAutoCleanup")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);

        let settings_path = format!("{}/.tamux/settings.json", home);
        if let Ok(settings_data) = std::fs::read_to_string(&settings_path) {
            if let Ok(settings_json) = serde_json::from_str::<serde_json::Value>(&settings_data) {
                self.config.gateway_enabled = settings_json
                    .get("gatewayEnabled")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                self.config.gateway_prefix = settings_json
                    .get("gatewayCommandPrefix")
                    .and_then(|v| v.as_str())
                    .unwrap_or("!tamux")
                    .to_string();
                self.config.slack_token = settings_json
                    .get("slackToken")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                self.config.slack_channel_filter = settings_json
                    .get("slackChannelFilter")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                self.config.telegram_token = settings_json
                    .get("telegramToken")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                self.config.telegram_allowed_chats = settings_json
                    .get("telegramAllowedChats")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                self.config.discord_token = settings_json
                    .get("discordToken")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                self.config.discord_channel_filter = settings_json
                    .get("discordChannelFilter")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                self.config.discord_allowed_users = settings_json
                    .get("discordAllowedUsers")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                self.config.whatsapp_allowed_contacts = settings_json
                    .get("whatsappAllowedContacts")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                self.config.whatsapp_token = settings_json
                    .get("whatsappToken")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                self.config.whatsapp_phone_id = settings_json
                    .get("whatsappPhoneNumberId")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                if self.config.gateway_enabled {
                    self.config.tool_gateway = true;
                }
            }
        }

        self.config.agent_config_raw = Some(json);
        self.refresh_openai_auth_status();
        self.refresh_snapshot_stats();
    }

    pub(super) fn save_settings(&self) {
        let home = std::env::var("HOME")
            .or_else(|_| std::env::var("USERPROFILE"))
            .unwrap_or_default();
        if home.is_empty() {
            return;
        }

        let dir = format!("{}/.tamux", home);
        let _ = std::fs::create_dir_all(&dir);
        let path = format!("{}/.tamux/agent-settings.json", home);

        let mut json: serde_json::Value = if let Ok(data) = std::fs::read_to_string(&path) {
            serde_json::from_str(&data).unwrap_or_else(|_| serde_json::json!({}))
        } else {
            serde_json::json!({})
        };

        json["activeProvider"] = serde_json::Value::String(self.config.provider.clone());
        json["reasoningEffort"] = serde_json::Value::String(self.config.reasoning_effort.clone());
        json["apiTransport"] = serde_json::Value::String(self.config.api_transport.clone());
        json["contextWindowTokens"] =
            serde_json::Value::Number(self.effective_current_context_window().into());
        json["enableBashTool"] = serde_json::Value::Bool(self.config.tool_bash);
        json["enableWebSearchTool"] = serde_json::Value::Bool(self.config.tool_web_search);
        json["enableWebBrowsingTool"] = serde_json::Value::Bool(self.config.tool_web_browse);
        json["enableVisionTool"] = serde_json::Value::Bool(self.config.tool_vision);
        json["searchToolProvider"] = serde_json::Value::String(self.config.search_provider.clone());
        json["firecrawlApiKey"] = serde_json::Value::String(self.config.firecrawl_api_key.clone());
        json["exaApiKey"] = serde_json::Value::String(self.config.exa_api_key.clone());
        json["tavilyApiKey"] = serde_json::Value::String(self.config.tavily_api_key.clone());
        json["searchMaxResults"] = serde_json::Value::Number(self.config.search_max_results.into());
        json["searchTimeoutSeconds"] =
            serde_json::Value::Number(self.config.search_timeout_secs.into());
        json["enableStreaming"] = serde_json::Value::Bool(self.config.enable_streaming);
        json["enableConversationMemory"] =
            serde_json::Value::Bool(self.config.enable_conversation_memory);
        json["enableHonchoMemory"] = serde_json::Value::Bool(self.config.enable_honcho_memory);
        json["honchoApiKey"] = serde_json::Value::String(self.config.honcho_api_key.clone());
        json["honchoBaseUrl"] = serde_json::Value::String(self.config.honcho_base_url.clone());
        json["honchoWorkspaceId"] =
            serde_json::Value::String(self.config.honcho_workspace_id.clone());
        json["autoCompactContext"] = serde_json::Value::Bool(self.config.auto_compact_context);
        json["maxContextMessages"] =
            serde_json::Value::Number(self.config.max_context_messages.into());
        json["maxToolLoops"] = serde_json::Value::Number(self.config.max_tool_loops.into());
        json["maxRetries"] = serde_json::Value::Number(self.config.max_retries.into());
        json["retryDelayMs"] = serde_json::Value::Number(self.config.retry_delay_ms.into());
        json["contextBudgetTokens"] =
            serde_json::Value::Number(self.config.context_budget_tokens.into());
        json["compactThresholdPercent"] =
            serde_json::Value::Number(self.config.compact_threshold_pct.into());
        json["keepRecentOnCompaction"] =
            serde_json::Value::Number(self.config.keep_recent_on_compact.into());
        json["bashTimeoutSeconds"] =
            serde_json::Value::Number(self.config.bash_timeout_secs.into());
        json["snapshotMaxCount"] = serde_json::Value::Number(self.config.snapshot_max_count.into());
        json["snapshotMaxSizeMb"] =
            serde_json::Value::Number(self.config.snapshot_max_size_mb.into());
        json["snapshotAutoCleanup"] = serde_json::Value::Bool(self.config.snapshot_auto_cleanup);
        json[&self.config.provider] = serde_json::json!({
            "baseUrl": &self.config.base_url,
            "model": &self.config.model,
            "apiKey": &self.config.api_key,
            "assistantId": &self.config.assistant_id,
            "apiTransport": &self.config.api_transport,
            "authSource": &self.config.auth_source,
            "customContextWindowTokens": self.config.custom_context_window_tokens,
        });

        if let Ok(data) = serde_json::to_string_pretty(&json) {
            if let Err(e) = std::fs::write(&path, data) {
                tracing::warn!("Failed to write agent-settings.json: {}", e);
            }
        }

        let settings_path = format!("{}/.tamux/settings.json", home);
        let mut settings_json: serde_json::Value =
            if let Ok(data) = std::fs::read_to_string(&settings_path) {
                serde_json::from_str(&data).unwrap_or_else(|_| serde_json::json!({}))
            } else {
                serde_json::json!({})
            };
        settings_json["gatewayEnabled"] = serde_json::Value::Bool(self.config.gateway_enabled);
        settings_json["gatewayCommandPrefix"] =
            serde_json::Value::String(self.config.gateway_prefix.clone());
        settings_json["slackToken"] = serde_json::Value::String(self.config.slack_token.clone());
        settings_json["slackChannelFilter"] =
            serde_json::Value::String(self.config.slack_channel_filter.clone());
        settings_json["telegramToken"] =
            serde_json::Value::String(self.config.telegram_token.clone());
        settings_json["telegramAllowedChats"] =
            serde_json::Value::String(self.config.telegram_allowed_chats.clone());
        settings_json["discordToken"] =
            serde_json::Value::String(self.config.discord_token.clone());
        settings_json["discordChannelFilter"] =
            serde_json::Value::String(self.config.discord_channel_filter.clone());
        settings_json["discordAllowedUsers"] =
            serde_json::Value::String(self.config.discord_allowed_users.clone());
        settings_json["whatsappAllowedContacts"] =
            serde_json::Value::String(self.config.whatsapp_allowed_contacts.clone());
        settings_json["whatsappToken"] =
            serde_json::Value::String(self.config.whatsapp_token.clone());
        settings_json["whatsappPhoneNumberId"] =
            serde_json::Value::String(self.config.whatsapp_phone_id.clone());
        if let Ok(data) = serde_json::to_string_pretty(&settings_json) {
            if let Err(e) = std::fs::write(&settings_path, data) {
                tracing::warn!("Failed to write settings.json: {}", e);
            }
        }
    }
}
