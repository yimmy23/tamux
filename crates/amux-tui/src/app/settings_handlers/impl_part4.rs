impl TuiModel {
    pub(super) fn activate_settings_field(&mut self) {
        let field = self.current_settings_field_name().to_string();
        match field.as_str() {
            "provider" => {
                self.settings_picker_target = Some(SettingsPickerTarget::Provider);
                self.execute_command("provider");
            }
            "model" => {
                if self.config.provider == PROVIDER_ID_CUSTOM {
                    self.begin_custom_model_edit();
                } else {
                    self.settings_picker_target = Some(SettingsPickerTarget::Model);
                    self.execute_command("model");
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
            "context_window_tokens" if self.config.provider == PROVIDER_ID_CUSTOM => {
                self.settings.start_editing(
                    "context_window_tokens",
                    &self
                        .config
                        .custom_context_window_tokens
                        .unwrap_or(128_000)
                        .to_string(),
                )
            }
            "max_context_messages" => self.settings.start_editing(
                "max_context_messages",
                &self.config.max_context_messages.to_string(),
            ),
            "max_tool_loops" => self
                .settings
                .start_editing("max_tool_loops", &self.config.max_tool_loops.to_string()),
            "max_retries" => self
                .settings
                .start_editing("max_retries", &self.config.max_retries.to_string()),
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
            "context_budget_tokens" => self.settings.start_editing(
                "context_budget_tokens",
                &self.config.context_budget_tokens.to_string(),
            ),
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
                self.config.compaction_weles_provider =
                    Self::cycle_provider_id(&self.config.compaction_weles_provider);
                if self.config.compaction_weles_model.trim().is_empty() {
                    self.config.compaction_weles_model =
                        providers::default_model_for_provider_auth(
                            &self.config.compaction_weles_provider,
                            "api_key",
                        );
                }
                self.sync_config_to_daemon();
            }
            "compaction_weles_model" => self.settings.start_editing(
                "compaction_weles_model",
                &self.config.compaction_weles_model.clone(),
            ),
            "compaction_weles_reasoning_effort" => {
                self.settings_picker_target = Some(SettingsPickerTarget::CompactionWelesReasoningEffort);
                self.execute_command("effort");
            }
            "compaction_custom_provider" => {
                let next = Self::cycle_provider_id(&self.config.compaction_custom_provider);
                self.apply_compaction_custom_provider(&next);
                self.sync_config_to_daemon();
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
                self.sync_config_to_daemon();
            }
            "compaction_custom_model" => self.settings.start_editing(
                "compaction_custom_model",
                &self.config.compaction_custom_model.clone(),
            ),
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
                    .unwrap_or("Tamux")
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
                self.settings_picker_target = Some(SettingsPickerTarget::ConciergeModel);
                let provider_id = self
                    .concierge
                    .provider
                    .clone()
                    .unwrap_or_else(|| self.config.provider.clone());
                let models = providers::known_models_for_provider_auth(&provider_id, "api_key");
                if !models.is_empty() {
                    self.config
                        .reduce(config::ConfigAction::ModelsFetched(models));
                }
                let count = widgets::model_picker::available_models(&self.config).len() + 1;
                self.modal
                    .reduce(modal::ModalAction::Push(modal::ModalKind::ModelPicker));
                self.modal.set_picker_item_count(count);
            }
            "concierge_reasoning_effort" => {
                self.settings_picker_target = Some(SettingsPickerTarget::ConciergeReasoningEffort);
                self.execute_command("effort");
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
