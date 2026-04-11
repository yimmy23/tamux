    #[test]
    fn chat_handles_empty_state() {
        let chat = ChatState::new();
        assert!(chat.active_thread().is_none());
        assert!(chat.streaming_content().is_empty());
    }

    #[test]
    fn hit_test_selects_clicked_message_row() {
        let chat = chat_with_messages(vec![
            AgentMessage {
                role: MessageRole::User,
                content: "first".into(),
                ..Default::default()
            },
            AgentMessage {
                role: MessageRole::User,
                content: "second".into(),
                ..Default::default()
            },
        ]);

        let hit = hit_test(
            Rect::new(0, 0, 80, 6),
            &chat,
            &ThemeTokens::default(),
            0,
            Position::new(2, 4),
        );

        assert_eq!(hit, Some(ChatHitTarget::Message(1)));
    }

    #[test]
    fn hit_test_marks_reasoning_header_as_toggle() {
        let chat = chat_with_messages(vec![AgentMessage {
            role: MessageRole::Assistant,
            content: "Answer".into(),
            reasoning: Some("Think".into()),
            ..Default::default()
        }]);

        let area = Rect::new(0, 0, 80, 5);
        let (inner, visible) = visible_rendered_lines(area, &chat, &ThemeTokens::default(), 0, false)
            .expect("chat should produce visible lines");
        let header_row = visible
            .iter()
            .position(|line| {
                line.message_index == Some(0)
                    && matches!(line.kind, RenderedLineKind::ReasoningToggle)
            })
            .expect("reasoning header should be visible");
        let hit_line = &visible[header_row];
        let (_, content_start, _) = rendered_line_content_bounds(hit_line);

        let hit = hit_test(
            area,
            &chat,
            &ThemeTokens::default(),
            0,
            Position::new(
                inner.x + content_start as u16,
                inner.y + header_row as u16,
            ),
        );

        assert_eq!(hit, Some(ChatHitTarget::ReasoningToggle(0)));
    }

    #[test]
    fn hit_test_reasoning_header_body_selects_message_instead_of_toggling() {
        let chat = chat_with_messages(vec![AgentMessage {
            role: MessageRole::Assistant,
            content: "Answer".into(),
            reasoning: Some("Think".into()),
            ..Default::default()
        }]);

        let area = Rect::new(0, 0, 80, 5);
        let (inner, visible) = visible_rendered_lines(area, &chat, &ThemeTokens::default(), 0, false)
            .expect("chat should produce visible lines");
        let header_row = visible
            .iter()
            .position(|line| {
                line.message_index == Some(0)
                    && matches!(line.kind, RenderedLineKind::ReasoningToggle)
            })
            .expect("reasoning header should be visible");
        let hit_line = &visible[header_row];
        let (plain, content_start, _) = rendered_line_content_bounds(hit_line);
        let label_offset = plain
            .find("Reasoning")
            .expect("reasoning label should be rendered");

        let hit = hit_test(
            area,
            &chat,
            &ThemeTokens::default(),
            0,
            Position::new(
                inner.x + (content_start + label_offset + 1) as u16,
                inner.y + header_row as u16,
            ),
        );

        assert_eq!(hit, Some(ChatHitTarget::Message(0)));
    }

    #[test]
    fn hit_test_marks_tool_header_as_toggle() {
        let chat = chat_with_messages(vec![AgentMessage {
            role: MessageRole::Tool,
            tool_name: Some("bash_command".into()),
            tool_status: Some("done".into()),
            content: "ok".into(),
            ..Default::default()
        }]);

        let hit = hit_test(
            Rect::new(0, 0, 80, 4),
            &chat,
            &ThemeTokens::default(),
            0,
            Position::new(2, 2),
        );

        assert_eq!(hit, Some(ChatHitTarget::ToolToggle(0)));
    }

    #[test]
    fn hit_test_tool_header_body_selects_message_instead_of_toggling() {
        let chat = chat_with_messages(vec![AgentMessage {
            role: MessageRole::Tool,
            tool_name: Some("bash_command".into()),
            tool_status: Some("done".into()),
            content: "ok".into(),
            ..Default::default()
        }]);

        let area = Rect::new(0, 0, 80, 4);
        let (inner, visible) = visible_rendered_lines(area, &chat, &ThemeTokens::default(), 0, false)
            .expect("chat should produce visible lines");
        let header_row = visible
            .iter()
            .position(|line| line.message_index == Some(0) && matches!(line.kind, RenderedLineKind::ToolToggle))
            .expect("tool header should be visible");
        let hit_line = &visible[header_row];
        let (plain, content_start, _) = rendered_line_content_bounds(hit_line);
        let gear_offset = plain
            .find("⚙")
            .expect("tool gear should be rendered");

        let hit = hit_test(
            area,
            &chat,
            &ThemeTokens::default(),
            0,
            Position::new(
                inner.x + (content_start + gear_offset) as u16,
                inner.y + header_row as u16,
            ),
        );

        assert_eq!(hit, Some(ChatHitTarget::Message(0)));
    }

#[test]
fn read_file_tool_row_renders_clickable_path_chip() {
    let chat = chat_with_messages(vec![AgentMessage {
        role: MessageRole::Tool,
        tool_name: Some("read_file".into()),
        tool_arguments: Some(r#"{"path":"/tmp/demo.txt"}"#.into()),
        tool_status: Some("done".into()),
        content: "file contents".into(),
        ..Default::default()
    }]);

    let (lines, _) = build_rendered_lines(&chat, &ThemeTokens::default(), 80, 0, false);
    let tool_line = lines
        .iter()
        .find(|line| line.message_index == Some(0) && matches!(line.kind, RenderedLineKind::ToolToggle))
        .expect("tool row should be rendered");
    let text = rendered_line_plain_text(tool_line);

    assert!(text.contains("read_file"));
    assert!(text.contains("[demo.txt]"));
    assert!(!text.contains("[/tmp/demo.txt]"));
}

#[test]
fn tool_row_renders_toggle_chevron_before_tool_name() {
    let chat = chat_with_messages(vec![AgentMessage {
        role: MessageRole::Tool,
        tool_name: Some("read_file".into()),
        tool_arguments: Some(r#"{"path":"/tmp/demo.txt"}"#.into()),
        tool_status: Some("done".into()),
        content: "file contents".into(),
        ..Default::default()
    }]);

    let (lines, _) = build_rendered_lines(&chat, &ThemeTokens::default(), 80, 0, false);
    let tool_line = lines
        .iter()
        .find(|line| line.message_index == Some(0) && matches!(line.kind, RenderedLineKind::ToolToggle))
        .expect("tool row should be rendered");
    let text = rendered_line_plain_text(tool_line);

    assert!(text.contains("▶"), "expected collapsed chevron, got: {text}");
    assert!(text.contains("⚙"), "expected tool gear, got: {text}");
}

#[test]
fn edit_tool_row_renders_clickable_path_chip() {
    let chat = chat_with_messages(vec![AgentMessage {
        role: MessageRole::Tool,
        tool_name: Some("write_file".into()),
        tool_arguments: Some(r#"{"path":"/tmp/demo.txt"}"#.into()),
        tool_status: Some("done".into()),
        content: "written".into(),
        ..Default::default()
    }]);

    let (lines, _) = build_rendered_lines(&chat, &ThemeTokens::default(), 80, 0, false);
    let tool_line = lines
        .iter()
        .find(|line| line.message_index == Some(0) && matches!(line.kind, RenderedLineKind::ToolToggle))
        .expect("tool row should be rendered");
    let text = rendered_line_plain_text(tool_line);

    assert!(text.contains("write_file"));
    assert!(text.contains("[demo.txt]"));
    assert!(!text.contains("[/tmp/demo.txt]"));
}

#[test]
fn all_file_mutation_tool_rows_use_filename_chip() {
    let cases = [
        (
            "create_file",
            serde_json::json!({
                "path": "/tmp/demo.txt",
                "content": "hello"
            })
            .to_string(),
        ),
        (
            "append_to_file",
            serde_json::json!({
                "path": "/tmp/demo.txt",
                "content": "hello"
            })
            .to_string(),
        ),
        (
            "replace_in_file",
            serde_json::json!({
                "path": "/tmp/demo.txt",
                "old_text": "old",
                "new_text": "new"
            })
            .to_string(),
        ),
        (
            "apply_file_patch",
            serde_json::json!({
                "path": "/tmp/demo.txt",
                "edits": [
                    {
                        "old_text": "old",
                        "new_text": "new"
                    }
                ]
            })
            .to_string(),
        ),
        (
            "apply_patch",
            serde_json::json!({
                "input": "*** Begin Patch\n*** Update File: /tmp/demo.txt\n@@\n-old\n+new\n*** End Patch"
            })
            .to_string(),
        ),
    ];

    for (tool_name, tool_arguments) in cases {
        let chat = chat_with_messages(vec![AgentMessage {
            role: MessageRole::Tool,
            tool_name: Some(tool_name.into()),
            tool_arguments: Some(tool_arguments),
            tool_status: Some("done".into()),
            content: "ok".into(),
            ..Default::default()
        }]);

        let (lines, _) = build_rendered_lines(&chat, &ThemeTokens::default(), 80, 0, false);
        let tool_line = lines
            .iter()
            .find(|line| {
                line.message_index == Some(0)
                    && matches!(line.kind, RenderedLineKind::ToolToggle)
            })
            .expect("tool row should be rendered");
        let text = rendered_line_plain_text(tool_line);

        assert!(text.contains(tool_name), "expected tool name in row: {text}");
        assert!(
            text.contains("[demo.txt]"),
            "expected filename chip for {tool_name}, got: {text}"
        );
        assert!(
            !text.contains("[/tmp/demo.txt]"),
            "unexpected full path chip for {tool_name}, got: {text}"
        );
    }
}

#[test]
fn apply_file_patch_tool_row_uses_filename_chip() {
    let chat = chat_with_messages(vec![AgentMessage {
        role: MessageRole::Tool,
        tool_name: Some("apply_file_patch".into()),
        tool_arguments: Some(
            serde_json::json!({
                "path": "/tmp/demo.txt",
                "edits": [
                    {
                        "old_text": "old",
                        "new_text": "new"
                    }
                ]
            })
            .to_string(),
        ),
        tool_status: Some("done".into()),
        content: "patched".into(),
        ..Default::default()
    }]);

    let (lines, _) = build_rendered_lines(&chat, &ThemeTokens::default(), 80, 0, false);
    let tool_line = lines
        .iter()
        .find(|line| line.message_index == Some(0) && matches!(line.kind, RenderedLineKind::ToolToggle))
        .expect("tool row should be rendered");
    let text = rendered_line_plain_text(tool_line);

    assert!(text.contains("apply_file_patch"));
    assert!(text.contains("[demo.txt]"));
    assert!(!text.contains("[/tmp/demo.txt]"));
}

#[test]
fn invalid_tool_arguments_do_not_create_file_chip() {
    let chat = chat_with_messages(vec![AgentMessage {
        role: MessageRole::Tool,
        tool_name: Some("read_file".into()),
        tool_arguments: Some("{not json}".into()),
        tool_status: Some("done".into()),
        content: "file contents".into(),
        ..Default::default()
    }]);

    let (lines, _) = build_rendered_lines(&chat, &ThemeTokens::default(), 80, 0, false);
    let tool_line = lines
        .iter()
        .find(|line| line.message_index == Some(0) && matches!(line.kind, RenderedLineKind::ToolToggle))
        .expect("tool row should be rendered");
    let text = rendered_line_plain_text(tool_line);

    assert!(text.contains("read_file"));
    assert!(!text.contains("[/tmp/demo.txt]"));
}

#[test]
fn hit_test_returns_tool_file_path_target() {
    let chat = chat_with_messages(vec![AgentMessage {
        role: MessageRole::Tool,
        tool_name: Some("read_file".into()),
        tool_arguments: Some(r#"{"path":"/tmp/demo.txt"}"#.into()),
        tool_status: Some("done".into()),
        content: "file contents".into(),
        ..Default::default()
    }]);

    let area = Rect::new(0, 0, 100, 6);
    let (inner, visible) = visible_rendered_lines(area, &chat, &ThemeTokens::default(), 0, false)
        .expect("chat should produce visible lines");
    let tool_row = visible
        .iter()
        .position(|line| line.message_index == Some(0) && matches!(line.kind, RenderedLineKind::ToolToggle))
        .expect("tool row should be visible");
    let hit_line = &visible[tool_row];
    let (plain, content_start, _) = rendered_line_content_bounds(hit_line);
    let chip_col = plain
        .find("[demo.txt]")
        .expect("path chip should be rendered on the tool row");

    let chip_hit = hit_test(
        area,
        &chat,
        &ThemeTokens::default(),
        0,
        Position::new(
            inner.x + chip_col as u16 + 1,
            inner.y + tool_row as u16,
        ),
    );
    let row_hit = hit_test(
        area,
        &chat,
        &ThemeTokens::default(),
        0,
        Position::new(
            inner.x + content_start as u16 + 4,
            inner.y + tool_row as u16,
        ),
    );

    assert_eq!(
        chip_hit,
        Some(ChatHitTarget::ToolFilePath { message_index: 0 })
    );
    assert_eq!(row_hit, Some(ChatHitTarget::Message(0)));
}

    #[test]
    fn streaming_append_preserves_locked_viewport() {
        let mut chat = ChatState::new();
        chat.reduce(ChatAction::ThreadCreated {
            thread_id: "t1".into(),
            title: "Test".into(),
        });
        let initial = (1..=40)
            .map(|idx| format!("line {idx}"))
            .collect::<Vec<_>>()
            .join("\n");
        chat.reduce(ChatAction::Delta {
            thread_id: "t1".into(),
            content: initial,
        });

        let inner_height = 8usize;
        let inner_width = 80usize;
        chat.reduce(ChatAction::ScrollChat(3));
        let (before_lines, before_ranges) =
            build_rendered_lines(&chat, &ThemeTokens::default(), inner_width, 0, false);
        let before_scroll =
            resolved_scroll(&chat, before_lines.len(), inner_height, &before_ranges);
        let (_, before_start, _) =
            visible_window_bounds(before_lines.len(), inner_height, before_scroll);

        chat.reduce(ChatAction::Delta {
            thread_id: "t1".into(),
            content: "\nline 41".into(),
        });

        let (after_lines, after_ranges) =
            build_rendered_lines(&chat, &ThemeTokens::default(), inner_width, 0, false);
        let after_scroll = resolved_scroll(&chat, after_lines.len(), inner_height, &after_ranges);
        let (_, after_start, _) =
            visible_window_bounds(after_lines.len(), inner_height, after_scroll);

        assert_eq!(
            after_start, before_start,
            "locked viewport should stay anchored while new streamed lines append"
        );
    }

    #[test]
    fn hit_test_full_mode_reasoning_header_only_selects_message() {
        let mut chat = chat_with_messages(vec![AgentMessage {
            role: MessageRole::Assistant,
            content: "Answer".into(),
            reasoning: Some("Think".into()),
            ..Default::default()
        }]);
        chat.reduce(ChatAction::SetTranscriptMode(TranscriptMode::Full));

        let hit = hit_test(
            Rect::new(0, 0, 80, 6),
            &chat,
            &ThemeTokens::default(),
            0,
            Position::new(2, 2),
        );

        assert_eq!(hit, Some(ChatHitTarget::Message(0)));
    }

    #[test]
    fn waiting_retry_row_shows_yes_countdown_and_no_action() {
        let mut chat = ChatState::new();
        chat.reduce(ChatAction::ThreadCreated {
            thread_id: "t1".into(),
            title: "Test".into(),
        });
        chat.reduce(ChatAction::SetRetryStatus {
            thread_id: "t1".into(),
            phase: RetryPhase::Waiting,
            attempt: 3,
            max_retries: 3,
            delay_ms: 30_000,
            failure_class: "rate_limit".into(),
            message: "429 Too Many Requests".into(),
            received_at_tick: 0,
        });

        let (lines, _) = build_rendered_lines(&chat, &ThemeTokens::default(), 80, 20, false);
        let action_line = lines
            .iter()
            .find(|line| matches!(line.kind, RenderedLineKind::RetryAction))
            .expect("retry action line should be rendered");
        let action_text = rendered_line_plain_text(action_line);
        assert!(action_text.contains("[Yes 29s]"));
        assert!(action_text.contains("[No]"));

        let area = Rect::new(0, 0, 80, 8);
        let (inner, visible) =
            visible_rendered_lines(area, &chat, &ThemeTokens::default(), 20, false)
            .expect("retry state should render visible lines");
        let retry_row = visible
            .iter()
            .position(|line| matches!(line.kind, RenderedLineKind::RetryAction))
            .expect("retry action row should be visible");
        let hit_line = &visible[retry_row];
        let (_, content_start, _) = rendered_line_content_bounds(hit_line);
        let yes_width = UnicodeWidthStr::width("[Yes 29s]");
        let no_x = inner.x + (content_start + yes_width + 2) as u16;
        let no_y = inner.y + retry_row as u16;

        let hit = hit_test(
            area,
            &chat,
            &ThemeTokens::default(),
            20,
            Position::new(no_x, no_y),
        );
        assert_eq!(hit, Some(ChatHitTarget::RetryStop));

        let yes_x = inner.x + (content_start + 1) as u16;
        let yes_hit = hit_test(
            area,
            &chat,
            &ThemeTokens::default(),
            20,
            Position::new(yes_x, no_y),
        );
        assert_eq!(yes_hit, Some(ChatHitTarget::RetryStartNow));
    }

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
        assert!(text.contains("[Copy]"), "tool action bar should keep copy: {text}");
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
