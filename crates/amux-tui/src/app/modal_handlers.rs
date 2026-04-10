use super::*;

#[path = "modal_handlers_enter.rs"]
mod enter;

impl TuiModel {
    pub(crate) fn begin_custom_model_edit(&mut self) {
        enter::begin_custom_model_edit(self);
    }
    pub(super) fn handle_key_modal(
        &mut self,
        code: KeyCode,
        modifiers: KeyModifiers,
        kind: modal::ModalKind,
    ) -> bool {
        if kind == modal::ModalKind::Notifications {
            match code {
                KeyCode::Esc => {
                    self.close_top_modal();
                }
                KeyCode::Tab | KeyCode::BackTab => {
                    if self.notifications.selected_row_action_index().is_some() {
                        let header_action = self.notifications.first_enabled_header_action();
                        self.notifications
                            .reduce(crate::state::NotificationsAction::FocusHeader(
                                header_action,
                            ));
                    } else {
                        let row_action = self.notifications.first_enabled_row_action_index();
                        self.notifications.reduce(
                            crate::state::NotificationsAction::FocusRowAction(row_action),
                        );
                    }
                }
                KeyCode::Left | KeyCode::Char('h') => {
                    if self.notifications.selected_row_action_index().is_some() {
                        self.notifications
                            .reduce(crate::state::NotificationsAction::StepRowAction(-1));
                    } else {
                        self.notifications
                            .reduce(crate::state::NotificationsAction::StepHeader(-1));
                    }
                }
                KeyCode::Right | KeyCode::Char('l') => {
                    if self.notifications.selected_row_action_index().is_some() {
                        self.notifications
                            .reduce(crate::state::NotificationsAction::StepRowAction(1));
                    } else {
                        self.notifications
                            .reduce(crate::state::NotificationsAction::StepHeader(1));
                    }
                }
                KeyCode::Up | KeyCode::Char('k') => {
                    self.notifications
                        .reduce(crate::state::NotificationsAction::FocusHeader(None));
                    self.notifications
                        .reduce(crate::state::NotificationsAction::FocusRowAction(None));
                    self.notifications
                        .reduce(crate::state::NotificationsAction::Navigate(-1));
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    self.notifications
                        .reduce(crate::state::NotificationsAction::FocusHeader(None));
                    self.notifications
                        .reduce(crate::state::NotificationsAction::FocusRowAction(None));
                    self.notifications
                        .reduce(crate::state::NotificationsAction::Navigate(1));
                }
                KeyCode::Enter | KeyCode::Char(' ') => {
                    if let Some(action_index) = self.notifications.selected_row_action_index() {
                        if let Some(id) = self
                            .notifications
                            .selected_item()
                            .map(|notification| notification.id.clone())
                        {
                            self.execute_notification_row_action(&id, action_index);
                        }
                    } else {
                        match self.notifications.selected_header_action() {
                            Some(crate::state::NotificationsHeaderAction::MarkAllRead) => {
                                self.mark_all_notifications_read();
                            }
                            Some(crate::state::NotificationsHeaderAction::ArchiveRead) => {
                                self.archive_read_notifications();
                            }
                            Some(crate::state::NotificationsHeaderAction::Close) => {
                                self.close_top_modal();
                            }
                            None => {
                                if let Some(id) = self
                                    .notifications
                                    .selected_item()
                                    .map(|notification| notification.id.clone())
                                {
                                    self.toggle_notification_expand(id);
                                }
                            }
                        }
                    }
                }
                KeyCode::Char('e') => {
                    if let Some(id) = self
                        .notifications
                        .selected_item()
                        .map(|notification| notification.id.clone())
                    {
                        self.toggle_notification_expand(id);
                    }
                }
                KeyCode::Char('r') => {
                    if let Some(id) = self
                        .notifications
                        .selected_item()
                        .map(|notification| notification.id.clone())
                    {
                        self.mark_notification_read(&id);
                    }
                }
                KeyCode::Char('a') => {
                    if let Some(id) = self
                        .notifications
                        .selected_item()
                        .map(|notification| notification.id.clone())
                    {
                        self.archive_notification(&id);
                    }
                }
                KeyCode::Char('x') => {
                    if let Some(id) = self
                        .notifications
                        .selected_item()
                        .map(|notification| notification.id.clone())
                    {
                        self.delete_notification(&id);
                    }
                }
                KeyCode::Char('o') => {
                    if let Some(notification_id) = self
                        .notifications
                        .selected_item()
                        .filter(|notification| !notification.actions.is_empty())
                        .map(|notification| notification.id.clone())
                    {
                        self.execute_notification_action(&notification_id, "", Some(0));
                    }
                }
                KeyCode::Char('A') if modifiers.contains(KeyModifiers::SHIFT) => {
                    self.archive_read_notifications();
                }
                KeyCode::Char('R') if modifiers.contains(KeyModifiers::SHIFT) => {
                    self.mark_all_notifications_read();
                }
                _ => {}
            }
            return false;
        }

        if kind == modal::ModalKind::Settings {
            if matches!(
                code,
                KeyCode::Char('v' | 'V') if modifiers.contains(KeyModifiers::CONTROL)
            ) || code == KeyCode::Char('\u{16}')
                || (code == KeyCode::Insert && modifiers.contains(KeyModifiers::SHIFT))
            {
                self.paste_from_clipboard();
                return false;
            }

            // Plugin settings fields use their own save path — bypass the
            // base config handler so Enter reaches handle_plugins_settings_key.
            if self.settings.is_editing() && self.settings.active_tab() == SettingsTab::Plugins {
                match code {
                    KeyCode::Enter => {
                        // Delegate to plugin handler which sends PluginUpdateSetting
                        if self.handle_plugins_settings_key(code) {
                            return false;
                        }
                    }
                    KeyCode::Esc => {
                        self.settings.reduce(SettingsAction::CancelEdit);
                        return false;
                    }
                    KeyCode::Backspace => {
                        self.settings.reduce(SettingsAction::Backspace);
                        return false;
                    }
                    KeyCode::Char(c) => {
                        self.settings.reduce(SettingsAction::InsertChar(c));
                        return false;
                    }
                    _ => return false,
                }
            }

            if self.settings.is_editing() {
                if self.settings.is_textarea() {
                    match code {
                        KeyCode::Enter if modifiers.contains(KeyModifiers::CONTROL) => {}
                        KeyCode::Enter => {
                            self.settings.reduce(SettingsAction::InsertChar('\n'));
                            return false;
                        }
                        KeyCode::Esc => {
                            self.settings.reduce(SettingsAction::CancelEdit);
                            return false;
                        }
                        KeyCode::Backspace => {
                            self.settings.reduce(SettingsAction::Backspace);
                            return false;
                        }
                        KeyCode::Char(c) => {
                            self.settings.reduce(SettingsAction::InsertChar(c));
                            return false;
                        }
                        _ => return false,
                    }
                }

                match code {
                    KeyCode::Enter => {
                        let field = self.settings.editing_field().unwrap_or("").to_string();
                        let value = self.settings.edit_buffer().to_string();
                        match field.as_str() {
                            "base_url" => self.config.base_url = value,
                            "api_key" => self.config.api_key = value,
                            "assistant_id" => self.config.assistant_id = value,
                            "compaction_weles_model" => {
                                self.config.compaction_weles_model = value.trim().to_string()
                            }
                            "compaction_custom_base_url" => {
                                self.config.compaction_custom_base_url = value
                            }
                            "compaction_custom_model" => {
                                self.config.compaction_custom_model = value.trim().to_string()
                            }
                            "compaction_custom_api_key" => {
                                self.config.compaction_custom_api_key = value
                            }
                            "compaction_custom_assistant_id" => {
                                self.config.compaction_custom_assistant_id = value
                            }
                            "custom_model_entry" => {
                                let trimmed = value.trim();
                                let (name, model_id) =
                                    if let Some((lhs, rhs)) = trimmed.split_once('|') {
                                        (lhs.trim(), rhs.trim())
                                    } else {
                                        ("", trimmed)
                                    };
                                if !model_id.is_empty() {
                                    self.config.model = model_id.to_string();
                                    let resolved_context_window =
                                        providers::resolve_context_window_for_provider_auth(
                                            &self.config.provider,
                                            &self.config.auth_source,
                                            &self.config.model,
                                            name,
                                        );
                                    if providers::model_uses_context_window_override(
                                        &self.config.provider,
                                        &self.config.auth_source,
                                        &self.config.model,
                                        name,
                                    ) {
                                        self.config.custom_model_name = if name.is_empty() {
                                            model_id.to_string()
                                        } else {
                                            name.to_string()
                                        };
                                        let next_context = resolved_context_window.unwrap_or(
                                            providers::default_custom_model_context_window(),
                                        );
                                        self.config.custom_context_window_tokens =
                                            Some(next_context);
                                        self.config.context_window_tokens = next_context;
                                    } else {
                                        self.config.custom_model_name = if name.is_empty() {
                                            String::new()
                                        } else {
                                            name.to_string()
                                        };
                                        self.config.custom_context_window_tokens = None;
                                        self.config.context_window_tokens =
                                            resolved_context_window.unwrap_or(128_000);
                                    }
                                }
                            }
                            "gateway_prefix" => self.config.gateway_prefix = value,
                            "slack_token" => self.config.slack_token = value,
                            "slack_channel_filter" => self.config.slack_channel_filter = value,
                            "telegram_token" => self.config.telegram_token = value,
                            "telegram_allowed_chats" => self.config.telegram_allowed_chats = value,
                            "discord_token" => self.config.discord_token = value,
                            "discord_channel_filter" => self.config.discord_channel_filter = value,
                            "discord_allowed_users" => self.config.discord_allowed_users = value,
                            "whatsapp_allowed_contacts" => {
                                self.config.whatsapp_allowed_contacts = value
                            }
                            "whatsapp_token" => self.config.whatsapp_token = value,
                            "whatsapp_phone_id" => self.config.whatsapp_phone_id = value,
                            "firecrawl_api_key" => self.config.firecrawl_api_key = value,
                            "exa_api_key" => self.config.exa_api_key = value,
                            "tavily_api_key" => self.config.tavily_api_key = value,
                            "search_max_results" => {
                                if let Ok(n) = value.parse::<u32>() {
                                    self.config.search_max_results = n.clamp(1, 20);
                                }
                            }
                            "search_timeout" => {
                                if let Ok(n) = value.parse::<u32>() {
                                    self.config.search_timeout_secs = n.clamp(3, 120);
                                }
                            }
                            "honcho_api_key" => self.config.honcho_api_key = value,
                            "honcho_base_url" => self.config.honcho_base_url = value,
                            "honcho_workspace_id" => self.config.honcho_workspace_id = value,
                            "compliance_retention_days" => {
                                if let Ok(n) = value.parse::<u32>() {
                                    self.config.compliance_retention_days = n.clamp(1, 3650);
                                }
                            }
                            "tool_synthesis_max_generated_tools" => {
                                if let Ok(n) = value.parse::<u32>() {
                                    self.config.tool_synthesis_max_generated_tools =
                                        n.clamp(1, 200);
                                }
                            }
                            "max_context_messages" => {
                                if let Ok(n) = value.parse::<u32>() {
                                    self.config.max_context_messages = n.clamp(10, 500);
                                }
                            }
                            "max_tool_loops" => {
                                if let Ok(n) = value.parse::<u32>() {
                                    self.config.max_tool_loops = n.clamp(0, 1000);
                                }
                            }
                            "max_retries" => {
                                if let Ok(n) = value.parse::<u32>() {
                                    self.config.max_retries = n.clamp(0, 10);
                                }
                            }
                            "retry_delay_ms" => {
                                if let Ok(n) = value.parse::<u32>() {
                                    self.config.retry_delay_ms = n.clamp(100, 60000);
                                }
                            }
                            "message_loop_delay_ms" => {
                                if let Ok(n) = value.parse::<u32>() {
                                    self.config.message_loop_delay_ms = n.clamp(0, 60000);
                                }
                            }
                            "tool_call_delay_ms" => {
                                if let Ok(n) = value.parse::<u32>() {
                                    self.config.tool_call_delay_ms = n.clamp(0, 60000);
                                }
                            }
                            "llm_stream_chunk_timeout_secs" => {
                                if let Ok(n) = value.parse::<u32>() {
                                    self.config.llm_stream_chunk_timeout_secs = n.clamp(30, 1800);
                                }
                            }
                            "context_window_tokens" => {
                                if providers::model_uses_context_window_override(
                                    &self.config.provider,
                                    &self.config.auth_source,
                                    &self.config.model,
                                    &self.config.custom_model_name,
                                ) {
                                    if let Ok(n) = value.parse::<u32>() {
                                        let next = n.clamp(1000, 2_000_000);
                                        self.config.custom_context_window_tokens = Some(next);
                                        self.config.context_window_tokens = next;
                                    }
                                }
                            }
                            "context_budget_tokens" => {
                                if let Ok(n) = value.parse::<u32>() {
                                    self.config.context_budget_tokens = n.clamp(10000, 500000);
                                }
                            }
                            "compact_threshold_pct" => {
                                if let Ok(n) = value.parse::<u32>() {
                                    self.config.compact_threshold_pct = n.clamp(50, 95);
                                }
                            }
                            "keep_recent_on_compact" => {
                                if let Ok(n) = value.parse::<u32>() {
                                    self.config.keep_recent_on_compact = n.clamp(1, 50);
                                }
                            }
                            "bash_timeout_secs" => {
                                if let Ok(n) = value.parse::<u32>() {
                                    self.config.bash_timeout_secs = n.clamp(5, 300);
                                }
                            }
                            "weles_max_concurrent_reviews" => {
                                if let Ok(n) = value.parse::<u32>() {
                                    self.config.weles_max_concurrent_reviews = n.clamp(1, 16);
                                }
                            }
                            "compaction_custom_context_window_tokens" => {
                                if let Ok(n) = value.parse::<u32>() {
                                    self.config.compaction_custom_context_window_tokens =
                                        n.clamp(1000, 2_000_000);
                                }
                            }
                            "snapshot_max_count" => {
                                if let Ok(n) = value.parse::<u32>() {
                                    self.config.snapshot_max_count = n.clamp(0, 100);
                                }
                            }
                            "snapshot_max_size_mb" => {
                                if let Ok(n) = value.parse::<u32>() {
                                    self.config.snapshot_max_size_mb = n.clamp(1_024, 500_000);
                                }
                            }
                            "agent_name" => {
                                if let Some(ref mut raw) = self.config.agent_config_raw {
                                    raw["agent_name"] = serde_json::Value::String(value);
                                }
                            }
                            "system_prompt" => {
                                if let Some(ref mut raw) = self.config.agent_config_raw {
                                    raw["system_prompt"] = serde_json::Value::String(value);
                                } else {
                                    let mut raw = serde_json::json!({});
                                    raw["system_prompt"] = serde_json::Value::String(value);
                                    self.config.agent_config_raw = Some(raw);
                                }
                            }
                            "subagent_name" => {
                                if let Some(editor) = self.subagents.editor.as_mut() {
                                    editor.name = value;
                                }
                            }
                            "subagent_role" => {
                                if let Some(editor) = self.subagents.editor.as_mut() {
                                    editor.role = value;
                                    editor.previous_role_preset = None;
                                }
                            }
                            "subagent_system_prompt" => {
                                if let Some(editor) = self.subagents.editor.as_mut() {
                                    editor.system_prompt = value;
                                }
                            }
                            "concierge_provider" => {
                                self.concierge.provider = if value.trim().is_empty() {
                                    None
                                } else {
                                    Some(value)
                                };
                                self.send_concierge_config();
                            }
                            "concierge_model" => {
                                self.concierge.model = if value.trim().is_empty() {
                                    None
                                } else {
                                    Some(value)
                                };
                                self.send_concierge_config();
                            }
                            // ── Features tab editable fields ──
                            "feat_heartbeat_cron" => {
                                self.send_daemon_command(DaemonCommand::SetConfigItem {
                                    key_path: "/heartbeat/cron".to_string(),
                                    value_json: format!("\"{}\"", value),
                                });
                                if let Some(ref mut raw) = self.config.agent_config_raw {
                                    if raw.get("heartbeat").is_none() {
                                        raw["heartbeat"] = serde_json::json!({});
                                    }
                                    raw["heartbeat"]["cron"] = serde_json::Value::String(value);
                                }
                            }
                            "feat_heartbeat_quiet_start" => {
                                self.send_daemon_command(DaemonCommand::SetConfigItem {
                                    key_path: "/heartbeat/quiet_start".to_string(),
                                    value_json: format!("\"{}\"", value),
                                });
                                if let Some(ref mut raw) = self.config.agent_config_raw {
                                    if raw.get("heartbeat").is_none() {
                                        raw["heartbeat"] = serde_json::json!({});
                                    }
                                    raw["heartbeat"]["quiet_start"] =
                                        serde_json::Value::String(value);
                                }
                            }
                            "feat_heartbeat_quiet_end" => {
                                self.send_daemon_command(DaemonCommand::SetConfigItem {
                                    key_path: "/heartbeat/quiet_end".to_string(),
                                    value_json: format!("\"{}\"", value),
                                });
                                if let Some(ref mut raw) = self.config.agent_config_raw {
                                    if raw.get("heartbeat").is_none() {
                                        raw["heartbeat"] = serde_json::json!({});
                                    }
                                    raw["heartbeat"]["quiet_end"] =
                                        serde_json::Value::String(value);
                                }
                            }
                            "feat_decay_half_life_hours" => {
                                if let Ok(n) = value.parse::<f64>() {
                                    let clamped = n.clamp(1.0, 10000.0);
                                    self.send_daemon_command(DaemonCommand::SetConfigItem {
                                        key_path: "/consolidation/decay_half_life_hours"
                                            .to_string(),
                                        value_json: format!("{}", clamped),
                                    });
                                    if let Some(ref mut raw) = self.config.agent_config_raw {
                                        if raw.get("consolidation").is_none() {
                                            raw["consolidation"] = serde_json::json!({});
                                        }
                                        raw["consolidation"]["decay_half_life_hours"] =
                                            serde_json::json!(clamped);
                                    }
                                }
                            }
                            "feat_heuristic_promotion_threshold" => {
                                if let Ok(n) = value.parse::<u64>() {
                                    let clamped = n.clamp(1, 100);
                                    self.send_daemon_command(DaemonCommand::SetConfigItem {
                                        key_path: "/consolidation/heuristic_promotion_threshold"
                                            .to_string(),
                                        value_json: format!("{}", clamped),
                                    });
                                    if let Some(ref mut raw) = self.config.agent_config_raw {
                                        if raw.get("consolidation").is_none() {
                                            raw["consolidation"] = serde_json::json!({});
                                        }
                                        raw["consolidation"]["heuristic_promotion_threshold"] =
                                            serde_json::json!(clamped);
                                    }
                                }
                            }
                            "feat_skill_community_preapprove_timeout_secs" => {
                                if let Ok(n) = value.parse::<u64>() {
                                    let clamped = n.clamp(5, 300);
                                    self.send_daemon_command(DaemonCommand::SetConfigItem {
                                        key_path:
                                            "/skill_recommendation/community_preapprove_timeout_secs"
                                                .to_string(),
                                        value_json: format!("{}", clamped),
                                    });
                                    if let Some(ref mut raw) = self.config.agent_config_raw {
                                        if raw.get("skill_recommendation").is_none() {
                                            raw["skill_recommendation"] = serde_json::json!({});
                                        }
                                        raw["skill_recommendation"]
                                            ["community_preapprove_timeout_secs"] =
                                            serde_json::json!(clamped);
                                    }
                                }
                            }
                            "feat_skill_suggest_global_enable_after_approvals" => {
                                if let Ok(n) = value.parse::<u64>() {
                                    let clamped = n.clamp(1, 12);
                                    self.send_daemon_command(DaemonCommand::SetConfigItem {
                                        key_path:
                                            "/skill_recommendation/suggest_global_enable_after_approvals"
                                                .to_string(),
                                        value_json: format!("{}", clamped),
                                    });
                                    if let Some(ref mut raw) = self.config.agent_config_raw {
                                        if raw.get("skill_recommendation").is_none() {
                                            raw["skill_recommendation"] = serde_json::json!({});
                                        }
                                        raw["skill_recommendation"]
                                            ["suggest_global_enable_after_approvals"] =
                                            serde_json::json!(clamped);
                                    }
                                }
                            }
                            _ => {}
                        }
                        self.settings.reduce(SettingsAction::ConfirmEdit);
                        if !matches!(
                            field.as_str(),
                            "subagent_name" | "subagent_role" | "subagent_system_prompt"
                        ) {
                            self.sync_config_to_daemon();
                        }
                    }
                    KeyCode::Esc => self.settings.reduce(SettingsAction::CancelEdit),
                    KeyCode::Backspace => self.settings.reduce(SettingsAction::Backspace),
                    KeyCode::Char(c) => self.settings.reduce(SettingsAction::InsertChar(c)),
                    _ => {}
                }
                return false;
            }

            match self.settings.active_tab() {
                SettingsTab::Auth => {
                    if self.handle_auth_settings_key(code) {
                        return false;
                    }
                }
                SettingsTab::SubAgents => {
                    if self.handle_subagent_settings_key(code) {
                        return false;
                    }
                }
                SettingsTab::Plugins => {
                    if self.handle_plugins_settings_key(code) {
                        return false;
                    }
                }
                _ => {}
            }

            match code {
                KeyCode::Tab => {
                    let all = SettingsTab::all();
                    let current = self.settings.active_tab();
                    let next_idx = all
                        .iter()
                        .position(|&tab| tab == current)
                        .map(|i| (i + 1) % all.len())
                        .unwrap_or(0);
                    let next_tab = all[next_idx];
                    self.settings.reduce(SettingsAction::SwitchTab(next_tab));
                    if matches!(next_tab, SettingsTab::SubAgents) {
                        self.send_daemon_command(DaemonCommand::ListSubAgents);
                    } else if matches!(next_tab, SettingsTab::Concierge) {
                        self.send_daemon_command(DaemonCommand::GetConciergeConfig);
                    } else if matches!(next_tab, SettingsTab::Gateway) {
                        self.send_daemon_command(DaemonCommand::WhatsAppLinkStatus);
                    } else if matches!(next_tab, SettingsTab::Plugins) {
                        self.plugin_settings.list_mode = true;
                        self.send_daemon_command(DaemonCommand::PluginList);
                    }
                    return false;
                }
                KeyCode::BackTab => {
                    let all = SettingsTab::all();
                    let current = self.settings.active_tab();
                    let prev_idx = all
                        .iter()
                        .position(|&tab| tab == current)
                        .map(|i| if i == 0 { all.len() - 1 } else { i - 1 })
                        .unwrap_or(0);
                    let prev_tab = all[prev_idx];
                    self.settings.reduce(SettingsAction::SwitchTab(prev_tab));
                    if matches!(prev_tab, SettingsTab::SubAgents) {
                        self.send_daemon_command(DaemonCommand::ListSubAgents);
                    } else if matches!(prev_tab, SettingsTab::Concierge) {
                        self.send_daemon_command(DaemonCommand::GetConciergeConfig);
                    } else if matches!(prev_tab, SettingsTab::Gateway) {
                        self.send_daemon_command(DaemonCommand::WhatsAppLinkStatus);
                    } else if matches!(prev_tab, SettingsTab::Plugins) {
                        self.plugin_settings.list_mode = true;
                        self.send_daemon_command(DaemonCommand::PluginList);
                    }
                    return false;
                }
                KeyCode::Down => {
                    self.settings.navigate_field(1, self.settings_field_count());
                    return false;
                }
                KeyCode::Up => {
                    self.settings
                        .navigate_field(-1, self.settings_field_count());
                    return false;
                }
                KeyCode::Enter => {
                    self.activate_settings_field();
                    return false;
                }
                KeyCode::Char(' ') => {
                    self.toggle_settings_field();
                    return false;
                }
                _ => {}
            }
        }

        if kind == modal::ModalKind::QueuedPrompts {
            match code {
                KeyCode::Esc => self.close_top_modal(),
                KeyCode::Up | KeyCode::Char('k') => {
                    self.modal.reduce(modal::ModalAction::Navigate(-1));
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    self.modal.reduce(modal::ModalAction::Navigate(1));
                }
                KeyCode::Left | KeyCode::Char('h') => {
                    self.queued_prompt_action = self.queued_prompt_action.step(-1);
                }
                KeyCode::Right | KeyCode::Char('l') => {
                    self.queued_prompt_action = self.queued_prompt_action.step(1);
                }
                KeyCode::Enter | KeyCode::Char(' ') => {
                    self.execute_selected_queued_prompt_action();
                }
                _ => {}
            }
            return false;
        }

        if kind == modal::ModalKind::ChatActionConfirm {
            match code {
                KeyCode::Esc | KeyCode::Char('n') | KeyCode::Char('N') => {
                    self.close_chat_action_confirm();
                }
                KeyCode::Left | KeyCode::Char('h') | KeyCode::Tab => {
                    self.chat_action_confirm_accept_selected =
                        !self.chat_action_confirm_accept_selected;
                }
                KeyCode::Right | KeyCode::Char('l') | KeyCode::BackTab => {
                    self.chat_action_confirm_accept_selected =
                        !self.chat_action_confirm_accept_selected;
                }
                KeyCode::Char('y') | KeyCode::Char('Y') => {
                    self.confirm_pending_chat_action();
                }
                KeyCode::Enter | KeyCode::Char(' ') => {
                    if self.chat_action_confirm_accept_selected {
                        self.confirm_pending_chat_action();
                    } else {
                        self.close_chat_action_confirm();
                    }
                }
                _ => {}
            }
            return false;
        }

        if kind == modal::ModalKind::ApprovalOverlay {
            match code {
                KeyCode::Esc => {}
                KeyCode::Char('y') | KeyCode::Char('Y') => {
                    if let Some(ap) = self.approval.selected_approval() {
                        self.resolve_approval(ap.approval_id.clone(), "allow_once");
                    }
                    if let Some(next) = self.next_current_thread_approval_id() {
                        self.approval
                            .reduce(crate::state::ApprovalAction::SelectApproval(next));
                    } else {
                        self.close_top_modal();
                    }
                }
                KeyCode::Char('a') | KeyCode::Char('A') => {
                    if let Some(ap) = self.approval.selected_approval() {
                        self.resolve_approval(ap.approval_id.clone(), "allow_session");
                    }
                    if let Some(next) = self.next_current_thread_approval_id() {
                        self.approval
                            .reduce(crate::state::ApprovalAction::SelectApproval(next));
                    } else {
                        self.close_top_modal();
                    }
                }
                KeyCode::Char('n') | KeyCode::Char('N') => {
                    if let Some(ap) = self.approval.selected_approval() {
                        self.resolve_approval(ap.approval_id.clone(), "reject");
                    }
                    if let Some(next) = self.next_current_thread_approval_id() {
                        self.approval
                            .reduce(crate::state::ApprovalAction::SelectApproval(next));
                    } else {
                        self.close_top_modal();
                    }
                }
                _ => {}
            }
            return false;
        }

        if kind == modal::ModalKind::ApprovalCenter {
            match code {
                KeyCode::Esc => self.close_top_modal(),
                KeyCode::Up | KeyCode::Char('k') => self.step_approval_selection(-1),
                KeyCode::Down | KeyCode::Char('j') => self.step_approval_selection(1),
                KeyCode::Left | KeyCode::Char('h') => {
                    let next_filter = match self.approval.filter() {
                        crate::state::ApprovalFilter::AllPending => {
                            crate::state::ApprovalFilter::CurrentWorkspace
                        }
                        crate::state::ApprovalFilter::CurrentThread => {
                            crate::state::ApprovalFilter::AllPending
                        }
                        crate::state::ApprovalFilter::CurrentWorkspace => {
                            crate::state::ApprovalFilter::CurrentThread
                        }
                    };
                    self.approval
                        .reduce(crate::state::ApprovalAction::SetFilter(next_filter));
                }
                KeyCode::Right | KeyCode::Char('l') => {
                    let next_filter = match self.approval.filter() {
                        crate::state::ApprovalFilter::AllPending => {
                            crate::state::ApprovalFilter::CurrentThread
                        }
                        crate::state::ApprovalFilter::CurrentThread => {
                            crate::state::ApprovalFilter::CurrentWorkspace
                        }
                        crate::state::ApprovalFilter::CurrentWorkspace => {
                            crate::state::ApprovalFilter::AllPending
                        }
                    };
                    self.approval
                        .reduce(crate::state::ApprovalAction::SetFilter(next_filter));
                }
                KeyCode::Char('a') => {
                    if let Some(ap) = self.approval.selected_visible_approval(
                        self.chat.active_thread_id(),
                        self.current_workspace_id(),
                    ) {
                        self.resolve_approval(ap.approval_id.clone(), "allow_once");
                    }
                }
                KeyCode::Char('s') => {
                    if let Some(ap) = self.approval.selected_visible_approval(
                        self.chat.active_thread_id(),
                        self.current_workspace_id(),
                    ) {
                        self.resolve_approval(ap.approval_id.clone(), "allow_session");
                    }
                }
                KeyCode::Char('d') => {
                    if let Some(ap) = self.approval.selected_visible_approval(
                        self.chat.active_thread_id(),
                        self.current_workspace_id(),
                    ) {
                        self.resolve_approval(ap.approval_id.clone(), "reject");
                    }
                }
                KeyCode::Enter => {}
                _ => {}
            }
            return false;
        }

        if kind == modal::ModalKind::OpenAIAuth {
            match code {
                KeyCode::Esc => {
                    self.close_top_modal();
                }
                KeyCode::Char('c') | KeyCode::Char('C') => {
                    if let Some(url) = self.openai_auth_url.as_deref() {
                        conversion::copy_to_clipboard(url);
                        self.status_line = "Copied ChatGPT login URL to clipboard".to_string();
                    }
                }
                KeyCode::Char('o') | KeyCode::Char('O') | KeyCode::Enter => {
                    self.handle_modal_enter(kind);
                }
                _ => {}
            }
            return false;
        }

        if kind == modal::ModalKind::PromptViewer {
            match code {
                KeyCode::Esc => {
                    self.close_top_modal();
                }
                KeyCode::Char('/') => {
                    self.close_top_modal();
                    self.input.reduce(input::InputAction::InsertChar('/'));
                    self.focus = FocusArea::Input;
                }
                KeyCode::Down | KeyCode::Char('j') => self.step_prompt_modal_scroll(1),
                KeyCode::Up | KeyCode::Char('k') => self.step_prompt_modal_scroll(-1),
                KeyCode::PageDown => self.page_prompt_modal_scroll(1),
                KeyCode::PageUp => self.page_prompt_modal_scroll(-1),
                KeyCode::Home => self.set_prompt_modal_scroll(0),
                KeyCode::End => {
                    let max_scroll = self.prompt_modal_max_scroll();
                    self.set_prompt_modal_scroll(max_scroll);
                }
                _ => {}
            }
            return false;
        }

        if kind == modal::ModalKind::Status {
            match code {
                KeyCode::Esc => {
                    self.close_top_modal();
                }
                KeyCode::Char('/') => {
                    self.close_top_modal();
                    self.input.reduce(input::InputAction::InsertChar('/'));
                    self.focus = FocusArea::Input;
                }
                KeyCode::Down | KeyCode::Char('j') => self.step_status_modal_scroll(1),
                KeyCode::Up | KeyCode::Char('k') => self.step_status_modal_scroll(-1),
                KeyCode::PageDown => self.page_status_modal_scroll(1),
                KeyCode::PageUp => self.page_status_modal_scroll(-1),
                KeyCode::Home => self.set_status_modal_scroll(0),
                KeyCode::End => {
                    let max_scroll = self.status_modal_max_scroll();
                    self.set_status_modal_scroll(max_scroll);
                }
                _ => {}
            }
            return false;
        }

        if kind == modal::ModalKind::WhatsAppLink {
            match code {
                KeyCode::Esc | KeyCode::Char('c') | KeyCode::Char('C') => {
                    self.close_top_modal();
                    self.status_line = "WhatsApp linking stopped".to_string();
                    return false;
                }
                _ => return false,
            }
        }

        let is_searchable = matches!(
            kind,
            modal::ModalKind::CommandPalette
                | modal::ModalKind::ThreadPicker
                | modal::ModalKind::GoalPicker
        );

        match code {
            KeyCode::Esc => {
                self.close_top_modal();
                self.input.reduce(input::InputAction::Clear);
            }
            KeyCode::Left if kind == modal::ModalKind::ThreadPicker => {
                let previous = self.modal.thread_picker_tab().prev();
                self.modal.set_thread_picker_tab(previous);
                self.sync_thread_picker_item_count();
            }
            KeyCode::Right if kind == modal::ModalKind::ThreadPicker => {
                let next = self.modal.thread_picker_tab().next();
                self.modal.set_thread_picker_tab(next);
                self.sync_thread_picker_item_count();
            }
            KeyCode::Down => self.modal.reduce(modal::ModalAction::Navigate(1)),
            KeyCode::Up => self.modal.reduce(modal::ModalAction::Navigate(-1)),
            KeyCode::Enter => {
                self.handle_modal_enter(kind);
                if self.pending_quit {
                    self.pending_quit = false;
                    return true;
                }
            }
            KeyCode::Backspace if is_searchable => {
                self.input.reduce(input::InputAction::Backspace);
                self.modal.reduce(modal::ModalAction::SetQuery(
                    self.input.buffer().to_string(),
                ));
                if kind == modal::ModalKind::ThreadPicker {
                    self.sync_thread_picker_item_count();
                } else if kind == modal::ModalKind::GoalPicker {
                    self.sync_goal_picker_item_count();
                }
            }
            KeyCode::Char(c) if is_searchable => {
                self.input.reduce(input::InputAction::InsertChar(c));
                self.modal.reduce(modal::ModalAction::SetQuery(
                    self.input.buffer().to_string(),
                ));
                if kind == modal::ModalKind::ThreadPicker {
                    self.sync_thread_picker_item_count();
                } else if kind == modal::ModalKind::GoalPicker {
                    self.sync_goal_picker_item_count();
                }
            }
            _ => {}
        }

        false
    }

    pub(super) fn handle_modal_enter(&mut self, kind: modal::ModalKind) {
        enter::handle_modal_enter(self, kind);
    }
}

#[cfg(test)]
#[path = "tests/modal_handlers.rs"]
mod tests;
