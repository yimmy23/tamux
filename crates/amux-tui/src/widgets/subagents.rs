#![allow(dead_code)]

use ratatui::prelude::*;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::state::sidebar::SidebarItemTarget;
use crate::state::sidebar::SidebarState;
use crate::state::task::{GoalRunStatus, TaskState, TaskStatus};
use crate::theme::ThemeTokens;

#[derive(Clone)]
struct SidebarRow {
    line: Line<'static>,
    target: Option<SidebarItemTarget>,
    selectable_index: Option<usize>,
}

pub fn render(
    frame: &mut Frame,
    area: Rect,
    tasks: &TaskState,
    sidebar: &SidebarState,
    theme: &ThemeTokens,
) {
    let rows = build_rows(tasks, sidebar, theme);
    let scroll = resolved_scroll(&rows, sidebar, area.height as usize);
    let paragraph = Paragraph::new(rows.into_iter().map(|row| row.line).collect::<Vec<_>>())
        .scroll((scroll as u16, 0));
    frame.render_widget(paragraph, area);
}

fn build_rows(tasks: &TaskState, sidebar: &SidebarState, theme: &ThemeTokens) -> Vec<SidebarRow> {
    let mut rows = Vec::new();
    let all_tasks = tasks.tasks();
    let goal_runs = tasks.goal_runs();
    let selected = sidebar.selected_item();
    let mut item_index = 0usize;

    // Goal-run groups
    for run in goal_runs {
        let expanded = sidebar.is_expanded(&run.id);
        let arrow = if expanded { "\u{25be}" } else { "\u{25b8}" };
        let dot = goal_run_dot(run.status, theme);
        let status_span = match run.status {
            Some(GoalRunStatus::Running) => Span::styled("running", theme.accent_secondary),
            Some(GoalRunStatus::Completed) => Span::styled("done", theme.accent_success),
            Some(GoalRunStatus::Failed) => Span::styled("failed", theme.accent_danger),
            _ => Span::styled("idle", theme.fg_dim),
        };

        rows.push(SidebarRow {
            line: if item_index == selected {
                Line::from(vec![
                    Span::styled("> ", theme.accent_primary),
                    Span::styled(arrow, theme.fg_active),
                    Span::raw(" "),
                    Span::raw(run.title.clone()),
                    Span::raw(" "),
                    dot,
                    Span::raw(" "),
                    status_span,
                ])
                .style(theme.fg_active.bg(ratatui::style::Color::Indexed(236)))
            } else {
                Line::from(vec![
                    Span::raw("  "),
                    Span::styled(arrow, theme.fg_active),
                    Span::raw(" "),
                    Span::raw(run.title.clone()),
                    Span::raw(" "),
                    dot,
                    Span::raw(" "),
                    status_span,
                ])
            },
            target: Some(SidebarItemTarget::GoalRun {
                goal_run_id: run.id.clone(),
                step_id: None,
            }),
            selectable_index: Some(item_index),
        });
        item_index += 1;

        if expanded {
            let child_tasks: Vec<_> = all_tasks
                .iter()
                .filter(|t| t.goal_run_id.as_deref() == Some(&run.id))
                .collect();

            if child_tasks.is_empty() {
                rows.push(SidebarRow {
                    line: Line::from(Span::styled("  (no tasks)", theme.fg_dim)),
                    target: None,
                    selectable_index: None,
                });
            } else {
                for task in child_tasks {
                    let dot = status_dot_for_task(task.status, theme);
                    let status_lbl = status_label_for_task(task.status, theme);
                    rows.push(SidebarRow {
                        line: if item_index == selected {
                            Line::from(vec![
                                Span::styled("> ", theme.accent_primary),
                                Span::raw("  "),
                                dot,
                                Span::raw(" "),
                                Span::raw(task.title.clone()),
                                Span::raw(" "),
                                status_lbl,
                            ])
                            .style(theme.fg_active.bg(ratatui::style::Color::Indexed(236)))
                        } else {
                            Line::from(vec![
                                Span::raw("    "),
                                dot,
                                Span::raw(" "),
                                Span::raw(task.title.clone()),
                                Span::raw(" "),
                                status_lbl,
                            ])
                        },
                        target: Some(SidebarItemTarget::Task {
                            task_id: task.id.clone(),
                        }),
                        selectable_index: Some(item_index),
                    });
                    item_index += 1;

                    if let Some(ref session_id) = task.session_id {
                        rows.push(SidebarRow {
                            line: Line::from(vec![
                                Span::raw("    "),
                                Span::styled(
                                    format!("\u{2514} thread: {}", session_id),
                                    theme.fg_dim,
                                ),
                            ]),
                            target: None,
                            selectable_index: None,
                        });
                    }
                }
            }
        }
    }

    // Daemon group (tasks without goal_run_id)
    let daemon_tasks: Vec<_> = all_tasks
        .iter()
        .filter(|t| t.goal_run_id.is_none())
        .collect();

    if !daemon_tasks.is_empty() {
        let expanded = sidebar.is_expanded("__daemon__");
        let arrow = if expanded { "\u{25be}" } else { "\u{25b8}" };
        rows.push(SidebarRow {
            line: Line::from(vec![
                Span::styled(arrow, theme.fg_dim),
                Span::raw(" "),
                Span::styled("daemon", theme.fg_active),
            ]),
            target: None,
            selectable_index: None,
        });

        if expanded {
            for task in daemon_tasks {
                let dot = status_dot_for_task(task.status, theme);
                let status_lbl = status_label_for_task(task.status, theme);
                rows.push(SidebarRow {
                    line: if item_index == selected {
                        Line::from(vec![
                            Span::styled("> ", theme.accent_primary),
                            Span::raw(" "),
                            dot,
                            Span::raw(" "),
                            Span::raw(task.title.clone()),
                            Span::raw(" "),
                            status_lbl,
                        ])
                        .style(theme.fg_active.bg(ratatui::style::Color::Indexed(236)))
                    } else {
                        Line::from(vec![
                            Span::raw("  "),
                            dot,
                            Span::raw(" "),
                            Span::raw(task.title.clone()),
                            Span::raw(" "),
                            status_lbl,
                        ])
                    },
                    target: Some(SidebarItemTarget::Task {
                        task_id: task.id.clone(),
                    }),
                    selectable_index: Some(item_index),
                });
                item_index += 1;
            }
        }
    }

    // Empty state
    if rows.is_empty() {
        rows.push(SidebarRow {
            line: Line::from(Span::styled(" No agents", theme.fg_dim)),
            target: None,
            selectable_index: None,
        });
    }

    // Footer aggregate counts
    if !goal_runs.is_empty() || !all_tasks.is_empty() {
        let running_count = all_tasks
            .iter()
            .filter(|t| t.status == Some(TaskStatus::InProgress))
            .count();
        let total_count = all_tasks.len();
        let group_count = goal_runs.len();

        let footer = Line::from(Span::styled(
            format!(
                " {} groups \u{00b7} {}/{} running",
                group_count, running_count, total_count
            ),
            theme.fg_dim,
        ));
        rows.push(SidebarRow {
            line: footer,
            target: None,
            selectable_index: None,
        });
    }

    rows
}

pub fn row_target_at(
    tasks: &TaskState,
    sidebar: &SidebarState,
    body_height: usize,
    row: usize,
) -> Option<SidebarItemTarget> {
    let rows = build_rows(tasks, sidebar, &ThemeTokens::default());
    let scroll = resolved_scroll(&rows, sidebar, body_height);
    let absolute_row = scroll + row;
    rows.get(absolute_row).and_then(|item| item.target.clone())
}

fn resolved_scroll(rows: &[SidebarRow], sidebar: &SidebarState, body_height: usize) -> usize {
    let max_scroll = rows.len().saturating_sub(body_height);
    let mut scroll = sidebar.scroll_offset().min(max_scroll);
    let Some(selected_row) = rows
        .iter()
        .position(|row| row.selectable_index == Some(sidebar.selected_item()))
    else {
        return scroll;
    };

    if selected_row < scroll {
        scroll = selected_row;
    } else if selected_row >= scroll.saturating_add(body_height) {
        scroll = selected_row.saturating_add(1).saturating_sub(body_height);
    }

    scroll.min(max_scroll)
}

fn goal_run_dot<'a>(status: Option<GoalRunStatus>, theme: &ThemeTokens) -> Span<'a> {
    match status {
        Some(GoalRunStatus::Running) => Span::styled("\u{25cf}", theme.accent_secondary),
        Some(GoalRunStatus::Completed) => Span::styled("\u{25cf}", theme.accent_success),
        Some(GoalRunStatus::Failed) => Span::styled("\u{25cf}", theme.accent_danger),
        _ => Span::styled("\u{25cf}", theme.fg_dim),
    }
}

fn status_dot_for_task<'a>(status: Option<TaskStatus>, theme: &ThemeTokens) -> Span<'a> {
    match status {
        Some(TaskStatus::InProgress) => Span::styled("\u{25cf}", theme.accent_secondary),
        Some(TaskStatus::Completed) => Span::styled("\u{25cf}", theme.accent_success),
        Some(TaskStatus::Failed) | Some(TaskStatus::FailedAnalyzing) => {
            Span::styled("\u{25cf}", theme.accent_danger)
        }
        Some(TaskStatus::Blocked) | Some(TaskStatus::AwaitingApproval) => {
            Span::styled("\u{25cf}", theme.accent_primary)
        }
        _ => Span::styled("\u{25cf}", theme.fg_dim),
    }
}

fn status_label_for_task<'a>(status: Option<TaskStatus>, theme: &ThemeTokens) -> Span<'a> {
    match status {
        Some(TaskStatus::InProgress) => Span::styled("running", theme.accent_secondary),
        Some(TaskStatus::Completed) => Span::styled("done", theme.accent_success),
        Some(TaskStatus::Failed) | Some(TaskStatus::FailedAnalyzing) => {
            Span::styled("failed", theme.accent_danger)
        }
        Some(TaskStatus::Blocked) => Span::styled("blocked", theme.fg_dim),
        Some(TaskStatus::AwaitingApproval) => Span::styled("awaiting", theme.accent_primary),
        Some(TaskStatus::Queued) | None => Span::styled("idle", theme.fg_dim),
        Some(TaskStatus::Cancelled) => Span::styled("cancelled", theme.fg_dim),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::sidebar::SidebarState;
    use crate::state::task::TaskState;

    #[test]
    fn subagents_handles_empty_state() {
        let tasks = TaskState::new();
        let _sidebar = SidebarState::new();
        let _theme = ThemeTokens::default();
        assert!(tasks.goal_runs().is_empty());
        assert!(tasks.tasks().is_empty());
    }
}
