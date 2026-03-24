use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::prelude::*;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Padding, Paragraph, Wrap};

use crate::state::config::ConfigState;
use crate::theme::{ThemeTokens, ROUNDED_BORDER};

pub fn render(frame: &mut Frame, area: Rect, config: &ConfigState, theme: &ThemeTokens) {
    if area.width < 40 || area.height < 10 {
        render_compact(frame, area, theme);
        return;
    }

    let card_height = area.height.min(16);
    let card_width = area.width.min(74);
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Fill(1),
            Constraint::Length(card_height),
            Constraint::Fill(1),
        ])
        .split(area);
    let horizontal = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Fill(1),
            Constraint::Length(card_width),
            Constraint::Fill(1),
        ])
        .split(vertical[1]);
    let card = horizontal[1];

    let block = Block::default()
        .title(Span::styled(" First Run ", theme.accent_primary))
        .borders(Borders::ALL)
        .border_type(ROUNDED_BORDER)
        .border_style(theme.accent_primary)
        .padding(Padding::new(2, 2, 1, 1));
    let inner = block.inner(card);
    frame.render_widget(block, card);

    let lines = vec![
        Line::from(Span::styled(
            "Welcome to TAMUX",
            theme.fg_active.add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(
            "No provider is configured yet, so there is nothing to send prompts to.",
            theme.fg_dim,
        )),
        Line::raw(""),
        Line::from(vec![
            Span::styled("Default setup target: ", theme.fg_dim),
            Span::styled(
                format!("{} / {}", config.provider, config.model),
                theme.accent_secondary,
            ),
        ]),
        Line::raw(""),
        Line::from(vec![
            Span::styled("1. ", theme.accent_primary),
            Span::styled("Open ", theme.fg_dim),
            Span::styled("Settings -> Provider", theme.fg_active),
        ]),
        Line::from(vec![
            Span::styled("2. ", theme.accent_primary),
            Span::styled("Choose a provider, auth mode, and model", theme.fg_dim),
        ]),
        Line::from(vec![
            Span::styled("3. ", theme.accent_primary),
            Span::styled("Add an API key or sign in, then save", theme.fg_dim),
        ]),
        Line::raw(""),
        Line::from(vec![
            Span::styled("Enter", theme.accent_success),
            Span::styled(" open provider setup", theme.fg_dim),
        ]),
        Line::from(vec![
            Span::styled("Ctrl+P", theme.accent_primary),
            Span::styled(" command palette", theme.fg_dim),
            Span::raw("   "),
            Span::styled("/settings", theme.accent_secondary),
            Span::styled(" from the input box", theme.fg_dim),
        ]),
    ];

    let paragraph = Paragraph::new(lines)
        .style(Style::default().fg(Color::White))
        .alignment(Alignment::Left)
        .wrap(Wrap { trim: true });
    frame.render_widget(paragraph, inner);
}

fn render_compact(frame: &mut Frame, area: Rect, theme: &ThemeTokens) {
    let lines = vec![
        Line::from(Span::styled("TAMUX setup required", theme.fg_active)),
        Line::from(Span::styled(
            "Configure a provider before sending prompts.",
            theme.fg_dim,
        )),
        Line::from(vec![
            Span::styled("Enter", theme.accent_success),
            Span::styled(" open setup", theme.fg_dim),
        ]),
    ];
    let paragraph = Paragraph::new(lines)
        .alignment(Alignment::Center)
        .wrap(Wrap { trim: true });
    frame.render_widget(paragraph, area);
}

#[cfg(test)]
mod tests {
    #[test]
    fn onboarding_module_exists() {
        assert!(true);
    }
}
