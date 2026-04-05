use amux_shared::providers::{
    PROVIDER_ID_ALIBABA_CODING_PLAN, PROVIDER_ID_ANTHROPIC, PROVIDER_ID_CUSTOM,
    PROVIDER_ID_GITHUB_COPILOT, PROVIDER_ID_OPENAI,
};

#[test]
fn whatsapp_link_device_probes_status_before_starting_link_flow() {
    let (mut model, mut daemon_rx) = make_model();
    model
        .settings
        .reduce(SettingsAction::SwitchTab(SettingsTab::Gateway));
    model.config.whatsapp_allowed_contacts = "+48663977535".to_string();
    model.settings.reduce(SettingsAction::NavigateField(12));
    assert_eq!(model.settings.current_field_name(), "whatsapp_link_device");

    model.activate_settings_field();

    assert!(matches!(
        daemon_rx.try_recv().expect("expected subscribe command"),
        DaemonCommand::WhatsAppLinkSubscribe
    ));
    assert!(matches!(
        daemon_rx.try_recv().expect("expected status probe"),
        DaemonCommand::WhatsAppLinkStatus
    ));
    assert!(matches!(
        daemon_rx.try_recv().expect("expected start command"),
        DaemonCommand::WhatsAppLinkStart
    ));
    assert!(daemon_rx.try_recv().is_err());
    assert_eq!(model.modal.top(), Some(modal::ModalKind::WhatsAppLink));
}

#[test]
fn whatsapp_link_device_does_not_reset_existing_link() {
    let (mut model, mut daemon_rx) = make_model();
    model
        .settings
        .reduce(SettingsAction::SwitchTab(SettingsTab::Gateway));
    model.config.whatsapp_allowed_contacts = "+48663977535".to_string();
    model.settings.reduce(SettingsAction::NavigateField(12));
    model
        .modal
        .set_whatsapp_link_connected(Some("+48663977535".to_string()));

    model.activate_settings_field();

    assert!(matches!(
        daemon_rx.try_recv().expect("expected subscribe command"),
        DaemonCommand::WhatsAppLinkSubscribe
    ));
    assert!(matches!(
        daemon_rx.try_recv().expect("expected status command"),
        DaemonCommand::WhatsAppLinkStatus
    ));
    assert!(daemon_rx.try_recv().is_err());
    assert_eq!(model.modal.top(), Some(modal::ModalKind::WhatsAppLink));
    assert_eq!(model.status_line, "Showing WhatsApp link status");
}

#[test]
fn whatsapp_relink_device_resets_existing_link_before_restart() {
    let (mut model, mut daemon_rx) = make_model();
    model
        .settings
        .reduce(SettingsAction::SwitchTab(SettingsTab::Gateway));
    model.config.whatsapp_allowed_contacts = "+48663977535".to_string();
    model.settings.reduce(SettingsAction::NavigateField(13));
    model
        .modal
        .set_whatsapp_link_connected(Some("+48663977535".to_string()));

    model.activate_settings_field();

    assert!(matches!(
        daemon_rx.try_recv().expect("expected subscribe command"),
        DaemonCommand::WhatsAppLinkSubscribe
    ));
    assert!(matches!(
        daemon_rx.try_recv().expect("expected status command"),
        DaemonCommand::WhatsAppLinkStatus
    ));
    assert!(matches!(
        daemon_rx.try_recv().expect("expected reset command"),
        DaemonCommand::WhatsAppLinkReset
    ));
    assert!(matches!(
        daemon_rx.try_recv().expect("expected start command"),
        DaemonCommand::WhatsAppLinkStart
    ));
    assert!(daemon_rx.try_recv().is_err());
}

#[test]
fn activating_model_for_custom_provider_starts_inline_custom_model_edit() {
    let (mut model, _daemon_rx) = make_model();
    model.apply_provider_selection(PROVIDER_ID_CUSTOM);
    model
        .settings
        .reduce(SettingsAction::SwitchTab(SettingsTab::Provider));
    model.settings.reduce(SettingsAction::NavigateField(3));
    assert_eq!(model.settings.current_field_name(), "model");

    model.activate_settings_field();

    assert_eq!(model.settings.editing_field(), Some("custom_model_entry"));
    assert_eq!(model.settings.field_cursor(), 3);
    assert_ne!(model.modal.top(), Some(modal::ModalKind::ModelPicker));
}

#[test]
fn activating_message_loop_delay_starts_inline_edit() {
    let (mut model, _daemon_rx) = make_model();
    focus_settings_field(&mut model, SettingsTab::Advanced, "message_loop_delay_ms");

    assert_eq!(
        model.settings.current_field_name_with_config(&model.config),
        "message_loop_delay_ms"
    );

    model.activate_settings_field();

    assert_eq!(
        model.settings.editing_field(),
        Some("message_loop_delay_ms")
    );
    assert_eq!(model.settings.edit_buffer(), "500");
}

#[test]
fn whatsapp_link_device_requires_allowed_contacts() {
    let (mut model, mut daemon_rx) = make_model();
    model
        .settings
        .reduce(SettingsAction::SwitchTab(SettingsTab::Gateway));
    model.settings.reduce(SettingsAction::NavigateField(12));

    model.activate_settings_field();

    assert!(daemon_rx.try_recv().is_err());
    assert_eq!(
        model.status_line,
        "Set at least one allowed WhatsApp phone number before linking"
    );
    assert_eq!(model.modal.top(), None);
}

#[test]
fn whatsapp_relink_device_requires_allowed_contacts() {
    let (mut model, mut daemon_rx) = make_model();
    model
        .settings
        .reduce(SettingsAction::SwitchTab(SettingsTab::Gateway));
    model.settings.reduce(SettingsAction::NavigateField(13));
    model
        .modal
        .set_whatsapp_link_connected(Some("+48663977535".to_string()));

    model.activate_settings_field();

    assert!(daemon_rx.try_recv().is_err());
    assert_eq!(
        model.status_line,
        "Set at least one allowed WhatsApp phone number before linking"
    );
    assert_eq!(model.modal.top(), None);
}

#[test]
fn whatsapp_link_device_starts_when_allowlist_is_present() {
    let (mut model, mut daemon_rx) = make_model();
    model
        .settings
        .reduce(SettingsAction::SwitchTab(SettingsTab::Gateway));
    model.config.whatsapp_allowed_contacts = "+48663977535\ninvalid".to_string();
    model.settings.reduce(SettingsAction::NavigateField(12));

    model.activate_settings_field();

    assert!(matches!(
        daemon_rx.try_recv().expect("expected subscribe command"),
        DaemonCommand::WhatsAppLinkSubscribe
    ));
    assert!(matches!(
        daemon_rx.try_recv().expect("expected status probe"),
        DaemonCommand::WhatsAppLinkStatus
    ));
    assert!(matches!(
        daemon_rx.try_recv().expect("expected start command"),
        DaemonCommand::WhatsAppLinkStart
    ));
    assert!(daemon_rx.try_recv().is_err());
    assert_eq!(model.status_line, "Starting WhatsApp link workflow");
    assert_eq!(model.modal.top(), Some(modal::ModalKind::WhatsAppLink));
}

#[test]
fn github_copilot_logout_clears_db_auth_and_refreshes_entries() {
    let _lock = auth_env_lock();
    let _guard = EnvGuard::new(&["TAMUX_PROVIDER_AUTH_DB_PATH"]);
    let db_path = unique_test_db_path("copilot-logout");
    std::env::set_var("TAMUX_PROVIDER_AUTH_DB_PATH", &db_path);
    write_provider_auth_row(&db_path, PROVIDER_ID_GITHUB_COPILOT, "github_copilot");

    let (mut model, mut daemon_rx) = make_model();
    model.auth.entries = vec![crate::state::auth::ProviderAuthEntry {
        provider_id: PROVIDER_ID_GITHUB_COPILOT.to_string(),
        provider_name: "GitHub Copilot".to_string(),
        authenticated: true,
        auth_source: "github_copilot".to_string(),
        model: "openai/gpt-4.1".to_string(),
    }];
    model.auth.action_cursor = 0;
    model.auth.selected = 0;

    model.run_auth_tab_action();

    assert!(!has_provider_auth_row(
        &db_path,
        PROVIDER_ID_GITHUB_COPILOT,
        "github_copilot"
    ));
    assert_eq!(model.status_line, "GitHub Copilot auth cleared");
    assert!(matches!(
        daemon_rx.try_recv().expect("expected auth refresh"),
        DaemonCommand::GetProviderAuthStates
    ));
    let _ = std::fs::remove_file(&db_path);
}

#[test]
fn openai_chatgpt_login_requests_daemon_flow_without_local_modal_state() {
    let (mut model, mut daemon_rx) = make_model();
    model.auth.entries = vec![crate::state::auth::ProviderAuthEntry {
        provider_id: PROVIDER_ID_OPENAI.to_string(),
        provider_name: "OpenAI".to_string(),
        authenticated: false,
        auth_source: "chatgpt_subscription".to_string(),
        model: "gpt-5.4".to_string(),
    }];
    model.auth.action_cursor = 0;
    model.auth.selected = 0;

    model.run_auth_tab_action();

    assert!(matches!(
        daemon_rx.try_recv().expect("expected daemon login command"),
        DaemonCommand::LoginOpenAICodex
    ));
    assert!(daemon_rx.try_recv().is_err());
    assert_ne!(model.modal.top(), Some(modal::ModalKind::OpenAIAuth));
    assert!(model.openai_auth_url.is_none());
    assert!(model.openai_auth_status_text.is_none());
}

#[test]
fn openai_chatgpt_logout_requests_daemon_flow() {
    let (mut model, mut daemon_rx) = make_model();
    model.auth.entries = vec![crate::state::auth::ProviderAuthEntry {
        provider_id: PROVIDER_ID_OPENAI.to_string(),
        provider_name: "OpenAI".to_string(),
        authenticated: true,
        auth_source: "chatgpt_subscription".to_string(),
        model: "gpt-5.4".to_string(),
    }];
    model.auth.action_cursor = 0;

    model.run_auth_tab_action();

    assert!(matches!(
        daemon_rx
            .try_recv()
            .expect("expected daemon logout command"),
        DaemonCommand::LogoutOpenAICodex
    ));
    assert!(daemon_rx.try_recv().is_err());
}

#[test]
fn openai_chatgpt_second_action_requests_login_when_auth_not_available() {
    let (mut model, mut daemon_rx) = make_model();
    model.auth.entries = vec![crate::state::auth::ProviderAuthEntry {
        provider_id: PROVIDER_ID_OPENAI.to_string(),
        provider_name: "OpenAI".to_string(),
        authenticated: true,
        auth_source: "chatgpt_subscription".to_string(),
        model: "gpt-5.4".to_string(),
    }];
    model.auth.action_cursor = 1;
    model.auth.selected = 0;
    model.config.provider = PROVIDER_ID_OPENAI.to_string();
    model.config.auth_source = "chatgpt_subscription".to_string();
    model.config.chatgpt_auth_available = false;
    model.status_line = "before refresh".to_string();

    model.run_auth_tab_action();

    assert!(matches!(
        daemon_rx
            .try_recv()
            .expect("expected daemon login command"),
        DaemonCommand::LoginOpenAICodex
    ));
    assert!(daemon_rx.try_recv().is_err());
    assert_eq!(model.status_line, "before refresh");
}

#[test]
fn openai_chatgpt_second_action_restarts_login_after_logout() {
    let (mut model, mut daemon_rx) = make_model();
    model.auth.entries = vec![crate::state::auth::ProviderAuthEntry {
        provider_id: PROVIDER_ID_OPENAI.to_string(),
        provider_name: "OpenAI".to_string(),
        authenticated: false,
        auth_source: "chatgpt_subscription".to_string(),
        model: "gpt-5.4".to_string(),
    }];
    model.auth.action_cursor = 1;
    model.auth.selected = 0;
    model.config.provider = PROVIDER_ID_OPENAI.to_string();
    model.config.chatgpt_auth_available = false;

    model.run_auth_tab_action();

    assert!(matches!(
        daemon_rx
            .try_recv()
            .expect("expected daemon auth status command"),
        DaemonCommand::LoginOpenAICodex
    ));
    assert!(daemon_rx.try_recv().is_err());
}

#[test]
fn api_transport_cycles_for_github_copilot() {
    let (mut model, _daemon_rx) = make_model();
    model.apply_provider_selection(PROVIDER_ID_GITHUB_COPILOT);
    model
        .settings
        .reduce(SettingsAction::SwitchTab(SettingsTab::Provider));
    model.settings.reduce(SettingsAction::NavigateField(4));
    assert_eq!(model.settings.current_field_name(), "api_transport");

    model.activate_settings_field();

    assert_eq!(model.config.api_transport, "chat_completions");

    model.activate_settings_field();

    assert_eq!(model.config.api_transport, "responses");
}

#[test]
fn github_copilot_preserves_responses_transport_when_loaded_from_saved_config() {
    let (mut model, _daemon_rx) = make_model();
    model.apply_config_json(&serde_json::json!({
        "provider": PROVIDER_ID_GITHUB_COPILOT,
        "providers": {
            "github-copilot": {
                "base_url": "https://api.githubcopilot.com",
                "model": "gpt-5.4",
                "api_transport": "responses",
                "auth_source": "github_copilot"
            }
        }
    }));

    model.apply_provider_selection(PROVIDER_ID_GITHUB_COPILOT);

    assert_eq!(model.config.provider, PROVIDER_ID_GITHUB_COPILOT);
    assert_eq!(model.config.api_transport, "responses");
}

#[test]
fn commit_subagent_editor_persists_existing_provider_model_and_effort_changes() {
    let (mut model, mut daemon_rx) = make_model();
    model.subagents.entries = vec![crate::state::SubAgentEntry {
        id: "weles_builtin".to_string(),
        name: "WELES".to_string(),
        provider: PROVIDER_ID_OPENAI.to_string(),
        model: "gpt-5.4-mini".to_string(),
        role: Some("code_review".to_string()),
        enabled: true,
        builtin: true,
        immutable_identity: true,
        disable_allowed: false,
        delete_allowed: false,
        protected_reason: Some("Protected builtin".to_string()),
        reasoning_effort: Some("medium".to_string()),
        raw_json: Some(serde_json::json!({
            "id": "weles_builtin",
            "name": "WELES",
            "provider": PROVIDER_ID_OPENAI,
            "model": "gpt-5.4-mini",
            "role": "code_review",
            "enabled": true,
            "builtin": true,
            "immutable_identity": true,
            "disable_allowed": false,
            "delete_allowed": false,
            "protected_reason": "Protected builtin",
            "reasoning_effort": "medium",
            "created_at": 1
        })),
    }];

    let mut editor = crate::state::subagents::SubAgentEditorState::new(
        Some("weles_builtin".to_string()),
        1,
        PROVIDER_ID_ANTHROPIC.to_string(),
        "claude-sonnet-4-5".to_string(),
    );
    editor.name = "WELES".to_string();
    editor.role = "code_review".to_string();
    editor.enabled = true;
    editor.builtin = true;
    editor.immutable_identity = true;
    editor.disable_allowed = false;
    editor.delete_allowed = false;
    editor.protected_reason = Some("Protected builtin".to_string());
    editor.reasoning_effort = Some("high".to_string());
    editor.raw_json = Some(serde_json::json!({
        "id": "weles_builtin",
        "name": "WELES",
        "provider": PROVIDER_ID_OPENAI,
        "model": "gpt-5.4-mini",
        "role": "code_review",
        "enabled": true,
        "builtin": true,
        "immutable_identity": true,
        "disable_allowed": false,
        "delete_allowed": false,
        "protected_reason": "Protected builtin",
        "reasoning_effort": "medium",
        "created_at": 1
    }));
    model.subagents.editor = Some(editor);

    model.commit_subagent_editor();

    let command = daemon_rx
        .try_recv()
        .expect("expected sub-agent update command");
    let DaemonCommand::SetSubAgent(payload) = command else {
        panic!("expected SetSubAgent command");
    };
    let saved: serde_json::Value =
        serde_json::from_str(&payload).expect("payload should be valid json");
    assert_eq!(
        saved.get("provider").and_then(|value| value.as_str()),
        Some(PROVIDER_ID_ANTHROPIC)
    );
    assert_eq!(
        saved.get("model").and_then(|value| value.as_str()),
        Some("claude-sonnet-4-5")
    );
    assert_eq!(
        saved
            .get("reasoning_effort")
            .and_then(|value| value.as_str()),
        Some("high")
    );

    let entry = model
        .subagents
        .entries
        .iter()
        .find(|entry| entry.id == "weles_builtin")
        .expect("updated entry should remain present");
    assert_eq!(entry.provider, "anthropic");
    assert_eq!(entry.model, "claude-sonnet-4-5");
    assert_eq!(entry.reasoning_effort.as_deref(), Some("high"));
}

fn focus_settings_field(model: &mut TuiModel, tab: SettingsTab, field_name: &str) {
    model.settings.reduce(SettingsAction::SwitchTab(tab));
    let count = model.settings.field_count_with_config(&model.config);
    for _ in 0..count {
        if model.settings.current_field_name_with_config(&model.config) == field_name {
            return;
        }
        model.settings.reduce(SettingsAction::NavigateField(1));
    }
    panic!("field {field_name} not found in {:?}", tab);
}

fn raw_json_string(value: &serde_json::Value, path: &[&str]) -> Option<String> {
    let mut current = value;
    for part in path {
        current = current.get(*part)?;
    }
    current.as_str().map(str::to_string)
}

fn raw_json_bool(value: &serde_json::Value, path: &[&str]) -> Option<bool> {
    let mut current = value;
    for part in path {
        current = current.get(*part)?;
    }
    current.as_bool()
}

fn raw_json_u64(value: &serde_json::Value, path: &[&str]) -> Option<u64> {
    let mut current = value;
    for part in path {
        current = current.get(*part)?;
    }
    current.as_u64()
}

#[test]
fn feature_toggle_fields_emit_expected_config_updates() {
    let cases = [
        (
            "feat_tier_override",
            "/tier/user_override",
            "\"familiar\"",
            Some((vec!["tier", "user_override"], "familiar")),
            None,
        ),
        (
            "feat_security_level",
            "/managed_security_level",
            "\"strict\"",
            Some((vec!["managed_security_level"], "strict")),
            None,
        ),
        (
            "feat_check_stale_todos",
            "/heartbeat/check_stale_todos",
            "false",
            None,
            Some((vec!["heartbeat", "check_stale_todos"], false)),
        ),
        (
            "feat_check_stuck_goals",
            "/heartbeat/check_stuck_goals",
            "false",
            None,
            Some((vec!["heartbeat", "check_stuck_goals"], false)),
        ),
        (
            "feat_check_unreplied_messages",
            "/heartbeat/check_unreplied_messages",
            "false",
            None,
            Some((vec!["heartbeat", "check_unreplied_messages"], false)),
        ),
        (
            "feat_check_repo_changes",
            "/heartbeat/check_repo_changes",
            "false",
            None,
            Some((vec!["heartbeat", "check_repo_changes"], false)),
        ),
        (
            "feat_consolidation_enabled",
            "/consolidation/enabled",
            "false",
            None,
            Some((vec!["consolidation", "enabled"], false)),
        ),
        (
            "feat_skill_discovery_enabled",
            "/skill_discovery/enabled",
            "false",
            None,
            Some((vec!["skill_discovery", "enabled"], false)),
        ),
    ];

    for (field, expected_key_path, expected_value_json, expected_string, expected_bool) in cases {
        let (mut model, mut daemon_rx) = make_model();
        model.config.agent_config_raw = Some(serde_json::json!({}));
        model
            .modal
            .reduce(modal::ModalAction::Push(modal::ModalKind::Settings));
        focus_settings_field(&mut model, SettingsTab::Features, field);

        let quit = model.handle_key_modal(
            KeyCode::Char(' '),
            KeyModifiers::NONE,
            modal::ModalKind::Settings,
        );
        assert!(!quit, "settings modal should remain open for {field}");

        match daemon_rx.try_recv() {
            Ok(DaemonCommand::SetConfigItem {
                key_path,
                value_json,
            }) => {
                assert_eq!(key_path, expected_key_path, "wrong key path for {field}");
                assert_eq!(
                    value_json, expected_value_json,
                    "wrong serialized value for {field}"
                );
            }
            other => panic!("expected SetConfigItem for {field}, got {other:?}"),
        }

        let raw = model
            .config
            .agent_config_raw
            .as_ref()
            .expect("feature toggles should keep raw config");
        if let Some((path, expected)) = expected_string {
            assert_eq!(raw_json_string(raw, &path), Some(expected.to_string()));
        }
        if let Some((path, expected)) = expected_bool {
            assert_eq!(raw_json_bool(raw, &path), Some(expected));
        }
    }
}

#[test]
fn feature_edit_fields_start_with_saved_values_and_submit_expected_updates() {
    let cases = [
        (
            "feat_heartbeat_cron",
            serde_json::json!({"heartbeat": {"cron": "*/30 * * * *"}}),
            "*/30 * * * *",
            "/heartbeat/cron",
            "\"*/30 * * * *\"",
        ),
        (
            "feat_heartbeat_quiet_start",
            serde_json::json!({"heartbeat": {"quiet_start": "21:30"}}),
            "21:30",
            "/heartbeat/quiet_start",
            "\"21:30\"",
        ),
        (
            "feat_heartbeat_quiet_end",
            serde_json::json!({"heartbeat": {"quiet_end": "06:30"}}),
            "06:30",
            "/heartbeat/quiet_end",
            "\"06:30\"",
        ),
        (
            "feat_decay_half_life_hours",
            serde_json::json!({"consolidation": {"decay_half_life_hours": 72.0}}),
            "72",
            "/consolidation/decay_half_life_hours",
            "72",
        ),
        (
            "feat_heuristic_promotion_threshold",
            serde_json::json!({"consolidation": {"heuristic_promotion_threshold": 9}}),
            "9",
            "/consolidation/heuristic_promotion_threshold",
            "9",
        ),
        (
            "feat_skill_promotion_threshold",
            serde_json::json!({"skill_discovery": {"promotion_threshold": 4}}),
            "4",
            "/skill_discovery/promotion_threshold",
            "4",
        ),
    ];

    for (field, raw, expected_buffer, expected_key_path, expected_value_json) in cases {
        let (mut model, mut daemon_rx) = make_model();
        model.config.agent_config_raw = Some(raw);
        model
            .modal
            .reduce(modal::ModalAction::Push(modal::ModalKind::Settings));
        focus_settings_field(&mut model, SettingsTab::Features, field);

        let quit = model.handle_key_modal(
            KeyCode::Enter,
            KeyModifiers::NONE,
            modal::ModalKind::Settings,
        );
        assert!(!quit, "settings modal should stay open while starting edit for {field}");
        assert_eq!(model.settings.editing_field(), Some(field));
        assert_eq!(model.settings.edit_buffer(), expected_buffer);

        let quit = model.handle_key_modal(
            KeyCode::Enter,
            KeyModifiers::NONE,
            modal::ModalKind::Settings,
        );
        assert!(!quit, "settings modal should stay open while committing edit for {field}");
        assert_eq!(model.settings.editing_field(), None);

        match daemon_rx.try_recv() {
            Ok(DaemonCommand::SetConfigItem {
                key_path,
                value_json,
            }) => {
                assert_eq!(key_path, expected_key_path, "wrong key path for {field}");
                assert_eq!(
                    value_json, expected_value_json,
                    "wrong serialized value for {field}"
                );
            }
            other => panic!("expected SetConfigItem for {field}, got {other:?}"),
        }
    }
}

#[test]
fn concierge_config_serializes_all_fields() {
    let (mut model, mut daemon_rx) = make_model();
    model.concierge.enabled = true;
    model.concierge.detail_level = "daily_briefing".to_string();
    model.concierge.provider = Some("anthropic".to_string());
    model.concierge.model = Some("claude-sonnet-4-5".to_string());
    model.concierge.reasoning_effort = Some("high".to_string());
    model.concierge.auto_cleanup_on_navigate = false;

    model.send_concierge_config();

    let payload = match daemon_rx.try_recv() {
        Ok(DaemonCommand::SetConciergeConfig(payload)) => payload,
        other => panic!("expected SetConciergeConfig, got {other:?}"),
    };
    let json: serde_json::Value = serde_json::from_str(&payload).expect("valid concierge json");
    assert_eq!(json["enabled"], true);
    assert_eq!(json["detail_level"], "daily_briefing");
    assert_eq!(json["provider"], "anthropic");
    assert_eq!(json["model"], "claude-sonnet-4-5");
    assert_eq!(json["reasoning_effort"], "high");
    assert_eq!(json["auto_cleanup_on_navigate"], false);
}

#[test]
fn concierge_settings_fields_dispatch_expected_actions() {
    let (mut model, mut daemon_rx) = make_model();
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::Settings));

    focus_settings_field(&mut model, SettingsTab::Concierge, "concierge_enabled");
    let quit = model.handle_key_modal(
        KeyCode::Enter,
        KeyModifiers::NONE,
        modal::ModalKind::Settings,
    );
    assert!(!quit);
    match daemon_rx.try_recv() {
        Ok(DaemonCommand::SetConciergeConfig(payload)) => {
            let json: serde_json::Value = serde_json::from_str(&payload).expect("json");
            assert_eq!(json["enabled"], false);
        }
        other => panic!("expected concierge config update, got {other:?}"),
    }

    focus_settings_field(&mut model, SettingsTab::Concierge, "concierge_detail_level");
    let quit = model.handle_key_modal(
        KeyCode::Enter,
        KeyModifiers::NONE,
        modal::ModalKind::Settings,
    );
    assert!(!quit);
    match daemon_rx.try_recv() {
        Ok(DaemonCommand::SetConciergeConfig(payload)) => {
            let json: serde_json::Value = serde_json::from_str(&payload).expect("json");
            assert_eq!(json["detail_level"], "daily_briefing");
        }
        other => panic!("expected concierge detail update, got {other:?}"),
    }

    focus_settings_field(&mut model, SettingsTab::Concierge, "concierge_provider");
    let quit = model.handle_key_modal(
        KeyCode::Enter,
        KeyModifiers::NONE,
        modal::ModalKind::Settings,
    );
    assert!(!quit);
    assert_eq!(model.modal.top(), Some(modal::ModalKind::ProviderPicker));
    model.close_top_modal();

    focus_settings_field(&mut model, SettingsTab::Concierge, "concierge_model");
    let quit = model.handle_key_modal(
        KeyCode::Enter,
        KeyModifiers::NONE,
        modal::ModalKind::Settings,
    );
    assert!(!quit);
    assert_eq!(model.modal.top(), Some(modal::ModalKind::ModelPicker));
    model.close_top_modal();

    focus_settings_field(&mut model, SettingsTab::Concierge, "concierge_reasoning_effort");
    let quit = model.handle_key_modal(
        KeyCode::Enter,
        KeyModifiers::NONE,
        modal::ModalKind::Settings,
    );
    assert!(!quit);
    assert_eq!(model.modal.top(), Some(modal::ModalKind::EffortPicker));
}
