use ratatui::prelude::*;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::state::chat::ChatState;
use crate::state::config::ConfigState;
use crate::theme::ThemeTokens;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeaderHitTarget {
    NotificationBell,
}

pub fn render(
    frame: &mut Frame,
    area: Rect,
    config: &ConfigState,
    chat: &ChatState,
    theme: &ThemeTokens,
    unread_notifications: usize,
    notifications_open: bool,
) {
    let block = Block::default()
        .borders(Borders::BOTTOM)
        .border_style(theme.fg_dim);

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let bell_width = bell_area_width(unread_notifications);
    let sections = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Min(1),
            Constraint::Length(bell_width.min(inner.width)),
        ])
        .split(inner);
    let title_area = sections[0];
    let bell_area = sections[1];

    let model = if config.model.is_empty() {
        "no model"
    } else {
        &config.model
    };

    let (in_tok, out_tok) = if let Some(thread) = chat.active_thread() {
        (thread.total_input_tokens, thread.total_output_tokens)
    } else {
        (0, 0)
    };
    let total_tok = in_tok + out_tok;
    let usage = if total_tok > 0 {
        format!("{:.1}k tok", total_tok as f64 / 1000.0)
    } else {
        "0k tok".to_string()
    };

    let mut spans = vec![
        Span::styled(
            "\u{2591}\u{2592}\u{2593}",
            Style::default().fg(Color::Indexed(24)),
        ),
        Span::styled("TAMUX", theme.accent_primary),
        Span::styled(
            "\u{2593}\u{2592}\u{2591} ",
            Style::default().fg(Color::Indexed(24)),
        ),
    ];

    if !config.provider.is_empty() {
        spans.push(Span::raw(&config.provider));
        spans.push(Span::raw(" "));
    }

    spans.push(Span::styled(model, theme.fg_active));

    if !config.reasoning_effort.is_empty() {
        spans.push(Span::raw(" ["));
        spans.push(Span::styled(
            &config.reasoning_effort,
            theme.accent_secondary,
        ));
        spans.push(Span::raw("]"));
    }

    spans.push(Span::raw("  "));
    spans.push(Span::styled(usage, theme.fg_dim));

    let header_text = Line::from(spans).alignment(Alignment::Center);
    let text_area = if title_area.height >= 2 {
        Rect::new(
            title_area.x,
            title_area.y + title_area.height.saturating_sub(1) / 2,
            title_area.width,
            1,
        )
    } else {
        title_area
    };
    frame.render_widget(
        Paragraph::new(header_text).alignment(Alignment::Center),
        text_area,
    );

    let bell_style = if notifications_open {
        theme.accent_primary
    } else if unread_notifications > 0 {
        theme.accent_secondary
    } else {
        theme.fg_dim
    };
    let bell_text = if unread_notifications > 0 {
        format!("\u{1F514} {}", unread_notifications.min(99))
    } else {
        "\u{1F514}".to_string()
    };
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(bell_text, bell_style))).alignment(Alignment::Right),
        bell_area,
    );
}

pub fn hit_test(
    area: Rect,
    unread_notifications: usize,
    position: Position,
) -> Option<HeaderHitTarget> {
    let inner = Block::default().borders(Borders::BOTTOM).inner(area);
    let bell_width = bell_area_width(unread_notifications).min(inner.width);
    let bell_area = Rect::new(
        inner.x + inner.width.saturating_sub(bell_width),
        inner.y,
        bell_width,
        inner.height,
    );

    if position.x >= bell_area.x
        && position.x < bell_area.x.saturating_add(bell_area.width)
        && position.y >= bell_area.y
        && position.y < bell_area.y.saturating_add(bell_area.height)
    {
        Some(HeaderHitTarget::NotificationBell)
    } else {
        None
    }
}

fn bell_area_width(unread_notifications: usize) -> u16 {
    if unread_notifications > 9 {
        8
    } else {
        6
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bell_hit_test_detects_click_inside_bell_area() {
        let area = Rect::new(0, 0, 80, 3);
        let hit = hit_test(area, 3, Position::new(78, 1));
        assert_eq!(hit, Some(HeaderHitTarget::NotificationBell));
    }
}
