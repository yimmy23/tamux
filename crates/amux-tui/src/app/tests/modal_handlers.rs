use super::*;
use amux_shared::providers::{
    PROVIDER_ID_ALIBABA_CODING_PLAN, PROVIDER_ID_CUSTOM, PROVIDER_ID_OPENAI, PROVIDER_ID_QWEN,
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
fn slash_status_opens_loading_modal_and_requests_status_without_sending_chat() {
    let (mut model, mut daemon_rx) = make_model();
    model.connected = true;
    model.input.set_text("/status");

    let quit = model.handle_key(KeyCode::Enter, KeyModifiers::NONE);

    assert!(!quit);
    assert_eq!(model.modal.top(), Some(modal::ModalKind::Status));
    assert!(model.status_modal_loading);
    match daemon_rx.try_recv() {
        Ok(DaemonCommand::RequestAgentStatus) => {}
        other => panic!("expected status request, got {:?}", other),
    }
    assert!(daemon_rx.try_recv().is_err());
}

#[test]
fn slash_statistics_opens_loading_modal_and_requests_all_time_statistics() {
    let (mut model, mut daemon_rx) = make_model();
    model.connected = true;
    model.input.set_text("/statistics");

    let quit = model.handle_key(KeyCode::Enter, KeyModifiers::NONE);

    assert!(!quit);
    assert_eq!(model.modal.top(), Some(modal::ModalKind::Statistics));
    assert!(model.statistics_modal_loading);
    match daemon_rx.try_recv() {
        Ok(DaemonCommand::RequestAgentStatistics { window }) => {
            assert_eq!(window, amux_protocol::AgentStatisticsWindow::All);
        }
        other => panic!("expected statistics request, got {:?}", other),
    }
    assert!(daemon_rx.try_recv().is_err());
}

#[test]
fn slash_compact_requests_forced_compaction_for_active_thread() {
    let (mut model, mut daemon_rx) = make_model();
    model.connected = true;
    model.chat.reduce(chat::ChatAction::ThreadCreated {
        thread_id: "thread-1".to_string(),
        title: "Compaction Thread".to_string(),
    });
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-1".to_string()));
    model.input.set_text("/compact");

    let quit = model.handle_key(KeyCode::Enter, KeyModifiers::NONE);

    assert!(!quit);
    match daemon_rx.try_recv() {
        Ok(DaemonCommand::ForceCompact { thread_id }) => {
            assert_eq!(thread_id, "thread-1");
        }
        other => panic!("expected force-compaction request, got {:?}", other),
    }
    assert!(daemon_rx.try_recv().is_err());
}

#[test]
fn slash_prompt_opens_loading_modal_and_requests_main_prompt() {
    let (mut model, mut daemon_rx) = make_model();
    model.connected = true;
    model.input.set_text("/prompt");

    let quit = model.handle_key(KeyCode::Enter, KeyModifiers::NONE);

    assert!(!quit);
    assert_eq!(model.modal.top(), Some(modal::ModalKind::PromptViewer));
    assert!(model.prompt_modal_loading);
    match daemon_rx.try_recv() {
        Ok(DaemonCommand::RequestPromptInspection { agent_id }) => {
            assert!(agent_id.is_none());
        }
        other => panic!("expected prompt inspection request, got {:?}", other),
    }
    assert!(daemon_rx.try_recv().is_err());
}

#[test]
fn slash_prompt_weles_requests_explicit_agent_prompt() {
    let (mut model, mut daemon_rx) = make_model();
    model.connected = true;
    model.input.set_text("/prompt weles");

    let quit = model.handle_key(KeyCode::Enter, KeyModifiers::NONE);

    assert!(!quit);
    match daemon_rx.try_recv() {
        Ok(DaemonCommand::RequestPromptInspection { agent_id }) => {
            assert_eq!(agent_id.as_deref(), Some("weles"));
        }
        other => panic!(
            "expected explicit prompt inspection request, got {:?}",
            other
        ),
    }
}

#[test]
fn slash_participants_opens_modal_with_thread_participant_sections() {
    let (mut model, _daemon_rx) = make_model();
    model.connected = true;
    model.handle_client_event(ClientEvent::ThreadDetail(Some(crate::wire::AgentThread {
        id: "thread-1".to_string(),
        title: "Participant Thread".to_string(),
        agent_name: Some("Svarog".to_string()),
        thread_participants: vec![
            crate::wire::ThreadParticipantState {
                agent_id: "weles".to_string(),
                agent_name: "Weles".to_string(),
                instruction: "verify claims".to_string(),
                status: "active".to_string(),
                created_at: 1,
                updated_at: 2,
                deactivated_at: None,
                last_contribution_at: Some(3),
                always_auto_response: false,
            },
            crate::wire::ThreadParticipantState {
                agent_id: "rarog".to_string(),
                agent_name: "Rarog".to_string(),
                instruction: "watch approvals".to_string(),
                status: "inactive".to_string(),
                created_at: 1,
                updated_at: 2,
                deactivated_at: Some(4),
                last_contribution_at: None,
                always_auto_response: false,
            },
        ],
        queued_participant_suggestions: vec![crate::wire::ThreadParticipantSuggestion {
            id: "sugg-1".to_string(),
            target_agent_id: "weles".to_string(),
            target_agent_name: "Weles".to_string(),
            instruction: "check the final answer".to_string(),
            suggestion_kind: "prepared_message".to_string(),
            force_send: false,
            status: "queued".to_string(),
            created_at: 5,
            updated_at: 5,
            auto_send_at: None,
            source_message_timestamp: None,
            error: None,
        }],
        ..Default::default()
    })));
    model.input.set_text("/participants");

    let quit = model.handle_key(KeyCode::Enter, KeyModifiers::NONE);

    assert!(!quit);
    assert_eq!(
        model.modal.top(),
        Some(modal::ModalKind::ThreadParticipants)
    );
    let body = model.thread_participants_modal_body();
    assert!(
        body.contains("Active Participants"),
        "missing active section: {body}"
    );
    assert!(body.contains("Weles"), "missing active participant: {body}");
    assert!(
        body.contains("Inactive Participants"),
        "missing inactive section: {body}"
    );
    assert!(
        body.contains("Rarog"),
        "missing inactive participant: {body}"
    );
    assert!(
        body.contains("Queued Suggestions"),
        "missing suggestion section: {body}"
    );
    assert!(
        body.contains("check the final answer"),
        "missing queued suggestion: {body}"
    );
}

#[test]
fn slash_notifications_opens_notifications_modal() {
    let (mut model, _daemon_rx) = make_model();
    model.connected = true;
    model.input.set_text("/notifications");

    let quit = model.handle_key(KeyCode::Enter, KeyModifiers::NONE);

    assert!(!quit);
    assert_eq!(model.modal.top(), Some(modal::ModalKind::Notifications));
}

#[test]
fn ctrl_n_opens_notifications_modal() {
    let (mut model, _daemon_rx) = make_model();

    let quit = model.handle_key(KeyCode::Char('n'), KeyModifiers::CONTROL);

    assert!(!quit);
    assert_eq!(model.modal.top(), Some(modal::ModalKind::Notifications));
}

#[test]
fn slash_approvals_opens_approval_center_modal() {
    let (mut model, _daemon_rx) = make_model();
    model.connected = true;
    model.input.set_text("/approvals");

    let quit = model.handle_key(KeyCode::Enter, KeyModifiers::NONE);

    assert!(!quit);
    assert_eq!(model.modal.top(), Some(modal::ModalKind::ApprovalCenter));
}

#[test]
fn prompt_viewer_down_scrolls_prompt_body() {
    let (mut model, _daemon_rx) = make_model();
    model.prompt_modal_snapshot = Some(crate::client::AgentPromptInspectionVm {
        agent_id: "swarog".to_string(),
        agent_name: "Svarog".to_string(),
        provider_id: "openai".to_string(),
        model: "gpt-5.4-mini".to_string(),
        sections: vec![crate::client::AgentPromptInspectionSectionVm {
            id: "base_prompt".to_string(),
            title: "Base Prompt".to_string(),
            content: (0..220)
                .map(|idx| format!("token-{idx}"))
                .collect::<Vec<_>>()
                .join(" "),
        }],
        final_prompt: (0..320)
            .map(|idx| format!("final-token-{idx}"))
            .collect::<Vec<_>>()
            .join(" "),
    });
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::PromptViewer));
    model.width = 120;
    model.height = 40;

    let quit = model.handle_key_modal(
        KeyCode::Down,
        KeyModifiers::NONE,
        modal::ModalKind::PromptViewer,
    );

    assert!(!quit);
    assert_eq!(model.prompt_modal_scroll, 1);
}

#[test]
fn status_viewer_down_scrolls_status_body() {
    let (mut model, _daemon_rx) = make_model();
    model.status_modal_snapshot = Some(crate::client::AgentStatusSnapshotVm {
        tier: "mission_control".to_string(),
        activity: "waiting_for_operator".to_string(),
        active_thread_id: Some("thread-1".to_string()),
        active_goal_run_id: None,
        active_goal_run_title: Some("Close release gap".to_string()),
        provider_health_json: r#"{"openai":{"can_execute":true,"trip_count":0}}"#.to_string(),
        gateway_statuses_json: r#"{"slack":{"status":"connected"}}"#.to_string(),
        recent_actions_json: serde_json::to_string(
            &(0..40)
                .map(|idx| {
                    serde_json::json!({
                        "action_type": format!("tool_{idx}"),
                        "summary": format!("summary {idx}"),
                        "timestamp": 1712345678_u64 + idx,
                    })
                })
                .collect::<Vec<_>>(),
        )
        .unwrap(),
    });
    model.status_modal_diagnostics_json = Some(
        serde_json::json!({
            "aline": {
                "available": true,
                "watcher_state": "running",
                "imported_count": 1,
                "generated_count": 1,
            }
        })
        .to_string(),
    );
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::Status));
    model.width = 120;
    model.height = 40;

    let quit = model.handle_key_modal(KeyCode::Down, KeyModifiers::NONE, modal::ModalKind::Status);

    assert!(!quit);
    assert_eq!(model.status_modal_scroll, 1);
}

#[test]
fn help_viewer_down_scrolls_help_body() {
    let (mut model, _daemon_rx) = make_model();
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::Help));
    model.width = 80;
    model.height = 20;

    let quit = model.handle_key_modal(KeyCode::Down, KeyModifiers::NONE, modal::ModalKind::Help);

    assert!(!quit);
    assert_eq!(model.help_modal_scroll, 1);
}

#[test]
fn statistics_modal_keyboard_cycles_tabs_and_filters() {
    let (mut model, mut daemon_rx) = make_model();
    model.connected = true;
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::Statistics));
    model.statistics_modal_snapshot = Some(amux_protocol::AgentStatisticsSnapshot {
        window: amux_protocol::AgentStatisticsWindow::All,
        generated_at: 1,
        has_incomplete_cost_history: false,
        totals: amux_protocol::AgentStatisticsTotals {
            input_tokens: 1,
            output_tokens: 2,
            total_tokens: 3,
            cost_usd: 0.1,
            provider_count: 1,
            model_count: 1,
        },
        providers: vec![amux_protocol::ProviderStatisticsRow {
            provider: "openai".to_string(),
            input_tokens: 1,
            output_tokens: 2,
            total_tokens: 3,
            cost_usd: 0.1,
        }],
        models: vec![amux_protocol::ModelStatisticsRow {
            provider: "openai".to_string(),
            model: "gpt-5.4-mini".to_string(),
            input_tokens: 1,
            output_tokens: 2,
            total_tokens: 3,
            cost_usd: 0.1,
        }],
        top_models_by_tokens: Vec::new(),
        top_models_by_cost: Vec::new(),
    });

    let quit = model.handle_key_modal(
        KeyCode::Right,
        KeyModifiers::NONE,
        modal::ModalKind::Statistics,
    );
    assert!(!quit);
    assert_eq!(
        model.statistics_modal_tab,
        crate::state::statistics::StatisticsTab::Providers
    );

    let quit = model.handle_key_modal(
        KeyCode::Char(']'),
        KeyModifiers::NONE,
        modal::ModalKind::Statistics,
    );
    assert!(!quit);
    assert_eq!(
        model.statistics_modal_window,
        amux_protocol::AgentStatisticsWindow::Today
    );
    match daemon_rx.try_recv() {
        Ok(DaemonCommand::RequestAgentStatistics { window }) => {
            assert_eq!(window, amux_protocol::AgentStatisticsWindow::Today);
        }
        other => panic!("expected statistics refetch, got {:?}", other),
    }
}

#[test]
fn command_palette_prompt_query_with_args_requests_target_agent() {
    let (mut model, mut daemon_rx) = make_model();
    model.connected = true;
    model.focus = FocusArea::Chat;

    let quit = model.handle_key(KeyCode::Char('/'), KeyModifiers::NONE);
    assert!(!quit);
    for ch in "prompt weles".chars() {
        let quit = model.handle_key(KeyCode::Char(ch), KeyModifiers::NONE);
        assert!(!quit);
    }
    let quit = model.handle_key(KeyCode::Enter, KeyModifiers::NONE);

    assert!(!quit);
    match daemon_rx.try_recv() {
        Ok(DaemonCommand::RequestPromptInspection { agent_id }) => {
            assert_eq!(agent_id.as_deref(), Some("weles"));
        }
        other => panic!(
            "expected prompt inspection request from command palette query, got {:?}",
            other
        ),
    }
}

#[test]
fn slash_command_can_restart_from_prompt_viewer_modal() {
    let (mut model, mut daemon_rx) = make_model();
    model.connected = true;
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::PromptViewer));

    for ch in "/prompt weles".chars() {
        let quit = model.handle_key(KeyCode::Char(ch), KeyModifiers::NONE);
        assert!(!quit);
    }
    let quit = model.handle_key(KeyCode::Enter, KeyModifiers::NONE);

    assert!(!quit);
    match daemon_rx.try_recv() {
        Ok(DaemonCommand::RequestPromptInspection { agent_id }) => {
            assert_eq!(agent_id.as_deref(), Some("weles"));
        }
        other => panic!(
            "expected prompt inspection request after restarting slash command, got {:?}",
            other
        ),
    }
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
        daemon_rx
            .try_recv()
            .expect("expected approval rules refresh command"),
        DaemonCommand::ListTaskApprovalRules
    ));
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
        daemon_rx
            .try_recv()
            .expect("expected approval rules refresh command"),
        DaemonCommand::ListTaskApprovalRules
    ));
    assert!(matches!(
        daemon_rx.try_recv().expect("expected approval resolution command"),
        DaemonCommand::ResolveTaskApproval {
            approval_id,
            decision
        } if approval_id == "approval-1" && decision == "allow_once"
    ));
}

#[test]
fn approval_center_keyboard_creates_always_approve_rule() {
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
                command: "orchestrator_policy_escalation".to_string(),
                risk_level: crate::state::RiskLevel::Medium,
                blast_radius: "thread".to_string(),
                received_at: 1,
                seen_at: None,
            },
        ));
    model.toggle_approval_center();

    let quit = model.handle_key_modal(
        KeyCode::Char('w'),
        KeyModifiers::NONE,
        modal::ModalKind::ApprovalCenter,
    );

    assert!(!quit);
    assert!(matches!(
        daemon_rx
            .try_recv()
            .expect("expected approval rules refresh command"),
        DaemonCommand::ListTaskApprovalRules
    ));
    assert!(matches!(
        daemon_rx
            .try_recv()
            .expect("expected rule creation command"),
        DaemonCommand::CreateTaskApprovalRule { approval_id } if approval_id == "approval-1"
    ));
    assert!(matches!(
        daemon_rx
            .try_recv()
            .expect("expected approval resolution command"),
        DaemonCommand::ResolveTaskApproval {
            approval_id,
            decision
        } if approval_id == "approval-1" && decision == "allow_once"
    ));
}

#[test]
fn approval_center_keyboard_revokes_saved_rule() {
    let (mut model, mut daemon_rx) = make_model();
    model
        .approval
        .reduce(crate::state::ApprovalAction::SetRules(vec![
            crate::state::approval::SavedApprovalRule {
                id: "rule-1".to_string(),
                command: "orchestrator_policy_escalation".to_string(),
                created_at: 1,
                last_used_at: Some(2),
                use_count: 3,
            },
        ]));
    model
        .approval
        .reduce(crate::state::ApprovalAction::SetFilter(
            crate::state::ApprovalFilter::SavedRules,
        ));
    model.toggle_approval_center();

    let quit = model.handle_key_modal(
        KeyCode::Char('r'),
        KeyModifiers::NONE,
        modal::ModalKind::ApprovalCenter,
    );

    assert!(!quit);
    assert!(matches!(
        daemon_rx
            .try_recv()
            .expect("expected approval rules refresh command"),
        DaemonCommand::ListTaskApprovalRules
    ));
    assert!(matches!(
        daemon_rx
            .try_recv()
            .expect("expected revoke rule command"),
        DaemonCommand::RevokeTaskApprovalRule { rule_id } if rule_id == "rule-1"
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
    assert!(defs
        .iter()
        .any(|provider| provider.id == PROVIDER_ID_OPENAI));
    assert!(defs
        .iter()
        .any(|provider| provider.id == PROVIDER_ID_CUSTOM));
    assert!(!defs.iter().any(|provider| provider.id == "groq"));
}

#[test]
fn model_command_skips_remote_fetch_for_static_provider_catalogs() {
    let (mut model, mut daemon_rx) = make_model();
    model.config.provider = PROVIDER_ID_ALIBABA_CODING_PLAN.to_string();
    model.config.base_url = "https://coding-intl.dashscope.aliyuncs.com/v1".to_string();
    model.config.model = "qwen3.6-plus".to_string();
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
        model: "qwen3.6-plus".to_string(),
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
fn selecting_compaction_weles_provider_updates_provider_and_opens_model_picker() {
    let (mut model, mut daemon_rx) = make_model();
    model.auth.entries = vec![
        crate::state::auth::ProviderAuthEntry {
            provider_id: PROVIDER_ID_OPENAI.to_string(),
            provider_name: "OpenAI".to_string(),
            authenticated: true,
            auth_source: "api_key".to_string(),
            model: "gpt-5.4".to_string(),
        },
        crate::state::auth::ProviderAuthEntry {
            provider_id: PROVIDER_ID_ALIBABA_CODING_PLAN.to_string(),
            provider_name: "Alibaba Coding Plan".to_string(),
            authenticated: true,
            auth_source: "api_key".to_string(),
            model: "qwen3.6-plus".to_string(),
        },
    ];

    let target_index = widgets::provider_picker::available_provider_defs(&model.auth)
        .iter()
        .position(|provider| provider.id == PROVIDER_ID_ALIBABA_CODING_PLAN)
        .expect("provider to exist");

    model.settings_picker_target = Some(SettingsPickerTarget::CompactionWelesProvider);
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::ProviderPicker));
    model.modal.set_picker_item_count(
        widgets::provider_picker::available_provider_defs(&model.auth).len(),
    );
    if target_index > 0 {
        model
            .modal
            .reduce(modal::ModalAction::Navigate(target_index as i32));
    }

    let quit = model.handle_key_modal(
        KeyCode::Enter,
        KeyModifiers::NONE,
        modal::ModalKind::ProviderPicker,
    );

    assert!(!quit);
    assert_eq!(
        model.config.compaction_weles_provider,
        PROVIDER_ID_ALIBABA_CODING_PLAN
    );
    assert_eq!(model.modal.top(), Some(modal::ModalKind::ModelPicker));
    while let Ok(command) = daemon_rx.try_recv() {
        if let DaemonCommand::FetchModels { .. } = command {
            panic!("static providers should not trigger remote model fetches");
        }
    }
}

#[test]
fn selecting_compaction_weles_model_updates_compaction_model() {
    let (mut model, _daemon_rx) = make_model();
    model.config.compaction_weles_provider = PROVIDER_ID_OPENAI.to_string();
    model
        .config
        .reduce(config::ConfigAction::ModelsFetched(vec![
            crate::state::config::FetchedModel {
                id: "gpt-5.4".to_string(),
                name: Some("GPT-5.4".to_string()),
                context_window: Some(128_000),
            },
            crate::state::config::FetchedModel {
                id: "gpt-5.4-mini".to_string(),
                name: Some("GPT-5.4 Mini".to_string()),
                context_window: Some(128_000),
            },
        ]));

    model.settings_picker_target = Some(SettingsPickerTarget::CompactionWelesModel);
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::ModelPicker));
    model.modal.reduce(modal::ModalAction::Navigate(1));

    let quit = model.handle_key_modal(
        KeyCode::Enter,
        KeyModifiers::NONE,
        modal::ModalKind::ModelPicker,
    );

    assert!(!quit);
    assert_eq!(model.config.compaction_weles_model, "gpt-5.4-mini");
}

#[test]
fn selecting_compaction_custom_provider_updates_provider_and_opens_model_picker() {
    let (mut model, mut daemon_rx) = make_model();
    model.auth.entries = vec![
        crate::state::auth::ProviderAuthEntry {
            provider_id: PROVIDER_ID_OPENAI.to_string(),
            provider_name: "OpenAI".to_string(),
            authenticated: true,
            auth_source: "api_key".to_string(),
            model: "gpt-5.4".to_string(),
        },
        crate::state::auth::ProviderAuthEntry {
            provider_id: PROVIDER_ID_ALIBABA_CODING_PLAN.to_string(),
            provider_name: "Alibaba Coding Plan".to_string(),
            authenticated: true,
            auth_source: "api_key".to_string(),
            model: "qwen3.6-plus".to_string(),
        },
    ];

    let target_index = widgets::provider_picker::available_provider_defs(&model.auth)
        .iter()
        .position(|provider| provider.id == PROVIDER_ID_ALIBABA_CODING_PLAN)
        .expect("provider to exist");

    model.settings_picker_target = Some(SettingsPickerTarget::CompactionCustomProvider);
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::ProviderPicker));
    model.modal.set_picker_item_count(
        widgets::provider_picker::available_provider_defs(&model.auth).len(),
    );
    if target_index > 0 {
        model
            .modal
            .reduce(modal::ModalAction::Navigate(target_index as i32));
    }

    let quit = model.handle_key_modal(
        KeyCode::Enter,
        KeyModifiers::NONE,
        modal::ModalKind::ProviderPicker,
    );

    assert!(!quit);
    assert_eq!(
        model.config.compaction_custom_provider,
        PROVIDER_ID_ALIBABA_CODING_PLAN
    );
    assert_eq!(model.modal.top(), Some(modal::ModalKind::ModelPicker));
    while let Ok(command) = daemon_rx.try_recv() {
        if let DaemonCommand::FetchModels { .. } = command {
            panic!("static providers should not trigger remote model fetches");
        }
    }
}

#[test]
fn selecting_compaction_custom_provider_copies_current_provider_transport() {
    let (mut model, _daemon_rx) = make_model();
    model.auth.entries = vec![crate::state::auth::ProviderAuthEntry {
        provider_id: PROVIDER_ID_QWEN.to_string(),
        provider_name: "Qwen".to_string(),
        authenticated: true,
        auth_source: "api_key".to_string(),
        model: "qwen-max".to_string(),
    }];
    model.config.provider = PROVIDER_ID_QWEN.to_string();
    model.config.auth_source = "api_key".to_string();
    model.config.api_transport = "chat_completions".to_string();

    model.settings_picker_target = Some(SettingsPickerTarget::CompactionCustomProvider);
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::ProviderPicker));
    model.modal.set_picker_item_count(
        widgets::provider_picker::available_provider_defs(&model.auth).len(),
    );

    let quit = model.handle_key_modal(
        KeyCode::Enter,
        KeyModifiers::NONE,
        modal::ModalKind::ProviderPicker,
    );

    assert!(!quit);
    assert_eq!(model.config.compaction_custom_provider, PROVIDER_ID_QWEN);
    assert_eq!(
        model.config.compaction_custom_api_transport,
        "chat_completions"
    );
}

#[test]
fn selecting_compaction_custom_model_updates_compaction_model() {
    let (mut model, _daemon_rx) = make_model();
    model.config.compaction_custom_provider = PROVIDER_ID_OPENAI.to_string();
    model
        .config
        .reduce(config::ConfigAction::ModelsFetched(vec![
            crate::state::config::FetchedModel {
                id: "gpt-5.4".to_string(),
                name: Some("GPT-5.4".to_string()),
                context_window: Some(128_000),
            },
            crate::state::config::FetchedModel {
                id: "gpt-5.4-mini".to_string(),
                name: Some("GPT-5.4 Mini".to_string()),
                context_window: Some(128_000),
            },
        ]));

    model.settings_picker_target = Some(SettingsPickerTarget::CompactionCustomModel);
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::ModelPicker));
    model.modal.reduce(modal::ModalAction::Navigate(1));

    let quit = model.handle_key_modal(
        KeyCode::Enter,
        KeyModifiers::NONE,
        modal::ModalKind::ModelPicker,
    );

    assert!(!quit);
    assert_eq!(model.config.compaction_custom_model, "gpt-5.4-mini");
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
        modal::ThreadPickerTab::Playgrounds
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
        modal::ThreadPickerTab::Playgrounds
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
        Ok(DaemonCommand::RequestThread {
            thread_id,
            message_limit,
            message_offset,
        }) => {
            assert_eq!(thread_id, "heartbeat-1");
            assert_eq!(message_limit, Some(50));
            assert_eq!(message_offset, Some(0));
        }
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
fn thread_picker_playgrounds_new_row_is_browse_only() {
    let (mut model, _daemon_rx) = make_model();
    model.chat.reduce(chat::ChatAction::ThreadListReceived(vec![
        chat::AgentThread {
            id: "playground:domowoj:thread-user".into(),
            title: "Participant Playground · Domowoj @ thread-user".into(),
            ..Default::default()
        },
    ]));
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::ThreadPicker));
    model
        .modal
        .set_thread_picker_tab(modal::ThreadPickerTab::Playgrounds);
    model.sync_thread_picker_item_count();

    let quit = model.handle_key_modal(
        KeyCode::Enter,
        KeyModifiers::NONE,
        modal::ModalKind::ThreadPicker,
    );

    assert!(!quit);
    assert_eq!(model.modal.top(), Some(modal::ModalKind::ThreadPicker));
    assert_eq!(model.chat.active_thread_id(), None);
    assert_eq!(model.status_line, "Playgrounds are created automatically");
}

#[test]
fn new_weles_conversation_uses_weles_profile_before_first_prompt() {
    let (mut model, _daemon_rx) = make_model();
    model.config.provider = "openai".to_string();
    model.config.model = "gpt-5.4".to_string();
    model.config.custom_model_name.clear();
    model.subagents.entries.push(crate::state::SubAgentEntry {
        id: "weles_builtin".to_string(),
        name: "WELES".to_string(),
        provider: "anthropic".to_string(),
        model: "claude-sonnet-4-5".to_string(),
        role: Some("testing".to_string()),
        enabled: true,
        builtin: true,
        immutable_identity: true,
        disable_allowed: false,
        delete_allowed: false,
        protected_reason: Some("Built-in reviewer".to_string()),
        reasoning_effort: Some("medium".to_string()),
        raw_json: Some(serde_json::json!({
            "id": "weles_builtin",
            "name": "WELES",
            "provider": "anthropic",
            "model": "claude-sonnet-4-5",
            "reasoning_effort": "medium"
        })),
    });

    model.start_new_thread_view_for_agent(Some("weles"));

    let profile = model.current_conversation_agent_profile();
    assert_eq!(profile.agent_label, "Weles");
    assert_eq!(profile.provider, "anthropic");
    assert_eq!(profile.model, "claude-sonnet-4-5");
    assert_eq!(profile.reasoning_effort.as_deref(), Some("medium"));
}

#[test]
fn new_weles_conversation_keeps_weles_profile_after_first_prompt_locally() {
    let (mut model, _daemon_rx) = make_model();
    model.connected = true;
    model.config.provider = "openai".to_string();
    model.config.model = "gpt-5.4".to_string();
    model.config.custom_model_name.clear();
    model.subagents.entries.push(crate::state::SubAgentEntry {
        id: "weles_builtin".to_string(),
        name: "WELES".to_string(),
        provider: "anthropic".to_string(),
        model: "claude-sonnet-4-5".to_string(),
        role: Some("testing".to_string()),
        enabled: true,
        builtin: true,
        immutable_identity: true,
        disable_allowed: false,
        delete_allowed: false,
        protected_reason: Some("Built-in reviewer".to_string()),
        reasoning_effort: Some("medium".to_string()),
        raw_json: None,
    });

    model.start_new_thread_view_for_agent(Some("weles"));
    model.submit_prompt("review this diff".to_string());

    let profile = model.current_conversation_agent_profile();
    assert_eq!(profile.agent_label, "Weles");
    assert_eq!(profile.provider, "anthropic");
    assert_eq!(profile.model, "claude-sonnet-4-5");
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
fn queued_prompts_modal_clicking_row_opens_prompt_viewer_with_full_message() {
    let (mut model, _daemon_rx) = make_model();
    model
        .queued_prompts
        .push(QueuedPrompt::new("preview line\nfull queued message body"));
    model.open_queued_prompts_modal();
    let (_, overlay_area) = model
        .current_modal_area()
        .expect("queued prompts modal should be visible");

    let row_pos = (overlay_area.y..overlay_area.y.saturating_add(overlay_area.height))
        .find_map(|row| {
            (overlay_area.x..overlay_area.x.saturating_add(overlay_area.width)).find_map(|column| {
                let pos = Position::new(column, row);
                if widgets::queued_prompts::hit_test(
                    overlay_area,
                    &model.queued_prompts,
                    model.modal.picker_cursor(),
                    model.tick_counter,
                    pos,
                ) == Some(widgets::queued_prompts::QueuedPromptsHitTarget::Row(0))
                {
                    Some(pos)
                } else {
                    None
                }
            })
        })
        .expect("queued prompt row should be clickable");

    model.handle_mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: row_pos.x,
        row: row_pos.y,
        modifiers: KeyModifiers::NONE,
    });

    assert_eq!(model.modal.top(), Some(modal::ModalKind::PromptViewer));
    assert!(
        model
            .prompt_modal_body()
            .contains("full queued message body"),
        "prompt viewer should show the full queued message body"
    );
}

#[test]
fn queued_prompts_modal_expand_action_opens_prompt_viewer_with_full_message() {
    let (mut model, _daemon_rx) = make_model();
    model
        .queued_prompts
        .push(QueuedPrompt::new("preview line\nexpanded via action"));
    model.open_queued_prompts_modal();
    let (_, overlay_area) = model
        .current_modal_area()
        .expect("queued prompts modal should be visible");

    let expand_pos = (overlay_area.y..overlay_area.y.saturating_add(overlay_area.height))
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
                    action: QueuedPromptAction::Expand,
                }) {
                    Some(pos)
                } else {
                    None
                }
            })
        })
        .expect("expand action should be clickable");

    model.handle_mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: expand_pos.x,
        row: expand_pos.y,
        modifiers: KeyModifiers::NONE,
    });

    assert_eq!(model.modal.top(), Some(modal::ModalKind::PromptViewer));
    assert!(
        model.prompt_modal_body().contains("expanded via action"),
        "expand action should open the full queued message"
    );
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
fn clicking_participant_summary_opens_thread_participants_modal() {
    let (mut model, _daemon_rx) = make_model();
    model.width = 120;
    model.height = 40;
    model.connected = true;
    model.handle_client_event(ClientEvent::ThreadDetail(Some(crate::wire::AgentThread {
        id: "thread-1".to_string(),
        title: "Participant Thread".to_string(),
        agent_name: Some("Svarog".to_string()),
        thread_participants: vec![crate::wire::ThreadParticipantState {
            agent_id: "weles".to_string(),
            agent_name: "Weles".to_string(),
            instruction: "verify claims".to_string(),
            status: "active".to_string(),
            created_at: 1,
            updated_at: 2,
            deactivated_at: None,
            last_contribution_at: Some(3),
            always_auto_response: false,
        }],
        ..Default::default()
    })));

    let chat_area = model.pane_layout().chat;
    let click = Position::new(chat_area.x.saturating_add(2), chat_area.y.saturating_add(1));

    model.handle_mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: click.x,
        row: click.y,
        modifiers: KeyModifiers::NONE,
    });

    assert_eq!(
        model.modal.top(),
        Some(modal::ModalKind::ThreadParticipants)
    );
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
