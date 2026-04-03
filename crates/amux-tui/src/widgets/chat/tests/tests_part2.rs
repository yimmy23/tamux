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

    #[test]
    fn assistant_messages_show_responder_labels_across_thread_handoffs() {
        let mut chat = ChatState::new();
        chat.reduce(ChatAction::ThreadCreated {
            thread_id: "t1".into(),
            title: "Test".into(),
        });
        chat.reduce(ChatAction::ThreadDetailReceived(AgentThread {
            id: "t1".into(),
            title: "Test".into(),
            messages: vec![
                AgentMessage {
                    role: MessageRole::Assistant,
                    content: "Main reply".into(),
                    ..Default::default()
                },
                AgentMessage {
                    role: MessageRole::System,
                    content: "[[handoff_event]]{\"from_agent_name\":\"Svarog\",\"to_agent_name\":\"Weles\"}".into(),
                    ..Default::default()
                },
                AgentMessage {
                    role: MessageRole::Assistant,
                    content: "Governance reply".into(),
                    ..Default::default()
                },
            ],
            ..Default::default()
        }));

        let (lines, _) = build_rendered_lines(&chat, &ThemeTokens::default(), 80, 0, false);
        let first_message_lines: Vec<String> = lines
            .iter()
            .filter(|line| line.message_index == Some(0))
            .map(rendered_line_plain_text)
            .collect();
        let handoff_message_lines: Vec<String> = lines
            .iter()
            .filter(|line| line.message_index == Some(2))
            .map(rendered_line_plain_text)
            .collect();

        assert!(
            first_message_lines
                .iter()
                .any(|line| line.contains("Responder: Svarog")),
            "expected main responder label, got: {first_message_lines:?}"
        );
        assert!(
            handoff_message_lines
                .iter()
                .any(|line| line.contains("Responder: Weles")),
            "expected handoff responder label, got: {handoff_message_lines:?}"
        );
    }

    #[test]
    fn assistant_messages_fall_back_to_thread_agent_name_when_handoff_marker_is_missing() {
        let mut chat = ChatState::new();
        chat.reduce(ChatAction::ThreadCreated {
            thread_id: "t1".into(),
            title: "Test".into(),
        });
        chat.reduce(ChatAction::ThreadDetailReceived(AgentThread {
            id: "t1".into(),
            agent_name: Some("Weles".into()),
            title: "Test".into(),
            messages: vec![AgentMessage {
                role: MessageRole::Assistant,
                content: "Governance reply".into(),
                ..Default::default()
            }],
            ..Default::default()
        }));

        let (lines, _) = build_rendered_lines(&chat, &ThemeTokens::default(), 80, 0, false);
        let message_lines: Vec<String> = lines
            .iter()
            .filter(|line| line.message_index == Some(0))
            .map(rendered_line_plain_text)
            .collect();

        assert!(
            message_lines
                .iter()
                .any(|line| line.contains("Responder: Weles")),
            "expected responder fallback from thread agent_name, got: {message_lines:?}"
        );
    }
