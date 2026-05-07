use ratatui::prelude::*;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::state::{NotificationsHeaderAction, NotificationsState};
use crate::theme::ThemeTokens;

use super::wrap_text_to_relative_time::{
    relative_time, severity_style, truncate_display, wrap_text,
};

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
pub(super) struct RowLayout {
    pub(super) index: usize,
    pub(super) top: u16,
    pub(super) bottom: u16,
    pub(super) action_y: u16,
    pub(super) action_regions: Vec<ActionRegion>,
}

#[derive(Debug, Clone)]
pub(super) struct ActionRegion {
    pub(super) target: NotificationsHitTarget,
    pub(super) x: u16,
    pub(super) width: u16,
    pub(super) y: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct HeaderButton {
    pub(super) action: NotificationsHeaderAction,
    pub(super) label: &'static str,
    pub(super) enabled: bool,
    pub(super) selected: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct RowActionButton {
    pub(super) label: String,
    pub(super) enabled: bool,
    pub(super) selected: bool,
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

pub(super) fn header_buttons(state: &NotificationsState) -> Vec<HeaderButton> {
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

pub(super) fn header_button_style(button: &HeaderButton, theme: &ThemeTokens) -> Style {
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
    notification: &zorai_protocol::InboxNotification,
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

pub(super) fn visible_layouts(area: Rect, state: &NotificationsState) -> Vec<RowLayout> {
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

pub(super) fn row_action_buttons(
    notification: &zorai_protocol::InboxNotification,
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

pub(super) fn row_action_button_style(button: &RowActionButton, theme: &ThemeTokens) -> Style {
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
    notification: &zorai_protocol::InboxNotification,
    expanded: bool,
) -> Vec<String> {
    row_action_buttons(notification, expanded, None)
        .into_iter()
        .map(|button| button.label)
        .collect()
}

fn body_lines(
    notification: &zorai_protocol::InboxNotification,
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
