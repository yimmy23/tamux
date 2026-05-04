#[test]
fn current_field_name_concierge_tab_includes_reasoning_effort() {
    let mut state = SettingsState::new();
    state.reduce(SettingsAction::SwitchTab(SettingsTab::Concierge));
    assert_eq!(state.current_field_name(), "concierge_enabled");
    state.reduce(SettingsAction::NavigateField(1));
    assert_eq!(state.current_field_name(), "concierge_detail_level");
    state.reduce(SettingsAction::NavigateField(1));
    assert_eq!(state.current_field_name(), "concierge_provider");
    state.reduce(SettingsAction::NavigateField(1));
    assert_eq!(state.current_field_name(), "concierge_model");
    state.reduce(SettingsAction::NavigateField(1));
    assert_eq!(state.current_field_name(), "concierge_reasoning_effort");
}

#[test]
fn current_field_name_websearch_tab() {
    let mut state = SettingsState::new();
    state.reduce(SettingsAction::SwitchTab(SettingsTab::WebSearch));
    assert_eq!(state.current_field_name(), "web_search_enabled");
    state.reduce(SettingsAction::NavigateField(1));
    assert_eq!(state.current_field_name(), "search_provider");
    state.reduce(SettingsAction::NavigateField(1));
    assert_eq!(state.current_field_name(), "duckduckgo_region");
    state.reduce(SettingsAction::NavigateField(1));
    assert_eq!(state.current_field_name(), "duckduckgo_safe_search");
    state.reduce(SettingsAction::NavigateField(1));
    assert_eq!(state.current_field_name(), "firecrawl_api_key");
    state.reduce(SettingsAction::NavigateField(1));
    assert_eq!(state.current_field_name(), "exa_api_key");
    state.reduce(SettingsAction::NavigateField(1));
    assert_eq!(state.current_field_name(), "tavily_api_key");
    state.reduce(SettingsAction::NavigateField(1));
    assert_eq!(state.current_field_name(), "search_max_results");
    state.reduce(SettingsAction::NavigateField(1));
    assert_eq!(state.current_field_name(), "search_timeout");
}

#[test]
fn current_field_name_plugins_tab() {
    let mut state = SettingsState::new();
    state.reduce(SettingsAction::SwitchTab(SettingsTab::Plugins));
    assert_eq!(state.current_field_name(), "plugin_field");
}

#[test]
fn plugin_settings_state_defaults() {
    let ps = PluginSettingsState::new();
    assert!(ps.plugins.is_empty());
    assert_eq!(ps.selected_index, 0);
    assert!(ps.list_mode);
    assert!(ps.test_result.is_none());
    assert!(!ps.loading);
    assert_eq!(ps.detail_field_count(), 0);
    assert!(ps.selected_plugin().is_none());
}

#[test]
fn plugin_settings_state_selected_plugin() {
    let mut ps = PluginSettingsState::new();
    ps.plugins.push(PluginListItem {
        name: "test-plugin".to_string(),
        version: "1.0.0".to_string(),
        enabled: true,
        has_api: true,
        has_auth: false,
        settings_count: 2,
        description: Some("A test plugin".to_string()),
        install_source: "npm".to_string(),
        auth_status: "not_configured".to_string(),
        connector_kind: Some("github".to_string()),
        readiness_state: "needs_setup".to_string(),
        readiness_message: Some("Missing required settings: token.".to_string()),
        recovery_hint: Some("Open plugin settings and add a token.".to_string()),
        setup_hint: Some("Add a token.".to_string()),
        docs_path: Some("plugins/zorai-plugin-github/README.md".to_string()),
        workflow_primitives: vec!["list_work_items".to_string()],
        read_actions: vec!["list_issues".to_string()],
        write_actions: vec!["comment_on_work_item".to_string()],
    });
    assert_eq!(ps.selected_plugin().unwrap().name, "test-plugin");
    assert_eq!(ps.detail_field_count(), 1);
}

#[test]
fn insert_char_ignored_when_not_editing() {
    let mut state = SettingsState::new();
    state.reduce(SettingsAction::InsertChar('x'));
    assert_eq!(state.edit_buffer(), "");
}
