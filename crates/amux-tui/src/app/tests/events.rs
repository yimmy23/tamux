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

fn next_thread_request(
    daemon_rx: &mut tokio::sync::mpsc::UnboundedReceiver<DaemonCommand>,
) -> Option<(String, Option<usize>, Option<usize>)> {
    while let Ok(command) = daemon_rx.try_recv() {
        if let DaemonCommand::RequestThread {
            thread_id,
            message_limit,
            message_offset,
        } = command
        {
            return Some((thread_id, message_limit, message_offset));
        }
    }
    None
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
fn collaboration_sessions_event_populates_workspace_state_without_error_modal() {
    let mut model = make_model();

    model.handle_collaboration_sessions_event(
        serde_json::json!([
            {
                "id": "session-1",
                "parent_task_id": "task-1",
                "agents": [{"role": "research"}, {"role": "testing"}],
                "disagreements": [
                    {
                        "id": "disagreement-1",
                        "topic": "deployment strategy",
                        "positions": ["roll forward", "roll back"],
                        "votes": [{"task_id": "subagent-1"}]
                    }
                ]
            }
        ])
        .to_string(),
    );

    assert!(matches!(model.main_pane_view, MainPaneView::Collaboration));
    assert_eq!(model.modal.top(), None);
    assert!(!model.error_active);
    assert_eq!(model.status_line, "Collaboration sessions loaded");
    assert_eq!(model.collaboration.rows().len(), 2);
    assert_eq!(
        model
            .collaboration
            .selected_row()
            .and_then(crate::state::collaboration::CollaborationRowVm::disagreement_id),
        Some("disagreement-1")
    );
}

#[test]
fn collaboration_vote_result_requests_refresh_and_updates_status() {
    let (mut model, mut daemon_rx) = make_model_with_daemon_rx();

    model.handle_client_event(ClientEvent::CollaborationVoteResult {
        report_json: serde_json::json!({
            "session_id": "session-1",
            "resolution": "resolved"
        })
        .to_string(),
    });

    assert_eq!(model.status_line, "Vote recorded: resolved.");
    assert!(matches!(
        daemon_rx
            .try_recv()
            .expect("expected collaboration refresh after vote result"),
        DaemonCommand::GetCollaborationSessions
    ));
}

#[test]
fn collaboration_sessions_event_surfaces_escalation_notice() {
    let mut model = make_model();

    model.handle_collaboration_sessions_event(
        serde_json::json!([
            {
                "id": "session-1",
                "parent_task_id": "task-1",
                "disagreements": [
                    {
                        "id": "disagreement-1",
                        "topic": "deployment strategy",
                        "positions": ["roll forward", "roll back"],
                        "resolution": "pending",
                        "confidence_gap": 0.1,
                        "votes": []
                    }
                ]
            }
        ])
        .to_string(),
    );

    let notice = model
        .input_notice_style()
        .expect("escalation should surface a notice");
    assert!(notice.0.contains("Collaboration escalation"));
    assert!(
        model
            .collaboration
            .selected_session()
            .and_then(|session| session.escalation.as_ref())
            .is_some(),
        "session should carry escalation summary for workspace rendering"
    );
}

#[test]
fn operator_question_event_appends_inline_message_and_actions_without_modal() {
    let mut model = make_model();
    model.chat.reduce(chat::ChatAction::ThreadCreated {
        thread_id: "thread-1".to_string(),
        title: "Thread".to_string(),
    });
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-1".to_string()));

    model.handle_client_event(ClientEvent::OperatorQuestion {
        question_id: "oq-1".to_string(),
        content: "Approve this slice?\nA - proceed\nB - revise".to_string(),
        options: vec!["A".to_string(), "B".to_string()],
        session_id: None,
        thread_id: Some("thread-1".to_string()),
    });

    let thread = model.chat.active_thread().expect("thread should exist");
    assert_eq!(
        thread.messages.len(),
        1,
        "operator question should append an inline transcript message"
    );
    let message = thread
        .messages
        .last()
        .expect("question message should exist");
    assert!(message.is_operator_question);
    assert_eq!(message.operator_question_id.as_deref(), Some("oq-1"));
    assert_eq!(
        message.content,
        "Approve this slice?\nA - proceed\nB - revise"
    );
    assert_eq!(message.actions.len(), 2);
    assert_eq!(message.actions[0].label, "A");
    assert_eq!(message.actions[1].label, "B");
    assert_eq!(model.modal.top(), None);
    assert_ne!(
        model.modal.top(),
        Some(modal::ModalKind::OperatorQuestionOverlay)
    );
}

#[test]
fn operator_question_resolved_event_marks_message_answered_and_clears_actions() {
    let mut model = make_model();
    model.chat.reduce(chat::ChatAction::ThreadCreated {
        thread_id: "thread-1".to_string(),
        title: "Thread".to_string(),
    });
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-1".to_string()));
    model.chat.reduce(chat::ChatAction::AppendMessage {
        thread_id: "thread-1".to_string(),
        message: chat::AgentMessage {
            role: chat::MessageRole::Assistant,
            content: "Approve this slice?\nA - proceed\nB - revise".to_string(),
            is_operator_question: true,
            operator_question_id: Some("oq-1".to_string()),
            actions: vec![chat::MessageAction {
                label: "A".to_string(),
                action_type: "operator_question_answer:oq-1:A".to_string(),
                thread_id: Some("thread-1".to_string()),
            }],
            ..Default::default()
        },
    });

    model.handle_client_event(ClientEvent::OperatorQuestionResolved {
        question_id: "oq-1".to_string(),
        answer: "A".to_string(),
    });

    let thread = model.chat.active_thread().expect("thread should exist");
    assert_eq!(thread.messages.len(), 1);
    let message = thread
        .messages
        .last()
        .expect("question message should exist");
    assert_eq!(message.operator_question_answer.as_deref(), Some("A"));
    assert!(message.actions.is_empty());
}

#[test]
fn operator_question_event_does_not_replace_existing_modal() {
    let mut model = make_model();
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::CommandPalette));
    model.chat.reduce(chat::ChatAction::ThreadCreated {
        thread_id: "thread-1".to_string(),
        title: "Thread".to_string(),
    });
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-1".to_string()));

    model.handle_client_event(ClientEvent::OperatorQuestion {
        question_id: "oq-1".to_string(),
        content: "Approve this slice?\nA - proceed\nB - revise".to_string(),
        options: vec!["A".to_string(), "B".to_string()],
        session_id: None,
        thread_id: Some("thread-1".to_string()),
    });

    let thread = model.chat.active_thread().expect("thread should exist");
    assert_eq!(
        thread.messages.len(),
        1,
        "operator question should append an inline transcript message"
    );
    let message = thread
        .messages
        .last()
        .expect("question message should exist");
    assert!(message.is_operator_question);
    assert_eq!(message.operator_question_id.as_deref(), Some("oq-1"));
    assert_eq!(
        message.content,
        "Approve this slice?\nA - proceed\nB - revise"
    );
    assert_eq!(message.actions.len(), 2);
    assert_eq!(message.actions[0].label, "A");
    assert_eq!(message.actions[1].label, "B");
    assert_eq!(model.modal.top(), Some(modal::ModalKind::CommandPalette));
    model.modal.reduce(modal::ModalAction::Pop);
    assert_eq!(model.modal.top(), None);
    assert_ne!(
        model.modal.top(),
        Some(modal::ModalKind::OperatorQuestionOverlay)
    );
}

#[test]
fn operator_profile_workflow_warning_surfaces_retry_notice() {
    let mut model = make_model();
    model.handle_client_event(ClientEvent::WorkflowNotice {
        thread_id: None,
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
fn auto_compaction_workflow_notice_requests_page_containing_artifact() {
    let (mut model, mut daemon_rx) = make_model_with_daemon_rx();

    model.handle_client_event(ClientEvent::ThreadCreated {
        thread_id: "thread-compaction".to_string(),
        title: "Compaction".to_string(),
        agent_name: Some("Swarog".to_string()),
    });
    model.chat.reduce(chat::ChatAction::SelectThread(
        "thread-compaction".to_string(),
    ));

    model.handle_client_event(ClientEvent::WorkflowNotice {
        thread_id: Some("thread-compaction".to_string()),
        kind: "auto-compaction".to_string(),
        message: "Auto compaction applied using heuristic.".to_string(),
        details: Some("{\"split_at\":20,\"total_message_count\":121}".to_string()),
    });

    let (thread_id, message_limit, message_offset) =
        next_thread_request(&mut daemon_rx).expect("expected targeted thread reload request");
    assert_eq!(thread_id, "thread-compaction");
    assert_eq!(message_limit, Some(chat::CHAT_HISTORY_PAGE_SIZE));
    assert_eq!(message_offset, Some(100));
}

#[test]
fn status_diagnostics_warning_mentions_sync_state() {
    let mut model = make_model();
    model.handle_client_event(ClientEvent::StatusDiagnostics {
        operator_profile_sync_state: "dirty".to_string(),
        operator_profile_sync_dirty: true,
        operator_profile_scheduler_fallback: false,
        diagnostics_json: r#"{"operator_profile_sync_state":"dirty","aline":{"available":true}}"#
            .to_string(),
    });
    assert!(
        model.status_line.contains("sync state: dirty"),
        "status line should expose dirty sync diagnostics"
    );
    assert!(
        model
            .status_modal_diagnostics_json
            .as_deref()
            .is_some_and(|diagnostics| diagnostics.contains("\"aline\"")),
        "status diagnostics should retain the raw payload for the status modal"
    );
}

#[test]
fn status_diagnostics_warning_mentions_mesh_state() {
    let mut model = make_model();
    model.handle_client_event(ClientEvent::StatusDiagnostics {
        operator_profile_sync_state: "clean".to_string(),
        operator_profile_sync_dirty: false,
        operator_profile_scheduler_fallback: false,
        diagnostics_json: r#"{"skill_mesh":{"backend":"mesh","state":"degraded"}}"#.to_string(),
    });
    assert!(
        model.status_line.contains("skill mesh: degraded"),
        "status line should expose degraded mesh diagnostics"
    );
}

#[test]
fn full_status_event_caches_snapshot_for_status_modal() {
    let mut model = make_model();

    model.handle_client_event(ClientEvent::StatusSnapshot(
        crate::client::AgentStatusSnapshotVm {
            tier: "mission_control".to_string(),
            activity: "waiting_for_operator".to_string(),
            active_thread_id: Some("thread-1".to_string()),
            active_goal_run_id: Some("goal-1".to_string()),
            active_goal_run_title: Some("Close release gap".to_string()),
            provider_health_json: r#"{"openai":{"can_execute":true,"trip_count":0}}"#.to_string(),
            gateway_statuses_json: r#"{"slack":{"status":"connected"}}"#.to_string(),
            recent_actions_json:
                r#"[{"action_type":"tool_call","summary":"Ran status","timestamp":1712345678}]"#
                    .to_string(),
        },
    ));
    model.handle_client_event(ClientEvent::StatusDiagnostics {
        operator_profile_sync_state: "clean".to_string(),
        operator_profile_sync_dirty: false,
        operator_profile_scheduler_fallback: false,
        diagnostics_json: r#"{"aline":{"available":true,"watcher_state":"running"}}"#.to_string(),
    });

    let snapshot = model
        .status_modal_snapshot
        .as_ref()
        .expect("status snapshot should be cached");
    assert_eq!(snapshot.tier, "mission_control");
    assert_eq!(snapshot.activity, "waiting_for_operator");
    assert_eq!(snapshot.active_thread_id.as_deref(), Some("thread-1"));
    assert_eq!(model.recent_actions.len(), 1);
    assert_eq!(model.recent_actions[0].summary, "Ran status");
    assert!(model.status_modal_body().contains("Watcher:"));
}

#[test]
fn status_query_failure_resolves_loading_modal() {
    let mut model = make_model();
    model.status_modal_loading = true;

    model.handle_client_event(ClientEvent::Error("boom".to_string()));

    assert!(!model.status_modal_loading);
}

#[test]
fn status_modal_failure_replaces_loading_body_with_error_text() {
    let mut model = make_model();
    model.open_status_modal_loading();

    model.handle_client_event(ClientEvent::Error("daemon unavailable".to_string()));

    assert!(model.status_modal_body().contains("daemon unavailable"));
}

#[test]
fn status_modal_latest_response_replaces_stale_content() {
    let mut model = make_model();
    model.open_status_modal_loading();
    model.handle_client_event(ClientEvent::StatusDiagnostics {
        operator_profile_sync_state: "older".to_string(),
        operator_profile_sync_dirty: true,
        operator_profile_scheduler_fallback: false,
        diagnostics_json: r#"{"aline":{"available":false,"watcher_state":"unknown"}}"#.to_string(),
    });
    model.handle_client_event(ClientEvent::StatusSnapshot(
        crate::client::AgentStatusSnapshotVm {
            tier: "mission_control".to_string(),
            activity: "older".to_string(),
            active_thread_id: None,
            active_goal_run_id: None,
            active_goal_run_title: None,
            provider_health_json: "{}".to_string(),
            gateway_statuses_json: "{}".to_string(),
            recent_actions_json: "[]".to_string(),
        },
    ));
    model.handle_client_event(ClientEvent::StatusDiagnostics {
        operator_profile_sync_state: "clean".to_string(),
        operator_profile_sync_dirty: false,
        operator_profile_scheduler_fallback: false,
        diagnostics_json: r#"{"aline":{"available":true,"watcher_state":"running"}}"#.to_string(),
    });
    model.handle_client_event(ClientEvent::StatusSnapshot(
        crate::client::AgentStatusSnapshotVm {
            tier: "mission_control".to_string(),
            activity: "newer".to_string(),
            active_thread_id: None,
            active_goal_run_id: None,
            active_goal_run_title: None,
            provider_health_json: "{}".to_string(),
            gateway_statuses_json: "{}".to_string(),
            recent_actions_json: "[]".to_string(),
        },
    ));

    assert!(model.status_modal_body().contains("newer"));
    assert!(model.status_modal_body().contains("running"));
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
fn unrelated_sync_does_not_clear_event_backed_pending_approval() {
    let mut model = make_model();

    model.handle_client_event(ClientEvent::ApprovalRequired {
        approval_id: "approval-1".to_string(),
        command: "git push".to_string(),
        rationale: Some("Push branch".to_string()),
        reasons: vec!["network access requested".to_string()],
        risk_level: "high".to_string(),
        blast_radius: "repo".to_string(),
    });

    model.handle_thread_list_event(vec![crate::wire::AgentThread {
        id: "thread-1".to_string(),
        title: "Thread".to_string(),
        ..Default::default()
    }]);

    assert!(
        model.approval.approval_by_id("approval-1").is_some(),
        "thread sync should not discard live approvals without an explicit resolution"
    );
}

#[test]
fn task_list_clears_approval_when_same_task_no_longer_waits_for_it() {
    let mut model = make_model();

    model.handle_task_list_event(vec![crate::wire::AgentTask {
        id: "task-1".to_string(),
        title: "Hydrated approval".to_string(),
        thread_id: Some("thread-1".to_string()),
        status: Some(crate::wire::TaskStatus::AwaitingApproval),
        awaiting_approval_id: Some("approval-1".to_string()),
        blocked_reason: Some("waiting for operator approval".to_string()),
        ..Default::default()
    }]);
    assert!(model.approval.approval_by_id("approval-1").is_some());

    model.handle_task_list_event(vec![crate::wire::AgentTask {
        id: "task-1".to_string(),
        title: "Hydrated approval".to_string(),
        thread_id: Some("thread-1".to_string()),
        status: Some(crate::wire::TaskStatus::Queued),
        awaiting_approval_id: None,
        ..Default::default()
    }]);

    assert!(
        model.approval.approval_by_id("approval-1").is_none(),
        "task snapshot should clear approvals only when the same task explicitly drops them"
    );
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
fn done_event_requests_authoritative_thread_refresh_for_participant_threads() {
    let (mut model, mut daemon_rx) = make_model_with_daemon_rx();

    model.handle_client_event(ClientEvent::ThreadDetail(Some(crate::wire::AgentThread {
        id: "thread-1".to_string(),
        title: "Thread".to_string(),
        messages: vec![crate::wire::AgentMessage {
            role: crate::wire::MessageRole::User,
            content: "hello".to_string(),
            timestamp: 1,
            ..Default::default()
        }],
        thread_participants: vec![crate::wire::ThreadParticipantState {
            agent_id: "weles".to_string(),
            agent_name: "Weles".to_string(),
            instruction: "verify claims".to_string(),
            status: "active".to_string(),
            created_at: 1,
            updated_at: 1,
            last_contribution_at: None,
            deactivated_at: None,
        }],
        created_at: 1,
        updated_at: 1,
        ..Default::default()
    })));

    while daemon_rx.try_recv().is_ok() {}

    model.handle_client_event(ClientEvent::Done {
        thread_id: "thread-1".to_string(),
        input_tokens: 10,
        output_tokens: 20,
        cost: None,
        provider: Some(PROVIDER_ID_GITHUB_COPILOT.to_string()),
        model: Some("gpt-5.4".to_string()),
        tps: None,
        generation_ms: None,
        reasoning: None,
        provider_final_result_json: Some("result_json".to_string()),
    });

    match next_thread_request(&mut daemon_rx) {
        Some((thread_id, message_limit, message_offset)) => {
            assert_eq!(thread_id, "thread-1");
            assert_eq!(message_limit, Some(chat::CHAT_HISTORY_PAGE_SIZE));
            assert_eq!(message_offset, Some(0));
        }
        other => panic!("expected authoritative thread refresh request, got {other:?}"),
    }
}

#[test]
fn stale_retry_status_after_done_does_not_restore_retrying_placeholder() {
    let mut model = make_model();
    model.chat.reduce(chat::ChatAction::ThreadCreated {
        thread_id: "thread-1".to_string(),
        title: "Thread".to_string(),
    });
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-1".to_string()));
    model.chat.reduce(chat::ChatAction::AppendMessage {
        thread_id: "thread-1".to_string(),
        message: chat::AgentMessage {
            role: chat::MessageRole::User,
            content: "Retry this".to_string(),
            ..Default::default()
        },
    });

    model.handle_client_event(ClientEvent::RetryStatus {
        thread_id: "thread-1".to_string(),
        phase: "retrying".to_string(),
        attempt: 1,
        max_retries: 3,
        delay_ms: 1_000,
        failure_class: "temporary_upstream".to_string(),
        message: "upstream timeout".to_string(),
    });
    assert_eq!(model.agent_activity.as_deref(), Some("retrying"));
    assert!(model.chat.retry_status().is_some());

    model.handle_client_event(ClientEvent::Delta {
        thread_id: "thread-1".to_string(),
        content: "Recovered answer".to_string(),
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
        reasoning: None,
        provider_final_result_json: Some("result_json".to_string()),
    });

    assert!(
        model.agent_activity.is_none(),
        "done should clear the retrying placeholder"
    );
    assert!(
        model.chat.retry_status().is_none(),
        "done should clear visible retry status"
    );

    model.handle_client_event(ClientEvent::RetryStatus {
        thread_id: "thread-1".to_string(),
        phase: "retrying".to_string(),
        attempt: 1,
        max_retries: 3,
        delay_ms: 1_000,
        failure_class: "temporary_upstream".to_string(),
        message: "late retry status".to_string(),
    });

    assert!(
        model.agent_activity.is_none(),
        "late retry events for a completed turn must not restore the retrying placeholder"
    );
    assert!(
        model.chat.retry_status().is_none(),
        "late retry events for a completed turn must not restore retry status UI"
    );
}

#[test]
fn header_uses_rarog_daemon_runtime_metadata_after_first_reply() {
    let mut model = make_model();
    model.concierge.provider = Some("alibaba-coding-plan".to_string());
    model.concierge.model = Some("qwen3.6-plus".to_string());
    model.concierge.reasoning_effort = Some("none".to_string());

    model.handle_client_event(ClientEvent::ThreadCreated {
        thread_id: "thread-rarog".to_string(),
        title: "Rarog Thread".to_string(),
        agent_name: Some("Rarog".to_string()),
    });
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-rarog".to_string()));
    model.handle_client_event(ClientEvent::Delta {
        thread_id: "thread-rarog".to_string(),
        content: "Runtime answer".to_string(),
    });

    model.handle_client_event(ClientEvent::Done {
        thread_id: "thread-rarog".to_string(),
        input_tokens: 10,
        output_tokens: 20,
        cost: None,
        provider: Some("alibaba-coding-plan".to_string()),
        model: Some("MiniMax-M2.5".to_string()),
        tps: None,
        generation_ms: None,
        reasoning: None,
        provider_final_result_json: Some(r#"{"reasoning":{"effort":"low"}}"#.to_string()),
    });

    let profile = model.current_header_agent_profile();
    assert_eq!(profile.agent_label, "Rarog");
    assert_eq!(profile.provider, "alibaba-coding-plan");
    assert_eq!(profile.model, "MiniMax-M2.5");
    assert_eq!(profile.reasoning_effort.as_deref(), Some("low"));
}

#[test]
fn header_usage_summary_uses_runtime_model_context_window_for_rarog() {
    let mut model = make_model();
    model.concierge.provider = Some("alibaba-coding-plan".to_string());
    model.concierge.model = Some("qwen3.6-plus".to_string());

    model.handle_client_event(ClientEvent::ThreadCreated {
        thread_id: "thread-rarog-usage".to_string(),
        title: "Rarog Thread".to_string(),
        agent_name: Some("Rarog".to_string()),
    });
    model.chat.reduce(chat::ChatAction::SelectThread(
        "thread-rarog-usage".to_string(),
    ));
    model.handle_client_event(ClientEvent::Delta {
        thread_id: "thread-rarog-usage".to_string(),
        content: "Runtime answer".to_string(),
    });
    model.handle_client_event(ClientEvent::Done {
        thread_id: "thread-rarog-usage".to_string(),
        input_tokens: 10,
        output_tokens: 20,
        cost: Some(0.25),
        provider: Some("alibaba-coding-plan".to_string()),
        model: Some("MiniMax-M2.5".to_string()),
        tps: None,
        generation_ms: None,
        reasoning: None,
        provider_final_result_json: None,
    });

    let usage = model.current_header_usage_summary();
    assert_eq!(usage.context_window_tokens, 205_000);
    assert_eq!(usage.compaction_target_tokens, 164_000);
    let total_cost = usage
        .total_cost_usd
        .expect("header should expose total cost");
    assert!(
        (total_cost - 0.25).abs() < 1e-9,
        "expected summed total cost to be 0.25, got {total_cost}"
    );
    assert!(usage.current_tokens > 0);
    assert!(usage.utilization_pct <= 100);
}

#[test]
fn header_usage_summary_uses_primary_threshold_for_heuristic_compaction() {
    let mut model = make_model();
    model.config.provider = PROVIDER_ID_GITHUB_COPILOT.to_string();
    model.config.auth_source = "github_copilot".to_string();
    model.config.model = "gpt-5.4".to_string();
    model.config.context_window_tokens = 400_000;
    model.config.compact_threshold_pct = 80;
    model.config.compaction_strategy = "heuristic".to_string();

    model.handle_client_event(ClientEvent::ThreadCreated {
        thread_id: "thread-heuristic-target".to_string(),
        title: "Heuristic".to_string(),
        agent_name: Some("Swarog".to_string()),
    });
    model.chat.reduce(chat::ChatAction::SelectThread(
        "thread-heuristic-target".to_string(),
    ));
    model.chat.reduce(chat::ChatAction::AppendMessage {
        thread_id: "thread-heuristic-target".to_string(),
        message: chat::AgentMessage {
            role: chat::MessageRole::User,
            content: "A".repeat(20_000),
            ..Default::default()
        },
    });

    let usage = model.current_header_usage_summary();
    assert_eq!(
        usage.compaction_target_tokens, 320_000,
        "heuristic compaction should use the main model threshold"
    );
    assert_eq!(usage.context_window_tokens, 400_000);
}

#[test]
fn header_usage_summary_caps_target_by_weles_compaction_window() {
    let mut model = make_model();
    model.config.provider = PROVIDER_ID_GITHUB_COPILOT.to_string();
    model.config.auth_source = "github_copilot".to_string();
    model.config.model = "gpt-5.4".to_string();
    model.config.context_window_tokens = 400_000;
    model.config.compact_threshold_pct = 80;
    model.config.compaction_strategy = "weles".to_string();
    model.config.compaction_weles_provider = "minimax-coding-plan".to_string();
    model.config.compaction_weles_model = "MiniMax-M2.7".to_string();

    model.handle_client_event(ClientEvent::ThreadCreated {
        thread_id: "thread-weles-target".to_string(),
        title: "Weles".to_string(),
        agent_name: Some("Swarog".to_string()),
    });
    model.chat.reduce(chat::ChatAction::SelectThread(
        "thread-weles-target".to_string(),
    ));

    let usage = model.current_header_usage_summary();
    assert_eq!(usage.compaction_target_tokens, 164_000);
    assert_eq!(usage.context_window_tokens, 400_000);
}

#[test]
fn header_usage_summary_caps_target_by_custom_compaction_window() {
    let mut model = make_model();
    model.config.provider = PROVIDER_ID_GITHUB_COPILOT.to_string();
    model.config.auth_source = "github_copilot".to_string();
    model.config.model = "gpt-5.4".to_string();
    model.config.context_window_tokens = 400_000;
    model.config.compact_threshold_pct = 80;
    model.config.compaction_strategy = "custom_model".to_string();
    model.config.compaction_custom_context_window_tokens = 160_000;

    model.handle_client_event(ClientEvent::ThreadCreated {
        thread_id: "thread-custom-target".to_string(),
        title: "Custom".to_string(),
        agent_name: Some("Swarog".to_string()),
    });
    model.chat.reduce(chat::ChatAction::SelectThread(
        "thread-custom-target".to_string(),
    ));

    let usage = model.current_header_usage_summary();
    assert_eq!(usage.compaction_target_tokens, 128_000);
    assert_eq!(usage.context_window_tokens, 400_000);
}

#[test]
fn header_usage_summary_resets_active_window_after_compaction_artifact() {
    let mut model = make_model();

    model.handle_client_event(ClientEvent::ThreadCreated {
        thread_id: "thread-compaction".to_string(),
        title: "Compaction".to_string(),
        agent_name: Some("Swarog".to_string()),
    });
    model.chat.reduce(chat::ChatAction::SelectThread(
        "thread-compaction".to_string(),
    ));

    model.chat.reduce(chat::ChatAction::AppendMessage {
        thread_id: "thread-compaction".to_string(),
        message: chat::AgentMessage {
            role: chat::MessageRole::User,
            content: "A".repeat(4_000),
            ..Default::default()
        },
    });
    model.chat.reduce(chat::ChatAction::AppendMessage {
        thread_id: "thread-compaction".to_string(),
        message: chat::AgentMessage {
            role: chat::MessageRole::Assistant,
            content: "B".repeat(4_000),
            cost: Some(0.10),
            ..Default::default()
        },
    });

    let before = model.current_header_usage_summary();

    model.chat.reduce(chat::ChatAction::AppendMessage {
        thread_id: "thread-compaction".to_string(),
        message: chat::AgentMessage {
            role: chat::MessageRole::Assistant,
            content: "rule based".to_string(),
            message_kind: "compaction_artifact".to_string(),
            compaction_payload: Some("Older context compacted for continuity".to_string()),
            ..Default::default()
        },
    });
    model.chat.reduce(chat::ChatAction::AppendMessage {
        thread_id: "thread-compaction".to_string(),
        message: chat::AgentMessage {
            role: chat::MessageRole::Assistant,
            content: "short follow-up".to_string(),
            cost: Some(0.15),
            ..Default::default()
        },
    });

    let after = model.current_header_usage_summary();
    assert!(
        after.current_tokens < before.current_tokens,
        "active context usage should drop after compaction: before={} after={}",
        before.current_tokens,
        after.current_tokens
    );
    let total_cost = after
        .total_cost_usd
        .expect("header should include summed total cost after compaction");
    assert!(
        (total_cost - 0.25).abs() < 1e-9,
        "expected summed total cost to stay at 0.25, got {total_cost}"
    );
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
fn thread_list_requests_detail_for_selected_thread_with_only_summary_data() {
    let (mut model, mut daemon_rx) = make_model_with_daemon_rx();
    model.config.tui_chat_history_page_size = 123;

    model.chat.reduce(chat::ChatAction::ThreadListReceived(vec![
        crate::state::chat::AgentThread {
            id: "thread-user".to_string(),
            title: "User Thread".to_string(),
            ..Default::default()
        },
    ]));
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-user".to_string()));

    model.handle_client_event(ClientEvent::ThreadList(vec![crate::wire::AgentThread {
        id: "thread-user".to_string(),
        title: "User Thread".to_string(),
        ..Default::default()
    }]));

    assert_eq!(model.thread_loading_id.as_deref(), Some("thread-user"));
    match daemon_rx.try_recv() {
        Ok(DaemonCommand::RequestThread {
            thread_id,
            message_limit,
            message_offset,
        }) => {
            assert_eq!(thread_id, "thread-user");
            assert_eq!(message_limit, Some(123));
            assert_eq!(message_offset, Some(0));
        }
        other => panic!("expected thread detail request, got {other:?}"),
    }
}

#[test]
fn thread_detail_clears_loading_state() {
    let mut model = make_model();
    model.thread_loading_id = Some("thread-user".to_string());

    model.handle_client_event(ClientEvent::ThreadDetail(Some(crate::wire::AgentThread {
        id: "thread-user".to_string(),
        title: "User Thread".to_string(),
        messages: vec![crate::wire::AgentMessage {
            role: crate::wire::MessageRole::Assistant,
            content: "Loaded".to_string(),
            timestamp: 1,
            message_kind: "normal".to_string(),
            ..Default::default()
        }],
        created_at: 1,
        updated_at: 1,
        ..Default::default()
    })));

    assert!(model.thread_loading_id.is_none());
}

#[test]
fn on_tick_requests_next_older_thread_page_when_scrolled_to_top_of_loaded_window() {
    let (mut model, mut daemon_rx) = make_model_with_daemon_rx();
    model.config.tui_chat_history_page_size = 123;
    model.chat.reduce(chat::ChatAction::ThreadDetailReceived(
        crate::state::chat::AgentThread {
            id: "thread-user".to_string(),
            title: "User Thread".to_string(),
            total_message_count: 120,
            loaded_message_start: 20,
            loaded_message_end: 120,
            messages: (20..120)
                .map(|index| crate::state::chat::AgentMessage {
                    id: Some(format!("msg-{index}")),
                    role: crate::state::chat::MessageRole::Assistant,
                    content: format!("msg {index}"),
                    ..Default::default()
                })
                .collect(),
            ..Default::default()
        },
    ));
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-user".to_string()));
    model
        .chat
        .reduce(chat::ChatAction::ScrollChat(i32::MAX / 2));

    model.on_tick();

    match daemon_rx.try_recv() {
        Ok(DaemonCommand::RequestThread {
            thread_id,
            message_limit,
            message_offset,
        }) => {
            assert_eq!(thread_id, "thread-user");
            assert_eq!(message_limit, Some(123));
            assert_eq!(message_offset, Some(100));
        }
        other => panic!("expected older-page request, got {other:?}"),
    }
}

#[test]
fn on_tick_debounces_follow_up_older_thread_page_requests_after_reload() {
    let (mut model, mut daemon_rx) = make_model_with_daemon_rx();
    model.config.tui_chat_history_page_size = 123;
    model.chat.reduce(chat::ChatAction::ThreadDetailReceived(
        crate::state::chat::AgentThread {
            id: "thread-user".to_string(),
            title: "User Thread".to_string(),
            total_message_count: 120,
            loaded_message_start: 20,
            loaded_message_end: 120,
            messages: (20..120)
                .map(|index| crate::state::chat::AgentMessage {
                    id: Some(format!("msg-{index}")),
                    role: crate::state::chat::MessageRole::Assistant,
                    content: format!("msg {index}"),
                    ..Default::default()
                })
                .collect(),
            ..Default::default()
        },
    ));
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-user".to_string()));
    model
        .chat
        .reduce(chat::ChatAction::ScrollChat(i32::MAX / 2));

    model.on_tick();

    match next_thread_request(&mut daemon_rx) {
        Some((thread_id, message_limit, message_offset)) => {
            assert_eq!(thread_id, "thread-user");
            assert_eq!(message_limit, Some(123));
            assert_eq!(message_offset, Some(100));
        }
        other => panic!("expected first older-page request, got {other:?}"),
    }

    model.handle_client_event(ClientEvent::ThreadDetail(Some(crate::wire::AgentThread {
        id: "thread-user".to_string(),
        title: "User Thread".to_string(),
        total_message_count: 240,
        loaded_message_start: 5,
        loaded_message_end: 128,
        messages: (5..128)
            .map(|index| crate::wire::AgentMessage {
                id: Some(format!("msg-{index}")),
                role: crate::wire::MessageRole::Assistant,
                content: format!("msg {index}"),
                timestamp: index as u64,
                message_kind: "normal".to_string(),
                ..Default::default()
            })
            .collect(),
        created_at: 1,
        updated_at: 1,
        ..Default::default()
    })));

    model.on_tick();
    assert!(
        next_thread_request(&mut daemon_rx).is_none(),
        "top-of-window reload should debounce follow-up history fetches"
    );

    for _ in 0..(chat::CHAT_HISTORY_FETCH_DEBOUNCE_TICKS - 1) {
        model.on_tick();
    }

    match next_thread_request(&mut daemon_rx) {
        Some((thread_id, message_limit, message_offset)) => {
            assert_eq!(thread_id, "thread-user");
            assert_eq!(message_limit, Some(123));
            assert_eq!(message_offset, Some(235));
        }
        other => panic!("expected debounced older-page request after cooldown, got {other:?}"),
    }
}

#[test]
fn prepending_older_history_releases_the_top_edge_until_user_scrolls_again() {
    let (mut model, mut daemon_rx) = make_model_with_daemon_rx();
    model.config.tui_chat_history_page_size = 123;
    model.chat.reduce(chat::ChatAction::ThreadDetailReceived(
        crate::state::chat::AgentThread {
            id: "thread-user".to_string(),
            title: "User Thread".to_string(),
            total_message_count: 400,
            loaded_message_start: 277,
            loaded_message_end: 400,
            messages: (277..400)
                .map(|index| crate::state::chat::AgentMessage {
                    id: Some(format!("msg-{index}")),
                    role: crate::state::chat::MessageRole::Assistant,
                    content: format!("msg {index}"),
                    ..Default::default()
                })
                .collect(),
            ..Default::default()
        },
    ));
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-user".to_string()));
    model
        .chat
        .reduce(chat::ChatAction::ScrollChat(i32::MAX / 2));

    model.on_tick();

    match next_thread_request(&mut daemon_rx) {
        Some((thread_id, message_limit, message_offset)) => {
            assert_eq!(thread_id, "thread-user");
            assert_eq!(message_limit, Some(123));
            assert_eq!(message_offset, Some(123));
        }
        other => panic!("expected first older-page request, got {other:?}"),
    }

    model.handle_client_event(ClientEvent::ThreadDetail(Some(crate::wire::AgentThread {
        id: "thread-user".to_string(),
        title: "User Thread".to_string(),
        total_message_count: 400,
        loaded_message_start: 154,
        loaded_message_end: 277,
        messages: (154..277)
            .map(|index| crate::wire::AgentMessage {
                id: Some(format!("msg-{index}")),
                role: crate::wire::MessageRole::Assistant,
                content: format!("msg {index}"),
                timestamp: index as u64,
                message_kind: "normal".to_string(),
                ..Default::default()
            })
            .collect(),
        created_at: 1,
        updated_at: 1,
        ..Default::default()
    })));

    for _ in 0..chat::CHAT_HISTORY_FETCH_DEBOUNCE_TICKS {
        model.on_tick();
    }

    assert!(
        next_thread_request(&mut daemon_rx).is_none(),
        "prepend anchor should move the viewport below the new top so history does not auto-fetch again"
    );
}

#[test]
fn thread_detail_preserves_message_author_metadata() {
    let mut model = make_model();

    model.handle_client_event(ClientEvent::ThreadDetail(Some(crate::wire::AgentThread {
        id: "thread-user".to_string(),
        title: "User Thread".to_string(),
        messages: vec![crate::wire::AgentMessage {
            role: crate::wire::MessageRole::Assistant,
            content: "Visible participant post.".to_string(),
            author_agent_id: Some("weles".to_string()),
            author_agent_name: Some("Weles".to_string()),
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
        .find(|thread| thread.id == "thread-user")
        .expect("thread detail should populate chat state");
    let message = thread
        .messages
        .first()
        .expect("thread should contain message");
    assert_eq!(message.author_agent_id.as_deref(), Some("weles"));
    assert_eq!(message.author_agent_name.as_deref(), Some("Weles"));
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
fn active_thread_reload_required_requests_detail_and_sidebar_context() {
    let (mut model, mut daemon_rx) = make_model_with_daemon_rx();
    model.config.tui_chat_history_page_size = 123;
    model.chat.reduce(chat::ChatAction::ThreadCreated {
        thread_id: "thread-user".to_string(),
        title: "User Thread".to_string(),
    });
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-user".to_string()));

    model.handle_client_event(ClientEvent::ThreadReloadRequired {
        thread_id: "thread-user".to_string(),
    });

    match daemon_rx.try_recv() {
        Ok(DaemonCommand::RequestThread {
            thread_id,
            message_limit,
            message_offset,
        }) => {
            assert_eq!(thread_id, "thread-user");
            assert_eq!(message_limit, Some(123));
            assert_eq!(message_offset, Some(0));
        }
        other => panic!("expected thread detail request, got {other:?}"),
    }
    match daemon_rx.try_recv() {
        Ok(DaemonCommand::RequestThreadTodos(thread_id)) => {
            assert_eq!(thread_id, "thread-user");
        }
        other => panic!("expected todos request, got {other:?}"),
    }
    match daemon_rx.try_recv() {
        Ok(DaemonCommand::RequestThreadWorkContext(thread_id)) => {
            assert_eq!(thread_id, "thread-user");
        }
        other => panic!("expected work-context request, got {other:?}"),
    }
}

#[test]
fn participant_managed_thread_reload_requests_expanded_latest_page() {
    let (mut model, mut daemon_rx) = make_model_with_daemon_rx();
    model.config.tui_chat_history_page_size = 123;
    model.handle_client_event(ClientEvent::ThreadDetail(Some(crate::wire::AgentThread {
        id: "thread-user".to_string(),
        title: "User Thread".to_string(),
        thread_participants: vec![crate::wire::ThreadParticipantState {
            agent_id: "weles".to_string(),
            agent_name: "Weles".to_string(),
            instruction: "verify claims".to_string(),
            status: "active".to_string(),
            created_at: 1,
            updated_at: 1,
            last_contribution_at: None,
            deactivated_at: None,
        }],
        created_at: 1,
        updated_at: 1,
        ..Default::default()
    })));
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-user".to_string()));

    while daemon_rx.try_recv().is_ok() {}

    model.handle_client_event(ClientEvent::ThreadReloadRequired {
        thread_id: "thread-user".to_string(),
    });

    match daemon_rx.try_recv() {
        Ok(DaemonCommand::RequestThread {
            thread_id,
            message_limit,
            message_offset,
        }) => {
            assert_eq!(thread_id, "thread-user");
            assert_eq!(message_limit, Some(246));
            assert_eq!(message_offset, Some(0));
        }
        other => {
            panic!("expected expanded participant-managed thread detail request, got {other:?}")
        }
    }
}

#[test]
fn inactive_thread_reload_required_does_not_interrupt_selected_thread() {
    let (mut model, mut daemon_rx) = make_model_with_daemon_rx();
    model.chat.reduce(chat::ChatAction::ThreadCreated {
        thread_id: "thread-user".to_string(),
        title: "User Thread".to_string(),
    });
    model.chat.reduce(chat::ChatAction::ThreadCreated {
        thread_id: "thread-other".to_string(),
        title: "Other Thread".to_string(),
    });
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-user".to_string()));
    model.handle_client_event(ClientEvent::ToolCall {
        thread_id: "thread-user".to_string(),
        call_id: "user-call".to_string(),
        name: "bash_command".to_string(),
        arguments: "{\"command\":\"pwd\"}".to_string(),
        weles_review: None,
    });

    model.handle_client_event(ClientEvent::ThreadReloadRequired {
        thread_id: "thread-other".to_string(),
    });

    assert_eq!(model.chat.active_thread_id(), Some("thread-user"));
    assert_eq!(model.chat.active_tool_calls().len(), 1);
    assert!(daemon_rx.try_recv().is_err());
}

#[test]
fn selected_internal_dm_thread_detail_is_loaded() {
    let mut model = make_model();
    model.chat.reduce(chat::ChatAction::ThreadCreated {
        thread_id: "dm:svarog:weles".to_string(),
        title: "Internal DM · Svarog ↔ WELES".to_string(),
    });
    model.chat.reduce(chat::ChatAction::SelectThread(
        "dm:svarog:weles".to_string(),
    ));

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
    assert_eq!(
        thread.messages[0].content,
        "Keep reviewing the migration plan."
    );
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
fn inactive_thread_events_do_not_replace_selected_thread_activity_badge() {
    let mut model = make_model();
    model.chat.reduce(chat::ChatAction::ThreadCreated {
        thread_id: "thread-user".to_string(),
        title: "User Thread".to_string(),
    });
    model.chat.reduce(chat::ChatAction::ThreadCreated {
        thread_id: "thread-other".to_string(),
        title: "Other Thread".to_string(),
    });
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-user".to_string()));

    model.handle_client_event(ClientEvent::Reasoning {
        thread_id: "thread-other".to_string(),
        content: "background reasoning".to_string(),
    });
    model.handle_client_event(ClientEvent::ToolCall {
        thread_id: "thread-other".to_string(),
        call_id: "background-call".to_string(),
        name: "bash_command".to_string(),
        arguments: "{\"command\":\"pwd\"}".to_string(),
        weles_review: None,
    });

    assert_eq!(model.chat.active_thread_id(), Some("thread-user"));
    assert!(model.agent_activity.is_none());
    assert_eq!(model.chat.streaming_content(), "");
    assert_eq!(model.chat.streaming_reasoning(), "");
    assert!(model.chat.active_tool_calls().is_empty());
}

#[test]
fn participant_playground_activity_surfaces_only_for_active_parent_thread() {
    let mut model = make_model();
    model.handle_client_event(ClientEvent::ThreadCreated {
        thread_id: "thread-other".to_string(),
        title: "Other Thread".to_string(),
        agent_name: None,
    });
    model.handle_client_event(ClientEvent::ThreadDetail(Some(crate::wire::AgentThread {
        id: "thread-user".to_string(),
        title: "User Thread".to_string(),
        thread_participants: vec![
            crate::wire::ThreadParticipantState {
                agent_id: "domowoj".to_string(),
                agent_name: "Domowoj".to_string(),
                instruction: "Look for gaps".to_string(),
                status: "active".to_string(),
                created_at: 1,
                updated_at: 1,
                last_contribution_at: None,
                deactivated_at: None,
            },
            crate::wire::ThreadParticipantState {
                agent_id: "weles".to_string(),
                agent_name: "Weles".to_string(),
                instruction: "Verify risky changes".to_string(),
                status: "active".to_string(),
                created_at: 1,
                updated_at: 1,
                last_contribution_at: None,
                deactivated_at: None,
            },
        ],
        created_at: 1,
        updated_at: 1,
        ..Default::default()
    })));
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-user".to_string()));

    model.handle_client_event(ClientEvent::Reasoning {
        thread_id: "playground:domowoj:thread-user".to_string(),
        content: "Hidden participant reasoning".to_string(),
    });
    assert_eq!(
        model.footer_activity_text().as_deref(),
        Some("Domowoj crafting response")
    );

    model.handle_client_event(ClientEvent::ToolCall {
        thread_id: "playground:weles:thread-user".to_string(),
        call_id: "hidden-call".to_string(),
        name: "bash_command".to_string(),
        arguments: "{\"command\":\"pwd\"}".to_string(),
        weles_review: None,
    });
    assert_eq!(
        model.footer_activity_text().as_deref(),
        Some("Domowoj +1 crafting responses")
    );

    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-other".to_string()));
    assert!(
        model.footer_activity_text().is_none(),
        "participant playground activity should stay scoped to the selected visible thread"
    );
}

#[test]
fn participant_playground_done_refreshes_active_visible_thread_and_surfaces_reply() {
    let (mut model, mut daemon_rx) = make_model_with_daemon_rx();
    model.config.tui_chat_history_page_size = 123;
    model.handle_client_event(ClientEvent::ThreadDetail(Some(crate::wire::AgentThread {
        id: "thread-user".to_string(),
        title: "User Thread".to_string(),
        messages: vec![crate::wire::AgentMessage {
            id: Some("msg-1".to_string()),
            role: crate::wire::MessageRole::Assistant,
            content: "Main agent reply".to_string(),
            timestamp: 1,
            message_kind: "normal".to_string(),
            ..Default::default()
        }],
        thread_participants: vec![crate::wire::ThreadParticipantState {
            agent_id: "domowoj".to_string(),
            agent_name: "Domowoj".to_string(),
            instruction: "Look for weak spots".to_string(),
            status: "active".to_string(),
            created_at: 1,
            updated_at: 1,
            last_contribution_at: None,
            deactivated_at: None,
        }],
        created_at: 1,
        updated_at: 1,
        ..Default::default()
    })));
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-user".to_string()));

    while daemon_rx.try_recv().is_ok() {}

    model.handle_client_event(ClientEvent::Delta {
        thread_id: "playground:domowoj:thread-user".to_string(),
        content: "Drafting visible reply".to_string(),
    });
    assert_eq!(
        model.footer_activity_text().as_deref(),
        Some("Domowoj crafting response")
    );

    model.handle_client_event(ClientEvent::Done {
        thread_id: "playground:domowoj:thread-user".to_string(),
        input_tokens: 0,
        output_tokens: 0,
        cost: None,
        provider: None,
        model: None,
        tps: None,
        generation_ms: None,
        reasoning: None,
        provider_final_result_json: None,
    });

    match next_thread_request(&mut daemon_rx) {
        Some((thread_id, message_limit, message_offset)) => {
            assert_eq!(thread_id, "thread-user");
            assert_eq!(message_limit, Some(246));
            assert_eq!(message_offset, Some(0));
        }
        other => {
            panic!("expected active visible thread refresh after playground done, got {other:?}")
        }
    }
    assert!(
        model.footer_activity_text().is_none(),
        "playground completion should clear the footer activity line"
    );

    model.handle_client_event(ClientEvent::ThreadDetail(Some(crate::wire::AgentThread {
        id: "thread-user".to_string(),
        title: "User Thread".to_string(),
        messages: vec![
            crate::wire::AgentMessage {
                id: Some("msg-1".to_string()),
                role: crate::wire::MessageRole::Assistant,
                content: "Main agent reply".to_string(),
                timestamp: 1,
                message_kind: "normal".to_string(),
                ..Default::default()
            },
            crate::wire::AgentMessage {
                id: Some("msg-2".to_string()),
                role: crate::wire::MessageRole::Assistant,
                content: "Visible participant reply".to_string(),
                author_agent_id: Some("domowoj".to_string()),
                author_agent_name: Some("Domowoj".to_string()),
                timestamp: 2,
                message_kind: "normal".to_string(),
                ..Default::default()
            },
        ],
        thread_participants: vec![crate::wire::ThreadParticipantState {
            agent_id: "domowoj".to_string(),
            agent_name: "Domowoj".to_string(),
            instruction: "Look for weak spots".to_string(),
            status: "active".to_string(),
            created_at: 1,
            updated_at: 2,
            last_contribution_at: Some(2),
            deactivated_at: None,
        }],
        created_at: 1,
        updated_at: 2,
        ..Default::default()
    })));

    let thread = model.chat.active_thread().expect("thread should exist");
    assert!(
        thread.messages.iter().any(|message| {
            message.content == "Visible participant reply"
                && message.author_agent_name.as_deref() == Some("Domowoj")
        }),
        "authoritative refresh should surface participant-authored visible replies"
    );
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
fn participant_suggestion_event_queues_prompt_with_agent_name() {
    let mut model = make_model();

    model.handle_client_event(ClientEvent::ParticipantSuggestion {
        thread_id: "thread-1".to_string(),
        suggestion: crate::wire::ThreadParticipantSuggestion {
            id: "sugg-1".to_string(),
            target_agent_id: "weles".to_string(),
            target_agent_name: "Weles".to_string(),
            instruction: "check claim".to_string(),
            force_send: false,
            status: "queued".to_string(),
            created_at: 1,
            updated_at: 1,
            error: None,
        },
    });

    assert_eq!(model.queued_prompts.len(), 1);
    assert_eq!(
        model.queued_prompts[0].participant_agent_name.as_deref(),
        Some("Weles")
    );
    assert_eq!(model.queued_prompts[0].display_text(), "Weles: check claim");
}

#[test]
fn participant_suggestion_does_not_auto_flush_as_user_message_after_done() {
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

    model.handle_client_event(ClientEvent::ParticipantSuggestion {
        thread_id: "thread-1".to_string(),
        suggestion: crate::wire::ThreadParticipantSuggestion {
            id: "sugg-1".to_string(),
            target_agent_id: "weles".to_string(),
            target_agent_name: "Weles".to_string(),
            instruction: "check claim".to_string(),
            force_send: false,
            status: "queued".to_string(),
            created_at: 1,
            updated_at: 1,
            error: None,
        },
    });

    model.handle_client_event(ClientEvent::Done {
        thread_id: "thread-1".to_string(),
        input_tokens: 0,
        output_tokens: 0,
        cost: None,
        provider: None,
        model: None,
        tps: None,
        generation_ms: None,
        reasoning: None,
        provider_final_result_json: None,
    });

    assert!(
        daemon_rx.try_recv().is_err(),
        "participant suggestions must not auto-submit through the normal send-message path"
    );
    assert_eq!(model.queued_prompts.len(), 1);
    assert_eq!(
        model.queued_prompts[0].suggestion_id.as_deref(),
        Some("sugg-1")
    );
}

#[test]
fn thread_detail_prunes_stale_participant_prompts_after_daemon_removes_suggestion() {
    let mut model = make_model();
    model.chat.reduce(chat::ChatAction::ThreadCreated {
        thread_id: "thread-1".to_string(),
        title: "Thread".to_string(),
    });
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-1".to_string()));

    model.handle_client_event(ClientEvent::ParticipantSuggestion {
        thread_id: "thread-1".to_string(),
        suggestion: crate::wire::ThreadParticipantSuggestion {
            id: "sugg-1".to_string(),
            target_agent_id: "weles".to_string(),
            target_agent_name: "Weles".to_string(),
            instruction: "check claim".to_string(),
            force_send: false,
            status: "queued".to_string(),
            created_at: 1,
            updated_at: 1,
            error: None,
        },
    });
    assert_eq!(model.queued_prompts.len(), 1);

    model.handle_thread_detail_event(crate::wire::AgentThread {
        id: "thread-1".to_string(),
        title: "Thread".to_string(),
        messages: vec![],
        queued_participant_suggestions: vec![],
        created_at: 1,
        updated_at: 2,
        ..Default::default()
    });

    assert!(
        model.queued_prompts.is_empty(),
        "thread detail should clear stale participant prompts once the daemon no longer reports them"
    );
}

#[test]
fn queued_participant_send_now_stops_stream_and_sends_participant_command() {
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
        content: "streaming".to_string(),
    });
    model.queue_participant_suggestion(
        "thread-1".to_string(),
        "sugg-1".to_string(),
        "weles".to_string(),
        "Weles".to_string(),
        "urgent fix".to_string(),
        true,
    );
    model.open_queued_prompts_modal();

    model.execute_selected_queued_prompt_action();

    assert!(matches!(
        daemon_rx.try_recv(),
        Ok(DaemonCommand::StopStream { thread_id }) if thread_id == "thread-1"
    ));
    assert!(matches!(
        daemon_rx.try_recv(),
        Ok(DaemonCommand::SendParticipantSuggestion { thread_id, suggestion_id })
            if thread_id == "thread-1" && suggestion_id == "sugg-1"
    ));
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
fn leading_internal_delegate_prompt_routes_to_internal_command() {
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

    model.submit_prompt("!weles verify the auth regression".to_string());

    match daemon_rx.try_recv() {
        Ok(DaemonCommand::InternalDelegate {
            thread_id,
            target_agent_id,
            content,
            ..
        }) => {
            assert_eq!(thread_id.as_deref(), Some("thread-1"));
            assert_eq!(target_agent_id, "weles");
            assert_eq!(content, "verify the auth regression");
        }
        other => panic!("expected internal delegate command, got {:?}", other),
    }
    assert!(
        model
            .chat
            .active_thread()
            .expect("thread should remain selected")
            .messages
            .is_empty(),
        "internal delegation should not append a visible user turn"
    );
}

#[test]
fn leading_participant_prompt_routes_to_participant_command() {
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

    model.submit_prompt("@weles verify claims before answering".to_string());

    match daemon_rx.try_recv() {
        Ok(DaemonCommand::ThreadParticipantCommand {
            thread_id,
            target_agent_id,
            action,
            instruction,
            ..
        }) => {
            assert_eq!(thread_id, "thread-1");
            assert_eq!(target_agent_id, "weles");
            assert_eq!(action, "upsert");
            assert_eq!(
                instruction.as_deref(),
                Some("verify claims before answering")
            );
        }
        other => panic!("expected participant command, got {:?}", other),
    }
    assert!(
        model
            .chat
            .active_thread()
            .expect("thread should remain selected")
            .messages
            .is_empty(),
        "participant registration should not append a visible user turn"
    );
    let (notice, _) = model
        .input_notice_style()
        .expect("participant command should surface a visible notice");
    assert!(
        notice.contains("Weles"),
        "expected agent name in notice, got: {notice}"
    );
    assert!(
        notice.contains("joined") || notice.contains("updated"),
        "expected participant update wording in notice, got: {notice}"
    );
}

#[test]
fn unconfigured_builtin_participant_prompt_opens_setup_and_retries_after_model_selection() {
    let (mut model, mut daemon_rx) = make_model_with_daemon_rx();
    model.connected = true;
    model.concierge.auto_cleanup_on_navigate = false;
    model.auth.entries = vec![crate::state::auth::ProviderAuthEntry {
        provider_id: amux_shared::providers::PROVIDER_ID_ALIBABA_CODING_PLAN.to_string(),
        provider_name: "Alibaba Coding Plan".to_string(),
        authenticated: true,
        auth_source: "api_key".to_string(),
        model: "qwen3.6-plus".to_string(),
    }];
    model.chat.reduce(chat::ChatAction::ThreadCreated {
        thread_id: "thread-1".to_string(),
        title: "Thread".to_string(),
    });
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-1".to_string()));

    model.submit_prompt("@swarozyc verify claims before answering".to_string());

    assert_eq!(
        model.modal.top(),
        Some(crate::state::modal::ModalKind::ProviderPicker)
    );
    assert!(
        daemon_rx.try_recv().is_err(),
        "setup should happen before any daemon command is emitted"
    );

    let provider_index = widgets::provider_picker::available_provider_defs(&model.auth)
        .iter()
        .position(|provider| provider.id == amux_shared::providers::PROVIDER_ID_ALIBABA_CODING_PLAN)
        .expect("provider to exist");
    if provider_index > 0 {
        model
            .modal
            .reduce(crate::state::modal::ModalAction::Navigate(
                provider_index as i32,
            ));
    }

    let quit = model.handle_key_modal(
        KeyCode::Enter,
        KeyModifiers::NONE,
        crate::state::modal::ModalKind::ProviderPicker,
    );
    assert!(!quit);
    assert_eq!(
        model.modal.top(),
        Some(crate::state::modal::ModalKind::ModelPicker)
    );

    let quit = model.handle_key_modal(
        KeyCode::Enter,
        KeyModifiers::NONE,
        crate::state::modal::ModalKind::ModelPicker,
    );
    assert!(!quit);

    match daemon_rx
        .try_recv()
        .expect("expected targeted builtin persona config command")
    {
        DaemonCommand::SetTargetAgentProviderModel {
            target_agent_id,
            provider_id,
            model,
        } => {
            assert_eq!(target_agent_id, "swarozyc");
            assert_eq!(
                provider_id,
                amux_shared::providers::PROVIDER_ID_ALIBABA_CODING_PLAN
            );
            assert_eq!(model, "qwen3.6-plus");
        }
        other => panic!("expected builtin persona provider/model command, got {other:?}"),
    }

    match daemon_rx
        .try_recv()
        .expect("expected retried participant command after setup")
    {
        DaemonCommand::ThreadParticipantCommand {
            thread_id,
            target_agent_id,
            action,
            instruction,
            ..
        } => {
            assert_eq!(thread_id, "thread-1");
            assert_eq!(target_agent_id, "swarozyc");
            assert_eq!(action, "upsert");
            assert_eq!(
                instruction.as_deref(),
                Some("verify claims before answering")
            );
        }
        other => panic!("expected participant command, got {other:?}"),
    }
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
