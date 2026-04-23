use ratatui::prelude::*;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, List, ListItem, Paragraph};

use crate::state::modal::ModalState;
use crate::theme::ThemeTokens;

fn visible_window(cursor: usize, item_count: usize, list_height: usize) -> (usize, usize) {
    if item_count == 0 || list_height == 0 {
        return (0, 0);
    }

    let height = list_height.min(item_count);
    let max_start = item_count.saturating_sub(height);
    let start = cursor
        .saturating_sub(height.saturating_sub(1))
        .min(max_start);
    (start, height)
}

pub fn render(frame: &mut Frame, area: Rect, modal: &ModalState, theme: &ThemeTokens) {
    let block = Block::default()
        .title(" COMMANDS ")
        .borders(Borders::ALL)
        .border_type(BorderType::Double)
        .border_style(theme.accent_secondary);

    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.height < 4 {
        return;
    }

    // Split: search input (1) + separator (1) + list (flex) + hints (1)
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // search
            Constraint::Length(1), // separator
            Constraint::Min(1),    // list
            Constraint::Length(1), // hints
        ])
        .split(inner);

    // Search input
    let query = modal.command_display_query();
    let input_line = Line::from(vec![
        Span::raw(" "),
        Span::styled("/", theme.fg_active),
        Span::styled(query, theme.fg_active),
        Span::raw("\u{2588}"),
    ]);
    frame.render_widget(Paragraph::new(input_line), chunks[0]);

    // Separator
    let sep = Line::from(Span::styled(
        "\u{2500}".repeat(chunks[1].width as usize),
        theme.fg_dim,
    ));
    frame.render_widget(Paragraph::new(sep), chunks[1]);

    // Command list
    let filtered = modal.filtered_items();
    let items = modal.command_items();
    let cursor = modal.picker_cursor();
    let list_h = chunks[2].height as usize;
    let (visible_start, visible_len) = visible_window(cursor, filtered.len(), list_h);

    let list_items: Vec<ListItem> = (0..list_h)
        .map(|i| {
            if i < visible_len {
                let absolute_index = visible_start + i;
                let idx = filtered[absolute_index];
                let item = &items[idx];
                let is_selected = absolute_index == cursor;

                if is_selected {
                    ListItem::new(Line::from(vec![
                        Span::raw(" > /"),
                        Span::raw(&item.command),
                        Span::raw("  "),
                        Span::raw(&item.description),
                    ]))
                    .style(Style::default().bg(Color::Indexed(178)).fg(Color::Black))
                } else {
                    ListItem::new(Line::from(vec![
                        Span::raw("   /"),
                        Span::styled(&item.command, theme.fg_active),
                        Span::raw("  "),
                        Span::styled(&item.description, theme.fg_dim),
                    ]))
                }
            } else {
                ListItem::new(Line::raw(""))
            }
        })
        .collect();

    let list = List::new(list_items);
    frame.render_widget(list, chunks[2]);

    // Hints
    let hints = Line::from(vec![
        Span::raw(" "),
        Span::styled("↑↓", theme.fg_active),
        Span::styled(" navigate  ", theme.fg_dim),
        Span::styled("Enter", theme.fg_active),
        Span::styled(" select  ", theme.fg_dim),
        Span::styled("Esc", theme.fg_active),
        Span::styled(" close", theme.fg_dim),
    ]);
    frame.render_widget(Paragraph::new(hints), chunks[3]);
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    #[test]
    fn command_palette_handles_empty_state() {
        let modal = ModalState::new();
        let _theme = ThemeTokens::default();
        assert!(modal.command_query().is_empty());
    }

    #[test]
    fn command_palette_highlights_first_filtered_row_without_rewriting_query() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).expect("test terminal should initialize");
        let mut modal = ModalState::new();
        modal.reduce(crate::state::modal::ModalAction::Push(
            crate::state::modal::ModalKind::CommandPalette,
        ));
        modal.reduce(crate::state::modal::ModalAction::SetQuery("/new".into()));

        terminal
            .draw(|frame| render(frame, frame.area(), &modal, &ThemeTokens::default()))
            .expect("render should not panic");

        let buffer = terminal.backend().buffer();
        let selected_bg = Color::Indexed(178);
        let new_row_has_selection = (0..24)
            .find_map(|y| {
                let row = (0..80)
                    .filter_map(|x| buffer.cell((x, y)).map(|cell| cell.symbol()))
                    .collect::<String>();
                row.contains("/new  New conversation").then(|| {
                    (0..80).any(|x| {
                        buffer
                            .cell((x, y))
                            .map(|cell| cell.bg == selected_bg)
                            .unwrap_or(false)
                    })
                })
            })
            .expect("new command row should be visible");

        assert!(
            new_row_has_selection,
            "expected the first filtered command row to stay highlighted"
        );
        assert_eq!(modal.command_display_query(), "new");
    }

    #[test]
    fn visible_window_scrolls_down_to_keep_cursor_visible() {
        assert_eq!(visible_window(0, 10, 4), (0, 4));
        assert_eq!(visible_window(3, 10, 4), (0, 4));
        assert_eq!(visible_window(4, 10, 4), (1, 4));
        assert_eq!(visible_window(9, 10, 4), (6, 4));
    }

    #[test]
    fn visible_window_rewinds_when_cursor_moves_back_up() {
        assert_eq!(visible_window(6, 10, 4), (3, 4));
        assert_eq!(visible_window(2, 10, 4), (0, 4));
    }
}
