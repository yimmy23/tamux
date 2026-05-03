#[test]
fn clicking_selected_message_copy_action_copies_that_message() {
    let mut model = build_model();
    model.show_sidebar_override = Some(false);
    model.focus = FocusArea::Chat;
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
            content: "first".to_string(),
            ..Default::default()
        },
    });
    model.chat.reduce(chat::ChatAction::AppendMessage {
        thread_id: "thread-1".to_string(),
        message: chat::AgentMessage {
            role: chat::MessageRole::Assistant,
            content: "second".to_string(),
            ..Default::default()
        },
    });
    model.chat.select_message(Some(1));

    let input_start_row = model.height.saturating_sub(model.input_height() + 1);
    let chat_area = Rect::new(0, 3, model.width, input_start_row.saturating_sub(3));
    let copy_pos = (chat_area.y..chat_area.y.saturating_add(chat_area.height))
        .find_map(|row| {
            (chat_area.x..chat_area.x.saturating_add(chat_area.width)).find_map(|column| {
                let pos = Position::new(column, row);
                if widgets::chat::hit_test(
                    chat_area,
                    &model.chat,
                    &model.theme,
                    model.tick_counter,
                    pos,
                ) == Some(chat::ChatHitTarget::CopyMessage(1))
                {
                    Some(pos)
                } else {
                    None
                }
            })
        })
        .expect("selected message should expose a clickable copy action");

    super::conversion::reset_last_copied_text();

    model.handle_mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: copy_pos.x,
        row: copy_pos.y,
        modifiers: KeyModifiers::NONE,
    });
    model.handle_mouse(MouseEvent {
        kind: MouseEventKind::Up(MouseButton::Left),
        column: copy_pos.x,
        row: copy_pos.y,
        modifiers: KeyModifiers::NONE,
    });

    assert_eq!(
        super::conversion::last_copied_text().as_deref(),
        Some("second")
    );
}

#[test]
fn pressing_enter_executes_selected_inline_message_action() {
    let mut model = build_model();
    model.show_sidebar_override = Some(false);
    model.focus = FocusArea::Chat;
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
            content: "second".to_string(),
            ..Default::default()
        },
    });
    model.chat.select_message(Some(0));
    model.chat.select_message_action(0);
    super::conversion::reset_last_copied_text();

    let handled = model.handle_key(KeyCode::Enter, KeyModifiers::NONE);

    assert!(!handled);
    assert_eq!(
        super::conversion::last_copied_text().as_deref(),
        Some("second")
    );
}

#[test]
fn operator_question_event_uses_keyboard_submission_from_inline_actions() {
    let (_daemon_tx, daemon_rx) = mpsc::channel();
    let (cmd_tx, mut cmd_rx) = unbounded_channel();
    let mut model = TuiModel::new(daemon_rx, cmd_tx);
    model.show_sidebar_override = Some(false);
    model.focus = FocusArea::Chat;
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
    assert_eq!(model.modal.top(), None);
    let thread = model.chat.active_thread().expect("thread should exist");
    let message = thread
        .messages
        .last()
        .expect("question message should exist");
    assert!(message.is_operator_question);
    assert_eq!(message.operator_question_id.as_deref(), Some("oq-1"));
    assert_eq!(message.actions[0].label, "A");
    assert_eq!(message.actions[1].label, "B");
    model.chat.select_message(Some(0));

    let handled = model.handle_key(KeyCode::Enter, KeyModifiers::NONE);

    assert!(!handled);
    let sent = cmd_rx
        .try_recv()
        .expect("pressing Enter should answer the operator question");
    match sent {
        DaemonCommand::AnswerOperatorQuestion {
            question_id,
            answer,
        } => {
            assert_eq!(question_id, "oq-1");
            assert_eq!(answer, "A");
        }
        other => panic!("unexpected command: {other:?}"),
    }
}

#[test]
fn click_without_drag_uses_press_location_for_message_selection() {
    let mut model = build_model();
    model.show_sidebar_override = Some(false);
    model.focus = FocusArea::Chat;
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
            content: "first".to_string(),
            ..Default::default()
        },
    });
    model.chat.reduce(chat::ChatAction::AppendMessage {
        thread_id: "thread-1".to_string(),
        message: chat::AgentMessage {
            role: chat::MessageRole::Assistant,
            content: "second".to_string(),
            ..Default::default()
        },
    });

    let input_start_row = model.height.saturating_sub(model.input_height() + 1);
    let chat_area = Rect::new(0, 3, model.width, input_start_row.saturating_sub(3));
    let message1_pos = (chat_area.y..chat_area.y.saturating_add(chat_area.height))
        .find_map(|row| {
            (chat_area.x..chat_area.x.saturating_add(chat_area.width)).find_map(|column| {
                let pos = Position::new(column, row);
                if widgets::chat::hit_test(
                    chat_area,
                    &model.chat,
                    &model.theme,
                    model.tick_counter,
                    pos,
                ) == Some(chat::ChatHitTarget::Message(1))
                {
                    Some(pos)
                } else {
                    None
                }
            })
        })
        .expect("second message should expose a clickable body position");
    let message0_pos = (chat_area.y..chat_area.y.saturating_add(chat_area.height))
        .find_map(|row| {
            (chat_area.x..chat_area.x.saturating_add(chat_area.width)).find_map(|column| {
                let pos = Position::new(column, row);
                if widgets::chat::hit_test(
                    chat_area,
                    &model.chat,
                    &model.theme,
                    model.tick_counter,
                    pos,
                ) == Some(chat::ChatHitTarget::Message(0))
                {
                    Some(pos)
                } else {
                    None
                }
            })
        })
        .expect("first message should expose a clickable body position");

    model.handle_mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: message1_pos.x,
        row: message1_pos.y,
        modifiers: KeyModifiers::NONE,
    });
    model.handle_mouse(MouseEvent {
        kind: MouseEventKind::Up(MouseButton::Left),
        column: message0_pos.x,
        row: message0_pos.y,
        modifiers: KeyModifiers::NONE,
    });

    assert_eq!(model.chat.selected_message(), Some(1));
}

#[test]
fn concierge_mouse_click_executes_visible_action() {
    let (_daemon_tx, daemon_rx) = mpsc::channel();
    let (cmd_tx, mut cmd_rx) = unbounded_channel();
    let mut model = TuiModel::new(daemon_rx, cmd_tx);
    model.connected = true;
    model.agent_config_loaded = true;
    model.focus = FocusArea::Chat;
    model.chat.reduce(chat::ChatAction::ThreadCreated {
        thread_id: "concierge".to_string(),
        title: "Concierge".to_string(),
    });
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("concierge".to_string()));
    model.chat.reduce(chat::ChatAction::AppendMessage {
        thread_id: "concierge".to_string(),
        message: chat::AgentMessage {
            role: chat::MessageRole::Assistant,
            content: "Welcome".to_string(),
            actions: vec![
                chat::MessageAction {
                    label: "One".to_string(),
                    action_type: "dismiss".to_string(),
                    thread_id: None,
                },
                chat::MessageAction {
                    label: "Two".to_string(),
                    action_type: "start_goal_run".to_string(),
                    thread_id: None,
                },
            ],
            is_concierge_welcome: true,
            ..Default::default()
        },
    });

    model.handle_mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: 8,
        row: 35,
        modifiers: KeyModifiers::NONE,
    });

    match cmd_rx.try_recv() {
        Ok(DaemonCommand::DismissConciergeWelcome) => {}
        other => panic!("expected dismiss command first, got {:?}", other),
    }
    match cmd_rx.try_recv() {
        Ok(DaemonCommand::RequestThread {
            thread_id,
            message_limit,
            message_offset,
        }) => {
            assert_eq!(thread_id, "concierge");
            assert_eq!(
                message_limit,
                Some(model.config.tui_chat_history_page_size as usize)
            );
            assert_eq!(message_offset, Some(0));
        }
        other => panic!("expected thread request command, got {:?}", other),
    }
    assert_eq!(model.focus, FocusArea::Input);
    assert_eq!(model.input.buffer(), "/new-goal ");
    assert!(model.chat.active_actions().is_empty());
}

#[test]
fn workspace_view_does_not_show_concierge_action_bar() {
    let mut model = build_model();
    model.connected = true;
    model.agent_config_loaded = true;
    model.main_pane_view = MainPaneView::Workspace;
    model.focus = FocusArea::Chat;
    model.chat.reduce(chat::ChatAction::ThreadCreated {
        thread_id: "concierge".to_string(),
        title: "Concierge".to_string(),
    });
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("concierge".to_string()));
    model.chat.reduce(chat::ChatAction::AppendMessage {
        thread_id: "concierge".to_string(),
        message: chat::AgentMessage {
            role: chat::MessageRole::Assistant,
            content: "Welcome".to_string(),
            actions: vec![chat::MessageAction {
                label: "Continue: research".to_string(),
                action_type: "dismiss".to_string(),
                thread_id: None,
            }],
            is_concierge_welcome: true,
            ..Default::default()
        },
    });

    assert!(!model.actions_bar_visible());
    assert_eq!(model.concierge_banner_height(), 0);
}

#[test]
fn workspace_enter_activates_new_task_not_concierge_action() {
    let (_daemon_tx, daemon_rx) = mpsc::channel();
    let (cmd_tx, mut cmd_rx) = unbounded_channel();
    let mut model = TuiModel::new(daemon_rx, cmd_tx);
    model.connected = true;
    model.agent_config_loaded = true;
    model.main_pane_view = MainPaneView::Workspace;
    model.focus = FocusArea::Chat;
    model.chat.reduce(chat::ChatAction::ThreadCreated {
        thread_id: "concierge".to_string(),
        title: "Concierge".to_string(),
    });
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("concierge".to_string()));
    model.chat.reduce(chat::ChatAction::AppendMessage {
        thread_id: "concierge".to_string(),
        message: chat::AgentMessage {
            role: chat::MessageRole::Assistant,
            content: "Welcome".to_string(),
            actions: vec![chat::MessageAction {
                label: "Dismiss".to_string(),
                action_type: "dismiss".to_string(),
                thread_id: None,
            }],
            is_concierge_welcome: true,
            ..Default::default()
        },
    });

    let handled = model.handle_key(KeyCode::Enter, KeyModifiers::NONE);

    assert!(!handled);
    assert_eq!(model.modal.top(), Some(modal::ModalKind::WorkspaceCreateTask));
    assert!(cmd_rx.try_recv().is_err());
}

#[test]
fn workspace_command_opens_workspace_picker_and_requests_list() {
    let (_daemon_tx, daemon_rx) = mpsc::channel();
    let (cmd_tx, mut cmd_rx) = unbounded_channel();
    let mut model = TuiModel::new(daemon_rx, cmd_tx);

    model.handle_workspace_command("");

    assert_eq!(model.modal.top(), Some(modal::ModalKind::WorkspacePicker));
    match cmd_rx.try_recv() {
        Ok(DaemonCommand::ListWorkspaceSettings) => {}
        other => panic!("expected workspace list request, got {other:?}"),
    }
}

#[test]
fn workspace_picker_enter_switches_workspace_and_loads_board() {
    let (_daemon_tx, daemon_rx) = mpsc::channel();
    let (cmd_tx, mut cmd_rx) = unbounded_channel();
    let mut model = TuiModel::new(daemon_rx, cmd_tx);
    model.open_workspace_picker();
    let _ = cmd_rx.try_recv();
    model.handle_client_event(ClientEvent::WorkspaceSettingsList(vec![
        zorai_protocol::WorkspaceSettings {
            workspace_id: "alpha".to_string(),
            workspace_root: None,
            operator: zorai_protocol::WorkspaceOperator::User,
            created_at: 1,
            updated_at: 1,
        },
        zorai_protocol::WorkspaceSettings {
            workspace_id: "beta".to_string(),
            workspace_root: None,
            operator: zorai_protocol::WorkspaceOperator::Svarog,
            created_at: 2,
            updated_at: 2,
        },
    ]));
    model.modal.reduce(modal::ModalAction::Navigate(2));

    model.submit_workspace_picker();

    assert_eq!(model.modal.top(), None);
    assert!(matches!(model.main_pane_view, MainPaneView::Workspace));
    assert_eq!(model.workspace.workspace_id(), "beta");
    loop {
        match cmd_rx.try_recv() {
            Ok(DaemonCommand::DismissConciergeWelcome) => continue,
            Ok(DaemonCommand::GetWorkspaceSettings { workspace_id }) => {
                assert_eq!(workspace_id, "beta");
                break;
            }
            other => panic!("expected settings request, got {other:?}"),
        }
    }
    loop {
        match cmd_rx.try_recv() {
            Ok(DaemonCommand::DismissConciergeWelcome) => continue,
            Ok(DaemonCommand::ListWorkspaceTasks {
                workspace_id,
                include_deleted,
            }) => {
                assert_eq!(workspace_id, "beta");
                assert!(!include_deleted);
                break;
            }
            other => panic!("expected task list request, got {other:?}"),
        }
    }
}

#[test]
fn workspace_picker_click_switches_workspace_and_loads_board() {
    let (_daemon_tx, daemon_rx) = mpsc::channel();
    let (cmd_tx, mut cmd_rx) = unbounded_channel();
    let mut model = TuiModel::new(daemon_rx, cmd_tx);
    model.open_workspace_picker();
    let _ = cmd_rx.try_recv();
    model.handle_client_event(ClientEvent::WorkspaceSettingsList(vec![
        zorai_protocol::WorkspaceSettings {
            workspace_id: "alpha".to_string(),
            workspace_root: None,
            operator: zorai_protocol::WorkspaceOperator::User,
            created_at: 1,
            updated_at: 1,
        },
        zorai_protocol::WorkspaceSettings {
            workspace_id: "beta".to_string(),
            workspace_root: None,
            operator: zorai_protocol::WorkspaceOperator::Svarog,
            created_at: 2,
            updated_at: 2,
        },
    ]));

    let (_, overlay_area) = model
        .current_modal_area()
        .expect("workspace picker should have an overlay area");
    let inner = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Double)
        .inner(overlay_area);
    let beta_row = inner.y.saturating_add(5);
    model.handle_mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: inner.x.saturating_add(2),
        row: beta_row,
        modifiers: KeyModifiers::NONE,
    });

    assert_eq!(model.modal.top(), None);
    assert!(matches!(model.main_pane_view, MainPaneView::Workspace));
    assert_eq!(model.workspace.workspace_id(), "beta");
    loop {
        match cmd_rx.try_recv() {
            Ok(DaemonCommand::DismissConciergeWelcome) => continue,
            Ok(DaemonCommand::GetWorkspaceSettings { workspace_id }) => {
                assert_eq!(workspace_id, "beta");
                break;
            }
            other => panic!("expected settings request, got {other:?}"),
        }
    }
    loop {
        match cmd_rx.try_recv() {
            Ok(DaemonCommand::DismissConciergeWelcome) => continue,
            Ok(DaemonCommand::ListWorkspaceTasks {
                workspace_id,
                include_deleted,
            }) => {
                assert_eq!(workspace_id, "beta");
                assert!(!include_deleted);
                break;
            }
            other => panic!("expected task list request, got {other:?}"),
        }
    }
}

#[test]
fn workspace_picker_selection_ignores_delayed_concierge_welcome() {
    let (_daemon_tx, daemon_rx) = mpsc::channel();
    let (cmd_tx, mut cmd_rx) = unbounded_channel();
    let mut model = TuiModel::new(daemon_rx, cmd_tx);
    model.connected = true;
    model.agent_config_loaded = true;
    model.chat.reduce(chat::ChatAction::ThreadCreated {
        thread_id: "concierge".to_string(),
        title: "Concierge".to_string(),
    });
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("concierge".to_string()));
    model.chat.reduce(chat::ChatAction::AppendMessage {
        thread_id: "concierge".to_string(),
        message: chat::AgentMessage {
            role: chat::MessageRole::Assistant,
            content: "Welcome".to_string(),
            actions: vec![chat::MessageAction {
                label: "Dismiss".to_string(),
                action_type: "dismiss".to_string(),
                thread_id: None,
            }],
            is_concierge_welcome: true,
            ..Default::default()
        },
    });
    model
        .concierge
        .reduce(crate::state::ConciergeAction::WelcomeReceived {
            content: "Welcome".to_string(),
            actions: vec![crate::state::ConciergeActionVm {
                label: "Dismiss".to_string(),
                action_type: "dismiss".to_string(),
                thread_id: None,
            }],
        });
    model.open_workspace_picker();
    let _ = cmd_rx.try_recv();
    model.handle_client_event(ClientEvent::WorkspaceSettingsList(vec![
        zorai_protocol::WorkspaceSettings {
            workspace_id: "alpha".to_string(),
            workspace_root: None,
            operator: zorai_protocol::WorkspaceOperator::User,
            created_at: 1,
            updated_at: 1,
        },
        zorai_protocol::WorkspaceSettings {
            workspace_id: "beta".to_string(),
            workspace_root: None,
            operator: zorai_protocol::WorkspaceOperator::Svarog,
            created_at: 2,
            updated_at: 2,
        },
    ]));
    model.modal.reduce(modal::ModalAction::Navigate(2));

    model.submit_workspace_picker();
    model.handle_concierge_welcome_event(
        "Late welcome".to_string(),
        vec![crate::state::ConciergeActionVm {
            label: "Start new session".to_string(),
            action_type: "start_new".to_string(),
            thread_id: None,
        }],
    );

    assert!(matches!(model.main_pane_view, MainPaneView::Workspace));
    assert_eq!(model.workspace.workspace_id(), "beta");
    assert!(!model.concierge.welcome_visible);
    assert!(model.chat.active_actions().is_empty());
    assert!(
        matches!(cmd_rx.try_recv(), Ok(DaemonCommand::DismissConciergeWelcome)),
        "workspace navigation should dismiss the active concierge welcome"
    );
}

#[test]
fn workspace_refresh_key_reloads_current_board() {
    let (_daemon_tx, daemon_rx) = mpsc::channel();
    let (cmd_tx, mut cmd_rx) = unbounded_channel();
    let mut model = TuiModel::new(daemon_rx, cmd_tx);
    model.main_pane_view = MainPaneView::Workspace;
    model.focus = FocusArea::Chat;

    let handled = model.handle_key(KeyCode::Char('r'), KeyModifiers::NONE);

    assert!(!handled);
    match cmd_rx.try_recv() {
        Ok(DaemonCommand::GetWorkspaceSettings { workspace_id }) => {
            assert_eq!(workspace_id, "main");
        }
        other => panic!("expected settings request, got {other:?}"),
    }
    match cmd_rx.try_recv() {
        Ok(DaemonCommand::ListWorkspaceTasks {
            workspace_id,
            include_deleted,
        }) => {
            assert_eq!(workspace_id, "main");
            assert!(!include_deleted);
        }
        other => panic!("expected task list request, got {other:?}"),
    }
}

#[test]
fn workspace_arrow_navigation_skips_removed_refresh_button() {
    let (_daemon_tx, daemon_rx) = mpsc::channel();
    let (cmd_tx, mut cmd_rx) = unbounded_channel();
    let mut model = TuiModel::new(daemon_rx, cmd_tx);
    model.main_pane_view = MainPaneView::Workspace;
    model.focus = FocusArea::Chat;

    model.handle_key(KeyCode::Right, KeyModifiers::NONE);
    model.handle_key(KeyCode::Enter, KeyModifiers::NONE);

    assert_eq!(
        model.workspace_board_selection,
        Some(widgets::workspace_board::WorkspaceBoardHitTarget::Toolbar(
            widgets::workspace_board::WorkspaceBoardToolbarAction::ToggleOperator
        ))
    );
    match cmd_rx.try_recv() {
        Ok(DaemonCommand::SetWorkspaceOperator { workspace_id, .. }) => {
            assert_eq!(workspace_id, "main");
        }
        other => panic!("expected operator update request, got {other:?}"),
    }
}

#[test]
fn in_review_run_action_opens_queued_review_task_thread() {
    let (_daemon_tx, daemon_rx) = mpsc::channel();
    let (cmd_tx, mut cmd_rx) = unbounded_channel();
    let mut model = TuiModel::new(daemon_rx, cmd_tx);
    model.main_pane_view = MainPaneView::Workspace;
    model.focus = FocusArea::Chat;
    model.workspace.set_tasks(
        "main".to_string(),
        vec![zorai_protocol::WorkspaceTask {
            id: "wtask-1".to_string(),
            workspace_id: "main".to_string(),
            title: "Review me".to_string(),
            task_type: zorai_protocol::WorkspaceTaskType::Thread,
            description: "Do it".to_string(),
            definition_of_done: None,
            priority: zorai_protocol::WorkspacePriority::Low,
            status: zorai_protocol::WorkspaceTaskStatus::InReview,
            sort_order: 1,
            reporter: zorai_protocol::WorkspaceActor::User,
            assignee: Some(zorai_protocol::WorkspaceActor::Agent("svarog".to_string())),
            reviewer: Some(zorai_protocol::WorkspaceActor::Subagent("qa".to_string())),
            thread_id: Some("workspace-thread".to_string()),
            goal_run_id: None,
            runtime_history: Vec::new(),
            created_at: 1,
            updated_at: 1,
            started_at: None,
            completed_at: None,
            deleted_at: None,
            last_notice_id: None,
        }],
    );
    model.workspace.set_notices(vec![zorai_protocol::WorkspaceNotice {
        id: "notice-1".to_string(),
        workspace_id: "main".to_string(),
        task_id: "wtask-1".to_string(),
        notice_type: "review_requested".to_string(),
        message: "Workspace task review requested from subagent:qa; queued review task review-task-1"
            .to_string(),
        actor: Some(zorai_protocol::WorkspaceActor::Subagent("qa".to_string())),
        created_at: 2,
    }]);
    model.tasks.reduce(task::TaskAction::TaskUpdate(task::AgentTask {
        id: "review-task-1".to_string(),
        title: "Review".to_string(),
        thread_id: Some("review-thread-1".to_string()),
        ..Default::default()
    }));

    model.activate_workspace_task_action(
        "wtask-1".to_string(),
        zorai_protocol::WorkspaceTaskStatus::InReview,
        widgets::workspace_board::WorkspaceBoardAction::Run,
    );

    loop {
        match cmd_rx.try_recv() {
            Ok(DaemonCommand::DismissConciergeWelcome) => continue,
            Ok(DaemonCommand::RequestThread { thread_id, .. }) => {
                assert_eq!(thread_id, "review-thread-1");
                break;
            }
            other => panic!("expected review thread request, got {other:?}"),
        }
    }
}

#[test]
fn in_review_open_action_opens_queued_review_task_thread() {
    let (_daemon_tx, daemon_rx) = mpsc::channel();
    let (cmd_tx, mut cmd_rx) = unbounded_channel();
    let mut model = TuiModel::new(daemon_rx, cmd_tx);
    model.main_pane_view = MainPaneView::Workspace;
    model.focus = FocusArea::Chat;
    model.workspace.set_tasks(
        "main".to_string(),
        vec![zorai_protocol::WorkspaceTask {
            id: "wtask-1".to_string(),
            workspace_id: "main".to_string(),
            title: "Review me".to_string(),
            task_type: zorai_protocol::WorkspaceTaskType::Thread,
            description: "Do it".to_string(),
            definition_of_done: None,
            priority: zorai_protocol::WorkspacePriority::Low,
            status: zorai_protocol::WorkspaceTaskStatus::InReview,
            sort_order: 1,
            reporter: zorai_protocol::WorkspaceActor::User,
            assignee: Some(zorai_protocol::WorkspaceActor::Agent("dola".to_string())),
            reviewer: Some(zorai_protocol::WorkspaceActor::Agent("swarog".to_string())),
            thread_id: Some("assignee-thread".to_string()),
            goal_run_id: None,
            runtime_history: Vec::new(),
            created_at: 1,
            updated_at: 1,
            started_at: None,
            completed_at: None,
            deleted_at: None,
            last_notice_id: None,
        }],
    );
    model.workspace.set_notices(vec![zorai_protocol::WorkspaceNotice {
        id: "notice-1".to_string(),
        workspace_id: "main".to_string(),
        task_id: "wtask-1".to_string(),
        notice_type: "review_requested".to_string(),
        message: "Workspace task review requested from agent:swarog; queued review task review-task-1"
            .to_string(),
        actor: Some(zorai_protocol::WorkspaceActor::Agent("swarog".to_string())),
        created_at: 2,
    }]);
    model.tasks.reduce(task::TaskAction::TaskUpdate(task::AgentTask {
        id: "review-task-1".to_string(),
        title: "Review".to_string(),
        thread_id: Some("review-thread-1".to_string()),
        ..Default::default()
    }));

    model.activate_workspace_task_action(
        "wtask-1".to_string(),
        zorai_protocol::WorkspaceTaskStatus::InReview,
        widgets::workspace_board::WorkspaceBoardAction::OpenRuntime,
    );

    loop {
        match cmd_rx.try_recv() {
            Ok(DaemonCommand::DismissConciergeWelcome) => continue,
            Ok(DaemonCommand::RequestThread { thread_id, .. }) => {
                assert_eq!(thread_id, "review-thread-1");
                break;
            }
            other => panic!("expected review thread request, got {other:?}"),
        }
    }
}

#[test]
fn in_review_open_action_does_not_open_stale_internal_dm_review_thread() {
    let (_daemon_tx, daemon_rx) = mpsc::channel();
    let (cmd_tx, mut cmd_rx) = unbounded_channel();
    let mut model = TuiModel::new(daemon_rx, cmd_tx);
    model.main_pane_view = MainPaneView::Workspace;
    model.focus = FocusArea::Chat;
    model.workspace.set_tasks(
        "main".to_string(),
        vec![zorai_protocol::WorkspaceTask {
            id: "wtask-1".to_string(),
            workspace_id: "main".to_string(),
            title: "Review me".to_string(),
            task_type: zorai_protocol::WorkspaceTaskType::Thread,
            description: "Do it".to_string(),
            definition_of_done: None,
            priority: zorai_protocol::WorkspacePriority::Low,
            status: zorai_protocol::WorkspaceTaskStatus::InReview,
            sort_order: 1,
            reporter: zorai_protocol::WorkspaceActor::User,
            assignee: Some(zorai_protocol::WorkspaceActor::Agent("dola".to_string())),
            reviewer: Some(zorai_protocol::WorkspaceActor::Agent("weles".to_string())),
            thread_id: Some("assignee-thread".to_string()),
            goal_run_id: None,
            runtime_history: Vec::new(),
            created_at: 1,
            updated_at: 1,
            started_at: None,
            completed_at: None,
            deleted_at: None,
            last_notice_id: None,
        }],
    );
    model.workspace.set_notices(vec![zorai_protocol::WorkspaceNotice {
        id: "notice-1".to_string(),
        workspace_id: "main".to_string(),
        task_id: "wtask-1".to_string(),
        notice_type: "review_requested".to_string(),
        message: "Workspace task review requested from agent:weles; queued review task review-task-1"
            .to_string(),
        actor: Some(zorai_protocol::WorkspaceActor::Agent("weles".to_string())),
        created_at: 2,
    }]);
    model.tasks.reduce(task::TaskAction::TaskUpdate(task::AgentTask {
        id: "review-task-1".to_string(),
        title: "Review".to_string(),
        thread_id: Some("dm:swarog:weles".to_string()),
        ..Default::default()
    }));

    model.activate_workspace_task_action(
        "wtask-1".to_string(),
        zorai_protocol::WorkspaceTaskStatus::InReview,
        widgets::workspace_board::WorkspaceBoardAction::OpenRuntime,
    );

    loop {
        match cmd_rx.try_recv() {
            Ok(DaemonCommand::DismissConciergeWelcome) => continue,
            Ok(DaemonCommand::ListTasks) => break,
            Ok(DaemonCommand::RequestThread { thread_id, .. }) => {
                panic!("must not open stale internal review thread {thread_id}")
            }
            other => panic!("expected task refresh, got {other:?}"),
        }
    }
}

#[test]
fn in_review_run_action_uses_runtime_history_reviewer_task_when_notice_is_missing() {
    let (_daemon_tx, daemon_rx) = mpsc::channel();
    let (cmd_tx, mut cmd_rx) = unbounded_channel();
    let mut model = TuiModel::new(daemon_rx, cmd_tx);
    model.main_pane_view = MainPaneView::Workspace;
    model.focus = FocusArea::Chat;
    let mut task = workspace_task_for_board(
        "wtask-1",
        zorai_protocol::WorkspaceTaskStatus::InReview,
        Some(zorai_protocol::WorkspaceActor::Agent("svarog".to_string())),
    );
    task.reviewer = Some(zorai_protocol::WorkspaceActor::Subagent("qa".to_string()));
    task.runtime_history = vec![zorai_protocol::WorkspaceTaskRuntimeHistoryEntry {
        task_type: zorai_protocol::WorkspaceTaskType::Thread,
        thread_id: None,
        goal_run_id: None,
        agent_task_id: Some("review-task-runtime".to_string()),
        source: Some("workspace_review".to_string()),
        title: Some("Review workspace task".to_string()),
        review_path: None,
        review_feedback: None,
        archived_at: 9,
    }];
    model.workspace.set_tasks("main".to_string(), vec![task]);
    model.tasks.reduce(task::TaskAction::TaskUpdate(task::AgentTask {
        id: "review-task-runtime".to_string(),
        title: "Review".to_string(),
        thread_id: Some("review-thread-runtime".to_string()),
        ..Default::default()
    }));

    model.activate_workspace_task_action(
        "wtask-1".to_string(),
        zorai_protocol::WorkspaceTaskStatus::InReview,
        widgets::workspace_board::WorkspaceBoardAction::Run,
    );

    loop {
        match cmd_rx.try_recv() {
            Ok(DaemonCommand::DismissConciergeWelcome) => continue,
            Ok(DaemonCommand::RequestThread { thread_id, .. }) => {
                assert_eq!(thread_id, "review-thread-runtime");
                break;
            }
            other => panic!("expected review thread request, got {other:?}"),
        }
    }
}

#[test]
fn todo_run_action_moves_task_to_in_progress_before_daemon_echo() {
    let (_daemon_tx, daemon_rx) = mpsc::channel();
    let (cmd_tx, mut cmd_rx) = unbounded_channel();
    let mut model = TuiModel::new(daemon_rx, cmd_tx);
    model.main_pane_view = MainPaneView::Workspace;
    model.focus = FocusArea::Chat;
    model.workspace.set_tasks(
        "main".to_string(),
        vec![workspace_task_for_board(
            "wtask-run",
            zorai_protocol::WorkspaceTaskStatus::Todo,
            Some(zorai_protocol::WorkspaceActor::Agent("svarog".to_string())),
        )],
    );

    model.activate_workspace_task_action(
        "wtask-run".to_string(),
        zorai_protocol::WorkspaceTaskStatus::Todo,
        widgets::workspace_board::WorkspaceBoardAction::Run,
    );

    match cmd_rx.try_recv() {
        Ok(DaemonCommand::RunWorkspaceTask(task_id)) => {
            assert_eq!(task_id, "wtask-run");
        }
        other => panic!("expected run command, got {other:?}"),
    }
    assert_eq!(
        model.workspace.task_by_id("wtask-run").map(|task| &task.status),
        Some(&zorai_protocol::WorkspaceTaskStatus::InProgress)
    );
    let in_progress_column = model
        .workspace
        .projection()
        .columns
        .iter()
        .find(|column| column.status == zorai_protocol::WorkspaceTaskStatus::InProgress)
        .expect("in-progress column");
    assert!(in_progress_column
        .tasks
        .iter()
        .any(|task| task.id == "wtask-run"));
}

#[test]
fn workspace_history_action_opens_previous_runtime_newest_first() {
    let (_daemon_tx, daemon_rx) = mpsc::channel();
    let (cmd_tx, mut cmd_rx) = unbounded_channel();
    let mut model = TuiModel::new(daemon_rx, cmd_tx);
    model.main_pane_view = MainPaneView::Workspace;
    let mut task = workspace_task_for_board(
        "wtask-1",
        zorai_protocol::WorkspaceTaskStatus::InProgress,
        Some(zorai_protocol::WorkspaceActor::Agent("svarog".to_string())),
    );
    task.thread_id = Some("workspace-thread:active".to_string());
    task.runtime_history = vec![
        zorai_protocol::WorkspaceTaskRuntimeHistoryEntry {
            task_type: zorai_protocol::WorkspaceTaskType::Thread,
            thread_id: Some("workspace-thread:old-2".to_string()),
            goal_run_id: None,
            agent_task_id: None,
            source: Some("workspace_runtime".to_string()),
            title: Some("Older run".to_string()),
            review_path: Some("task-wtask-1/failed-review.md".to_string()),
            review_feedback: Some("Second review failed".to_string()),
            archived_at: 20,
        },
        zorai_protocol::WorkspaceTaskRuntimeHistoryEntry {
            task_type: zorai_protocol::WorkspaceTaskType::Thread,
            thread_id: Some("workspace-thread:old-1".to_string()),
            goal_run_id: None,
            agent_task_id: None,
            source: Some("workspace_runtime".to_string()),
            title: Some("Oldest run".to_string()),
            review_path: Some("task-wtask-1/failed-review.md".to_string()),
            review_feedback: Some("First review failed".to_string()),
            archived_at: 10,
        },
    ];
    model.workspace.set_tasks("main".to_string(), vec![task]);

    model.activate_workspace_task_action(
        "wtask-1".to_string(),
        zorai_protocol::WorkspaceTaskStatus::InProgress,
        widgets::workspace_board::WorkspaceBoardAction::History,
    );

    assert_eq!(
        model.modal.top(),
        Some(crate::state::modal::ModalKind::WorkspaceTaskHistory)
    );
    let body = model.workspace_history_modal_body();
    assert!(body.find("active").unwrap() < body.find("old-2").unwrap());
    assert!(body.find("old-2").unwrap() < body.find("old-1").unwrap());

    model.modal.reduce(modal::ModalAction::Navigate(1));
    model.submit_workspace_history_modal();
    loop {
        match cmd_rx.try_recv() {
            Ok(DaemonCommand::DismissConciergeWelcome) => continue,
            Ok(DaemonCommand::RequestThread { thread_id, .. }) => {
                assert_eq!(thread_id, "workspace-thread:old-2");
                break;
            }
            other => panic!("expected historical thread request, got {other:?}"),
        }
    }
}

#[test]
fn workspace_history_button_click_opens_modal() {
    let (_daemon_tx, daemon_rx) = mpsc::channel();
    let (cmd_tx, _cmd_rx) = unbounded_channel();
    let mut model = TuiModel::new(daemon_rx, cmd_tx);
    model.width = 140;
    model.height = 40;
    model.show_sidebar_override = Some(false);
    model.main_pane_view = MainPaneView::Workspace;
    model.focus = FocusArea::Chat;
    let mut task = workspace_task_for_board(
        "wtask-1",
        zorai_protocol::WorkspaceTaskStatus::InProgress,
        Some(zorai_protocol::WorkspaceActor::Agent("svarog".to_string())),
    );
    task.runtime_history = vec![zorai_protocol::WorkspaceTaskRuntimeHistoryEntry {
        task_type: zorai_protocol::WorkspaceTaskType::Thread,
        thread_id: Some("workspace-thread:old".to_string()),
        goal_run_id: None,
        agent_task_id: None,
        source: Some("workspace_runtime".to_string()),
        title: Some("Old run".to_string()),
        review_path: Some("task-wtask-1/failed-review.md".to_string()),
        review_feedback: Some("Needs one more check".to_string()),
        archived_at: 10,
    }];
    model.workspace.set_tasks("main".to_string(), vec![task]);
    model
        .workspace_expanded_task_ids
        .insert("wtask-1".to_string());

    let click = workspace_hit_position(&model, |target| {
        matches!(
            target,
            widgets::workspace_board::WorkspaceBoardHitTarget::Action {
                task_id,
                action: widgets::workspace_board::WorkspaceBoardAction::History,
                ..
            } if task_id == "wtask-1"
        )
    });

    model.handle_mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: click.x,
        row: click.y,
        modifiers: KeyModifiers::NONE,
    });

    assert_eq!(
        model.modal.top(),
        Some(crate::state::modal::ModalKind::WorkspaceTaskHistory)
    );
    assert!(model.workspace_history_modal_body().contains("workspace-thread:old"));
}

#[test]
fn workspace_history_button_opens_empty_state_for_legacy_tasks() {
    let (_daemon_tx, daemon_rx) = mpsc::channel();
    let (cmd_tx, _cmd_rx) = unbounded_channel();
    let mut model = TuiModel::new(daemon_rx, cmd_tx);
    let mut task = workspace_task_for_board(
        "wtask-legacy",
        zorai_protocol::WorkspaceTaskStatus::InProgress,
        Some(zorai_protocol::WorkspaceActor::Agent("svarog".to_string())),
    );
    task.thread_id = None;
    task.goal_run_id = None;
    model.workspace.set_tasks("main".to_string(), vec![task]);

    model.activate_workspace_task_action(
        "wtask-legacy".to_string(),
        zorai_protocol::WorkspaceTaskStatus::InProgress,
        widgets::workspace_board::WorkspaceBoardAction::History,
    );

    assert_eq!(
        model.modal.top(),
        Some(crate::state::modal::ModalKind::WorkspaceTaskHistory)
    );
    assert!(model
        .workspace_history_modal_body()
        .contains("No previous thread or goal runs"));
}

#[test]
fn workspace_history_modal_uses_active_runtime_when_history_missing() {
    let (_daemon_tx, daemon_rx) = mpsc::channel();
    let (cmd_tx, mut cmd_rx) = unbounded_channel();
    let mut model = TuiModel::new(daemon_rx, cmd_tx);
    let mut task = workspace_task_for_board(
        "wtask-legacy",
        zorai_protocol::WorkspaceTaskStatus::InReview,
        Some(zorai_protocol::WorkspaceActor::Agent("svarog".to_string())),
    );
    task.thread_id = Some("workspace-thread:active-legacy".to_string());
    task.runtime_history.clear();
    model.workspace.set_tasks("main".to_string(), vec![task]);

    model.open_workspace_history_modal("wtask-legacy".to_string());

    let body = model.workspace_history_modal_body();
    assert!(body.contains("workspace-thread:active-legacy"), "{body}");
    model.submit_workspace_history_modal();
    loop {
        match cmd_rx.try_recv() {
            Ok(DaemonCommand::DismissConciergeWelcome) => continue,
            Ok(DaemonCommand::RequestThread { thread_id, .. }) => {
                assert_eq!(thread_id, "workspace-thread:active-legacy");
                break;
            }
            other => panic!("expected active runtime thread request, got {other:?}"),
        }
    }
}

fn workspace_settings_for_operator(
    operator: zorai_protocol::WorkspaceOperator,
) -> zorai_protocol::WorkspaceSettings {
    zorai_protocol::WorkspaceSettings {
        workspace_id: "main".to_string(),
        workspace_root: None,
        operator,
        created_at: 1,
        updated_at: 1,
    }
}

fn workspace_task_for_board(
    id: &str,
    status: zorai_protocol::WorkspaceTaskStatus,
    assignee: Option<zorai_protocol::WorkspaceActor>,
) -> zorai_protocol::WorkspaceTask {
    zorai_protocol::WorkspaceTask {
        id: id.to_string(),
        workspace_id: "main".to_string(),
        title: id.to_string(),
        task_type: zorai_protocol::WorkspaceTaskType::Thread,
        description: "Description".to_string(),
        definition_of_done: None,
        priority: zorai_protocol::WorkspacePriority::Low,
        status,
        sort_order: 1,
        reporter: zorai_protocol::WorkspaceActor::User,
        assignee,
        reviewer: Some(zorai_protocol::WorkspaceActor::User),
        thread_id: Some(format!("workspace-thread:{id}")),
        goal_run_id: None,
        runtime_history: Vec::new(),
        created_at: 1,
        updated_at: 1,
        started_at: None,
        completed_at: None,
        deleted_at: None,
        last_notice_id: None,
    }
}

fn workspace_hit_position(
    model: &TuiModel,
    matches_target: impl Fn(widgets::workspace_board::WorkspaceBoardHitTarget) -> bool,
) -> Position {
    let chat_area = model.pane_layout().chat;
    (chat_area.y..chat_area.y.saturating_add(chat_area.height))
        .find_map(|row| {
            (chat_area.x..chat_area.x.saturating_add(chat_area.width)).find_map(|column| {
                let position = Position::new(column, row);
                widgets::workspace_board::hit_test_with_scroll(
                    chat_area,
                    &model.workspace,
                    &model.workspace_expanded_task_ids,
                    &model.workspace_board_scroll,
                    position,
                )
                    .filter(|target| matches_target(target.clone()))
                    .map(|_| position)
            })
        })
        .expect("workspace board target should be visible")
}

#[test]
fn workspace_operator_switch_updates_projection_before_daemon_echo() {
    let (_daemon_tx, daemon_rx) = mpsc::channel();
    let (cmd_tx, _cmd_rx) = unbounded_channel();
    let mut model = TuiModel::new(daemon_rx, cmd_tx);
    model
        .workspace
        .set_settings(workspace_settings_for_operator(zorai_protocol::WorkspaceOperator::Svarog));

    model.switch_workspace_operator_from_ui(zorai_protocol::WorkspaceOperator::User);

    assert_eq!(model.workspace.operator(), zorai_protocol::WorkspaceOperator::User);
    assert_eq!(
        model.workspace.projection().operator,
        zorai_protocol::WorkspaceOperator::User
    );
}

#[test]
fn workspace_drag_todo_to_in_progress_moves_unassigned_task_without_running() {
    let (_daemon_tx, daemon_rx) = mpsc::channel();
    let (cmd_tx, mut cmd_rx) = unbounded_channel();
    let mut model = TuiModel::new(daemon_rx, cmd_tx);
    model.width = 120;
    model.height = 40;
    model.show_sidebar_override = Some(false);
    model.main_pane_view = MainPaneView::Workspace;
    model.focus = FocusArea::Chat;
    model
        .workspace
        .set_settings(workspace_settings_for_operator(zorai_protocol::WorkspaceOperator::User));
    model.workspace.set_tasks(
        "main".to_string(),
        vec![workspace_task_for_board(
            "todo-1",
            zorai_protocol::WorkspaceTaskStatus::Todo,
            None,
        )],
    );
    let start = workspace_hit_position(&model, |target| {
        matches!(
            target,
            widgets::workspace_board::WorkspaceBoardHitTarget::Task { task_id, status }
                if task_id == "todo-1" && status == zorai_protocol::WorkspaceTaskStatus::Todo
        )
    });
    let drop = workspace_hit_position(&model, |target| {
        matches!(
            target,
            widgets::workspace_board::WorkspaceBoardHitTarget::Column { status }
                if status == zorai_protocol::WorkspaceTaskStatus::InProgress
        )
    });

    model.handle_mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: start.x,
        row: start.y,
        modifiers: KeyModifiers::NONE,
    });
    model.handle_mouse(MouseEvent {
        kind: MouseEventKind::Up(MouseButton::Left),
        column: drop.x,
        row: drop.y,
        modifiers: KeyModifiers::NONE,
    });

    match cmd_rx.try_recv() {
        Ok(DaemonCommand::MoveWorkspaceTask(request)) => {
            assert_eq!(request.task_id, "todo-1");
            assert_eq!(request.status, zorai_protocol::WorkspaceTaskStatus::InProgress);
        }
        other => panic!("expected move command, got {other:?}"),
    }
    assert!(cmd_rx.try_recv().is_err(), "drag should not auto-run unassigned tasks");
}

#[test]
fn workspace_drag_from_collapsed_action_row_moves_task() {
    let (_daemon_tx, daemon_rx) = mpsc::channel();
    let (cmd_tx, mut cmd_rx) = unbounded_channel();
    let mut model = TuiModel::new(daemon_rx, cmd_tx);
    model.width = 120;
    model.height = 40;
    model.show_sidebar_override = Some(false);
    model.main_pane_view = MainPaneView::Workspace;
    model.focus = FocusArea::Chat;
    model
        .workspace
        .set_settings(workspace_settings_for_operator(zorai_protocol::WorkspaceOperator::User));
    model.workspace.set_tasks(
        "main".to_string(),
        vec![workspace_task_for_board(
            "todo-1",
            zorai_protocol::WorkspaceTaskStatus::Todo,
            None,
        )],
    );
    let chat_area = model.pane_layout().chat;
    let board_inner = Rect::new(
        chat_area.x.saturating_add(1),
        chat_area.y.saturating_add(1),
        chat_area.width.saturating_sub(2),
        chat_area.height.saturating_sub(2),
    );
    let board_area = Rect::new(
        board_inner.x,
        board_inner.y.saturating_add(1),
        board_inner.width,
        board_inner.height.saturating_sub(1),
    );
    let column_width = board_area.width / 4;
    let todo_column = Rect::new(board_area.x, board_area.y, column_width, board_area.height);
    let todo_body = Rect::new(
        todo_column.x.saturating_add(1),
        todo_column.y.saturating_add(1),
        todo_column.width.saturating_sub(2),
        todo_column.height.saturating_sub(2),
    );
    let task_body = Rect::new(
        todo_body.x.saturating_add(1),
        todo_body.y.saturating_add(1),
        todo_body.width.saturating_sub(2),
        8,
    );
    let start = Position::new(task_body.x + 8, task_body.y + 4);
    let drop = workspace_hit_position(&model, |target| {
        matches!(
            target,
            widgets::workspace_board::WorkspaceBoardHitTarget::Column { status }
                if status == zorai_protocol::WorkspaceTaskStatus::InProgress
        )
    });

    model.handle_mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: start.x,
        row: start.y,
        modifiers: KeyModifiers::NONE,
    });
    model.handle_mouse(MouseEvent {
        kind: MouseEventKind::Up(MouseButton::Left),
        column: drop.x,
        row: drop.y,
        modifiers: KeyModifiers::NONE,
    });

    match cmd_rx.try_recv() {
        Ok(DaemonCommand::MoveWorkspaceTask(request)) => {
            assert_eq!(request.task_id, "todo-1");
            assert_eq!(request.status, zorai_protocol::WorkspaceTaskStatus::InProgress);
        }
        other => panic!("expected move command, got {other:?}"),
    }
    assert!(cmd_rx.try_recv().is_err(), "drag should not auto-run unassigned tasks");
}

#[test]
fn workspace_task_open_thread_renders_return_to_workspace_and_b_restores_board() {
    let mut model = build_model();
    model.width = 120;
    model.height = 40;
    model.connected = true;
    model.agent_config_loaded = true;
    model.show_sidebar_override = Some(false);
    model.main_pane_view = MainPaneView::Workspace;
    model.focus = FocusArea::Chat;
    model
        .workspace
        .set_settings(workspace_settings_for_operator(zorai_protocol::WorkspaceOperator::User));
    model.chat.reduce(chat::ChatAction::ThreadCreated {
        thread_id: "workspace-thread:thread-task".to_string(),
        title: "Workspace task thread".to_string(),
    });
    model.workspace.set_tasks(
        "main".to_string(),
        vec![workspace_task_for_board(
            "thread-task",
            zorai_protocol::WorkspaceTaskStatus::InProgress,
            Some(zorai_protocol::WorkspaceActor::Agent("svarog".to_string())),
        )],
    );

    model.open_workspace_task_runtime("thread-task".to_string());

    assert!(matches!(model.main_pane_view, MainPaneView::Conversation));
    let plain = render_chat_plain(&mut model);
    assert!(plain.contains("Return to workspace"), "{plain}");

    let handled = model.handle_key(KeyCode::Char('b'), KeyModifiers::NONE);

    assert!(!handled);
    assert!(matches!(model.main_pane_view, MainPaneView::Workspace));
}

#[test]
fn workspace_task_open_thread_uses_subagent_assignee_as_responder_hint() {
    let mut model = build_model();
    model.width = 120;
    model.height = 40;
    model.connected = true;
    model.agent_config_loaded = true;
    model.show_sidebar_override = Some(false);
    model.main_pane_view = MainPaneView::Workspace;
    model.focus = FocusArea::Chat;
    model.subagents.entries.push(crate::state::SubAgentEntry {
        id: "dola".to_string(),
        name: "Dola".to_string(),
        provider: "openai".to_string(),
        model: "gpt-5.4-mini".to_string(),
        role: Some("implementation".to_string()),
        enabled: true,
        builtin: false,
        immutable_identity: false,
        disable_allowed: true,
        delete_allowed: true,
        protected_reason: None,
        reasoning_effort: Some("medium".to_string()),
        openrouter_provider_order: String::new(),
        openrouter_provider_ignore: String::new(),
        openrouter_allow_fallbacks: true,
        raw_json: None,
    });
    model
        .workspace
        .set_settings(workspace_settings_for_operator(zorai_protocol::WorkspaceOperator::User));
    model.chat.reduce(chat::ChatAction::ThreadCreated {
        thread_id: "workspace-thread:dola-task".to_string(),
        title: "Workspace task thread".to_string(),
    });
    model.chat.reduce(chat::ChatAction::AppendMessage {
        thread_id: "workspace-thread:dola-task".to_string(),
        message: chat::AgentMessage {
            role: chat::MessageRole::Assistant,
            content: "Dola is handling the task".to_string(),
            ..Default::default()
        },
    });
    model.workspace.set_tasks(
        "main".to_string(),
        vec![workspace_task_for_board(
            "dola-task",
            zorai_protocol::WorkspaceTaskStatus::InProgress,
            Some(zorai_protocol::WorkspaceActor::Subagent("dola".to_string())),
        )],
    );

    model.open_workspace_task_runtime("dola-task".to_string());

    let plain = render_chat_plain(&mut model);
    assert!(plain.contains("Responder: Dola"), "{plain}");
    assert!(!plain.contains("Responder: Svarog"), "{plain}");
}

#[test]
fn workspace_task_open_goal_renders_return_to_workspace_and_b_restores_board() {
    let mut model = goal_sidebar_model();
    model.connected = true;
    model.agent_config_loaded = true;
    model.show_sidebar_override = Some(false);
    model.main_pane_view = MainPaneView::Workspace;
    model.focus = FocusArea::Chat;
    model
        .workspace
        .set_settings(workspace_settings_for_operator(zorai_protocol::WorkspaceOperator::User));
    let mut workspace_task = workspace_task_for_board(
        "goal-task",
        zorai_protocol::WorkspaceTaskStatus::InProgress,
        Some(zorai_protocol::WorkspaceActor::Agent("svarog".to_string())),
    );
    workspace_task.goal_run_id = Some("goal-1".to_string());
    model
        .workspace
        .set_tasks("main".to_string(), vec![workspace_task]);

    model.open_workspace_task_runtime("goal-task".to_string());

    assert!(matches!(
        model.main_pane_view,
        MainPaneView::Task(SidebarItemTarget::GoalRun { ref goal_run_id, .. })
            if goal_run_id == "goal-1"
    ));
    let plain = render_chat_plain(&mut model);
    assert!(plain.contains("Return to workspace"), "{plain}");

    let button = model
        .task_return_to_workspace_button_area()
        .expect("workspace return button should be rendered");
    let click_column = button.x.saturating_add(1);
    let click_row = button.y;

    model.handle_mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: click_column,
        row: click_row,
        modifiers: KeyModifiers::NONE,
    });
    model.handle_mouse(MouseEvent {
        kind: MouseEventKind::Up(MouseButton::Left),
        column: click_column,
        row: click_row,
        modifiers: KeyModifiers::NONE,
    });

    assert!(matches!(model.main_pane_view, MainPaneView::Workspace));
}

#[test]
fn dismissing_concierge_welcome_returns_to_local_landing() {
    let (_daemon_tx, daemon_rx) = mpsc::channel();
    let (cmd_tx, mut cmd_rx) = unbounded_channel();
    let mut model = TuiModel::new(daemon_rx, cmd_tx);
    model.connected = true;
    model.agent_config_loaded = true;
    model.focus = FocusArea::Chat;
    model.chat.reduce(chat::ChatAction::ThreadCreated {
        thread_id: "concierge".to_string(),
        title: "Concierge".to_string(),
    });
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("concierge".to_string()));
    model.chat.reduce(chat::ChatAction::AppendMessage {
        thread_id: "concierge".to_string(),
        message: chat::AgentMessage {
            role: chat::MessageRole::Assistant,
            content: "Welcome".to_string(),
            actions: vec![chat::MessageAction {
                label: "Dismiss".to_string(),
                action_type: "dismiss".to_string(),
                thread_id: None,
            }],
            is_concierge_welcome: true,
            ..Default::default()
        },
    });
    model
        .concierge
        .reduce(crate::state::ConciergeAction::WelcomeReceived {
            content: "Welcome".to_string(),
            actions: vec![crate::state::ConciergeActionVm {
                label: "Dismiss".to_string(),
                action_type: "dismiss".to_string(),
                thread_id: None,
            }],
        });

    model.run_concierge_action(crate::state::ConciergeActionVm {
        label: "Dismiss".to_string(),
        action_type: "dismiss".to_string(),
        thread_id: None,
    });

    assert_eq!(model.chat.active_thread_id(), None);
    assert!(model.should_show_local_landing());
    assert_eq!(model.focus, FocusArea::Input);
    match cmd_rx.try_recv() {
        Ok(DaemonCommand::DismissConciergeWelcome) => {}
        other => panic!("expected dismiss command, got {:?}", other),
    }
    assert!(cmd_rx.try_recv().is_err(), "unexpected follow-up command");
}

#[test]
fn drag_selection_keeps_original_anchor_point_when_chat_scrolls() {
    let mut model = build_model();
    model.show_sidebar_override = Some(false);
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
            content: (1..=80)
                .map(|idx| format!("line {idx}"))
                .collect::<Vec<_>>()
                .join("\n"),
            ..Default::default()
        },
    });

    let input_start_row = model.height.saturating_sub(model.input_height() + 1);
    let chat_area = Rect::new(0, 3, model.width, input_start_row.saturating_sub(3));
    let preferred_row = chat_area.y.saturating_add(chat_area.height / 2);
    let start_row = (preferred_row..chat_area.y.saturating_add(chat_area.height))
        .chain(chat_area.y..preferred_row)
        .find(|row| {
            widgets::chat::selection_point_from_mouse(
                chat_area,
                &model.chat,
                &model.theme,
                model.tick_counter,
                Position::new(3, *row),
            )
            .is_some()
        })
        .expect("chat transcript should expose at least one selectable row");

    model.handle_mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: 3,
        row: start_row,
        modifiers: KeyModifiers::NONE,
    });
    let anchor_point = model
        .chat_drag_anchor_point
        .expect("mouse down should capture a document anchor point");

    for _ in 0..4 {
        model.handle_mouse(MouseEvent {
            kind: MouseEventKind::ScrollUp,
            column: 3,
            row: start_row,
            modifiers: KeyModifiers::NONE,
        });
    }

    let current_point = model
        .chat_drag_current_point
        .expect("dragging should keep updating the current document point");
    assert_eq!(
        model.chat_drag_anchor_point,
        Some(anchor_point),
        "autoscroll should not rewrite the original selection anchor"
    );
    assert!(
            current_point.row < anchor_point.row,
            "dragging upward with autoscroll should extend the selection into older transcript rows: anchor={anchor_point:?} current={current_point:?}"
        );
}
