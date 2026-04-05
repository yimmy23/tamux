use super::*;
use amux_shared::providers::PROVIDER_ID_CUSTOM;

pub(super) fn normalize_provider_auth_source(provider_id: &str, auth_source: &str) -> String {
    if providers::supported_auth_sources_for(provider_id).contains(&auth_source) {
        auth_source.to_string()
    } else {
        providers::default_auth_source_for(provider_id).to_string()
    }
}

pub(super) fn normalize_provider_transport(provider_id: &str, api_transport: &str) -> String {
    if providers::supported_transports_for(provider_id).contains(&api_transport) {
        api_transport.to_string()
    } else {
        providers::default_transport_for(provider_id).to_string()
    }
}

pub(super) fn normalize_compliance_mode(mode: &str) -> String {
    match mode {
        "standard" | "soc2" | "hipaa" | "fedramp" => mode.to_string(),
        _ => "standard".to_string(),
    }
}

fn escape_pointer_segment(segment: &str) -> String {
    segment.replace('~', "~0").replace('/', "~1")
}

pub(super) fn flatten_config_value(
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
    pub(in super::super) fn provider_field_str<'a>(
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

    pub(in super::super) fn provider_field_u64(
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

    pub(in super::super) fn refresh_openai_auth_status(&mut self) {
        self.send_daemon_command(DaemonCommand::GetOpenAICodexAuthStatus);
    }

    pub(in super::super) fn effective_context_window_for_provider_value(
        provider_id: &str,
        provider_value: &serde_json::Value,
    ) -> u32 {
        if provider_id == PROVIDER_ID_CUSTOM {
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
        if self.config.provider == PROVIDER_ID_CUSTOM {
            self.config.custom_context_window_tokens.unwrap_or(128_000)
        } else {
            providers::known_context_window_for(&self.config.provider, &self.config.model)
                .unwrap_or(128_000)
        }
    }

    pub(in super::super) fn provider_config_value(&self, provider_id: &str) -> serde_json::Value {
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
            "context_window_tokens": if provider_id == PROVIDER_ID_CUSTOM { serde_json::Value::from(128_000u32) } else { serde_json::Value::Null },
        })
    }

    pub(in super::super) fn provider_wire_config_value(
        &self,
        provider_id: &str,
    ) -> serde_json::Value {
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
            "context_window_tokens": if provider_id == PROVIDER_ID_CUSTOM {
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

    pub(in super::super) fn refresh_snapshot_stats(&mut self) {
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
            let Some(name) = path.file_name().and_then(|value| value.to_str()) else {
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

    pub(in super::super) fn build_config_patch_value(&mut self) -> serde_json::Value {
        let mut patch = self
            .config
            .agent_config_raw
            .clone()
            .unwrap_or_else(|| serde_json::json!({}));

        patch["provider"] = serde_json::Value::String(self.config.provider.clone());
        patch["providers"] = serde_json::Value::Object(self.all_provider_wire_config_values());
        patch[&self.config.provider] = self.provider_wire_config_value(&self.config.provider);
        if let Some(providers) = patch["providers"].as_object_mut() {
            for value in providers.values_mut() {
                if let Some(map) = value.as_object_mut() {
                    map.remove("api_key");
                }
            }
        }
        if let Some(map) = patch[&self.config.provider].as_object_mut() {
            map.remove("api_key");
        }
        patch["base_url"] = serde_json::Value::String(self.config.base_url.clone());
        patch["model"] = serde_json::Value::String(self.config.model.clone());
        patch["reasoning_effort"] = serde_json::Value::String(self.config.reasoning_effort.clone());
        patch["context_window_tokens"] =
            serde_json::Value::from(self.effective_current_context_window() as u64);
        patch["search_provider"] = serde_json::Value::String(self.config.search_provider.clone());
        patch["firecrawl_api_key"] =
            serde_json::Value::String(self.config.firecrawl_api_key.clone());
        patch["exa_api_key"] = serde_json::Value::String(self.config.exa_api_key.clone());
        patch["tavily_api_key"] = serde_json::Value::String(self.config.tavily_api_key.clone());
        patch["search_max_results"] = serde_json::Value::from(self.config.search_max_results);
        patch["search_timeout_secs"] = serde_json::Value::from(self.config.search_timeout_secs);
        patch["browse_provider"] = serde_json::Value::String(self.config.browse_provider.clone());
        patch["enable_streaming"] = serde_json::Value::Bool(self.config.enable_streaming);
        patch["enable_conversation_memory"] =
            serde_json::Value::Bool(self.config.enable_conversation_memory);
        patch["enable_honcho_memory"] = serde_json::Value::Bool(self.config.enable_honcho_memory);
        patch["honcho_api_key"] = serde_json::Value::String(self.config.honcho_api_key.clone());
        patch["honcho_base_url"] = serde_json::Value::String(self.config.honcho_base_url.clone());
        patch["honcho_workspace_id"] =
            serde_json::Value::String(self.config.honcho_workspace_id.clone());
        patch["tool_gateway"] = serde_json::Value::Bool(self.config.tool_gateway);
        patch["tools"] = serde_json::json!({
            "bash": self.config.tool_bash,
            "file_operations": self.config.tool_file_ops,
            "web_search": self.config.tool_web_search,
            "web_browse": self.config.tool_web_browse,
            "vision": self.config.tool_vision,
            "system_info": self.config.tool_system_info,
            "gateway_messaging": self.config.tool_gateway,
        });
        patch["anticipatory"] = serde_json::json!({
            "enabled": self.config.anticipatory_enabled,
            "morning_brief": self.config.anticipatory_morning_brief,
            "predictive_hydration": self.config.anticipatory_predictive_hydration,
            "stuck_detection": self.config.anticipatory_stuck_detection,
        });
        patch["collaboration"] =
            serde_json::json!({ "enabled": self.config.collaboration_enabled });
        patch["operator_model"] = serde_json::json!({
            "enabled": self.config.operator_model_enabled,
            "allow_message_statistics": self.config.operator_model_allow_message_statistics,
            "allow_approval_learning": self.config.operator_model_allow_approval_learning,
            "allow_attention_tracking": self.config.operator_model_allow_attention_tracking,
            "allow_implicit_feedback": self.config.operator_model_allow_implicit_feedback,
        });
        patch["compliance"] = serde_json::json!({
            "mode": normalize_compliance_mode(&self.config.compliance_mode),
            "retention_days": self.config.compliance_retention_days,
            "sign_all_events": self.config.compliance_sign_all_events,
        });
        patch["tool_synthesis"] = serde_json::json!({
            "enabled": self.config.tool_synthesis_enabled,
            "require_activation": self.config.tool_synthesis_require_activation,
            "max_generated_tools": self.config.tool_synthesis_max_generated_tools,
        });
        patch["managed_execution"] = serde_json::json!({
            "sandbox_enabled": self.config.managed_sandbox_enabled,
            "security_level": self.config.managed_security_level,
        });
        patch["gateway"] = serde_json::json!({
            "enabled": self.config.gateway_enabled,
            "command_prefix": self.config.gateway_prefix,
            "slack_token": self.config.slack_token,
            "slack_channel_filter": self.config.slack_channel_filter,
            "telegram_token": self.config.telegram_token,
            "telegram_allowed_chats": self.config.telegram_allowed_chats,
            "discord_token": self.config.discord_token,
            "discord_channel_filter": self.config.discord_channel_filter,
            "discord_allowed_users": self.config.discord_allowed_users,
            "whatsapp_token": self.config.whatsapp_token,
            "whatsapp_phone_id": self.config.whatsapp_phone_id,
            "whatsapp_allowed_contacts": self.config.whatsapp_allowed_contacts,
        });
        patch["auto_compact_context"] = serde_json::Value::Bool(self.config.auto_compact_context);
        patch["max_context_messages"] = serde_json::Value::from(self.config.max_context_messages);
        patch["max_tool_loops"] = serde_json::Value::from(self.config.max_tool_loops);
        patch["max_retries"] = serde_json::Value::from(self.config.max_retries);
        patch["retry_delay_ms"] = serde_json::Value::from(self.config.retry_delay_ms);
        patch["message_loop_delay_ms"] = serde_json::Value::from(self.config.message_loop_delay_ms);
        patch["tool_call_delay_ms"] = serde_json::Value::from(self.config.tool_call_delay_ms);
        patch["llm_stream_chunk_timeout_secs"] =
            serde_json::Value::from(self.config.llm_stream_chunk_timeout_secs);
        patch["auto_retry"] = serde_json::Value::Bool(self.config.auto_retry);
        patch["context_budget_tokens"] = serde_json::Value::from(self.config.context_budget_tokens);
        patch["compact_threshold_pct"] = serde_json::Value::from(self.config.compact_threshold_pct);
        patch["keep_recent_on_compact"] =
            serde_json::Value::from(self.config.keep_recent_on_compact);
        patch["bash_timeout_seconds"] = serde_json::Value::from(self.config.bash_timeout_secs);
        if patch.get("builtin_sub_agents").is_none() {
            patch["builtin_sub_agents"] = serde_json::json!({});
        }
        if patch["builtin_sub_agents"].get("weles").is_none() {
            patch["builtin_sub_agents"]["weles"] = serde_json::json!({});
        }
        patch["builtin_sub_agents"]["weles"]["max_concurrent_reviews"] =
            serde_json::Value::from(self.config.weles_max_concurrent_reviews);
        patch["compaction"] = serde_json::json!({
            "strategy": self.config.compaction_strategy,
            "weles": {
                "provider": self.config.compaction_weles_provider,
                "model": self.config.compaction_weles_model,
                "reasoning_effort": self.config.compaction_weles_reasoning_effort,
            },
            "custom_model": {
                "provider": self.config.compaction_custom_provider,
                "base_url": self.config.compaction_custom_base_url,
                "model": self.config.compaction_custom_model,
                "api_key": self.config.compaction_custom_api_key,
                "assistant_id": self.config.compaction_custom_assistant_id,
                "auth_source": normalize_provider_auth_source(
                    &self.config.compaction_custom_provider,
                    &self.config.compaction_custom_auth_source,
                ),
                "api_transport": normalize_provider_transport(
                    &self.config.compaction_custom_provider,
                    &self.config.compaction_custom_api_transport,
                ),
                "reasoning_effort": self.config.compaction_custom_reasoning_effort,
                "context_window_tokens": self.config.compaction_custom_context_window_tokens,
            },
        });
        patch["snapshot_retention"] = serde_json::json!({
            "max_snapshots": self.config.snapshot_max_count,
            "max_total_size_mb": self.config.snapshot_max_size_mb,
            "auto_cleanup": self.config.snapshot_auto_cleanup,
        });

        patch
    }
}
