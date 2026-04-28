use ratatui::prelude::*;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, List, ListItem, Paragraph};

use crate::state::modal::ModalState;
use crate::state::workspace::WorkspaceState;
use crate::theme::ThemeTokens;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkspacePickerHitTarget {
    Item(usize),
}

fn picker_layout(inner: Rect) -> [Rect; 5] {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Min(1),
            Constraint::Length(1),
        ])
        .split(inner);
    [chunks[0], chunks[1], chunks[2], chunks[3], chunks[4]]
}

pub fn render(
    frame: &mut Frame,
    area: Rect,
    workspace: &WorkspaceState,
    modal: &ModalState,
    theme: &ThemeTokens,
) {
    let block = Block::default()
        .title(" WORKSPACES ")
        .borders(Borders::ALL)
        .border_type(BorderType::Double)
        .border_style(theme.accent_secondary);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.height < 5 {
        return;
    }

    let [tabs_row, search_row, separator_row, list_row, hints_row] = picker_layout(inner);
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            "[Workspaces]",
            theme.accent_primary,
        ))),
        tabs_row,
    );

    let query = modal.command_query();
    let input_line = Line::from(vec![
        Span::raw(" "),
        Span::styled(
            if query.is_empty() {
                "Search workspaces..."
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
    frame.render_widget(Paragraph::new(input_line), search_row);

    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            "\u{2500}".repeat(separator_row.width as usize),
            theme.fg_dim,
        ))),
        separator_row,
    );

    let items = workspace.workspace_picker_items(query);
    let cursor = modal.picker_cursor();
    let list_h = list_row.height as usize;
    let (visible_start, visible_len) =
        crate::widgets::thread_picker::visible_window(cursor, items.len(), list_h);
    let current = workspace.workspace_id();
    let list_items = (0..list_h)
        .map(|row| {
            if row >= visible_len {
                return ListItem::new(Line::raw(""));
            }
            let index = visible_start + row;
            let Some(settings) = items.get(index) else {
                return ListItem::new(Line::raw(""));
            };
            let is_selected = cursor == index;
            let is_current = settings.workspace_id == current;
            let dot_style = if is_current {
                theme.accent_success
            } else {
                theme.fg_dim
            };
            let root = settings
                .workspace_root
                .as_deref()
                .map(|root| format!("  {root}"))
                .unwrap_or_default();
            let line = if is_selected {
                Line::from(vec![
                    Span::styled("\u{25cf}", dot_style),
                    Span::raw(" "),
                    Span::raw(settings.workspace_id.clone()),
                    Span::raw("  "),
                    Span::raw(format!("{:?}", settings.operator)),
                    Span::raw(root),
                ])
            } else {
                Line::from(vec![
                    Span::raw("  "),
                    Span::styled("\u{25cf}", dot_style),
                    Span::raw(" "),
                    Span::styled(settings.workspace_id.clone(), theme.fg_active),
                    Span::raw("  "),
                    Span::styled(format!("{:?}", settings.operator), theme.fg_dim),
                    Span::styled(root, theme.fg_dim),
                ])
            };
            if is_selected {
                ListItem::new(line).style(Style::default().bg(Color::Indexed(178)).fg(Color::Black))
            } else {
                ListItem::new(line)
            }
        })
        .collect::<Vec<_>>();
    frame.render_widget(List::new(list_items), list_row);

    let hints = Line::from(vec![
        Span::raw(" "),
        Span::styled("\u{2191}\u{2193}", theme.fg_active),
        Span::styled(" navigate  ", theme.fg_dim),
        Span::styled("Enter", theme.fg_active),
        Span::styled(" select  ", theme.fg_dim),
        Span::styled("Shift+R", theme.fg_active),
        Span::styled(" refresh  ", theme.fg_dim),
        Span::styled("Esc", theme.fg_active),
        Span::styled(" close", theme.fg_dim),
    ]);
    frame.render_widget(Paragraph::new(hints), hints_row);
}

pub fn hit_test(
    area: Rect,
    workspace: &WorkspaceState,
    modal: &ModalState,
    mouse: Position,
) -> Option<WorkspacePickerHitTarget> {
    if !area.contains(mouse) {
        return None;
    }

    let inner = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Double)
        .inner(area);
    if inner.height < 5 {
        return None;
    }

    let [_, _, _, list_row, _] = picker_layout(inner);
    if !list_row.contains(mouse) {
        return None;
    }

    let items = workspace.workspace_picker_items(modal.command_query());
    let row_idx = mouse.y.saturating_sub(list_row.y) as usize;
    let (visible_start, visible_len) = crate::widgets::thread_picker::visible_window(
        modal.picker_cursor(),
        items.len(),
        list_row.height as usize,
    );
    if row_idx < visible_len {
        Some(WorkspacePickerHitTarget::Item(visible_start + row_idx))
    } else {
        None
    }
}
