#[cfg(test)]
use super::*;
use tokio::sync::mpsc::unbounded_channel;

fn make_model() -> TuiModel {
    let (_event_tx, event_rx) = std::sync::mpsc::channel();
    let (daemon_tx, _daemon_rx) = unbounded_channel();
    TuiModel::new(event_rx, daemon_tx)
}

fn make_model_with_daemon_rx() -> (
    TuiModel,
    tokio::sync::mpsc::UnboundedReceiver<DaemonCommand>,
) {
    let (_event_tx, event_rx) = std::sync::mpsc::channel();
    let (daemon_tx, daemon_rx) = unbounded_channel();
    (TuiModel::new(event_rx, daemon_tx), daemon_rx)
}

#[test]
fn connected_event_defers_concierge_welcome_until_config_loads() {
    let (mut model, mut daemon_rx) = make_model_with_daemon_rx();

    model.handle_connected_event();

    let mut saw_refresh = false;
    let mut saw_refresh_services = false;
    while let Ok(command) = daemon_rx.try_recv() {
        match command {
            DaemonCommand::Refresh => saw_refresh = true,
            DaemonCommand::RefreshServices => saw_refresh_services = true,
            DaemonCommand::RequestConciergeWelcome => {
                panic!("concierge welcome should wait until config is loaded")
            }
            _ => {}
        }
    }

    assert!(saw_refresh, "connect should still request thread refresh");
    assert!(
        saw_refresh_services,
        "connect should still request service refresh including config"
    );
    assert!(
        !model.concierge.loading,
        "concierge loading should not start until welcome is actually requested"
    );
}

#[test]
fn first_raw_config_load_triggers_concierge_welcome_request() {
    let (mut model, mut daemon_rx) = make_model_with_daemon_rx();
    model.connected = true;
    model.agent_config_loaded = false;

    model.handle_agent_config_raw_event(serde_json::json!({
        "provider": "openai",
        "base_url": "https://api.openai.com/v1",
        "model": "gpt-5.4",
        "managed_execution": {
            "sandbox_enabled": false,
            "security_level": "yolo"
        }
    }));

    assert!(
        model.agent_config_loaded,
        "raw config should mark config as loaded"
    );
    assert_eq!(model.config.managed_security_level, "yolo");
    assert!(
        model.concierge.loading,
        "first config load should start concierge welcome"
    );
    assert!(matches!(
        daemon_rx
            .try_recv()
            .expect("expected concierge welcome request"),
        DaemonCommand::RequestConciergeWelcome
    ));
}

#[test]
fn whatsapp_qr_event_opens_modal_and_sets_ascii_payload() {
    let mut model = make_model();
    assert!(model.modal.top().is_none());

    model.handle_client_event(ClientEvent::WhatsAppLinkQr {
        ascii_qr: "██\n██".to_string(),
        expires_at_ms: Some(123),
    });

    assert_eq!(
        model.modal.top(),
        Some(crate::state::modal::ModalKind::WhatsAppLink)
    );
    assert_eq!(model.modal.whatsapp_link().ascii_qr(), Some("██\n██"));
    assert_eq!(model.modal.whatsapp_link().expires_at_ms(), Some(123));
}

#[test]
fn whatsapp_status_events_update_modal_state() {
    let mut model = make_model();
    model.handle_client_event(ClientEvent::WhatsAppLinkStatus {
        state: "connected".to_string(),
        phone: Some("+12065550123".to_string()),
        last_error: None,
    });
    assert_eq!(
        model.modal.whatsapp_link().phase(),
        crate::state::modal::WhatsAppLinkPhase::Connected
    );

    model.handle_client_event(ClientEvent::WhatsAppLinkError {
        message: "scan timeout".to_string(),
        recoverable: true,
    });
    assert_eq!(
        model.modal.whatsapp_link().phase(),
        crate::state::modal::WhatsAppLinkPhase::Error
    );
    assert!(model
        .modal
        .whatsapp_link()
        .status_text()
        .contains("scan timeout"));

    model.handle_client_event(ClientEvent::WhatsAppLinkDisconnected {
        reason: Some("socket closed".to_string()),
    });
    assert_eq!(
        model.modal.whatsapp_link().phase(),
        crate::state::modal::WhatsAppLinkPhase::Disconnected
    );
    assert!(model
        .modal
        .whatsapp_link()
        .status_text()
        .contains("socket closed"));
}

#[test]
fn operator_profile_workflow_warning_surfaces_retry_notice() {
    let mut model = make_model();
    model.handle_client_event(ClientEvent::WorkflowNotice {
        kind: "operator-profile-warning".to_string(),
        message: "Operator profile operation failed".to_string(),
        details: Some("{\"retry_action\":\"request_concierge_welcome\"}".to_string()),
    });
    let rendered = model
        .input_notice_style()
        .expect("warning should be visible");
    assert!(
        rendered.0.contains("Ctrl+R"),
        "warning notice should include retry hint"
    );
}

#[test]
fn status_diagnostics_warning_mentions_sync_state() {
    let mut model = make_model();
    model.handle_client_event(ClientEvent::StatusDiagnostics {
        operator_profile_sync_state: "dirty".to_string(),
        operator_profile_sync_dirty: true,
        operator_profile_scheduler_fallback: false,
    });
    assert!(
        model.status_line.contains("sync state: dirty"),
        "status line should expose dirty sync diagnostics"
    );
}

#[test]
fn repeated_gateway_status_does_not_keep_overwriting_status_line() {
    let mut model = make_model();
    model.status_line = "Prompt sent".to_string();

    model.handle_client_event(ClientEvent::GatewayStatus {
        platform: "discord".to_string(),
        status: "disconnected".to_string(),
        last_error: Some("socket closed".to_string()),
        consecutive_failures: 1,
    });
    assert_eq!(model.status_line, "🌐 Gateway discord: disconnected");

    model.status_line = "Prompt sent".to_string();
    model.handle_client_event(ClientEvent::GatewayStatus {
        platform: "discord".to_string(),
        status: "disconnected".to_string(),
        last_error: Some("socket closed".to_string()),
        consecutive_failures: 2,
    });

    assert_eq!(
        model.status_line, "Prompt sent",
        "repeated gateway status should not keep stealing the footer"
    );
}

#[test]
fn operator_profile_question_event_shows_onboarding_notice() {
    let mut model = make_model();
    model.handle_client_event(ClientEvent::OperatorProfileSessionStarted {
        session_id: "sess-1".to_string(),
        kind: "first_run_onboarding".to_string(),
    });
    model.handle_client_event(ClientEvent::OperatorProfileQuestion {
        session_id: "sess-1".to_string(),
        question_id: "name".to_string(),
        field_key: "name".to_string(),
        prompt: "What should I call you?".to_string(),
        input_kind: "text".to_string(),
        optional: false,
    });

    assert!(model.should_show_operator_profile_onboarding());
    assert_eq!(
        model
            .operator_profile
            .question
            .as_ref()
            .map(|q| q.field_key.as_str()),
        Some("name")
    );
}

#[test]
fn operator_profile_progress_requests_next_question() {
    let (_event_tx, event_rx) = std::sync::mpsc::channel();
    let (daemon_tx, mut daemon_rx) = unbounded_channel();
    let mut model = TuiModel::new(event_rx, daemon_tx);
    model.handle_client_event(ClientEvent::OperatorProfileSessionStarted {
        session_id: "sess-1".to_string(),
        kind: "first_run_onboarding".to_string(),
    });

    model.handle_client_event(ClientEvent::OperatorProfileProgress {
        session_id: "sess-1".to_string(),
        answered: 1,
        remaining: 2,
        completion_ratio: 0.33,
    });

    let mut found_next = false;
    while let Ok(command) = daemon_rx.try_recv() {
        if matches!(
            command,
            crate::state::DaemonCommand::NextOperatorProfileQuestion { .. }
        ) {
            found_next = true;
            break;
        }
    }
    assert!(found_next, "progress should trigger next-question command");
}

#[test]
fn weles_health_update_surfaces_degraded_status() {
    let mut model = make_model();

    model.handle_client_event(ClientEvent::WelesHealthUpdate {
        state: "degraded".to_string(),
        reason: Some("WELES review unavailable for guarded actions".to_string()),
        checked_at: 77,
    });

    assert_eq!(
        model
            .weles_health
            .as_ref()
            .map(|health| health.state.as_str()),
        Some("degraded")
    );
    assert!(
        model.status_line.contains("WELES degraded"),
        "status line should mention degraded WELES health"
    );
}

#[test]
fn models_fetched_updates_picker_count_for_open_model_picker() {
    let mut model = make_model();
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::ModelPicker));
    model.modal.set_picker_item_count(1);

    model.handle_client_event(ClientEvent::ModelsFetched(vec![
        crate::wire::FetchedModel {
            id: "m1".to_string(),
            name: Some("Model One".to_string()),
            context_window: Some(128_000),
        },
        crate::wire::FetchedModel {
            id: "m2".to_string(),
            name: Some("Model Two".to_string()),
            context_window: Some(128_000),
        },
        crate::wire::FetchedModel {
            id: "m3".to_string(),
            name: Some("Model Three".to_string()),
            context_window: Some(128_000),
        },
    ]));

    model.modal.reduce(modal::ModalAction::Navigate(1));
    model.modal.reduce(modal::ModalAction::Navigate(1));

    assert_eq!(model.modal.picker_cursor(), 2);
}

#[test]
fn done_event_persists_final_reasoning_into_chat_message() {
    let mut model = make_model();
    model.chat.reduce(chat::ChatAction::ThreadCreated {
        thread_id: "thread-1".to_string(),
        title: "Thread".to_string(),
    });
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-1".to_string()));
    model.chat.reduce(chat::ChatAction::Delta {
        thread_id: "thread-1".to_string(),
        content: "Answer".to_string(),
    });

    model.handle_client_event(ClientEvent::Done {
        thread_id: "thread-1".to_string(),
        input_tokens: 10,
        output_tokens: 20,
        cost: None,
        provider: Some("github-copilot".to_string()),
        model: Some("gpt-5.4".to_string()),
        tps: None,
        generation_ms: None,
        reasoning: Some("Final reasoning summary".to_string()),
    });

    let thread = model.chat.active_thread().expect("thread should exist");
    let last = thread
        .messages
        .last()
        .expect("assistant message should exist");
    assert_eq!(last.reasoning.as_deref(), Some("Final reasoning summary"));
}

#[test]
fn subagent_error_requests_refresh_to_clear_rejected_optimistic_state() {
    let (_event_tx, event_rx) = std::sync::mpsc::channel();
    let (daemon_tx, mut daemon_rx) = unbounded_channel();
    let mut model = TuiModel::new(event_rx, daemon_tx);
    model.subagents.entries = vec![crate::state::SubAgentEntry {
        id: "weles_builtin".to_string(),
        name: "Legacy WELES".to_string(),
        provider: "openai".to_string(),
        model: "gpt-5.4-mini".to_string(),
        role: Some("testing".to_string()),
        enabled: true,
        builtin: false,
        immutable_identity: false,
        disable_allowed: true,
        delete_allowed: true,
        protected_reason: None,
        reasoning_effort: None,
        raw_json: Some(serde_json::json!({
            "id": "weles_builtin",
            "name": "Legacy WELES"
        })),
    }];

    model.handle_client_event(ClientEvent::Error(
        "protected mutation: reserved built-in sub-agent".to_string(),
    ));

    assert_eq!(
        model.subagents.entries.len(),
        1,
        "stale optimistic entry remains until refresh arrives"
    );
    assert!(matches!(
        daemon_rx
            .try_recv()
            .expect("subagent error should request authoritative refresh"),
        DaemonCommand::ListSubAgents
    ));
}
