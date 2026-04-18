#![allow(dead_code)]

use ratatui::prelude::*;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::state::sidebar::SidebarItemTarget;
use crate::state::sidebar::SidebarState;
use crate::state::task::{GoalRunStatus, HeartbeatOutcome, TaskState, TaskStatus};
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
    let rows = build_rows(tasks, sidebar, theme, area.width as usize);
    let scroll = resolved_scroll(&rows, sidebar, area.height as usize);
    let paragraph = Paragraph::new(rows.into_iter().map(|row| row.line).collect::<Vec<_>>())
        .scroll((scroll as u16, 0));
    frame.render_widget(paragraph, area);
}

pub fn row_target_at(
    tasks: &TaskState,
    sidebar: &SidebarState,
    body_height: usize,
    row: usize,
) -> Option<SidebarItemTarget> {
    let rows = build_rows(tasks, sidebar, &ThemeTokens::default(), 80);
    let scroll = resolved_scroll(&rows, sidebar, body_height);
    let absolute_row = scroll + row;
    rows.get(absolute_row).and_then(|item| item.target.clone())
}

fn build_rows(
    tasks: &TaskState,
    sidebar: &SidebarState,
    theme: &ThemeTokens,
    width: usize,
) -> Vec<SidebarRow> {
    let mut rows = Vec::new();
    let selected = sidebar.selected_item();
    // item_index tracks selectable items (goal runs, steps, standalone tasks)
    let mut item_index: usize = 0;
    let sel_style = Style::default().bg(Color::Indexed(236));

    // Zone 1: Goal Runs
    let goal_runs = tasks.goal_runs();

    for run in goal_runs {
        let expanded = sidebar.is_expanded(&run.id);
        let arrow = if expanded { "\u{25be}" } else { "\u{25b8}" };
        let dot = goal_run_status_dot(run.status, theme);
        let status_span = goal_run_status_label(run.status, theme);

        let is_selected = item_index == selected;
        let spans = vec![
            if is_selected {
                Span::styled("> ", Style::default().fg(Color::Indexed(178)))
            } else {
                Span::raw("  ")
            },
            Span::styled(arrow.to_string(), theme.fg_active),
            Span::raw(" "),
            Span::raw(run.title.clone()),
            Span::raw(" "),
            dot,
            status_span,
        ];
        let line = if is_selected {
            Line::from(spans).style(sel_style)
        } else {
            Line::from(spans)
        };
        rows.push(SidebarRow {
            line,
            target: Some(SidebarItemTarget::GoalRun {
                goal_run_id: run.id.clone(),
                step_id: None,
            }),
            selectable_index: Some(item_index),
        });
        item_index += 1;

        if expanded {
            let mut steps = run.steps.clone();
            steps.sort_by_key(|s| s.order);

            for step in &steps {
                let chip = step_status_chip(step.status, theme);
                let is_sel = item_index == selected;
                let line = if is_sel {
                    Line::from(vec![
                        Span::styled("> ", Style::default().fg(Color::Indexed(178))),
                        Span::raw("  "),
                        chip,
                        Span::raw(" "),
                        Span::raw(step.title.clone()),
                    ])
                    .style(sel_style)
                } else {
                    Line::from(vec![
                        Span::raw("    "),
                        chip,
                        Span::raw(" "),
                        Span::raw(step.title.clone()),
                    ])
                };
                rows.push(SidebarRow {
                    line,
                    target: Some(SidebarItemTarget::GoalRun {
                        goal_run_id: run.id.clone(),
                        step_id: Some(step.id.clone()),
                    }),
                    selectable_index: Some(item_index),
                });
                item_index += 1;
            }
        }
    }

    // Zone 2: Standalone Tasks
    let standalone: Vec<_> = tasks
        .tasks()
        .iter()
        .filter(|t| t.goal_run_id.is_none())
        .collect();

    if !standalone.is_empty() {
        if !goal_runs.is_empty() {
            rows.push(SidebarRow {
                line: Line::from(Span::styled("\u{2500}".repeat(width.min(40)), theme.fg_dim)),
                target: None,
                selectable_index: None,
            });
        }
        rows.push(SidebarRow {
            line: Line::from(Span::styled("Standalone Tasks".to_string(), theme.fg_dim)),
            target: None,
            selectable_index: None,
        });

        for task in standalone {
            let chip = task_status_chip(task.status, theme);
            let is_sel = item_index == selected;
            let line = if is_sel {
                Line::from(vec![
                    Span::styled("> ", Style::default().fg(Color::Indexed(178))),
                    chip,
                    Span::raw(" "),
                    Span::raw(task.title.clone()),
                ])
                .style(sel_style)
            } else {
                Line::from(vec![
                    Span::raw("  "),
                    chip,
                    Span::raw(" "),
                    Span::raw(task.title.clone()),
                ])
            };
            rows.push(SidebarRow {
                line,
                target: Some(SidebarItemTarget::Task {
                    task_id: task.id.clone(),
                }),
                selectable_index: Some(item_index),
            });
            item_index += 1;
        }
    }

    // Zone 3: Heartbeat
    let heartbeat_items = tasks.heartbeat_items();

    if !heartbeat_items.is_empty() {
        rows.push(SidebarRow {
            line: Line::from(Span::styled("\u{2500}".repeat(width.min(40)), theme.fg_dim)),
            target: None,
            selectable_index: None,
        });
        rows.push(SidebarRow {
            line: Line::from(Span::styled(
                "\u{2665} Heartbeat".to_string(),
                theme.accent_danger,
            )),
            target: None,
            selectable_index: None,
        });

        for item in heartbeat_items {
            let dot = heartbeat_dot(item.outcome, theme);
            let msg = item.message.as_deref().unwrap_or("OK");
            let mut spans = vec![
                Span::raw("  "),
                dot,
                Span::raw(" "),
                Span::raw(item.label.clone()),
                Span::raw("  "),
                Span::styled(msg.to_string(), theme.fg_dim),
            ];
            if matches!(item.outcome, Some(HeartbeatOutcome::Error)) {
                spans.push(Span::styled(" !", theme.accent_danger));
            }
            rows.push(SidebarRow {
                line: Line::from(spans),
                target: None,
                selectable_index: None,
            });
        }
    }

    // Zone 4: Heartbeat Digest (latest structured digest from LLM synthesis)
    if let Some(digest) = tasks.last_digest() {
        if digest.actionable && !digest.items.is_empty() {
            if heartbeat_items.is_empty() {
                // Only show separator if Zone 3 didn't already render one
                rows.push(SidebarRow {
                    line: Line::from(Span::styled("\u{2500}".repeat(width.min(40)), theme.fg_dim)),
                    target: None,
                    selectable_index: None,
                });
            }
            rows.push(SidebarRow {
                line: Line::from(Span::styled(
                    "\u{2665} Heartbeat Digest".to_string(),
                    theme.accent_danger,
                )),
                target: None,
                selectable_index: None,
            });
            for item in &digest.items {
                let priority_indicator = match item.priority {
                    1 => Span::styled("!!", theme.accent_danger),
                    2 => Span::styled("! ", theme.accent_secondary),
                    _ => Span::styled("  ", theme.fg_dim),
                };
                rows.push(SidebarRow {
                    line: Line::from(vec![
                        Span::raw("  "),
                        priority_indicator,
                        Span::raw(" "),
                        Span::raw(item.title.clone()),
                    ]),
                    target: None,
                    selectable_index: None,
                });
                if !item.suggestion.is_empty() {
                    rows.push(SidebarRow {
                        line: Line::from(vec![
                            Span::raw("     "),
                            Span::styled(item.suggestion.clone(), theme.fg_dim),
                        ]),
                        target: None,
                        selectable_index: None,
                    });
                }
            }
            // Inline explanation per D-01: render explanation text beneath digest items
            if let Some(explanation) = &digest.explanation {
                if !explanation.is_empty() {
                    // Wrap long explanation text to available width
                    let max_text_width = width.saturating_sub(4);
                    for chunk in wrap_text(explanation, max_text_width) {
                        rows.push(SidebarRow {
                            line: Line::from(vec![
                                Span::raw("  "),
                                Span::styled(chunk, theme.fg_dim),
                            ]),
                            target: None,
                            selectable_index: None,
                        });
                    }
                }
            }
        }
    }

    // Empty state
    if rows.is_empty() {
        rows.push(SidebarRow {
            line: Line::from(Span::styled(" No tasks".to_string(), theme.fg_dim)),
            target: None,
            selectable_index: None,
        });
    }

    rows
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

fn goal_run_status_dot(status: Option<GoalRunStatus>, theme: &ThemeTokens) -> Span<'static> {
    match status {
        Some(GoalRunStatus::Running) => Span::styled("\u{25cf}", theme.accent_secondary),
        Some(GoalRunStatus::Paused) => Span::styled("\u{25cf}", theme.accent_primary),
        Some(GoalRunStatus::Completed) => Span::styled("\u{25cf}", theme.accent_success),
        Some(GoalRunStatus::Failed) => Span::styled("\u{25cf}", theme.accent_danger),
        _ => Span::styled("\u{25cf}", theme.fg_dim),
    }
}

fn goal_run_status_label(status: Option<GoalRunStatus>, theme: &ThemeTokens) -> Span<'static> {
    match status {
        Some(GoalRunStatus::Running) => Span::styled(" running", theme.accent_secondary),
        Some(GoalRunStatus::Paused) => Span::styled(" paused", theme.accent_primary),
        Some(GoalRunStatus::Completed) => Span::styled(" done", theme.accent_success),
        Some(GoalRunStatus::Failed) => Span::styled(" failed", theme.accent_danger),
        Some(GoalRunStatus::Cancelled) => Span::styled(" cancelled", theme.fg_dim),
        Some(GoalRunStatus::Planning) => Span::styled(" planning", theme.fg_dim),
        Some(GoalRunStatus::AwaitingApproval) => Span::styled(" awaiting approval", theme.fg_dim),
        _ => Span::styled(" queued", theme.fg_dim),
    }
}

fn step_status_chip(status: Option<GoalRunStatus>, theme: &ThemeTokens) -> Span<'static> {
    match status {
        None
        | Some(GoalRunStatus::Queued)
        | Some(GoalRunStatus::Planning)
        | Some(GoalRunStatus::AwaitingApproval) => Span::styled("[ ]", theme.fg_dim),
        Some(GoalRunStatus::Running) => Span::styled("[~]", theme.accent_secondary),
        Some(GoalRunStatus::Paused) => Span::styled("[P]", theme.accent_primary),
        Some(GoalRunStatus::Completed) => Span::styled("[x]", theme.accent_success),
        Some(GoalRunStatus::Failed) => Span::styled("[!]", theme.accent_danger),
        Some(GoalRunStatus::Cancelled) => Span::styled("[C]", theme.fg_dim),
    }
}

fn task_status_chip(status: Option<TaskStatus>, theme: &ThemeTokens) -> Span<'static> {
    match status {
        None | Some(TaskStatus::Queued) => Span::styled("[ ]", theme.fg_dim),
        Some(TaskStatus::InProgress) => Span::styled("[~]", theme.accent_secondary),
        Some(TaskStatus::Completed) => Span::styled("[x]", theme.accent_success),
        Some(TaskStatus::Failed) | Some(TaskStatus::FailedAnalyzing) => {
            Span::styled("[!]", theme.accent_danger)
        }
        Some(TaskStatus::Blocked) => Span::styled("[B]", theme.fg_dim),
        Some(TaskStatus::AwaitingApproval) => Span::styled("[?]", theme.accent_primary),
        Some(TaskStatus::Cancelled) => Span::styled("[C]", theme.fg_dim),
    }
}

fn heartbeat_dot(outcome: Option<HeartbeatOutcome>, theme: &ThemeTokens) -> Span<'static> {
    match outcome {
        Some(HeartbeatOutcome::Ok) | None => Span::styled("\u{25cf}", theme.accent_success),
        Some(HeartbeatOutcome::Warn) => Span::styled("\u{25cf}", theme.accent_secondary),
        Some(HeartbeatOutcome::Error) => Span::styled("\u{25cf}", theme.accent_danger),
    }
}

/// Simple word-aware text wrapping that splits long lines at word boundaries.
fn wrap_text(text: &str, max_width: usize) -> Vec<String> {
    if max_width == 0 {
        return vec![text.to_string()];
    }
    let mut lines = Vec::new();
    let mut current = String::new();
    for word in text.split_whitespace() {
        if current.is_empty() {
            current = word.to_string();
        } else if current.len() + 1 + word.len() <= max_width {
            current.push(' ');
            current.push_str(word);
        } else {
            lines.push(current);
            current = word.to_string();
        }
    }
    if !current.is_empty() {
        lines.push(current);
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::sidebar::SidebarState;
    use crate::state::task::TaskState;

    #[test]
    fn task_tree_handles_empty_state() {
        let tasks = TaskState::new();
        let _sidebar = SidebarState::new();
        let _theme = ThemeTokens::default();
        assert!(tasks.goal_runs().is_empty());
        assert!(tasks.tasks().is_empty());
    }

    #[test]
    fn task_status_chips_all_variants() {
        let theme = ThemeTokens::default();
        let _ = task_status_chip(None, &theme);
        let _ = task_status_chip(Some(TaskStatus::Queued), &theme);
        let _ = task_status_chip(Some(TaskStatus::InProgress), &theme);
        let _ = task_status_chip(Some(TaskStatus::Completed), &theme);
        let _ = task_status_chip(Some(TaskStatus::Failed), &theme);
        let _ = task_status_chip(Some(TaskStatus::Blocked), &theme);
        let _ = task_status_chip(Some(TaskStatus::AwaitingApproval), &theme);
    }
}
