use ratatui::prelude::*;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, List, ListItem, Paragraph};

use crate::state::modal::ModalState;
use crate::state::task::TaskState;
use crate::theme::ThemeTokens;
use crate::widgets::thread_picker::visible_window;

pub fn render(
    frame: &mut Frame,
    area: Rect,
    tasks: &TaskState,
    modal: &ModalState,
    theme: &ThemeTokens,
) {
    let block = Block::default()
        .title(" GOALS ")
        .borders(Borders::ALL)
        .border_type(BorderType::Double)
        .border_style(theme.accent_secondary);

    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.height < 4 {
        return;
    }

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Min(1),
            Constraint::Length(1),
        ])
        .split(inner);

    let query = modal.command_query();
    let input_line = Line::from(vec![
        Span::raw(" "),
        Span::styled(
            if query.is_empty() {
                "Search goals..."
            } else {
                query
            },
            theme.fg_active,
        ),
        if query.is_empty() {
            Span::raw("")
        } else {
            Span::raw("\u{2588}")
        },
    ]);
    frame.render_widget(Paragraph::new(input_line), chunks[0]);

    let sep = Line::from(Span::styled(
        "\u{2500}".repeat(chunks[1].width as usize),
        theme.fg_dim,
    ));
    frame.render_widget(Paragraph::new(sep), chunks[1]);

    let search = query.to_lowercase();
    let filtered_runs: Vec<_> = tasks
        .goal_runs()
        .iter()
        .filter(|run| {
            search.is_empty()
                || run.title.to_lowercase().contains(&search)
                || run.goal.to_lowercase().contains(&search)
        })
        .collect();

    let cursor = modal.picker_cursor();
    let list_h = chunks[2].height as usize;
    let inner_w = inner.width as usize;
    let total_items = filtered_runs.len() + 1;
    let (visible_start, visible_len) = visible_window(cursor, total_items, list_h);

    let list_items: Vec<ListItem> = (0..list_h)
        .map(|i| {
            if i >= visible_len {
                return ListItem::new(Line::raw(""));
            }

            let item_index = visible_start + i;
            let is_selected = item_index == cursor;

            if item_index == 0 {
                let line = Line::from(vec![
                    Span::raw(if is_selected { " > " } else { "   " }),
                    Span::styled("+ New goal run", theme.accent_primary),
                ]);
                return if is_selected {
                    ListItem::new(line)
                        .style(Style::default().bg(Color::Indexed(178)).fg(Color::Black))
                } else {
                    ListItem::new(line)
                };
            }

            let run = filtered_runs[item_index - 1];
            let title_max = inner_w.saturating_sub(20);
            let title = if run.title.len() > title_max && title_max > 3 {
                format!("{}...", &run.title[..title_max - 3])
            } else {
                run.title.clone()
            };
            let status = format!(
                "{:?}",
                run.status
                    .unwrap_or(crate::state::task::GoalRunStatus::Queued)
            )
            .to_ascii_lowercase();

            if is_selected {
                ListItem::new(Line::from(vec![
                    Span::raw(" > "),
                    Span::raw(title),
                    Span::raw("  "),
                    Span::raw(status),
                ]))
                .style(Style::default().bg(Color::Indexed(178)).fg(Color::Black))
            } else {
                ListItem::new(Line::from(vec![
                    Span::raw("   "),
                    Span::styled(title, theme.fg_active),
                    Span::raw("  "),
                    Span::styled(status, theme.fg_dim),
                ]))
            }
        })
        .collect();

    frame.render_widget(List::new(list_items), chunks[2]);

    let mut hints = vec![
        Span::raw(" "),
        Span::styled("↑↓", theme.fg_active),
        Span::styled(" navigate  ", theme.fg_dim),
        Span::styled("Enter", theme.fg_active),
        Span::styled(" open/create  ", theme.fg_dim),
    ];
    if cursor > 0 {
        hints.push(Span::styled("Del", theme.fg_active));
        hints.push(Span::styled(" delete  ", theme.fg_dim));
        hints.push(Span::styled("Ctrl+S", theme.fg_active));
        hints.push(Span::styled(" pause/resume  ", theme.fg_dim));
    }
    hints.push(Span::styled("Shift+R", theme.fg_active));
    hints.push(Span::styled(" refresh  ", theme.fg_dim));
    hints.push(Span::styled("Esc", theme.fg_active));
    hints.push(Span::styled(" close", theme.fg_dim));
    let hints = Line::from(hints);
    frame.render_widget(Paragraph::new(hints), chunks[3]);
}
