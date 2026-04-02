    #[test]
    fn hit_test_selects_last_visible_message_instead_of_previous_padding_block() {
        let chat = chat_with_messages(vec![
            AgentMessage {
                role: MessageRole::Assistant,
                content: "first".into(),
                ..Default::default()
            },
            AgentMessage {
                role: MessageRole::Tool,
                tool_name: Some("read_file".into()),
                tool_status: Some("done".into()),
                content: "tool output".into(),
                ..Default::default()
            },
            AgentMessage {
                role: MessageRole::User,
                content: "continue, also write up your ideas into files".into(),
                ..Default::default()
            },
        ]);

        let area = Rect::new(0, 0, 80, 10);
        let (inner, visible) =
            visible_rendered_lines(area, &chat, &ThemeTokens::default(), 0, false)
            .expect("chat should produce visible lines");
        let last_message_row = visible
            .iter()
            .rposition(|line| {
                line.message_index == Some(2) && matches!(line.kind, RenderedLineKind::MessageBody)
            })
            .expect("last message should be visible");
        let hit_line = &visible[last_message_row];
        let (_, content_start, _) = rendered_line_content_bounds(hit_line);

        let hit = hit_test(
            area,
            &chat,
            &ThemeTokens::default(),
            0,
            Position::new(
                inner.x + content_start as u16 + 1,
                inner.y + last_message_row as u16,
            ),
        );

        assert_eq!(hit, Some(ChatHitTarget::Message(2)));
    }

    #[test]
    fn selected_message_action_bar_stays_visible_at_bottom_edge() {
        let mut chat = chat_with_messages(vec![
            AgentMessage {
                role: MessageRole::Assistant,
                content: "older".into(),
                ..Default::default()
            },
            AgentMessage {
                role: MessageRole::User,
                content: "latest".into(),
                ..Default::default()
            },
        ]);
        chat.select_message(Some(1));

        let area = Rect::new(0, 0, 80, 4);
        let (_, visible) =
            visible_rendered_lines(area, &chat, &ThemeTokens::default(), 0, false)
            .expect("chat should produce visible lines");

        assert!(
            visible.iter().any(|line| {
                matches!(line.kind, RenderedLineKind::ActionBar) && line.message_index == Some(1)
            }),
            "selected message should keep its action row visible even when it is the last visible message"
        );
    }

    #[test]
    fn selecting_message_does_not_shift_visible_window() {
        let mut chat = chat_with_messages(
            (0..8)
                .map(|idx| AgentMessage {
                    role: MessageRole::Assistant,
                    content: format!("message {idx}"),
                    ..Default::default()
                })
                .collect(),
        );
        chat.reduce(ChatAction::ScrollChat(4));

        let area = Rect::new(0, 0, 80, 6);
        let (_, before_visible) =
            visible_rendered_lines(area, &chat, &ThemeTokens::default(), 0, false)
            .expect("chat should produce visible lines before selection");
        let before_last_visible_message = before_visible
            .iter()
            .filter_map(|line| line.message_index)
            .max()
            .expect("a message should be visible before selection");

        chat.select_message(Some(0));

        let (_, after_visible) =
            visible_rendered_lines(area, &chat, &ThemeTokens::default(), 0, false)
            .expect("chat should produce visible lines after selection");
        let after_last_visible_message = after_visible
            .iter()
            .filter_map(|line| line.message_index)
            .max()
            .expect("a message should be visible after selection");

        assert_eq!(
            after_last_visible_message, before_last_visible_message,
            "message selection should not auto-scroll the transcript window"
        );
    }
