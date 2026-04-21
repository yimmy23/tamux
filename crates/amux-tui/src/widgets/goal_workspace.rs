#![allow(dead_code)]

use crate::state::goal_workspace::GoalWorkspaceState;
use crate::state::task::TaskState;
use crate::theme::ThemeTokens;
use crate::widgets::chat::SelectionPoint;
use ratatui::prelude::*;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use unicode_width::UnicodeWidthChar;

#[path = "goal_workspace_plan.rs"]
mod plan;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GoalWorkspaceHitTarget {
    PlanStep(String),
    PlanTodo { step_id: String, todo_id: String },
    TimelineRow(usize),
    DetailFile(String),
    DetailCheckpoint(String),
}

pub fn render(
    frame: &mut Frame,
    area: Rect,
    tasks: &TaskState,
    goal_run_id: &str,
    state: &GoalWorkspaceState,
    theme: &ThemeTokens,
) {
    if area.width < 3 || area.height < 6 {
        return;
    }

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(4), Constraint::Min(1)])
        .split(area);
    render_summary(frame, layout[0], theme);

    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(40),
            Constraint::Percentage(32),
            Constraint::Min(24),
        ])
        .split(layout[1]);

    render_plan(frame, columns[0], tasks, goal_run_id, state, theme);
    render_timeline(frame, columns[1], tasks, goal_run_id, theme);
    render_details(frame, columns[2], tasks, goal_run_id, state, theme);
}

pub fn hit_test(
    area: Rect,
    tasks: &TaskState,
    goal_run_id: &str,
    state: &GoalWorkspaceState,
    mouse: Position,
) -> Option<GoalWorkspaceHitTarget> {
    if area.width < 3
        || area.height < 6
        || mouse.x < area.x
        || mouse.x >= area.x.saturating_add(area.width)
        || mouse.y < area.y
        || mouse.y >= area.y.saturating_add(area.height)
    {
        return None;
    }

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(4), Constraint::Min(1)])
        .split(area);
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(40),
            Constraint::Percentage(32),
            Constraint::Min(24),
        ])
        .split(layout[1]);

    let plan_area = columns[0];
    if mouse.x < plan_area.x || mouse.x >= plan_area.x.saturating_add(plan_area.width) {
        return None;
    }
    if mouse.y < plan_area.y || mouse.y >= plan_area.y.saturating_add(plan_area.height) {
        return None;
    }

    let inner = Block::default().borders(Borders::ALL).inner(plan_area);
    if mouse.x < inner.x
        || mouse.x >= inner.x.saturating_add(inner.width)
        || mouse.y < inner.y
        || mouse.y >= inner.y.saturating_add(inner.height)
    {
        return None;
    }

    let rows = plan::build_rows(tasks, goal_run_id, state);
    let row_index = resolved_plan_scroll(rows.len(), inner.height as usize, state)
        .saturating_add(mouse.y.saturating_sub(inner.y) as usize);
    rows.get(row_index).and_then(|row| row.target.clone())
}

pub fn max_plan_scroll(
    area: Rect,
    tasks: &TaskState,
    goal_run_id: &str,
    state: &GoalWorkspaceState,
) -> usize {
    if area.width < 3 || area.height < 6 {
        return 0;
    }

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(4), Constraint::Min(1)])
        .split(area);
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(40),
            Constraint::Percentage(32),
            Constraint::Min(24),
        ])
        .split(layout[1]);
    let inner = Block::default().borders(Borders::ALL).inner(columns[0]);
    let rows = plan::build_rows(tasks, goal_run_id, state);
    rows.len().saturating_sub(inner.height as usize)
}

pub fn selection_point_from_mouse(
    area: Rect,
    tasks: &TaskState,
    goal_run_id: &str,
    state: &GoalWorkspaceState,
    mouse: Position,
) -> Option<SelectionPoint> {
    let (inner, row_index) = plan_inner_row_index(area, tasks, goal_run_id, state, mouse)?;
    let rows = plan::build_rows(tasks, goal_run_id, state);
    let line = &rows.get(row_index)?.line;
    let width = line_display_width(line);
    let col = mouse.x.saturating_sub(inner.x) as usize;
    Some(SelectionPoint {
        row: row_index,
        col: col.min(width),
    })
}

pub fn selected_text(
    _area: Rect,
    tasks: &TaskState,
    goal_run_id: &str,
    state: &GoalWorkspaceState,
    start: SelectionPoint,
    end: SelectionPoint,
) -> Option<String> {
    let rows = plan::build_rows(tasks, goal_run_id, state);
    let (start_point, end_point) =
        if start.row <= end.row || (start.row == end.row && start.col <= end.col) {
            (start, end)
        } else {
            (end, start)
        };
    if start_point == end_point {
        return None;
    }

    let mut lines = Vec::new();
    for row in start_point.row..=end_point.row {
        let line = &rows.get(row)?.line;
        let plain = line_plain_text(line);
        let width = line_display_width(line);
        let from = if row == start_point.row {
            start_point.col.min(width)
        } else {
            0
        };
        let to = if row == end_point.row {
            end_point.col.min(width).max(from)
        } else {
            width
        };
        lines.push(display_slice(&plain, from, to));
    }

    let text = lines.join("\n");
    if text.is_empty() {
        None
    } else {
        Some(text)
    }
}

fn render_summary(frame: &mut Frame, area: Rect, theme: &ThemeTokens) {
    let block = Block::default()
        .title(" Goal Mission Control ")
        .borders(Borders::ALL);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let text = Line::from(vec![
        Span::styled("Goal", theme.accent_secondary),
        Span::styled("  Progress  Active agent  Needs attention", theme.fg_dim),
    ]);
    frame.render_widget(Paragraph::new(text), inner);
}

fn render_plan(
    frame: &mut Frame,
    area: Rect,
    tasks: &TaskState,
    goal_run_id: &str,
    state: &GoalWorkspaceState,
    _theme: &ThemeTokens,
) {
    let block = Block::default().title(" Plan ").borders(Borders::ALL);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let selected_style = Style::default().bg(Color::Indexed(236));
    let lines = plan::build_rows(tasks, goal_run_id, state)
        .into_iter()
        .enumerate()
        .map(|(index, row)| {
            if index == state.selected_plan_row() {
                row.line.style(selected_style)
            } else {
                row.line
            }
        })
        .collect::<Vec<_>>();
    let scroll = resolved_plan_scroll(lines.len(), inner.height as usize, state);
    frame.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .scroll((scroll.min(u16::MAX as usize) as u16, 0)),
        inner,
    );
}

fn render_placeholder(frame: &mut Frame, area: Rect, title: &str, body: &str, theme: &ThemeTokens) {
    let block = Block::default().title(title).borders(Borders::ALL);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    frame.render_widget(
        Paragraph::new(body)
            .style(theme.fg_dim)
            .wrap(Wrap { trim: false }),
        inner,
    );
}

fn render_timeline(
    frame: &mut Frame,
    area: Rect,
    tasks: &TaskState,
    goal_run_id: &str,
    theme: &ThemeTokens,
) {
    let block = Block::default()
        .title(" Run timeline ")
        .borders(Borders::ALL);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let mut lines = Vec::new();
    if let Some(run) = tasks.goal_run_by_id(goal_run_id) {
        if run.events.is_empty() {
            lines.push(Line::from(Span::styled(
                "Waiting for run events.",
                theme.fg_dim,
            )));
        } else {
            for event in run.events.iter().rev().take(inner.height as usize) {
                let label = if event.message.trim().is_empty() {
                    "event".to_string()
                } else {
                    event.message.clone()
                };
                lines.push(Line::from(vec![
                    Span::styled("• ", theme.accent_secondary),
                    Span::styled(label, theme.fg_active),
                ]));
            }
        }
    }

    if lines.is_empty() {
        lines.push(Line::from(Span::styled(
            "No timeline available.",
            theme.fg_dim,
        )));
    }
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
}

fn render_details(
    frame: &mut Frame,
    area: Rect,
    tasks: &TaskState,
    goal_run_id: &str,
    state: &GoalWorkspaceState,
    theme: &ThemeTokens,
) {
    let block = Block::default().title(" Details ").borders(Borders::ALL);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let selected_step = state
        .selected_plan_item()
        .and_then(|selection| match selection {
            crate::state::goal_workspace::GoalPlanSelection::Step { step_id }
            | crate::state::goal_workspace::GoalPlanSelection::Todo { step_id, .. } => {
                Some(step_id.as_str())
            }
        })
        .and_then(|step_id| {
            tasks
                .goal_steps_in_display_order(goal_run_id)
                .into_iter()
                .find(|step| step.id == step_id)
        })
        .or_else(|| {
            tasks
                .goal_steps_in_display_order(goal_run_id)
                .into_iter()
                .next()
        });

    let mut lines = Vec::new();
    if let Some(step) = selected_step {
        lines.push(Line::from(vec![
            Span::styled("Selected ", theme.fg_dim),
            Span::styled(step.title.clone(), theme.fg_active),
        ]));
        for checkpoint in tasks
            .goal_step_checkpoints(goal_run_id, step.order as usize)
            .into_iter()
            .take(2)
        {
            lines.push(Line::from(vec![
                Span::styled("checkpoint ", theme.fg_dim),
                Span::styled(checkpoint.checkpoint_type.clone(), theme.fg_active),
            ]));
        }
        if let Some(run) = tasks.goal_run_by_id(goal_run_id) {
            if let Some(thread_id) = run.thread_id.as_deref() {
                for entry in tasks
                    .goal_step_files(goal_run_id, thread_id, step.order as usize)
                    .into_iter()
                    .take(2)
                {
                    lines.push(Line::from(vec![
                        Span::styled("file ", theme.fg_dim),
                        Span::styled(entry.path.clone(), theme.fg_active),
                    ]));
                }
            }
        }
    }

    if lines.is_empty() {
        lines.push(Line::from(Span::styled(
            "No step details selected.",
            theme.fg_dim,
        )));
    }
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
}

fn resolved_plan_scroll(
    row_count: usize,
    viewport_height: usize,
    state: &GoalWorkspaceState,
) -> usize {
    row_count
        .saturating_sub(viewport_height)
        .min(state.plan_scroll())
}

fn plan_inner_row_index(
    area: Rect,
    tasks: &TaskState,
    goal_run_id: &str,
    state: &GoalWorkspaceState,
    mouse: Position,
) -> Option<(Rect, usize)> {
    if area.width < 3
        || area.height < 6
        || mouse.x < area.x
        || mouse.x >= area.x.saturating_add(area.width)
        || mouse.y < area.y
        || mouse.y >= area.y.saturating_add(area.height)
    {
        return None;
    }

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(4), Constraint::Min(1)])
        .split(area);
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(40),
            Constraint::Percentage(32),
            Constraint::Min(24),
        ])
        .split(layout[1]);

    let plan_area = columns[0];
    if mouse.x < plan_area.x
        || mouse.x >= plan_area.x.saturating_add(plan_area.width)
        || mouse.y < plan_area.y
        || mouse.y >= plan_area.y.saturating_add(plan_area.height)
    {
        return None;
    }

    let inner = Block::default().borders(Borders::ALL).inner(plan_area);
    if mouse.x < inner.x
        || mouse.x >= inner.x.saturating_add(inner.width)
        || mouse.y < inner.y
        || mouse.y >= inner.y.saturating_add(inner.height)
    {
        return None;
    }

    let rows = plan::build_rows(tasks, goal_run_id, state);
    let row_index = resolved_plan_scroll(rows.len(), inner.height as usize, state)
        .saturating_add(mouse.y.saturating_sub(inner.y) as usize);
    if row_index >= rows.len() {
        return None;
    }
    Some((inner, row_index))
}

fn line_plain_text(line: &Line<'static>) -> String {
    line.spans
        .iter()
        .map(|span| span.content.as_ref())
        .collect()
}

fn line_display_width(line: &Line<'static>) -> usize {
    line_plain_text(line)
        .chars()
        .map(|ch| UnicodeWidthChar::width(ch).unwrap_or(0))
        .sum()
}

fn display_slice(text: &str, start_col: usize, end_col: usize) -> String {
    if start_col >= end_col {
        return String::new();
    }

    let mut result = String::new();
    let mut col = 0usize;
    for ch in text.chars() {
        let width = UnicodeWidthChar::width(ch).unwrap_or(0);
        let next = col + width;
        let overlaps = if width == 0 {
            col >= start_col && col < end_col
        } else {
            next > start_col && col < end_col
        };
        if overlaps {
            result.push(ch);
        }
        col = next;
        if col >= end_col {
            break;
        }
    }
    result
}

#[cfg(test)]
#[path = "tests/goal_workspace.rs"]
mod tests;
