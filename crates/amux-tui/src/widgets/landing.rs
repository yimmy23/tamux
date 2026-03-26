use ratatui::prelude::*;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::theme::ThemeTokens;

pub fn render(frame: &mut Frame, area: Rect, theme: &ThemeTokens) {
    let height = area.height as usize;
    let mut lines: Vec<Line<'static>> = Vec::new();

    let pad_top = height.saturating_div(4);
    for _ in 0..pad_top {
        lines.push(Line::raw(""));
    }

    lines.push(Line::from(vec![
        Span::styled("\u{2591}", Style::default().fg(Color::Indexed(24))),
        Span::styled("\u{2592}", Style::default().fg(Color::Indexed(31))),
        Span::styled("\u{2593}", Style::default().fg(Color::Indexed(38))),
        Span::styled("\u{2588}", Style::default().fg(Color::Indexed(75))),
        Span::styled(" T A M U X ", theme.accent_primary),
        Span::styled("\u{2588}", Style::default().fg(Color::Indexed(75))),
        Span::styled("\u{2593}", Style::default().fg(Color::Indexed(38))),
        Span::styled("\u{2592}", Style::default().fg(Color::Indexed(31))),
        Span::styled("\u{2591}", Style::default().fg(Color::Indexed(24))),
    ]));
    lines.push(Line::from(Span::styled(
        "think \u{00b7} plan \u{00b7} ship",
        theme.fg_dim,
    )));
    lines.push(Line::raw(""));
    lines.push(Line::from(Span::styled(
        "Clean thread. No concierge noise. Type to begin.",
        theme.fg_dim,
    )));
    lines.push(Line::raw(""));
    lines.push(Line::from(vec![
        Span::styled("[ Think ]", theme.accent_primary),
        Span::raw("  "),
        Span::styled("[ Plan ]", theme.accent_secondary),
        Span::raw("  "),
        Span::styled("[ Ship ]", theme.fg_active),
    ]));
    lines.push(Line::raw(""));
    lines.push(Line::from(vec![
        Span::styled("Ctrl+P", theme.accent_primary),
        Span::styled(" command palette  ", theme.fg_dim),
        Span::styled("Ctrl+T", theme.accent_primary),
        Span::styled(" threads", theme.fg_dim),
    ]));

    while lines.len() < height {
        lines.push(Line::raw(""));
    }
    lines.truncate(height);

    frame.render_widget(Paragraph::new(lines).alignment(Alignment::Center), area);
}
