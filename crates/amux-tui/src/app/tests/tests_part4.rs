#[test]
fn drag_selection_does_not_rebuild_full_transcript_for_every_mouse_event() {
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
            content: "alpha beta gamma".to_string(),
            ..Default::default()
        },
    });

    let input_start_row = model.height.saturating_sub(model.input_height() + 1);
    let chat_area = Rect::new(0, 3, model.width, input_start_row.saturating_sub(3));
    let row = (chat_area.y..chat_area.y.saturating_add(chat_area.height))
        .find(|candidate| {
            widgets::chat::selection_point_from_mouse(
                chat_area,
                &model.chat,
                &model.theme,
                model.tick_counter,
                Position::new(3, *candidate),
            )
            .is_some()
        })
        .expect("chat transcript should expose a selectable row");

    widgets::chat::reset_build_rendered_lines_call_count();

    model.handle_mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: 3,
        row,
        modifiers: KeyModifiers::NONE,
    });
    model.handle_mouse(MouseEvent {
        kind: MouseEventKind::Drag(MouseButton::Left),
        column: 12,
        row,
        modifiers: KeyModifiers::NONE,
    });
    model.handle_mouse(MouseEvent {
        kind: MouseEventKind::Up(MouseButton::Left),
        column: 12,
        row,
        modifiers: KeyModifiers::NONE,
    });

    assert_eq!(
        widgets::chat::build_rendered_lines_call_count(),
        1,
        "dragging a static selection should reuse one transcript snapshot"
    );
}

#[test]
fn render_during_active_drag_reuses_cached_snapshot_and_shows_highlight() {
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
            content: (1..=80)
                .map(|idx| format!("line {idx}"))
                .collect::<Vec<_>>()
                .join("\n"),
            ..Default::default()
        },
    });
    model.chat.reduce(chat::ChatAction::ScrollChat(8));

    let input_start_row = model.height.saturating_sub(model.input_height() + 1);
    let chat_area = Rect::new(0, 3, model.width, input_start_row.saturating_sub(3));
    let row = (chat_area.y..chat_area.y.saturating_add(chat_area.height))
        .find(|candidate| {
            widgets::chat::selection_point_from_mouse(
                chat_area,
                &model.chat,
                &model.theme,
                model.tick_counter,
                Position::new(3, *candidate),
            )
            .is_some()
        })
        .expect("chat transcript should expose at least one selectable row");

    widgets::chat::reset_build_rendered_lines_call_count();
    model.handle_mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: 3,
        row,
        modifiers: KeyModifiers::NONE,
    });
    model.handle_mouse(MouseEvent {
        kind: MouseEventKind::Drag(MouseButton::Left),
        column: 12,
        row,
        modifiers: KeyModifiers::NONE,
    });

    let backend = TestBackend::new(model.width, model.height);
    let mut terminal = Terminal::new(backend).expect("test terminal should initialize");
    terminal
        .draw(|frame| model.render(frame))
        .expect("model render should succeed");

    assert_eq!(
        widgets::chat::build_rendered_lines_call_count(),
        1,
        "active drag rendering should reuse the cached transcript snapshot"
    );

    let buffer = terminal.backend().buffer();
    let highlighted = (0..model.height)
        .flat_map(|y| (0..model.width).filter_map(move |x| buffer.cell((x, y))))
        .filter(|cell| cell.bg == Color::Indexed(31))
        .count();
    assert!(
        highlighted > 0,
        "active drag should paint a visible selection highlight even while scrolled"
    );
}

#[test]
fn stale_cached_snapshot_is_ignored_after_sidebar_layout_change() {
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
            content: "hello world".to_string(),
            ..Default::default()
        },
    });

    let full_width_area = Rect::new(
        0,
        3,
        model.width,
        model.height.saturating_sub(model.input_height() + 4),
    );
    model.chat_selection_snapshot = widgets::chat::build_selection_snapshot(
        full_width_area,
        &model.chat,
        &model.theme,
        model.tick_counter,
        model.retry_wait_start_selected,
    );
    model.chat_drag_anchor = None;
    model.chat_drag_current = None;
    model.chat_drag_anchor_point = None;
    model.chat_drag_current_point = None;

    model.tasks.reduce(task::TaskAction::WorkContextReceived(
        task::ThreadWorkContext {
            thread_id: "thread-1".to_string(),
            entries: vec![task::WorkContextEntry {
                path: "/tmp/demo.txt".to_string(),
                is_text: true,
                ..Default::default()
            }],
        },
    ));
    model.show_sidebar_override = Some(true);

    widgets::chat::reset_build_rendered_lines_call_count();
    let backend = TestBackend::new(model.width, model.height);
    let mut terminal = Terminal::new(backend).expect("test terminal should initialize");
    terminal
        .draw(|frame| model.render(frame))
        .expect("render should fall back to fresh layout instead of using stale snapshot");

    assert_eq!(
        widgets::chat::build_rendered_lines_call_count(),
        1,
        "layout changes should ignore stale cached snapshots and rebuild visible chat rows"
    );
}

#[test]
fn mouse_drag_snapshot_uses_rendered_chat_area_without_sidebar() {
    let mut model = build_model();
    model.width = 100;
    model.height = 40;
    model.show_sidebar_override = Some(false);
    model.chat.reduce(chat::ChatAction::ThreadCreated {
        thread_id: "thread-1".to_string(),
        title: "Thread".to_string(),
    });
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-1".to_string()));
    model
        .anticipatory
        .reduce(crate::state::AnticipatoryAction::Replace(vec![
            crate::wire::AnticipatoryItem {
                id: "digest-1".to_string(),
                ..Default::default()
            },
        ]));
    model.chat.reduce(chat::ChatAction::AppendMessage {
        thread_id: "thread-1".to_string(),
        message: chat::AgentMessage {
            role: chat::MessageRole::Assistant,
            content: "alpha\nbeta\ngamma\ndelta".to_string(),
            ..Default::default()
        },
    });

    let chat_area = rendered_chat_area(&model);
    model.handle_mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: chat_area.x.saturating_add(3),
        row: chat_area
            .y
            .saturating_add(chat_area.height.saturating_sub(2)),
        modifiers: KeyModifiers::NONE,
    });

    let snapshot = model
        .chat_selection_snapshot
        .as_ref()
        .expect("mouse down should create a chat selection snapshot");
    assert!(
        widgets::chat::cached_snapshot_matches_area(snapshot, chat_area),
        "drag snapshots must use the exact rendered chat area"
    );
}

#[test]
fn mouse_drag_snapshot_uses_rendered_chat_area_with_sidebar() {
    let mut model = build_model();
    model.width = 100;
    model.height = 40;
    model.show_sidebar_override = Some(true);
    model.chat.reduce(chat::ChatAction::ThreadCreated {
        thread_id: "thread-1".to_string(),
        title: "Thread".to_string(),
    });
    model
        .chat
        .reduce(chat::ChatAction::SelectThread("thread-1".to_string()));
    model
        .anticipatory
        .reduce(crate::state::AnticipatoryAction::Replace(vec![
            crate::wire::AnticipatoryItem {
                id: "digest-1".to_string(),
                ..Default::default()
            },
        ]));
    model.chat.reduce(chat::ChatAction::AppendMessage {
        thread_id: "thread-1".to_string(),
        message: chat::AgentMessage {
            role: chat::MessageRole::Assistant,
            content: "alpha\nbeta\ngamma\ndelta".to_string(),
            ..Default::default()
        },
    });

    let chat_area = rendered_chat_area(&model);
    model.handle_mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: chat_area.x.saturating_add(3),
        row: chat_area
            .y
            .saturating_add(chat_area.height.saturating_sub(2)),
        modifiers: KeyModifiers::NONE,
    });

    let snapshot = model
        .chat_selection_snapshot
        .as_ref()
        .expect("mouse down should create a chat selection snapshot");
    assert!(
        widgets::chat::cached_snapshot_matches_area(snapshot, chat_area),
        "sidebar drag snapshots must use the exact rendered chat area"
    );
}

#[test]
fn thread_detail_refresh_clears_active_chat_drag_snapshot() {
    let (daemon_tx, daemon_rx) = mpsc::channel();
    let (cmd_tx, _cmd_rx) = unbounded_channel();
    let mut model = TuiModel::new(daemon_rx, cmd_tx);
    model.show_sidebar_override = Some(true);
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
            content: "short content".to_string(),
            ..Default::default()
        },
    });

    let chat_area = rendered_chat_area(&model);
    model.handle_mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: chat_area.x.saturating_add(3),
        row: chat_area
            .y
            .saturating_add(chat_area.height.saturating_sub(2)),
        modifiers: KeyModifiers::NONE,
    });
    assert!(model.chat_selection_snapshot.is_some());

    daemon_tx
        .send(ClientEvent::ThreadDetail(Some(crate::wire::AgentThread {
            id: "thread-1".to_string(),
            title: "Thread".to_string(),
            messages: vec![crate::wire::AgentMessage {
                role: crate::wire::MessageRole::Assistant,
                content: (1..=120)
                    .map(|idx| format!("line {idx}"))
                    .collect::<Vec<_>>()
                    .join("\n"),
                ..Default::default()
            }],
            ..Default::default()
        })))
        .expect("thread detail event should send");
    model.pump_daemon_events();

    assert!(
        model.chat_selection_snapshot.is_none(),
        "thread-detail refresh should invalidate stale drag snapshots"
    );
    assert!(model.chat_drag_anchor.is_none());
    assert!(model.chat_drag_current.is_none());
    assert!(model.chat_drag_anchor_point.is_none());
    assert!(model.chat_drag_current_point.is_none());
}

#[test]
fn thread_detail_conversion_preserves_weles_review_metadata() {
    let (daemon_tx, daemon_rx) = mpsc::channel();
    let (cmd_tx, _cmd_rx) = unbounded_channel();
    let mut model = TuiModel::new(daemon_rx, cmd_tx);

    daemon_tx
        .send(ClientEvent::ThreadDetail(Some(crate::wire::AgentThread {
            id: "thread-1".to_string(),
            title: "Thread".to_string(),
            messages: vec![crate::wire::AgentMessage {
                role: crate::wire::MessageRole::Tool,
                content: "done".to_string(),
                tool_name: Some("bash_command".to_string()),
                tool_call_id: Some("call-1".to_string()),
                tool_status: Some("done".to_string()),
                weles_review: Some(crate::wire::WelesReviewMetaVm {
                    weles_reviewed: false,
                    verdict: "allow".to_string(),
                    reasons: vec!["governance_not_run".to_string()],
                    audit_id: Some("audit-wire-1".to_string()),
                    security_override_mode: None,
                }),
                ..Default::default()
            }],
            ..Default::default()
        })))
        .expect("thread detail event should send");
    model.pump_daemon_events();

    let thread = model.chat.active_thread().expect("thread should exist");
    let message = thread.messages.last().expect("message should exist");
    let stored = message
        .weles_review
        .as_ref()
        .expect("weles review should survive conversion");
    assert!(!stored.weles_reviewed);
    assert_eq!(stored.verdict, "allow");
    assert_eq!(stored.audit_id.as_deref(), Some("audit-wire-1"));
}
