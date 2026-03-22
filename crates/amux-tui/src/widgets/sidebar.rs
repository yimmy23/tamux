use ratatui::prelude::*;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::state::sidebar::{SidebarState, SidebarTab};
use crate::state::task::TaskState;
use crate::theme::ThemeTokens;

const TAB_LABELS: [&str; 2] = ["Files", "Todos"];

#[derive(Debug, Clone)]
struct SidebarRow {
    line: Line<'static>,
    file_path: Option<String>,
}

pub enum SidebarHitTarget {
    Tab(SidebarTab),
    File(String),
    Todo(usize),
}

fn tab_hit_test(tab_area: Rect, mouse_x: u16) -> Option<SidebarTab> {
    let cells = tab_cells(tab_area);
    if mouse_x >= cells[0].x && mouse_x < cells[0].x.saturating_add(cells[0].width) {
        Some(SidebarTab::Files)
    } else if mouse_x >= cells[1].x && mouse_x < cells[1].x.saturating_add(cells[1].width) {
        Some(SidebarTab::Todos)
    } else {
        None
    }
}

fn tab_cells(tab_area: Rect) -> [Rect; 2] {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(tab_area);
    [chunks[0], chunks[1]]
}

fn tab_label(tab: SidebarTab) -> &'static str {
    match tab {
        SidebarTab::Files => " Files ",
        SidebarTab::Todos => " Todos ",
    }
}

fn tab_hint_line(theme: &ThemeTokens) -> Line<'static> {
    Line::from(vec![
        Span::styled("[", theme.accent_primary),
        Span::styled(" files ", theme.fg_dim),
        Span::styled("]", theme.accent_primary),
        Span::styled("  ", theme.fg_dim),
        Span::styled("[", theme.accent_primary),
        Span::styled(" todos ", theme.fg_dim),
        Span::styled("]", theme.accent_primary),
        Span::styled("  click tab", theme.fg_dim),
    ])
}

fn rows_for_thread(
    tasks: &TaskState,
    sidebar: &SidebarState,
    thread_id: Option<&str>,
    theme: &ThemeTokens,
    width: usize,
) -> Vec<SidebarRow> {
    let Some(thread_id) = thread_id else {
        return vec![SidebarRow {
            line: Line::from(Span::styled(" No thread selected", theme.fg_dim)),
            file_path: None,
        }];
    };

    let selected = sidebar.selected_item();
    let selected_style = Style::default().bg(Color::Indexed(236));

    match sidebar.active_tab() {
        SidebarTab::Files => {
            let Some(context) = tasks.work_context_for_thread(thread_id) else {
                return vec![SidebarRow {
                    line: Line::from(Span::styled(" No files", theme.fg_dim)),
                    file_path: None,
                }];
            };
            if context.entries.is_empty() {
                return vec![SidebarRow {
                    line: Line::from(Span::styled(" No files", theme.fg_dim)),
                    file_path: None,
                }];
            }

            context
                .entries
                .iter()
                .enumerate()
                .map(|(idx, entry)| {
                    let label = entry.change_kind.as_deref().unwrap_or_else(|| {
                        entry
                            .kind
                            .map(|kind| match kind {
                                crate::state::task::WorkContextEntryKind::RepoChange => "diff",
                                crate::state::task::WorkContextEntryKind::Artifact => "file",
                                crate::state::task::WorkContextEntryKind::GeneratedSkill => "skill",
                            })
                            .unwrap_or("file")
                    });
                    let mut path = entry.path.clone();
                    let max_len = width.saturating_sub(12).max(8);
                    if path.chars().count() > max_len {
                        let tail: String = path
                            .chars()
                            .rev()
                            .take(max_len.saturating_sub(1))
                            .collect::<Vec<_>>()
                            .into_iter()
                            .rev()
                            .collect();
                        path = format!("…{tail}");
                    }

                    let line = Line::from(vec![
                        Span::styled(
                            if idx == selected { "> " } else { "  " },
                            theme.accent_primary,
                        ),
                        Span::styled(format!("[{}]", label), theme.fg_dim),
                        Span::raw(" "),
                        Span::styled(path, theme.fg_active),
                    ]);

                    SidebarRow {
                        line: if idx == selected {
                            line.style(selected_style)
                        } else {
                            line
                        },
                        file_path: Some(entry.path.clone()),
                    }
                })
                .collect()
        }
        SidebarTab::Todos => {
            let todos = tasks.todos_for_thread(thread_id);
            if todos.is_empty() {
                return vec![SidebarRow {
                    line: Line::from(Span::styled(" No todos", theme.fg_dim)),
                    file_path: None,
                }];
            }

            todos
                .iter()
                .enumerate()
                .map(|(idx, todo)| {
                    let marker = match todo.status {
                        Some(crate::state::task::TodoStatus::Completed) => "[x]",
                        Some(crate::state::task::TodoStatus::InProgress) => "[~]",
                        Some(crate::state::task::TodoStatus::Blocked) => "[!]",
                        _ => "[ ]",
                    };
                    let mut text = todo.content.clone();
                    let max_len = width.saturating_sub(8).max(8);
                    if text.chars().count() > max_len {
                        text = format!(
                            "{}…",
                            text.chars()
                                .take(max_len.saturating_sub(1))
                                .collect::<String>()
                        );
                    }
                    let line = Line::from(vec![
                        Span::styled(
                            if idx == selected { "> " } else { "  " },
                            theme.accent_primary,
                        ),
                        Span::styled(marker, theme.fg_dim),
                        Span::raw(" "),
                        Span::styled(text, theme.fg_active),
                    ]);
                    SidebarRow {
                        line: if idx == selected {
                            line.style(selected_style)
                        } else {
                            line
                        },
                        file_path: None,
                    }
                })
                .collect()
        }
    }
}

fn resolved_scroll(rows: &[SidebarRow], sidebar: &SidebarState, body_height: usize) -> usize {
    let max_scroll = rows.len().saturating_sub(body_height);
    let mut scroll = sidebar.scroll_offset().min(max_scroll);
    let selected = sidebar.selected_item().min(rows.len().saturating_sub(1));
    if selected < scroll {
        scroll = selected;
    } else if selected >= scroll.saturating_add(body_height) {
        scroll = selected.saturating_add(1).saturating_sub(body_height);
    }
    scroll.min(max_scroll)
}

pub fn render(
    frame: &mut Frame,
    area: Rect,
    sidebar: &SidebarState,
    tasks: &TaskState,
    thread_id: Option<&str>,
    theme: &ThemeTokens,
    _focused: bool,
) {
    if area.height < 3 {
        return;
    }

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Min(1),
        ])
        .split(area);
    for (tab, cell) in [
        (SidebarTab::Files, tab_cells(chunks[0])[0]),
        (SidebarTab::Todos, tab_cells(chunks[0])[1]),
    ] {
        let style = if sidebar.active_tab() == tab {
            theme.fg_active.bg(Color::Indexed(236))
        } else {
            theme.fg_dim
        };
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(tab_label(tab), style)))
                .alignment(Alignment::Center),
            cell,
        );
    }
    frame.render_widget(Paragraph::new(tab_hint_line(theme)), chunks[1]);

    let rows = rows_for_thread(tasks, sidebar, thread_id, theme, chunks[2].width as usize);
    let scroll = resolved_scroll(&rows, sidebar, chunks[2].height as usize);
    let paragraph = Paragraph::new(rows.into_iter().map(|row| row.line).collect::<Vec<_>>())
        .scroll((scroll as u16, 0));
    frame.render_widget(paragraph, chunks[2]);
}

pub fn body_item_count(
    tasks: &TaskState,
    sidebar: &SidebarState,
    thread_id: Option<&str>,
) -> usize {
    match (sidebar.active_tab(), thread_id) {
        (SidebarTab::Files, Some(thread_id)) => tasks
            .work_context_for_thread(thread_id)
            .map(|ctx| ctx.entries.len().max(1))
            .unwrap_or(1),
        (SidebarTab::Todos, Some(thread_id)) => tasks.todos_for_thread(thread_id).len().max(1),
        _ => 1,
    }
}

pub fn hit_test(
    area: Rect,
    sidebar: &SidebarState,
    tasks: &TaskState,
    thread_id: Option<&str>,
    mouse: Position,
) -> Option<SidebarHitTarget> {
    if area.height < 3
        || mouse.x < area.x
        || mouse.x >= area.x.saturating_add(area.width)
        || mouse.y < area.y
        || mouse.y >= area.y.saturating_add(area.height)
    {
        return None;
    }

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Min(1),
        ])
        .split(area);

    if mouse.y == chunks[0].y {
        return tab_hit_test(chunks[0], mouse.x).map(SidebarHitTarget::Tab);
    }
    if mouse.y == chunks[1].y {
        return None;
    }

    let rows = rows_for_thread(
        tasks,
        sidebar,
        thread_id,
        &ThemeTokens::default(),
        chunks[2].width as usize,
    );
    let scroll = resolved_scroll(&rows, sidebar, chunks[2].height as usize);
    let row_idx = scroll + mouse.y.saturating_sub(chunks[2].y) as usize;
    let row = rows.get(row_idx)?;
    if let Some(path) = &row.file_path {
        Some(SidebarHitTarget::File(path.clone()))
    } else {
        Some(SidebarHitTarget::Todo(row_idx))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::sidebar::SidebarState;
    use crate::state::task::TaskState;

    #[test]
    fn sidebar_handles_empty_state() {
        let sidebar = SidebarState::new();
        let tasks = TaskState::new();
        let _theme = ThemeTokens::default();
        assert_eq!(
            sidebar.active_tab(),
            crate::state::sidebar::SidebarTab::Files
        );
        assert_eq!(body_item_count(&tasks, &sidebar, None), 1);
    }

    #[test]
    fn tab_hit_test_uses_rendered_label_positions() {
        let area = Rect::new(10, 3, 30, 1);
        let cells = tab_cells(area);
        assert_eq!(tab_hit_test(area, cells[0].x + 1), Some(SidebarTab::Files));
        assert_eq!(tab_hit_test(area, cells[1].x + 1), Some(SidebarTab::Todos));
        let boundary = cells[1].x;
        assert_eq!(
            tab_hit_test(area, boundary.saturating_sub(1)),
            Some(SidebarTab::Files)
        );
        assert_eq!(
            tab_hit_test(area, boundary.saturating_add(1)),
            Some(SidebarTab::Todos)
        );
    }
}
