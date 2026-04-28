#![allow(dead_code)]

use ratatui::prelude::*;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph, Wrap};

use crate::theme::ThemeTokens;

pub fn render(
    frame: &mut Frame,
    area: Rect,
    content: &str,
    options: &[String],
    selected_index: usize,
    theme: &ThemeTokens,
) {
    let block = Block::default()
        .title(" QUESTION ")
        .borders(Borders::ALL)
        .border_type(BorderType::Double)
        .border_style(theme.accent_secondary);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let mut lines = vec![
        Line::raw(""),
        Line::from(vec![
            Span::raw("  "),
            Span::styled(content, theme.fg_active),
        ]),
        Line::raw(""),
        Line::from(vec![
            Span::raw("  "),
            Span::styled("Choose one:", theme.fg_dim),
        ]),
        Line::raw(""),
    ];

    let mut option_spans = vec![Span::raw("  ")];
    for (index, option) in options.iter().enumerate() {
        if index > 0 {
            option_spans.push(Span::raw("  "));
        }
        let style = if index == selected_index {
            theme.accent_secondary
        } else {
            theme.fg_active
        };
        option_spans.push(Span::styled(format!("[{}]", option), style));
    }
    lines.push(Line::from(option_spans));
    lines.push(Line::raw(""));
    lines.push(Line::from(vec![
        Span::raw("  "),
        Span::styled(
            "Left/Right to choose • Enter to submit • matching key selects directly",
            theme.fg_dim,
        ),
    ]));

    let paragraph = Paragraph::new(lines).wrap(Wrap { trim: false });
    frame.render_widget(paragraph, inner);
}
