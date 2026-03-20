use ratatui::prelude::*;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph};

use crate::state::sidebar::SidebarItemTarget;
use crate::state::task::{AgentTask, GoalRun, GoalRunStatus, GoalRunStep, TaskState, TaskStatus};
use crate::theme::ThemeTokens;
use crate::widgets::message::wrap_text;

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

fn push_wrapped_text(
    lines: &mut Vec<Line<'static>>,
    text: &str,
    style: Style,
    width: usize,
    indent: usize,
) {
    let available = width.saturating_sub(indent).max(1);
    for wrapped in wrap_text(text, available) {
        lines.push(Line::from(vec![
            Span::raw(" ".repeat(indent)),
            Span::styled(wrapped, style),
        ]));
    }
}

fn push_section_title(lines: &mut Vec<Line<'static>>, title: &str, style: Style) {
    if !lines.is_empty() {
        lines.push(Line::raw(""));
    }
    lines.push(Line::from(Span::styled(title.to_string(), style)));
}

fn related_tasks_for_step<'a>(tasks: &'a TaskState, run: &GoalRun, step: &GoalRunStep) -> Vec<&'a AgentTask> {
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

fn render_goal_summary(
    lines: &mut Vec<Line<'static>>,
    run: &GoalRun,
    theme: &ThemeTokens,
    width: usize,
) {
    lines.push(Line::from(vec![
        Span::styled("Status: ", theme.fg_dim),
        Span::styled(goal_status_label(run.status), theme.fg_active),
    ]));
    lines.push(Line::from(vec![
        Span::styled("Tasks: ", theme.fg_dim),
        Span::styled(run.child_task_count.to_string(), theme.fg_active),
        Span::raw("  "),
        Span::styled("Approvals: ", theme.fg_dim),
        Span::styled(run.approval_count.to_string(), theme.fg_active),
    ]));
    if let Some(current_step_title) = &run.current_step_title {
        lines.push(Line::from(vec![
            Span::styled("Current Step: ", theme.fg_dim),
            Span::styled(current_step_title.clone(), theme.fg_active),
        ]));
    }
    if !run.goal.is_empty() {
        push_section_title(lines, "Goal Definition", theme.accent_primary.add_modifier(Modifier::BOLD));
        push_wrapped_text(lines, &run.goal, theme.fg_active, width, 0);
    }
    if let Some(last_error) = &run.last_error {
        push_section_title(lines, "Last Error", theme.accent_primary.add_modifier(Modifier::BOLD));
        push_wrapped_text(lines, last_error, theme.accent_danger, width, 0);
    }
}

fn render_steps(
    lines: &mut Vec<Line<'static>>,
    tasks: &TaskState,
    run: &GoalRun,
    selected_step_id: Option<&str>,
    theme: &ThemeTokens,
    width: usize,
) {
    push_section_title(lines, "Execution Plan", theme.accent_primary.add_modifier(Modifier::BOLD));

    let mut steps = run.steps.clone();
    steps.sort_by_key(|step| step.order);

    if steps.is_empty() {
        lines.push(Line::from(Span::styled("No steps", theme.fg_dim)));
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
        let mut header = Line::from(vec![
            Span::styled(chip, theme.fg_dim),
            Span::raw(" "),
            Span::styled(step.title.clone(), theme.fg_active),
        ]);
        if selected_step_id == Some(step.id.as_str()) {
            header = header.style(Style::default().bg(Color::Indexed(236)));
        }
        lines.push(header);

        if !step.instructions.is_empty() {
            push_wrapped_text(lines, &step.instructions, theme.fg_dim, width, 2);
        }
        if let Some(summary) = &step.summary {
            push_wrapped_text(lines, summary, theme.fg_active, width, 2);
        }
        if let Some(error) = &step.error {
            push_wrapped_text(lines, error, theme.accent_danger, width, 2);
        }

        let step_tasks = related_tasks_for_step(tasks, run, step);
        for task in step_tasks {
            lines.push(Line::from(vec![
                Span::raw("  "),
                Span::styled("• ", theme.fg_dim),
                Span::styled(task.title.clone(), theme.fg_active),
                Span::raw(" "),
                Span::styled(task_status_label(task.status), theme.fg_dim),
            ]));
            if let Some(reason) = &task.blocked_reason {
                push_wrapped_text(lines, reason, theme.accent_danger, width, 4);
            }
        }
    }
}

fn render_recent_activity(
    lines: &mut Vec<Line<'static>>,
    run: &GoalRun,
    theme: &ThemeTokens,
    width: usize,
) {
    if run.events.is_empty() {
        return;
    }

    push_section_title(lines, "Recent Activity", theme.accent_primary.add_modifier(Modifier::BOLD));

    for event in run.events.iter().rev().take(18).rev() {
        let mut prefix = format!("[{}] {}", event.phase, event.message);
        if let Some(step_index) = event.step_index {
            prefix = format!("step {} • {}", step_index + 1, prefix);
        }
        push_wrapped_text(lines, &prefix, theme.fg_active, width, 0);
        if let Some(details) = &event.details {
            push_wrapped_text(lines, details, theme.fg_dim, width, 2);
        }
    }
}

fn build_lines(
    tasks: &TaskState,
    target: &SidebarItemTarget,
    theme: &ThemeTokens,
    width: usize,
) -> (String, Vec<Line<'static>>) {
    let mut lines = Vec::new();
    let section_style = theme.accent_primary.add_modifier(Modifier::BOLD);
    let highlight_style = Style::default().bg(Color::Indexed(236));
    let rule = "─".repeat(width.saturating_sub(2).min(64).max(1));

    match target {
        SidebarItemTarget::GoalRun {
            goal_run_id,
            step_id,
        } => {
            let Some(run) = tasks.goal_run_by_id(goal_run_id) else {
                return (
                    " Goal ".to_string(),
                    vec![Line::from(Span::styled(
                        "Goal run not found",
                        theme.accent_danger,
                    ))],
                );
            };

            lines.push(Line::from(vec![
                Span::styled("ID: ", theme.fg_dim),
                Span::styled(run.id.clone(), theme.fg_active),
            ]));
            render_goal_summary(&mut lines, run, theme, width);
            render_steps(&mut lines, tasks, run, step_id.as_deref(), theme, width);

            let child_tasks: Vec<_> = tasks
                .tasks()
                .iter()
                .filter(|task| task.goal_run_id.as_deref() == Some(goal_run_id.as_str()))
                .collect();
            push_section_title(&mut lines, &rule, theme.fg_dim);
            lines.push(Line::from(Span::styled("Related Tasks", section_style)));

            if child_tasks.is_empty() {
                lines.push(Line::from(Span::styled("No tasks", theme.fg_dim)));
            } else {
                for task in child_tasks {
                    lines.push(Line::from(vec![
                        Span::styled("[ ] ", theme.fg_dim),
                        Span::styled(task.title.clone(), theme.fg_active),
                        Span::raw(" "),
                        Span::styled(task_status_label(task.status), theme.fg_dim),
                    ]));
                    if let Some(step_title) = &task.goal_step_title {
                        push_wrapped_text(&mut lines, step_title, theme.fg_dim, width, 2);
                    }
                    if let Some(blocked_reason) = &task.blocked_reason {
                        push_wrapped_text(&mut lines, blocked_reason, theme.accent_danger, width, 2);
                    }
                }
            }

            if let Some(summary) = &run.reflection_summary {
                push_section_title(&mut lines, &rule, theme.fg_dim);
                lines.push(Line::from(Span::styled("Reflection", section_style)));
                push_wrapped_text(&mut lines, summary, theme.fg_dim, width, 0);
            }

            if !run.memory_updates.is_empty() {
                push_section_title(&mut lines, &rule, theme.fg_dim);
                lines.push(Line::from(Span::styled("Memory Updates", section_style)));
                for update in &run.memory_updates {
                    push_wrapped_text(&mut lines, &format!("• {}", update), theme.fg_dim, width, 0);
                }
            }

            render_recent_activity(&mut lines, run, theme, width);

            (format!(" Goal: {} ", run.title), lines)
        }
        SidebarItemTarget::Task { task_id } => {
            let Some(task) = tasks.task_by_id(task_id) else {
                return (
                    " Task ".to_string(),
                    vec![Line::from(Span::styled(
                        "Task not found",
                        theme.accent_danger,
                    ))],
                );
            };

            lines.push(Line::from(vec![
                Span::styled("Status: ", theme.fg_dim),
                Span::styled(task_status_label(task.status), theme.fg_active),
            ]));
            lines.push(Line::from(vec![
                Span::styled("Progress: ", theme.fg_dim),
                Span::styled(format!("{}%", task.progress), theme.fg_active),
            ]));
            if let Some(session_id) = &task.session_id {
                lines.push(Line::from(vec![
                    Span::styled("Session: ", theme.fg_dim),
                    Span::styled(session_id.clone(), theme.fg_active),
                ]));
            }

            if let Some(run) = task
                .goal_run_id
                .as_deref()
                .and_then(|goal_run_id| tasks.goal_run_by_id(goal_run_id))
            {
                push_section_title(&mut lines, "Parent Goal", section_style);
                push_wrapped_text(&mut lines, &run.title, theme.fg_active, width, 0);
                if !run.goal.is_empty() {
                    push_wrapped_text(&mut lines, &run.goal, theme.fg_dim, width, 0);
                }
                if let Some(step) = run.steps.iter().find(|step| {
                    step.task_id.as_deref() == Some(task.id.as_str())
                        || task.goal_step_title.as_deref() == Some(step.title.as_str())
                }) {
                    lines.push(Line::raw(""));
                    let mut step_line = Line::from(vec![
                        Span::styled("Step: ", theme.fg_dim),
                        Span::styled(step.title.clone(), theme.fg_active),
                    ]);
                    step_line = step_line.style(highlight_style);
                    lines.push(step_line);
                    if !step.instructions.is_empty() {
                        push_wrapped_text(&mut lines, &step.instructions, theme.fg_dim, width, 2);
                    }
                    if let Some(summary) = &step.summary {
                        push_wrapped_text(&mut lines, summary, theme.fg_active, width, 2);
                    }
                    if let Some(error) = &step.error {
                        push_wrapped_text(&mut lines, error, theme.accent_danger, width, 2);
                    }
                }

                let related_events: Vec<_> = run
                    .events
                    .iter()
                    .filter(|event| {
                        task.goal_step_title
                            .as_deref()
                            .is_some_and(|step_title| run.steps.iter().enumerate().any(|(idx, step)| {
                                step.title == step_title && event.step_index == Some(idx)
                            }))
                    })
                    .collect();
                if !related_events.is_empty() {
                    push_section_title(&mut lines, "Recent Activity", section_style);
                    for event in related_events.into_iter().rev().take(10).rev() {
                        push_wrapped_text(
                            &mut lines,
                            &format!("[{}] {}", event.phase, event.message),
                            theme.fg_active,
                            width,
                            0,
                        );
                        if let Some(details) = &event.details {
                            push_wrapped_text(&mut lines, details, theme.fg_dim, width, 2);
                        }
                    }
                }
            }

            if let Some(blocked_reason) = &task.blocked_reason {
                push_section_title(&mut lines, "Blocked Reason", section_style);
                push_wrapped_text(&mut lines, blocked_reason, theme.accent_danger, width, 0);
            }

            (format!(" Task: {} ", task.title), lines)
        }
    }
}

pub fn render(
    frame: &mut Frame,
    area: Rect,
    tasks: &TaskState,
    target: &SidebarItemTarget,
    theme: &ThemeTokens,
    focused: bool,
    scroll: usize,
) {
    let border_style = if focused {
        theme.accent_primary
    } else {
        theme.fg_dim
    };
    let title_style = if focused { theme.fg_active } else { theme.fg_dim };
    let (title, lines) = build_lines(tasks, target, theme, area.width as usize);
    let block = Block::default()
        .title(Span::styled(title, title_style))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(border_style);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.width == 0 || inner.height == 0 {
        return;
    }

    let max_scroll = lines.len().saturating_sub(inner.height as usize);
    let paragraph = Paragraph::new(lines).scroll((scroll.min(max_scroll) as u16, 0));
    frame.render_widget(paragraph, inner);
}
