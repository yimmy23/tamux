use super::*;
use amux_shared::providers::{
    AudioToolKind, PROVIDER_ID_ALIBABA_CODING_PLAN, PROVIDER_ID_AZURE_OPENAI, PROVIDER_ID_CHUTES,
    PROVIDER_ID_CUSTOM, PROVIDER_ID_OPENAI, PROVIDER_ID_OPENROUTER, PROVIDER_ID_QWEN,
    PROVIDER_ID_XAI,
};
use ratatui::backend::TestBackend;
use ratatui::layout::Rect;
use ratatui::Terminal;
use tokio::sync::mpsc::unbounded_channel;

fn make_model() -> (
    TuiModel,
    tokio::sync::mpsc::UnboundedReceiver<DaemonCommand>,
) {
    let (_event_tx, event_rx) = std::sync::mpsc::channel();
    let (daemon_tx, daemon_rx) = unbounded_channel();
    (TuiModel::new(event_rx, daemon_tx), daemon_rx)
}

fn make_goal_run(
    id: &str,
    title: &str,
    status: crate::state::task::GoalRunStatus,
) -> crate::state::task::GoalRun {
    crate::state::task::GoalRun {
        id: id.to_string(),
        title: title.to_string(),
        status: Some(status),
        goal: format!("Goal for {title}"),
        ..Default::default()
    }
}

fn make_goal_run_with_steps(
    id: &str,
    title: &str,
    status: crate::state::task::GoalRunStatus,
    steps: Vec<crate::state::task::GoalRunStep>,
) -> crate::state::task::GoalRun {
    crate::state::task::GoalRun {
        id: id.to_string(),
        title: title.to_string(),
        status: Some(status),
        goal: format!("Goal for {title}"),
        steps,
        ..Default::default()
    }
}

fn next_goal_run_detail_request(
    daemon_rx: &mut tokio::sync::mpsc::UnboundedReceiver<DaemonCommand>,
) -> Option<String> {
    while let Ok(command) = daemon_rx.try_recv() {
        if let DaemonCommand::RequestGoalRunDetail(goal_run_id) = command {
            return Some(goal_run_id);
        }
    }
    None
}

fn next_goal_run_checkpoints_request(
    daemon_rx: &mut tokio::sync::mpsc::UnboundedReceiver<DaemonCommand>,
) -> Option<String> {
    while let Ok(command) = daemon_rx.try_recv() {
        if let DaemonCommand::RequestGoalRunCheckpoints(goal_run_id) = command {
            return Some(goal_run_id);
        }
    }
    None
}

fn next_goal_hydration_schedule(
    daemon_rx: &mut tokio::sync::mpsc::UnboundedReceiver<DaemonCommand>,
) -> Option<String> {
    while let Ok(command) = daemon_rx.try_recv() {
        if let DaemonCommand::ScheduleGoalHydrationRefresh(goal_run_id) = command {
            return Some(goal_run_id);
        }
    }
    None
}

fn seed_goal_sidebar_model() -> (
    TuiModel,
    tokio::sync::mpsc::UnboundedReceiver<DaemonCommand>,
) {
    let (mut model, daemon_rx) = make_model();
    model.chat.reduce(chat::ChatAction::ThreadCreated {
        thread_id: "thread-1".to_string(),
        title: "Goal Thread".to_string(),
    });
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-1".to_string()));
    model.tasks.reduce(task::TaskAction::TaskListReceived(vec![
        task::AgentTask {
            id: "task-1".to_string(),
            title: "Child Task One".to_string(),
            thread_id: Some("thread-1".to_string()),
            goal_run_id: Some("goal-1".to_string()),
            created_at: 1,
            ..Default::default()
        },
        task::AgentTask {
            id: "task-2".to_string(),
            title: "Child Task Two".to_string(),
            thread_id: Some("thread-2".to_string()),
            goal_run_id: Some("goal-1".to_string()),
            created_at: 2,
            ..Default::default()
        },
    ]));
    model.tasks.reduce(task::TaskAction::GoalRunDetailReceived(
        crate::state::task::GoalRun {
            id: "goal-1".to_string(),
            title: "Goal One".to_string(),
            thread_id: Some("thread-1".to_string()),
            goal: "Goal for Goal One".to_string(),
            child_task_ids: vec!["task-1".to_string(), "task-2".to_string()],
            steps: vec![
                crate::state::task::GoalRunStep {
                    id: "step-1".to_string(),
                    title: "Plan".to_string(),
                    order: 0,
                    ..Default::default()
                },
                crate::state::task::GoalRunStep {
                    id: "step-2".to_string(),
                    title: "Implement".to_string(),
                    order: 1,
                    ..Default::default()
                },
            ],
            ..Default::default()
        },
    ));
    model
        .tasks
        .reduce(task::TaskAction::GoalRunCheckpointsReceived {
            goal_run_id: "goal-1".to_string(),
            checkpoints: vec![
                task::GoalRunCheckpointSummary {
                    id: "checkpoint-1".to_string(),
                    checkpoint_type: "plan".to_string(),
                    step_index: Some(1),
                    context_summary_preview: Some("Implement checkpoint".to_string()),
                    ..Default::default()
                },
                task::GoalRunCheckpointSummary {
                    id: "checkpoint-2".to_string(),
                    checkpoint_type: "note".to_string(),
                    context_summary_preview: Some("Loose checkpoint".to_string()),
                    ..Default::default()
                },
            ],
        });
    model.tasks.reduce(task::TaskAction::WorkContextReceived(
        task::ThreadWorkContext {
            thread_id: "thread-1".to_string(),
            entries: vec![
                task::WorkContextEntry {
                    path: "/tmp/first.md".to_string(),
                    goal_run_id: Some("goal-1".to_string()),
                    is_text: true,
                    ..Default::default()
                },
                task::WorkContextEntry {
                    path: "/tmp/second.md".to_string(),
                    goal_run_id: Some("goal-1".to_string()),
                    is_text: true,
                    ..Default::default()
                },
            ],
        },
    ));
    model.main_pane_view = MainPaneView::Task(SidebarItemTarget::GoalRun {
        goal_run_id: "goal-1".to_string(),
        step_id: Some("step-1".to_string()),
    });
    model.focus = FocusArea::Sidebar;
    (model, daemon_rx)
}

fn render_screen(model: &mut TuiModel) -> Vec<String> {
    let backend = TestBackend::new(model.width, model.height);
    let mut terminal = Terminal::new(backend).expect("test terminal should initialize");
    terminal
        .draw(|frame| model.render(frame))
        .expect("render should succeed");

    let buffer = terminal.backend().buffer();
    (0..model.height)
        .map(|y| {
            (0..model.width)
                .filter_map(|x| buffer.cell((x, y)).map(|cell| cell.symbol()))
                .collect::<String>()
        })
        .collect()
}

fn collect_daemon_commands(
    daemon_rx: &mut tokio::sync::mpsc::UnboundedReceiver<DaemonCommand>,
) -> Vec<DaemonCommand> {
    let mut commands = Vec::new();
    while let Ok(command) = daemon_rx.try_recv() {
        commands.push(command);
    }
    commands
}

fn seed_goal_approval_overlay(
    model: &mut TuiModel,
    approval_id: &str,
    goal_run_id: &str,
    thread_id: &str,
) {
    model.chat.reduce(chat::ChatAction::ThreadCreated {
        thread_id: thread_id.to_string(),
        title: "Goal Thread".to_string(),
    });
    model.tasks.reduce(task::TaskAction::GoalRunDetailReceived(
        crate::state::task::GoalRun {
            id: goal_run_id.to_string(),
            title: "Goal plan review".to_string(),
            thread_id: Some(thread_id.to_string()),
            status: Some(crate::state::task::GoalRunStatus::AwaitingApproval),
            current_step_title: Some("review plan".to_string()),
            approval_count: 1,
            awaiting_approval_id: Some(approval_id.to_string()),
            ..Default::default()
        },
    ));
    model
        .approval
        .reduce(crate::state::ApprovalAction::ApprovalRequired(
            crate::state::PendingApproval {
                approval_id: approval_id.to_string(),
                task_id: goal_run_id.to_string(),
                task_title: Some("Goal plan review".to_string()),
                thread_id: Some(thread_id.to_string()),
                thread_title: Some("Goal Thread".to_string()),
                workspace_id: Some(model.config.honcho_workspace_id.clone()),
                rationale: Some("Review the plan before continuing".to_string()),
                reasons: vec!["operator approval required".to_string()],
                command: "review goal step: review plan".to_string(),
                risk_level: crate::state::RiskLevel::Medium,
                blast_radius: "review plan".to_string(),
                received_at: 1,
                seen_at: None,
            },
        ));
    model
        .approval
        .reduce(crate::state::ApprovalAction::SelectApproval(
            approval_id.to_string(),
        ));
    model.main_pane_view = MainPaneView::Task(SidebarItemTarget::GoalRun {
        goal_run_id: goal_run_id.to_string(),
        step_id: None,
    });
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::ApprovalOverlay));
}

fn sample_subagent(id: &str, name: &str, builtin: bool) -> crate::state::SubAgentEntry {
    crate::state::SubAgentEntry {
        id: id.to_string(),
        name: name.to_string(),
        provider: "openai".to_string(),
        model: "gpt-5.4-mini".to_string(),
        role: Some("testing".to_string()),
        enabled: true,
        builtin,
        immutable_identity: builtin,
        disable_allowed: !builtin,
        delete_allowed: !builtin,
        protected_reason: builtin.then(|| "builtin".to_string()),
        reasoning_effort: Some("medium".to_string()),
        raw_json: None,
    }
}

fn navigate_model_picker_to(model: &mut TuiModel, model_id: &str) {
    let index = model
        .available_model_picker_models()
        .iter()
        .position(|entry| entry.id == model_id)
        .expect("expected model to exist in picker");
    if index > 0 {
        model
            .modal
            .reduce(modal::ModalAction::Navigate(index as i32));
    }
}

fn make_runtime_assignment(
    role_id: &str,
    provider: &str,
    model: &str,
    reasoning_effort: Option<&str>,
) -> task::GoalAgentAssignment {
    task::GoalAgentAssignment {
        role_id: role_id.to_string(),
        enabled: true,
        provider: provider.to_string(),
        model: model.to_string(),
        reasoning_effort: reasoning_effort.map(str::to_string),
        inherit_from_main: false,
    }
}

fn make_goal_owner_profile(
    agent_label: &str,
    provider: &str,
    model: &str,
    reasoning_effort: Option<&str>,
) -> task::GoalRuntimeOwnerProfile {
    task::GoalRuntimeOwnerProfile {
        agent_label: agent_label.to_string(),
        provider: provider.to_string(),
        model: model.to_string(),
        reasoning_effort: reasoning_effort.map(str::to_string),
    }
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
    let tools_index = model
        .modal
        .command_items()
        .iter()
        .position(|item| item.command == "tools")
        .expect("tools command should exist");
    model
        .modal
        .reduce(modal::ModalAction::Navigate(tools_index as i32));

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
fn clicking_rendered_settings_tab_switches_tabs() {
    let (mut model, _daemon_rx) = make_model();
    model.width = 100;
    model.height = 40;
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::Settings));

    let (_, overlay_area) = model
        .current_modal_area()
        .expect("settings modal should expose its overlay area");
    let screen = render_screen(&mut model);
    let tab_row = overlay_area.y.saturating_add(1) as usize;
    let chat_col = screen[tab_row]
        .find("Chat")
        .map(|idx| idx as u16)
        .expect("rendered settings tabs should include Chat");

    model.handle_mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: chat_col,
        row: overlay_area.y.saturating_add(1),
        modifiers: KeyModifiers::NONE,
    });

    assert_eq!(model.settings.active_tab(), SettingsTab::Chat);
}

#[test]
fn settings_modal_mouse_wheel_scrolls_overflowing_content() {
    let (mut model, _daemon_rx) = make_model();
    model.width = 100;
    model.height = 16;
    model
        .settings
        .reduce(SettingsAction::SwitchTab(SettingsTab::Chat));
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::Settings));

    let (_, overlay_area) = model
        .current_modal_area()
        .expect("settings modal should expose its overlay area");
    let before = render_screen(&mut model).join("\n");
    assert!(
        !before.contains("Inspect Generated Tools"),
        "expected overflowing settings content to be clipped before scrolling"
    );

    for _ in 0..8 {
        model.handle_mouse(MouseEvent {
            kind: MouseEventKind::ScrollDown,
            column: overlay_area.x.saturating_add(2),
            row: overlay_area.y.saturating_add(4),
            modifiers: KeyModifiers::NONE,
        });
    }

    let after = render_screen(&mut model).join("\n");
    assert!(
        after.contains("Inspect Generated Tools"),
        "expected mouse wheel scrolling to reveal lower settings rows"
    );
}

#[test]
fn settings_modal_keyboard_navigation_scrolls_selected_field_into_view() {
    let (mut model, _daemon_rx) = make_model();
    model.width = 100;
    model.height = 16;
    model
        .settings
        .reduce(SettingsAction::SwitchTab(SettingsTab::Chat));
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::Settings));

    let before = render_screen(&mut model).join("\n");
    assert!(
        !before.contains("Inspect Generated Tools"),
        "expected overflowing settings content to be clipped before keyboard navigation"
    );

    for _ in 0..23 {
        let quit = model.handle_key_modal(
            KeyCode::Down,
            KeyModifiers::NONE,
            modal::ModalKind::Settings,
        );
        assert!(!quit);
    }

    assert_eq!(model.settings.field_cursor(), 23);
    assert!(
        model.settings_modal_scroll > 0,
        "expected keyboard navigation to advance the settings scroll offset"
    );

    let after = render_screen(&mut model).join("\n");
    assert!(
        after.contains("Inspect Generated Tools"),
        "expected keyboard navigation to reveal the selected lower settings row"
    );
}

#[test]
fn settings_modal_auth_keyboard_navigation_scrolls_selected_provider_into_view() {
    let (mut model, _daemon_rx) = make_model();
    model.width = 100;
    model.height = 16;
    model.auth.loaded = true;
    model.auth.entries = (0..12)
        .map(|i| crate::state::auth::ProviderAuthEntry {
            provider_id: format!("provider-{i}"),
            provider_name: format!("Provider {i}"),
            authenticated: i % 2 == 0,
            auth_source: "api_key".to_string(),
            model: String::new(),
        })
        .collect();
    model
        .settings
        .reduce(SettingsAction::SwitchTab(SettingsTab::Auth));
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::Settings));

    let before = render_screen(&mut model).join("\n");
    assert!(
        !before.contains("Provider 10"),
        "expected lower auth rows to be clipped before keyboard navigation"
    );

    for _ in 0..10 {
        let quit = model.handle_key_modal(
            KeyCode::Down,
            KeyModifiers::NONE,
            modal::ModalKind::Settings,
        );
        assert!(!quit);
    }

    assert_eq!(model.auth.selected, 10);
    assert!(
        model.settings_modal_scroll > 0,
        "expected auth keyboard navigation to advance the settings scroll offset"
    );

    let after = render_screen(&mut model).join("\n");
    assert!(
        after.contains("Provider 10"),
        "expected auth keyboard navigation to reveal the selected provider row"
    );
}

#[test]
fn settings_modal_features_keyboard_navigation_scrolls_audio_fields_into_view() {
    let (mut model, _daemon_rx) = make_model();
    model.width = 100;
    model.height = 16;
    model
        .settings
        .reduce(SettingsAction::SwitchTab(SettingsTab::Features));
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::Settings));

    let before = render_screen(&mut model).join("\n");
    assert!(
        !before.contains("TTS Voice"),
        "expected lower Features rows to be clipped before keyboard navigation"
    );

    for _ in 0..22 {
        let quit = model.handle_key_modal(
            KeyCode::Down,
            KeyModifiers::NONE,
            modal::ModalKind::Settings,
        );
        assert!(!quit);
    }

    assert_eq!(model.settings.field_cursor(), 22);
    assert!(
        model.settings_modal_scroll > 0,
        "expected Features keyboard navigation to advance the settings scroll offset"
    );

    let after = render_screen(&mut model).join("\n");
    assert!(
        after.contains("TTS Voice"),
        "expected Features keyboard navigation to reveal the selected audio field"
    );
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
fn command_palette_enter_prefers_highlighted_command_over_partial_query() {
    let (mut model, _daemon_rx) = make_model();
    model.focus = FocusArea::Chat;

    let quit = model.handle_key(KeyCode::Char('/'), KeyModifiers::NONE);
    assert!(!quit);
    for ch in "new".chars() {
        let quit = model.handle_key(KeyCode::Char(ch), KeyModifiers::NONE);
        assert!(!quit);
    }

    let quit = model.handle_key_modal(
        KeyCode::Down,
        KeyModifiers::NONE,
        modal::ModalKind::CommandPalette,
    );
    assert!(!quit);
    assert_eq!(
        model
            .modal
            .selected_command()
            .map(|item| item.command.as_str()),
        Some("new-goal")
    );
    assert_eq!(model.input.buffer(), "");
    assert_eq!(model.modal.command_display_query(), "new");
    assert!(model.modal.command_palette_has_explicit_selection());

    let quit = model.handle_key(KeyCode::Enter, KeyModifiers::NONE);
    assert!(!quit);
    assert!(matches!(model.main_pane_view, MainPaneView::GoalComposer));
    assert_eq!(model.focus, FocusArea::Input);
    assert!(model.modal.top().is_none());
    assert_eq!(model.input.buffer(), "");
}

#[test]
fn command_palette_typing_does_not_preview_first_match_before_navigation() {
    let (mut model, _daemon_rx) = make_model();
    model.focus = FocusArea::Chat;

    let quit = model.handle_key(KeyCode::Char('/'), KeyModifiers::NONE);
    assert!(!quit);
    for ch in "new-g".chars() {
        let quit = model.handle_key(KeyCode::Char(ch), KeyModifiers::NONE);
        assert!(!quit);
    }

    assert_eq!(model.input.buffer(), "");
    assert_eq!(model.modal.command_display_query(), "new-g");
    assert!(!model.modal.command_palette_has_explicit_selection());
}

#[test]
fn slash_opened_command_palette_keeps_raw_filter_text() {
    let (mut model, _daemon_rx) = make_model();
    model.focus = FocusArea::Chat;

    let quit = model.handle_key(KeyCode::Char('/'), KeyModifiers::NONE);
    assert!(!quit);
    for ch in "new".chars() {
        let quit = model.handle_key(KeyCode::Char(ch), KeyModifiers::NONE);
        assert!(!quit);
    }

    assert_eq!(model.modal.command_display_query(), "new");

    let quit = model.handle_key_modal(
        KeyCode::Down,
        KeyModifiers::NONE,
        modal::ModalKind::CommandPalette,
    );
    assert!(!quit);
    assert_eq!(model.modal.command_display_query(), "new");
}

#[test]
fn command_palette_goa_filter_survives_navigation() {
    let (mut model, _daemon_rx) = make_model();
    model.focus = FocusArea::Chat;

    let quit = model.handle_key(KeyCode::Char('/'), KeyModifiers::NONE);
    assert!(!quit);
    for ch in "goa".chars() {
        let quit = model.handle_key(KeyCode::Char(ch), KeyModifiers::NONE);
        assert!(!quit);
    }

    assert_eq!(model.input.buffer(), "");
    assert_eq!(model.modal.command_display_query(), "goa");
    assert_eq!(
        model
            .modal
            .selected_command()
            .map(|item| item.command.as_str()),
        Some("new-goal")
    );

    let quit = model.handle_key_modal(
        KeyCode::Down,
        KeyModifiers::NONE,
        modal::ModalKind::CommandPalette,
    );
    assert!(!quit);
    assert_eq!(model.input.buffer(), "");
    assert_eq!(model.modal.command_display_query(), "goa");
    assert_eq!(
        model
            .modal
            .selected_command()
            .map(|item| item.command.as_str()),
        Some("goal")
    );

    let quit = model.handle_key_modal(
        KeyCode::Up,
        KeyModifiers::NONE,
        modal::ModalKind::CommandPalette,
    );
    assert!(!quit);
    assert_eq!(model.input.buffer(), "");
    assert_eq!(model.modal.command_display_query(), "goa");
    assert_eq!(
        model
            .modal
            .selected_command()
            .map(|item| item.command.as_str()),
        Some("new-goal")
    );
}

#[test]
fn goal_composer_command_palette_typing_keeps_goal_draft_intact() {
    let (mut model, _daemon_rx) = make_model();
    model.main_pane_view = MainPaneView::GoalComposer;
    model.focus = FocusArea::Input;
    model.input.set_text("Ship release");
    model
        .goal_mission_control
        .set_prompt_text("Ship release".to_string());

    let quit = model.handle_key(KeyCode::Char('p'), KeyModifiers::CONTROL);
    assert!(!quit);
    assert_eq!(model.modal.top(), Some(modal::ModalKind::CommandPalette));

    let quit = model.handle_key(KeyCode::Char('n'), KeyModifiers::NONE);
    assert!(!quit);

    assert_eq!(model.modal.command_display_query(), "n");
    assert_eq!(model.input.buffer(), "Ship release");
    assert_eq!(model.goal_mission_control.prompt_text(), "Ship release");
}

#[test]
fn goal_composer_command_palette_reopens_fresh_after_close() {
    let (mut model, _daemon_rx) = make_model();
    model.main_pane_view = MainPaneView::GoalComposer;
    model.focus = FocusArea::Input;
    model.input.set_text("Ship release");
    model
        .goal_mission_control
        .set_prompt_text("Ship release".to_string());

    let quit = model.handle_key(KeyCode::Char('p'), KeyModifiers::CONTROL);
    assert!(!quit);
    for ch in "new".chars() {
        let quit = model.handle_key(KeyCode::Char(ch), KeyModifiers::NONE);
        assert!(!quit);
    }
    assert_eq!(model.modal.command_display_query(), "new");

    let quit = model.handle_key(KeyCode::Esc, KeyModifiers::NONE);
    assert!(!quit);
    assert!(model.modal.top().is_none());
    assert_eq!(model.input.buffer(), "Ship release");
    assert_eq!(model.goal_mission_control.prompt_text(), "Ship release");

    let quit = model.handle_key(KeyCode::Char('p'), KeyModifiers::CONTROL);
    assert!(!quit);
    assert_eq!(model.modal.top(), Some(modal::ModalKind::CommandPalette));
    assert_eq!(model.modal.command_display_query(), "");
    assert_eq!(model.input.buffer(), "Ship release");
    assert_eq!(model.goal_mission_control.prompt_text(), "Ship release");
}

#[test]
fn chat_command_palette_typing_keeps_chat_draft_intact() {
    let (mut model, _daemon_rx) = make_model();
    model.focus = FocusArea::Input;
    model.input.set_text("hello chat");

    let quit = model.handle_key(KeyCode::Char('p'), KeyModifiers::CONTROL);
    assert!(!quit);
    assert_eq!(model.modal.top(), Some(modal::ModalKind::CommandPalette));

    let quit = model.handle_key(KeyCode::Char('n'), KeyModifiers::NONE);
    assert!(!quit);

    assert_eq!(model.modal.command_display_query(), "n");
    assert_eq!(model.input.buffer(), "hello chat");
}

#[test]
fn command_palette_enter_runs_first_match_without_navigation() {
    let (mut model, _daemon_rx) = make_model();
    model.focus = FocusArea::Chat;

    let quit = model.handle_key(KeyCode::Char('/'), KeyModifiers::NONE);
    assert!(!quit);
    for ch in "new-g".chars() {
        let quit = model.handle_key(KeyCode::Char(ch), KeyModifiers::NONE);
        assert!(!quit);
    }

    assert_eq!(model.input.buffer(), "");
    assert_eq!(model.modal.command_display_query(), "new-g");

    let quit = model.handle_key(KeyCode::Enter, KeyModifiers::NONE);
    assert!(!quit);
    assert!(matches!(model.main_pane_view, MainPaneView::GoalComposer));
    assert_eq!(model.focus, FocusArea::Input);
    assert!(model.modal.top().is_none());
    assert_eq!(model.input.buffer(), "");
}

#[test]
fn command_palette_mouse_selection_executes_selected_command_without_rewriting_query() {
    let (mut model, _daemon_rx) = make_model();
    model.focus = FocusArea::Chat;

    let quit = model.handle_key(KeyCode::Char('/'), KeyModifiers::NONE);
    assert!(!quit);
    for ch in "new".chars() {
        let quit = model.handle_key(KeyCode::Char(ch), KeyModifiers::NONE);
        assert!(!quit);
    }

    model.modal_navigate_to(1);
    assert_eq!(
        model
            .modal
            .selected_command()
            .map(|item| item.command.as_str()),
        Some("new-goal")
    );
    assert_eq!(model.input.buffer(), "");
    assert_eq!(model.modal.command_display_query(), "new");
    assert!(model.modal.command_palette_has_explicit_selection());

    model.handle_modal_enter(modal::ModalKind::CommandPalette);
    assert!(matches!(model.main_pane_view, MainPaneView::GoalComposer));
    assert_eq!(model.focus, FocusArea::Input);
    assert!(model.modal.top().is_none());
    assert_eq!(model.input.buffer(), "");
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
fn goal_approval_reject_from_overlay_opens_followup_prompt_without_resolving_yet() {
    let (mut model, mut daemon_rx) = make_model();
    seed_goal_approval_overlay(&mut model, "approval-1", "goal-1", "thread-1");

    let quit = model.handle_key_modal(
        KeyCode::Char('n'),
        KeyModifiers::NONE,
        modal::ModalKind::ApprovalOverlay,
    );

    assert!(!quit);
    assert_eq!(
        model.modal.top(),
        Some(modal::ModalKind::GoalApprovalRejectPrompt)
    );
    assert!(
        daemon_rx.try_recv().is_err(),
        "reject follow-up should not resolve approval until the operator chooses rewrite or stop"
    );
}

#[test]
fn goal_approval_rewrite_choice_rejects_and_prefills_guidance_input() {
    let (mut model, mut daemon_rx) = make_model();
    seed_goal_approval_overlay(&mut model, "approval-1", "goal-1", "thread-1");
    let _ = model.handle_key_modal(
        KeyCode::Char('n'),
        KeyModifiers::NONE,
        modal::ModalKind::ApprovalOverlay,
    );

    let quit = model.handle_key_modal(
        KeyCode::Char('r'),
        KeyModifiers::NONE,
        modal::ModalKind::GoalApprovalRejectPrompt,
    );

    assert!(!quit);
    assert!(matches!(
        daemon_rx.try_recv().expect("expected approval rejection command"),
        DaemonCommand::ResolveTaskApproval {
            approval_id,
            decision
        } if approval_id == "approval-1" && decision == "reject"
    ));
    assert!(
        daemon_rx.try_recv().is_err(),
        "rewrite path should not stop the goal"
    );
    assert_eq!(model.focus, FocusArea::Input);
    assert!(model
        .input
        .buffer()
        .contains("Rewrite the blocked goal step"));
    assert_eq!(model.modal.top(), None);
}

#[test]
fn goal_approval_stop_choice_rejects_and_stops_goal() {
    let (mut model, mut daemon_rx) = make_model();
    seed_goal_approval_overlay(&mut model, "approval-1", "goal-1", "thread-1");
    let _ = model.handle_key_modal(
        KeyCode::Char('n'),
        KeyModifiers::NONE,
        modal::ModalKind::ApprovalOverlay,
    );

    let quit = model.handle_key_modal(
        KeyCode::Char('s'),
        KeyModifiers::NONE,
        modal::ModalKind::GoalApprovalRejectPrompt,
    );

    assert!(!quit);
    assert!(matches!(
        daemon_rx.try_recv().expect("expected approval rejection command"),
        DaemonCommand::ResolveTaskApproval {
            approval_id,
            decision
        } if approval_id == "approval-1" && decision == "reject"
    ));
    assert!(matches!(
        daemon_rx.try_recv().expect("expected stop goal command"),
        DaemonCommand::ControlGoalRun {
            goal_run_id,
            action,
            step_index: None
        } if goal_run_id == "goal-1" && action == "stop"
    ));
    assert_eq!(model.modal.top(), None);
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
fn provider_picker_fetches_remote_models_for_chutes() {
    let (mut model, mut daemon_rx) = make_model();
    model.auth.entries = vec![crate::state::auth::ProviderAuthEntry {
        provider_id: PROVIDER_ID_CHUTES.to_string(),
        provider_name: "Chutes".to_string(),
        authenticated: true,
        auth_source: "api_key".to_string(),
        model: "deepseek-ai/DeepSeek-R1".to_string(),
    }];
    model.config.agent_config_raw = Some(serde_json::json!({
        "providers": {
            PROVIDER_ID_CHUTES: {
                "base_url": "https://llm.chutes.ai/v1",
                "api_key": "chutes-key",
                "auth_source": "api_key"
            }
        }
    }));

    let chutes_index = widgets::provider_picker::available_provider_defs(&model.auth)
        .iter()
        .position(|provider| provider.id == PROVIDER_ID_CHUTES)
        .expect("chutes to exist");

    model.settings_picker_target = Some(SettingsPickerTarget::Provider);
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::ProviderPicker));
    model.modal.set_picker_item_count(
        widgets::provider_picker::available_provider_defs(&model.auth).len(),
    );
    if chutes_index > 0 {
        model
            .modal
            .reduce(modal::ModalAction::Navigate(chutes_index as i32));
    }

    let quit = model.handle_key_modal(
        KeyCode::Enter,
        KeyModifiers::NONE,
        modal::ModalKind::ProviderPicker,
    );

    assert!(!quit);
    assert_eq!(model.config.provider, PROVIDER_ID_CHUTES);
    assert_eq!(model.modal.top(), Some(modal::ModalKind::ModelPicker));
    match daemon_rx.try_recv() {
        Ok(DaemonCommand::FetchModels {
            provider_id,
            base_url,
            api_key,
            output_modalities,
        }) => {
            assert_eq!(provider_id, PROVIDER_ID_CHUTES);
            assert_eq!(base_url, "https://llm.chutes.ai/v1");
            assert_eq!(api_key, "chutes-key");
            assert_eq!(output_modalities, None);
        }
        other => panic!("expected FetchModels for Chutes provider picker, got {other:?}"),
    }
}

#[test]
fn provider_picker_uses_chatgpt_subscription_auth_without_remote_model_fetch() {
    let (mut model, mut daemon_rx) = make_model();
    model.auth.entries = vec![crate::state::auth::ProviderAuthEntry {
        provider_id: PROVIDER_ID_OPENAI.to_string(),
        provider_name: "OpenAI".to_string(),
        authenticated: true,
        auth_source: "chatgpt_subscription".to_string(),
        model: "gpt-5.4".to_string(),
    }];
    model.config.chatgpt_auth_available = true;
    model.config.chatgpt_auth_source = Some("tamux-daemon".to_string());

    let openai_index = widgets::provider_picker::available_provider_defs(&model.auth)
        .iter()
        .position(|provider| provider.id == PROVIDER_ID_OPENAI)
        .expect("openai to exist");

    model.settings_picker_target = Some(SettingsPickerTarget::Provider);
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::ProviderPicker));
    model.modal.set_picker_item_count(
        widgets::provider_picker::available_provider_defs(&model.auth).len(),
    );
    if openai_index > 0 {
        model
            .modal
            .reduce(modal::ModalAction::Navigate(openai_index as i32));
    }

    let quit = model.handle_key_modal(
        KeyCode::Enter,
        KeyModifiers::NONE,
        modal::ModalKind::ProviderPicker,
    );

    assert!(!quit);
    assert_eq!(model.config.provider, PROVIDER_ID_OPENAI);
    assert_eq!(model.config.auth_source, "chatgpt_subscription");
    assert_eq!(model.config.api_transport, "responses");
    assert_eq!(model.modal.top(), Some(modal::ModalKind::ModelPicker));
    while let Ok(command) = daemon_rx.try_recv() {
        if let DaemonCommand::FetchModels { .. } = command {
            panic!("chatgpt subscription auth should not trigger remote model fetches");
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
                pricing: None,
                metadata: None,
            },
            crate::state::config::FetchedModel {
                id: "gpt-5.4-mini".to_string(),
                name: Some("GPT-5.4 Mini".to_string()),
                context_window: Some(128_000),
                pricing: None,
                metadata: None,
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
                pricing: None,
                metadata: None,
            },
            crate::state::config::FetchedModel {
                id: "gpt-5.4-mini".to_string(),
                name: Some("GPT-5.4 Mini".to_string()),
                context_window: Some(128_000),
                pricing: None,
                metadata: None,
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
fn selecting_audio_stt_provider_updates_audio_provider_and_opens_model_picker() {
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
            provider_id: PROVIDER_ID_AZURE_OPENAI.to_string(),
            provider_name: "Azure OpenAI".to_string(),
            authenticated: true,
            auth_source: "api_key".to_string(),
            model: "gpt-4.1".to_string(),
        },
        crate::state::auth::ProviderAuthEntry {
            provider_id: PROVIDER_ID_XAI.to_string(),
            provider_name: "xAI".to_string(),
            authenticated: true,
            auth_source: "api_key".to_string(),
            model: "grok-4".to_string(),
        },
    ];
    model.config.agent_config_raw = Some(serde_json::json!({
        "audio": {
            "stt": {
                "provider": PROVIDER_ID_OPENAI,
                "model": "whisper-1"
            }
        }
    }));

    let target_index = widgets::provider_picker::available_audio_provider_defs(
        &model.auth,
        AudioToolKind::SpeechToText,
    )
    .iter()
    .position(|provider| provider.id == PROVIDER_ID_XAI)
    .expect("provider to exist");

    model.settings_picker_target = Some(SettingsPickerTarget::AudioSttProvider);
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::ProviderPicker));
    model.modal.set_picker_item_count(
        widgets::provider_picker::available_audio_provider_defs(
            &model.auth,
            AudioToolKind::SpeechToText,
        )
        .len(),
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
        model
            .config
            .agent_config_raw
            .as_ref()
            .and_then(|raw| raw.get("audio"))
            .and_then(|audio| audio.get("stt"))
            .and_then(|stt| stt.get("provider"))
            .and_then(|value| value.as_str()),
        Some(PROVIDER_ID_XAI)
    );
    assert_eq!(
        model
            .config
            .agent_config_raw
            .as_ref()
            .and_then(|raw| raw.get("audio"))
            .and_then(|audio| audio.get("stt"))
            .and_then(|stt| stt.get("model"))
            .and_then(|value| value.as_str()),
        Some("grok-4")
    );
    assert_eq!(model.modal.top(), Some(modal::ModalKind::ModelPicker));
}

#[test]
fn selecting_audio_stt_model_updates_audio_model() {
    let (mut model, _daemon_rx) = make_model();
    model.config.agent_config_raw = Some(serde_json::json!({
        "audio": {
            "stt": {
                "provider": PROVIDER_ID_OPENAI,
                "model": "whisper-1"
            }
        }
    }));
    model
        .config
        .reduce(config::ConfigAction::ModelsFetched(vec![
            crate::state::config::FetchedModel {
                id: "gpt-4o-transcribe".to_string(),
                name: Some("GPT-4o Transcribe".to_string()),
                context_window: Some(128_000),
                pricing: None,
                metadata: None,
            },
            crate::state::config::FetchedModel {
                id: "whisper-1".to_string(),
                name: Some("Whisper 1".to_string()),
                context_window: None,
                pricing: None,
                metadata: None,
            },
        ]));

    model.settings_picker_target = Some(SettingsPickerTarget::AudioSttModel);
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::ModelPicker));

    let quit = model.handle_key_modal(
        KeyCode::Enter,
        KeyModifiers::NONE,
        modal::ModalKind::ModelPicker,
    );

    assert!(!quit);
    assert_eq!(
        model
            .config
            .agent_config_raw
            .as_ref()
            .and_then(|raw| raw.get("audio"))
            .and_then(|audio| audio.get("stt"))
            .and_then(|stt| stt.get("model"))
            .and_then(|value| value.as_str()),
        Some("gpt-4o-transcribe")
    );
}

#[test]
fn selecting_audio_tts_provider_updates_audio_provider_and_opens_model_picker() {
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
            provider_id: PROVIDER_ID_AZURE_OPENAI.to_string(),
            provider_name: "Azure OpenAI".to_string(),
            authenticated: true,
            auth_source: "api_key".to_string(),
            model: "gpt-4.1".to_string(),
        },
        crate::state::auth::ProviderAuthEntry {
            provider_id: PROVIDER_ID_XAI.to_string(),
            provider_name: "xAI".to_string(),
            authenticated: true,
            auth_source: "api_key".to_string(),
            model: "grok-4".to_string(),
        },
    ];
    model.config.agent_config_raw = Some(serde_json::json!({
        "audio": {
            "tts": {
                "provider": PROVIDER_ID_OPENAI,
                "model": "gpt-4o-mini-tts"
            }
        }
    }));

    let target_index = widgets::provider_picker::available_audio_provider_defs(
        &model.auth,
        AudioToolKind::TextToSpeech,
    )
    .iter()
    .position(|provider| provider.id == PROVIDER_ID_XAI)
    .expect("provider to exist");

    model.settings_picker_target = Some(SettingsPickerTarget::AudioTtsProvider);
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::ProviderPicker));
    model.modal.set_picker_item_count(
        widgets::provider_picker::available_audio_provider_defs(
            &model.auth,
            AudioToolKind::TextToSpeech,
        )
        .len(),
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
        model
            .config
            .agent_config_raw
            .as_ref()
            .and_then(|raw| raw.get("audio"))
            .and_then(|audio| audio.get("tts"))
            .and_then(|tts| tts.get("provider"))
            .and_then(|value| value.as_str()),
        Some(PROVIDER_ID_XAI)
    );
    assert_eq!(
        model
            .config
            .agent_config_raw
            .as_ref()
            .and_then(|raw| raw.get("audio"))
            .and_then(|audio| audio.get("tts"))
            .and_then(|tts| tts.get("model"))
            .and_then(|value| value.as_str()),
        Some("grok-4")
    );
    assert_eq!(model.modal.top(), Some(modal::ModalKind::ModelPicker));
}

#[test]
fn authenticated_provider_picker_lists_xai() {
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
            provider_id: PROVIDER_ID_XAI.to_string(),
            provider_name: "xAI".to_string(),
            authenticated: true,
            auth_source: "api_key".to_string(),
            model: "grok-4".to_string(),
        },
    ];

    let providers = widgets::provider_picker::available_provider_defs(&model.auth);

    assert!(providers
        .iter()
        .any(|provider| provider.id == PROVIDER_ID_XAI));
}

#[test]
fn selecting_audio_tts_model_updates_audio_model() {
    let (mut model, _daemon_rx) = make_model();
    model.config.agent_config_raw = Some(serde_json::json!({
        "audio": {
            "tts": {
                "provider": PROVIDER_ID_OPENAI,
                "model": "gpt-4o-mini-tts"
            }
        }
    }));
    model
        .config
        .reduce(config::ConfigAction::ModelsFetched(vec![
            crate::state::config::FetchedModel {
                id: "gpt-4o-mini-tts".to_string(),
                name: Some("GPT-4o Mini TTS".to_string()),
                context_window: Some(128_000),
                pricing: None,
                metadata: None,
            },
            crate::state::config::FetchedModel {
                id: "tts-1".to_string(),
                name: Some("TTS 1".to_string()),
                context_window: None,
                pricing: None,
                metadata: None,
            },
        ]));

    model.settings_picker_target = Some(SettingsPickerTarget::AudioTtsModel);
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
    assert_eq!(
        model
            .config
            .agent_config_raw
            .as_ref()
            .and_then(|raw| raw.get("audio"))
            .and_then(|audio| audio.get("tts"))
            .and_then(|tts| tts.get("model"))
            .and_then(|value| value.as_str()),
        Some("tts-1")
    );
}

#[test]
fn selecting_image_generation_provider_updates_image_provider_and_opens_model_picker() {
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
            provider_id: PROVIDER_ID_OPENROUTER.to_string(),
            provider_name: "OpenRouter".to_string(),
            authenticated: true,
            auth_source: "api_key".to_string(),
            model: "openai/gpt-5.4".to_string(),
        },
    ];
    model.config.agent_config_raw = Some(serde_json::json!({
        "image": {
            "generation": {
                "provider": PROVIDER_ID_OPENAI,
                "model": "gpt-image-1"
            }
        }
    }));

    let target_index = widgets::provider_picker::available_provider_defs(&model.auth)
        .iter()
        .position(|provider| provider.id == PROVIDER_ID_OPENROUTER)
        .expect("provider to exist");

    model.settings_picker_target = Some(SettingsPickerTarget::ImageGenerationProvider);
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
        model
            .config
            .agent_config_raw
            .as_ref()
            .and_then(|raw| raw.get("image"))
            .and_then(|image| image.get("generation"))
            .and_then(|generation| generation.get("provider"))
            .and_then(|value| value.as_str()),
        Some(PROVIDER_ID_OPENROUTER)
    );
    assert_eq!(
        model
            .config
            .agent_config_raw
            .as_ref()
            .and_then(|raw| raw.get("image"))
            .and_then(|image| image.get("generation"))
            .and_then(|generation| generation.get("model"))
            .and_then(|value| value.as_str()),
        Some("openai/gpt-image-1")
    );
    assert_eq!(model.modal.top(), Some(modal::ModalKind::ModelPicker));
}

#[test]
fn selecting_image_generation_model_updates_image_model() {
    let (mut model, _daemon_rx) = make_model();
    model.config.agent_config_raw = Some(serde_json::json!({
        "image": {
            "generation": {
                "provider": PROVIDER_ID_OPENAI,
                "model": "gpt-image-1"
            }
        }
    }));
    model
        .config
        .reduce(config::ConfigAction::ModelsFetched(vec![
            crate::state::config::FetchedModel {
                id: "openai/gpt-image-1".to_string(),
                name: Some("OpenAI GPT Image 1".to_string()),
                context_window: None,
                pricing: Some(crate::state::config::FetchedModelPricing {
                    image: Some("0.00001".to_string()),
                    ..Default::default()
                }),
                metadata: Some(serde_json::json!({
                    "output_modalities": ["image"]
                })),
            },
            crate::state::config::FetchedModel {
                id: "gpt-4o-mini".to_string(),
                name: Some("GPT-4o Mini".to_string()),
                context_window: Some(128_000),
                pricing: None,
                metadata: Some(serde_json::json!({
                    "output_modalities": ["text"]
                })),
            },
        ]));

    model.settings_picker_target = Some(SettingsPickerTarget::ImageGenerationModel);
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::ModelPicker));

    let quit = model.handle_key_modal(
        KeyCode::Enter,
        KeyModifiers::NONE,
        modal::ModalKind::ModelPicker,
    );

    assert!(!quit);
    assert_eq!(
        model
            .config
            .agent_config_raw
            .as_ref()
            .and_then(|raw| raw.get("image"))
            .and_then(|image| image.get("generation"))
            .and_then(|generation| generation.get("model"))
            .and_then(|value| value.as_str()),
        Some("gpt-image-1")
    );
}

#[test]
fn selecting_main_image_capable_model_enables_vision() {
    let (mut model, mut daemon_rx) = make_model();
    model.config.tool_vision = false;
    model.config.model.clear();
    model.config.agent_config_raw = Some(serde_json::json!({
        "tools": {
            "vision": false
        }
    }));
    model
        .config
        .reduce(config::ConfigAction::ModelsFetched(vec![
            crate::state::config::FetchedModel {
                id: "gpt-4.1-image".to_string(),
                name: Some("GPT 4.1 Image".to_string()),
                context_window: Some(128_000),
                pricing: Some(crate::state::config::FetchedModelPricing {
                    image: Some("0.00001".to_string()),
                    ..Default::default()
                }),
                metadata: Some(serde_json::json!({
                    "architecture": {
                        "input_modalities": ["text", "image"],
                        "output_modalities": ["text"]
                    }
                })),
            },
        ]));
    model.settings_picker_target = Some(SettingsPickerTarget::Model);
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::ModelPicker));
    navigate_model_picker_to(&mut model, "gpt-4.1-image");

    let quit = model.handle_key_modal(
        KeyCode::Enter,
        KeyModifiers::NONE,
        modal::ModalKind::ModelPicker,
    );

    assert!(!quit);
    assert_eq!(model.config.model, "gpt-4.1-image");
    assert!(model.config.tool_vision);
    assert_eq!(
        model
            .config
            .agent_config_raw
            .as_ref()
            .and_then(|raw| raw.get("tools"))
            .and_then(|tools| tools.get("vision"))
            .and_then(|value| value.as_bool()),
        Some(true)
    );

    let commands = collect_daemon_commands(&mut daemon_rx);
    assert!(commands.iter().any(|command| {
        matches!(
            command,
            DaemonCommand::SetConfigItem { key_path, value_json }
                if key_path == "/tools/vision" && value_json == "true"
        )
    }));
}

#[test]
fn selecting_main_audio_capable_model_prompts_for_stt_reuse() {
    let (mut model, mut daemon_rx) = make_model();
    model.config.model.clear();
    model.config.agent_config_raw = Some(serde_json::json!({
        "audio": {
            "stt": {
                "provider": PROVIDER_ID_OPENAI,
                "model": "whisper-1"
            }
        }
    }));
    model
        .config
        .reduce(config::ConfigAction::ModelsFetched(vec![
            crate::state::config::FetchedModel {
                id: "gpt-4o-audio-preview".to_string(),
                name: Some("GPT-4o Audio Preview".to_string()),
                context_window: Some(128_000),
                pricing: Some(crate::state::config::FetchedModelPricing {
                    audio: Some("0.000032".to_string()),
                    ..Default::default()
                }),
                metadata: Some(serde_json::json!({
                    "architecture": {
                        "input_modalities": ["text", "audio"],
                        "output_modalities": ["text", "audio"]
                    }
                })),
            },
        ]));
    model.settings_picker_target = Some(SettingsPickerTarget::Model);
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::ModelPicker));
    navigate_model_picker_to(&mut model, "gpt-4o-audio-preview");

    let quit = model.handle_key_modal(
        KeyCode::Enter,
        KeyModifiers::NONE,
        modal::ModalKind::ModelPicker,
    );

    assert!(!quit);
    assert_eq!(model.config.model, "gpt-4o-audio-preview");
    assert_eq!(model.modal.top(), Some(modal::ModalKind::ChatActionConfirm));
    assert_eq!(
        model
            .pending_chat_action_confirm
            .as_ref()
            .map(PendingConfirmAction::modal_body)
            .as_deref(),
        Some("Selected model supports audio. Use it as the STT model too?")
    );
    assert_eq!(
        model
            .config
            .agent_config_raw
            .as_ref()
            .and_then(|raw| raw.get("audio"))
            .and_then(|audio| audio.get("stt"))
            .and_then(|stt| stt.get("model"))
            .and_then(|value| value.as_str()),
        Some("whisper-1")
    );

    let commands = collect_daemon_commands(&mut daemon_rx);
    assert!(!commands.iter().any(|command| {
        matches!(
            command,
            DaemonCommand::SetConfigItem { key_path, .. } if key_path == "/audio/stt/model"
        )
    }));
}

#[test]
fn selecting_main_model_with_only_generic_audio_metadata_does_not_prompt_for_stt_reuse() {
    let (mut model, mut daemon_rx) = make_model();
    model.config.model.clear();
    model.config.agent_config_raw = Some(serde_json::json!({
        "audio": {
            "stt": {
                "provider": PROVIDER_ID_OPENAI,
                "model": "whisper-1"
            }
        }
    }));
    model
        .config
        .reduce(config::ConfigAction::ModelsFetched(vec![
            crate::state::config::FetchedModel {
                id: "openai/generic-audio".to_string(),
                name: Some("Generic Audio".to_string()),
                context_window: Some(128_000),
                pricing: Some(crate::state::config::FetchedModelPricing {
                    audio: Some("0.000032".to_string()),
                    ..Default::default()
                }),
                metadata: Some(serde_json::json!({
                    "modalities": ["text", "audio"]
                })),
            },
        ]));
    model.settings_picker_target = Some(SettingsPickerTarget::Model);
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::ModelPicker));
    navigate_model_picker_to(&mut model, "openai/generic-audio");

    let quit = model.handle_key_modal(
        KeyCode::Enter,
        KeyModifiers::NONE,
        modal::ModalKind::ModelPicker,
    );

    assert!(!quit);
    assert_eq!(model.config.model, "openai/generic-audio");
    assert_ne!(model.modal.top(), Some(modal::ModalKind::ChatActionConfirm));
    assert_eq!(
        model
            .config
            .agent_config_raw
            .as_ref()
            .and_then(|raw| raw.get("audio"))
            .and_then(|audio| audio.get("stt"))
            .and_then(|stt| stt.get("model"))
            .and_then(|value| value.as_str()),
        Some("whisper-1")
    );

    let commands = collect_daemon_commands(&mut daemon_rx);
    assert!(!commands.iter().any(|command| {
        matches!(
            command,
            DaemonCommand::SetConfigItem { key_path, .. } if key_path == "/audio/stt/model"
        )
    }));
}

#[test]
fn selecting_main_model_with_nondirectional_modality_string_does_not_prompt_for_stt_reuse() {
    let (mut model, mut daemon_rx) = make_model();
    model.config.model.clear();
    model.config.agent_config_raw = Some(serde_json::json!({
        "audio": {
            "stt": {
                "provider": PROVIDER_ID_OPENAI,
                "model": "whisper-1"
            }
        }
    }));
    model
        .config
        .reduce(config::ConfigAction::ModelsFetched(vec![
            crate::state::config::FetchedModel {
                id: "openai/plain-modality-audio".to_string(),
                name: Some("Plain Modality Audio".to_string()),
                context_window: Some(128_000),
                pricing: Some(crate::state::config::FetchedModelPricing {
                    audio: Some("0.000032".to_string()),
                    ..Default::default()
                }),
                metadata: Some(serde_json::json!({
                    "architecture": {
                        "modality": "text+audio"
                    }
                })),
            },
        ]));
    model.settings_picker_target = Some(SettingsPickerTarget::Model);
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::ModelPicker));
    navigate_model_picker_to(&mut model, "openai/plain-modality-audio");

    let quit = model.handle_key_modal(
        KeyCode::Enter,
        KeyModifiers::NONE,
        modal::ModalKind::ModelPicker,
    );

    assert!(!quit);
    assert_eq!(model.config.model, "openai/plain-modality-audio");
    assert_ne!(model.modal.top(), Some(modal::ModalKind::ChatActionConfirm));
    assert_eq!(
        model
            .config
            .agent_config_raw
            .as_ref()
            .and_then(|raw| raw.get("audio"))
            .and_then(|audio| audio.get("stt"))
            .and_then(|stt| stt.get("model"))
            .and_then(|value| value.as_str()),
        Some("whisper-1")
    );

    let commands = collect_daemon_commands(&mut daemon_rx);
    assert!(!commands.iter().any(|command| {
        matches!(
            command,
            DaemonCommand::SetConfigItem { key_path, .. } if key_path == "/audio/stt/model"
        )
    }));
}

#[test]
fn accepting_audio_model_stt_reuse_updates_stt_model() {
    let (mut model, mut daemon_rx) = make_model();
    model.config.model.clear();
    model.config.agent_config_raw = Some(serde_json::json!({
        "audio": {
            "stt": {
                "provider": PROVIDER_ID_OPENAI,
                "model": "whisper-1"
            }
        }
    }));
    model
        .config
        .reduce(config::ConfigAction::ModelsFetched(vec![
            crate::state::config::FetchedModel {
                id: "gpt-4o-audio-preview".to_string(),
                name: Some("GPT-4o Audio Preview".to_string()),
                context_window: Some(128_000),
                pricing: Some(crate::state::config::FetchedModelPricing {
                    audio: Some("0.000032".to_string()),
                    ..Default::default()
                }),
                metadata: Some(serde_json::json!({
                    "architecture": {
                        "input_modalities": ["text", "audio"],
                        "output_modalities": ["text", "audio"]
                    }
                })),
            },
        ]));
    model.settings_picker_target = Some(SettingsPickerTarget::Model);
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::ModelPicker));
    navigate_model_picker_to(&mut model, "gpt-4o-audio-preview");

    let quit = model.handle_key_modal(
        KeyCode::Enter,
        KeyModifiers::NONE,
        modal::ModalKind::ModelPicker,
    );
    assert!(!quit);
    assert_eq!(model.modal.top(), Some(modal::ModalKind::ChatActionConfirm));

    let quit = model.handle_key_modal(
        KeyCode::Enter,
        KeyModifiers::NONE,
        modal::ModalKind::ChatActionConfirm,
    );

    assert!(!quit);
    assert!(model.modal.top().is_none());
    assert_eq!(
        model
            .config
            .agent_config_raw
            .as_ref()
            .and_then(|raw| raw.get("audio"))
            .and_then(|audio| audio.get("stt"))
            .and_then(|stt| stt.get("provider"))
            .and_then(|value| value.as_str()),
        Some(PROVIDER_ID_OPENAI)
    );
    assert_eq!(
        model
            .config
            .agent_config_raw
            .as_ref()
            .and_then(|raw| raw.get("audio"))
            .and_then(|audio| audio.get("stt"))
            .and_then(|stt| stt.get("model"))
            .and_then(|value| value.as_str()),
        Some("gpt-4o-audio-preview")
    );

    let commands = collect_daemon_commands(&mut daemon_rx);
    assert!(commands.iter().any(|command| {
        matches!(
            command,
            DaemonCommand::SetConfigItem { key_path, value_json }
                if key_path == "/audio/stt/model"
                    && value_json == "\"gpt-4o-audio-preview\""
        )
    }));
    assert!(!commands.iter().any(|command| {
        matches!(
            command,
            DaemonCommand::SetConfigItem { key_path, .. } if key_path == "/audio/stt/provider"
        )
    }));
}

#[test]
fn declining_audio_model_stt_reuse_preserves_existing_stt_model() {
    let (mut model, mut daemon_rx) = make_model();
    model.config.model.clear();
    model.config.agent_config_raw = Some(serde_json::json!({
        "audio": {
            "stt": {
                "provider": PROVIDER_ID_OPENAI,
                "model": "whisper-1"
            }
        }
    }));
    model
        .config
        .reduce(config::ConfigAction::ModelsFetched(vec![
            crate::state::config::FetchedModel {
                id: "gpt-4o-audio-preview".to_string(),
                name: Some("GPT-4o Audio Preview".to_string()),
                context_window: Some(128_000),
                pricing: Some(crate::state::config::FetchedModelPricing {
                    audio: Some("0.000032".to_string()),
                    ..Default::default()
                }),
                metadata: Some(serde_json::json!({
                    "architecture": {
                        "input_modalities": ["text", "audio"],
                        "output_modalities": ["text", "audio"]
                    }
                })),
            },
        ]));
    model.settings_picker_target = Some(SettingsPickerTarget::Model);
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::ModelPicker));
    navigate_model_picker_to(&mut model, "gpt-4o-audio-preview");

    let quit = model.handle_key_modal(
        KeyCode::Enter,
        KeyModifiers::NONE,
        modal::ModalKind::ModelPicker,
    );
    assert!(!quit);
    assert_eq!(model.modal.top(), Some(modal::ModalKind::ChatActionConfirm));

    let quit = model.handle_key_modal(
        KeyCode::Tab,
        KeyModifiers::NONE,
        modal::ModalKind::ChatActionConfirm,
    );
    assert!(!quit);

    let quit = model.handle_key_modal(
        KeyCode::Enter,
        KeyModifiers::NONE,
        modal::ModalKind::ChatActionConfirm,
    );

    assert!(!quit);
    assert!(model.modal.top().is_none());
    assert_eq!(
        model
            .config
            .agent_config_raw
            .as_ref()
            .and_then(|raw| raw.get("audio"))
            .and_then(|audio| audio.get("stt"))
            .and_then(|stt| stt.get("provider"))
            .and_then(|value| value.as_str()),
        Some(PROVIDER_ID_OPENAI)
    );
    assert_eq!(
        model
            .config
            .agent_config_raw
            .as_ref()
            .and_then(|raw| raw.get("audio"))
            .and_then(|audio| audio.get("stt"))
            .and_then(|stt| stt.get("model"))
            .and_then(|value| value.as_str()),
        Some("whisper-1")
    );

    let commands = collect_daemon_commands(&mut daemon_rx);
    assert!(!commands.iter().any(|command| {
        matches!(
            command,
            DaemonCommand::SetConfigItem { key_path, .. } if key_path == "/audio/stt/model"
        )
    }));
}

#[test]
fn audio_stt_custom_model_entry_keeps_audio_field_selected() {
    let (mut model, _daemon_rx) = make_model();
    model.config.agent_config_raw = Some(serde_json::json!({
        "audio": {
            "stt": {
                "provider": PROVIDER_ID_OPENAI,
                "model": "whisper-1"
            }
        }
    }));
    model
        .settings
        .reduce(SettingsAction::SwitchTab(SettingsTab::Features));
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::Settings));
    model
        .config
        .reduce(config::ConfigAction::ModelsFetched(vec![
            crate::state::config::FetchedModel {
                id: "whisper-1".to_string(),
                name: Some("Whisper 1".to_string()),
                context_window: None,
                pricing: None,
                metadata: None,
            },
        ]));
    model.settings_picker_target = Some(SettingsPickerTarget::AudioSttModel);
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::ModelPicker));
    let custom_row = model.available_model_picker_models().len();
    model
        .modal
        .reduce(modal::ModalAction::Navigate(custom_row as i32));

    let quit = model.handle_key_modal(
        KeyCode::Enter,
        KeyModifiers::NONE,
        modal::ModalKind::ModelPicker,
    );

    assert!(!quit);
    assert_eq!(model.settings.active_tab(), SettingsTab::Features);
    assert_eq!(model.settings.field_cursor(), 18);
    assert_eq!(model.settings.editing_field(), Some("feat_audio_stt_model"));
}

#[test]
fn audio_tts_custom_model_entry_keeps_audio_field_selected() {
    let (mut model, _daemon_rx) = make_model();
    model.config.agent_config_raw = Some(serde_json::json!({
        "audio": {
            "tts": {
                "provider": PROVIDER_ID_OPENAI,
                "model": "gpt-4o-mini-tts"
            }
        }
    }));
    model
        .settings
        .reduce(SettingsAction::SwitchTab(SettingsTab::Features));
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::Settings));
    model
        .config
        .reduce(config::ConfigAction::ModelsFetched(vec![
            crate::state::config::FetchedModel {
                id: "gpt-4o-mini-tts".to_string(),
                name: Some("GPT-4o Mini TTS".to_string()),
                context_window: Some(128_000),
                pricing: None,
                metadata: None,
            },
        ]));
    model.settings_picker_target = Some(SettingsPickerTarget::AudioTtsModel);
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::ModelPicker));
    let custom_row = model.available_model_picker_models().len();
    model
        .modal
        .reduce(modal::ModalAction::Navigate(custom_row as i32));

    let quit = model.handle_key_modal(
        KeyCode::Enter,
        KeyModifiers::NONE,
        modal::ModalKind::ModelPicker,
    );

    assert!(!quit);
    assert_eq!(model.settings.active_tab(), SettingsTab::Features);
    assert_eq!(model.settings.field_cursor(), 21);
    assert_eq!(model.settings.editing_field(), Some("feat_audio_tts_model"));
}

#[test]
fn audio_model_picker_filters_fetched_models_to_audio_capable_entries() {
    let (mut model, _daemon_rx) = make_model();
    model.config.agent_config_raw = Some(serde_json::json!({
        "audio": {
            "stt": {
                "provider": PROVIDER_ID_OPENROUTER,
                "model": "openai/gpt-audio"
            }
        }
    }));
    model
        .config
        .reduce(config::ConfigAction::ModelsFetched(vec![
            crate::state::config::FetchedModel {
                id: "openai/gpt-audio".to_string(),
                name: Some("GPT Audio".to_string()),
                context_window: Some(128_000),
                pricing: Some(crate::state::config::FetchedModelPricing {
                    audio: Some("0.000032".to_string()),
                    ..Default::default()
                }),
                metadata: Some(serde_json::json!({
                    "architecture": {
                        "input_modalities": ["text", "audio"],
                        "output_modalities": ["text", "audio"]
                    }
                })),
            },
            crate::state::config::FetchedModel {
                id: "openai/gpt-text".to_string(),
                name: Some("GPT Text".to_string()),
                context_window: Some(128_000),
                pricing: Some(crate::state::config::FetchedModelPricing {
                    prompt: Some("0.000002".to_string()),
                    completion: Some("0.000008".to_string()),
                    ..Default::default()
                }),
                metadata: Some(serde_json::json!({
                    "architecture": {
                        "input_modalities": ["text"],
                        "output_modalities": ["text"]
                    }
                })),
            },
        ]));
    model.settings_picker_target = Some(SettingsPickerTarget::AudioSttModel);

    let models = model.available_model_picker_models();

    assert!(models.iter().any(|model| model.id == "openai/gpt-audio"));
    assert!(!models.iter().any(|model| model.id == "openai/gpt-text"));
}

#[test]
fn audio_model_picker_keeps_input_only_models_out_of_tts() {
    let (mut model, _daemon_rx) = make_model();
    model.config.agent_config_raw = Some(serde_json::json!({
        "audio": {
            "stt": {
                "provider": PROVIDER_ID_OPENROUTER,
                "model": "openai/gpt-stt-only"
            },
            "tts": {
                "provider": PROVIDER_ID_OPENROUTER,
                "model": "openai/gpt-tts-only"
            }
        }
    }));
    model
        .config
        .reduce(config::ConfigAction::ModelsFetched(vec![
            crate::state::config::FetchedModel {
                id: "openai/gpt-stt-only".to_string(),
                name: Some("GPT STT Only".to_string()),
                context_window: Some(128_000),
                pricing: Some(crate::state::config::FetchedModelPricing {
                    audio: Some("0.000032".to_string()),
                    ..Default::default()
                }),
                metadata: Some(serde_json::json!({
                    "architecture": {
                        "input_modalities": ["text", "audio"],
                        "output_modalities": ["text"]
                    }
                })),
            },
            crate::state::config::FetchedModel {
                id: "openai/gpt-tts-only".to_string(),
                name: Some("GPT TTS Only".to_string()),
                context_window: Some(128_000),
                pricing: Some(crate::state::config::FetchedModelPricing {
                    audio: Some("0.000032".to_string()),
                    ..Default::default()
                }),
                metadata: Some(serde_json::json!({
                    "architecture": {
                        "input_modalities": ["text"],
                        "output_modalities": ["text", "audio"]
                    }
                })),
            },
        ]));

    model.settings_picker_target = Some(SettingsPickerTarget::AudioSttModel);
    let stt_models = model
        .available_model_picker_models()
        .into_iter()
        .map(|entry| entry.id)
        .collect::<Vec<_>>();

    model.settings_picker_target = Some(SettingsPickerTarget::AudioTtsModel);
    let tts_models = model
        .available_model_picker_models()
        .into_iter()
        .map(|entry| entry.id)
        .collect::<Vec<_>>();

    assert!(stt_models.iter().any(|id| id == "openai/gpt-stt-only"));
    assert!(!stt_models.iter().any(|id| id == "openai/gpt-tts-only"));
    assert!(tts_models.iter().any(|id| id == "openai/gpt-tts-only"));
    assert!(!tts_models.iter().any(|id| id == "openai/gpt-stt-only"));
}

#[test]
fn audio_model_picker_uses_directional_audio_metadata_when_modality_is_sparse() {
    let (mut model, _daemon_rx) = make_model();
    model.config.agent_config_raw = Some(serde_json::json!({
        "audio": {
            "stt": {
                "provider": PROVIDER_ID_OPENROUTER,
                "model": "xai/grok-listen"
            },
            "tts": {
                "provider": PROVIDER_ID_OPENROUTER,
                "model": "xai/grok-speak"
            }
        }
    }));
    model
        .config
        .reduce(config::ConfigAction::ModelsFetched(vec![
            crate::state::config::FetchedModel {
                id: "xai/grok-listen".to_string(),
                name: Some("xAI Grok Listen".to_string()),
                context_window: Some(128_000),
                pricing: Some(crate::state::config::FetchedModelPricing {
                    audio: Some("0.000032".to_string()),
                    ..Default::default()
                }),
                metadata: Some(serde_json::json!({
                    "input_modalities": ["audio"]
                })),
            },
            crate::state::config::FetchedModel {
                id: "xai/grok-speak".to_string(),
                name: Some("xAI Grok Speak".to_string()),
                context_window: Some(128_000),
                pricing: Some(crate::state::config::FetchedModelPricing {
                    audio: Some("0.000032".to_string()),
                    ..Default::default()
                }),
                metadata: Some(serde_json::json!({
                    "output_modalities": ["audio"]
                })),
            },
        ]));

    model.settings_picker_target = Some(SettingsPickerTarget::AudioSttModel);
    let stt_models = model
        .available_model_picker_models()
        .into_iter()
        .map(|entry| entry.id)
        .collect::<Vec<_>>();

    model.settings_picker_target = Some(SettingsPickerTarget::AudioTtsModel);
    let tts_models = model
        .available_model_picker_models()
        .into_iter()
        .map(|entry| entry.id)
        .collect::<Vec<_>>();

    assert!(stt_models.iter().any(|id| id == "xai/grok-listen"));
    assert!(!stt_models.iter().any(|id| id == "xai/grok-speak"));
    assert!(tts_models.iter().any(|id| id == "xai/grok-speak"));
    assert!(!tts_models.iter().any(|id| id == "xai/grok-listen"));
}

#[test]
fn audio_model_picker_does_not_treat_generic_modalities_audio_as_directional_support() {
    let (mut model, _daemon_rx) = make_model();
    model.config.agent_config_raw = Some(serde_json::json!({
        "audio": {
            "stt": {
                "provider": PROVIDER_ID_OPENROUTER,
                "model": "openai/gpt-stt-only"
            },
            "tts": {
                "provider": PROVIDER_ID_OPENROUTER,
                "model": "openai/gpt-tts-only"
            }
        }
    }));
    model
        .config
        .reduce(config::ConfigAction::ModelsFetched(vec![
            crate::state::config::FetchedModel {
                id: "openai/generic-audio".to_string(),
                name: Some("Generic Audio".to_string()),
                context_window: Some(128_000),
                pricing: Some(crate::state::config::FetchedModelPricing {
                    audio: Some("0.000032".to_string()),
                    ..Default::default()
                }),
                metadata: Some(serde_json::json!({
                    "modalities": ["text", "audio"]
                })),
            },
        ]));

    model.settings_picker_target = Some(SettingsPickerTarget::AudioSttModel);
    let stt_models = model
        .available_model_picker_models()
        .into_iter()
        .map(|entry| entry.id)
        .collect::<Vec<_>>();

    model.settings_picker_target = Some(SettingsPickerTarget::AudioTtsModel);
    let tts_models = model
        .available_model_picker_models()
        .into_iter()
        .map(|entry| entry.id)
        .collect::<Vec<_>>();

    assert!(!stt_models.iter().any(|id| id == "openai/generic-audio"));
    assert!(!tts_models.iter().any(|id| id == "openai/generic-audio"));
}

#[test]
fn audio_model_picker_does_not_treat_nondirectional_modality_string_as_directional_support() {
    let (mut model, _daemon_rx) = make_model();
    model.config.agent_config_raw = Some(serde_json::json!({
        "audio": {
            "stt": {
                "provider": PROVIDER_ID_OPENROUTER,
                "model": "openai/gpt-stt-only"
            },
            "tts": {
                "provider": PROVIDER_ID_OPENROUTER,
                "model": "openai/gpt-tts-only"
            }
        }
    }));
    model
        .config
        .reduce(config::ConfigAction::ModelsFetched(vec![
            crate::state::config::FetchedModel {
                id: "openai/plain-modality-audio".to_string(),
                name: Some("Plain Modality Audio".to_string()),
                context_window: Some(128_000),
                pricing: Some(crate::state::config::FetchedModelPricing {
                    audio: Some("0.000032".to_string()),
                    ..Default::default()
                }),
                metadata: Some(serde_json::json!({
                    "architecture": {
                        "modality": "text+audio"
                    }
                })),
            },
        ]));

    model.settings_picker_target = Some(SettingsPickerTarget::AudioSttModel);
    let stt_models = model
        .available_model_picker_models()
        .into_iter()
        .map(|entry| entry.id)
        .collect::<Vec<_>>();

    model.settings_picker_target = Some(SettingsPickerTarget::AudioTtsModel);
    let tts_models = model
        .available_model_picker_models()
        .into_iter()
        .map(|entry| entry.id)
        .collect::<Vec<_>>();

    assert!(!stt_models
        .iter()
        .any(|id| id == "openai/plain-modality-audio"));
    assert!(!tts_models
        .iter()
        .any(|id| id == "openai/plain-modality-audio"));
}

#[test]
fn audio_model_picker_render_uses_same_filtered_models_as_selection() {
    let (mut model, _daemon_rx) = make_model();
    model.width = 120;
    model.height = 40;
    model.config.agent_config_raw = Some(serde_json::json!({
        "audio": {
            "stt": {
                "provider": PROVIDER_ID_OPENROUTER,
                "model": "openai/gpt-audio-mini"
            }
        }
    }));
    model
        .config
        .reduce(config::ConfigAction::ModelsFetched(vec![
            crate::state::config::FetchedModel {
                id: "anthropic/claude-opus-4.6".to_string(),
                name: Some("Anthropic: Claude Opus 4.6".to_string()),
                context_window: Some(1_000_000),
                pricing: Some(crate::state::config::FetchedModelPricing {
                    prompt: Some("0.000015".to_string()),
                    completion: Some("0.000075".to_string()),
                    ..Default::default()
                }),
                metadata: Some(serde_json::json!({
                    "architecture": {
                        "input_modalities": ["text"],
                        "output_modalities": ["text"]
                    }
                })),
            },
            crate::state::config::FetchedModel {
                id: "xiaomi/mimo-v2-omni".to_string(),
                name: Some("Xiaomi: MiMo-V2-Omni".to_string()),
                context_window: Some(262_000),
                pricing: Some(crate::state::config::FetchedModelPricing {
                    audio: Some("0.000032".to_string()),
                    ..Default::default()
                }),
                metadata: Some(serde_json::json!({
                    "architecture": {
                        "input_modalities": ["text", "audio"],
                        "output_modalities": ["text"]
                    }
                })),
            },
        ]));
    model.settings_picker_target = Some(SettingsPickerTarget::AudioSttModel);
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::ModelPicker));

    let screen = render_screen(&mut model).join("\n");

    assert!(
        !screen.contains("Anthropic: Claude Opus 4.6"),
        "audio picker should not render text-only fetched models"
    );
    assert!(
        screen.contains("Xiaomi: MiMo-V2-Omni"),
        "audio picker should render audio-capable fetched models"
    );
}

#[test]
fn protected_weles_editor_can_open_provider_model_role_and_effort_pickers() {
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
        editor.field = crate::state::subagents::SubAgentEditorField::Role;
    }
    let quit = model.handle_key_modal(
        KeyCode::Enter,
        KeyModifiers::NONE,
        modal::ModalKind::Settings,
    );
    assert!(!quit);
    assert_eq!(model.modal.top(), Some(modal::ModalKind::RolePicker));

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
fn subagent_role_picker_applies_selected_role_preset() {
    let (mut model, _daemon_rx) = make_model();
    let mut editor = crate::state::subagents::SubAgentEditorState::new(
        Some("worker".to_string()),
        1,
        PROVIDER_ID_OPENAI.to_string(),
        "gpt-5.4-mini".to_string(),
    );
    editor.field = crate::state::subagents::SubAgentEditorField::Role;
    model.subagents.editor = Some(editor);
    model.settings_picker_target = Some(SettingsPickerTarget::SubAgentRole);
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::RolePicker));

    let planning_index = crate::state::subagents::SUBAGENT_ROLE_PRESETS
        .iter()
        .position(|preset| preset.id == "planning")
        .expect("planning preset should exist");
    model
        .modal
        .reduce(modal::ModalAction::Navigate(planning_index as i32));

    let quit = model.handle_key_modal(
        KeyCode::Enter,
        KeyModifiers::NONE,
        modal::ModalKind::RolePicker,
    );

    assert!(!quit);
    assert!(model.modal.top().is_none());
    assert_eq!(
        model
            .subagents
            .editor
            .as_ref()
            .map(|editor| editor.role.as_str()),
        Some("planning")
    );
    assert_eq!(
        model
            .subagents
            .editor
            .as_ref()
            .map(|editor| editor.system_prompt.as_str()),
        crate::state::subagents::find_role_preset("planning").map(|preset| preset.system_prompt)
    );
    assert_eq!(model.status_line, "Sub-agent role: Planning");
    assert!(!model.settings.is_editing());
}

#[test]
fn subagent_role_picker_custom_option_starts_inline_edit() {
    let (mut model, _daemon_rx) = make_model();
    let mut editor = crate::state::subagents::SubAgentEditorState::new(
        Some("worker".to_string()),
        1,
        PROVIDER_ID_OPENAI.to_string(),
        "gpt-5.4-mini".to_string(),
    );
    editor.field = crate::state::subagents::SubAgentEditorField::Role;
    editor.role = "my_custom_role".to_string();
    model.subagents.editor = Some(editor);
    model
        .settings
        .reduce(SettingsAction::SwitchTab(SettingsTab::SubAgents));
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::Settings));
    model.settings_picker_target = Some(SettingsPickerTarget::SubAgentRole);
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::RolePicker));
    model.modal.reduce(modal::ModalAction::Navigate(
        crate::state::subagents::role_picker_custom_index() as i32,
    ));

    let quit = model.handle_key_modal(
        KeyCode::Enter,
        KeyModifiers::NONE,
        modal::ModalKind::RolePicker,
    );

    assert!(!quit);
    assert_eq!(model.modal.top(), Some(modal::ModalKind::Settings));
    assert_eq!(model.settings.editing_field(), Some("subagent_role"));
    assert_eq!(model.settings.edit_buffer(), "my_custom_role");
    assert_eq!(model.status_line, "Enter sub-agent role ID");
}

#[test]
fn enter_on_honcho_memory_opens_inline_editor() {
    let (mut model, _daemon_rx) = make_model();
    model
        .settings
        .reduce(SettingsAction::SwitchTab(SettingsTab::Chat));
    model
        .settings
        .navigate_field(2, model.settings_field_count());
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::Settings));

    let quit = model.handle_key_modal(
        KeyCode::Enter,
        KeyModifiers::NONE,
        modal::ModalKind::Settings,
    );

    assert!(!quit);
    assert!(model.config.honcho_editor.is_some());
}

#[test]
fn honcho_editor_save_updates_config() {
    let (mut model, _daemon_rx) = make_model();
    model.config.enable_honcho_memory = false;
    model.config.honcho_workspace_id = "tamux".to_string();
    model
        .settings
        .reduce(SettingsAction::SwitchTab(SettingsTab::Chat));
    model
        .settings
        .navigate_field(2, model.settings_field_count());
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::Settings));
    let _ = model.handle_key_modal(
        KeyCode::Enter,
        KeyModifiers::NONE,
        modal::ModalKind::Settings,
    );

    let editor = model
        .config
        .honcho_editor
        .as_mut()
        .expect("honcho editor should be open");
    editor.enabled = true;
    editor.api_key = "hc_test".to_string();
    editor.base_url = "https://honcho.example".to_string();
    editor.workspace_id = "tamux-lab".to_string();
    editor.field = crate::state::config::HonchoEditorField::Save;

    let quit = model.handle_key_modal(
        KeyCode::Enter,
        KeyModifiers::NONE,
        modal::ModalKind::Settings,
    );

    assert!(!quit);
    assert!(model.config.honcho_editor.is_none());
    assert!(model.config.enable_honcho_memory);
    assert_eq!(model.config.honcho_api_key, "hc_test");
    assert_eq!(model.config.honcho_base_url, "https://honcho.example");
    assert_eq!(model.config.honcho_workspace_id, "tamux-lab");
}

#[test]
fn honcho_editor_cancel_discards_staged_values() {
    let (mut model, _daemon_rx) = make_model();
    model.config.enable_honcho_memory = false;
    model.config.honcho_api_key = "persisted".to_string();
    model
        .settings
        .reduce(SettingsAction::SwitchTab(SettingsTab::Chat));
    model
        .settings
        .navigate_field(2, model.settings_field_count());
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::Settings));
    let _ = model.handle_key_modal(
        KeyCode::Enter,
        KeyModifiers::NONE,
        modal::ModalKind::Settings,
    );

    let editor = model
        .config
        .honcho_editor
        .as_mut()
        .expect("honcho editor should be open");
    editor.enabled = true;
    editor.api_key = "staged".to_string();

    let quit = model.handle_key_modal(KeyCode::Esc, KeyModifiers::NONE, modal::ModalKind::Settings);

    assert!(!quit);
    assert!(model.config.honcho_editor.is_none());
    assert!(!model.config.enable_honcho_memory);
    assert_eq!(model.config.honcho_api_key, "persisted");
}

#[test]
fn honcho_editor_space_toggles_staged_enabled_only() {
    let (mut model, _daemon_rx) = make_model();
    model.config.enable_honcho_memory = false;
    model
        .settings
        .reduce(SettingsAction::SwitchTab(SettingsTab::Chat));
    model
        .settings
        .navigate_field(2, model.settings_field_count());
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::Settings));
    let _ = model.handle_key_modal(
        KeyCode::Enter,
        KeyModifiers::NONE,
        modal::ModalKind::Settings,
    );

    let quit = model.handle_key_modal(
        KeyCode::Char(' '),
        KeyModifiers::NONE,
        modal::ModalKind::Settings,
    );

    assert!(!quit);
    assert!(!model.config.enable_honcho_memory);
    assert!(model
        .config
        .honcho_editor
        .as_ref()
        .is_some_and(|editor| editor.enabled));
}

#[test]
fn subagent_model_picker_uses_subagent_current_model_instead_of_primary_model() {
    let (mut model, _daemon_rx) = make_model();
    model.config.model = "gpt-5.4".to_string();
    model
        .config
        .reduce(config::ConfigAction::ModelsFetched(vec![
            crate::state::config::FetchedModel {
                id: "gpt-5.4-mini".to_string(),
                name: Some("GPT-5.4 Mini".to_string()),
                context_window: Some(128_000),
                pricing: None,
                metadata: None,
            },
        ]));

    let mut editor = crate::state::subagents::SubAgentEditorState::new(
        Some("weles_builtin".to_string()),
        1,
        PROVIDER_ID_OPENAI.to_string(),
        "claude-sonnet-4-5".to_string(),
    );
    editor.name = "WELES".to_string();
    editor.builtin = true;
    editor.immutable_identity = true;
    model.subagents.editor = Some(editor);
    model.settings_picker_target = Some(SettingsPickerTarget::SubAgentModel);
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::ModelPicker));

    let quit = model.handle_key_modal(
        KeyCode::Enter,
        KeyModifiers::NONE,
        modal::ModalKind::ModelPicker,
    );

    assert!(!quit);
    assert_eq!(
        model
            .subagents
            .editor
            .as_ref()
            .map(|editor| editor.model.as_str()),
        Some("claude-sonnet-4-5")
    );
    assert_eq!(model.config.model, "gpt-5.4");
}

#[test]
fn subagent_custom_model_entry_does_not_mutate_primary_model() {
    let (mut model, _daemon_rx) = make_model();
    model.config.model = "gpt-5.4".to_string();
    model
        .config
        .reduce(config::ConfigAction::ModelsFetched(vec![
            crate::state::config::FetchedModel {
                id: "gpt-5.4-mini".to_string(),
                name: Some("GPT-5.4 Mini".to_string()),
                context_window: Some(128_000),
                pricing: None,
                metadata: None,
            },
        ]));

    let mut editor = crate::state::subagents::SubAgentEditorState::new(
        Some("weles_builtin".to_string()),
        1,
        PROVIDER_ID_OPENAI.to_string(),
        "gpt-5.4-mini".to_string(),
    );
    editor.name = "WELES".to_string();
    editor.builtin = true;
    editor.immutable_identity = true;
    editor.field = crate::state::subagents::SubAgentEditorField::Model;
    model.subagents.editor = Some(editor);
    model
        .settings
        .reduce(SettingsAction::SwitchTab(SettingsTab::SubAgents));
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::Settings));
    model.settings_picker_target = Some(SettingsPickerTarget::SubAgentModel);
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
    assert_eq!(model.settings.editing_field(), Some("subagent_model"));
    model.settings.reduce(SettingsAction::InsertChar('x'));

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
            .map(|editor| editor.model.as_str()),
        Some("gpt-5.4-minix")
    );
    assert_eq!(model.config.model, "gpt-5.4");
}

#[test]
fn concierge_model_picker_uses_rarog_current_model_instead_of_primary_model() {
    let (mut model, _daemon_rx) = make_model();
    model.config.model = "gpt-5.4".to_string();
    model.concierge.model = Some("claude-sonnet-4-5".to_string());
    model
        .config
        .reduce(config::ConfigAction::ModelsFetched(vec![
            crate::state::config::FetchedModel {
                id: "gpt-5.4-mini".to_string(),
                name: Some("GPT-5.4 Mini".to_string()),
                context_window: Some(128_000),
                pricing: None,
                metadata: None,
            },
        ]));
    model.settings_picker_target = Some(SettingsPickerTarget::ConciergeModel);
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::ModelPicker));

    let quit = model.handle_key_modal(
        KeyCode::Enter,
        KeyModifiers::NONE,
        modal::ModalKind::ModelPicker,
    );

    assert!(!quit);
    assert_eq!(model.concierge.model.as_deref(), Some("claude-sonnet-4-5"));
    assert_eq!(model.config.model, "gpt-5.4");
}

#[test]
fn concierge_custom_model_entry_does_not_mutate_primary_model() {
    let (mut model, _daemon_rx) = make_model();
    model.config.model = "gpt-5.4".to_string();
    model.concierge.model = Some("gpt-5.4-mini".to_string());
    model
        .settings
        .reduce(SettingsAction::SwitchTab(SettingsTab::Concierge));
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::Settings));
    model
        .config
        .reduce(config::ConfigAction::ModelsFetched(vec![
            crate::state::config::FetchedModel {
                id: "gpt-5.4-mini".to_string(),
                name: Some("GPT-5.4 Mini".to_string()),
                context_window: Some(128_000),
                pricing: None,
                metadata: None,
            },
        ]));
    model.settings_picker_target = Some(SettingsPickerTarget::ConciergeModel);
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
    assert_eq!(model.settings.editing_field(), Some("concierge_model"));
    model.settings.reduce(SettingsAction::InsertChar('x'));

    let quit = model.handle_key_modal(
        KeyCode::Enter,
        KeyModifiers::NONE,
        modal::ModalKind::Settings,
    );

    assert!(!quit);
    assert_eq!(model.concierge.model.as_deref(), Some("gpt-5.4-minix"));
    assert_eq!(model.config.model, "gpt-5.4");
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
        modal::ThreadPickerTab::Goals
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
            assert_eq!(
                message_limit,
                Some(model.config.tui_chat_history_page_size as usize)
            );
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
fn slash_new_defaults_to_svarog_target_for_first_prompt() {
    let (mut model, mut daemon_rx) = make_model();
    model.connected = true;

    assert!(model.execute_slash_command_line("/new"));
    model.submit_prompt("default me".to_string());

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
                assert_eq!(
                    target_agent_id.as_deref(),
                    Some(amux_protocol::AGENT_ID_SWAROG)
                );
                assert_eq!(content, "default me");
                break;
            }
            other => panic!("expected send-message command, got {:?}", other),
        }
    }
}

#[test]
fn slash_new_with_custom_subagent_targets_first_prompt() {
    let (mut model, mut daemon_rx) = make_model();
    model.connected = true;
    model
        .subagents
        .entries
        .push(sample_subagent("domowoj", "Domowoj", false));

    assert!(model.execute_slash_command_line("/new domowoj"));
    model.submit_prompt("inspect the workspace".to_string());

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
                assert_eq!(target_agent_id.as_deref(), Some("domowoj"));
                assert_eq!(content, "inspect the workspace");
                break;
            }
            other => panic!("expected send-message command, got {:?}", other),
        }
    }
}

#[test]
fn slash_thread_with_agent_preselects_matching_source() {
    let (mut model, _daemon_rx) = make_model();
    model
        .subagents
        .entries
        .push(sample_subagent("domowoj", "Domowoj", false));

    assert!(model.execute_slash_command_line("/thread domowoj"));

    assert_eq!(model.modal.top(), Some(modal::ModalKind::ThreadPicker));
    assert_eq!(
        model.modal.thread_picker_tab(),
        modal::ThreadPickerTab::Agent("domowoj".to_string())
    );
}

#[test]
fn slash_image_prompt_dispatches_generate_image_for_active_thread() {
    let (mut model, mut daemon_rx) = make_model();
    model.connected = true;
    model.chat.reduce(chat::ChatAction::ThreadCreated {
        thread_id: "thread-1".to_string(),
        title: "Thread".to_string(),
    });
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-1".to_string()));

    assert!(model.execute_slash_command_line("/image retro robot portrait"));

    loop {
        match daemon_rx.try_recv() {
            Ok(DaemonCommand::DismissConciergeWelcome) => {}
            Ok(DaemonCommand::GenerateImage { args_json }) => {
                let payload: serde_json::Value =
                    serde_json::from_str(&args_json).expect("image payload should parse");
                assert_eq!(
                    payload.get("thread_id").and_then(|v| v.as_str()),
                    Some("thread-1")
                );
                assert_eq!(
                    payload.get("prompt").and_then(|v| v.as_str()),
                    Some("retro robot portrait")
                );
                break;
            }
            other => panic!("expected generate-image command, got {:?}", other),
        }
    }
}

#[test]
fn thread_picker_delete_requires_confirmation_before_sending_delete_thread() {
    let (mut model, mut daemon_rx) = make_model();
    model.chat.reduce(chat::ChatAction::ThreadListReceived(vec![
        chat::AgentThread {
            id: "thread-1".into(),
            title: "Thread One".into(),
            ..Default::default()
        },
    ]));
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::ThreadPicker));
    model.sync_thread_picker_item_count();
    model.modal.reduce(modal::ModalAction::Navigate(1));

    let quit = model.handle_key_modal(
        KeyCode::Delete,
        KeyModifiers::NONE,
        modal::ModalKind::ThreadPicker,
    );

    assert!(!quit);
    assert_eq!(model.modal.top(), Some(modal::ModalKind::ChatActionConfirm));
    assert!(
        daemon_rx.try_recv().is_err(),
        "delete should wait for confirmation"
    );

    let quit = model.handle_key_modal(
        KeyCode::Enter,
        KeyModifiers::NONE,
        modal::ModalKind::ChatActionConfirm,
    );

    assert!(!quit);
    assert!(matches!(
        daemon_rx.try_recv().expect("expected delete-thread command"),
        DaemonCommand::DeleteThread { thread_id } if thread_id == "thread-1"
    ));
}

#[test]
fn thread_picker_ctrl_s_busy_thread_requires_confirmation_before_stop() {
    let (mut model, mut daemon_rx) = make_model();
    model.chat.reduce(chat::ChatAction::ThreadListReceived(vec![
        chat::AgentThread {
            id: "thread-1".into(),
            title: "Thread One".into(),
            ..Default::default()
        },
    ]));
    model.open_thread_conversation("thread-1".into());
    loop {
        match daemon_rx.try_recv() {
            Ok(DaemonCommand::RequestThread { .. }) => break,
            Ok(_) => continue,
            Err(_) => panic!("expected thread request when opening conversation"),
        }
    }
    model.handle_delta_event("thread-1".into(), "streaming".into());
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::ThreadPicker));
    model.sync_thread_picker_item_count();
    model.modal.reduce(modal::ModalAction::Navigate(1));

    let quit = model.handle_key_modal(
        KeyCode::Char('s'),
        KeyModifiers::CONTROL,
        modal::ModalKind::ThreadPicker,
    );

    assert!(!quit);
    assert_eq!(model.modal.top(), Some(modal::ModalKind::ChatActionConfirm));

    let quit = model.handle_key_modal(
        KeyCode::Enter,
        KeyModifiers::NONE,
        modal::ModalKind::ChatActionConfirm,
    );

    assert!(!quit);
    assert!(matches!(
        daemon_rx.try_recv().expect("expected stop-stream command"),
        DaemonCommand::StopStream { thread_id } if thread_id == "thread-1"
    ));
}

#[test]
fn thread_picker_ctrl_s_on_stopped_thread_requires_confirmation_before_resume() {
    let (mut model, mut daemon_rx) = make_model();
    model.chat.reduce(chat::ChatAction::ThreadListReceived(vec![
        chat::AgentThread {
            id: "thread-1".into(),
            title: "Thread One".into(),
            messages: vec![chat::AgentMessage {
                role: chat::MessageRole::Assistant,
                content: "partial answer [stopped]".into(),
                ..Default::default()
            }],
            total_message_count: 1,
            loaded_message_end: 1,
            ..Default::default()
        },
    ]));
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::ThreadPicker));
    model.sync_thread_picker_item_count();
    model.modal.reduce(modal::ModalAction::Navigate(1));

    let quit = model.handle_key_modal(
        KeyCode::Char('s'),
        KeyModifiers::CONTROL,
        modal::ModalKind::ThreadPicker,
    );

    assert!(!quit);
    assert_eq!(model.modal.top(), Some(modal::ModalKind::ChatActionConfirm));

    let quit = model.handle_key_modal(
        KeyCode::Enter,
        KeyModifiers::NONE,
        modal::ModalKind::ChatActionConfirm,
    );

    assert!(!quit);
    assert!(matches!(
        daemon_rx.try_recv().expect("expected send-message resume command"),
        DaemonCommand::SendMessage {
            thread_id,
            content,
            ..
        } if thread_id.as_deref() == Some("thread-1") && content == "continue"
    ));
}

#[test]
fn goal_picker_delete_requires_confirmation_before_sending_delete_goal() {
    let (mut model, mut daemon_rx) = make_model();
    model
        .tasks
        .reduce(task::TaskAction::GoalRunListReceived(vec![make_goal_run(
            "goal-1",
            "Goal One",
            task::GoalRunStatus::Cancelled,
        )]));
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::GoalPicker));
    model.sync_goal_picker_item_count();
    model.modal.reduce(modal::ModalAction::Navigate(1));

    let quit = model.handle_key_modal(
        KeyCode::Delete,
        KeyModifiers::NONE,
        modal::ModalKind::GoalPicker,
    );

    assert!(!quit);
    assert_eq!(model.modal.top(), Some(modal::ModalKind::ChatActionConfirm));
    assert!(
        daemon_rx.try_recv().is_err(),
        "delete should wait for confirmation"
    );

    let quit = model.handle_key_modal(
        KeyCode::Enter,
        KeyModifiers::NONE,
        modal::ModalKind::ChatActionConfirm,
    );

    assert!(!quit);
    assert!(matches!(
        daemon_rx.try_recv().expect("expected delete-goal command"),
        DaemonCommand::DeleteGoalRun { goal_run_id } if goal_run_id == "goal-1"
    ));
}

#[test]
fn goal_picker_ctrl_s_running_goal_requires_confirmation_before_pause() {
    let (mut model, mut daemon_rx) = make_model();
    model
        .tasks
        .reduce(task::TaskAction::GoalRunListReceived(vec![make_goal_run(
            "goal-1",
            "Goal One",
            task::GoalRunStatus::Running,
        )]));
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::GoalPicker));
    model.sync_goal_picker_item_count();
    model.modal.reduce(modal::ModalAction::Navigate(1));

    let quit = model.handle_key_modal(
        KeyCode::Char('s'),
        KeyModifiers::CONTROL,
        modal::ModalKind::GoalPicker,
    );

    assert!(!quit);
    assert_eq!(model.modal.top(), Some(modal::ModalKind::ChatActionConfirm));
    assert_eq!(
        model
            .pending_chat_action_confirm
            .as_ref()
            .map(PendingConfirmAction::modal_body)
            .as_deref(),
        Some("Pause goal run \"Goal One\"?")
    );

    let quit = model.handle_key_modal(
        KeyCode::Enter,
        KeyModifiers::NONE,
        modal::ModalKind::ChatActionConfirm,
    );

    assert!(!quit);
    assert!(matches!(
        daemon_rx.try_recv().expect("expected control-goal command"),
        DaemonCommand::ControlGoalRun {
            goal_run_id,
            action,
            ..
        }
            if goal_run_id == "goal-1" && action == "pause"
    ));
    assert_eq!(model.status_line, "Pausing goal run...");
}

#[test]
fn goal_picker_ctrl_s_from_handle_key_routes_to_pause_confirmation() {
    let (mut model, _daemon_rx) = make_model();
    model
        .tasks
        .reduce(task::TaskAction::GoalRunListReceived(vec![make_goal_run(
            "goal-1",
            "Goal One",
            task::GoalRunStatus::Running,
        )]));
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::GoalPicker));
    model.sync_goal_picker_item_count();
    model.modal.reduce(modal::ModalAction::Navigate(1));

    let handled = model.handle_key(KeyCode::Char('s'), KeyModifiers::CONTROL);

    assert!(!handled);
    assert_eq!(model.modal.top(), Some(modal::ModalKind::ChatActionConfirm));
    assert_eq!(
        model
            .pending_chat_action_confirm
            .as_ref()
            .map(PendingConfirmAction::modal_body)
            .as_deref(),
        Some("Pause goal run \"Goal One\"?")
    );
}

#[test]
fn goal_picker_ctrl_s_paused_goal_requires_confirmation_before_resume() {
    let (mut model, mut daemon_rx) = make_model();
    model
        .tasks
        .reduce(task::TaskAction::GoalRunListReceived(vec![make_goal_run(
            "goal-1",
            "Goal One",
            task::GoalRunStatus::Paused,
        )]));
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::GoalPicker));
    model.sync_goal_picker_item_count();
    model.modal.reduce(modal::ModalAction::Navigate(1));

    let quit = model.handle_key_modal(
        KeyCode::Char('s'),
        KeyModifiers::CONTROL,
        modal::ModalKind::GoalPicker,
    );

    assert!(!quit);
    assert_eq!(model.modal.top(), Some(modal::ModalKind::ChatActionConfirm));

    let quit = model.handle_key_modal(
        KeyCode::Enter,
        KeyModifiers::NONE,
        modal::ModalKind::ChatActionConfirm,
    );

    assert!(!quit);
    assert!(matches!(
        daemon_rx.try_recv().expect("expected control-goal command"),
        DaemonCommand::ControlGoalRun {
            goal_run_id,
            action,
            ..
        }
            if goal_run_id == "goal-1" && action == "resume"
    ));
}

#[test]
fn goal_view_ctrl_s_paused_goal_requires_confirmation_before_resume() {
    let (mut model, _daemon_rx) = make_model();
    model.focus = FocusArea::Chat;
    model
        .tasks
        .reduce(task::TaskAction::GoalRunDetailReceived(make_goal_run(
            "goal-1",
            "Goal One",
            task::GoalRunStatus::Paused,
        )));
    model.main_pane_view = MainPaneView::Task(SidebarItemTarget::GoalRun {
        goal_run_id: "goal-1".to_string(),
        step_id: None,
    });

    let handled = model.handle_key(KeyCode::Char('s'), KeyModifiers::CONTROL);

    assert!(!handled);
    assert_eq!(model.modal.top(), Some(modal::ModalKind::ChatActionConfirm));
    assert_eq!(
        model
            .pending_chat_action_confirm
            .as_ref()
            .map(PendingConfirmAction::modal_body)
            .as_deref(),
        Some("Resume goal run \"Goal One\"?")
    );
}

#[test]
fn goal_view_action_menu_can_pause_running_goal_without_step_selection() {
    let (mut model, mut daemon_rx) = make_model();
    model.focus = FocusArea::Chat;
    model
        .tasks
        .reduce(task::TaskAction::GoalRunDetailReceived(make_goal_run(
            "goal-1",
            "Goal One",
            task::GoalRunStatus::Running,
        )));
    model.main_pane_view = MainPaneView::Task(SidebarItemTarget::GoalRun {
        goal_run_id: "goal-1".to_string(),
        step_id: None,
    });

    let handled = model.handle_key(KeyCode::Char('a'), KeyModifiers::NONE);

    assert!(!handled);
    assert_eq!(
        model.modal.top(),
        Some(modal::ModalKind::GoalStepActionPicker)
    );

    let handled = model.handle_key_modal(
        KeyCode::Enter,
        KeyModifiers::NONE,
        modal::ModalKind::GoalStepActionPicker,
    );
    assert!(!handled);
    assert_eq!(model.modal.top(), Some(modal::ModalKind::ChatActionConfirm));
    assert_eq!(
        model
            .pending_chat_action_confirm
            .as_ref()
            .map(PendingConfirmAction::modal_body)
            .as_deref(),
        Some("Pause goal run \"Goal One\"?")
    );

    let handled = model.handle_key_modal(
        KeyCode::Enter,
        KeyModifiers::NONE,
        modal::ModalKind::ChatActionConfirm,
    );
    assert!(!handled);
    assert!(matches!(
        daemon_rx.try_recv().expect("expected control-goal command"),
        DaemonCommand::ControlGoalRun {
            goal_run_id,
            action,
            step_index: None,
        } if goal_run_id == "goal-1" && action == "pause"
    ));
}

#[test]
fn goal_view_retry_uses_current_step_without_explicit_step_selection() {
    let (mut model, _daemon_rx) = make_model();
    model.focus = FocusArea::Chat;
    model.tasks.reduce(task::TaskAction::GoalRunDetailReceived(
        make_goal_run_with_steps(
            "goal-1",
            "Goal One",
            task::GoalRunStatus::Failed,
            vec![
                task::GoalRunStep {
                    id: "step-1".to_string(),
                    title: "Plan".to_string(),
                    order: 0,
                    status: Some(task::GoalRunStatus::Completed),
                    ..Default::default()
                },
                task::GoalRunStep {
                    id: "step-2".to_string(),
                    title: "Deploy".to_string(),
                    order: 1,
                    status: Some(task::GoalRunStatus::Failed),
                    ..Default::default()
                },
            ],
        ),
    ));
    if let Some(run) = model.tasks.goal_run_by_id_mut("goal-1") {
        run.current_step_index = 1;
        run.current_step_title = Some("Deploy".to_string());
    }
    model.main_pane_view = MainPaneView::Task(SidebarItemTarget::GoalRun {
        goal_run_id: "goal-1".to_string(),
        step_id: None,
    });

    let handled = model.handle_key(KeyCode::Char('r'), KeyModifiers::NONE);

    assert!(!handled);
    assert_eq!(model.modal.top(), Some(modal::ModalKind::ChatActionConfirm));
    assert_eq!(
        model
            .pending_chat_action_confirm
            .as_ref()
            .map(PendingConfirmAction::modal_body)
            .as_deref(),
        Some("Retry step 2 \"Deploy\" in goal \"Goal One\"?")
    );
}

#[test]
fn goal_view_retry_from_prompt_without_steps_opens_confirmation() {
    let (mut model, _daemon_rx) = make_model();
    model.focus = FocusArea::Chat;
    model
        .tasks
        .reduce(task::TaskAction::GoalRunDetailReceived(make_goal_run(
            "goal-1",
            "Goal One",
            task::GoalRunStatus::Failed,
        )));
    model.main_pane_view = MainPaneView::Task(SidebarItemTarget::GoalRun {
        goal_run_id: "goal-1".to_string(),
        step_id: None,
    });

    let handled = model.handle_key(KeyCode::Char('r'), KeyModifiers::NONE);

    assert!(!handled);
    assert_eq!(model.modal.top(), Some(modal::ModalKind::ChatActionConfirm));
    assert_eq!(
        model
            .pending_chat_action_confirm
            .as_ref()
            .map(PendingConfirmAction::modal_body)
            .as_deref(),
        Some("Retry goal \"Goal One\" from the current prompt?")
    );
}

#[test]
fn goal_view_rerun_from_prompt_without_steps_opens_confirmation() {
    let (mut model, _daemon_rx) = make_model();
    model.focus = FocusArea::Chat;
    model
        .tasks
        .reduce(task::TaskAction::GoalRunDetailReceived(make_goal_run(
            "goal-1",
            "Goal One",
            task::GoalRunStatus::Failed,
        )));
    model.main_pane_view = MainPaneView::Task(SidebarItemTarget::GoalRun {
        goal_run_id: "goal-1".to_string(),
        step_id: None,
    });

    let handled = model.handle_key(KeyCode::Char('R'), KeyModifiers::SHIFT);

    assert!(!handled);
    assert_eq!(model.modal.top(), Some(modal::ModalKind::ChatActionConfirm));
    assert_eq!(
        model
            .pending_chat_action_confirm
            .as_ref()
            .map(PendingConfirmAction::modal_body)
            .as_deref(),
        Some("Rerun goal \"Goal One\" from the current prompt?")
    );
}

#[test]
fn goal_view_ctrl_r_requests_authoritative_goal_refresh() {
    let (mut model, mut daemon_rx) = make_model();
    model.focus = FocusArea::Chat;
    model
        .tasks
        .reduce(task::TaskAction::GoalRunDetailReceived(make_goal_run(
            "goal-1",
            "Goal One",
            task::GoalRunStatus::Running,
        )));
    model.main_pane_view = MainPaneView::Task(SidebarItemTarget::GoalRun {
        goal_run_id: "goal-1".to_string(),
        step_id: None,
    });

    let handled = model.handle_key(KeyCode::Char('r'), KeyModifiers::CONTROL);

    assert!(!handled);
    assert_eq!(
        next_goal_run_detail_request(&mut daemon_rx).as_deref(),
        Some("goal-1")
    );
    assert_eq!(
        next_goal_run_checkpoints_request(&mut daemon_rx).as_deref(),
        Some("goal-1")
    );
    assert!(matches!(daemon_rx.try_recv(), Ok(DaemonCommand::Refresh)));
    assert!(matches!(
        daemon_rx.try_recv(),
        Ok(DaemonCommand::RefreshServices)
    ));
}

#[test]
fn goal_workspace_refresh_action_requests_authoritative_goal_refresh() {
    let (mut model, mut daemon_rx) = make_model();
    model.focus = FocusArea::Chat;
    model
        .tasks
        .reduce(task::TaskAction::GoalRunDetailReceived(make_goal_run(
            "goal-1",
            "Goal One",
            task::GoalRunStatus::Running,
        )));
    model.main_pane_view = MainPaneView::Task(SidebarItemTarget::GoalRun {
        goal_run_id: "goal-1".to_string(),
        step_id: None,
    });

    let handled = model.activate_goal_workspace_action(
        crate::widgets::goal_workspace::GoalWorkspaceAction::RefreshGoal,
    );

    assert!(handled);
    assert_eq!(
        next_goal_run_detail_request(&mut daemon_rx).as_deref(),
        Some("goal-1")
    );
    assert_eq!(
        next_goal_run_checkpoints_request(&mut daemon_rx).as_deref(),
        Some("goal-1")
    );
    assert!(matches!(daemon_rx.try_recv(), Ok(DaemonCommand::Refresh)));
    assert!(matches!(
        daemon_rx.try_recv(),
        Ok(DaemonCommand::RefreshServices)
    ));
}

#[test]
fn selected_goal_step_r_opens_retry_confirmation() {
    let (mut model, _daemon_rx) = make_model();
    model.focus = FocusArea::Chat;
    model.tasks.reduce(task::TaskAction::GoalRunDetailReceived(
        make_goal_run_with_steps(
            "goal-1",
            "Goal One",
            task::GoalRunStatus::Failed,
            vec![
                task::GoalRunStep {
                    id: "step-1".to_string(),
                    title: "Plan".to_string(),
                    order: 0,
                    status: Some(task::GoalRunStatus::Completed),
                    ..Default::default()
                },
                task::GoalRunStep {
                    id: "step-2".to_string(),
                    title: "Deploy".to_string(),
                    order: 1,
                    status: Some(task::GoalRunStatus::Failed),
                    ..Default::default()
                },
            ],
        ),
    ));
    model.main_pane_view = MainPaneView::Task(SidebarItemTarget::GoalRun {
        goal_run_id: "goal-1".to_string(),
        step_id: Some("step-2".to_string()),
    });

    let handled = model.handle_key(KeyCode::Char('r'), KeyModifiers::NONE);

    assert!(!handled);
    assert_eq!(model.modal.top(), Some(modal::ModalKind::ChatActionConfirm));
    assert_eq!(
        model
            .pending_chat_action_confirm
            .as_ref()
            .map(PendingConfirmAction::modal_body)
            .as_deref(),
        Some("Retry step 2 \"Deploy\" in goal \"Goal One\"?")
    );
}

#[test]
fn selected_goal_step_shift_r_opens_rerun_confirmation() {
    let (mut model, _daemon_rx) = make_model();
    model.focus = FocusArea::Chat;
    model.tasks.reduce(task::TaskAction::GoalRunDetailReceived(
        make_goal_run_with_steps(
            "goal-1",
            "Goal One",
            task::GoalRunStatus::Failed,
            vec![
                task::GoalRunStep {
                    id: "step-1".to_string(),
                    title: "Plan".to_string(),
                    order: 0,
                    status: Some(task::GoalRunStatus::Completed),
                    ..Default::default()
                },
                task::GoalRunStep {
                    id: "step-2".to_string(),
                    title: "Deploy".to_string(),
                    order: 1,
                    status: Some(task::GoalRunStatus::Failed),
                    ..Default::default()
                },
            ],
        ),
    ));
    model.main_pane_view = MainPaneView::Task(SidebarItemTarget::GoalRun {
        goal_run_id: "goal-1".to_string(),
        step_id: Some("step-2".to_string()),
    });

    let handled = model.handle_key(KeyCode::Char('R'), KeyModifiers::SHIFT);

    assert!(!handled);
    assert_eq!(model.modal.top(), Some(modal::ModalKind::ChatActionConfirm));
    assert_eq!(
        model
            .pending_chat_action_confirm
            .as_ref()
            .map(PendingConfirmAction::modal_body)
            .as_deref(),
        Some("Rerun from step 2 \"Deploy\" in goal \"Goal One\"?")
    );
}

#[test]
fn selected_goal_step_shift_r_lowercase_key_opens_rerun_confirmation() {
    let (mut model, _daemon_rx) = make_model();
    model.focus = FocusArea::Chat;
    model.tasks.reduce(task::TaskAction::GoalRunDetailReceived(
        make_goal_run_with_steps(
            "goal-1",
            "Goal One",
            task::GoalRunStatus::Failed,
            vec![
                task::GoalRunStep {
                    id: "step-1".to_string(),
                    title: "Plan".to_string(),
                    order: 0,
                    status: Some(task::GoalRunStatus::Completed),
                    ..Default::default()
                },
                task::GoalRunStep {
                    id: "step-2".to_string(),
                    title: "Deploy".to_string(),
                    order: 1,
                    status: Some(task::GoalRunStatus::Failed),
                    ..Default::default()
                },
            ],
        ),
    ));
    model.main_pane_view = MainPaneView::Task(SidebarItemTarget::GoalRun {
        goal_run_id: "goal-1".to_string(),
        step_id: Some("step-2".to_string()),
    });

    let handled = model.handle_key(KeyCode::Char('r'), KeyModifiers::SHIFT);

    assert!(!handled);
    assert_eq!(model.modal.top(), Some(modal::ModalKind::ChatActionConfirm));
    assert_eq!(
        model
            .pending_chat_action_confirm
            .as_ref()
            .map(PendingConfirmAction::modal_body)
            .as_deref(),
        Some("Rerun from step 2 \"Deploy\" in goal \"Goal One\"?")
    );
}

#[test]
fn selected_goal_step_action_menu_can_send_retry_step() {
    let (mut model, mut daemon_rx) = make_model();
    model.focus = FocusArea::Chat;
    model.tasks.reduce(task::TaskAction::GoalRunDetailReceived(
        make_goal_run_with_steps(
            "goal-1",
            "Goal One",
            task::GoalRunStatus::Failed,
            vec![
                task::GoalRunStep {
                    id: "step-1".to_string(),
                    title: "Plan".to_string(),
                    order: 0,
                    status: Some(task::GoalRunStatus::Completed),
                    ..Default::default()
                },
                task::GoalRunStep {
                    id: "step-2".to_string(),
                    title: "Deploy".to_string(),
                    order: 1,
                    status: Some(task::GoalRunStatus::Failed),
                    ..Default::default()
                },
            ],
        ),
    ));
    model.main_pane_view = MainPaneView::Task(SidebarItemTarget::GoalRun {
        goal_run_id: "goal-1".to_string(),
        step_id: Some("step-2".to_string()),
    });

    let handled = model.handle_key(KeyCode::Char('a'), KeyModifiers::NONE);
    assert!(!handled);
    assert_eq!(
        model.modal.top(),
        Some(modal::ModalKind::GoalStepActionPicker)
    );

    let handled = model.handle_key_modal(
        KeyCode::Enter,
        KeyModifiers::NONE,
        modal::ModalKind::GoalStepActionPicker,
    );
    assert!(!handled);
    assert_eq!(model.modal.top(), Some(modal::ModalKind::ChatActionConfirm));

    let handled = model.handle_key_modal(
        KeyCode::Enter,
        KeyModifiers::NONE,
        modal::ModalKind::ChatActionConfirm,
    );
    assert!(!handled);
    assert!(matches!(
        daemon_rx.try_recv().expect("expected control-goal command"),
        DaemonCommand::ControlGoalRun {
            goal_run_id,
            action,
            step_index: Some(1),
        } if goal_run_id == "goal-1" && action == "retry_step"
    ));
}

#[test]
fn thread_picker_shift_r_requests_refresh() {
    let (mut model, mut daemon_rx) = make_model();
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::ThreadPicker));
    model.sync_thread_picker_item_count();

    let handled = model.handle_key_modal(
        KeyCode::Char('R'),
        KeyModifiers::SHIFT,
        modal::ModalKind::ThreadPicker,
    );

    assert!(!handled);
    assert!(matches!(daemon_rx.try_recv(), Ok(DaemonCommand::Refresh)));
}

#[test]
fn thread_picker_shift_r_lowercase_key_requests_refresh() {
    let (mut model, mut daemon_rx) = make_model();
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::ThreadPicker));
    model.sync_thread_picker_item_count();

    let handled = model.handle_key_modal(
        KeyCode::Char('r'),
        KeyModifiers::SHIFT,
        modal::ModalKind::ThreadPicker,
    );

    assert!(!handled);
    assert!(matches!(daemon_rx.try_recv(), Ok(DaemonCommand::Refresh)));
}

#[test]
fn goal_picker_shift_r_requests_refresh() {
    let (mut model, mut daemon_rx) = make_model();
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::GoalPicker));
    model.sync_goal_picker_item_count();

    let handled = model.handle_key_modal(
        KeyCode::Char('R'),
        KeyModifiers::SHIFT,
        modal::ModalKind::GoalPicker,
    );

    assert!(!handled);
    assert!(matches!(daemon_rx.try_recv(), Ok(DaemonCommand::Refresh)));
}

#[test]
fn goal_picker_shift_r_requests_goal_run_list_refresh() {
    let (mut model, mut daemon_rx) = make_model();
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::GoalPicker));
    model.sync_goal_picker_item_count();

    let handled = model.handle_key_modal(
        KeyCode::Char('R'),
        KeyModifiers::SHIFT,
        modal::ModalKind::GoalPicker,
    );

    assert!(!handled);
    assert!(matches!(daemon_rx.try_recv(), Ok(DaemonCommand::Refresh)));
    assert!(matches!(
        daemon_rx.try_recv(),
        Ok(DaemonCommand::RefreshServices)
    ));
}

#[test]
fn goal_picker_shift_r_lowercase_key_requests_refresh() {
    let (mut model, mut daemon_rx) = make_model();
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::GoalPicker));
    model.sync_goal_picker_item_count();

    let handled = model.handle_key_modal(
        KeyCode::Char('r'),
        KeyModifiers::SHIFT,
        modal::ModalKind::GoalPicker,
    );

    assert!(!handled);
    assert!(matches!(daemon_rx.try_recv(), Ok(DaemonCommand::Refresh)));
}

#[test]
fn goal_picker_open_selected_running_goal_starts_background_hydration() {
    let (mut model, mut daemon_rx) = make_model();
    model
        .tasks
        .reduce(task::TaskAction::GoalRunListReceived(vec![make_goal_run(
            "goal-1",
            "Goal One",
            task::GoalRunStatus::Running,
        )]));
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::GoalPicker));
    model.sync_goal_picker_item_count();
    model.modal.reduce(modal::ModalAction::Navigate(1));

    model.handle_modal_enter(modal::ModalKind::GoalPicker);

    assert!(matches!(
        model.main_pane_view,
        MainPaneView::Task(SidebarItemTarget::GoalRun { ref goal_run_id, step_id: None })
            if goal_run_id == "goal-1"
    ));
    assert_eq!(
        next_goal_run_detail_request(&mut daemon_rx).as_deref(),
        Some("goal-1")
    );
    assert_eq!(
        next_goal_run_checkpoints_request(&mut daemon_rx).as_deref(),
        Some("goal-1")
    );
    assert_eq!(
        next_goal_hydration_schedule(&mut daemon_rx).as_deref(),
        Some("goal-1")
    );
    assert!(
        model.pending_goal_hydration_refreshes.contains("goal-1"),
        "opening a live goal should keep background hydration armed for new timeline events"
    );
}

#[test]
fn sidebar_goal_enter_opens_selected_child_task() {
    let (mut model, _daemon_rx) = seed_goal_sidebar_model();
    model.goal_sidebar.cycle_tab_right();
    model.goal_sidebar.cycle_tab_right();
    model.goal_sidebar.navigate(1, 2);

    let handled = model.handle_goal_sidebar_enter();

    assert!(handled);
    assert_eq!(model.focus, FocusArea::Chat);
    assert!(matches!(
        model.main_pane_view,
        MainPaneView::Task(SidebarItemTarget::Task { task_id }) if task_id == "task-2"
    ));
}

#[test]
fn sidebar_goal_enter_skips_stale_child_task_ids() {
    let (mut model, _daemon_rx) = seed_goal_sidebar_model();
    if let Some(run) = model.tasks.goal_run_by_id_mut("goal-1") {
        run.child_task_ids = vec!["missing-task".to_string(), "task-1".to_string()];
    }
    model.goal_sidebar.cycle_tab_right();
    model.goal_sidebar.cycle_tab_right();

    let handled = model.handle_goal_sidebar_enter();

    assert!(handled);
    assert_eq!(model.focus, FocusArea::Chat);
    assert!(matches!(
        model.main_pane_view,
        MainPaneView::Task(SidebarItemTarget::Task { task_id }) if task_id == "task-1"
    ));
}

#[test]
fn sidebar_goal_enter_opens_selected_work_context_file() {
    let (mut model, _daemon_rx) = seed_goal_sidebar_model();
    model.goal_sidebar.cycle_tab_right();
    model.goal_sidebar.cycle_tab_right();
    model.goal_sidebar.cycle_tab_right();
    model.goal_sidebar.navigate(1, 2);

    let handled = model.handle_goal_sidebar_enter();

    assert!(handled);
    assert_eq!(model.focus, FocusArea::Chat);
    assert!(matches!(model.main_pane_view, MainPaneView::WorkContext));
    assert_eq!(
        model.tasks.selected_work_path("thread-1"),
        Some("/tmp/second.md")
    );
}

#[test]
fn sidebar_goal_enter_checkpoint_with_step_index_selects_goal_step() {
    let (mut model, _daemon_rx) = seed_goal_sidebar_model();
    model.goal_sidebar.cycle_tab_right();

    let handled = model.handle_goal_sidebar_enter();

    assert!(handled);
    assert_eq!(model.focus, FocusArea::Chat);
    assert!(matches!(
        model.main_pane_view,
        MainPaneView::Task(SidebarItemTarget::GoalRun {
            goal_run_id,
            step_id: Some(step_id),
        }) if goal_run_id == "goal-1" && step_id == "step-2"
    ));
}

#[test]
fn sidebar_goal_enter_checkpoint_without_step_index_is_non_destructive() {
    let (mut model, _daemon_rx) = seed_goal_sidebar_model();
    model.goal_sidebar.cycle_tab_right();
    model.goal_sidebar.navigate(1, 2);

    let handled = model.handle_goal_sidebar_enter();

    assert!(!handled);
    assert_eq!(model.focus, FocusArea::Sidebar);
    assert!(matches!(
        model.main_pane_view,
        MainPaneView::Task(SidebarItemTarget::GoalRun {
            goal_run_id,
            step_id: Some(step_id),
        }) if goal_run_id == "goal-1" && step_id == "step-1"
    ));
}

#[test]
fn goal_run_task_view_bracket_keys_cycle_selected_step() {
    let (mut model, _daemon_rx) = make_model();
    model.focus = FocusArea::Chat;
    model.tasks.reduce(task::TaskAction::GoalRunDetailReceived(
        make_goal_run_with_steps(
            "goal-1",
            "Goal One",
            task::GoalRunStatus::Running,
            vec![
                task::GoalRunStep {
                    id: "step-1".to_string(),
                    title: "Plan".to_string(),
                    order: 0,
                    status: Some(task::GoalRunStatus::Completed),
                    ..Default::default()
                },
                task::GoalRunStep {
                    id: "step-2".to_string(),
                    title: "Execute".to_string(),
                    order: 1,
                    status: Some(task::GoalRunStatus::Running),
                    ..Default::default()
                },
                task::GoalRunStep {
                    id: "step-3".to_string(),
                    title: "Verify".to_string(),
                    order: 2,
                    status: Some(task::GoalRunStatus::Queued),
                    ..Default::default()
                },
            ],
        ),
    ));
    model.main_pane_view = MainPaneView::Task(SidebarItemTarget::GoalRun {
        goal_run_id: "goal-1".to_string(),
        step_id: Some("step-2".to_string()),
    });

    let handled = model.handle_key(KeyCode::Char(']'), KeyModifiers::NONE);
    assert!(!handled);
    assert!(matches!(
        model.main_pane_view,
        MainPaneView::Task(SidebarItemTarget::GoalRun {
            ref goal_run_id,
            step_id: Some(ref step_id),
        }) if goal_run_id == "goal-1" && step_id == "step-3"
    ));

    let handled = model.handle_key(KeyCode::Char('['), KeyModifiers::NONE);
    assert!(!handled);
    assert!(matches!(
        model.main_pane_view,
        MainPaneView::Task(SidebarItemTarget::GoalRun {
            ref goal_run_id,
            step_id: Some(ref step_id),
        }) if goal_run_id == "goal-1" && step_id == "step-2"
    ));
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
fn thread_picker_new_conversation_uses_dynamic_agent_tab_for_first_prompt() {
    let (mut model, mut daemon_rx) = make_model();
    model.connected = true;
    model
        .subagents
        .entries
        .push(sample_subagent("domowoj", "Domowoj", false));
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::ThreadPicker));
    model
        .modal
        .set_thread_picker_tab(modal::ThreadPickerTab::Agent("domowoj".to_string()));
    model.sync_thread_picker_item_count();

    let quit = model.handle_key_modal(
        KeyCode::Enter,
        KeyModifiers::NONE,
        modal::ModalKind::ThreadPicker,
    );
    assert!(!quit);

    model.submit_prompt("look around".to_string());

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
                assert_eq!(target_agent_id.as_deref(), Some("domowoj"));
                assert_eq!(content, "look around");
                break;
            }
            other => panic!("expected send-message command, got {:?}", other),
        }
    }
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
                if widgets::thread_picker::hit_test(
                    overlay_area,
                    &model.chat,
                    &model.modal,
                    &model.subagents,
                    pos,
                ) == Some(widgets::thread_picker::ThreadPickerHitTarget::Tab(
                    modal::ThreadPickerTab::Rarog,
                )) {
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
                model.voice_recording,
                model.voice_player.is_some(),
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
    model.agent_config_loaded = true;
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

#[test]
fn settings_textarea_ctrl_j_confirms_whatsapp_allowlist_edit() {
    let (mut model, _daemon_rx) = make_model();
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::Settings));
    model
        .settings
        .reduce(SettingsAction::SwitchTab(SettingsTab::Gateway));
    model
        .settings
        .start_editing("whatsapp_allowed_contacts", "123123123123");
    model.settings.reduce(SettingsAction::InsertChar('\n'));
    model.settings.reduce(SettingsAction::InsertChar('4'));

    let quit = model.handle_key_modal(
        KeyCode::Char('j'),
        KeyModifiers::CONTROL,
        modal::ModalKind::Settings,
    );

    assert!(!quit);
    assert_eq!(model.settings.editing_field(), None);
    assert_eq!(model.config.whatsapp_allowed_contacts, "123123123123\n4");
}

#[test]
fn settings_textarea_ctrl_m_confirms_whatsapp_allowlist_edit() {
    let (mut model, _daemon_rx) = make_model();
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::Settings));
    model
        .settings
        .reduce(SettingsAction::SwitchTab(SettingsTab::Gateway));
    model
        .settings
        .start_editing("whatsapp_allowed_contacts", "123123123123");
    model.settings.reduce(SettingsAction::InsertChar('\n'));
    model.settings.reduce(SettingsAction::InsertChar('5'));

    let quit = model.handle_key_modal(
        KeyCode::Char('m'),
        KeyModifiers::CONTROL,
        modal::ModalKind::Settings,
    );

    assert!(!quit);
    assert_eq!(model.settings.editing_field(), None);
    assert_eq!(model.config.whatsapp_allowed_contacts, "123123123123\n5");
}

#[test]
fn settings_textarea_ctrl_s_confirms_whatsapp_allowlist_edit() {
    let (mut model, _daemon_rx) = make_model();
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::Settings));
    model
        .settings
        .reduce(SettingsAction::SwitchTab(SettingsTab::Gateway));
    model
        .settings
        .start_editing("whatsapp_allowed_contacts", "123123123123");
    model.settings.reduce(SettingsAction::InsertChar('\n'));
    model.settings.reduce(SettingsAction::InsertChar('6'));

    let quit = model.handle_key_modal(
        KeyCode::Char('s'),
        KeyModifiers::CONTROL,
        modal::ModalKind::Settings,
    );

    assert!(!quit);
    assert_eq!(model.settings.editing_field(), None);
    assert_eq!(model.config.whatsapp_allowed_contacts, "123123123123\n6");
}

#[test]
fn subagent_system_prompt_textarea_supports_arrow_keys() {
    let (mut model, _daemon_rx) = make_model();
    let editor = crate::state::subagents::SubAgentEditorState::new(
        None,
        1,
        "openai".to_string(),
        "gpt-5.4".to_string(),
    );
    model.subagents.editor = Some(editor);
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::Settings));
    model
        .settings
        .reduce(SettingsAction::SwitchTab(SettingsTab::SubAgents));
    model
        .settings
        .start_editing("subagent_system_prompt", "abc\ndef");

    let quit = model.handle_key_modal(
        KeyCode::Left,
        KeyModifiers::NONE,
        modal::ModalKind::Settings,
    );
    assert!(!quit);
    assert_eq!(model.settings.edit_cursor_line_col(), (1, 2));

    let quit = model.handle_key_modal(KeyCode::Up, KeyModifiers::NONE, modal::ModalKind::Settings);
    assert!(!quit);
    assert_eq!(model.settings.edit_cursor_line_col(), (0, 2));

    let quit = model.handle_key_modal(
        KeyCode::Right,
        KeyModifiers::NONE,
        modal::ModalKind::Settings,
    );
    assert!(!quit);
    assert_eq!(model.settings.edit_cursor_line_col(), (0, 3));

    let quit = model.handle_key_modal(
        KeyCode::Down,
        KeyModifiers::NONE,
        modal::ModalKind::Settings,
    );
    assert!(!quit);
    assert_eq!(model.settings.edit_cursor_line_col(), (1, 3));
}

#[test]
fn subagent_editor_navigation_wraps_between_first_and_last_fields() {
    let (mut model, _daemon_rx) = make_model();
    let editor = crate::state::subagents::SubAgentEditorState::new(
        None,
        1,
        "openai".to_string(),
        "gpt-5.4".to_string(),
    );
    model.subagents.editor = Some(editor);
    model
        .modal
        .reduce(modal::ModalAction::Push(modal::ModalKind::Settings));
    model
        .settings
        .reduce(SettingsAction::SwitchTab(SettingsTab::SubAgents));

    let quit = model.handle_key_modal(KeyCode::Up, KeyModifiers::NONE, modal::ModalKind::Settings);
    assert!(!quit);
    assert_eq!(
        model.subagents.editor.as_ref().map(|editor| editor.field),
        Some(crate::state::subagents::SubAgentEditorField::Cancel)
    );

    let quit = model.handle_key_modal(
        KeyCode::Down,
        KeyModifiers::NONE,
        modal::ModalKind::Settings,
    );
    assert!(!quit);
    assert_eq!(
        model.subagents.editor.as_ref().map(|editor| editor.field),
        Some(crate::state::subagents::SubAgentEditorField::Name)
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
fn notifications_modal_shift_r_lowercase_marks_all_read() {
    let (mut model, _daemon_rx) = make_model();
    let mut unread = sample_notification(None);
    unread.id = "n-unread".to_string();
    let mut still_read = sample_notification(Some(5));
    still_read.id = "n-read".to_string();
    model
        .notifications
        .reduce(crate::state::NotificationsAction::Replace(vec![
            unread, still_read,
        ]));
    model.toggle_notifications_modal();

    let handled = model.handle_key_modal(
        KeyCode::Char('r'),
        KeyModifiers::SHIFT,
        modal::ModalKind::Notifications,
    );

    assert!(!handled);
    assert_eq!(model.notifications.unread_count(), 0);
}

#[test]
fn notifications_modal_shift_a_lowercase_archives_read() {
    let (mut model, _daemon_rx) = make_model();
    let mut unread = sample_notification(None);
    unread.id = "n-unread".to_string();
    unread.updated_at = 10;
    let mut read = sample_notification(Some(5));
    read.id = "n-read".to_string();
    read.updated_at = 5;
    model
        .notifications
        .reduce(crate::state::NotificationsAction::Replace(vec![
            unread, read,
        ]));
    model.toggle_notifications_modal();

    let handled = model.handle_key_modal(
        KeyCode::Char('a'),
        KeyModifiers::SHIFT,
        modal::ModalKind::Notifications,
    );

    assert!(!handled);
    let active_ids = model
        .notifications
        .active_items()
        .into_iter()
        .map(|item| item.id.clone())
        .collect::<Vec<_>>();
    assert_eq!(active_ids, vec!["n-unread".to_string()]);
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

#[test]
fn pinned_budget_modal_dismiss_restores_chat_focus() {
    let (mut model, _daemon_rx) = make_model();
    model.chat.reduce(chat::ChatAction::ThreadCreated {
        thread_id: "thread-1".to_string(),
        title: "Pinned".to_string(),
    });
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-1".to_string()));
    model.chat.reduce(chat::ChatAction::AppendMessage {
        thread_id: "thread-1".to_string(),
        message: chat::AgentMessage {
            id: Some("message-1".to_string()),
            role: chat::MessageRole::Assistant,
            content: "Pinned content".to_string(),
            ..Default::default()
        },
    });
    model.chat.select_message(Some(0));
    model.pending_pinned_budget_exceeded = Some(crate::app::PendingPinnedBudgetExceeded {
        thread_id: "thread-1".to_string(),
        message_id: "message-1".to_string(),
        current_pinned_chars: 100,
        pinned_budget_chars: 120,
        candidate_pinned_chars: 160,
    });
    model.modal.reduce(modal::ModalAction::Push(
        modal::ModalKind::PinnedBudgetExceeded,
    ));

    let quit = model.handle_key_modal(
        KeyCode::Enter,
        KeyModifiers::NONE,
        modal::ModalKind::PinnedBudgetExceeded,
    );

    assert!(!quit);
    assert!(model.modal.top().is_none());
    assert_eq!(model.chat.selected_message(), Some(0));
    assert!(model.pending_pinned_budget_exceeded.is_none());
}

#[test]
fn goal_view_action_menu_runtime_assignment_reassign_requires_confirmation() {
    let (mut model, mut daemon_rx) = make_model();
    model.focus = FocusArea::Chat;
    model
        .config
        .reduce(config::ConfigAction::ModelsFetched(vec![
            config::FetchedModel {
                id: "gpt-5.4".to_string(),
                name: Some("GPT-5.4".to_string()),
                context_window: Some(128_000),
                pricing: None,
                metadata: None,
            },
            config::FetchedModel {
                id: "gpt-5.4-mini".to_string(),
                name: Some("GPT-5.4 Mini".to_string()),
                context_window: Some(128_000),
                pricing: None,
                metadata: None,
            },
        ]));
    model
        .tasks
        .reduce(task::TaskAction::GoalRunDetailReceived(task::GoalRun {
            id: "goal-1".to_string(),
            title: "Goal One".to_string(),
            status: Some(task::GoalRunStatus::Running),
            goal: "Mission Control runtime edit".to_string(),
            thread_id: Some("thread-1".to_string()),
            root_thread_id: Some("thread-1".to_string()),
            active_thread_id: Some("thread-2".to_string()),
            runtime_assignment_list: vec![
                make_runtime_assignment(
                    amux_protocol::AGENT_ID_SWAROG,
                    "openai",
                    "gpt-5.4",
                    Some("medium"),
                ),
                make_runtime_assignment("reviewer", "openai", "gpt-5.4", Some("low")),
            ],
            launch_assignment_snapshot: vec![
                make_runtime_assignment(
                    amux_protocol::AGENT_ID_SWAROG,
                    "openai",
                    "gpt-5.4",
                    Some("medium"),
                ),
                make_runtime_assignment("reviewer", "openai", "gpt-5.4", Some("low")),
            ],
            current_step_owner_profile: Some(make_goal_owner_profile(
                "Swarog",
                "openai",
                "gpt-5.4",
                Some("medium"),
            )),
            ..Default::default()
        }));
    model.main_pane_view = MainPaneView::Task(SidebarItemTarget::GoalRun {
        goal_run_id: "goal-1".to_string(),
        step_id: None,
    });

    let handled = model.handle_key(KeyCode::Char('a'), KeyModifiers::NONE);
    assert!(!handled);
    assert_eq!(
        model.modal.top(),
        Some(modal::ModalKind::GoalStepActionPicker)
    );

    let model_edit_index = model
        .goal_action_picker_items()
        .iter()
        .position(|item| item.label() == "Edit Runtime Model")
        .expect("runtime model action should be available");
    if model_edit_index > 0 {
        model
            .modal
            .reduce(modal::ModalAction::Navigate(model_edit_index as i32));
    }

    let handled = model.handle_key_modal(
        KeyCode::Enter,
        KeyModifiers::NONE,
        modal::ModalKind::GoalStepActionPicker,
    );
    assert!(!handled);
    assert!(matches!(model.main_pane_view, MainPaneView::GoalComposer));
    assert_eq!(model.modal.top(), Some(modal::ModalKind::ModelPicker));
    let _ = collect_daemon_commands(&mut daemon_rx);

    navigate_model_picker_to(&mut model, "gpt-5.4-mini");

    let handled = model.handle_key_modal(
        KeyCode::Enter,
        KeyModifiers::NONE,
        modal::ModalKind::ModelPicker,
    );
    assert!(!handled);
    assert_eq!(
        model.modal.top(),
        Some(modal::ModalKind::GoalStepActionPicker)
    );

    let reassign_index = model
        .goal_action_picker_items()
        .iter()
        .position(|item| item.label() == "Reassign Active Step")
        .expect("reassign action should be available");
    if reassign_index > 0 {
        model
            .modal
            .reduce(modal::ModalAction::Navigate(reassign_index as i32));
    }

    let handled = model.handle_key_modal(
        KeyCode::Enter,
        KeyModifiers::NONE,
        modal::ModalKind::GoalStepActionPicker,
    );
    assert!(!handled);
    assert_eq!(model.modal.top(), Some(modal::ModalKind::ChatActionConfirm));
    assert_eq!(
        model
            .pending_chat_action_confirm
            .as_ref()
            .map(PendingConfirmAction::modal_body)
            .as_deref(),
        Some("Reassign the active step with the pending Mission Control roster change?")
    );

    let handled = model.handle_key_modal(
        KeyCode::Enter,
        KeyModifiers::NONE,
        modal::ModalKind::ChatActionConfirm,
    );
    assert!(!handled);
    assert!(
        daemon_rx.try_recv().is_err(),
        "runtime assignment edit should stay TUI-local without daemon command"
    );
    assert_eq!(
        model.goal_mission_control.pending_role_assignments.as_ref(),
        Some(&vec![
            make_runtime_assignment(
                amux_protocol::AGENT_ID_SWAROG,
                "openai",
                "gpt-5.4-mini",
                Some("medium"),
            ),
            make_runtime_assignment("reviewer", "openai", "gpt-5.4", Some("low")),
        ])
    );
    assert_eq!(
        model.goal_mission_control.pending_runtime_apply_modes,
        vec![
            Some(goal_mission_control::RuntimeAssignmentApplyMode::ReassignActiveStep),
            None,
        ]
    );
}

#[test]
fn goal_view_action_menu_runtime_assignment_cancel_clears_pending_confirmation_state() {
    let (mut model, mut daemon_rx_cmd) = make_model();
    model.connected = true;
    model.focus = FocusArea::Chat;
    model
        .config
        .reduce(config::ConfigAction::ModelsFetched(vec![
            config::FetchedModel {
                id: "gpt-5.4".to_string(),
                name: Some("GPT-5.4".to_string()),
                context_window: Some(128_000),
                pricing: None,
                metadata: None,
            },
            config::FetchedModel {
                id: "gpt-5.4-mini".to_string(),
                name: Some("GPT-5.4 Mini".to_string()),
                context_window: Some(128_000),
                pricing: None,
                metadata: None,
            },
        ]));
    model
        .tasks
        .reduce(task::TaskAction::GoalRunDetailReceived(task::GoalRun {
            id: "goal-1".to_string(),
            title: "Goal One".to_string(),
            status: Some(task::GoalRunStatus::Running),
            goal: "Mission Control runtime edit".to_string(),
            thread_id: Some("thread-1".to_string()),
            root_thread_id: Some("thread-1".to_string()),
            active_thread_id: Some("thread-2".to_string()),
            runtime_assignment_list: vec![
                make_runtime_assignment(
                    amux_protocol::AGENT_ID_SWAROG,
                    "openai",
                    "gpt-5.4",
                    Some("medium"),
                ),
                make_runtime_assignment("reviewer", "openai", "gpt-5.4", Some("low")),
            ],
            launch_assignment_snapshot: vec![
                make_runtime_assignment(
                    amux_protocol::AGENT_ID_SWAROG,
                    "openai",
                    "gpt-5.4",
                    Some("medium"),
                ),
                make_runtime_assignment("reviewer", "openai", "gpt-5.4", Some("low")),
            ],
            current_step_owner_profile: Some(make_goal_owner_profile(
                "Swarog",
                "openai",
                "gpt-5.4",
                Some("medium"),
            )),
            ..Default::default()
        }));
    model.main_pane_view = MainPaneView::Task(SidebarItemTarget::GoalRun {
        goal_run_id: "goal-1".to_string(),
        step_id: None,
    });

    let handled = model.handle_key(KeyCode::Char('a'), KeyModifiers::NONE);
    assert!(!handled);
    assert_eq!(
        model.modal.top(),
        Some(modal::ModalKind::GoalStepActionPicker)
    );

    let model_edit_index = model
        .goal_action_picker_items()
        .iter()
        .position(|item| item.label() == "Edit Runtime Model")
        .expect("runtime model action should be available");
    if model_edit_index > 0 {
        model
            .modal
            .reduce(modal::ModalAction::Navigate(model_edit_index as i32));
    }

    let handled = model.handle_key_modal(
        KeyCode::Enter,
        KeyModifiers::NONE,
        modal::ModalKind::GoalStepActionPicker,
    );
    assert!(!handled);
    assert_eq!(model.modal.top(), Some(modal::ModalKind::ModelPicker));
    let _ = collect_daemon_commands(&mut daemon_rx_cmd);

    navigate_model_picker_to(&mut model, "gpt-5.4-mini");

    let handled = model.handle_key_modal(
        KeyCode::Enter,
        KeyModifiers::NONE,
        modal::ModalKind::ModelPicker,
    );
    assert!(!handled);
    assert_eq!(
        model.modal.top(),
        Some(modal::ModalKind::GoalStepActionPicker)
    );

    let reassign_index = model
        .goal_action_picker_items()
        .iter()
        .position(|item| item.label() == "Reassign Active Step")
        .expect("reassign action should be available");
    if reassign_index > 0 {
        model
            .modal
            .reduce(modal::ModalAction::Navigate(reassign_index as i32));
    }

    let handled = model.handle_key_modal(
        KeyCode::Enter,
        KeyModifiers::NONE,
        modal::ModalKind::GoalStepActionPicker,
    );
    assert!(!handled);
    assert_eq!(model.modal.top(), Some(modal::ModalKind::ChatActionConfirm));

    let handled = model.handle_key_modal(
        KeyCode::Esc,
        KeyModifiers::NONE,
        modal::ModalKind::ChatActionConfirm,
    );
    assert!(!handled);
    assert!(model.goal_mission_control.pending_runtime_change.is_none());
    assert!(
        daemon_rx_cmd.try_recv().is_err(),
        "runtime assignment edit should stay TUI-local without daemon command"
    );

    model.main_pane_view = MainPaneView::Task(SidebarItemTarget::GoalRun {
        goal_run_id: "goal-1".to_string(),
        step_id: None,
    });
    model.focus = FocusArea::Chat;

    let labels: Vec<_> = model
        .goal_action_picker_items()
        .iter()
        .map(|item| item.label())
        .collect();
    assert!(labels.contains(&"Edit Runtime Model"), "{labels:?}");
    assert!(!labels.contains(&"Reassign Active Step"), "{labels:?}");
}
