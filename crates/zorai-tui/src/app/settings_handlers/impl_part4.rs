impl TuiModel {
    fn openrouter_endpoint_url_for(model: &str, base_url: &str) -> Option<String> {
        let (author, slug) = model.trim().split_once('/')?;
        if author.trim().is_empty() || slug.trim().is_empty() {
            return None;
        }
        let base_url = if base_url.trim().is_empty() {
            providers::find_by_id(PROVIDER_ID_OPENROUTER)
                .map(|def| def.default_base_url)
                .unwrap_or("https://openrouter.ai/api/v1")
        } else {
            base_url.trim()
        }
        .trim_end_matches('/');
        let base_url = base_url
            .strip_suffix("/chat/completions")
            .or_else(|| base_url.strip_suffix("/responses"))
            .unwrap_or(base_url);
        Some(format!("{base_url}/models/{author}/{slug}/endpoints"))
    }

    fn fetch_openrouter_endpoint_provider_slugs_for(
        model: &str,
        base_url: &str,
        api_key: &str,
    ) -> Result<Vec<String>, String> {
        let url = Self::openrouter_endpoint_url_for(model, base_url)
            .ok_or_else(|| "OpenRouter model id must look like author/model".to_string())?;
        let mut request = ureq::get(&url)
            .config()
            .timeout_global(Some(std::time::Duration::from_secs(8)))
            .build();
        if !api_key.trim().is_empty() {
            request = request.header(
                "Authorization",
                format!("Bearer {}", api_key.trim()),
            );
        }
        let mut response = request.call().map_err(|error| error.to_string())?;
        let body = response
            .body_mut()
            .read_to_string()
            .map_err(|error| error.to_string())?;
        let payload: serde_json::Value =
            serde_json::from_str(&body).map_err(|error| error.to_string())?;
        let mut slugs = Vec::new();
        if let Some(endpoints) = payload
            .get("data")
            .and_then(|data| data.get("endpoints"))
            .and_then(|endpoints| endpoints.as_array())
        {
            for endpoint in endpoints {
                let Some(slug) = endpoint
                    .get("tag")
                    .and_then(|value| value.as_str())
                    .or_else(|| endpoint.get("slug").and_then(|value| value.as_str()))
                    .or_else(|| {
                        endpoint
                            .get("provider_slug")
                            .and_then(|value| value.as_str())
                    })
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                else {
                    continue;
                };
                if !slugs.iter().any(|existing| existing == slug) {
                    slugs.push(slug.to_string());
                }
            }
        }
        Ok(slugs)
    }

    fn open_openrouter_provider_picker_for(
        &mut self,
        target: SettingsPickerTarget,
        model: String,
        base_url: String,
        api_key: String,
    ) {
        match Self::fetch_openrouter_endpoint_provider_slugs_for(&model, &base_url, &api_key) {
            Ok(slugs) if !slugs.is_empty() => {
                self.config.openrouter_endpoint_providers = slugs;
                self.settings_picker_target = Some(target);
                self.modal.reduce(modal::ModalAction::Push(
                    modal::ModalKind::OpenRouterProviderPicker,
                ));
                self.sync_openrouter_provider_picker_item_count();
                self.status_line = "OpenRouter endpoint providers loaded".to_string();
            }
            Ok(_) => {
                self.config.openrouter_endpoint_providers.clear();
                self.status_line =
                    "OpenRouter returned no endpoint providers for this model".to_string();
            }
            Err(error) => {
                self.config.openrouter_endpoint_providers.clear();
                self.status_line = format!("OpenRouter provider lookup failed: {error}");
            }
        }
    }

    fn open_openrouter_provider_picker(&mut self, target: SettingsPickerTarget) {
        if self.config.provider != PROVIDER_ID_OPENROUTER {
            self.status_line = "OpenRouter provider routing only applies to OpenRouter".to_string();
            return;
        }
        self.open_openrouter_provider_picker_for(
            target,
            self.config.model.clone(),
            self.config.base_url.clone(),
            self.config.api_key.clone(),
        );
    }

    pub(super) fn activate_settings_field(&mut self) {
        let field = self.current_settings_field_name().to_string();
        match field.as_str() {
            "provider" => {
                self.open_provider_picker(SettingsPickerTarget::Provider);
            }
            "model" => {
                if self.config.provider == PROVIDER_ID_CUSTOM {
                    self.begin_custom_model_edit();
                } else {
                    self.open_provider_backed_model_picker(
                        SettingsPickerTarget::Model,
                        self.config.provider.clone(),
                        self.config.base_url.clone(),
                        self.config.api_key.clone(),
                        self.config.auth_source.clone(),
                    );
                }
            }
            "auth_source" => {
                let supported = providers::supported_auth_sources_for(&self.config.provider);
                let current_idx = supported
                    .iter()
                    .position(|source| *source == self.config.auth_source)
                    .unwrap_or(0);
                let next_idx = (current_idx + 1) % supported.len().max(1);
                self.config.auth_source = supported
                    .get(next_idx)
                    .copied()
                    .unwrap_or("api_key")
                    .to_string();
                if self.config.provider == PROVIDER_ID_OPENAI
                    && self.config.auth_source == "chatgpt_subscription"
                {
                    self.refresh_openai_auth_status();
                }
                self.refresh_provider_models_for_current_auth();
                if self.config.provider == PROVIDER_ID_OPENAI
                    && self.config.auth_source == "chatgpt_subscription"
                {
                    self.config.api_transport = "responses".to_string();
                }
                self.sync_config_to_daemon();
            }
            "api_transport" => {
                if let Some(fixed_transport) =
                    providers::fixed_transport_for_model(&self.config.provider, &self.config.model)
                {
                    let transport_label = match fixed_transport {
                        "native_assistant" => "native assistant",
                        "anthropic_messages" => "anthropic messages",
                        "responses" => "responses",
                        _ => "chat completions",
                    };
                    self.status_line = format!("This model uses {transport_label} only.");
                    return;
                }
                if providers::uses_fixed_anthropic_messages(
                    &self.config.provider,
                    &self.config.model,
                ) {
                    self.status_line =
                        "This provider uses the Anthropic messages protocol.".to_string();
                    return;
                }
                let supported = providers::supported_transports_for(&self.config.provider);
                if supported.len() <= 1 {
                    let only = supported.first().copied().unwrap_or("chat_completions");
                    let transport_label = match only {
                        "native_assistant" => "native assistant",
                        "anthropic_messages" => "anthropic messages",
                        "responses" => "responses",
                        _ => "chat completions",
                    };
                    let provider_name = providers::find_by_id(&self.config.provider)
                        .map(|def| def.name)
                        .unwrap_or("This provider");
                    self.status_line = format!("{provider_name} supports {transport_label} only.");
                    return;
                }
                let current_idx = supported
                    .iter()
                    .position(|transport| *transport == self.config.api_transport)
                    .unwrap_or(0);
                let next_idx = (current_idx + 1) % supported.len().max(1);
                self.config.api_transport = supported
                    .get(next_idx)
                    .copied()
                    .unwrap_or("chat_completions")
                    .to_string();
                if self.config.provider == PROVIDER_ID_OPENAI
                    && self.config.auth_source == "chatgpt_subscription"
                {
                    self.config.api_transport = "responses".to_string();
                }
                self.sync_config_to_daemon();
            }
            "assistant_id" => self
                .settings
                .start_editing("assistant_id", &self.config.assistant_id.clone()),
            "reasoning_effort" => self.execute_command("effort"),
            "base_url" => self
                .settings
                .start_editing("base_url", &self.config.base_url.clone()),
            "openrouter_provider_order" => self.open_openrouter_provider_picker(
                SettingsPickerTarget::OpenRouterPreferredProviders,
            ),
            "openrouter_provider_ignore" => self
                .open_openrouter_provider_picker(SettingsPickerTarget::OpenRouterExcludedProviders),
            "openrouter_allow_fallbacks" => {
                if self.config.provider == PROVIDER_ID_OPENROUTER {
                    self.config.openrouter_allow_fallbacks =
                        !self.config.openrouter_allow_fallbacks;
                    self.sync_config_to_daemon();
                } else {
                    self.status_line =
                        "OpenRouter provider routing only applies to OpenRouter".to_string();
                }
            }
            "openrouter_response_cache_enabled" => {
                if self.config.provider == PROVIDER_ID_OPENROUTER {
                    self.config.openrouter_response_cache_enabled =
                        !self.config.openrouter_response_cache_enabled;
                    self.sync_config_to_daemon();
                } else {
                    self.status_line =
                        "OpenRouter response caching only applies to OpenRouter".to_string();
                }
            }
            "gateway_prefix" => {
                self.settings
                    .start_editing("gateway_prefix", &self.config.gateway_prefix.clone());
            }
            "slack_token" => {
                self.settings
                    .start_editing("slack_token", &self.config.slack_token.clone());
            }
            "slack_channel_filter" => {
                self.settings.start_editing(
                    "slack_channel_filter",
                    &self.config.slack_channel_filter.clone(),
                );
            }
            "telegram_token" => {
                self.settings
                    .start_editing("telegram_token", &self.config.telegram_token.clone());
            }
            "telegram_allowed_chats" => {
                self.settings.start_editing(
                    "telegram_allowed_chats",
                    &self.config.telegram_allowed_chats.clone(),
                );
            }
            "discord_token" => {
                self.settings
                    .start_editing("discord_token", &self.config.discord_token.clone());
            }
            "discord_channel_filter" => {
                self.settings.start_editing(
                    "discord_channel_filter",
                    &self.config.discord_channel_filter.clone(),
                );
            }
            "discord_allowed_users" => {
                self.settings.start_editing(
                    "discord_allowed_users",
                    &self.config.discord_allowed_users.clone(),
                );
            }
            "whatsapp_allowed_contacts" => {
                self.settings.start_editing(
                    "whatsapp_allowed_contacts",
                    &self.config.whatsapp_allowed_contacts.clone(),
                );
            }
            "whatsapp_token" => {
                self.settings
                    .start_editing("whatsapp_token", &self.config.whatsapp_token.clone());
            }
            "whatsapp_phone_id" => {
                self.settings
                    .start_editing("whatsapp_phone_id", &self.config.whatsapp_phone_id.clone());
            }
            "whatsapp_link_device" => {
                if !self.whatsapp_linking_allowed() {
                    self.status_line =
                        "Set at least one allowed WhatsApp phone number before linking".to_string();
                    return;
                }
                self.send_daemon_command(DaemonCommand::WhatsAppLinkSubscribe);
                self.send_daemon_command(DaemonCommand::WhatsAppLinkStatus);
                if self.modal.whatsapp_link().phase() == modal::WhatsAppLinkPhase::Connected {
                    self.status_line = "Showing WhatsApp link status".to_string();
                } else {
                    self.modal.set_whatsapp_link_starting();
                    self.send_daemon_command(DaemonCommand::WhatsAppLinkStart);
                    self.status_line = "Starting WhatsApp link workflow".to_string();
                }
                self.modal
                    .reduce(modal::ModalAction::Push(modal::ModalKind::WhatsAppLink));
            }
            "whatsapp_relink_device" => {
                if !self.whatsapp_linking_allowed() {
                    self.status_line =
                        "Set at least one allowed WhatsApp phone number before linking".to_string();
                    return;
                }
                if self.modal.whatsapp_link().phase() != modal::WhatsAppLinkPhase::Connected {
                    self.status_line =
                        "WhatsApp is not linked yet — use Link Device first".to_string();
                    return;
                }
                self.modal.set_whatsapp_link_starting();
                self.send_daemon_command(DaemonCommand::WhatsAppLinkSubscribe);
                self.send_daemon_command(DaemonCommand::WhatsAppLinkStatus);
                self.send_daemon_command(DaemonCommand::WhatsAppLinkReset);
                self.send_daemon_command(DaemonCommand::WhatsAppLinkStart);
                self.modal
                    .reduce(modal::ModalAction::Push(modal::ModalKind::WhatsAppLink));
                self.status_line = "Restarting WhatsApp link workflow".to_string();
            }
            "search_provider" => {
                let next = match self.config.search_provider.as_str() {
                    "none" | "" => "firecrawl",
                    "firecrawl" => "exa",
                    "exa" => "tavily",
                    _ => "none",
                };
                self.config.search_provider = next.to_string();
                self.sync_config_to_daemon();
            }
            "firecrawl_api_key" => self
                .settings
                .start_editing("firecrawl_api_key", &self.config.firecrawl_api_key.clone()),
            "exa_api_key" => self
                .settings
                .start_editing("exa_api_key", &self.config.exa_api_key.clone()),
            "tavily_api_key" => self
                .settings
                .start_editing("tavily_api_key", &self.config.tavily_api_key.clone()),
            "search_max_results" => self.settings.start_editing(
                "search_max_results",
                &self.config.search_max_results.to_string(),
            ),
            "search_timeout" => self.settings.start_editing(
                "search_timeout",
                &self.config.search_timeout_secs.to_string(),
            ),
            "browse_provider" => {
                let next = match self.config.browse_provider.as_str() {
                    "auto" | "" => "lightpanda",
                    "lightpanda" => "chrome",
                    "chrome" => "none",
                    _ => "auto",
                };
                self.config.browse_provider = next.to_string();
                self.sync_config_to_daemon();
            }
            "enable_honcho_memory" => {
                self.open_honcho_editor();
            }
            "honcho_api_key" => self
                .settings
                .start_editing("honcho_api_key", &self.config.honcho_api_key.clone()),
            "honcho_base_url" => self
                .settings
                .start_editing("honcho_base_url", &self.config.honcho_base_url.clone()),
            "honcho_workspace_id" => self.settings.start_editing(
                "honcho_workspace_id",
                &self.config.honcho_workspace_id.clone(),
            ),
            "operator_model_inspect" => {
                self.send_daemon_command(DaemonCommand::GetOperatorModel);
                self.status_line = "Loading operator model snapshot".to_string();
            }
            "operator_model_reset" => {
                self.send_daemon_command(DaemonCommand::ResetOperatorModel);
                self.status_line = "Resetting operator model".to_string();
            }
            "collaboration_sessions_inspect" => {
                self.main_pane_view = MainPaneView::Collaboration;
                self.focus = FocusArea::Chat;
                self.send_daemon_command(DaemonCommand::GetCollaborationSessions);
                self.status_line = "Loading collaboration sessions".to_string();
            }
            "generated_tools_inspect" => {
                self.send_daemon_command(DaemonCommand::GetGeneratedTools);
                self.status_line = "Loading generated tools".to_string();
            }
            "compliance_mode" => {
                let next = match self.config.compliance_mode.as_str() {
                    "standard" => "soc2",
                    "soc2" => "hipaa",
                    "hipaa" => "fedramp",
                    _ => "standard",
                };
                self.config.compliance_mode = next.to_string();
                self.sync_config_to_daemon();
            }
            "compliance_retention_days" => self.settings.start_editing(
                "compliance_retention_days",
                &self.config.compliance_retention_days.to_string(),
            ),
            "tool_synthesis_max_generated_tools" => self.settings.start_editing(
                "tool_synthesis_max_generated_tools",
                &self.config.tool_synthesis_max_generated_tools.to_string(),
            ),
            "context_window_tokens"
                if providers::model_uses_context_window_override(
                    &self.config.provider,
                    &self.config.auth_source,
                    &self.config.model,
                    &self.config.custom_model_name,
                ) =>
            {
                self.settings.start_editing(
                    "context_window_tokens",
                    &self
                        .config
                        .custom_context_window_tokens
                        .unwrap_or(providers::default_custom_model_context_window())
                        .to_string(),
                )
            }
            "max_context_messages" => self.settings.start_editing(
                "max_context_messages",
                &self.config.max_context_messages.to_string(),
            ),
            "tui_chat_history_page_size" => self.settings.start_editing(
                "tui_chat_history_page_size",
                &self.config.tui_chat_history_page_size.to_string(),
            ),
            "participant_observer_restore_window_hours" => self.settings.start_editing(
                "participant_observer_restore_window_hours",
                &self
                    .config
                    .participant_observer_restore_window_hours
                    .to_string(),
            ),
            "max_tool_loops" => self
                .settings
                .start_editing("max_tool_loops", &self.config.max_tool_loops.to_string()),
            "max_retries" => self
                .settings
                .start_editing("max_retries", &self.config.max_retries.to_string()),
            "auto_refresh_interval_secs" => self.settings.start_editing(
                "auto_refresh_interval_secs",
                &self.config.auto_refresh_interval_secs.to_string(),
            ),
            "retry_delay_ms" => self
                .settings
                .start_editing("retry_delay_ms", &self.config.retry_delay_ms.to_string()),
            "message_loop_delay_ms" => self.settings.start_editing(
                "message_loop_delay_ms",
                &self.config.message_loop_delay_ms.to_string(),
            ),
            "tool_call_delay_ms" => self.settings.start_editing(
                "tool_call_delay_ms",
                &self.config.tool_call_delay_ms.to_string(),
            ),
            "llm_stream_chunk_timeout_secs" => self.settings.start_editing(
                "llm_stream_chunk_timeout_secs",
                &self.config.llm_stream_chunk_timeout_secs.to_string(),
            ),
            "auto_retry" => {
                self.config.auto_retry = !self.config.auto_retry;
                self.sync_config_to_daemon();
            }
            "compact_threshold_pct" => self.settings.start_editing(
                "compact_threshold_pct",
                &self.config.compact_threshold_pct.to_string(),
            ),
            "keep_recent_on_compact" => self.settings.start_editing(
                "keep_recent_on_compact",
                &self.config.keep_recent_on_compact.to_string(),
            ),
            "bash_timeout_secs" => self.settings.start_editing(
                "bash_timeout_secs",
                &self.config.bash_timeout_secs.to_string(),
            ),
            "weles_max_concurrent_reviews" => self.settings.start_editing(
                "weles_max_concurrent_reviews",
                &self.config.weles_max_concurrent_reviews.to_string(),
            ),
            "compaction_strategy" => self.cycle_compaction_strategy(),
            "compaction_weles_provider" => {
                self.open_provider_picker(SettingsPickerTarget::CompactionWelesProvider);
            }
            "compaction_weles_model" => self.open_compaction_weles_model_picker(),
            "compaction_weles_reasoning_effort" => {
                self.settings_picker_target =
                    Some(SettingsPickerTarget::CompactionWelesReasoningEffort);
                self.execute_command("effort");
            }
            "compaction_custom_provider" => {
                self.open_provider_picker(SettingsPickerTarget::CompactionCustomProvider);
            }
            "compaction_custom_base_url" => self.settings.start_editing(
                "compaction_custom_base_url",
                &self.config.compaction_custom_base_url.clone(),
            ),
            "compaction_custom_auth_source" => {
                let supported =
                    providers::supported_auth_sources_for(&self.config.compaction_custom_provider);
                let current_idx = supported
                    .iter()
                    .position(|source| *source == self.config.compaction_custom_auth_source)
                    .unwrap_or(0);
                let next_idx = (current_idx + 1) % supported.len().max(1);
                self.config.compaction_custom_auth_source = supported
                    .get(next_idx)
                    .copied()
                    .unwrap_or("api_key")
                    .to_string();
                self.normalize_compaction_custom_transport();
                self.sync_config_to_daemon();
            }
            "compaction_custom_model" => self.open_compaction_custom_model_picker(),
            "compaction_custom_api_transport" => {
                let supported =
                    providers::supported_transports_for(&self.config.compaction_custom_provider);
                let current_idx = supported
                    .iter()
                    .position(|transport| *transport == self.config.compaction_custom_api_transport)
                    .unwrap_or(0);
                let next_idx = (current_idx + 1) % supported.len().max(1);
                self.config.compaction_custom_api_transport = supported
                    .get(next_idx)
                    .copied()
                    .unwrap_or("chat_completions")
                    .to_string();
                self.normalize_compaction_custom_transport();
                self.sync_config_to_daemon();
            }
            "compaction_custom_api_key" => self.settings.start_editing(
                "compaction_custom_api_key",
                &self.config.compaction_custom_api_key.clone(),
            ),
            "compaction_custom_assistant_id" => self.settings.start_editing(
                "compaction_custom_assistant_id",
                &self.config.compaction_custom_assistant_id.clone(),
            ),
            "compaction_custom_reasoning_effort" => {
                self.settings_picker_target =
                    Some(SettingsPickerTarget::CompactionCustomReasoningEffort);
                self.execute_command("effort");
            }
            "compaction_custom_context_window_tokens" => self.settings.start_editing(
                "compaction_custom_context_window_tokens",
                &self
                    .config
                    .compaction_custom_context_window_tokens
                    .to_string(),
            ),
            "snapshot_max_count" => self.settings.start_editing(
                "snapshot_max_count",
                &self.config.snapshot_max_count.to_string(),
            ),
            "snapshot_max_size_mb" => self.settings.start_editing(
                "snapshot_max_size_mb",
                &self.config.snapshot_max_size_mb.to_string(),
            ),
            "snapshot_stats" => {
                // Read-only field, no-op on Enter
            }
            "agent_name" => {
                let current = self
                    .config
                    .agent_config_raw
                    .as_ref()
                    .and_then(|raw| raw.get("agent_name"))
                    .and_then(|value| value.as_str())
                    .unwrap_or("Zorai")
                    .to_string();
                self.settings.start_editing("agent_name", &current);
            }
            "system_prompt" => {
                let current = self
                    .config
                    .agent_config_raw
                    .as_ref()
                    .and_then(|raw| raw.get("system_prompt"))
                    .and_then(|value| value.as_str())
                    .unwrap_or("")
                    .to_string();
                self.settings.start_editing("system_prompt", &current);
            }
            // ── Sub-Agents tab ──
            "subagent_list" => {
                self.subagents.actions_focused = false;
            }
            "concierge_enabled" => {
                self.concierge.enabled = !self.concierge.enabled;
                self.send_concierge_config();
            }
            "concierge_detail_level" => {
                let levels = [
                    "minimal",
                    "context_summary",
                    "proactive_triage",
                    "daily_briefing",
                ];
                let current_idx = levels
                    .iter()
                    .position(|level| *level == self.concierge.detail_level)
                    .unwrap_or(0);
                self.concierge.detail_level = levels[(current_idx + 1) % levels.len()].to_string();
                self.send_concierge_config();
            }
            "concierge_provider" => {
                self.settings_picker_target = Some(SettingsPickerTarget::ConciergeProvider);
                self.execute_command("provider");
            }
            "concierge_model" => {
                let provider_id = self
                    .concierge
                    .provider
                    .clone()
                    .unwrap_or_else(|| self.config.provider.clone());
                let (base_url, api_key, auth_source) = self.provider_auth_snapshot(&provider_id);
                self.open_provider_backed_model_picker(
                    SettingsPickerTarget::ConciergeModel,
                    provider_id,
                    base_url,
                    api_key,
                    auth_source,
                );
            }
            "concierge_reasoning_effort" => {
                self.settings_picker_target = Some(SettingsPickerTarget::ConciergeReasoningEffort);
                self.execute_command("effort");
            }
            "concierge_openrouter_provider_order" => self.open_concierge_openrouter_provider_picker(
                SettingsPickerTarget::ConciergeOpenRouterPreferredProviders,
            ),
            "concierge_openrouter_provider_ignore" => self.open_concierge_openrouter_provider_picker(
                SettingsPickerTarget::ConciergeOpenRouterExcludedProviders,
            ),
            "concierge_openrouter_allow_fallbacks" => {
                if self.concierge.provider.as_deref() == Some(PROVIDER_ID_OPENROUTER) {
                    self.concierge.openrouter_allow_fallbacks =
                        !self.concierge.openrouter_allow_fallbacks;
                    self.send_concierge_config();
                } else {
                    self.status_line =
                        "OpenRouter provider routing only applies to OpenRouter agents"
                            .to_string();
                }
            }
            "managed_security_level" => {
                let levels = ["highest", "moderate", "lowest", "yolo"];
                let current_idx = levels
                    .iter()
                    .position(|level| *level == self.config.managed_security_level)
                    .unwrap_or(2);
                self.config.managed_security_level =
                    levels[(current_idx + 1) % levels.len()].to_string();
                self.sync_config_to_daemon();
            }
            _ if self.activate_feature_settings_field(field.as_str()) => {}
            _ => {}
        }
    }
}
