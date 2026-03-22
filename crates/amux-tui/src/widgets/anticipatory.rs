use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::prelude::*;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Padding, Paragraph, Wrap};

use crate::state::AnticipatoryState;
use crate::theme::{ThemeTokens, ROUNDED_BORDER};

pub fn render(
    frame: &mut Frame,
    area: Rect,
    anticipatory: &AnticipatoryState,
    theme: &ThemeTokens,
) {
    if area.height < 3 || !anticipatory.has_items() {
        return;
    }

    let block = Block::default()
        .title(Span::styled(" Anticipatory ", theme.accent_secondary))
        .borders(Borders::ALL)
        .border_type(ROUNDED_BORDER)
        .border_style(theme.accent_secondary)
        .padding(Padding::new(1, 1, 0, 0));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let visible = anticipatory.items().iter().take(2).collect::<Vec<_>>();
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints(
            visible
                .iter()
                .map(|_| Constraint::Length(3))
                .collect::<Vec<_>>(),
        )
        .split(inner);

    for (index, item) in visible.into_iter().enumerate() {
        let mut lines = vec![
            Line::from(vec![
                Span::styled(item.title.as_str(), theme.fg_active),
                Span::styled(
                    format!("  {:.0}%", item.confidence * 100.0),
                    confidence_style(item.confidence, theme),
                ),
            ]),
            Line::from(Span::styled(item.summary.as_str(), theme.fg_dim)),
        ];
        if let Some(first_bullet) = item.bullets.first() {
            lines.push(Line::from(vec![
                Span::styled("• ", theme.accent_primary),
                Span::styled(first_bullet.as_str(), theme.fg_dim),
            ]));
        }

        let paragraph = Paragraph::new(lines).wrap(Wrap { trim: true });
        frame.render_widget(paragraph, rows[index]);
    }
}

fn confidence_style(confidence: f64, theme: &ThemeTokens) -> Style {
    if confidence >= 0.9 {
        theme.accent_success
    } else if confidence >= 0.75 {
        theme.accent_secondary
    } else {
        theme.fg_dim
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn confidence_style_prefers_success_for_high_confidence() {
        let theme = ThemeTokens::default();
        assert_eq!(confidence_style(0.95, &theme), theme.accent_success);
    }
}
