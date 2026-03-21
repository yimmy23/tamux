use ratatui::prelude::*;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::state::sidebar::SidebarItemTarget;
use crate::state::task::{
    AgentTask, GoalRun, GoalRunStatus, GoalRunStep, TaskState, TaskStatus, TodoItem, TodoStatus,
    WorkContextEntryKind,
};
use crate::theme::ThemeTokens;
use crate::widgets::message::wrap_text;

fn content_inner(area: Rect) -> Rect {
    area
}

#[derive(Clone)]
struct RenderRow {
    line: Line<'static>,
    work_path: Option<String>,
}

pub enum TaskViewHitTarget {
    WorkPath(String),
}

fn goal_status_label(status: Option<GoalRunStatus>) -> &'static str {
    match status {
        Some(GoalRunStatus::Running) => "running",
        Some(GoalRunStatus::Completed) => "done",
        Some(GoalRunStatus::Failed) => "failed",
        Some(GoalRunStatus::Cancelled) => "cancelled",
        _ => "pending",
    }
}

fn task_status_label(status: Option<TaskStatus>) -> &'static str {
    match status {
        Some(TaskStatus::InProgress) => "running",
        Some(TaskStatus::Completed) => "done",
        Some(TaskStatus::Failed) | Some(TaskStatus::FailedAnalyzing) => "failed",
        Some(TaskStatus::Blocked) => "blocked",
        Some(TaskStatus::AwaitingApproval) => "awaiting approval",
        Some(TaskStatus::Cancelled) => "cancelled",
        _ => "queued",
    }
}

fn todo_status_chip(status: Option<TodoStatus>) -> &'static str {
    match status {
        Some(TodoStatus::InProgress) => "[~]",
        Some(TodoStatus::Completed) => "[x]",
        Some(TodoStatus::Blocked) => "[!]",
        _ => "[ ]",
    }
}

fn work_kind_label(kind: Option<WorkContextEntryKind>) -> &'static str {
    match kind {
        Some(WorkContextEntryKind::GeneratedSkill) => "skill",
        Some(WorkContextEntryKind::Artifact) => "file",
        _ => "diff",
    }
}

fn push_wrapped_text(
    rows: &mut Vec<RenderRow>,
    text: &str,
    style: Style,
    width: usize,
    indent: usize,
) {
    let available = width.saturating_sub(indent).max(1);
    for wrapped in wrap_text(text, available) {
        rows.push(RenderRow {
            line: Line::from(vec![
                Span::raw(" ".repeat(indent)),
                Span::styled(wrapped, style),
            ]),
            work_path: None,
        });
    }
}

fn push_blank(rows: &mut Vec<RenderRow>) {
    rows.push(RenderRow {
        line: Line::raw(""),
        work_path: None,
    });
}

fn push_section_title(rows: &mut Vec<RenderRow>, title: &str, style: Style) {
    if !rows.is_empty() {
        push_blank(rows);
    }
    rows.push(RenderRow {
        line: Line::from(Span::styled(title.to_string(), style)),
        work_path: None,
    });
}

fn related_tasks_for_step<'a>(
    tasks: &'a TaskState,
    run: &GoalRun,
    step: &GoalRunStep,
) -> Vec<&'a AgentTask> {
    tasks
        .tasks()
        .iter()
        .filter(|task| {
            task.goal_run_id.as_deref() == Some(run.id.as_str())
                && (task.goal_step_title.as_deref() == Some(step.title.as_str())
                    || step
                        .task_id
                        .as_deref()
                        .is_some_and(|task_id| task.id == task_id))
        })
        .collect()
}

fn push_todo_items(
    rows: &mut Vec<RenderRow>,
    items: &[TodoItem],
    theme: &ThemeTokens,
    _width: usize,
    indent: usize,
) {
    if items.is_empty() {
        rows.push(RenderRow {
            line: Line::from(vec![
                Span::raw(" ".repeat(indent)),
                Span::styled("No todos", theme.fg_dim),
            ]),
            work_path: None,
        });
        return;
    }

    let mut sorted = items.to_vec();
    sorted.sort_by_key(|item| item.position);
    for item in sorted {
        rows.push(RenderRow {
            line: Line::from(vec![
                Span::raw(" ".repeat(indent)),
                Span::styled(todo_status_chip(item.status), theme.fg_dim),
                Span::raw(" "),
                Span::styled(item.content, theme.fg_active),
            ]),
            work_path: None,
        });
    }
}

fn render_goal_summary(
    rows: &mut Vec<RenderRow>,
    run: &GoalRun,
    theme: &ThemeTokens,
    width: usize,
) {
    rows.push(RenderRow {
        line: Line::from(vec![
            Span::styled("Status: ", theme.fg_dim),
            Span::styled(goal_status_label(run.status), theme.fg_active),
        ]),
        work_path: None,
    });
    rows.push(RenderRow {
        line: Line::from(vec![
            Span::styled("Tasks: ", theme.fg_dim),
            Span::styled(run.child_task_count.to_string(), theme.fg_active),
            Span::raw("  "),
            Span::styled("Approvals: ", theme.fg_dim),
            Span::styled(run.approval_count.to_string(), theme.fg_active),
        ]),
        work_path: None,
    });
    if let Some(current_step_title) = &run.current_step_title {
        rows.push(RenderRow {
            line: Line::from(vec![
                Span::styled("Current Step: ", theme.fg_dim),
                Span::styled(current_step_title.clone(), theme.fg_active),
            ]),
            work_path: None,
        });
    }
    if !run.goal.is_empty() {
        push_section_title(
            rows,
            "Goal Definition",
            theme.accent_primary.add_modifier(Modifier::BOLD),
        );
        push_wrapped_text(rows, &run.goal, theme.fg_active, width, 0);
    }
    if let Some(last_error) = &run.last_error {
        push_section_title(
            rows,
            "Last Error",
            theme.accent_primary.add_modifier(Modifier::BOLD),
        );
        push_wrapped_text(rows, last_error, theme.accent_danger, width, 0);
    }
}

fn render_steps(
    rows: &mut Vec<RenderRow>,
    tasks: &TaskState,
    run: &GoalRun,
    selected_step_id: Option<&str>,
    theme: &ThemeTokens,
    width: usize,
) {
    push_section_title(
        rows,
        "Execution Plan",
        theme.accent_primary.add_modifier(Modifier::BOLD),
    );

    let mut steps = run.steps.clone();
    steps.sort_by_key(|step| step.order);

    if steps.is_empty() {
        rows.push(RenderRow {
            line: Line::from(Span::styled("No steps", theme.fg_dim)),
            work_path: None,
        });
        return;
    }

    for step in &steps {
        let chip = match step.status {
            None | Some(GoalRunStatus::Pending) => "[ ]",
            Some(GoalRunStatus::Running) => "[~]",
            Some(GoalRunStatus::Completed) => "[x]",
            Some(GoalRunStatus::Failed) => "[!]",
            Some(GoalRunStatus::Cancelled) => "[-]",
        };
        let mut line = Line::from(vec![
            Span::styled(chip, theme.fg_dim),
            Span::raw(" "),
            Span::styled(step.title.clone(), theme.fg_active),
        ]);
        if selected_step_id == Some(step.id.as_str()) {
            line = line.style(Style::default().bg(Color::Indexed(236)));
        }
        rows.push(RenderRow {
            line,
            work_path: None,
        });

        if !step.instructions.is_empty() {
            push_wrapped_text(rows, &step.instructions, theme.fg_dim, width, 2);
        }
        if let Some(summary) = &step.summary {
            push_wrapped_text(rows, summary, theme.fg_active, width, 2);
        }
        if let Some(error) = &step.error {
            push_wrapped_text(rows, error, theme.accent_danger, width, 2);
        }

        for task in related_tasks_for_step(tasks, run, step) {
            rows.push(RenderRow {
                line: Line::from(vec![
                    Span::raw("  "),
                    Span::styled("• ", theme.fg_dim),
                    Span::styled(task.title.clone(), theme.fg_active),
                    Span::raw(" "),
                    Span::styled(task_status_label(task.status), theme.fg_dim),
                ]),
                work_path: None,
            });
        }
    }
}

fn render_step_timeline(
    rows: &mut Vec<RenderRow>,
    run: &GoalRun,
    theme: &ThemeTokens,
    width: usize,
) {
    if run.events.is_empty() {
        return;
    }

    push_section_title(
        rows,
        "Step Timeline",
        theme.accent_primary.add_modifier(Modifier::BOLD),
    );
    for event in run.events.iter().rev().take(18).rev() {
        let mut prefix = format!("[{}] {}", event.phase, event.message);
        if let Some(step_index) = event.step_index {
            prefix = format!("step {} • {}", step_index + 1, prefix);
        }
        push_wrapped_text(rows, &prefix, theme.fg_active, width, 0);
        if let Some(details) = &event.details {
            push_wrapped_text(rows, details, theme.fg_dim, width, 2);
        }
        if !event.todo_snapshot.is_empty() {
            push_todo_items(rows, &event.todo_snapshot, theme, width, 4);
        }
    }
}

fn render_live_todos(
    rows: &mut Vec<RenderRow>,
    tasks: &TaskState,
    thread_id: Option<&str>,
    theme: &ThemeTokens,
    width: usize,
) {
    let Some(thread_id) = thread_id else {
        return;
    };
    push_section_title(
        rows,
        "Live Todos",
        theme.accent_primary.add_modifier(Modifier::BOLD),
    );
    push_todo_items(rows, tasks.todos_for_thread(thread_id), theme, width, 0);
}

fn render_work_context(
    rows: &mut Vec<RenderRow>,
    tasks: &TaskState,
    thread_id: Option<&str>,
    theme: &ThemeTokens,
    width: usize,
) {
    let Some(thread_id) = thread_id else {
        return;
    };
    let Some(context) = tasks.work_context_for_thread(thread_id) else {
        return;
    };

    push_section_title(
        rows,
        "Files",
        theme.accent_primary.add_modifier(Modifier::BOLD),
    );
    if context.entries.is_empty() {
        rows.push(RenderRow {
            line: Line::from(Span::styled(
                "No file or artifact activity yet.",
                theme.fg_dim,
            )),
            work_path: None,
        });
        return;
    }

    let selected_path = tasks.selected_work_path(thread_id);
    for entry in &context.entries {
        let label = entry
            .change_kind
            .as_deref()
            .unwrap_or_else(|| work_kind_label(entry.kind));
        let marker = if selected_path == Some(entry.path.as_str()) {
            ">"
        } else {
            " "
        };
        rows.push(RenderRow {
            line: Line::from(vec![
                Span::styled(marker, theme.accent_primary),
                Span::raw(" "),
                Span::styled(format!("[{}]", label), theme.fg_dim),
                Span::raw(" "),
                Span::styled(entry.path.clone(), theme.fg_active),
            ]),
            work_path: Some(entry.path.clone()),
        });
        if let Some(previous_path) = &entry.previous_path {
            push_wrapped_text(
                rows,
                &format!("from {}", previous_path),
                theme.fg_dim,
                width,
                4,
            );
        }
    }

    let Some(selected_path) = selected_path else {
        return;
    };
    let Some(selected_entry) = context
        .entries
        .iter()
        .find(|entry| entry.path == selected_path)
    else {
        return;
    };

    push_section_title(
        rows,
        "Preview",
        theme.accent_primary.add_modifier(Modifier::BOLD),
    );
    if let Some(repo_root) = selected_entry.repo_root.as_deref() {
        if let Some(diff) = tasks.diff_for_path(repo_root, &selected_entry.path) {
            if diff.trim().is_empty() {
                rows.push(RenderRow {
                    line: Line::from(Span::styled(
                        "No diff preview available for the selected file.",
                        theme.fg_dim,
                    )),
                    work_path: None,
                });
            } else {
                push_wrapped_text(rows, diff, theme.fg_dim, width, 0);
            }
            return;
        }
    }

    let preview_key = if selected_entry.repo_root.is_some() {
        selected_entry
            .repo_root
            .as_deref()
            .map(|repo_root| format!("{repo_root}/{}", selected_entry.path))
            .unwrap_or_else(|| selected_entry.path.clone())
    } else {
        selected_entry.path.clone()
    };
    if let Some(preview) = tasks.preview_for_path(&preview_key) {
        if preview.is_text {
            push_wrapped_text(rows, &preview.content, theme.fg_dim, width, 0);
        } else {
            rows.push(RenderRow {
                line: Line::from(Span::styled(
                    "Binary file preview is not available.",
                    theme.fg_dim,
                )),
                work_path: None,
            });
        }
    } else {
        rows.push(RenderRow {
            line: Line::from(Span::styled("Loading preview...", theme.fg_dim)),
            work_path: None,
        });
    }
}

fn build_rows(
    tasks: &TaskState,
    target: &SidebarItemTarget,
    theme: &ThemeTokens,
    width: usize,
) -> (String, Vec<RenderRow>) {
    let mut rows = Vec::new();
    let section_style = theme.accent_primary.add_modifier(Modifier::BOLD);
    let highlight_style = Style::default().bg(Color::Indexed(236));

    match target {
        SidebarItemTarget::GoalRun {
            goal_run_id,
            step_id,
        } => {
            let Some(run) = tasks.goal_run_by_id(goal_run_id) else {
                return (
                    " Goal ".to_string(),
                    vec![RenderRow {
                        line: Line::from(Span::styled("Goal run not found", theme.accent_danger)),
                        work_path: None,
                    }],
                );
            };

            rows.push(RenderRow {
                line: Line::from(vec![
                    Span::styled("ID: ", theme.fg_dim),
                    Span::styled(run.id.clone(), theme.fg_active),
                ]),
                work_path: None,
            });
            render_goal_summary(&mut rows, run, theme, width);
            render_live_todos(&mut rows, tasks, run.thread_id.as_deref(), theme, width);
            render_steps(&mut rows, tasks, run, step_id.as_deref(), theme, width);

            let child_tasks: Vec<_> = tasks
                .tasks()
                .iter()
                .filter(|task| task.goal_run_id.as_deref() == Some(goal_run_id.as_str()))
                .collect();
            push_section_title(&mut rows, "Related Tasks", section_style);
            if child_tasks.is_empty() {
                rows.push(RenderRow {
                    line: Line::from(Span::styled("No tasks", theme.fg_dim)),
                    work_path: None,
                });
            } else {
                for task in child_tasks {
                    rows.push(RenderRow {
                        line: Line::from(vec![
                            Span::styled("[ ] ", theme.fg_dim),
                            Span::styled(task.title.clone(), theme.fg_active),
                            Span::raw(" "),
                            Span::styled(task_status_label(task.status), theme.fg_dim),
                        ]),
                        work_path: None,
                    });
                }
            }

            if let Some(summary) = &run.reflection_summary {
                push_section_title(&mut rows, "Reflection", section_style);
                push_wrapped_text(&mut rows, summary, theme.fg_dim, width, 0);
            }
            if !run.memory_updates.is_empty() {
                push_section_title(&mut rows, "Memory Updates", section_style);
                for update in &run.memory_updates {
                    push_wrapped_text(&mut rows, &format!("• {}", update), theme.fg_dim, width, 0);
                }
            }
            render_step_timeline(&mut rows, run, theme, width);
            render_work_context(&mut rows, tasks, run.thread_id.as_deref(), theme, width);

            (format!(" Goal: {} ", run.title), rows)
        }
        SidebarItemTarget::Task { task_id } => {
            let Some(task) = tasks.task_by_id(task_id) else {
                return (
                    " Task ".to_string(),
                    vec![RenderRow {
                        line: Line::from(Span::styled("Task not found", theme.accent_danger)),
                        work_path: None,
                    }],
                );
            };

            rows.push(RenderRow {
                line: Line::from(vec![
                    Span::styled("Status: ", theme.fg_dim),
                    Span::styled(task_status_label(task.status), theme.fg_active),
                ]),
                work_path: None,
            });
            rows.push(RenderRow {
                line: Line::from(vec![
                    Span::styled("Progress: ", theme.fg_dim),
                    Span::styled(format!("{}%", task.progress), theme.fg_active),
                ]),
                work_path: None,
            });
            if let Some(session_id) = &task.session_id {
                rows.push(RenderRow {
                    line: Line::from(vec![
                        Span::styled("Session: ", theme.fg_dim),
                        Span::styled(session_id.clone(), theme.fg_active),
                    ]),
                    work_path: None,
                });
            }

            let parent_goal = task
                .goal_run_id
                .as_deref()
                .and_then(|goal_run_id| tasks.goal_run_by_id(goal_run_id));
            if let Some(run) = parent_goal {
                push_section_title(&mut rows, "Parent Goal", section_style);
                push_wrapped_text(&mut rows, &run.title, theme.fg_active, width, 0);
                if !run.goal.is_empty() {
                    push_wrapped_text(&mut rows, &run.goal, theme.fg_dim, width, 0);
                }
                if let Some(step) = run.steps.iter().find(|step| {
                    step.task_id.as_deref() == Some(task.id.as_str())
                        || task.goal_step_title.as_deref() == Some(step.title.as_str())
                }) {
                    rows.push(RenderRow {
                        line: Line::from(vec![
                            Span::styled("Step: ", theme.fg_dim),
                            Span::styled(step.title.clone(), theme.fg_active),
                        ])
                        .style(highlight_style),
                        work_path: None,
                    });
                    if !step.instructions.is_empty() {
                        push_wrapped_text(&mut rows, &step.instructions, theme.fg_dim, width, 2);
                    }
                    if let Some(summary) = &step.summary {
                        push_wrapped_text(&mut rows, summary, theme.fg_active, width, 2);
                    }
                }
                render_step_timeline(&mut rows, run, theme, width);
            }

            render_live_todos(&mut rows, tasks, task.thread_id.as_deref(), theme, width);
            if let Some(blocked_reason) = &task.blocked_reason {
                push_section_title(&mut rows, "Blocked Reason", section_style);
                push_wrapped_text(&mut rows, blocked_reason, theme.accent_danger, width, 0);
            }
            render_work_context(&mut rows, tasks, task.thread_id.as_deref(), theme, width);

            (format!(" Task: {} ", task.title), rows)
        }
    }
}

pub fn hit_test(
    area: Rect,
    tasks: &TaskState,
    target: &SidebarItemTarget,
    theme: &ThemeTokens,
    scroll: usize,
    position: Position,
) -> Option<TaskViewHitTarget> {
    let inner = content_inner(area);
    if !inner.contains(position) {
        return None;
    }

    let (_, rows) = build_rows(tasks, target, theme, inner.width as usize);
    let row_index = scroll + position.y.saturating_sub(inner.y) as usize;
    rows.get(row_index)
        .and_then(|row| row.work_path.clone())
        .map(TaskViewHitTarget::WorkPath)
}

pub fn render(
    frame: &mut Frame,
    area: Rect,
    tasks: &TaskState,
    target: &SidebarItemTarget,
    theme: &ThemeTokens,
    _focused: bool,
    scroll: usize,
) {
    let (_, rows) = build_rows(tasks, target, theme, area.width as usize);
    let inner = content_inner(area);

    if inner.width == 0 || inner.height == 0 {
        return;
    }

    let lines = rows.into_iter().map(|row| row.line).collect::<Vec<_>>();
    let max_scroll = lines.len().saturating_sub(inner.height as usize);
    let paragraph = Paragraph::new(lines).scroll((scroll.min(max_scroll) as u16, 0));
    frame.render_widget(paragraph, inner);
}
