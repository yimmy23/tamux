use amux_shared::providers::{PROVIDER_ID_CUSTOM, PROVIDER_ID_OPENAI};

#[test]
fn settings_handles_empty_state() {
    let settings = SettingsState::new();
    let config = ConfigState::new();
    let _theme = ThemeTokens::default();
    assert_eq!(settings.active_tab(), SettingsTab::Auth);
    assert_eq!(config.model(), "gpt-5.4");
}

#[test]
fn settings_api_key_is_masked() {
    let masked = mask_api_key("sk-abcdefgh12345678abcd");
    assert!(!masked.contains("abcdefgh"));
    assert!(masked.contains("\u{2022}"));
}

#[test]
fn mask_api_key_short_returns_dots() {
    assert_eq!(
        mask_api_key("short"),
        "\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}"
    );
}

#[test]
fn mask_api_key_empty_returns_not_set() {
    assert_eq!(mask_api_key(""), "(not set)");
}

#[test]
fn tab_hit_test_uses_rendered_label_positions() {
    let area = Rect::new(10, 3, 80, 1);
    let visible = visible_tabs(area, active_tab_index(SettingsTab::Concierge));
    assert!(visible.iter().any(|tab| tab.tab == SettingsTab::Concierge));
    for tab in visible {
        assert_eq!(
            tab_hit_test(area, SettingsTab::Concierge, tab.start_x),
            Some(tab.tab)
        );
    }
}

#[test]
fn gateway_tab_mentions_whatsapp_qr_linking_instructions() {
    let mut settings = SettingsState::new();
    settings.reduce(crate::state::settings::SettingsAction::SwitchTab(
        SettingsTab::Gateway,
    ));
    let config = ConfigState::new();
    let modal = ModalState::new();
    let lines = render_gateway_tab(&settings, &config, &modal, &ThemeTokens::default());
    let text = lines
        .iter()
        .map(|line| line.to_string())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        text.contains("Allowed Contacts accepts comma or newline separated phone numbers."),
        "Gateway tab should explain how to enter the WhatsApp allowlist"
    );
}

#[test]
fn gateway_tab_shows_allowlist_requirement_before_linking() {
    let mut settings = SettingsState::new();
    settings.reduce(crate::state::settings::SettingsAction::SwitchTab(
        SettingsTab::Gateway,
    ));
    settings.reduce(crate::state::settings::SettingsAction::NavigateField(12));
    let config = ConfigState::new();
    let modal = ModalState::new();

    let lines = render_gateway_tab(&settings, &config, &modal, &ThemeTokens::default());
    let text = lines
        .iter()
        .map(|line| line.to_string())
        .collect::<Vec<_>>()
        .join("\n");

    assert!(text.contains("> Link Device  [Enter]  (requires allowed contacts)"));
    assert!(text.contains("Add at least one allowed phone number before QR linking."));
}

#[test]
fn settings_tab_bar_uses_swarog_and_rarog_labels() {
    let area = Rect::new(0, 0, 120, 1);
    let mut settings = SettingsState::new();
    settings.reduce(crate::state::settings::SettingsAction::SwitchTab(
        SettingsTab::Concierge,
    ));
    let tabs = visible_tabs(area, active_tab_index(SettingsTab::Concierge));
    let line = render_tabs_line(&tabs, &settings, &ThemeTokens::default());
    let text = line.to_string();

    assert!(text.contains("Svar"));
    assert!(text.contains("Rar"));
    assert!(!text.contains("Con"));
    assert!(!text.contains("Prov"));
    let auth_idx = text.find("Auth").expect("auth tab should be visible");
    let swar_idx = text.find("Svar").expect("svarog tab should be visible");
    let rar_idx = text.find("Rar").expect("rarog tab should be visible");
    assert!(auth_idx < swar_idx && swar_idx < rar_idx);
}

#[test]
fn settings_tab_bar_includes_about_label() {
    let area = Rect::new(0, 0, 140, 1);
    let mut settings = SettingsState::new();
    settings.reduce(crate::state::settings::SettingsAction::SwitchTab(
        SettingsTab::About,
    ));

    let tabs = visible_tabs(area, active_tab_index(SettingsTab::About));
    let line = render_tabs_line(&tabs, &settings, &ThemeTokens::default());
    let text = line.to_string();

    assert!(text.contains("About"));
}

#[test]
fn chat_tab_keeps_honcho_as_single_row_in_main_list() {
    let mut settings = SettingsState::new();
    settings.reduce(crate::state::settings::SettingsAction::SwitchTab(
        SettingsTab::Chat,
    ));
    settings.reduce(crate::state::settings::SettingsAction::NavigateField(2));
    let config = ConfigState::new();
    let modal = ModalState::new();
    let auth = crate::state::auth::AuthState::new();
    let subagents = SubAgentsState::new();
    let concierge = crate::state::concierge::ConciergeState::new();
    let tier = crate::state::tier::TierState::from_tier("power_user");
    let plugin_settings = crate::state::settings::PluginSettingsState::new();

    let lines = render_tab_content(
        100,
        &settings,
        &config,
        &modal,
        &auth,
        &subagents,
        &concierge,
        &tier,
        &plugin_settings,
        &ThemeTokens::default(),
    );
    let text = lines
        .iter()
        .map(|line| line.to_string())
        .collect::<Vec<_>>()
        .join("\n");

    assert!(text.contains("Honcho Memory"));
    assert!(!text.contains("Honcho API Key"));
    assert!(!text.contains("Honcho Base URL"));
    assert!(!text.contains("Honcho Workspace"));
}

#[test]
fn chat_tab_renders_inline_honcho_editor_when_active() {
    let mut settings = SettingsState::new();
    settings.reduce(crate::state::settings::SettingsAction::SwitchTab(
        SettingsTab::Chat,
    ));
    let mut config = ConfigState::new();
    config.honcho_editor = Some(crate::state::config::HonchoEditorState {
        enabled: true,
        api_key: "hc_test".to_string(),
        base_url: "https://honcho.example".to_string(),
        workspace_id: "tamux-lab".to_string(),
        field: crate::state::config::HonchoEditorField::ApiKey,
    });
    let modal = ModalState::new();
    let auth = crate::state::auth::AuthState::new();
    let subagents = SubAgentsState::new();
    let concierge = crate::state::concierge::ConciergeState::new();
    let tier = crate::state::tier::TierState::from_tier("power_user");
    let plugin_settings = crate::state::settings::PluginSettingsState::new();

    let lines = render_tab_content(
        100,
        &settings,
        &config,
        &modal,
        &auth,
        &subagents,
        &concierge,
        &tier,
        &plugin_settings,
        &ThemeTokens::default(),
    );
    let text = lines
        .iter()
        .map(|line| line.to_string())
        .collect::<Vec<_>>()
        .join("\n");

    assert!(text.contains("Honcho Memory Settings"));
    assert!(text.contains("API Key"));
    assert!(text.contains("Base URL"));
    assert!(text.contains("Workspace"));
    assert!(text.contains("[Save]"));
    assert!(text.contains("[Cancel]"));
}

#[test]
fn chat_tab_places_honcho_editor_below_last_checkbox() {
    let mut settings = SettingsState::new();
    settings.reduce(crate::state::settings::SettingsAction::SwitchTab(
        SettingsTab::Chat,
    ));
    let mut config = ConfigState::new();
    config.honcho_editor = Some(crate::state::config::HonchoEditorState {
        enabled: false,
        api_key: String::new(),
        base_url: String::new(),
        workspace_id: "tamux".to_string(),
        field: crate::state::config::HonchoEditorField::Enabled,
    });
    let modal = ModalState::new();
    let auth = crate::state::auth::AuthState::new();
    let subagents = SubAgentsState::new();
    let concierge = crate::state::concierge::ConciergeState::new();
    let tier = crate::state::tier::TierState::from_tier("power_user");
    let plugin_settings = crate::state::settings::PluginSettingsState::new();

    let lines = render_tab_content(
        100,
        &settings,
        &config,
        &modal,
        &auth,
        &subagents,
        &concierge,
        &tier,
        &plugin_settings,
        &ThemeTokens::default(),
    );
    let text = lines
        .iter()
        .map(|line| line.to_string())
        .collect::<Vec<_>>()
        .join("\n");

    let require_activation = text
        .find("Require Activation")
        .expect("require activation row should render");
    let honcho_editor = text
        .find("Honcho Memory Settings")
        .expect("honcho editor heading should render");
    let tool_limit = text.find("Tool Limit:").expect("tool limit row should render");

    assert!(require_activation < honcho_editor);
    assert!(honcho_editor < tool_limit);
}

#[test]
fn about_tab_renders_product_metadata() {
    let mut settings = SettingsState::new();
    settings.reduce(crate::state::settings::SettingsAction::SwitchTab(
        SettingsTab::About,
    ));
    let config = ConfigState::new();
    let modal = ModalState::new();
    let auth = crate::state::auth::AuthState::new();
    let subagents = SubAgentsState::new();
    let concierge = crate::state::concierge::ConciergeState::new();
    let tier = crate::state::tier::TierState::from_tier("power_user");
    let plugin_settings = crate::state::settings::PluginSettingsState::new();

    let lines = render_tab_content(
        100,
        &settings,
        &config,
        &modal,
        &auth,
        &subagents,
        &concierge,
        &tier,
        &plugin_settings,
        &ThemeTokens::default(),
    );
    let text = lines
        .iter()
        .map(|line| line.to_string())
        .collect::<Vec<_>>()
        .join("\n");

    assert!(text.contains("Version:"));
    assert!(text.contains("Mariusz Kurman"));
    assert!(text.contains("mkurman/tamux"));
    assert!(text.contains("tamux.app"));
}

#[test]
fn gateway_tab_contains_selectable_link_device_row() {
    let mut settings = SettingsState::new();
    settings.reduce(crate::state::settings::SettingsAction::SwitchTab(
        SettingsTab::Gateway,
    ));
    settings.reduce(crate::state::settings::SettingsAction::NavigateField(12));
    let config = ConfigState::new();
    let modal = ModalState::new();
    let lines = render_gateway_tab(&settings, &config, &modal, &ThemeTokens::default());
    let text = lines
        .iter()
        .map(|line| line.to_string())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(text.contains("> Link Device"));
}

#[test]
fn advanced_tab_renders_sleep_delay_rows() {
    let mut settings = SettingsState::new();
    settings.reduce(crate::state::settings::SettingsAction::SwitchTab(
        SettingsTab::Advanced,
    ));
    settings.reduce(crate::state::settings::SettingsAction::NavigateField(7));
    let config = ConfigState::new();
    let modal = ModalState::new();
    let auth = crate::state::auth::AuthState::new();
    let subagents = SubAgentsState::new();
    let concierge = crate::state::concierge::ConciergeState::new();
    let tier = crate::state::tier::TierState::from_tier("power_user");
    let plugin_settings = crate::state::settings::PluginSettingsState::new();

    let lines = render_tab_content(
        100,
        &settings,
        &config,
        &modal,
        &auth,
        &subagents,
        &concierge,
        &tier,
        &plugin_settings,
        &ThemeTokens::default(),
    );
    let text = lines
        .iter()
        .map(|line| line.to_string())
        .collect::<Vec<_>>()
        .join("\n");

    assert!(text.contains("Message Loop (ms):"));
    assert!(text.contains("Tool Call Gap (ms):"));
    assert!(text.contains("LLM Stream Timeout (s):"));
}

#[test]
fn auth_tab_shows_chatgpt_logout_when_daemon_auth_is_available() {
    let settings = SettingsState::new();
    let mut config = ConfigState::new();
    config.chatgpt_auth_available = true;
    let modal = ModalState::new();
    let mut auth = crate::state::auth::AuthState::new();
    auth.loaded = true;
    auth.entries = vec![crate::state::auth::ProviderAuthEntry {
        provider_id: PROVIDER_ID_OPENAI.to_string(),
        provider_name: "OpenAI".to_string(),
        authenticated: false,
        auth_source: "api_key".to_string(),
        model: "gpt-5.4".to_string(),
    }];
    let subagents = SubAgentsState::new();
    let concierge = crate::state::concierge::ConciergeState::new();
    let tier = crate::state::tier::TierState::from_tier("power_user");
    let plugin_settings = crate::state::settings::PluginSettingsState::new();

    let lines = render_tab_content(
        100,
        &settings,
        &config,
        &modal,
        &auth,
        &subagents,
        &concierge,
        &tier,
        &plugin_settings,
        &ThemeTokens::default(),
    );
    let text = lines
        .iter()
        .map(|line| line.to_string())
        .collect::<Vec<_>>()
        .join("\n");

    assert!(
        text.contains("[API Key] [Logout]"),
        "expected ChatGPT action to switch to logout, got: {text}"
    );
}

#[test]
fn auth_tab_shows_provider_test_result_inline() {
    let settings = SettingsState::new();
    let config = ConfigState::new();
    let modal = ModalState::new();
    let mut auth = crate::state::auth::AuthState::new();
    auth.loaded = true;
    auth.entries = vec![crate::state::auth::ProviderAuthEntry {
        provider_id: PROVIDER_ID_OPENAI.to_string(),
        provider_name: "OpenAI".to_string(),
        authenticated: true,
        auth_source: "api_key".to_string(),
        model: "gpt-5.4".to_string(),
    }];
    auth.reduce(crate::state::auth::AuthAction::ValidationResult {
        provider_id: PROVIDER_ID_OPENAI.to_string(),
        valid: true,
        error: None,
    });
    let subagents = SubAgentsState::new();
    let concierge = crate::state::concierge::ConciergeState::new();
    let tier = crate::state::tier::TierState::from_tier("power_user");
    let plugin_settings = crate::state::settings::PluginSettingsState::new();

    let lines = render_tab_content(
        100,
        &settings,
        &config,
        &modal,
        &auth,
        &subagents,
        &concierge,
        &tier,
        &plugin_settings,
        &ThemeTokens::default(),
    );
    let text = lines
        .iter()
        .map(|line| line.to_string())
        .collect::<Vec<_>>()
        .join("\n");

    assert!(
        text.contains("Connection OK"),
        "expected inline test result in auth tab, got: {text}"
    );
}

#[test]
fn gateway_tab_shows_connected_whatsapp_status_and_split_actions() {
    let mut settings = SettingsState::new();
    settings.reduce(crate::state::settings::SettingsAction::SwitchTab(
        SettingsTab::Gateway,
    ));
    settings.reduce(crate::state::settings::SettingsAction::NavigateField(12));
    let mut config = ConfigState::new();
    config.whatsapp_allowed_contacts = "+48663977535".to_string();
    let mut modal = ModalState::new();
    modal.set_whatsapp_link_connected(Some("+48663977535".to_string()));

    let lines = render_gateway_tab(&settings, &config, &modal, &ThemeTokens::default());
    let text = lines
        .iter()
        .map(|line| line.to_string())
        .collect::<Vec<_>>()
        .join("\n");

    assert!(text.contains("> Link Status"));
    assert!(text.contains("Re-link Device"));
    assert!(text.contains("Linked: +48663977535"));
    assert!(text.contains("Only allowed numbers will be forwarded and can receive replies."));
}

#[test]
fn gateway_tab_renders_whatsapp_allowlist_as_textarea_when_editing() {
    let mut settings = SettingsState::new();
    settings.start_editing("whatsapp_allowed_contacts", "+15551234567\n+15557654321");
    let mut config = ConfigState::new();
    config.whatsapp_allowed_contacts = "+15551234567\n+15557654321".to_string();
    let modal = ModalState::new();

    let lines = render_gateway_tab(&settings, &config, &modal, &ThemeTokens::default());
    let text = lines
        .iter()
        .map(|line| line.to_string())
        .collect::<Vec<_>>()
        .join("\n");

    assert!(text.contains("Allowed Contacts [Ctrl+S/Ctrl+Enter: save, Esc: cancel]"));
    assert!(text.contains("+15551234567"));
    assert!(text.contains("+15557654321"));
    assert!(text.contains("╭"));
    assert!(text.contains("╰"));
}

#[test]
fn custom_provider_model_field_invites_inline_edit() {
    let mut settings = SettingsState::new();
    settings.reduce(crate::state::settings::SettingsAction::SwitchTab(
        SettingsTab::Provider,
    ));
    settings.reduce(crate::state::settings::SettingsAction::NavigateField(3));
    let mut config = ConfigState::new();
    config.provider = PROVIDER_ID_CUSTOM.to_string();
    config.model = "my-model".to_string();

    let lines = render_provider_tab(&settings, &config, &ThemeTokens::default());
    let text = lines
        .iter()
        .map(|line| line.to_string())
        .collect::<Vec<_>>()
        .join("\n");

    assert!(text.contains("> Model           my-model [Enter: edit]"));
    assert!(!text.contains("> Model           my-model [Enter: pick]"));
}

#[test]
fn custom_provider_model_row_shows_active_edit_buffer() {
    let mut settings = SettingsState::new();
    settings.reduce(crate::state::settings::SettingsAction::SwitchTab(
        SettingsTab::Provider,
    ));
    settings.reduce(crate::state::settings::SettingsAction::NavigateField(3));
    settings.start_editing("custom_model_entry", "my-model");
    let mut config = ConfigState::new();
    config.provider = PROVIDER_ID_CUSTOM.to_string();
    config.model = "my-model".to_string();

    let lines = render_provider_tab(&settings, &config, &ThemeTokens::default());
    let text = lines
        .iter()
        .map(|line| line.to_string())
        .collect::<Vec<_>>()
        .join("\n");

    assert!(text.contains("> Model           my-model█"));
    assert!(!text.contains("> Provider        custom█"));
}

#[test]
fn custom_model_context_row_invites_edit_for_non_custom_provider() {
    let mut settings = SettingsState::new();
    settings.reduce(crate::state::settings::SettingsAction::SwitchTab(
        SettingsTab::Provider,
    ));
    settings.reduce(crate::state::settings::SettingsAction::NavigateField(7));
    let mut config = ConfigState::new();
    config.provider = "openrouter".to_string();
    config.auth_source = "api_key".to_string();
    config.model = "openrouter/custom-preview".to_string();
    config.custom_model_name = "Custom Preview".to_string();
    config.context_window_tokens = 333_000;
    config.custom_context_window_tokens = Some(333_000);

    let lines = render_provider_tab(&settings, &config, &ThemeTokens::default());
    let text = lines
        .iter()
        .map(|line| line.to_string())
        .collect::<Vec<_>>()
        .join("\n");

    assert!(text.contains("> Ctx Length      333000 tok [Enter: edit]"));
    assert!(!text.contains("[derived]"));
}

#[test]
fn subagent_editor_shows_live_name_edit_buffer() {
    let mut settings = SettingsState::new();
    settings.reduce(crate::state::settings::SettingsAction::SwitchTab(
        SettingsTab::SubAgents,
    ));
    settings.start_editing("subagent_name", "Draft Name");
    let config = ConfigState::new();
    let modal = ModalState::new();
    let auth = crate::state::auth::AuthState::new();
    let mut subagents = SubAgentsState::new();
    let mut editor = crate::state::subagents::SubAgentEditorState::new(
        None,
        0,
        PROVIDER_ID_OPENAI.to_string(),
        "gpt-5.4".to_string(),
    );
    editor.name = "Old Name".to_string();
    editor.field = crate::state::subagents::SubAgentEditorField::Name;
    subagents.editor = Some(editor);
    let concierge = crate::state::concierge::ConciergeState::new();
    let tier = crate::state::tier::TierState::from_tier("power_user");
    let plugin_settings = crate::state::settings::PluginSettingsState::new();

    let lines = render_tab_content(
        80,
        &settings,
        &config,
        &modal,
        &auth,
        &subagents,
        &concierge,
        &tier,
        &plugin_settings,
        &ThemeTokens::default(),
    );
    let text = lines
        .iter()
        .map(|line| line.to_string())
        .collect::<Vec<_>>()
        .join("\n");

    assert!(text.contains("Draft Name█"));
    assert!(!text.contains("Old Name"));
}

#[test]
fn subagent_system_prompt_textarea_wraps_long_lines_to_content_width() {
    let mut settings = SettingsState::new();
    settings.reduce(crate::state::settings::SettingsAction::SwitchTab(
        SettingsTab::SubAgents,
    ));
    settings.start_editing(
        "subagent_system_prompt",
        "You are tamux, an always-on agentic terminal multiplexer assistant. You can execute terminal commands and coordinate subagents carefully.",
    );
    let config = ConfigState::new();
    let modal = ModalState::new();
    let auth = crate::state::auth::AuthState::new();
    let mut subagents = SubAgentsState::new();
    let mut editor = crate::state::subagents::SubAgentEditorState::new(
        None,
        0,
        PROVIDER_ID_OPENAI.to_string(),
        "gpt-5.4".to_string(),
    );
    editor.field = crate::state::subagents::SubAgentEditorField::SystemPrompt;
    subagents.editor = Some(editor);
    let concierge = crate::state::concierge::ConciergeState::new();
    let tier = crate::state::tier::TierState::from_tier("power_user");
    let plugin_settings = crate::state::settings::PluginSettingsState::new();

    let lines = render_tab_content(
        60,
        &settings,
        &config,
        &modal,
        &auth,
        &subagents,
        &concierge,
        &tier,
        &plugin_settings,
        &ThemeTokens::default(),
    );
    let rendered_lines = lines
        .iter()
        .map(|line| line.to_string())
        .collect::<Vec<_>>();
    let prompt_lines = rendered_lines
        .iter()
        .filter(|line| line.starts_with("  │ "))
        .collect::<Vec<_>>();

    assert!(
        prompt_lines.len() >= 2,
        "expected wrapped textarea content, got: {:?}",
        prompt_lines
    );
    assert!(
        prompt_lines
            .iter()
            .all(|line| unicode_width::UnicodeWidthStr::width(line.as_str()) <= 60),
        "wrapped textarea should stay within content width, got: {:?}",
        prompt_lines
    );
}

#[test]
fn provider_tab_mentions_swarog() {
    let mut settings = SettingsState::new();
    settings.reduce(crate::state::settings::SettingsAction::SwitchTab(
        SettingsTab::Provider,
    ));
    let config = ConfigState::new();

    let lines = render_provider_tab(&settings, &config, &ThemeTokens::default());
    let text = lines
        .iter()
        .map(|line| line.to_string())
        .collect::<Vec<_>>()
        .join("\n");

    assert!(text.contains("Svarog"));
}

#[test]
fn agent_tab_uses_swarog_label() {
    let mut settings = SettingsState::new();
    settings.reduce(crate::state::settings::SettingsAction::SwitchTab(
        SettingsTab::Agent,
    ));
    let config = ConfigState::new();

    let lines = render_agent_tab(&settings, &config, &ThemeTokens::default());
    let text = lines
        .iter()
        .map(|line| line.to_string())
        .collect::<Vec<_>>()
        .join("\n");

    assert!(text.contains("Svarog"));
}

#[test]
fn agent_tab_includes_provider_controls() {
    let mut settings = SettingsState::new();
    settings.reduce(crate::state::settings::SettingsAction::SwitchTab(
        SettingsTab::Agent,
    ));
    let config = ConfigState::new();

    let lines = render_agent_tab(&settings, &config, &ThemeTokens::default());
    let text = lines
        .iter()
        .map(|line| line.to_string())
        .collect::<Vec<_>>()
        .join("\n");

    assert!(text.contains("Svarog Provider"));
    assert!(text.contains("Provider"));
    assert!(text.contains("System Prompt"));
}

#[test]
fn agent_tab_shows_fixed_swarog_name() {
    let mut settings = SettingsState::new();
    settings.reduce(crate::state::settings::SettingsAction::SwitchTab(
        SettingsTab::Agent,
    ));
    let config = ConfigState::new();

    let lines = render_agent_tab(&settings, &config, &ThemeTokens::default());
    let text = lines
        .iter()
        .map(|line| line.to_string())
        .collect::<Vec<_>>()
        .join("\n");

    assert!(text.contains("Svarog"));
    assert!(!text.contains("Agent Name"));
    assert!(!text.contains("Svarog Name [Enter: edit]"));
}

#[test]
fn rarog_tab_uses_rarog_label() {
    let mut settings = SettingsState::new();
    settings.reduce(crate::state::settings::SettingsAction::SwitchTab(
        SettingsTab::Concierge,
    ));
    let concierge = crate::state::concierge::ConciergeState::new();

    let lines = render_concierge_tab(&settings, &concierge, &ThemeTokens::default());
    let text = lines
        .iter()
        .map(|line| line.to_string())
        .collect::<Vec<_>>()
        .join("\n");

    assert!(text.contains("Rarog"));
    assert!(!text.contains("Concierge"));
}

#[test]
fn concierge_tab_renders_reasoning_effort_field() {
    let mut settings = SettingsState::new();
    settings.reduce(crate::state::settings::SettingsAction::SwitchTab(
        SettingsTab::Concierge,
    ));
    let mut concierge = crate::state::concierge::ConciergeState::new();
    concierge.reasoning_effort = Some("xhigh".to_string());

    let lines = render_concierge_tab(&settings, &concierge, &ThemeTokens::default());
    let text = lines
        .iter()
        .map(|line| line.to_string())
        .collect::<Vec<_>>()
        .join("\n");

    assert!(text.contains("Reasoning"));
    assert!(text.contains("xhigh"));
}

#[test]
fn protected_weles_row_hides_delete_and_disable_actions() {
    let mut settings = SettingsState::new();
    settings.reduce(crate::state::settings::SettingsAction::SwitchTab(
        SettingsTab::SubAgents,
    ));
    let config = ConfigState::new();
    let modal = ModalState::new();
    let auth = crate::state::auth::AuthState::new();
    let mut subagents = SubAgentsState::new();
    subagents
        .entries
        .push(crate::state::subagents::SubAgentEntry {
            id: "weles_builtin".to_string(),
            name: "WELES".to_string(),
            provider: PROVIDER_ID_OPENAI.to_string(),
            model: "gpt-5.4-mini".to_string(),
            role: Some("testing".to_string()),
            enabled: true,
            builtin: true,
            immutable_identity: true,
            disable_allowed: false,
            delete_allowed: false,
            protected_reason: Some("Daemon-owned governance agent".to_string()),
            reasoning_effort: Some("medium".to_string()),
            raw_json: Some(serde_json::json!({ "id": "weles_builtin" })),
        });
    let concierge = crate::state::concierge::ConciergeState::new();
    let tier = crate::state::tier::TierState::from_tier("power_user");
    let plugin_settings = crate::state::settings::PluginSettingsState::new();

    let lines = render_tab_content(
        100,
        &settings,
        &config,
        &modal,
        &auth,
        &subagents,
        &concierge,
        &tier,
        &plugin_settings,
        &ThemeTokens::default(),
    );
    let text = lines
        .iter()
        .map(|line| line.to_string())
        .collect::<Vec<_>>()
        .join("\n");

    assert!(
        text.contains("[built-in]"),
        "expected protected marker, got: {text}"
    );
    assert!(
        text.contains("[Locked]"),
        "expected locked action label, got: {text}"
    );
    assert!(
        text.contains("[Protected]"),
        "expected protected action label, got: {text}"
    );
    assert!(
        !text.contains("[Delete]"),
        "protected WELES row should hide delete action: {text}"
    );
    assert!(
        !text.contains("[Disable]"),
        "protected WELES row should hide disable action: {text}"
    );
}

#[test]
fn subagent_editor_renders_reasoning_effort_field() {
    let mut settings = SettingsState::new();
    settings.reduce(crate::state::settings::SettingsAction::SwitchTab(
        SettingsTab::SubAgents,
    ));
    let config = ConfigState::new();
    let modal = ModalState::new();
    let auth = crate::state::auth::AuthState::new();
    let mut subagents = SubAgentsState::new();
    let mut editor = crate::state::subagents::SubAgentEditorState::new(
        Some("weles_builtin".to_string()),
        1,
        PROVIDER_ID_OPENAI.to_string(),
        "gpt-5.4-mini".to_string(),
    );
    editor.name = "WELES".to_string();
    editor.reasoning_effort = Some("medium".to_string());
    editor.field = crate::state::subagents::SubAgentEditorField::ReasoningEffort;
    subagents.editor = Some(editor);
    let concierge = crate::state::concierge::ConciergeState::new();
    let tier = crate::state::tier::TierState::from_tier("power_user");
    let plugin_settings = crate::state::settings::PluginSettingsState::new();

    let lines = render_tab_content(
        100,
        &settings,
        &config,
        &modal,
        &auth,
        &subagents,
        &concierge,
        &tier,
        &plugin_settings,
        &ThemeTokens::default(),
    );
    let text = lines
        .iter()
        .map(|line| line.to_string())
        .collect::<Vec<_>>()
        .join("\n");

    assert!(text.contains("Reasoning"));
    assert!(text.contains("medium"));
}
