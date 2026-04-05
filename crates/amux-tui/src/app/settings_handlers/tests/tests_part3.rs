#[test]
fn collaboration_sessions_inspect_field_requests_collaboration_snapshot() {
    let (mut model, mut daemon_rx) = make_model();
    model
        .settings
        .reduce(SettingsAction::SwitchTab(SettingsTab::Chat));
    model.settings.reduce(SettingsAction::NavigateField(24));
    assert_eq!(model.settings.current_field_name(), "collaboration_sessions_inspect");

    model.activate_settings_field();

    assert!(matches!(
        daemon_rx.try_recv().expect("expected collaboration inspect command"),
        DaemonCommand::GetCollaborationSessions
    ));
    assert!(daemon_rx.try_recv().is_err());
}

#[test]
fn generated_tools_inspect_field_requests_generated_tools_snapshot() {
    let (mut model, mut daemon_rx) = make_model();
    model
        .settings
        .reduce(SettingsAction::SwitchTab(SettingsTab::Chat));
    model.settings.reduce(SettingsAction::NavigateField(25));
    assert_eq!(model.settings.current_field_name(), "generated_tools_inspect");

    model.activate_settings_field();

    assert!(matches!(
        daemon_rx.try_recv().expect("expected generated tools inspect command"),
        DaemonCommand::GetGeneratedTools
    ));
    assert!(daemon_rx.try_recv().is_err());
}