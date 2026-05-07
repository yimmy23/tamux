    use super::*;
    use super::render_to_body_lines::{
        header_button_style, header_buttons, row_action_button_style, row_action_buttons,
        visible_layouts,
    };
    use crate::state::{NotificationsAction, NotificationsHeaderAction, NotificationsState};
    use crate::theme::ThemeTokens;
    use ratatui::layout::{Position, Rect};
    use ratatui::style::{Color, Modifier, Style};
    use ratatui::widgets::{Block, Borders};

    fn state_with_notification() -> NotificationsState {
        let mut state = NotificationsState::new();
        state.reduce(crate::state::NotificationsAction::Replace(vec![
            zorai_protocol::InboxNotification {
                id: "n1".to_string(),
                source: "plugin_auth".to_string(),
                kind: "plugin_needs_reconnect".to_string(),
                title: "Reconnect plugin".to_string(),
                body: "Reconnect Gmail before it expires.".to_string(),
                subtitle: Some("gmail".to_string()),
                severity: "warning".to_string(),
                created_at: 1,
                updated_at: 1,
                read_at: None,
                archived_at: None,
                deleted_at: None,
                actions: vec![zorai_protocol::InboxNotificationAction {
                    id: "open_plugin_settings".to_string(),
                    label: "Open plugin settings".to_string(),
                    action_type: "open_plugin_settings".to_string(),
                    target: Some("gmail".to_string()),
                    payload_json: None,
                }],
                metadata_json: None,
            },
        ]));
        state
    }

    #[test]
    fn row_hit_test_returns_action_for_button_region() {
        let state = state_with_notification();
        let area = Rect::new(0, 0, 80, 16);
        let inner = Block::default().borders(Borders::ALL).inner(area);
        let list_area = Rect::new(
            inner.x,
            inner.y + 2,
            inner.width,
            inner.height.saturating_sub(2),
        );
        let layout = visible_layouts(list_area, &state).remove(0);
        let hit = hit_test(
            area,
            &state,
            Position::new(layout.action_regions[0].x, layout.action_y),
        );
        assert_eq!(
            hit,
            Some(NotificationsHitTarget::ToggleExpand("n1".to_string()))
        );
    }

    #[test]
    fn header_buttons_dim_inactive_actions_and_highlight_focus() {
        let mut state = state_with_notification();
        state.reduce(NotificationsAction::FocusHeader(Some(
            NotificationsHeaderAction::MarkAllRead,
        )));

        let buttons = header_buttons(&state);
        let theme = ThemeTokens::default();

        assert_eq!(buttons[0].action, NotificationsHeaderAction::MarkAllRead);
        assert!(buttons[0].enabled);
        assert!(buttons[0].selected);
        assert_eq!(
            header_button_style(&buttons[0], &theme),
            theme.fg_active.bg(Color::Indexed(236))
        );

        assert_eq!(buttons[1].action, NotificationsHeaderAction::ArchiveRead);
        assert!(!buttons[1].enabled);
        assert!(!buttons[1].selected);
        assert_eq!(
            header_button_style(&buttons[1], &theme),
            Style::default().fg(Color::DarkGray)
        );

        assert_eq!(buttons[2].action, NotificationsHeaderAction::Close);
        assert!(buttons[2].enabled);
        assert!(!buttons[2].selected);
        assert_eq!(header_button_style(&buttons[2], &theme), theme.fg_dim);
    }

    #[test]
    fn row_action_buttons_dim_inactive_actions_and_highlight_focus() {
        let mut state = state_with_notification();
        state.reduce(crate::state::NotificationsAction::FocusRowAction(Some(1)));

        let notification = state
            .selected_item()
            .expect("notification should be selected");
        let buttons = row_action_buttons(notification, false, state.selected_row_action_index());
        let theme = ThemeTokens::default();

        assert_eq!(buttons[0].label, "[Expand]");
        assert!(buttons[0].enabled);
        assert!(!buttons[0].selected);
        assert_eq!(row_action_button_style(&buttons[0], &theme), theme.fg_dim);

        assert_eq!(buttons[1].label, "[Read]");
        assert!(buttons[1].enabled);
        assert!(buttons[1].selected);
        assert_eq!(
            row_action_button_style(&buttons[1], &theme),
            theme
                .fg_active
                .bg(Color::Indexed(236))
                .add_modifier(Modifier::BOLD)
        );

        assert_eq!(buttons[2].label, "[Archive]");
        assert!(buttons[2].enabled);
        assert!(!buttons[2].selected);
        assert_eq!(row_action_button_style(&buttons[2], &theme), theme.fg_dim);
    }
