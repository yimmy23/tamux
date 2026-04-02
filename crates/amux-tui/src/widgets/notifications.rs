use ratatui::prelude::*;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::state::{NotificationsHeaderAction, NotificationsState};
use crate::theme::ThemeTokens;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NotificationsHitTarget {
    MarkAllRead,
    ArchiveRead,
    Close,
    Row(usize),
    ToggleExpand(String),
    MarkRead(String),
    Archive(String),
    Delete(String),
    Action {
        notification_id: String,
        action_index: usize,
    },
}

#[derive(Debug, Clone)]
struct RowLayout {
    index: usize,
    top: u16,
    bottom: u16,
    action_y: u16,
    action_regions: Vec<ActionRegion>,
}

#[derive(Debug, Clone)]
struct ActionRegion {
    target: NotificationsHitTarget,
    x: u16,
    width: u16,
    y: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct HeaderButton {
    action: NotificationsHeaderAction,
    label: &'static str,
    enabled: bool,
    selected: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RowActionButton {
    label: String,
    enabled: bool,
    selected: bool,
}

pub fn render(frame: &mut Frame, area: Rect, state: &NotificationsState, theme: &ThemeTokens) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Notifications ")
        .border_style(theme.fg_dim);
    frame.render_widget(block.clone(), area);
    let inner = block.inner(area);
    if inner.height < 4 || inner.width < 24 {
        return;
    }

    render_header(frame, inner, state, theme);

    let list_area = Rect::new(
        inner.x,
        inner.y.saturating_add(2),
        inner.width,
        inner.height.saturating_sub(2),
    );
    let active_items = state.active_items();
    if active_items.is_empty() {
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled("No notifications", theme.fg_dim))),
            list_area,
        );
        return;
    }

    let layouts = visible_layouts(list_area, state);
    for layout in layouts {
        let Some(notification) = active_items.get(layout.index).copied() else {
            continue;
        };
        render_row(
            frame,
            list_area,
            &layout,
            notification,
            state.selected_index() == layout.index,
            state.expanded_id() == Some(notification.id.as_str()),
            if state.selected_index() == layout.index {
                state.selected_row_action_index()
            } else {
                None
            },
            theme,
        );
    }
}

pub fn hit_test(
    area: Rect,
    state: &NotificationsState,
    position: Position,
) -> Option<NotificationsHitTarget> {
    let block = Block::default().borders(Borders::ALL);
    let inner = block.inner(area);
    if position.y == inner.y {
        if let Some(target) = header_hit_test(inner, state, position) {
            return Some(target);
        }
    }

    let list_area = Rect::new(
        inner.x,
        inner.y.saturating_add(2),
        inner.width,
        inner.height.saturating_sub(2),
    );
    for layout in visible_layouts(list_area, state) {
        for region in &layout.action_regions {
            if position.y == region.y
                && position.x >= region.x
                && position.x < region.x.saturating_add(region.width)
            {
                return Some(region.target.clone());
            }
        }

        if position.y >= layout.top
            && position.y <= layout.bottom
            && position.x >= list_area.x
            && position.x < list_area.x.saturating_add(list_area.width)
        {
            return Some(NotificationsHitTarget::Row(layout.index));
        }
    }

    None
}

fn render_header(frame: &mut Frame, inner: Rect, state: &NotificationsState, theme: &ThemeTokens) {
    let buttons = header_buttons(state);
    let buttons_width = buttons
        .iter()
        .map(|button| button.label.chars().count() as u16)
        .sum::<u16>()
        + buttons.len().saturating_sub(1) as u16;
    let left = format!(
        "{} unread  {} total",
        state.unread_count(),
        state.active_items().len()
    );
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(left, theme.fg_dim))),
        Rect::new(
            inner.x,
            inner.y,
            inner.width.saturating_sub(buttons_width.saturating_add(1)),
            1,
        ),
    );

    let mut x = inner
        .x
        .saturating_add(inner.width.saturating_sub(buttons_width));
    for button in buttons {
        let width = button.label.chars().count() as u16;
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                button.label.to_string(),
                header_button_style(&button, theme),
            )))
            .alignment(Alignment::Left),
            Rect::new(x, inner.y, width, 1),
        );
        x = x.saturating_add(width + 1);
    }

    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            "Tab switches focus  ←→ active group  ↑↓ rows  Enter activates  e expands",
            theme.fg_dim,
        ))),
        Rect::new(inner.x, inner.y + 1, inner.width, 1),
    );
}

fn header_hit_test(
    inner: Rect,
    state: &NotificationsState,
    position: Position,
) -> Option<NotificationsHitTarget> {
    let buttons = header_buttons(state);
    let buttons_width = buttons
        .iter()
        .map(|button| button.label.chars().count() as u16)
        .sum::<u16>()
        + buttons.len().saturating_sub(1) as u16;
    let mut x = inner
        .x
        .saturating_add(inner.width.saturating_sub(buttons_width));
    for button in buttons {
        let width = button.label.chars().count() as u16;
        if position.x >= x && position.x < x.saturating_add(width) {
            return button.enabled.then_some(match button.action {
                NotificationsHeaderAction::MarkAllRead => NotificationsHitTarget::MarkAllRead,
                NotificationsHeaderAction::ArchiveRead => NotificationsHitTarget::ArchiveRead,
                NotificationsHeaderAction::Close => NotificationsHitTarget::Close,
            });
        }
        x = x.saturating_add(width + 1);
    }
    None
}

fn header_buttons(state: &NotificationsState) -> Vec<HeaderButton> {
    [
        (NotificationsHeaderAction::MarkAllRead, "[Read all]"),
        (NotificationsHeaderAction::ArchiveRead, "[Archive read]"),
        (NotificationsHeaderAction::Close, "[Close]"),
    ]
    .into_iter()
    .map(|(action, label)| HeaderButton {
        action,
        label,
        enabled: state.is_header_action_enabled(action),
        selected: state.selected_header_action() == Some(action),
    })
    .collect()
}

fn header_button_style(button: &HeaderButton, theme: &ThemeTokens) -> Style {
    if button.selected {
        theme.fg_active.bg(Color::Indexed(236))
    } else if button.enabled {
        theme.fg_dim
    } else {
        Style::default().fg(Color::DarkGray)
    }
}

fn render_row(
    frame: &mut Frame,
    list_area: Rect,
    layout: &RowLayout,
    notification: &amux_protocol::InboxNotification,
    selected: bool,
    expanded: bool,
    focused_row_action: Option<usize>,
    theme: &ThemeTokens,
) {
    let bg = if selected {
        Style::default().bg(Color::Indexed(236))
    } else {
        Style::default()
    };
    let title_style = severity_style(notification.severity.as_str(), theme).add_modifier(
        if notification.read_at.is_none() {
            Modifier::BOLD
        } else {
            Modifier::empty()
        },
    );
    let marker = if notification.read_at.is_none() {
        "●"
    } else {
        " "
    };
    let age = relative_time(notification.updated_at);
    let title_label = truncate_display(
        &notification.title,
        list_area.width.saturating_sub(8) as usize,
    );
    let title_line = Line::from(vec![
        Span::styled(if selected { ">" } else { " " }, bg),
        Span::styled(format!("{} {}", marker, title_label), title_style.patch(bg)),
    ]);
    frame.render_widget(
        Paragraph::new(title_line),
        Rect::new(list_area.x, layout.top, list_area.width, 1),
    );
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(age, theme.fg_dim.patch(bg))))
            .alignment(Alignment::Right),
        Rect::new(list_area.x, layout.top, list_area.width, 1),
    );

    let meta = format!(
        "{}  {}{}",
        notification.source,
        notification.severity,
        notification
            .subtitle
            .as_deref()
            .map(|subtitle| format!("  {}", subtitle))
            .unwrap_or_default()
    );
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            truncate_display(&meta, list_area.width as usize),
            theme.fg_dim.patch(bg),
        ))),
        Rect::new(list_area.x, layout.top + 1, list_area.width, 1),
    );

    let body_lines = body_lines(notification, expanded, list_area.width as usize);
    for (offset, line) in body_lines.iter().enumerate() {
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                line.clone(),
                Style::default().fg(Color::Indexed(252)).patch(bg),
            ))),
            Rect::new(
                list_area.x,
                layout.top + 2 + offset as u16,
                list_area.width,
                1,
            ),
        );
    }

    let action_labels = row_action_buttons(notification, expanded, focused_row_action);
    let mut x = list_area.x;
    for button in action_labels {
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                button.label.clone(),
                row_action_button_style(&button, theme).patch(bg),
            ))),
            Rect::new(x, layout.action_y, button.label.chars().count() as u16, 1),
        );
        x = x.saturating_add(button.label.chars().count() as u16 + 1);
    }
}

fn visible_layouts(area: Rect, state: &NotificationsState) -> Vec<RowLayout> {
    let active_items = state.active_items();
    if active_items.is_empty() || area.height == 0 {
        return Vec::new();
    }

    let estimated_rows = 5usize.max(area.height as usize / 4);
    let start_index = state
        .selected_index()
        .saturating_sub(estimated_rows.saturating_div(2));
    let mut y = area.y;
    let mut layouts = Vec::new();

    for index in start_index..active_items.len() {
        let item = active_items[index];
        let expanded = state.expanded_id() == Some(item.id.as_str());
        let body_len = body_lines(item, expanded, area.width as usize).len() as u16;
        let height = 2u16.saturating_add(body_len).saturating_add(1);
        if y.saturating_add(height) > area.y.saturating_add(area.height) {
            break;
        }

        let mut action_regions = Vec::new();
        let mut x = area.x;
        for (action_index, label) in action_hit_labels(item, expanded).into_iter().enumerate() {
            let width = label.chars().count() as u16;
            let target = match action_index {
                0 => NotificationsHitTarget::ToggleExpand(item.id.clone()),
                1 => NotificationsHitTarget::MarkRead(item.id.clone()),
                2 => NotificationsHitTarget::Archive(item.id.clone()),
                3 => NotificationsHitTarget::Delete(item.id.clone()),
                other => NotificationsHitTarget::Action {
                    notification_id: item.id.clone(),
                    action_index: other.saturating_sub(4),
                },
            };
            action_regions.push(ActionRegion {
                target,
                x,
                width,
                y: y + height - 1,
            });
            x = x.saturating_add(width + 1);
        }

        layouts.push(RowLayout {
            index,
            top: y,
            bottom: y + height - 1,
            action_y: y + height - 1,
            action_regions,
        });
        y = y.saturating_add(height);
    }

    layouts
}

fn row_action_buttons(
    notification: &amux_protocol::InboxNotification,
    expanded: bool,
    focused_row_action: Option<usize>,
) -> Vec<RowActionButton> {
    let mut labels = vec![
        RowActionButton {
            label: if expanded { "[Collapse]" } else { "[Expand]" }.to_string(),
            enabled: true,
            selected: focused_row_action == Some(0),
        },
        RowActionButton {
            label: "[Read]".to_string(),
            enabled: notification.read_at.is_none(),
            selected: focused_row_action == Some(1),
        },
        RowActionButton {
            label: "[Archive]".to_string(),
            enabled: true,
            selected: focused_row_action == Some(2),
        },
        RowActionButton {
            label: "[Delete]".to_string(),
            enabled: true,
            selected: focused_row_action == Some(3),
        },
    ];
    labels.extend(
        notification
            .actions
            .iter()
            .enumerate()
            .map(|(index, action)| RowActionButton {
                label: format!("[{}]", action.label),
                enabled: true,
                selected: focused_row_action == Some(index + 4),
            }),
    );
    labels
}

fn row_action_button_style(button: &RowActionButton, theme: &ThemeTokens) -> Style {
    if button.selected {
        theme
            .fg_active
            .bg(Color::Indexed(236))
            .add_modifier(Modifier::BOLD)
    } else if button.enabled {
        theme.fg_dim
    } else {
        Style::default().fg(Color::DarkGray)
    }
}

fn action_hit_labels(
    notification: &amux_protocol::InboxNotification,
    expanded: bool,
) -> Vec<String> {
    row_action_buttons(notification, expanded, None)
        .into_iter()
        .map(|button| button.label)
        .collect()
}

fn body_lines(
    notification: &amux_protocol::InboxNotification,
    expanded: bool,
    width: usize,
) -> Vec<String> {
    let wrapped = wrap_text(&notification.body, width.saturating_sub(1), 5);
    if expanded {
        wrapped
    } else {
        vec![wrapped.first().cloned().unwrap_or_default()]
    }
}

fn wrap_text(text: &str, width: usize, max_lines: usize) -> Vec<String> {
    if width == 0 {
        return vec![String::new()];
    }
    let mut lines = Vec::new();
    let mut current = String::new();
    for word in text.split_whitespace() {
        if current.is_empty() {
            current = word.to_string();
        } else if current.len() + 1 + word.len() <= width {
            current.push(' ');
            current.push_str(word);
        } else {
            lines.push(current);
            current = word.to_string();
        }
        if lines.len() + 1 >= max_lines && current.len() >= width.saturating_sub(1) {
            break;
        }
    }
    if !current.is_empty() && lines.len() < max_lines {
        lines.push(current);
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    if lines.len() == max_lines && text.split_whitespace().count() > 1 {
        if let Some(last) = lines.last_mut() {
            if last.len() + 1 < width {
                last.push('…');
            }
        }
    }
    lines
}

fn truncate_display(text: &str, width: usize) -> String {
    if text.chars().count() <= width {
        return text.to_string();
    }
    text.chars()
        .take(width.saturating_sub(1))
        .collect::<String>()
        + "…"
}

fn severity_style(severity: &str, theme: &ThemeTokens) -> Style {
    match severity {
        "error" => theme.accent_danger,
        "warning" | "alert" => theme.accent_secondary,
        "info" => theme.accent_primary,
        _ => theme.fg_active,
    }
}

fn relative_time(timestamp_ms: i64) -> String {
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0);
    let elapsed_ms = now_ms.saturating_sub(timestamp_ms).max(0);
    let elapsed_secs = elapsed_ms / 1000;
    if elapsed_secs < 60 {
        format!("{}s", elapsed_secs)
    } else if elapsed_secs < 3600 {
        format!("{}m", elapsed_secs / 60)
    } else if elapsed_secs < 86_400 {
        format!("{}h", elapsed_secs / 3600)
    } else {
        format!("{}d", elapsed_secs / 86_400)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{NotificationsAction, NotificationsHeaderAction};

    fn state_with_notification() -> NotificationsState {
        let mut state = NotificationsState::new();
        state.reduce(crate::state::NotificationsAction::Replace(vec![
            amux_protocol::InboxNotification {
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
                actions: vec![amux_protocol::InboxNotificationAction {
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
}
