use ratatui::prelude::*;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::state::NotificationsState;
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
        if let Some(target) = header_hit_test(inner, position) {
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
    let left = format!(
        "{} unread  {} total",
        state.unread_count(),
        state.active_items().len()
    );
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(left, theme.fg_dim))),
        Rect::new(inner.x, inner.y, inner.width.saturating_sub(34), 1),
    );

    let mut x = inner.x.saturating_add(inner.width.saturating_sub(32));
    for (label, style) in [
        ("[Read all]", theme.accent_primary),
        ("[Archive read]", theme.fg_dim),
        ("[Close]", theme.accent_secondary),
    ] {
        let width = label.chars().count() as u16;
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(label.to_string(), style)))
                .alignment(Alignment::Left),
            Rect::new(x, inner.y, width, 1),
        );
        x = x.saturating_add(width + 1);
    }

    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            "Enter expands and marks read  a archives  x deletes",
            theme.fg_dim,
        ))),
        Rect::new(inner.x, inner.y + 1, inner.width, 1),
    );
}

fn header_hit_test(inner: Rect, position: Position) -> Option<NotificationsHitTarget> {
    let mut x = inner.x.saturating_add(inner.width.saturating_sub(32));
    for (label, target) in [
        ("[Read all]", NotificationsHitTarget::MarkAllRead),
        ("[Archive read]", NotificationsHitTarget::ArchiveRead),
        ("[Close]", NotificationsHitTarget::Close),
    ] {
        let width = label.chars().count() as u16;
        if position.x >= x && position.x < x.saturating_add(width) {
            return Some(target);
        }
        x = x.saturating_add(width + 1);
    }
    None
}

fn render_row(
    frame: &mut Frame,
    list_area: Rect,
    layout: &RowLayout,
    notification: &amux_protocol::InboxNotification,
    selected: bool,
    expanded: bool,
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

    let action_labels = action_labels(notification, expanded);
    let mut x = list_area.x;
    for (label, style) in action_labels {
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(label.clone(), style.patch(bg)))),
            Rect::new(x, layout.action_y, label.chars().count() as u16, 1),
        );
        x = x.saturating_add(label.chars().count() as u16 + 1);
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

fn action_labels(
    notification: &amux_protocol::InboxNotification,
    expanded: bool,
) -> Vec<(String, Style)> {
    let mut labels = vec![
        (
            if expanded { "[Collapse]" } else { "[Expand]" }.to_string(),
            Style::default().fg(Color::Indexed(81)),
        ),
        (
            "[Read]".to_string(),
            Style::default().fg(Color::Indexed(114)),
        ),
        (
            "[Archive]".to_string(),
            Style::default().fg(Color::Indexed(245)),
        ),
        (
            "[Delete]".to_string(),
            Style::default().fg(Color::Indexed(203)),
        ),
    ];
    labels.extend(notification.actions.iter().map(|action| {
        (
            format!("[{}]", action.label),
            Style::default().fg(Color::Indexed(39)),
        )
    }));
    labels
}

fn action_hit_labels(
    notification: &amux_protocol::InboxNotification,
    expanded: bool,
) -> Vec<String> {
    action_labels(notification, expanded)
        .into_iter()
        .map(|(label, _)| label)
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
}
