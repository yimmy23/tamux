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
