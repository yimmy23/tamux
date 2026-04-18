use ratatui::prelude::*;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

#[path = "task_view_sections.rs"]
mod sections;

use sections::{
    render_checkpoints, render_live_todos, render_step_timeline, render_steps, render_work_context,
};

use crate::state::sidebar::SidebarItemTarget;
use crate::state::task::{
    AgentTask, GoalRun, GoalRunStatus, GoalRunStep, TaskState, TaskStatus, TodoItem, TodoStatus,
    WorkContextEntryKind,
};
use crate::theme::ThemeTokens;
use crate::widgets::message::{render_markdown_pub, wrap_text};

fn content_inner(area: Rect) -> Rect {
    area
}

const SCROLLBAR_WIDTH: u16 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TaskViewScrollbarLayout {
    pub content: Rect,
    pub scrollbar: Rect,
    pub thumb: Rect,
    pub scroll: usize,
    pub max_scroll: usize,
}

#[derive(Clone)]
struct RenderRow {
    line: Line<'static>,
    work_path: Option<String>,
    close_preview: bool,
}

pub enum TaskViewHitTarget {
    WorkPath(String),
    ClosePreview,
}

fn goal_status_label(status: Option<GoalRunStatus>) -> &'static str {
    match status {
        Some(GoalRunStatus::Queued) => "queued",
        Some(GoalRunStatus::Planning) => "planning",
        Some(GoalRunStatus::Running) => "running",
        Some(GoalRunStatus::AwaitingApproval) => "awaiting approval",
        Some(GoalRunStatus::Paused) => "paused",
        Some(GoalRunStatus::Completed) => "done",
        Some(GoalRunStatus::Failed) => "failed",
        Some(GoalRunStatus::Cancelled) => "cancelled",
        _ => "queued",
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
            close_preview: false,
        });
    }
}

fn push_blank(rows: &mut Vec<RenderRow>) {
    rows.push(RenderRow {
        line: Line::raw(""),
        work_path: None,
        close_preview: false,
    });
}

fn push_section_title(rows: &mut Vec<RenderRow>, title: &str, style: Style) {
    if !rows.is_empty() {
        push_blank(rows);
    }
    rows.push(RenderRow {
        line: Line::from(Span::styled(title.to_string(), style)),
        work_path: None,
        close_preview: false,
    });
}

fn is_markdown_path(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    lower.ends_with(".md") || lower.ends_with(".markdown") || lower.ends_with(".mdx")
}

fn push_preview_text(
    rows: &mut Vec<RenderRow>,
    path: &str,
    content: &str,
    theme: &ThemeTokens,
    width: usize,
) {
    if is_markdown_path(path) {
        for line in render_markdown_pub(content, width.max(1)) {
            rows.push(RenderRow {
                line,
                work_path: None,
                close_preview: false,
            });
        }
    } else {
        push_wrapped_text(rows, content, theme.fg_dim, width, 0);
    }
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
            close_preview: false,
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
            close_preview: false,
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
        close_preview: false,
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
        close_preview: false,
    });
    if let Some(current_step_title) = &run.current_step_title {
        rows.push(RenderRow {
            line: Line::from(vec![
                Span::styled("Current Step: ", theme.fg_dim),
                Span::styled(current_step_title.clone(), theme.fg_active),
            ]),
            work_path: None,
            close_preview: false,
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

fn build_rows(
    tasks: &TaskState,
    target: &SidebarItemTarget,
    theme: &ThemeTokens,
    width: usize,
    show_live_todos: bool,
    show_timeline: bool,
    show_files: bool,
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
                        close_preview: false,
                    }],
                );
            };

            rows.push(RenderRow {
                line: Line::from(vec![
                    Span::styled("ID: ", theme.fg_dim),
                    Span::styled(run.id.clone(), theme.fg_active),
                ]),
                work_path: None,
                close_preview: false,
            });
            render_goal_summary(&mut rows, run, theme, width);
            render_checkpoints(&mut rows, tasks, run, theme, width);
            if show_live_todos {
                render_live_todos(&mut rows, tasks, run.thread_id.as_deref(), theme, width);
            }
            if show_files {
                render_work_context(&mut rows, tasks, run.thread_id.as_deref(), theme, width);
            }
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
                    close_preview: false,
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
                        close_preview: false,
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
            if show_timeline {
                render_step_timeline(&mut rows, run, theme, width);
            }

            (format!(" Goal: {} ", run.title), rows)
        }
        SidebarItemTarget::Task { task_id } => {
            let Some(task) = tasks.task_by_id(task_id) else {
                return (
                    " Task ".to_string(),
                    vec![RenderRow {
                        line: Line::from(Span::styled("Task not found", theme.accent_danger)),
                        work_path: None,
                        close_preview: false,
                    }],
                );
            };

            rows.push(RenderRow {
                line: Line::from(vec![
                    Span::styled("Status: ", theme.fg_dim),
                    Span::styled(task_status_label(task.status), theme.fg_active),
                ]),
                work_path: None,
                close_preview: false,
            });
            rows.push(RenderRow {
                line: Line::from(vec![
                    Span::styled("Progress: ", theme.fg_dim),
                    Span::styled(format!("{}%", task.progress), theme.fg_active),
                ]),
                work_path: None,
                close_preview: false,
            });
            if let Some(session_id) = &task.session_id {
                rows.push(RenderRow {
                    line: Line::from(vec![
                        Span::styled("Session: ", theme.fg_dim),
                        Span::styled(session_id.clone(), theme.fg_active),
                    ]),
                    work_path: None,
                    close_preview: false,
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
                        close_preview: false,
                    });
                    if !step.instructions.is_empty() {
                        push_wrapped_text(&mut rows, &step.instructions, theme.fg_dim, width, 2);
                    }
                    if let Some(summary) = &step.summary {
                        push_wrapped_text(&mut rows, summary, theme.fg_active, width, 2);
                    }
                }
                if show_timeline {
                    render_step_timeline(&mut rows, run, theme, width);
                }
            }

            if show_live_todos {
                render_live_todos(&mut rows, tasks, task.thread_id.as_deref(), theme, width);
            }
            if show_files {
                render_work_context(&mut rows, tasks, task.thread_id.as_deref(), theme, width);
            }
            if let Some(blocked_reason) = &task.blocked_reason {
                push_section_title(&mut rows, "Blocked Reason", section_style);
                push_wrapped_text(&mut rows, blocked_reason, theme.accent_danger, width, 0);
            }

            (format!(" Task: {} ", task.title), rows)
        }
    }
}

fn rows_for_width(
    tasks: &TaskState,
    target: &SidebarItemTarget,
    theme: &ThemeTokens,
    width: usize,
    show_live_todos: bool,
    show_timeline: bool,
    show_files: bool,
) -> Vec<RenderRow> {
    let (_, rows) = build_rows(
        tasks,
        target,
        theme,
        width,
        show_live_todos,
        show_timeline,
        show_files,
    );
    rows
}

fn scrollbar_layout_from_metrics(
    area: Rect,
    total_rows: usize,
    scroll: usize,
) -> Option<TaskViewScrollbarLayout> {
    if area.width <= SCROLLBAR_WIDTH || area.height == 0 || total_rows <= area.height as usize {
        return None;
    }

    let viewport = area.height as usize;
    let max_scroll = total_rows.saturating_sub(viewport);
    let scroll = scroll.min(max_scroll);
    let content = Rect::new(
        area.x,
        area.y,
        area.width.saturating_sub(SCROLLBAR_WIDTH),
        area.height,
    );
    let scrollbar = Rect::new(
        area.x
            .saturating_add(area.width)
            .saturating_sub(SCROLLBAR_WIDTH),
        area.y,
        SCROLLBAR_WIDTH,
        area.height,
    );
    let thumb_height = ((viewport * viewport) / total_rows).max(1).min(viewport) as u16;
    let track_span = scrollbar.height.saturating_sub(thumb_height);
    let thumb_offset = if max_scroll == 0 || track_span == 0 {
        0
    } else {
        ((scroll * track_span as usize) + (max_scroll / 2)) / max_scroll
    } as u16;
    let thumb = Rect::new(
        scrollbar.x,
        scrollbar.y.saturating_add(thumb_offset),
        scrollbar.width,
        thumb_height,
    );

    Some(TaskViewScrollbarLayout {
        content,
        scrollbar,
        thumb,
        scroll,
        max_scroll,
    })
}

pub fn hit_test(
    area: Rect,
    tasks: &TaskState,
    target: &SidebarItemTarget,
    theme: &ThemeTokens,
    scroll: usize,
    show_live_todos: bool,
    show_timeline: bool,
    show_files: bool,
    position: Position,
) -> Option<TaskViewHitTarget> {
    let inner = content_inner(area);
    let layout = scrollbar_layout(
        area,
        tasks,
        target,
        theme,
        scroll,
        show_live_todos,
        show_timeline,
        show_files,
    );
    let content = layout.map(|layout| layout.content).unwrap_or(inner);
    if !content.contains(position) {
        return None;
    }

    let rows = rows_for_width(
        tasks,
        target,
        theme,
        content.width as usize,
        show_live_todos,
        show_timeline,
        show_files,
    );
    let resolved_scroll = layout.map(|layout| layout.scroll).unwrap_or(scroll);
    let row_index = resolved_scroll + position.y.saturating_sub(content.y) as usize;
    rows.get(row_index).and_then(|row| {
        if row.close_preview {
            Some(TaskViewHitTarget::ClosePreview)
        } else {
            row.work_path.clone().map(TaskViewHitTarget::WorkPath)
        }
    })
}

pub fn render(
    frame: &mut Frame,
    area: Rect,
    tasks: &TaskState,
    target: &SidebarItemTarget,
    theme: &ThemeTokens,
    _focused: bool,
    scroll: usize,
    show_live_todos: bool,
    show_timeline: bool,
    show_files: bool,
) {
    let inner = content_inner(area);

    if inner.width == 0 || inner.height == 0 {
        return;
    }

    if let Some(layout) = scrollbar_layout(
        area,
        tasks,
        target,
        theme,
        scroll,
        show_live_todos,
        show_timeline,
        show_files,
    ) {
        let lines = rows_for_width(
            tasks,
            target,
            theme,
            layout.content.width as usize,
            show_live_todos,
            show_timeline,
            show_files,
        )
        .into_iter()
        .map(|row| row.line)
        .collect::<Vec<_>>();
        let paragraph = Paragraph::new(lines).scroll((layout.scroll as u16, 0));
        frame.render_widget(paragraph, layout.content);

        let scrollbar_lines = (0..layout.scrollbar.height)
            .map(|offset| {
                let y = layout.scrollbar.y.saturating_add(offset);
                let (glyph, style) = if y >= layout.thumb.y
                    && y < layout.thumb.y.saturating_add(layout.thumb.height)
                {
                    ("█", theme.accent_primary)
                } else {
                    ("│", theme.fg_dim)
                };
                Line::from(Span::styled(glyph, style))
            })
            .collect::<Vec<_>>();
        frame.render_widget(Paragraph::new(scrollbar_lines), layout.scrollbar);
        return;
    }

    let lines = rows_for_width(
        tasks,
        target,
        theme,
        inner.width as usize,
        show_live_todos,
        show_timeline,
        show_files,
    )
    .into_iter()
    .map(|row| row.line)
    .collect::<Vec<_>>();
    let max_scroll = lines.len().saturating_sub(inner.height as usize);
    let paragraph = Paragraph::new(lines).scroll((scroll.min(max_scroll) as u16, 0));
    frame.render_widget(paragraph, inner);
}

pub fn max_scroll(
    area: Rect,
    tasks: &TaskState,
    target: &SidebarItemTarget,
    theme: &ThemeTokens,
    show_live_todos: bool,
    show_timeline: bool,
    show_files: bool,
) -> usize {
    let inner = content_inner(area);
    if inner.width == 0 || inner.height == 0 {
        return 0;
    }

    scrollbar_layout(
        area,
        tasks,
        target,
        theme,
        0,
        show_live_todos,
        show_timeline,
        show_files,
    )
    .map(|layout| layout.max_scroll)
    .unwrap_or_else(|| {
        let rows = rows_for_width(
            tasks,
            target,
            theme,
            inner.width as usize,
            show_live_todos,
            show_timeline,
            show_files,
        );
        rows.len().saturating_sub(inner.height as usize)
    })
}

pub fn scrollbar_layout(
    area: Rect,
    tasks: &TaskState,
    target: &SidebarItemTarget,
    theme: &ThemeTokens,
    scroll: usize,
    show_live_todos: bool,
    show_timeline: bool,
    show_files: bool,
) -> Option<TaskViewScrollbarLayout> {
    let inner = content_inner(area);
    if inner.width <= SCROLLBAR_WIDTH || inner.height == 0 {
        return None;
    }

    let full_rows = rows_for_width(
        tasks,
        target,
        theme,
        inner.width as usize,
        show_live_todos,
        show_timeline,
        show_files,
    );
    if full_rows.len() <= inner.height as usize {
        return None;
    }

    let content_width = inner.width.saturating_sub(SCROLLBAR_WIDTH) as usize;
    let rows = rows_for_width(
        tasks,
        target,
        theme,
        content_width,
        show_live_todos,
        show_timeline,
        show_files,
    );
    scrollbar_layout_from_metrics(inner, rows.len(), scroll)
}
