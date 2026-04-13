use super::*;

#[test]
fn push_and_pop_modal() {
    let mut state = ModalState::new();
    assert!(state.top().is_none());
    state.reduce(ModalAction::Push(ModalKind::CommandPalette));
    assert_eq!(state.top(), Some(ModalKind::CommandPalette));
    state.reduce(ModalAction::Pop);
    assert!(state.top().is_none());
}

#[test]
fn stacked_modals_pop_in_order() {
    let mut state = ModalState::new();
    state.reduce(ModalAction::Push(ModalKind::CommandPalette));
    state.reduce(ModalAction::Push(ModalKind::ProviderPicker));
    assert_eq!(state.top(), Some(ModalKind::ProviderPicker));
    state.reduce(ModalAction::Pop);
    assert_eq!(state.top(), Some(ModalKind::CommandPalette));
}

#[test]
fn remove_all_modal_kind_removes_nested_instances() {
    let mut state = ModalState::new();
    state.reduce(ModalAction::Push(ModalKind::Settings));
    state.reduce(ModalAction::Push(ModalKind::OpenAIAuth));
    state.reduce(ModalAction::Push(ModalKind::CommandPalette));
    state.reduce(ModalAction::Push(ModalKind::OpenAIAuth));

    state.reduce(ModalAction::RemoveAll(ModalKind::OpenAIAuth));

    assert_eq!(state.top(), Some(ModalKind::CommandPalette));
    state.reduce(ModalAction::Pop);
    assert_eq!(state.top(), Some(ModalKind::Settings));
}

#[test]
fn fuzzy_filter_narrows_items() {
    let mut state = ModalState::new();
    state.reduce(ModalAction::SetQuery("pro".into()));
    // "provider" and "prompt" should match "pro"
    let filtered_commands: Vec<&str> = state
        .filtered_items()
        .iter()
        .map(|&idx| state.command_items()[idx].command.as_str())
        .collect();
    assert!(filtered_commands.contains(&"provider"));
    assert!(filtered_commands.contains(&"prompt"));
    assert!(!filtered_commands.contains(&"model"));
}

#[test]
fn slash_prefix_stripped_for_matching() {
    let mut state = ModalState::new();
    state.reduce(ModalAction::SetQuery("/mod".into()));
    let filtered_commands: Vec<&str> = state
        .filtered_items()
        .iter()
        .map(|&idx| state.command_items()[idx].command.as_str())
        .collect();
    assert!(filtered_commands.contains(&"model"));
}

#[test]
fn command_palette_matches_command_head_when_query_has_arguments() {
    let mut state = ModalState::new();
    state.reduce(ModalAction::SetQuery("/prompt weles".into()));
    let filtered_commands: Vec<&str> = state
        .filtered_items()
        .iter()
        .map(|&idx| state.command_items()[idx].command.as_str())
        .collect();
    assert!(filtered_commands.contains(&"prompt"));
}

#[test]
fn install_queries_match_predefined_helper_commands() {
    let mut state = ModalState::new();
    state.reduce(ModalAction::SetQuery("install".into()));
    let filtered_commands: Vec<&str> = state
        .filtered_items()
        .iter()
        .map(|&idx| state.command_items()[idx].command.as_str())
        .collect();
    assert!(filtered_commands.contains(&"plugins install"));
    assert!(filtered_commands.contains(&"skills install"));
}

#[test]
fn command_palette_seeds_include_status_command() {
    let state = ModalState::new();

    assert!(state
        .command_items()
        .iter()
        .any(|item| item.command == "status"));
}

#[test]
fn command_palette_seeds_include_notifications_and_approvals_commands() {
    let state = ModalState::new();

    assert!(state
        .command_items()
        .iter()
        .any(|item| item.command == "notifications"));
    assert!(state
        .command_items()
        .iter()
        .any(|item| item.command == "approvals"));
}

#[test]
fn command_palette_seeds_include_compact_command() {
    let state = ModalState::new();

    assert!(state
        .command_items()
        .iter()
        .any(|item| item.command == "compact"));
}

#[test]
fn navigation_clamps_to_bounds() {
    let mut state = ModalState::new();
    state.reduce(ModalAction::Navigate(-1));
    assert_eq!(state.picker_cursor(), 0);
    for _ in 0..100 {
        state.reduce(ModalAction::Navigate(1));
    }
    assert!(state.picker_cursor() < state.command_items().len());
}

#[test]
fn selected_command_returns_correct_item() {
    let mut state = ModalState::new();
    state.reduce(ModalAction::Navigate(2));
    let selected = state.selected_command().unwrap();
    assert_eq!(selected.command, "tools");
}

#[test]
fn push_resets_query_and_cursor() {
    let mut state = ModalState::new();
    state.reduce(ModalAction::SetQuery("test".into()));
    state.reduce(ModalAction::Navigate(3));
    state.reduce(ModalAction::Push(ModalKind::CommandPalette));
    assert_eq!(state.command_query(), "");
    assert_eq!(state.picker_cursor(), 0);
}

#[test]
fn thread_picker_push_resets_to_swarog_tab() {
    let mut state = ModalState::new();
    state.set_thread_picker_tab(ThreadPickerTab::Internal);

    state.reduce(ModalAction::Push(ModalKind::ThreadPicker));

    assert_eq!(state.thread_picker_tab(), ThreadPickerTab::Swarog);
}

#[test]
fn empty_filter_shows_all_items() {
    let state = ModalState::new();
    assert_eq!(state.filtered_items().len(), state.command_items().len());
}

#[test]
fn whatsapp_status_maps_to_connected_error_and_disconnected() {
    let mut state = ModalState::new();
    state.set_whatsapp_link_status("qr_ready", None, None);
    assert_eq!(
        state.whatsapp_link().phase(),
        WhatsAppLinkPhase::AwaitingScan
    );
    assert!(state
        .whatsapp_link()
        .status_text()
        .contains("Scan the QR code"));

    state.set_whatsapp_link_status("connected", Some("+12065550123".to_string()), None);
    assert_eq!(state.whatsapp_link().phase(), WhatsAppLinkPhase::Connected);
    assert!(state.whatsapp_link().status_text().contains("Connected"));

    state.set_whatsapp_link_error("pairing failed".to_string());
    assert_eq!(state.whatsapp_link().phase(), WhatsAppLinkPhase::Error);
    assert!(state
        .whatsapp_link()
        .status_text()
        .contains("pairing failed"));

    state.set_whatsapp_link_disconnected(Some("socket closed".to_string()));
    assert_eq!(
        state.whatsapp_link().phase(),
        WhatsAppLinkPhase::Disconnected
    );
    assert!(state
        .whatsapp_link()
        .status_text()
        .contains("socket closed"));
}

#[test]
fn whatsapp_qr_updates_ascii_payload() {
    let mut state = ModalState::new();
    state.set_whatsapp_link_qr("██ QR".to_string(), Some(42));
    assert_eq!(
        state.whatsapp_link().phase(),
        WhatsAppLinkPhase::AwaitingScan
    );
    assert_eq!(state.whatsapp_link().ascii_qr(), Some("██ QR"));
    assert_eq!(state.whatsapp_link().expires_at_ms(), Some(42));
}

#[test]
fn whatsapp_terminal_states_clear_qr_expiry() {
    let mut state = ModalState::new();
    state.set_whatsapp_link_qr("██ QR".to_string(), Some(42));
    assert_eq!(state.whatsapp_link().expires_at_ms(), Some(42));

    state.set_whatsapp_link_connected(Some("+12065550123".to_string()));
    assert_eq!(state.whatsapp_link().expires_at_ms(), None);

    state.set_whatsapp_link_qr("██ QR".to_string(), Some(77));
    state.set_whatsapp_link_error("pairing failed".to_string());
    assert_eq!(state.whatsapp_link().expires_at_ms(), None);

    state.set_whatsapp_link_qr("██ QR".to_string(), Some(99));
    state.set_whatsapp_link_disconnected(Some("socket closed".to_string()));
    assert_eq!(state.whatsapp_link().expires_at_ms(), None);
}
