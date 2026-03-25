use super::*;

impl TuiModel {
    fn begin_custom_model_edit(&mut self) {
        let current = if self.config.custom_model_name.trim().is_empty()
            || self.config.custom_model_name == self.config.model
        {
            self.config.model.clone()
        } else {
            format!("{} | {}", self.config.custom_model_name, self.config.model)
        };
        if self.modal.top() != Some(modal::ModalKind::Settings) {
            self.modal
                .reduce(modal::ModalAction::Push(modal::ModalKind::Settings));
        }
        self.settings
            .reduce(SettingsAction::SwitchTab(SettingsTab::Provider));
        self.settings.start_editing("custom_model_entry", &current);
        self.status_line = "Enter custom model as `Name | ID` or just `ID`".to_string();
    }

    pub(super) fn handle_key_modal(
        &mut self,
        code: KeyCode,
        modifiers: KeyModifiers,
        kind: modal::ModalKind,
    ) -> bool {
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
                                    self.config.custom_model_name = if name.is_empty() {
                                        model_id.to_string()
                                    } else {
                                        name.to_string()
                                    };
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
                            "context_window_tokens" => {
                                if self.config.provider == "custom" {
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
                            "snapshot_max_count" => {
                                if let Ok(n) = value.parse::<u32>() {
                                    self.config.snapshot_max_count = n.clamp(1, 100);
                                }
                            }
                            "snapshot_max_size_mb" => {
                                if let Ok(n) = value.parse::<u32>() {
                                    self.config.snapshot_max_size_mb = n.clamp(100, 500_000);
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
                            "feat_skill_promotion_threshold" => {
                                if let Ok(n) = value.parse::<u64>() {
                                    let clamped = n.clamp(1, 100);
                                    self.send_daemon_command(DaemonCommand::SetConfigItem {
                                        key_path: "/skill_discovery/promotion_threshold"
                                            .to_string(),
                                        value_json: format!("{}", clamped),
                                    });
                                    if let Some(ref mut raw) = self.config.agent_config_raw {
                                        if raw.get("skill_discovery").is_none() {
                                            raw["skill_discovery"] = serde_json::json!({});
                                        }
                                        raw["skill_discovery"]["promotion_threshold"] =
                                            serde_json::json!(clamped);
                                    }
                                }
                            }
                            _ => {}
                        }
                        self.settings.reduce(SettingsAction::ConfirmEdit);
                        self.sync_config_to_daemon();
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
                    } else if matches!(prev_tab, SettingsTab::Plugins) {
                        self.plugin_settings.list_mode = true;
                        self.send_daemon_command(DaemonCommand::PluginList);
                    }
                    return false;
                }
                KeyCode::Down => {
                    self.settings.reduce(SettingsAction::NavigateField(1));
                    return false;
                }
                KeyCode::Up => {
                    self.settings.reduce(SettingsAction::NavigateField(-1));
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

        if kind == modal::ModalKind::ApprovalOverlay {
            match code {
                KeyCode::Char('y') | KeyCode::Char('Y') => {
                    if let Some(ap) = self.approval.current_approval() {
                        let approval_id = ap.approval_id.clone();
                        self.approval.reduce(crate::state::ApprovalAction::Resolve {
                            approval_id: approval_id.clone(),
                            decision: "allow_once".to_string(),
                        });
                        self.send_daemon_command(DaemonCommand::ResolveTaskApproval {
                            approval_id,
                            decision: "allow_once".to_string(),
                        });
                    }
                    self.close_top_modal();
                }
                KeyCode::Char('a') | KeyCode::Char('A') => {
                    if let Some(ap) = self.approval.current_approval() {
                        let approval_id = ap.approval_id.clone();
                        self.approval.reduce(crate::state::ApprovalAction::Resolve {
                            approval_id: approval_id.clone(),
                            decision: "allow_session".to_string(),
                        });
                        self.send_daemon_command(DaemonCommand::ResolveTaskApproval {
                            approval_id,
                            decision: "allow_session".to_string(),
                        });
                    }
                    self.close_top_modal();
                }
                KeyCode::Char('n') | KeyCode::Char('N') => {
                    if let Some(ap) = self.approval.current_approval() {
                        let approval_id = ap.approval_id.clone();
                        self.approval.reduce(crate::state::ApprovalAction::Resolve {
                            approval_id: approval_id.clone(),
                            decision: "reject".to_string(),
                        });
                        self.send_daemon_command(DaemonCommand::ResolveTaskApproval {
                            approval_id,
                            decision: "reject".to_string(),
                        });
                    }
                    self.close_top_modal();
                }
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
                        if let Ok(mut clipboard) = arboard::Clipboard::new() {
                            let _ = clipboard.set_text(url.to_string());
                            self.status_line = "Copied ChatGPT login URL to clipboard".to_string();
                        }
                    }
                }
                KeyCode::Char('o') | KeyCode::Char('O') | KeyCode::Enter => {
                    self.handle_modal_enter(kind);
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
        tracing::info!("handle_modal_enter: {:?}", kind);
        match kind {
            modal::ModalKind::CommandPalette => {
                let cmd_name = self.modal.selected_command().map(|cmd| cmd.command.clone());
                tracing::info!(
                    "selected_command: {:?}, cursor: {}, filtered: {:?}",
                    cmd_name,
                    self.modal.picker_cursor(),
                    self.modal.filtered_items()
                );
                self.close_top_modal();
                self.input.reduce(input::InputAction::Clear);
                if let Some(command) = cmd_name {
                    self.execute_command(&command);
                }
            }
            modal::ModalKind::ThreadPicker => {
                let cursor = self.modal.picker_cursor();
                self.close_top_modal();
                self.input.reduce(input::InputAction::Clear);
                if cursor == 0 {
                    self.start_new_thread_view();
                    self.status_line = "New conversation".to_string();
                } else if let Some(thread) = self.chat.threads().get(cursor - 1) {
                    let tid = thread.id.clone();
                    let title = thread.title.clone();
                    self.open_thread_conversation(tid);
                    self.status_line = format!("Thread: {}", title);
                }
            }
            modal::ModalKind::GoalPicker => {
                let cursor = self.modal.picker_cursor();
                let selected = if cursor == 0 {
                    None
                } else {
                    self.filtered_goal_runs().get(cursor - 1).map(|run| {
                        sidebar::SidebarItemTarget::GoalRun {
                            goal_run_id: run.id.clone(),
                            step_id: None,
                        }
                    })
                };
                self.close_top_modal();
                self.input.reduce(input::InputAction::Clear);
                if cursor == 0 {
                    self.open_new_goal_view();
                } else if let Some(target) = selected {
                    self.open_sidebar_target(target);
                } else {
                    self.status_line = "No goals available".to_string();
                }
            }
            modal::ModalKind::ProviderPicker => {
                let cursor = self.modal.picker_cursor();
                if let Some(def) = providers::PROVIDERS.get(cursor) {
                    match self
                        .settings_picker_target
                        .unwrap_or(SettingsPickerTarget::Provider)
                    {
                        SettingsPickerTarget::Provider => {
                            self.apply_provider_selection(def.id);
                            // Chain into model picker so user can choose a model
                            // for the newly selected provider.
                            let models = providers::known_models_for_provider_auth(
                                &self.config.provider,
                                &self.config.auth_source,
                            );
                            if !models.is_empty() {
                                self.config
                                    .reduce(config::ConfigAction::ModelsFetched(models));
                            }
                            self.send_daemon_command(DaemonCommand::FetchModels {
                                provider_id: self.config.provider.clone(),
                                base_url: self.config.base_url.clone(),
                                api_key: self.config.api_key.clone(),
                            });
                            let count =
                                widgets::model_picker::available_models(&self.config).len() + 1;
                            self.settings_picker_target = None;
                            self.close_top_modal();
                            self.modal
                                .reduce(modal::ModalAction::Push(modal::ModalKind::ModelPicker));
                            self.modal.set_picker_item_count(count);
                            return;
                        }
                        SettingsPickerTarget::SubAgentProvider => {
                            if let Some(editor) = self.subagents.editor.as_mut() {
                                editor.provider = def.id.to_string();
                                let default_model =
                                    providers::default_model_for_provider_auth(def.id, "api_key");
                                if editor.model.trim().is_empty()
                                    || !providers::known_models_for_provider_auth(def.id, "api_key")
                                        .iter()
                                        .any(|model| model.id == editor.model)
                                {
                                    editor.model = default_model;
                                }
                            }
                            self.status_line = format!("Sub-agent provider: {}", def.name);
                        }
                        SettingsPickerTarget::ConciergeProvider => {
                            self.concierge.provider = Some(def.id.to_string());
                            let default_model =
                                providers::default_model_for_provider_auth(def.id, "api_key");
                            if self.concierge.model.as_deref().unwrap_or("").is_empty() {
                                self.concierge.model = Some(default_model);
                            }
                            self.send_concierge_config();
                            self.status_line = format!("Concierge provider: {}", def.name);
                        }
                        SettingsPickerTarget::Model
                        | SettingsPickerTarget::SubAgentModel
                        | SettingsPickerTarget::ConciergeModel => {}
                    }
                }
                self.settings_picker_target = None;
                self.close_top_modal();
            }
            modal::ModalKind::ModelPicker => {
                let models = widgets::model_picker::available_models(&self.config);
                let cursor = self.modal.picker_cursor();
                if cursor == models.len() {
                    self.settings_picker_target = None;
                    self.close_top_modal();
                    self.begin_custom_model_edit();
                    return;
                }
                if models.is_empty() {
                    self.status_line = "No models available. Set model in /settings".to_string();
                } else {
                    if let Some(model) = models.get(cursor) {
                        let model_id = model.id.clone();
                        let model_context_window = model.context_window;
                        match self
                            .settings_picker_target
                            .unwrap_or(SettingsPickerTarget::Model)
                        {
                            SettingsPickerTarget::Model => {
                                self.config
                                    .reduce(config::ConfigAction::SetModel(model_id.clone()));
                                self.config.custom_model_name =
                                    if providers::known_models_for_provider_auth(
                                        &self.config.provider,
                                        &self.config.auth_source,
                                    )
                                    .iter()
                                    .any(|entry| entry.id == model_id)
                                    {
                                        String::new()
                                    } else {
                                        model.name.clone().unwrap_or_else(|| model_id.clone())
                                    };
                                if self.config.provider != "custom" {
                                    self.config.context_window_tokens =
                                        model_context_window.unwrap_or(128_000);
                                }
                                self.status_line = format!("Model: {}", model_id);
                                if let Ok(value_json) = serde_json::to_string(
                                    &serde_json::Value::String(model_id.clone()),
                                ) {
                                    self.send_daemon_command(DaemonCommand::SetConfigItem {
                                        key_path: "/model".to_string(),
                                        value_json: value_json.clone(),
                                    });
                                    self.send_daemon_command(DaemonCommand::SetConfigItem {
                                        key_path: format!(
                                            "/providers/{}/model",
                                            self.config.provider
                                        ),
                                        value_json: value_json.clone(),
                                    });
                                    self.send_daemon_command(DaemonCommand::SetConfigItem {
                                        key_path: format!("/{}/model", self.config.provider),
                                        value_json,
                                    });
                                }
                                self.save_settings();
                            }
                            SettingsPickerTarget::SubAgentModel => {
                                if let Some(editor) = self.subagents.editor.as_mut() {
                                    editor.model = model_id.clone();
                                }
                                self.status_line = format!("Sub-agent model: {}", model_id);
                            }
                            SettingsPickerTarget::ConciergeModel => {
                                self.concierge.model = Some(model_id.clone());
                                self.send_concierge_config();
                                self.status_line = format!("Concierge model: {}", model_id);
                            }
                            SettingsPickerTarget::Provider
                            | SettingsPickerTarget::SubAgentProvider
                            | SettingsPickerTarget::ConciergeProvider => {}
                        }
                    }
                }
                self.settings_picker_target = None;
                self.close_top_modal();
            }
            modal::ModalKind::OpenAIAuth => {
                if let Some(url) = self.openai_auth_url.clone() {
                    if crate::auth::open_external_url(&url).is_ok() {
                        self.status_line = "Opened ChatGPT login in browser".to_string();
                    } else if let Ok(mut clipboard) = arboard::Clipboard::new() {
                        let _ = clipboard.set_text(url);
                        self.status_line = "Copied ChatGPT login URL to clipboard".to_string();
                    }
                }
            }
            modal::ModalKind::EffortPicker => {
                let efforts = ["", "low", "medium", "high", "xhigh"];
                let cursor = self.modal.picker_cursor();
                if let Some(&effort) = efforts.get(cursor) {
                    self.config
                        .reduce(config::ConfigAction::SetReasoningEffort(effort.to_string()));
                    if let Ok(value_json) =
                        serde_json::to_string(&serde_json::Value::String(effort.to_string()))
                    {
                        self.send_daemon_command(DaemonCommand::SetConfigItem {
                            key_path: "/reasoning_effort".to_string(),
                            value_json: value_json.clone(),
                        });
                        self.send_daemon_command(DaemonCommand::SetConfigItem {
                            key_path: format!(
                                "/providers/{}/reasoning_effort",
                                self.config.provider
                            ),
                            value_json: value_json.clone(),
                        });
                        self.send_daemon_command(DaemonCommand::SetConfigItem {
                            key_path: format!("/{}/reasoning_effort", self.config.provider),
                            value_json,
                        });
                    }
                    self.status_line = if effort.is_empty() {
                        "Effort: off".to_string()
                    } else {
                        format!("Effort: {}", effort)
                    };
                    self.save_settings();
                }
                self.close_top_modal();
            }
            modal::ModalKind::WhatsAppLink => {}
            _ => {
                self.close_top_modal();
                self.input.reduce(input::InputAction::Clear);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::mpsc::unbounded_channel;

    fn make_model() -> (
        TuiModel,
        tokio::sync::mpsc::UnboundedReceiver<DaemonCommand>,
    ) {
        let (_event_tx, event_rx) = std::sync::mpsc::channel();
        let (daemon_tx, daemon_rx) = unbounded_channel();
        (TuiModel::new(event_rx, daemon_tx), daemon_rx)
    }

    #[test]
    fn whatsapp_modal_esc_sends_stop_and_closes() {
        let (mut model, mut daemon_rx) = make_model();
        model
            .modal
            .reduce(modal::ModalAction::Push(modal::ModalKind::WhatsAppLink));
        assert_eq!(model.modal.top(), Some(modal::ModalKind::WhatsAppLink));

        let quit = model.handle_key_modal(
            KeyCode::Esc,
            KeyModifiers::NONE,
            modal::ModalKind::WhatsAppLink,
        );
        assert!(!quit);
        assert!(model.modal.top().is_none());
        assert!(matches!(
            daemon_rx.try_recv().expect("expected stop command"),
            DaemonCommand::WhatsAppLinkStop
        ));
        assert!(matches!(
            daemon_rx.try_recv().expect("expected unsubscribe command"),
            DaemonCommand::WhatsAppLinkUnsubscribe
        ));
    }

    #[test]
    fn whatsapp_modal_cancel_sends_stop_and_closes() {
        let (mut model, mut daemon_rx) = make_model();
        model
            .modal
            .reduce(modal::ModalAction::Push(modal::ModalKind::WhatsAppLink));

        let quit = model.handle_key_modal(
            KeyCode::Char('c'),
            KeyModifiers::NONE,
            modal::ModalKind::WhatsAppLink,
        );
        assert!(!quit);
        assert!(model.modal.top().is_none());
        assert!(matches!(
            daemon_rx.try_recv().expect("expected stop command"),
            DaemonCommand::WhatsAppLinkStop
        ));
        assert!(matches!(
            daemon_rx.try_recv().expect("expected unsubscribe command"),
            DaemonCommand::WhatsAppLinkUnsubscribe
        ));
    }

    #[test]
    fn stacked_modal_pop_only_cleans_whatsapp_when_top() {
        let (mut model, mut daemon_rx) = make_model();
        model
            .modal
            .reduce(modal::ModalAction::Push(modal::ModalKind::WhatsAppLink));
        model
            .modal
            .reduce(modal::ModalAction::Push(modal::ModalKind::CommandPalette));

        let quit = model.handle_key_modal(
            KeyCode::Esc,
            KeyModifiers::NONE,
            modal::ModalKind::CommandPalette,
        );
        assert!(!quit);
        assert_eq!(model.modal.top(), Some(modal::ModalKind::WhatsAppLink));
        assert!(daemon_rx.try_recv().is_err());

        let quit = model.handle_key_modal(
            KeyCode::Esc,
            KeyModifiers::NONE,
            modal::ModalKind::WhatsAppLink,
        );
        assert!(!quit);
        assert!(model.modal.top().is_none());
        assert!(matches!(
            daemon_rx.try_recv().expect("expected stop command"),
            DaemonCommand::WhatsAppLinkStop
        ));
        assert!(matches!(
            daemon_rx.try_recv().expect("expected unsubscribe command"),
            DaemonCommand::WhatsAppLinkUnsubscribe
        ));
    }
}
