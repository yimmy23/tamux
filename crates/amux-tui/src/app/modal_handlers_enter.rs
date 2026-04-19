use super::*;
use amux_shared::providers::{AudioToolKind, PROVIDER_ID_CUSTOM};

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

fn json_array_contains_modality(value: Option<&serde_json::Value>, modality: &str) -> bool {
    value
        .and_then(|value| value.as_array())
        .map(|items| {
            items.iter().any(|item| {
                item.as_str()
                    .map(str::trim)
                    .map(|value| value.eq_ignore_ascii_case(modality))
                    .unwrap_or(false)
            })
        })
        .unwrap_or(false)
}

fn json_string_has_modality(value: Option<&serde_json::Value>, modality: &str) -> bool {
    value
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_ascii_lowercase().contains(modality))
        .unwrap_or(false)
}

fn selected_model_supports_image_input(model: &crate::state::config::FetchedModel) -> bool {
    let metadata = model.metadata.as_ref();
    json_array_contains_modality(
        metadata
            .and_then(|value| value.pointer("/architecture/input_modalities"))
            .or_else(|| metadata.and_then(|value| value.pointer("/input_modalities")))
            .or_else(|| metadata.and_then(|value| value.pointer("/modalities"))),
        "image",
    ) || json_string_has_modality(
        metadata
            .and_then(|value| value.pointer("/architecture/modality"))
            .or_else(|| metadata.and_then(|value| value.pointer("/modality"))),
        "image",
    )
}

fn selected_model_supports_audio(model: &crate::state::config::FetchedModel) -> bool {
    if model
        .pricing
        .as_ref()
        .and_then(|pricing| pricing.audio.as_deref())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .is_some()
    {
        return true;
    }

    let metadata = model.metadata.as_ref();
    json_array_contains_modality(
        metadata
            .and_then(|value| value.pointer("/architecture/input_modalities"))
            .or_else(|| metadata.and_then(|value| value.pointer("/input_modalities")))
            .or_else(|| metadata.and_then(|value| value.pointer("/modalities"))),
        "audio",
    ) || json_array_contains_modality(
        metadata
            .and_then(|value| value.pointer("/architecture/output_modalities"))
            .or_else(|| metadata.and_then(|value| value.pointer("/output_modalities")))
            .or_else(|| metadata.and_then(|value| value.pointer("/modalities"))),
        "audio",
    ) || json_string_has_modality(
        metadata
            .and_then(|value| value.pointer("/architecture/modality"))
            .or_else(|| metadata.and_then(|value| value.pointer("/modality"))),
        "audio",
    )
}

fn enable_vision_tool(model: &mut TuiModel) {
    model.config.tool_vision = true;
    if let Some(raw) = model.config.agent_config_raw.as_mut() {
        if raw.get("tools").is_none() {
            raw["tools"] = serde_json::json!({});
        }
        raw["tools"]["vision"] = serde_json::Value::Bool(true);
    }
    model.send_daemon_command(DaemonCommand::SetConfigItem {
        key_path: "/tools/vision".to_string(),
        value_json: "true".to_string(),
    });
}

pub(super) fn handle_modal_enter(model: &mut TuiModel, kind: modal::ModalKind) {
    tracing::info!("handle_modal_enter: {:?}", kind);
    match kind {
        modal::ModalKind::CommandPalette => {
            let cmd_name = model
                .modal
                .selected_command()
                .map(|cmd| cmd.command.clone());
            let query = model.modal.command_query().trim().to_string();
            tracing::info!(
                "selected_command: {:?}, cursor: {}, filtered: {:?}",
                cmd_name,
                model.modal.picker_cursor(),
                model.modal.filtered_items()
            );
            model.close_top_modal();
            model.input.reduce(input::InputAction::Clear);
            let query_head = query
                .trim_start_matches('/')
                .split_whitespace()
                .next()
                .unwrap_or("");
            if !query.is_empty()
                && !query_head.is_empty()
                && (cmd_name.as_deref() == Some(query_head) || model.is_builtin_command(query_head))
            {
                let command_line = if query.starts_with('/') {
                    query
                } else {
                    format!("/{query}")
                };
                model.execute_slash_command_line(&command_line);
            } else if let Some(command) = cmd_name {
                model.execute_command(&command);
            }
        }
        modal::ModalKind::ThreadPicker => {
            let cursor = model.modal.picker_cursor();
            let thread_picker_tab = model.modal.thread_picker_tab();
            if cursor == 0 {
                if thread_picker_tab == modal::ThreadPickerTab::Playgrounds {
                    model.status_line = "Playgrounds are created automatically".to_string();
                    return;
                }
                model.close_top_modal();
                model.input.reduce(input::InputAction::Clear);
                model.start_new_thread_view_for_agent(TuiModel::thread_picker_target_agent_id(
                    thread_picker_tab,
                ));
                model.status_line = "New conversation".to_string();
            } else if let Some((tid, title)) =
                widgets::thread_picker::filtered_threads(&model.chat, &model.modal)
                    .get(cursor - 1)
                    .map(|thread| {
                        (
                            thread.id.clone(),
                            widgets::thread_picker::thread_display_title(thread),
                        )
                    })
            {
                model.close_top_modal();
                model.input.reduce(input::InputAction::Clear);
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
        modal::ModalKind::PinnedBudgetExceeded => {
            model.close_pinned_budget_exceeded_modal();
        }
        modal::ModalKind::ProviderPicker => {
            let cursor = model.modal.picker_cursor();
            let provider_defs = match model.settings_picker_target {
                Some(SettingsPickerTarget::AudioSttProvider) => {
                    widgets::provider_picker::available_audio_provider_defs(
                        &model.auth,
                        AudioToolKind::SpeechToText,
                    )
                }
                Some(SettingsPickerTarget::AudioTtsProvider) => {
                    widgets::provider_picker::available_audio_provider_defs(
                        &model.auth,
                        AudioToolKind::TextToSpeech,
                    )
                }
                _ => widgets::provider_picker::available_provider_defs(&model.auth),
            };
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
                            if model.should_fetch_remote_models(
                                &model.config.provider,
                                &model.config.auth_source,
                            ) {
                                model.send_daemon_command(DaemonCommand::FetchModels {
                                    provider_id: model.config.provider.clone(),
                                    base_url: model.config.base_url.clone(),
                                    api_key: model.config.api_key.clone(),
                                });
                            }
                            model
                                .modal
                                .reduce(modal::ModalAction::Push(modal::ModalKind::ModelPicker));
                            model.sync_model_picker_item_count();
                        }
                        return;
                    }
                    SettingsPickerTarget::AudioSttProvider => {
                        let current_model = model.config.audio_stt_model();
                        let known_models = TuiModel::audio_catalog_models("stt", def.id);
                        let next_model = if current_model.trim().is_empty()
                            || (!known_models.is_empty()
                                && !known_models.iter().any(|entry| entry.id == current_model))
                        {
                            TuiModel::default_audio_model_for("stt", def.id)
                        } else {
                            current_model
                        };
                        model.set_audio_config_string("stt", "provider", def.id.to_string());
                        if !next_model.trim().is_empty() {
                            model.set_audio_config_string("stt", "model", next_model);
                        }
                        model.close_top_modal();
                        model.open_audio_model_picker("stt");
                        model.status_line = format!("STT provider: {}", def.name);
                        return;
                    }
                    SettingsPickerTarget::AudioTtsProvider => {
                        let current_model = model.config.audio_tts_model();
                        let known_models = TuiModel::audio_catalog_models("tts", def.id);
                        let next_model = if current_model.trim().is_empty()
                            || (!known_models.is_empty()
                                && !known_models.iter().any(|entry| entry.id == current_model))
                        {
                            TuiModel::default_audio_model_for("tts", def.id)
                        } else {
                            current_model
                        };
                        model.set_audio_config_string("tts", "provider", def.id.to_string());
                        if !next_model.trim().is_empty() {
                            model.set_audio_config_string("tts", "model", next_model);
                        }
                        model.close_top_modal();
                        model.open_audio_model_picker("tts");
                        model.status_line = format!("TTS provider: {}", def.name);
                        return;
                    }
                    SettingsPickerTarget::BuiltinPersonaProvider => {
                        model.apply_provider_selection_without_sync(def.id);
                        model.close_top_modal();
                        if model.config.provider == PROVIDER_ID_CUSTOM {
                            model.status_line =
                                "Custom provider setup for builtin personas is not supported here"
                                    .to_string();
                            model.restore_builtin_persona_setup_config_snapshot();
                            model.pending_builtin_persona_setup = None;
                            model.settings_picker_target = None;
                            return;
                        }
                        if model.should_fetch_remote_models(
                            &model.config.provider,
                            &model.config.auth_source,
                        ) {
                            model.send_daemon_command(DaemonCommand::FetchModels {
                                provider_id: model.config.provider.clone(),
                                base_url: model.config.base_url.clone(),
                                api_key: model.config.api_key.clone(),
                            });
                        }
                        model.settings_picker_target =
                            Some(SettingsPickerTarget::BuiltinPersonaModel);
                        model
                            .modal
                            .reduce(modal::ModalAction::Push(modal::ModalKind::ModelPicker));
                        model.sync_model_picker_item_count();
                        if let Some(setup) = model.pending_builtin_persona_setup.as_ref() {
                            model.status_line =
                                format!("Configure {} model", setup.target_agent_name);
                        }
                        return;
                    }
                    SettingsPickerTarget::CompactionWelesProvider => {
                        let (_, _, auth_source) = model.provider_auth_snapshot(def.id);
                        model.config.compaction_weles_provider = def.id.to_string();
                        if model.config.compaction_weles_model.trim().is_empty()
                            || !providers::known_models_for_provider_auth(def.id, &auth_source)
                                .iter()
                                .any(|model_entry| {
                                    model_entry.id == model.config.compaction_weles_model
                                })
                        {
                            model.config.compaction_weles_model =
                                providers::default_model_for_provider_auth(def.id, &auth_source);
                        }
                        model.close_top_modal();
                        model.open_compaction_weles_model_picker();
                        model.sync_config_to_daemon();
                        return;
                    }
                    SettingsPickerTarget::CompactionCustomProvider => {
                        let (base_url, api_key, auth_source) = model.provider_auth_snapshot(def.id);
                        model.apply_compaction_custom_provider(def.id);
                        model.config.compaction_custom_base_url = base_url;
                        model.config.compaction_custom_api_key = api_key;
                        model.config.compaction_custom_auth_source = auth_source.clone();
                        if model.config.compaction_custom_model.trim().is_empty()
                            || !providers::known_models_for_provider_auth(def.id, &auth_source)
                                .iter()
                                .any(|model_entry| {
                                    model_entry.id == model.config.compaction_custom_model
                                })
                        {
                            model.config.compaction_custom_model =
                                providers::default_model_for_provider_auth(def.id, &auth_source);
                        }
                        model.normalize_compaction_custom_transport();
                        model.close_top_modal();
                        model.open_compaction_custom_model_picker();
                        model.sync_config_to_daemon();
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
                    | SettingsPickerTarget::AudioSttModel
                    | SettingsPickerTarget::AudioTtsModel
                    | SettingsPickerTarget::BuiltinPersonaModel
                    | SettingsPickerTarget::CompactionWelesModel
                    | SettingsPickerTarget::CompactionCustomModel
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
            let models = model.available_model_picker_models();
            let cursor = model.modal.picker_cursor();
            if cursor == models.len() {
                let picker_target = model.settings_picker_target;
                model.settings_picker_target = None;
                model.close_top_modal();
                model.begin_targeted_custom_model_edit(picker_target);
                return;
            }
            if models.is_empty() {
                model.status_line = "No models available. Set model in /settings".to_string();
            } else if let Some(model_entry) = models.get(cursor) {
                let model_id = model_entry.id.clone();
                let model_context_window = model_entry.context_window;
                let mut follow_up_confirm = None;
                match model
                    .settings_picker_target
                    .unwrap_or(SettingsPickerTarget::Model)
                {
                    SettingsPickerTarget::Model => {
                        model
                            .config
                            .reduce(config::ConfigAction::SetModel(model_id.clone()));
                        if providers::model_uses_context_window_override(
                            &model.config.provider,
                            &model.config.auth_source,
                            &model.config.model,
                            &model.config.custom_model_name,
                        ) {
                            model.config.custom_model_name =
                                model_entry.name.clone().unwrap_or_else(|| model_id.clone());
                            let next_context = model_context_window.unwrap_or(
                                model
                                    .config
                                    .custom_context_window_tokens
                                    .unwrap_or(providers::default_custom_model_context_window()),
                            );
                            model.config.custom_context_window_tokens = Some(next_context);
                            model.config.context_window_tokens = next_context;
                        } else {
                            model.config.custom_model_name.clear();
                            model.config.custom_context_window_tokens = None;
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
                        if selected_model_supports_image_input(model_entry)
                            && !model.config.tool_vision
                        {
                            enable_vision_tool(model);
                        }
                        if selected_model_supports_audio(model_entry)
                            && model.config.audio_stt_model() != model_id
                        {
                            follow_up_confirm = Some(PendingConfirmAction::ReuseModelAsStt {
                                model_id: model_id.clone(),
                            });
                        }
                        model.save_settings();
                    }
                    SettingsPickerTarget::AudioSttModel => {
                        model.set_audio_config_string("stt", "model", model_id.clone());
                        model.status_line = format!("STT model: {}", model_id);
                    }
                    SettingsPickerTarget::AudioTtsModel => {
                        model.set_audio_config_string("tts", "model", model_id.clone());
                        model.status_line = format!("TTS model: {}", model_id);
                    }
                    SettingsPickerTarget::BuiltinPersonaModel => {
                        let Some(setup) = model.pending_builtin_persona_setup.clone() else {
                            model.status_line = "No builtin persona setup is active".to_string();
                            model.settings_picker_target = None;
                            model.close_top_modal();
                            return;
                        };
                        let selected_provider = model.config.provider.clone();
                        model.send_daemon_command(DaemonCommand::SetTargetAgentProviderModel {
                            target_agent_id: setup.target_agent_id.clone(),
                            provider_id: selected_provider.clone(),
                            model: model_id.clone(),
                        });
                        let mut raw = model
                            .config
                            .agent_config_raw
                            .clone()
                            .unwrap_or_else(|| serde_json::json!({}));
                        if raw.get("builtin_sub_agents").is_none() {
                            raw["builtin_sub_agents"] = serde_json::json!({});
                        }
                        raw["builtin_sub_agents"][setup.target_agent_id.as_str()]["provider"] =
                            serde_json::Value::String(selected_provider.clone());
                        raw["builtin_sub_agents"][setup.target_agent_id.as_str()]["model"] =
                            serde_json::Value::String(model_id.clone());
                        model.config.agent_config_raw = Some(raw);
                        model.restore_builtin_persona_setup_config_snapshot();
                        model.pending_builtin_persona_setup = None;
                        model.status_line = format!(
                            "{} configured with {} / {}",
                            setup.target_agent_name, selected_provider, model_id
                        );
                        model.settings_picker_target = None;
                        model.close_top_modal();
                        model.submit_prompt(setup.prompt);
                        return;
                    }
                    SettingsPickerTarget::CompactionWelesModel => {
                        model.config.compaction_weles_model = model_id.clone();
                        model.status_line = format!("Compaction WELES model: {}", model_id);
                        model.sync_config_to_daemon();
                    }
                    SettingsPickerTarget::CompactionCustomModel => {
                        model.config.compaction_custom_model = model_id.clone();
                        if let Some(context_window) = model_context_window {
                            model.config.compaction_custom_context_window_tokens = context_window;
                        }
                        model.normalize_compaction_custom_transport();
                        model.status_line = format!("Compaction custom model: {}", model_id);
                        model.sync_config_to_daemon();
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
                    | SettingsPickerTarget::AudioSttProvider
                    | SettingsPickerTarget::AudioTtsProvider
                    | SettingsPickerTarget::BuiltinPersonaProvider
                    | SettingsPickerTarget::CompactionWelesProvider
                    | SettingsPickerTarget::CompactionCustomProvider
                    | SettingsPickerTarget::SubAgentProvider
                    | SettingsPickerTarget::SubAgentReasoningEffort
                    | SettingsPickerTarget::ConciergeProvider
                    | SettingsPickerTarget::ConciergeReasoningEffort
                    | SettingsPickerTarget::CompactionWelesReasoningEffort
                    | SettingsPickerTarget::CompactionCustomReasoningEffort => {}
                }
                model.settings_picker_target = None;
                model.close_top_modal();
                if let Some(action) = follow_up_confirm {
                    model.open_pending_action_confirm(action);
                }
                return;
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
