use ratatui::prelude::*;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Tabs};

use crate::state::sidebar::{SidebarState, SidebarTab};
use crate::state::task::TaskState;
use crate::theme::ThemeTokens;

const TAB_LABELS: [&str; 2] = ["Files", "Todos"];
const TAB_DIVIDER: &str = " | ";

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
    let tabs = [SidebarTab::Files, SidebarTab::Todos];
    let divider_width = TAB_DIVIDER.chars().count() as u16;
    let mut x = tab_area.x;

    for (idx, label) in TAB_LABELS.iter().enumerate() {
        let label_width = label.chars().count() as u16;
        if mouse_x >= x && mouse_x < x.saturating_add(label_width) {
            return tabs.get(idx).copied();
        }
        x = x.saturating_add(label_width);
        if idx + 1 < TAB_LABELS.len() {
            x = x.saturating_add(divider_width);
        }
    }

    None
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
                        entry.kind
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
                        Span::styled(if idx == selected { "> " } else { "  " }, theme.accent_primary),
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

            todos.iter()
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
                            text.chars().take(max_len.saturating_sub(1)).collect::<String>()
                        );
                    }
                    let line = Line::from(vec![
                        Span::styled(if idx == selected { "> " } else { "  " }, theme.accent_primary),
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
    if area.height < 2 {
        return;
    }

    let tabs = Tabs::new(TAB_LABELS)
        .select(match sidebar.active_tab() {
            SidebarTab::Files => 0,
            SidebarTab::Todos => 1,
        })
        .style(theme.fg_dim)
        .highlight_style(theme.fg_active)
        .divider(Span::styled(TAB_DIVIDER, theme.fg_dim));

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(1)])
        .split(area);
    frame.render_widget(tabs, chunks[0]);

    let rows = rows_for_thread(tasks, sidebar, thread_id, theme, chunks[1].width as usize);
    let scroll = resolved_scroll(&rows, sidebar, chunks[1].height as usize);
    let paragraph = Paragraph::new(rows.into_iter().map(|row| row.line).collect::<Vec<_>>())
        .scroll((scroll as u16, 0));
    frame.render_widget(paragraph, chunks[1]);
}

pub fn body_item_count(tasks: &TaskState, sidebar: &SidebarState, thread_id: Option<&str>) -> usize {
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
    if area.height < 2
        || mouse.x < area.x
        || mouse.x >= area.x.saturating_add(area.width)
        || mouse.y < area.y
        || mouse.y >= area.y.saturating_add(area.height)
    {
        return None;
    }

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(1)])
        .split(area);

    if mouse.y == chunks[0].y {
        return tab_hit_test(chunks[0], mouse.x).map(SidebarHitTarget::Tab);
    }

    let rows = rows_for_thread(tasks, sidebar, thread_id, &ThemeTokens::default(), chunks[1].width as usize);
    let scroll = resolved_scroll(&rows, sidebar, chunks[1].height as usize);
    let row_idx = scroll + mouse.y.saturating_sub(chunks[1].y) as usize;
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
        assert_eq!(sidebar.active_tab(), crate::state::sidebar::SidebarTab::Files);
        assert_eq!(body_item_count(&tasks, &sidebar, None), 1);
    }

    #[test]
    fn tab_hit_test_uses_rendered_label_positions() {
        let area = Rect::new(10, 3, 30, 1);
        assert_eq!(tab_hit_test(area, 10), Some(SidebarTab::Files));
        assert_eq!(tab_hit_test(area, 18), Some(SidebarTab::Todos));
        assert_eq!(tab_hit_test(area, 15), None);
    }
}
