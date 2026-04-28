use ratatui::prelude::*;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, List, ListItem, Paragraph};

use crate::state::modal::ModalState;
use crate::theme::ThemeTokens;

pub fn render(
    frame: &mut Frame,
    area: Rect,
    modal: &ModalState,
    current_role: &str,
    theme: &ThemeTokens,
) {
    let block = Block::default()
        .title(" ROLE ")
        .borders(Borders::ALL)
        .border_type(BorderType::Double)
        .border_style(theme.accent_secondary);

    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.height < 3 {
        return;
    }

    let current_role = current_role.trim();
    let custom_index = crate::state::subagents::role_picker_custom_index();
    let cursor = modal.picker_cursor();
    let current_is_custom = !current_role.is_empty()
        && crate::state::subagents::role_picker_index_for_id(current_role).is_none();

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(inner);

    let list_h = chunks[0].height as usize;
    let total_rows = crate::state::subagents::role_picker_item_count();
    let window_start = cursor
        .saturating_sub(list_h.saturating_sub(1))
        .min(total_rows.saturating_sub(list_h));
    let window_end = (window_start + list_h).min(total_rows);

    let items: Vec<ListItem> = (window_start..window_end)
        .map(|absolute_index| {
            if absolute_index == custom_index {
                let is_selected = absolute_index == cursor;
                let label = if current_is_custom {
                    format!("Custom... ({current_role})")
                } else {
                    "Custom...".to_string()
                };
                return if is_selected {
                    ListItem::new(Line::from(vec![Span::raw(" > "), Span::raw(label)]))
                        .style(Style::default().bg(Color::Indexed(178)).fg(Color::Black))
                } else {
                    let marker = if current_is_custom { "• " } else { "  " };
                    ListItem::new(Line::from(vec![
                        Span::raw("   "),
                        Span::styled(marker, theme.accent_secondary),
                        Span::styled(label, theme.accent_secondary),
                    ]))
                };
            }

            let Some(choice) = crate::state::subagents::role_picker_choice(absolute_index) else {
                return ListItem::new(Line::from(""));
            };
            let is_selected = absolute_index == cursor;
            let is_current = choice.id.eq_ignore_ascii_case(current_role);

            if is_selected {
                ListItem::new(Line::from(vec![
                    Span::raw(" > "),
                    Span::raw(choice.label),
                    Span::styled(format!(" ({})", choice.id), Style::default()),
                ]))
                .style(Style::default().bg(Color::Indexed(178)).fg(Color::Black))
            } else {
                let marker = if is_current { "• " } else { "  " };
                let style = if is_current {
                    theme.accent_secondary
                } else {
                    theme.fg_active
                };
                ListItem::new(Line::from(vec![
                    Span::raw("   "),
                    Span::styled(marker, style),
                    Span::styled(choice.label, style),
                    Span::styled(format!(" ({})", choice.id), theme.fg_dim),
                ]))
            }
        })
        .collect();

    frame.render_widget(List::new(items), chunks[0]);

    let hints = Line::from(vec![
        Span::styled("↑↓", theme.fg_active),
        Span::styled(" nav  ", theme.fg_dim),
        Span::styled("Enter", theme.fg_active),
        Span::styled(" sel  ", theme.fg_dim),
        Span::styled("Esc", theme.fg_active),
        Span::styled(" close", theme.fg_dim),
    ]);
    frame.render_widget(Paragraph::new(hints), chunks[1]);
}
