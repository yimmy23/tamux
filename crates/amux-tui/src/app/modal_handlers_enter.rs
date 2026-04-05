use super::*;
use amux_shared::providers::PROVIDER_ID_CUSTOM;

pub(super) fn begin_custom_model_edit(model: &mut TuiModel) {
    let current = if model.config.custom_model_name.trim().is_empty()
        || model.config.custom_model_name == model.config.model
    {
        model.config.model.clone()
    } else {
        format!(
            "{} | {}",
            model.config.custom_model_name, model.config.model
        )
    };
    if model.modal.top() != Some(modal::ModalKind::Settings) {
        model
            .modal
            .reduce(modal::ModalAction::Push(modal::ModalKind::Settings));
    }
    model
        .settings
        .reduce(SettingsAction::SwitchTab(SettingsTab::Agent));
    model.settings_navigate_to(3);
    model.settings.start_editing("custom_model_entry", &current);
    model.status_line = "Enter custom model as `Name | ID` or just `ID`".to_string();
}

pub(super) fn handle_modal_enter(model: &mut TuiModel, kind: modal::ModalKind) {
    tracing::info!("handle_modal_enter: {:?}", kind);
    match kind {
        modal::ModalKind::CommandPalette => {
            let cmd_name = model
                .modal
                .selected_command()
                .map(|cmd| cmd.command.clone());
            tracing::info!(
                "selected_command: {:?}, cursor: {}, filtered: {:?}",
                cmd_name,
                model.modal.picker_cursor(),
                model.modal.filtered_items()
            );
            model.close_top_modal();
            model.input.reduce(input::InputAction::Clear);
            if let Some(command) = cmd_name {
                model.execute_command(&command);
            }
        }
        modal::ModalKind::ThreadPicker => {
            let cursor = model.modal.picker_cursor();
            let thread_picker_tab = model.modal.thread_picker_tab();
            model.close_top_modal();
            model.input.reduce(input::InputAction::Clear);
            if cursor == 0 {
                model.start_new_thread_view_for_agent(TuiModel::thread_picker_target_agent_id(
                    thread_picker_tab,
                ));
                model.status_line = "New conversation".to_string();
            } else if let Some(thread) =
                widgets::thread_picker::filtered_threads(&model.chat, &model.modal).get(cursor - 1)
            {
                let tid = thread.id.clone();
                let title = widgets::thread_picker::thread_display_title(thread);
                model.open_thread_conversation(tid);
                model.status_line = format!("Thread: {}", title);
            }
        }
        modal::ModalKind::GoalPicker => {
            let cursor = model.modal.picker_cursor();
            let selected = if cursor == 0 {
                None
            } else {
                model.filtered_goal_runs().get(cursor - 1).map(|run| {
                    sidebar::SidebarItemTarget::GoalRun {
                        goal_run_id: run.id.clone(),
                        step_id: None,
                    }
                })
            };
            model.close_top_modal();
            model.input.reduce(input::InputAction::Clear);
            if cursor == 0 {
                model.open_new_goal_view();
            } else if let Some(target) = selected {
                model.open_sidebar_target(target);
            } else {
                model.status_line = "No goals available".to_string();
            }
        }
        modal::ModalKind::QueuedPrompts => {
            model.execute_selected_queued_prompt_action();
        }
        modal::ModalKind::ProviderPicker => {
            let cursor = model.modal.picker_cursor();
            let provider_defs = widgets::provider_picker::available_provider_defs(&model.auth);
            if let Some(def) = provider_defs.get(cursor) {
                match model
                    .settings_picker_target
                    .unwrap_or(SettingsPickerTarget::Provider)
                {
                    SettingsPickerTarget::Provider => {
                        model.apply_provider_selection(def.id);
                        model.settings_picker_target = None;
                        model.close_top_modal();
                        if model.config.provider == PROVIDER_ID_CUSTOM {
                            model.settings_navigate_to(3);
                        } else {
                            let models = providers::known_models_for_provider_auth(
                                &model.config.provider,
                                &model.config.auth_source,
                            );
                            if !models.is_empty() {
                                model
                                    .config
                                    .reduce(config::ConfigAction::ModelsFetched(models));
                            }
                            if providers::supports_model_fetch_for(&model.config.provider) {
                                model.send_daemon_command(DaemonCommand::FetchModels {
                                    provider_id: model.config.provider.clone(),
                                    base_url: model.config.base_url.clone(),
                                    api_key: model.config.api_key.clone(),
                                });
                            }
                            let count =
                                widgets::model_picker::available_models(&model.config).len() + 1;
                            model
                                .modal
                                .reduce(modal::ModalAction::Push(modal::ModalKind::ModelPicker));
                            model.modal.set_picker_item_count(count);
                        }
                        return;
                    }
                    SettingsPickerTarget::SubAgentProvider => {
                        if let Some(editor) = model.subagents.editor.as_mut() {
                            editor.provider = def.id.to_string();
                            let default_model =
                                providers::default_model_for_provider_auth(def.id, "api_key");
                            if editor.model.trim().is_empty()
                                || !providers::known_models_for_provider_auth(def.id, "api_key")
                                    .iter()
                                    .any(|model_entry| model_entry.id == editor.model)
                            {
                                editor.model = default_model;
                            }
                        }
                        model.status_line = format!("Sub-agent provider: {}", def.name);
                    }
                    SettingsPickerTarget::ConciergeProvider => {
                        model.concierge.provider = Some(def.id.to_string());
                        let default_model =
                            providers::default_model_for_provider_auth(def.id, "api_key");
                        if model.concierge.model.as_deref().unwrap_or("").is_empty() {
                            model.concierge.model = Some(default_model);
                        }
                        model.send_concierge_config();
                        model.status_line = format!("Rarog provider: {}", def.name);
                    }
                    SettingsPickerTarget::Model
                    | SettingsPickerTarget::SubAgentModel
                    | SettingsPickerTarget::SubAgentReasoningEffort
                    | SettingsPickerTarget::ConciergeModel
                    | SettingsPickerTarget::ConciergeReasoningEffort
                    | SettingsPickerTarget::CompactionWelesReasoningEffort
                    | SettingsPickerTarget::CompactionCustomReasoningEffort => {}
                }
            } else {
                model.status_line = "No authenticated providers available".to_string();
            }
            model.settings_picker_target = None;
            model.close_top_modal();
        }
        modal::ModalKind::ModelPicker => {
            let models = widgets::model_picker::available_models(&model.config);
            let cursor = model.modal.picker_cursor();
            if cursor == models.len() {
                model.settings_picker_target = None;
                model.close_top_modal();
                begin_custom_model_edit(model);
                return;
            }
            if models.is_empty() {
                model.status_line = "No models available. Set model in /settings".to_string();
            } else if let Some(model_entry) = models.get(cursor) {
                let model_id = model_entry.id.clone();
                let model_context_window = model_entry.context_window;
                match model
                    .settings_picker_target
                    .unwrap_or(SettingsPickerTarget::Model)
                {
                    SettingsPickerTarget::Model => {
                        model
                            .config
                            .reduce(config::ConfigAction::SetModel(model_id.clone()));
                        model.config.custom_model_name =
                            if providers::known_models_for_provider_auth(
                                &model.config.provider,
                                &model.config.auth_source,
                            )
                            .iter()
                            .any(|entry| entry.id == model_id)
                            {
                                String::new()
                            } else {
                                model_entry.name.clone().unwrap_or_else(|| model_id.clone())
                            };
                        if model.config.provider != PROVIDER_ID_CUSTOM {
                            model.config.context_window_tokens =
                                model_context_window.unwrap_or(128_000);
                        }
                        model.status_line = format!("Model: {}", model_id);
                        if let Ok(value_json) =
                            serde_json::to_string(&serde_json::Value::String(model_id.clone()))
                        {
                            model.send_daemon_command(DaemonCommand::SetConfigItem {
                                key_path: "/model".to_string(),
                                value_json: value_json.clone(),
                            });
                            model.send_daemon_command(DaemonCommand::SetConfigItem {
                                key_path: format!("/providers/{}/model", model.config.provider),
                                value_json: value_json.clone(),
                            });
                            model.send_daemon_command(DaemonCommand::SetConfigItem {
                                key_path: format!("/{}/model", model.config.provider),
                                value_json,
                            });
                        }
                        model.save_settings();
                    }
                    SettingsPickerTarget::SubAgentModel => {
                        if let Some(editor) = model.subagents.editor.as_mut() {
                            editor.model = model_id.clone();
                        }
                        model.status_line = format!("Sub-agent model: {}", model_id);
                    }
                    SettingsPickerTarget::ConciergeModel => {
                        model.concierge.model = Some(model_id.clone());
                        model.send_concierge_config();
                        model.status_line = format!("Rarog model: {}", model_id);
                    }
                    SettingsPickerTarget::Provider
                    | SettingsPickerTarget::SubAgentProvider
                    | SettingsPickerTarget::SubAgentReasoningEffort
                    | SettingsPickerTarget::ConciergeProvider
                    | SettingsPickerTarget::ConciergeReasoningEffort
                    | SettingsPickerTarget::CompactionWelesReasoningEffort
                    | SettingsPickerTarget::CompactionCustomReasoningEffort => {}
                }
            }
            model.settings_picker_target = None;
            model.close_top_modal();
        }
        modal::ModalKind::OpenAIAuth => {
            if let Some(url) = model.openai_auth_url.clone() {
                if crate::auth::open_external_url(&url).is_ok() {
                    model.status_line = "Opened ChatGPT login in browser".to_string();
                } else if let Ok(mut clipboard) = arboard::Clipboard::new() {
                    let _ = clipboard.set_text(url);
                    model.status_line = "Copied ChatGPT login URL to clipboard".to_string();
                }
            }
        }
        modal::ModalKind::EffortPicker => {
            let efforts = ["", "minimal", "low", "medium", "high", "xhigh"];
            let cursor = model.modal.picker_cursor();
            if let Some(&effort) = efforts.get(cursor) {
                match model.settings_picker_target {
                    Some(SettingsPickerTarget::SubAgentReasoningEffort) => {
                        if let Some(editor) = model.subagents.editor.as_mut() {
                            editor.reasoning_effort = if effort.is_empty() {
                                Some("none".to_string())
                            } else {
                                Some(effort.to_string())
                            };
                        }
                        model.status_line = if effort.is_empty() {
                            "Sub-agent effort: none".to_string()
                        } else {
                            format!("Sub-agent effort: {}", effort)
                        };
                    }
                    Some(SettingsPickerTarget::ConciergeReasoningEffort) => {
                        model.concierge.reasoning_effort = if effort.is_empty() {
                            None
                        } else {
                            Some(effort.to_string())
                        };
                        model.send_concierge_config();
                        model.status_line = if effort.is_empty() {
                            "Rarog effort: none".to_string()
                        } else {
                            format!("Rarog effort: {}", effort)
                        };
                    }
                    Some(SettingsPickerTarget::CompactionWelesReasoningEffort) => {
                        model.config.compaction_weles_reasoning_effort = if effort.is_empty() {
                            "none".to_string()
                        } else {
                            effort.to_string()
                        };
                        model.sync_config_to_daemon();
                        model.status_line = if effort.is_empty() {
                            "Compaction WELES effort: none".to_string()
                        } else {
                            format!("Compaction WELES effort: {}", effort)
                        };
                    }
                    Some(SettingsPickerTarget::CompactionCustomReasoningEffort) => {
                        model.config.compaction_custom_reasoning_effort = if effort.is_empty() {
                            "none".to_string()
                        } else {
                            effort.to_string()
                        };
                        model.sync_config_to_daemon();
                        model.status_line = if effort.is_empty() {
                            "Compaction custom effort: none".to_string()
                        } else {
                            format!("Compaction custom effort: {}", effort)
                        };
                    }
                    _ => {
                        model
                            .config
                            .reduce(config::ConfigAction::SetReasoningEffort(effort.to_string()));
                        if let Ok(value_json) =
                            serde_json::to_string(&serde_json::Value::String(effort.to_string()))
                        {
                            model.send_daemon_command(DaemonCommand::SetConfigItem {
                                key_path: "/reasoning_effort".to_string(),
                                value_json: value_json.clone(),
                            });
                            model.send_daemon_command(DaemonCommand::SetConfigItem {
                                key_path: format!(
                                    "/providers/{}/reasoning_effort",
                                    model.config.provider
                                ),
                                value_json: value_json.clone(),
                            });
                            model.send_daemon_command(DaemonCommand::SetConfigItem {
                                key_path: format!("/{}/reasoning_effort", model.config.provider),
                                value_json,
                            });
                        }
                        model.status_line = if effort.is_empty() {
                            "Effort: none".to_string()
                        } else {
                            format!("Effort: {}", effort)
                        };
                        model.save_settings();
                    }
                }
            }
            model.settings_picker_target = None;
            model.close_top_modal();
        }
        modal::ModalKind::WhatsAppLink => {}
        _ => {
            model.close_top_modal();
            model.input.reduce(input::InputAction::Clear);
        }
    }
}
