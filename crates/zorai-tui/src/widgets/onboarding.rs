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
            "Welcome to ZORAI",
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
        Line::from(Span::styled("ZORAI setup required", theme.fg_active)),
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
    use super::render;
    use crate::state::config::ConfigState;
    use crate::theme::ThemeTokens;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    fn render_to_lines(width: u16, height: u16) -> Vec<String> {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).expect("test terminal");
        let config = ConfigState::new();
        let theme = ThemeTokens::default();
        terminal
            .draw(|frame| render(frame, frame.area(), &config, &theme))
            .expect("onboarding render");
        let buffer = terminal.backend().buffer();
        (0..height)
            .map(|y| {
                (0..width)
                    .filter_map(|x| buffer.cell((x, y)).map(|cell| cell.symbol()))
                    .collect::<String>()
            })
            .collect()
    }

    #[test]
    fn onboarding_full_renders_welcome_card() {
        let rows = render_to_lines(80, 20);
        let body = rows.join("\n");
        assert!(
            body.contains("Welcome to ZORAI"),
            "expected welcome card title, got:\n{body}"
        );
    }

    #[test]
    fn onboarding_compact_renders_setup_prompt() {
        // Below 40x10 the compact branch is taken.
        let rows = render_to_lines(30, 6);
        let body = rows.join("\n");
        assert!(
            body.contains("ZORAI setup required"),
            "expected compact title, got:\n{body}"
        );
    }
}
