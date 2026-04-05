#[cfg(test)]
use super::*;
use amux_shared::providers::{PROVIDER_ID_GITHUB_COPILOT, PROVIDER_ID_OPENAI};
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
        "provider": PROVIDER_ID_OPENAI,
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
    let mut saw_welcome = false;
    while let Ok(command) = daemon_rx.try_recv() {
        if matches!(command, DaemonCommand::RequestConciergeWelcome) {
            saw_welcome = true;
            break;
        }
    }
    assert!(saw_welcome, "expected concierge welcome request");
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
fn upgrade_notification_updates_status_line_and_inbox() {
    let mut model = make_model();

    model.handle_client_event(ClientEvent::NotificationUpsert(
        amux_protocol::TamuxUpdateStatus::from_versions("0.2.3", "0.2.4")
            .expect("status should parse valid versions")
            .into_notification(100),
    ));

    assert_eq!(model.notifications.unread_count(), 1);
    assert_eq!(model.status_line, "🔔 tamux 0.2.4 is available");
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
fn approval_required_in_current_thread_opens_blocking_modal() {
    let mut model = make_model();
    model.chat.reduce(chat::ChatAction::ThreadCreated {
        thread_id: "thread-1".to_string(),
        title: "Active Thread".to_string(),
    });
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-1".to_string()));
    model.handle_task_list_event(vec![crate::wire::AgentTask {
        id: "task-1".to_string(),
        title: "WELES review".to_string(),
        thread_id: Some("thread-1".to_string()),
        status: Some(crate::wire::TaskStatus::AwaitingApproval),
        awaiting_approval_id: Some("approval-1".to_string()),
        ..Default::default()
    }]);

    model.handle_client_event(ClientEvent::ApprovalRequired {
        approval_id: "approval-1".to_string(),
        command: "git push".to_string(),
        rationale: Some("Push release branch to origin".to_string()),
        reasons: vec!["network access requested".to_string()],
        risk_level: "high".to_string(),
        blast_radius: "repo".to_string(),
    });

    assert_eq!(model.modal.top(), Some(modal::ModalKind::ApprovalOverlay));
    assert_eq!(model.approval.selected_approval_id(), Some("approval-1"));
}

#[test]
fn approval_required_in_background_thread_shows_notice_without_modal() {
    let mut model = make_model();
    model.chat.reduce(chat::ChatAction::ThreadCreated {
        thread_id: "thread-1".to_string(),
        title: "Active Thread".to_string(),
    });
    model.chat.reduce(chat::ChatAction::ThreadCreated {
        thread_id: "thread-2".to_string(),
        title: "Background Thread".to_string(),
    });
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-1".to_string()));
    model.handle_task_list_event(vec![crate::wire::AgentTask {
        id: "task-2".to_string(),
        title: "WELES review".to_string(),
        thread_id: Some("thread-2".to_string()),
        status: Some(crate::wire::TaskStatus::AwaitingApproval),
        awaiting_approval_id: Some("approval-2".to_string()),
        ..Default::default()
    }]);

    model.handle_client_event(ClientEvent::ApprovalRequired {
        approval_id: "approval-2".to_string(),
        command: "git clone".to_string(),
        rationale: Some("Clone support repository into workspace".to_string()),
        reasons: vec!["network access requested".to_string()],
        risk_level: "medium".to_string(),
        blast_radius: "workspace".to_string(),
    });

    assert_eq!(model.modal.top(), None);
    assert_eq!(model.approval.pending_approvals().len(), 1);
    assert!(model
        .input_notice_style()
        .expect("approval banner should be visible")
        .0
        .contains("Ctrl+A"));
}

#[test]
fn task_list_hydrates_pending_approvals_from_awaiting_approval_tasks() {
    let mut model = make_model();
    model.chat.reduce(chat::ChatAction::ThreadCreated {
        thread_id: "thread-1".to_string(),
        title: "Hydrated Thread".to_string(),
    });

    model.handle_task_list_event(vec![crate::wire::AgentTask {
        id: "task-1".to_string(),
        title: "Hydrated approval".to_string(),
        thread_id: Some("thread-1".to_string()),
        status: Some(crate::wire::TaskStatus::AwaitingApproval),
        awaiting_approval_id: Some("approval-1".to_string()),
        blocked_reason: Some("waiting for operator approval".to_string()),
        ..Default::default()
    }]);

    let approval = model
        .approval
        .approval_by_id("approval-1")
        .expect("task snapshot should hydrate approval queue");
    assert_eq!(approval.task_id, "task-1");
    assert_eq!(approval.thread_title.as_deref(), Some("Hydrated Thread"));
}

#[test]
fn task_list_hydrates_policy_escalation_rationale_from_thread_messages() {
    let mut model = make_model();
    model.handle_thread_detail_event(crate::wire::AgentThread {
        id: "thread-1".to_string(),
        title: "Hydrated Thread".to_string(),
        messages: vec![crate::wire::AgentMessage {
            role: crate::wire::MessageRole::System,
            content: "Policy escalation requested operator guidance: Cloning scientific skills repository from GitHub as part of WELES governance review task".to_string(),
            ..Default::default()
        }],
        ..Default::default()
    });

    model.handle_task_list_event(vec![crate::wire::AgentTask {
        id: "task-1".to_string(),
        title: "WELES".to_string(),
        thread_id: Some("thread-1".to_string()),
        status: Some(crate::wire::TaskStatus::AwaitingApproval),
        awaiting_approval_id: Some("approval-1".to_string()),
        blocked_reason: Some(
            "waiting for operator approval: orchestrator_policy_escalation".to_string(),
        ),
        ..Default::default()
    }]);

    let approval = model
        .approval
        .approval_by_id("approval-1")
        .expect("task snapshot should hydrate approval queue");
    assert_eq!(
        approval.rationale.as_deref(),
        Some(
            "Cloning scientific skills repository from GitHub as part of WELES governance review task"
        )
    );
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
        provider: Some(PROVIDER_ID_GITHUB_COPILOT.to_string()),
        model: Some("gpt-5.4".to_string()),
        tps: None,
        generation_ms: None,
        reasoning: Some("Final reasoning summary".to_string()),
        provider_final_result_json: Some("result_json".to_string()),
    });

    let thread = model.chat.active_thread().expect("thread should exist");
    let last = thread
        .messages
        .last()
        .expect("assistant message should exist");
    assert_eq!(last.reasoning.as_deref(), Some("Final reasoning summary"));
}

#[test]
fn internal_dm_thread_created_does_not_hijack_active_thread() {
    let mut model = make_model();
    model.chat.reduce(chat::ChatAction::ThreadCreated {
        thread_id: "thread-user".to_string(),
        title: "User Thread".to_string(),
    });
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-user".to_string()));

    model.handle_client_event(ClientEvent::ThreadCreated {
        thread_id: "dm:svarog:weles".to_string(),
        title: "Internal DM · Swarog ↔ WELES".to_string(),
        agent_name: None,
    });

    assert_eq!(model.chat.active_thread_id(), Some("thread-user"));
}

#[test]
fn hidden_handoff_thread_created_does_not_hijack_active_thread() {
    let mut model = make_model();
    model.chat.reduce(chat::ChatAction::ThreadCreated {
        thread_id: "thread-user".to_string(),
        title: "User Thread".to_string(),
    });
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-user".to_string()));

    model.handle_client_event(ClientEvent::ThreadCreated {
        thread_id: "handoff:thread-user:handoff-1".to_string(),
        title: "Handoff · Svarog -> Weles".to_string(),
        agent_name: None,
    });

    assert_eq!(model.chat.active_thread_id(), Some("thread-user"));
    assert!(
        model
            .chat
            .threads()
            .iter()
            .all(|thread| thread.id != "handoff:thread-user:handoff-1"),
        "hidden handoff threads should not be added to visible chat state"
    );
}

#[test]
fn thread_created_event_preserves_agent_name_for_responder_fallback() {
    let mut model = make_model();

    model.handle_client_event(ClientEvent::ThreadCreated {
        thread_id: "thread-weles".to_string(),
        title: "Governance".to_string(),
        agent_name: Some("Weles".to_string()),
    });

    let thread = model
        .chat
        .threads()
        .iter()
        .find(|thread| thread.id == "thread-weles")
        .expect("thread should be added to chat state");

    assert_eq!(thread.agent_name.as_deref(), Some("Weles"));
}

#[test]
fn hidden_handoff_thread_detail_is_ignored() {
    let mut model = make_model();
    model.chat.reduce(chat::ChatAction::ThreadCreated {
        thread_id: "thread-user".to_string(),
        title: "User Thread".to_string(),
    });
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-user".to_string()));

    model.handle_client_event(ClientEvent::ThreadDetail(Some(crate::wire::AgentThread {
        id: "handoff:thread-user:handoff-1".to_string(),
        title: "Handoff · Svarog -> Weles".to_string(),
        messages: vec![crate::wire::AgentMessage {
            role: crate::wire::MessageRole::System,
            content: "{\"kind\":\"thread_handoff_context\"}".to_string(),
            timestamp: 1,
            message_kind: "normal".to_string(),
            ..Default::default()
        }],
        created_at: 1,
        updated_at: 1,
        ..Default::default()
    })));

    assert_eq!(model.chat.active_thread_id(), Some("thread-user"));
    assert!(
        model
            .chat
            .threads()
            .iter()
            .all(|thread| thread.id != "handoff:thread-user:handoff-1"),
        "hidden handoff thread detail should not populate visible chat state"
    );
}

#[test]
fn hidden_handoff_threads_are_filtered_from_thread_list() {
    let mut model = make_model();

    model.handle_client_event(ClientEvent::ThreadList(vec![
        crate::wire::AgentThread {
            id: "thread-user".to_string(),
            title: "User Thread".to_string(),
            ..Default::default()
        },
        crate::wire::AgentThread {
            id: "handoff:thread-user:handoff-1".to_string(),
            title: "Handoff · Svarog -> Weles".to_string(),
            ..Default::default()
        },
    ]));

    let visible_ids: Vec<&str> = model
        .chat
        .threads()
        .iter()
        .map(|thread| thread.id.as_str())
        .collect();
    assert_eq!(visible_ids, vec!["thread-user"]);
}

#[test]
fn internal_dm_threads_are_retained_in_thread_list_for_internal_picker() {
    let mut model = make_model();

    model.handle_client_event(ClientEvent::ThreadList(vec![
        crate::wire::AgentThread {
            id: "thread-user".to_string(),
            title: "User Thread".to_string(),
            ..Default::default()
        },
        crate::wire::AgentThread {
            id: "dm:svarog:weles".to_string(),
            title: "Internal DM · Svarog ↔ WELES".to_string(),
            ..Default::default()
        },
    ]));

    let visible_ids: Vec<&str> = model
        .chat
        .threads()
        .iter()
        .map(|thread| thread.id.as_str())
        .collect();
    assert_eq!(visible_ids, vec!["thread-user", "dm:svarog:weles"]);
}

#[test]
fn hidden_handoff_thread_reload_required_is_ignored() {
    let (mut model, mut daemon_rx) = make_model_with_daemon_rx();

    model.handle_client_event(ClientEvent::ThreadReloadRequired {
        thread_id: "handoff:thread-user:handoff-1".to_string(),
    });

    assert!(daemon_rx.try_recv().is_err());
}

#[test]
fn selected_internal_dm_thread_detail_is_loaded() {
    let mut model = make_model();
    model.chat.reduce(chat::ChatAction::ThreadCreated {
        thread_id: "dm:svarog:weles".to_string(),
        title: "Internal DM · Svarog ↔ WELES".to_string(),
    });
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("dm:svarog:weles".to_string()));

    model.handle_client_event(ClientEvent::ThreadDetail(Some(crate::wire::AgentThread {
        id: "dm:svarog:weles".to_string(),
        title: "Internal DM · Svarog ↔ WELES".to_string(),
        messages: vec![crate::wire::AgentMessage {
            role: crate::wire::MessageRole::Assistant,
            content: "Keep reviewing the migration plan.".to_string(),
            timestamp: 1,
            message_kind: "normal".to_string(),
            ..Default::default()
        }],
        created_at: 1,
        updated_at: 1,
        ..Default::default()
    })));

    let thread = model
        .chat
        .threads()
        .iter()
        .find(|thread| thread.id == "dm:svarog:weles")
        .expect("selected internal dm thread should remain in chat state");
    assert_eq!(model.chat.active_thread_id(), Some("dm:svarog:weles"));
    assert_eq!(thread.messages.len(), 1);
    assert_eq!(thread.messages[0].content, "Keep reviewing the migration plan.");
}

#[test]
fn internal_dm_tool_activity_does_not_block_normal_thread_completion() {
    let mut model = make_model();
    model.chat.reduce(chat::ChatAction::ThreadCreated {
        thread_id: "thread-user".to_string(),
        title: "User Thread".to_string(),
    });
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-user".to_string()));

    model.handle_client_event(ClientEvent::ToolCall {
        thread_id: "dm:svarog:weles".to_string(),
        call_id: "internal-call".to_string(),
        name: "message_agent".to_string(),
        arguments: "{}".to_string(),
        weles_review: None,
    });
    assert!(
        model.chat.active_tool_calls().is_empty(),
        "internal tool calls should not enter the visible running-tool tracker"
    );
    model.handle_client_event(ClientEvent::ToolCall {
        thread_id: "thread-user".to_string(),
        call_id: "user-call".to_string(),
        name: "bash_command".to_string(),
        arguments: "{\"command\":\"pwd\"}".to_string(),
        weles_review: None,
    });
    assert_eq!(model.chat.active_tool_calls().len(), 1);

    model.handle_client_event(ClientEvent::ThreadCreated {
        thread_id: "dm:svarog:weles".to_string(),
        title: "Internal DM · Swarog ↔ WELES".to_string(),
        agent_name: None,
    });
    assert_eq!(model.chat.active_thread_id(), Some("thread-user"));

    model.handle_client_event(ClientEvent::Done {
        thread_id: "thread-user".to_string(),
        input_tokens: 0,
        output_tokens: 0,
        cost: None,
        provider: None,
        model: None,
        tps: None,
        generation_ms: None,
        reasoning: None,
        provider_final_result_json: Some("result_json".to_string()),
    });

    assert!(
        model.chat.active_tool_calls().is_empty(),
        "visible thread completion should still clear running tools"
    );
    assert_eq!(model.chat.active_thread_id(), Some("thread-user"));
}

#[test]
fn queued_prompt_flushes_after_last_tool_result_before_turn_done() {
    let (mut model, mut daemon_rx) = make_model_with_daemon_rx();
    model.connected = true;
    model.concierge.auto_cleanup_on_navigate = false;
    model.chat.reduce(chat::ChatAction::ThreadCreated {
        thread_id: "thread-1".to_string(),
        title: "Thread".to_string(),
    });
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-1".to_string()));

    model.handle_client_event(ClientEvent::ToolCall {
        thread_id: "thread-1".to_string(),
        call_id: "call-1".to_string(),
        name: "bash_command".to_string(),
        arguments: "{\"command\":\"pwd\"}".to_string(),
        weles_review: None,
    });

    model.submit_prompt("stay on the migration task".to_string());
    assert_eq!(model.queued_prompts.len(), 1);
    assert!(daemon_rx.try_recv().is_err());

    model.handle_client_event(ClientEvent::ToolResult {
        thread_id: "thread-1".to_string(),
        call_id: "call-1".to_string(),
        name: "bash_command".to_string(),
        content: "/repo".to_string(),
        is_error: false,
        weles_review: None,
    });

    match daemon_rx.try_recv() {
        Ok(DaemonCommand::SendMessage {
            thread_id, content, ..
        }) => {
            assert_eq!(thread_id.as_deref(), Some("thread-1"));
            assert_eq!(content, "stay on the migration task");
        }
        other => panic!("expected queued send after tool result, got {:?}", other),
    }
    assert!(
        model.queued_prompts.is_empty(),
        "queued prompt should flush as soon as the last tool finishes"
    );
}

#[test]
fn prompt_during_text_stream_without_running_tools_waits_for_done() {
    let (mut model, mut daemon_rx) = make_model_with_daemon_rx();
    model.connected = true;
    model.concierge.auto_cleanup_on_navigate = false;
    model.chat.reduce(chat::ChatAction::ThreadCreated {
        thread_id: "thread-1".to_string(),
        title: "Thread".to_string(),
    });
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-1".to_string()));

    model.handle_client_event(ClientEvent::Delta {
        thread_id: "thread-1".to_string(),
        content: "Partial answer".to_string(),
    });
    assert!(
        model.chat.active_tool_calls().is_empty(),
        "plain streaming should not fabricate running tools"
    );

    model.submit_prompt("switch to the auth bug instead".to_string());
    assert_eq!(model.queued_prompts.len(), 1);
    assert!(daemon_rx.try_recv().is_err());

    model.handle_client_event(ClientEvent::Done {
        thread_id: "thread-1".to_string(),
        input_tokens: 10,
        output_tokens: 20,
        cost: None,
        provider: None,
        model: None,
        tps: None,
        generation_ms: None,
        reasoning: None,
        provider_final_result_json: Some("result_json".to_string()),
    });

    match daemon_rx.try_recv() {
        Ok(DaemonCommand::SendMessage {
            thread_id, content, ..
        }) => {
            assert_eq!(thread_id.as_deref(), Some("thread-1"));
            assert_eq!(content, "switch to the auth bug instead");
        }
        other => panic!(
            "expected queued send after done when text is streaming, got {:?}",
            other
        ),
    }
    assert!(
        model.queued_prompts.is_empty(),
        "message should flush once the streaming assistant message completes"
    );
}

#[test]
fn follow_up_prompt_after_cancel_keeps_processing_new_events_on_same_thread() {
    let (mut model, mut daemon_rx) = make_model_with_daemon_rx();
    model.connected = true;
    model.concierge.auto_cleanup_on_navigate = false;
    model.chat.reduce(chat::ChatAction::ThreadCreated {
        thread_id: "thread-1".to_string(),
        title: "Thread".to_string(),
    });
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-1".to_string()));

    model.handle_client_event(ClientEvent::Delta {
        thread_id: "thread-1".to_string(),
        content: "Partial answer".to_string(),
    });
    model.cancelled_thread_id = Some("thread-1".to_string());
    model.chat.reduce(chat::ChatAction::ForceStopStreaming);

    model.submit_prompt("follow up".to_string());

    match daemon_rx.try_recv() {
        Ok(DaemonCommand::SendMessage {
            thread_id, content, ..
        }) => {
            assert_eq!(thread_id.as_deref(), Some("thread-1"));
            assert_eq!(content, "follow up");
        }
        other => panic!("expected follow-up send on same thread, got {:?}", other),
    }

    model.handle_client_event(ClientEvent::Delta {
        thread_id: "thread-1".to_string(),
        content: "Visible answer".to_string(),
    });

    assert_eq!(
        model.chat.streaming_content(),
        "Visible answer",
        "new stream chunks on the same thread should not be dropped after a cancelled turn"
    );
}

#[test]
fn subagent_error_requests_refresh_to_clear_rejected_optimistic_state() {
    let (_event_tx, event_rx) = std::sync::mpsc::channel();
    let (daemon_tx, mut daemon_rx) = unbounded_channel();
    let mut model = TuiModel::new(event_rx, daemon_tx);
    model.subagents.entries = vec![crate::state::SubAgentEntry {
        id: "weles_builtin".to_string(),
        name: "Legacy WELES".to_string(),
        provider: PROVIDER_ID_OPENAI.to_string(),
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

#[test]
fn openai_codex_auth_events_update_config_and_modal_state() {
    let (mut model, mut daemon_rx) = make_model_with_daemon_rx();

    model.handle_client_event(ClientEvent::OpenAICodexAuthStatus(
        crate::client::OpenAICodexAuthStatusVm {
            available: false,
            auth_mode: Some("chatgpt_subscription".to_string()),
            account_id: None,
            expires_at: None,
            source: Some("tamux-daemon".to_string()),
            error: None,
            auth_url: None,
            status: Some("pending".to_string()),
        },
    ));

    assert!(!model.config.chatgpt_auth_available);
    assert_eq!(
        model.config.chatgpt_auth_source.as_deref(),
        Some("tamux-daemon")
    );

    model.handle_client_event(ClientEvent::OpenAICodexAuthLoginResult(
        crate::client::OpenAICodexAuthStatusVm {
            available: false,
            auth_mode: Some("chatgpt_subscription".to_string()),
            account_id: None,
            expires_at: None,
            source: Some("tamux-daemon".to_string()),
            error: None,
            auth_url: Some("https://auth.openai.com/oauth/authorize?flow=tui".to_string()),
            status: Some("pending".to_string()),
        },
    ));

    assert_eq!(
        model.modal.top(),
        Some(crate::state::modal::ModalKind::OpenAIAuth)
    );
    assert_eq!(
        model.openai_auth_url.as_deref(),
        Some("https://auth.openai.com/oauth/authorize?flow=tui")
    );
    assert!(model
        .openai_auth_status_text
        .as_deref()
        .is_some_and(|text| text.contains("complete ChatGPT authentication")));
    assert!(matches!(
        daemon_rx
            .try_recv()
            .expect("expected auth state refresh after status"),
        DaemonCommand::GetProviderAuthStates
    ));
    assert!(matches!(
        daemon_rx
            .try_recv()
            .expect("expected auth state refresh after login"),
        DaemonCommand::GetProviderAuthStates
    ));

    model.handle_client_event(ClientEvent::OpenAICodexAuthLogoutResult {
        ok: true,
        error: None,
    });

    assert!(!model.config.chatgpt_auth_available);
    assert!(model.config.chatgpt_auth_source.is_none());
    assert!(model.openai_auth_url.is_none());
    assert!(model.openai_auth_status_text.is_none());
    assert!(matches!(
        daemon_rx
            .try_recv()
            .expect("expected auth state refresh after logout"),
        DaemonCommand::GetProviderAuthStates
    ));
    assert_eq!(model.status_line, "ChatGPT subscription auth cleared");
}

#[test]
fn connected_event_requests_openai_codex_auth_status_from_daemon() {
    let (mut model, mut daemon_rx) = make_model_with_daemon_rx();

    model.handle_connected_event();

    let mut saw_auth_status = false;
    while let Ok(command) = daemon_rx.try_recv() {
        if matches!(command, DaemonCommand::GetOpenAICodexAuthStatus) {
            saw_auth_status = true;
            break;
        }
    }

    assert!(saw_auth_status, "connect should request codex auth status");
}

#[test]
fn provider_auth_states_overlay_chatgpt_auth_when_openai_is_configured_for_chatgpt_subscription() {
    let mut model = make_model();
    model.config.provider = PROVIDER_ID_OPENAI.to_string();
    model.config.auth_source = "chatgpt_subscription".to_string();
    model.config.chatgpt_auth_available = true;
    model.config.chatgpt_auth_source = Some("tamux-daemon".to_string());

    model.handle_provider_auth_states_event(vec![crate::state::ProviderAuthEntry {
        provider_id: PROVIDER_ID_OPENAI.to_string(),
        provider_name: "OpenAI".to_string(),
        authenticated: false,
        auth_source: "api_key".to_string(),
        model: "gpt-5.4".to_string(),
    }]);

    let openai = model
        .auth
        .entries
        .iter()
        .find(|entry| entry.provider_id == PROVIDER_ID_OPENAI)
        .expect("openai auth entry should exist");
    assert!(
        openai.authenticated,
        "chatgpt daemon auth should surface as connected"
    );
    assert_eq!(openai.auth_source, "chatgpt_subscription");
}

#[test]
fn openai_codex_auth_status_event_clears_stale_modal_state() {
    let (mut model, mut daemon_rx) = make_model_with_daemon_rx();
    model.openai_auth_url = Some("https://stale.example/login".to_string());
    model.openai_auth_status_text = Some("stale".to_string());
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::Settings));
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::OpenAIAuth));
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::CommandPalette));

    model.handle_client_event(ClientEvent::OpenAICodexAuthStatus(
        crate::client::OpenAICodexAuthStatusVm {
            available: false,
            auth_mode: Some("chatgpt_subscription".to_string()),
            account_id: None,
            expires_at: None,
            source: Some("tamux-daemon".to_string()),
            error: Some("Timed out waiting for callback".to_string()),
            auth_url: None,
            status: Some("error".to_string()),
        },
    ));

    assert!(model.openai_auth_url.is_none());
    assert_eq!(
        model.openai_auth_status_text.as_deref(),
        Some("Timed out waiting for callback")
    );
    assert_eq!(
        model.modal.top(),
        Some(crate::state::modal::ModalKind::OpenAIAuth)
    );
    model.close_top_modal();
    assert_eq!(
        model.modal.top(),
        Some(crate::state::modal::ModalKind::CommandPalette)
    );
    assert!(matches!(
        daemon_rx
            .try_recv()
            .expect("expected auth state refresh after status"),
        DaemonCommand::GetProviderAuthStates
    ));
    assert_eq!(model.status_line, "Timed out waiting for callback");
}

#[test]
fn openai_codex_auth_status_event_removes_all_stale_nested_openai_modals() {
    let mut model = make_model();
    model.openai_auth_url = Some("https://stale.example/login".to_string());
    model.openai_auth_status_text = Some("stale".to_string());
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::Settings));
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::OpenAIAuth));
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::CommandPalette));
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::OpenAIAuth));

    model.handle_client_event(ClientEvent::OpenAICodexAuthStatus(
        crate::client::OpenAICodexAuthStatusVm {
            available: false,
            auth_mode: Some("chatgpt_subscription".to_string()),
            account_id: None,
            expires_at: None,
            source: Some("tamux-daemon".to_string()),
            error: Some("Timed out waiting for callback".to_string()),
            auth_url: None,
            status: Some("error".to_string()),
        },
    ));

    assert_eq!(model.modal.top(), Some(modal::ModalKind::OpenAIAuth));
    model.close_top_modal();
    assert_eq!(model.modal.top(), Some(modal::ModalKind::CommandPalette));
    model.close_top_modal();
    assert_eq!(model.modal.top(), Some(modal::ModalKind::Settings));
}

#[test]
fn disconnect_and_reconnect_clear_openai_auth_modal_even_when_nested() {
    let mut model = make_model();
    model.openai_auth_url = Some("https://auth.openai.com/oauth/authorize?flow=tui".to_string());
    model.openai_auth_status_text = Some("pending".to_string());
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::Settings));
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::OpenAIAuth));
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::CommandPalette));

    model.handle_disconnected_event();

    assert!(model.openai_auth_url.is_none());
    assert!(model.openai_auth_status_text.is_none());
    assert_eq!(model.modal.top(), Some(modal::ModalKind::CommandPalette));

    model.openai_auth_url = Some("https://auth.openai.com/oauth/authorize?flow=tui".to_string());
    model.openai_auth_status_text = Some("pending".to_string());
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::OpenAIAuth));

    model.handle_reconnecting_event(3);

    assert!(model.openai_auth_url.is_none());
    assert!(model.openai_auth_status_text.is_none());
    assert_eq!(model.modal.top(), Some(modal::ModalKind::CommandPalette));
}
