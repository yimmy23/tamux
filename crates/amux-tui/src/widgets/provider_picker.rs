use ratatui::prelude::*;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, List, ListItem, Paragraph};

use crate::providers::PROVIDERS;
use crate::state::config::ConfigState;
use crate::state::modal::ModalState;
use crate::theme::ThemeTokens;

pub fn render(
    frame: &mut Frame,
    area: Rect,
    modal: &ModalState,
    config: &ConfigState,
    theme: &ThemeTokens,
) {
    let block = Block::default()
        .title(" PROVIDER ")
        .borders(Borders::ALL)
        .border_type(BorderType::Double)
        .border_style(theme.accent_secondary);

    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.height < 3 {
        return;
    }

    // Split: list (flex) + hints (1)
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(inner);

    let cursor = modal.picker_cursor();
    let active_provider = config.provider();

    let list_items: Vec<ListItem> = PROVIDERS
        .iter()
        .enumerate()
        .map(|(i, provider_def)| {
            let is_selected = i == cursor;
            let is_active = provider_def.id == active_provider
                || provider_def.name.eq_ignore_ascii_case(active_provider);

            if is_selected {
                ListItem::new(Line::from(vec![
                    Span::raw(" > "),
                    Span::raw(provider_def.name),
                ]))
                .style(Style::default().bg(Color::Indexed(178)).fg(Color::Black))
            } else if is_active && !active_provider.is_empty() {
                ListItem::new(Line::from(vec![
                    Span::raw("  "),
                    Span::styled(
                        format!("\u{2022} {}", provider_def.name),
                        theme.accent_secondary,
                    ),
                ]))
            } else {
                ListItem::new(Line::from(vec![
                    Span::raw("   "),
                    Span::styled(provider_def.name, theme.fg_active),
                ]))
            }
        })
        .collect();

    let list = List::new(list_items);
    frame.render_widget(list, chunks[0]);

    // Hints
    let hints = Line::from(vec![
        Span::raw(" "),
        Span::styled("↑↓", theme.fg_active),
        Span::styled(" nav  ", theme.fg_dim),
        Span::styled("Enter", theme.fg_active),
        Span::styled(" sel  ", theme.fg_dim),
        Span::styled("Esc", theme.fg_active),
        Span::styled(" close", theme.fg_dim),
    ]);
    frame.render_widget(Paragraph::new(hints), chunks[1]);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_list_has_20_entries() {
        assert_eq!(PROVIDERS.len(), 20);
    }

    #[test]
    fn provider_picker_handles_empty_state() {
        let modal = ModalState::new();
        let config = ConfigState::new();
        let _theme = ThemeTokens::default();
        assert_eq!(modal.picker_cursor(), 0);
        assert_eq!(config.provider(), "openai");
    }
}
