#[test]
fn waiting_retry_row_highlights_yes_when_selected() {
    let mut chat = ChatState::new();
    chat.reduce(ChatAction::ThreadCreated {
        thread_id: "t1".into(),
        title: "Test".into(),
    });
    chat.reduce(ChatAction::SetRetryStatus {
        thread_id: "t1".into(),
        phase: RetryPhase::Waiting,
        attempt: 1,
        max_retries: 0,
        delay_ms: 30_000,
        failure_class: "transport".into(),
        message: "upstream transport error".into(),
        received_at_tick: 0,
    });

    let (lines, _) = build_rendered_lines(&chat, &ThemeTokens::default(), 80, 0, true);
    let action_line = lines
        .iter()
        .find(|line| matches!(line.kind, RenderedLineKind::RetryAction))
        .expect("retry action line should be rendered");
    let yes_span = action_line
        .line
        .spans
        .iter()
        .find(|span| span.content.contains("[Yes 30s]"))
        .expect("yes action should be present");
    let no_span = action_line
        .line
        .spans
        .iter()
        .find(|span| span.content.contains("[No]"))
        .expect("no action should be present");

    assert_eq!(yes_span.style.fg, ThemeTokens::default().accent_primary.fg);
    assert_eq!(no_span.style.fg, ThemeTokens::default().fg_dim.fg);
}

#[test]
fn hit_test_targets_copy_action_for_selected_message_bar() {
    let mut chat = chat_with_messages(vec![
        AgentMessage {
            role: MessageRole::User,
            content: "first".into(),
            ..Default::default()
        },
        AgentMessage {
            role: MessageRole::Assistant,
            content: "second".into(),
            ..Default::default()
        },
    ]);
    chat.select_message(Some(1));

    let area = Rect::new(0, 0, 80, 10);
    let (inner, visible) = visible_rendered_lines(area, &chat, &ThemeTokens::default(), 0, false)
        .expect("chat should produce visible lines");
    let action_row = visible
        .iter()
        .position(|line| {
            matches!(line.kind, RenderedLineKind::ActionBar) && line.message_index == Some(1)
        })
        .expect("selected message should render an inline action bar");
    let hit_line = &visible[action_row];
    let (_, content_start, _) = rendered_line_content_bounds(hit_line);
    let hit = hit_test(
        area,
        &chat,
        &ThemeTokens::default(),
        0,
        Position::new(
            inner.x + content_start as u16 + 1,
            inner.y + action_row as u16,
        ),
    );

    assert_eq!(hit, Some(ChatHitTarget::CopyMessage(1)));
}

#[test]
fn hit_test_targets_copy_action_when_clicking_button_padding() {
    let mut chat = chat_with_messages(vec![AgentMessage {
        role: MessageRole::Assistant,
        content: "second".into(),
        ..Default::default()
    }]);
    chat.select_message(Some(0));

    let area = Rect::new(0, 0, 80, 8);
    let (inner, visible) = visible_rendered_lines(area, &chat, &ThemeTokens::default(), 0, false)
        .expect("chat should produce visible lines");
    let action_row = visible
        .iter()
        .position(|line| {
            matches!(line.kind, RenderedLineKind::ActionBar) && line.message_index == Some(0)
        })
        .expect("selected message should render an inline action bar");
    let hit_line = &visible[action_row];
    let (_, content_start, _) = rendered_line_content_bounds(hit_line);
    let copy_label_width = UnicodeWidthStr::width("[Copy]");
    let trailing_padding_col = inner.x + content_start as u16 + copy_label_width as u16 + 1;
    let hit = hit_test(
        area,
        &chat,
        &ThemeTokens::default(),
        0,
        Position::new(trailing_padding_col, inner.y + action_row as u16),
    );

    assert_eq!(
        hit,
        Some(ChatHitTarget::CopyMessage(0)),
        "clicking padded button space should still resolve to the copy action"
    );
}

#[test]
fn hit_test_targets_copy_action_when_clicking_row_below_action_bar() {
    let mut chat = chat_with_messages(vec![AgentMessage {
        role: MessageRole::Assistant,
        content: "second".into(),
        ..Default::default()
    }]);
    chat.select_message(Some(0));

    let area = Rect::new(0, 0, 80, 9);
    let (inner, visible) = visible_rendered_lines(area, &chat, &ThemeTokens::default(), 0, false)
        .expect("chat should produce visible lines");
    let action_row = visible
        .iter()
        .position(|line| {
            matches!(line.kind, RenderedLineKind::ActionBar) && line.message_index == Some(0)
        })
        .expect("selected message should render an inline action bar");
    let hit_line = &visible[action_row];
    let (_, content_start, _) = rendered_line_content_bounds(hit_line);
    let hit = hit_test(
        area,
        &chat,
        &ThemeTokens::default(),
        0,
        Position::new(
            inner.x + content_start as u16 + 1,
            inner.y + action_row as u16 + 1,
        ),
    );

    assert_eq!(
        hit,
        Some(ChatHitTarget::CopyMessage(0)),
        "clicking the padding row below the action bar should resolve to the same button"
    );
}

#[test]
fn selected_message_action_bar_highlights_only_primary_action() {
    let mut chat = chat_with_messages(vec![AgentMessage {
        role: MessageRole::User,
        content: "first".into(),
        ..Default::default()
    }]);
    chat.select_message(Some(0));

    let (lines, _) = build_rendered_lines(&chat, &ThemeTokens::default(), 80, 0, false);
    let action_line = lines
        .iter()
        .find(|line| matches!(line.kind, RenderedLineKind::ActionBar))
        .expect("selected message should render an action bar");

    let copy_span = action_line
        .line
        .spans
        .iter()
        .find(|span| span.content.contains("[Copy]"))
        .expect("copy action should be present");
    let resend_span = action_line
        .line
        .spans
        .iter()
        .find(|span| span.content.contains("[Resend]"))
        .expect("resend action should be present");

    assert_eq!(copy_span.style.fg, ThemeTokens::default().accent_primary.fg);
    assert_eq!(copy_span.style.bg, Some(Color::Indexed(236)));
    assert_eq!(resend_span.style.fg, ThemeTokens::default().fg_dim.fg);
    assert_eq!(resend_span.style.bg, Some(Color::Indexed(236)));
}

#[test]
fn user_message_actions_include_pin_before_delete() {
    let chat = chat_with_messages(vec![AgentMessage {
        id: Some("message-1".into()),
        role: MessageRole::User,
        content: "first".into(),
        pinned_for_compaction: false,
        ..Default::default()
    }]);

    let message = chat
        .active_thread()
        .and_then(|thread| thread.messages.first())
        .expect("message should exist");
    let labels: Vec<String> = message_action_targets(&chat, 0, message, 0)
        .into_iter()
        .map(|(label, _)| label)
        .collect();

    assert_eq!(labels, vec!["[Copy]", "[Resend]", "[Pin]", "[Delete]"]);
}

#[test]
fn assistant_pinned_message_actions_include_unpin_before_delete() {
    let chat = chat_with_messages(vec![AgentMessage {
        id: Some("message-1".into()),
        role: MessageRole::Assistant,
        content: "answer".into(),
        pinned_for_compaction: true,
        ..Default::default()
    }]);

    let message = chat
        .active_thread()
        .and_then(|thread| thread.messages.first())
        .expect("message should exist");
    let labels: Vec<String> = message_action_targets(&chat, 0, message, 0)
        .into_iter()
        .map(|(label, _)| label)
        .collect();

    assert_eq!(
        labels,
        vec!["[Copy]", "[Regenerate]", "[Unpin]", "[Delete]"]
    );
}

#[test]
fn selected_expanded_tool_message_action_bar_shows_collapse() {
    let mut chat = chat_with_messages(vec![AgentMessage {
        role: MessageRole::Tool,
        tool_name: Some("read_file".into()),
        tool_status: Some("done".into()),
        content: "file contents".into(),
        ..Default::default()
    }]);
    chat.select_message(Some(0));
    chat.toggle_tool_expansion(0);

    let (lines, _) = build_rendered_lines(&chat, &ThemeTokens::default(), 80, 0, false);
    let action_line = lines
        .iter()
        .find(|line| {
            line.message_index == Some(0) && matches!(line.kind, RenderedLineKind::ActionBar)
        })
        .expect("selected tool message should render an action bar");
    let text = rendered_line_plain_text(action_line);

    assert!(
        text.contains("[Collapse]"),
        "expanded tool action bar should expose a collapse control: {text}"
    );
    assert!(
        text.contains("[Copy]"),
        "tool action bar should keep copy: {text}"
    );
}

#[test]
fn selected_expanded_reasoning_message_action_bar_shows_collapse() {
    let mut chat = chat_with_messages(vec![AgentMessage {
        role: MessageRole::Assistant,
        content: "Answer".into(),
        reasoning: Some("Think".into()),
        ..Default::default()
    }]);
    chat.select_message(Some(0));
    chat.toggle_reasoning(0);

    let (lines, _) = build_rendered_lines(&chat, &ThemeTokens::default(), 80, 0, false);
    let action_line = lines
        .iter()
        .find(|line| {
            line.message_index == Some(0) && matches!(line.kind, RenderedLineKind::ActionBar)
        })
        .expect("selected reasoning message should render an action bar");
    let text = rendered_line_plain_text(action_line);

    assert!(
        text.contains("[Collapse]"),
        "expanded reasoning action bar should expose a collapse control: {text}"
    );
    assert!(
        text.contains("[Copy]"),
        "reasoning action bar should keep copy: {text}"
    );
}

#[test]
fn selected_expanded_reasoning_message_action_bar_targets_toggle() {
    let mut chat = chat_with_messages(vec![AgentMessage {
        role: MessageRole::Assistant,
        content: "Answer".into(),
        reasoning: Some("Think".into()),
        ..Default::default()
    }]);
    chat.select_message(Some(0));
    chat.toggle_reasoning(0);

    let area = Rect::new(0, 0, 80, 10);
    let (inner, visible) = visible_rendered_lines(area, &chat, &ThemeTokens::default(), 0, false)
        .expect("chat should produce visible lines");
    let action_row = visible
        .iter()
        .position(|line| {
            matches!(line.kind, RenderedLineKind::ActionBar) && line.message_index == Some(0)
        })
        .expect("selected reasoning message should render an inline action bar");
    let hit_line = &visible[action_row];
    let (_, content_start, _) = rendered_line_content_bounds(hit_line);

    let hit = hit_test(
        area,
        &chat,
        &ThemeTokens::default(),
        0,
        Position::new(
            inner.x + content_start as u16 + 1,
            inner.y + action_row as u16,
        ),
    );

    assert_eq!(
        hit,
        Some(ChatHitTarget::ReasoningToggle(0)),
        "clicking the first reasoning action should toggle the reasoning block"
    );
}

#[test]
fn meta_cognition_header_uses_reasoning_toggle_hit_target() {
    let chat = chat_with_messages(vec![AgentMessage {
        role: MessageRole::System,
        content: "Meta-cognitive intervention: warning before tool execution.\nPlanned tool: read_file".into(),
        ..Default::default()
    }]);

    let area = Rect::new(0, 0, 80, 5);
    let (inner, visible) = visible_rendered_lines(area, &chat, &ThemeTokens::default(), 0, false)
        .expect("chat should produce visible lines");
    let header_row = visible
        .iter()
        .position(|line| {
            line.message_index == Some(0) && matches!(line.kind, RenderedLineKind::ReasoningToggle)
        })
        .expect("meta-cognition header should be toggleable");
    let hit_line = &visible[header_row];
    let (_, content_start, _) = rendered_line_content_bounds(hit_line);

    let hit = hit_test(
        area,
        &chat,
        &ThemeTokens::default(),
        0,
        Position::new(inner.x + content_start as u16, inner.y + header_row as u16),
    );

    assert_eq!(hit, Some(ChatHitTarget::ReasoningToggle(0)));
}

#[test]
fn selected_meta_cognition_message_action_bar_targets_expand() {
    let mut chat = chat_with_messages(vec![AgentMessage {
        role: MessageRole::System,
        content: "Meta-cognitive intervention: warning before tool execution.\nPlanned tool: read_file".into(),
        ..Default::default()
    }]);
    chat.select_message(Some(0));

    let area = Rect::new(0, 0, 80, 10);
    let (inner, visible) = visible_rendered_lines(area, &chat, &ThemeTokens::default(), 0, false)
        .expect("chat should produce visible lines");
    let action_row = visible
        .iter()
        .position(|line| {
            matches!(line.kind, RenderedLineKind::ActionBar) && line.message_index == Some(0)
        })
        .expect("selected meta-cognition message should render an inline action bar");
    let hit_line = &visible[action_row];
    let (_, content_start, _) = rendered_line_content_bounds(hit_line);
    let action_text = rendered_line_plain_text(hit_line);

    let hit = hit_test(
        area,
        &chat,
        &ThemeTokens::default(),
        0,
        Position::new(
            inner.x + content_start as u16 + 1,
            inner.y + action_row as u16,
        ),
    );

    assert!(
        action_text.contains("[Expand]"),
        "meta-cognition action bar should expose expand control: {action_text}"
    );
    assert_eq!(hit, Some(ChatHitTarget::ReasoningToggle(0)));
}

#[test]
fn selected_background_operation_finished_message_action_bar_targets_expand() {
    let mut chat = chat_with_messages(vec![AgentMessage {
        role: MessageRole::System,
        content: "Background operation finished.\n\noperation_id: op-123\ntool: shell\nstate: succeeded\nregistered_at: 123\n\nOperation status:\n{\"state\":\"succeeded\"}".into(),
        ..Default::default()
    }]);
    chat.select_message(Some(0));

    let area = Rect::new(0, 0, 80, 10);
    let (inner, visible) = visible_rendered_lines(area, &chat, &ThemeTokens::default(), 0, false)
        .expect("chat should produce visible lines");
    let action_row = visible
        .iter()
        .position(|line| {
            matches!(line.kind, RenderedLineKind::ActionBar) && line.message_index == Some(0)
        })
        .expect("selected background operation message should render an inline action bar");
    let hit_line = &visible[action_row];
    let (_, content_start, _) = rendered_line_content_bounds(hit_line);
    let action_text = rendered_line_plain_text(hit_line);

    let hit = hit_test(
        area,
        &chat,
        &ThemeTokens::default(),
        0,
        Position::new(
            inner.x + content_start as u16 + 1,
            inner.y + action_row as u16,
        ),
    );

    assert!(
        action_text.contains("[Expand]"),
        "background operation action bar should expose expand control: {action_text}"
    );
    assert_eq!(hit, Some(ChatHitTarget::ReasoningToggle(0)));
}

#[test]
fn render_draws_highlight_for_mouse_selection() {
    let chat = chat_with_messages(vec![AgentMessage {
        role: MessageRole::User,
        content: "alpha beta gamma".into(),
        ..Default::default()
    }]);
    let area = Rect::new(0, 0, 40, 8);
    let row = (0..area.height)
        .find(|candidate| {
            selection_point_from_mouse(
                area,
                &chat,
                &ThemeTokens::default(),
                0,
                Position::new(4, *candidate),
            )
            .is_some()
        })
        .expect("chat should expose a selectable row");
    let start = selection_point_from_mouse(
        area,
        &chat,
        &ThemeTokens::default(),
        0,
        Position::new(4, row),
    )
    .expect("selection start should resolve");
    let end = selection_point_from_mouse(
        area,
        &chat,
        &ThemeTokens::default(),
        0,
        Position::new(10, row),
    )
    .expect("selection end should resolve");

    let backend = TestBackend::new(area.width, area.height);
    let mut terminal = Terminal::new(backend).expect("test terminal should initialize");
    terminal
        .draw(|frame| {
            render(
                frame,
                area,
                &chat,
                &ThemeTokens::default(),
                0,
                false,
                true,
                Some((start, end)),
            );
        })
        .expect("chat render should succeed");

    let buffer = terminal.backend().buffer();
    let highlighted = (0..area.height)
        .flat_map(|y| (0..area.width).filter_map(move |x| buffer.cell((x, y))))
        .filter(|cell| cell.bg == Color::Indexed(31))
        .count();

    assert!(
        highlighted > 0,
        "mouse selection should paint a visible highlight in the buffer"
    );
}
