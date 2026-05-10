use super::*;
use crate::state::settings::{SettingsAction, SettingsTab};
use crate::theme::ThemeTokens;

#[test]
fn advanced_tab_shows_repo_monitor_checkbox_state() {
    let mut settings = SettingsState::new();
    settings.reduce(SettingsAction::SwitchTab(SettingsTab::Advanced));
    for _ in 0..22 {
        settings.reduce(SettingsAction::NavigateField(1));
    }

    let mut config = ConfigState::new();
    config.workspace_repo_monitor_enabled = true;

    let lines = render_advanced_tab(&settings, &config, &ThemeTokens::default());
    let text = lines
        .iter()
        .map(|line| line.to_string())
        .collect::<Vec<_>>()
        .join("\n");

    assert!(
        text.contains("[x] Enable Repo Monitor"),
        "advanced tab should show the repo monitor checkbox state: {text}"
    );
}