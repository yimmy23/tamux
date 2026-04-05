use super::*;
use amux_shared::providers::{
    PROVIDER_ID_ALIBABA_CODING_PLAN, PROVIDER_ID_CUSTOM, PROVIDER_ID_OPENAI,
};
use ratatui::layout::Rect;
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
fn whatsapp_modal_esc_keeps_connected_session_running() {
    let (mut model, mut daemon_rx) = make_model();
    model
        .modal
        .set_whatsapp_link_connected(Some("+48663977535".to_string()));
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::WhatsAppLink));

    let quit = model.handle_key_modal(
        KeyCode::Esc,
        KeyModifiers::NONE,
        modal::ModalKind::WhatsAppLink,
    );
    assert!(!quit);
    assert!(model.modal.top().is_none());
    assert!(matches!(
        daemon_rx.try_recv().expect("expected unsubscribe command"),
        DaemonCommand::WhatsAppLinkUnsubscribe
    ));
    assert!(daemon_rx.try_recv().is_err());
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
fn command_palette_tools_opens_settings_tools_tab() {
    let (mut model, mut daemon_rx) = make_model();
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::CommandPalette));
    model.modal.reduce(modal::ModalAction::Navigate(2));

    let quit = model.handle_key_modal(
        KeyCode::Enter,
        KeyModifiers::NONE,
        modal::ModalKind::CommandPalette,
    );

    assert!(!quit);
    assert_eq!(model.modal.top(), Some(modal::ModalKind::Settings));
    assert_eq!(model.settings.active_tab(), SettingsTab::Tools);
    assert!(matches!(
        daemon_rx.try_recv().expect("expected auth refresh"),
        DaemonCommand::GetProviderAuthStates
    ));
    assert!(matches!(
        daemon_rx.try_recv().expect("expected codex auth refresh"),
        DaemonCommand::GetOpenAICodexAuthStatus
    ));
    assert!(matches!(
        daemon_rx.try_recv().expect("expected sub-agent refresh"),
        DaemonCommand::ListSubAgents
    ));
    assert!(matches!(
        daemon_rx.try_recv().expect("expected rarog refresh"),
        DaemonCommand::GetConciergeConfig
    ));
}

#[test]
fn ctrl_e_in_error_modal_clears_error() {
    let (mut model, _daemon_rx) = make_model();
    model.last_error = Some("boom".to_string());
    model.error_active = true;

    let handled = model.handle_key(KeyCode::Char('e'), KeyModifiers::CONTROL);

    assert!(!handled);
    assert_eq!(model.modal.top(), Some(modal::ModalKind::ErrorViewer));
    assert_eq!(model.last_error.as_deref(), Some("boom"));

    let handled = model.handle_key(KeyCode::Char('e'), KeyModifiers::CONTROL);

    assert!(!handled);
    assert!(model.modal.top().is_none());
    assert!(
        model.last_error.is_none(),
        "second Ctrl+E should clear the stored error"
    );
    assert!(
        !model.error_active,
        "clearing the error should also clear the active error badge"
    );
}

#[test]
fn openai_auth_modal_enter_uses_daemon_provided_url() {
    let (mut model, _daemon_rx) = make_model();
    model.openai_auth_url = Some("https://auth.openai.com/oauth/authorize?flow=daemon".to_string());
    model.openai_auth_status_text =
        Some("Open this URL in your browser to complete ChatGPT authentication.".to_string());
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::OpenAIAuth));

    let quit = model.handle_key_modal(
        KeyCode::Enter,
        KeyModifiers::NONE,
        modal::ModalKind::OpenAIAuth,
    );

    assert!(!quit);
    assert_eq!(
        model.openai_auth_url.as_deref(),
        Some("https://auth.openai.com/oauth/authorize?flow=daemon")
    );
}

#[test]
fn openai_auth_modal_copy_uses_shared_clipboard_helper() {
    let (mut model, _daemon_rx) = make_model();
    crate::app::conversion::reset_last_copied_text();
    model.openai_auth_url = Some("https://auth.openai.com/oauth/authorize?flow=daemon".to_string());
    model.openai_auth_status_text =
        Some("Open this URL in your browser to complete ChatGPT authentication.".to_string());
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::OpenAIAuth));

    let quit = model.handle_key_modal(
        KeyCode::Char('c'),
        KeyModifiers::NONE,
        modal::ModalKind::OpenAIAuth,
    );

    assert!(!quit);
    assert_eq!(
        crate::app::conversion::last_copied_text().as_deref(),
        Some("https://auth.openai.com/oauth/authorize?flow=daemon")
    );
    assert_eq!(model.status_line, "Copied ChatGPT login URL to clipboard");
    assert_eq!(model.modal.top(), Some(modal::ModalKind::OpenAIAuth));
}

#[test]
fn ctrl_a_toggles_approval_center_modal() {
    let (mut model, _daemon_rx) = make_model();

    let handled = model.handle_key(KeyCode::Char('a'), KeyModifiers::CONTROL);
    assert!(!handled);
    assert_eq!(model.modal.top(), Some(modal::ModalKind::ApprovalCenter));

    let handled = model.handle_key(KeyCode::Char('a'), KeyModifiers::CONTROL);
    assert!(!handled);
    assert!(model.modal.top().is_none());
}

#[test]
fn approval_center_keyboard_resolves_selected_approval() {
    let (mut model, mut daemon_rx) = make_model();
    model.chat.reduce(chat::ChatAction::ThreadCreated {
        thread_id: "thread-1".to_string(),
        title: "Thread".to_string(),
    });
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-1".to_string()));
    model
        .approval
        .reduce(crate::state::ApprovalAction::ApprovalRequired(
            crate::state::PendingApproval {
                approval_id: "approval-1".to_string(),
                task_id: "task-1".to_string(),
                task_title: Some("Task".to_string()),
                thread_id: Some("thread-1".to_string()),
                thread_title: Some("Thread".to_string()),
                workspace_id: Some(model.config.honcho_workspace_id.clone()),
                rationale: None,
                reasons: Vec::new(),
                command: "git push".to_string(),
                risk_level: crate::state::RiskLevel::High,
                blast_radius: "repo".to_string(),
                received_at: 1,
                seen_at: None,
            },
        ));
    model.toggle_approval_center();

    let quit = model.handle_key_modal(
        KeyCode::Char('a'),
        KeyModifiers::NONE,
        modal::ModalKind::ApprovalCenter,
    );

    assert!(!quit);
    assert!(model.approval.pending_approvals().is_empty());
    assert!(matches!(
        daemon_rx.try_recv().expect("expected approval resolution command"),
        DaemonCommand::ResolveTaskApproval {
            approval_id,
            decision
        } if approval_id == "approval-1" && decision == "allow_once"
    ));
}

#[test]
fn approval_center_mouse_click_executes_approve_once() {
    let (mut model, mut daemon_rx) = make_model();
    model.width = 120;
    model.height = 40;
    model.chat.reduce(chat::ChatAction::ThreadCreated {
        thread_id: "thread-1".to_string(),
        title: "Thread".to_string(),
    });
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-1".to_string()));
    model
        .approval
        .reduce(crate::state::ApprovalAction::ApprovalRequired(
            crate::state::PendingApproval {
                approval_id: "approval-1".to_string(),
                task_id: "task-1".to_string(),
                task_title: Some("Task".to_string()),
                thread_id: Some("thread-1".to_string()),
                thread_title: Some("Thread".to_string()),
                workspace_id: Some(model.config.honcho_workspace_id.clone()),
                rationale: Some("Needed".to_string()),
                reasons: vec!["network".to_string()],
                command: "git push".to_string(),
                risk_level: crate::state::RiskLevel::High,
                blast_radius: "repo".to_string(),
                received_at: 1,
                seen_at: None,
            },
        ));
    model.toggle_approval_center();
    let (_, area) = model
        .current_modal_area()
        .expect("approval center modal area should exist");
    let click = (area.y..area.y.saturating_add(area.height))
        .flat_map(|row| {
            (area.x..area.x.saturating_add(area.width)).map(move |column| (column, row))
        })
        .find(|(column, row)| {
            widgets::approval_center::hit_test(
                area,
                &model.approval,
                model.chat.active_thread_id(),
                model.current_workspace_id(),
                ratatui::layout::Position::new(*column, *row),
            ) == Some(
                widgets::approval_center::ApprovalCenterHitTarget::ApproveOnce(
                    "approval-1".to_string(),
                ),
            )
        })
        .expect("approve-once button should be hittable");

    model.handle_mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: click.0,
        row: click.1,
        modifiers: KeyModifiers::NONE,
    });

    assert!(matches!(
        daemon_rx.try_recv().expect("expected approval resolution command"),
        DaemonCommand::ResolveTaskApproval {
            approval_id,
            decision
        } if approval_id == "approval-1" && decision == "allow_once"
    ));
}

#[test]
fn command_palette_plugins_install_seeds_terminal_command() {
    let (mut model, _daemon_rx) = make_model();
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::CommandPalette));
    model
        .modal
        .reduce(modal::ModalAction::SetQuery("plugins install".into()));

    let quit = model.handle_key_modal(
        KeyCode::Enter,
        KeyModifiers::NONE,
        modal::ModalKind::CommandPalette,
    );

    assert!(!quit);
    assert_eq!(model.input.buffer(), "tamux install plugin ");
}

#[test]
fn command_palette_skills_install_seeds_terminal_command() {
    let (mut model, _daemon_rx) = make_model();
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::CommandPalette));
    model
        .modal
        .reduce(modal::ModalAction::SetQuery("skills install".into()));

    let quit = model.handle_key_modal(
        KeyCode::Enter,
        KeyModifiers::NONE,
        modal::ModalKind::CommandPalette,
    );

    assert!(!quit);
    assert_eq!(model.input.buffer(), "tamux skill import ");
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

#[test]
fn stacked_modal_pop_preserves_connected_whatsapp_session() {
    let (mut model, mut daemon_rx) = make_model();
    model
        .modal
        .set_whatsapp_link_connected(Some("+48663977535".to_string()));
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
        daemon_rx.try_recv().expect("expected unsubscribe command"),
        DaemonCommand::WhatsAppLinkUnsubscribe
    ));
    assert!(daemon_rx.try_recv().is_err());
}

#[test]
fn selecting_custom_provider_does_not_chain_into_model_picker() {
    let (mut model, _daemon_rx) = make_model();
    let custom_index = widgets::provider_picker::available_provider_defs(&model.auth)
        .iter()
        .position(|provider| provider.id == PROVIDER_ID_CUSTOM)
        .expect("custom provider to exist");

    model.settings_picker_target = Some(SettingsPickerTarget::Provider);
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::ProviderPicker));
    model.modal.set_picker_item_count(
        widgets::provider_picker::available_provider_defs(&model.auth).len(),
    );
    if custom_index > 0 {
        model
            .modal
            .reduce(modal::ModalAction::Navigate(custom_index as i32));
    }

    let quit = model.handle_key_modal(
        KeyCode::Enter,
        KeyModifiers::NONE,
        modal::ModalKind::ProviderPicker,
    );

    assert!(!quit);
    assert_eq!(model.config.provider, PROVIDER_ID_CUSTOM);
    assert_ne!(model.modal.top(), Some(modal::ModalKind::ModelPicker));
}

#[test]
fn selecting_custom_provider_focuses_model_field_for_inline_entry() {
    let (mut model, _daemon_rx) = make_model();
    let custom_index = widgets::provider_picker::available_provider_defs(&model.auth)
        .iter()
        .position(|provider| provider.id == PROVIDER_ID_CUSTOM)
        .expect("custom provider to exist");

    model
        .settings
        .reduce(SettingsAction::SwitchTab(SettingsTab::Provider));
    model.settings_picker_target = Some(SettingsPickerTarget::Provider);
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::ProviderPicker));
    model.modal.set_picker_item_count(
        widgets::provider_picker::available_provider_defs(&model.auth).len(),
    );
    if custom_index > 0 {
        model
            .modal
            .reduce(modal::ModalAction::Navigate(custom_index as i32));
    }

    let quit = model.handle_key_modal(
        KeyCode::Enter,
        KeyModifiers::NONE,
        modal::ModalKind::ProviderPicker,
    );

    assert!(!quit);
    assert_eq!(model.config.provider, PROVIDER_ID_CUSTOM);
    assert_eq!(model.settings.current_field_name(), "model");
    assert_eq!(model.settings.field_cursor(), 3);
}

#[test]
fn provider_picker_filters_to_authenticated_entries_plus_custom() {
    let (mut model, _daemon_rx) = make_model();
    model.auth.entries = vec![
        crate::state::auth::ProviderAuthEntry {
            provider_id: PROVIDER_ID_OPENAI.to_string(),
            provider_name: "OpenAI".to_string(),
            authenticated: true,
            auth_source: "api_key".to_string(),
            model: "gpt-5.4".to_string(),
        },
        crate::state::auth::ProviderAuthEntry {
            provider_id: "groq".to_string(),
            provider_name: "Groq".to_string(),
            authenticated: false,
            auth_source: "api_key".to_string(),
            model: "llama".to_string(),
        },
    ];

    let defs = widgets::provider_picker::available_provider_defs(&model.auth);
    assert!(defs.iter().any(|provider| provider.id == PROVIDER_ID_OPENAI));
    assert!(defs.iter().any(|provider| provider.id == PROVIDER_ID_CUSTOM));
    assert!(!defs.iter().any(|provider| provider.id == "groq"));
}

#[test]
fn model_command_skips_remote_fetch_for_static_provider_catalogs() {
    let (mut model, mut daemon_rx) = make_model();
    model.config.provider = PROVIDER_ID_ALIBABA_CODING_PLAN.to_string();
    model.config.base_url = "https://coding-intl.dashscope.aliyuncs.com/v1".to_string();
    model.config.model = "qwen3.5-plus".to_string();
    model.config.auth_source = "api_key".to_string();
    model.config.api_key = "dashscope-key".to_string();

    model.execute_command("model");

    assert_eq!(model.modal.top(), Some(modal::ModalKind::ModelPicker));
    while let Ok(command) = daemon_rx.try_recv() {
        if let DaemonCommand::FetchModels { .. } = command {
            panic!("static providers should not trigger remote model fetches");
        }
    }
}

#[test]
fn provider_picker_skips_remote_fetch_for_static_provider_catalogs() {
    let (mut model, mut daemon_rx) = make_model();
    model.auth.entries = vec![crate::state::auth::ProviderAuthEntry {
        provider_id: PROVIDER_ID_ALIBABA_CODING_PLAN.to_string(),
        provider_name: "Alibaba Coding Plan".to_string(),
        authenticated: true,
        auth_source: "api_key".to_string(),
        model: "qwen3.5-plus".to_string(),
    }];

    let alibaba_index = widgets::provider_picker::available_provider_defs(&model.auth)
        .iter()
        .position(|provider| provider.id == PROVIDER_ID_ALIBABA_CODING_PLAN)
        .expect("alibaba-coding-plan to exist");

    model.settings_picker_target = Some(SettingsPickerTarget::Provider);
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::ProviderPicker));
    model.modal.set_picker_item_count(
        widgets::provider_picker::available_provider_defs(&model.auth).len(),
    );
    if alibaba_index > 0 {
        model
            .modal
            .reduce(modal::ModalAction::Navigate(alibaba_index as i32));
    }

    let quit = model.handle_key_modal(
        KeyCode::Enter,
        KeyModifiers::NONE,
        modal::ModalKind::ProviderPicker,
    );

    assert!(!quit);
    assert_eq!(model.config.provider, PROVIDER_ID_ALIBABA_CODING_PLAN);
    assert_eq!(model.modal.top(), Some(modal::ModalKind::ModelPicker));
    while let Ok(command) = daemon_rx.try_recv() {
        if let DaemonCommand::FetchModels { .. } = command {
            panic!("static providers should not trigger remote model fetches");
        }
    }
}

#[test]
fn protected_weles_editor_can_open_provider_model_and_effort_pickers() {
    let (mut model, _daemon_rx) = make_model();
    model.auth.entries = vec![crate::state::auth::ProviderAuthEntry {
        provider_id: PROVIDER_ID_OPENAI.to_string(),
        provider_name: "OpenAI".to_string(),
        authenticated: true,
        auth_source: "api_key".to_string(),
        model: "gpt-5.4".to_string(),
    }];

    let mut editor = crate::state::subagents::SubAgentEditorState::new(
        Some("weles_builtin".to_string()),
        1,
        PROVIDER_ID_OPENAI.to_string(),
        "gpt-5.4-mini".to_string(),
    );
    editor.name = "WELES".to_string();
    editor.builtin = true;
    editor.immutable_identity = true;
    editor.disable_allowed = false;
    editor.delete_allowed = false;
    editor.reasoning_effort = Some("medium".to_string());
    editor.field = crate::state::subagents::SubAgentEditorField::Provider;
    model.subagents.editor = Some(editor.clone());
    model
        .settings
        .reduce(SettingsAction::SwitchTab(SettingsTab::SubAgents));
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::Settings));

    let quit = model.handle_key_modal(
        KeyCode::Enter,
        KeyModifiers::NONE,
        modal::ModalKind::Settings,
    );
    assert!(!quit);
    assert_eq!(model.modal.top(), Some(modal::ModalKind::ProviderPicker));

    model.close_top_modal();
    if let Some(editor) = model.subagents.editor.as_mut() {
        editor.field = crate::state::subagents::SubAgentEditorField::Model;
    }
    let quit = model.handle_key_modal(
        KeyCode::Enter,
        KeyModifiers::NONE,
        modal::ModalKind::Settings,
    );
    assert!(!quit);
    assert_eq!(model.modal.top(), Some(modal::ModalKind::ModelPicker));

    model.close_top_modal();
    if let Some(editor) = model.subagents.editor.as_mut() {
        editor.field = crate::state::subagents::SubAgentEditorField::ReasoningEffort;
    }
    let quit = model.handle_key_modal(
        KeyCode::Enter,
        KeyModifiers::NONE,
        modal::ModalKind::Settings,
    );
    assert!(!quit);
    assert_eq!(model.modal.top(), Some(modal::ModalKind::EffortPicker));
}

#[test]
fn thread_picker_right_arrow_switches_to_rarog_tab() {
    let (mut model, _daemon_rx) = make_model();
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::ThreadPicker));

    let quit = model.handle_key_modal(
        KeyCode::Right,
        KeyModifiers::NONE,
        modal::ModalKind::ThreadPicker,
    );

    assert!(!quit);
    assert_eq!(
        model.modal.thread_picker_tab(),
        modal::ThreadPickerTab::Rarog
    );
}

#[test]
fn thread_picker_left_right_cycles_all_sources() {
    let (mut model, _daemon_rx) = make_model();
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::ThreadPicker));

    let quit = model.handle_key_modal(
        KeyCode::Right,
        KeyModifiers::NONE,
        modal::ModalKind::ThreadPicker,
    );
    assert!(!quit);
    assert_eq!(
        model.modal.thread_picker_tab(),
        modal::ThreadPickerTab::Rarog
    );

    let quit = model.handle_key_modal(
        KeyCode::Right,
        KeyModifiers::NONE,
        modal::ModalKind::ThreadPicker,
    );
    assert!(!quit);
    assert_eq!(
        model.modal.thread_picker_tab(),
        modal::ThreadPickerTab::Weles
    );

    let quit = model.handle_key_modal(
        KeyCode::Right,
        KeyModifiers::NONE,
        modal::ModalKind::ThreadPicker,
    );
    assert!(!quit);
    assert_eq!(
        model.modal.thread_picker_tab(),
        modal::ThreadPickerTab::Internal
    );

    let quit = model.handle_key_modal(
        KeyCode::Left,
        KeyModifiers::NONE,
        modal::ModalKind::ThreadPicker,
    );
    assert!(!quit);
    assert_eq!(
        model.modal.thread_picker_tab(),
        modal::ThreadPickerTab::Weles
    );
}

#[test]
fn thread_picker_enter_selects_filtered_rarog_thread() {
    let (mut model, mut daemon_rx) = make_model();
    model.concierge.auto_cleanup_on_navigate = false;
    model.chat.reduce(chat::ChatAction::ThreadListReceived(vec![
        chat::AgentThread {
            id: "regular-thread".into(),
            title: "Regular work".into(),
            ..Default::default()
        },
        chat::AgentThread {
            id: "heartbeat-1".into(),
            title: "HEARTBEAT SYNTHESIS".into(),
            ..Default::default()
        },
    ]));
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::ThreadPicker));
    model
        .modal
        .set_thread_picker_tab(modal::ThreadPickerTab::Rarog);
    model.sync_thread_picker_item_count();
    model.modal.reduce(modal::ModalAction::Navigate(1));

    let quit = model.handle_key_modal(
        KeyCode::Enter,
        KeyModifiers::NONE,
        modal::ModalKind::ThreadPicker,
    );

    assert!(!quit);
    assert_eq!(model.chat.active_thread_id(), Some("heartbeat-1"));
    match daemon_rx.try_recv() {
        Ok(DaemonCommand::RequestThread(thread_id)) => assert_eq!(thread_id, "heartbeat-1"),
        other => panic!("expected thread request, got {:?}", other),
    }
}

#[test]
fn thread_picker_new_conversation_uses_selected_agent_for_first_prompt() {
    let (mut model, mut daemon_rx) = make_model();
    model.connected = true;
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::ThreadPicker));
    model
        .modal
        .set_thread_picker_tab(modal::ThreadPickerTab::Weles);
    model.sync_thread_picker_item_count();

    let quit = model.handle_key_modal(
        KeyCode::Enter,
        KeyModifiers::NONE,
        modal::ModalKind::ThreadPicker,
    );
    assert!(!quit);

    model.submit_prompt("tell me your secrets".to_string());

    loop {
        match daemon_rx.try_recv() {
            Ok(DaemonCommand::DismissConciergeWelcome) => {}
            Ok(DaemonCommand::SendMessage {
                thread_id,
                target_agent_id,
                content,
                ..
            }) => {
                assert_eq!(thread_id, None);
                assert_eq!(target_agent_id.as_deref(), Some("weles"));
                assert_eq!(content, "tell me your secrets");
                break;
            }
            other => panic!("expected send-message command, got {:?}", other),
        }
    }
}

#[test]
fn thread_picker_mouse_click_switches_to_rarog_tab() {
    let (mut model, _daemon_rx) = make_model();
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::ThreadPicker));
    let (_, overlay_area) = model
        .current_modal_area()
        .expect("thread picker modal should be visible");

    let rarog_pos = (overlay_area.y..overlay_area.y.saturating_add(overlay_area.height))
        .find_map(|row| {
            (overlay_area.x..overlay_area.x.saturating_add(overlay_area.width)).find_map(|column| {
                let pos = Position::new(column, row);
                if widgets::thread_picker::hit_test(overlay_area, &model.chat, &model.modal, pos)
                    == Some(widgets::thread_picker::ThreadPickerHitTarget::Tab(
                        modal::ThreadPickerTab::Rarog,
                    ))
                {
                    Some(pos)
                } else {
                    None
                }
            })
        })
        .expect("thread picker should expose a clickable Rarog tab");

    model.handle_mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: rarog_pos.x,
        row: rarog_pos.y,
        modifiers: KeyModifiers::NONE,
    });

    assert_eq!(
        model.modal.thread_picker_tab(),
        modal::ThreadPickerTab::Rarog
    );
}

#[test]
fn ctrl_q_opens_queued_prompts_modal() {
    let (mut model, _daemon_rx) = make_model();
    model.queued_prompts.push(QueuedPrompt::new("steer prompt"));

    let quit = model.handle_key(KeyCode::Char('q'), KeyModifiers::CONTROL);

    assert!(!quit);
    assert_eq!(model.modal.top(), Some(modal::ModalKind::QueuedPrompts));
    assert_eq!(model.modal.picker_cursor(), 0);
}

#[test]
fn queued_prompts_modal_send_now_stops_stream_and_sends_selected_prompt() {
    let (mut model, mut daemon_rx) = make_model();
    model.connected = true;
    model.concierge.auto_cleanup_on_navigate = false;
    model.chat.reduce(chat::ChatAction::ThreadCreated {
        thread_id: "thread-1".to_string(),
        title: "Thread".to_string(),
    });
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-1".to_string()));
    model.handle_tool_call_event(
        "thread-1".to_string(),
        "call-1".to_string(),
        "bash_command".to_string(),
        "{\"command\":\"pwd\"}".to_string(),
        None,
    );
    model
        .queued_prompts
        .push(QueuedPrompt::new("send this now"));
    model.open_queued_prompts_modal();

    let quit = model.handle_key_modal(
        KeyCode::Enter,
        KeyModifiers::NONE,
        modal::ModalKind::QueuedPrompts,
    );

    assert!(!quit);
    match daemon_rx.try_recv() {
        Ok(DaemonCommand::StopStream { thread_id }) => assert_eq!(thread_id, "thread-1"),
        other => panic!("expected stop-stream before send-now, got {:?}", other),
    }
    match daemon_rx.try_recv() {
        Ok(DaemonCommand::SendMessage {
            thread_id, content, ..
        }) => {
            assert_eq!(thread_id.as_deref(), Some("thread-1"));
            assert_eq!(content, "send this now");
        }
        other => panic!("expected send-now prompt dispatch, got {:?}", other),
    }
    assert!(model.queued_prompts.is_empty());
}

#[test]
fn queued_prompts_modal_copy_marks_item_as_copied_for_five_seconds() {
    let (mut model, _daemon_rx) = make_model();
    model.queued_prompts.push(QueuedPrompt::new("copy me"));
    model.open_queued_prompts_modal();

    let quit = model.handle_key_modal(
        KeyCode::Right,
        KeyModifiers::NONE,
        modal::ModalKind::QueuedPrompts,
    );
    assert!(!quit);

    let quit = model.handle_key_modal(
        KeyCode::Enter,
        KeyModifiers::NONE,
        modal::ModalKind::QueuedPrompts,
    );
    assert!(!quit);
    assert!(model.queued_prompts[0].is_copied(model.tick_counter));

    for _ in 0..100 {
        model.on_tick();
    }
    assert!(!model.queued_prompts[0].is_copied(model.tick_counter));
}

#[test]
fn queued_prompts_modal_delete_action_removes_clicked_item() {
    let (mut model, _daemon_rx) = make_model();
    model.queued_prompts.push(QueuedPrompt::new("delete me"));
    model.open_queued_prompts_modal();
    let (_, overlay_area) = model
        .current_modal_area()
        .expect("queued prompts modal should be visible");

    let delete_pos = (overlay_area.y..overlay_area.y.saturating_add(overlay_area.height))
        .find_map(|row| {
            (overlay_area.x..overlay_area.x.saturating_add(overlay_area.width)).find_map(|column| {
                let pos = Position::new(column, row);
                if widgets::queued_prompts::hit_test(
                    overlay_area,
                    &model.queued_prompts,
                    model.modal.picker_cursor(),
                    model.tick_counter,
                    pos,
                ) == Some(widgets::queued_prompts::QueuedPromptsHitTarget::Action {
                    message_index: 0,
                    action: QueuedPromptAction::Delete,
                }) {
                    Some(pos)
                } else {
                    None
                }
            })
        })
        .expect("delete action should be clickable");

    model.handle_mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: delete_pos.x,
        row: delete_pos.y,
        modifiers: KeyModifiers::NONE,
    });

    assert!(model.queued_prompts.is_empty());
    assert!(model.modal.top().is_none());
}

#[test]
fn clicking_footer_queue_indicator_opens_queued_prompts_modal() {
    let (mut model, _daemon_rx) = make_model();
    model.queued_prompts.push(QueuedPrompt::new("preview me"));

    let status_area = Rect::new(0, model.height.saturating_sub(1), model.width, 1);
    let queue_pos = (status_area.x..status_area.x.saturating_add(status_area.width))
        .find_map(|column| {
            let pos = Position::new(column, status_area.y);
            if widgets::footer::status_bar_hit_test(
                status_area,
                model.connected,
                model.last_error.is_some(),
                model.queued_prompts.len(),
                pos,
            ) == Some(widgets::footer::StatusBarHitTarget::QueuedPrompts)
            {
                Some(pos)
            } else {
                None
            }
        })
        .expect("queue indicator should be clickable");

    model.handle_mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: queue_pos.x,
        row: queue_pos.y,
        modifiers: KeyModifiers::NONE,
    });

    assert_eq!(model.modal.top(), Some(modal::ModalKind::QueuedPrompts));
}

#[test]
fn subagent_inline_edit_does_not_sync_main_config() {
    let (mut model, mut daemon_rx) = make_model();
    model.connected = true;
    model.agent_config_loaded = true;
    model.config.agent_config_raw = Some(serde_json::json!({}));

    let mut editor = crate::state::subagents::SubAgentEditorState::new(
        None,
        1,
        "openai".to_string(),
        "gpt-5.4".to_string(),
    );
    editor.name = "Draft".to_string();
    model.subagents.editor = Some(editor);
    model.settings.start_editing("subagent_name", "Draft");
    model.settings.reduce(SettingsAction::InsertChar('X'));

    let quit = model.handle_key_modal(
        KeyCode::Enter,
        KeyModifiers::NONE,
        modal::ModalKind::Settings,
    );

    assert!(!quit);
    assert_eq!(
        model
            .subagents
            .editor
            .as_ref()
            .map(|editor| editor.name.as_str()),
        Some("DraftX")
    );
    assert!(
        daemon_rx.try_recv().is_err(),
        "sub-agent field edits should stay local until Save"
    );
}

fn sample_notification(read_at: Option<i64>) -> amux_protocol::InboxNotification {
    amux_protocol::InboxNotification {
        id: "n1".to_string(),
        source: "plugin_auth".to_string(),
        kind: "plugin_auth_warning".to_string(),
        title: "Refresh needed".to_string(),
        body: "Reconnect plugin auth before it expires.".to_string(),
        subtitle: Some("gmail".to_string()),
        severity: "warning".to_string(),
        created_at: 1,
        updated_at: 1,
        read_at,
        archived_at: None,
        deleted_at: None,
        actions: Vec::new(),
        metadata_json: None,
    }
}

#[test]
fn notifications_modal_uses_wider_overlay_width() {
    let (mut model, _daemon_rx) = make_model();
    model.width = 100;
    model.height = 40;
    model.toggle_notifications_modal();

    let (_, overlay_area) = model
        .current_modal_area()
        .expect("notifications modal should be visible");

    assert_eq!(overlay_area.width, 78);
}

#[test]
fn notifications_modal_left_right_changes_header_focus_and_enter_uses_it() {
    let (mut model, _daemon_rx) = make_model();
    model
        .notifications
        .reduce(crate::state::NotificationsAction::Replace(vec![
            sample_notification(None),
        ]));
    model.toggle_notifications_modal();

    assert_eq!(
        model.notifications.selected_header_action(),
        Some(crate::state::NotificationsHeaderAction::MarkAllRead)
    );

    let quit = model.handle_key_modal(
        KeyCode::Right,
        KeyModifiers::NONE,
        modal::ModalKind::Notifications,
    );
    assert!(!quit);
    assert_eq!(
        model.notifications.selected_header_action(),
        Some(crate::state::NotificationsHeaderAction::Close)
    );

    let quit = model.handle_key_modal(
        KeyCode::Enter,
        KeyModifiers::NONE,
        modal::ModalKind::Notifications,
    );
    assert!(!quit);
    assert!(model.modal.top().is_none());
}

#[test]
fn notifications_modal_down_clears_header_focus_and_enter_expands_row() {
    let (mut model, _daemon_rx) = make_model();
    model
        .notifications
        .reduce(crate::state::NotificationsAction::Replace(vec![
            sample_notification(None),
        ]));
    model.toggle_notifications_modal();

    let quit = model.handle_key_modal(
        KeyCode::Down,
        KeyModifiers::NONE,
        modal::ModalKind::Notifications,
    );
    assert!(!quit);
    assert_eq!(model.notifications.selected_header_action(), None);

    let quit = model.handle_key_modal(
        KeyCode::Enter,
        KeyModifiers::NONE,
        modal::ModalKind::Notifications,
    );
    assert!(!quit);
    assert_eq!(model.notifications.expanded_id(), Some("n1"));
}

#[test]
fn notifications_modal_tab_switches_between_header_and_row_actions() {
    let (mut model, _daemon_rx) = make_model();
    model
        .notifications
        .reduce(crate::state::NotificationsAction::Replace(vec![
            sample_notification(None),
        ]));
    model.toggle_notifications_modal();

    assert_eq!(
        model.notifications.selected_header_action(),
        Some(crate::state::NotificationsHeaderAction::MarkAllRead)
    );
    assert_eq!(model.notifications.selected_row_action_index(), None);

    let quit = model.handle_key_modal(
        KeyCode::Tab,
        KeyModifiers::NONE,
        modal::ModalKind::Notifications,
    );
    assert!(!quit);
    assert_eq!(model.notifications.selected_header_action(), None);
    assert_eq!(model.notifications.selected_row_action_index(), Some(0));

    let quit = model.handle_key_modal(
        KeyCode::Tab,
        KeyModifiers::NONE,
        modal::ModalKind::Notifications,
    );
    assert!(!quit);
    assert_eq!(
        model.notifications.selected_header_action(),
        Some(crate::state::NotificationsHeaderAction::MarkAllRead)
    );
    assert_eq!(model.notifications.selected_row_action_index(), None);
}

#[test]
fn notifications_modal_row_action_focus_uses_left_right_and_enter() {
    let (mut model, _daemon_rx) = make_model();
    model
        .notifications
        .reduce(crate::state::NotificationsAction::Replace(vec![
            sample_notification(None),
        ]));
    model.toggle_notifications_modal();

    let quit = model.handle_key_modal(
        KeyCode::Tab,
        KeyModifiers::NONE,
        modal::ModalKind::Notifications,
    );
    assert!(!quit);
    assert_eq!(model.notifications.selected_row_action_index(), Some(0));

    let quit = model.handle_key_modal(
        KeyCode::Right,
        KeyModifiers::NONE,
        modal::ModalKind::Notifications,
    );
    assert!(!quit);
    assert_eq!(model.notifications.selected_row_action_index(), Some(1));

    let quit = model.handle_key_modal(
        KeyCode::Enter,
        KeyModifiers::NONE,
        modal::ModalKind::Notifications,
    );
    assert!(!quit);
    assert!(model
        .notifications
        .selected_item()
        .and_then(|notification| notification.read_at)
        .is_some());
}
