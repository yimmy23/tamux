#![allow(dead_code)]

use crate::state::goal_sidebar::{GoalSidebarState, GoalSidebarTab};
use crate::state::task::{
    AgentTask, GoalRun, GoalRunCheckpointSummary, GoalRunStatus, GoalRunStep, TaskState,
    WorkContextEntry,
};
use crate::theme::ThemeTokens;
use ratatui::prelude::*;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

#[derive(Clone)]
struct GoalSidebarRow {
    line: Line<'static>,
    target: Option<GoalSidebarHitTarget>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GoalSidebarHitTarget {
    Tab(GoalSidebarTab),
    Step(usize),
    Checkpoint(usize),
    Task(usize),
    File(usize),
}

pub fn render(
    frame: &mut Frame,
    area: Rect,
    tasks: &TaskState,
    goal_run_id: &str,
    state: &GoalSidebarState,
    theme: &ThemeTokens,
) {
    if area.width == 0 || area.height < 2 {
        return;
    }

    let (tab_area, body_area) = split_area(area);
    let active_tab = state.active_tab();
    render_tabs(frame, tab_area, active_tab, theme);

    let rows = rows_for_tab(
        tasks,
        goal_run_id,
        active_tab,
        state.selected_row(),
        body_area.width as usize,
        theme,
    );
    let scroll = resolved_scroll(&rows, state, body_area.height as usize);
    let paragraph = Paragraph::new(rows.into_iter().map(|row| row.line).collect::<Vec<_>>())
        .scroll((scroll as u16, 0));
    frame.render_widget(paragraph, body_area);
}

pub fn item_count(tasks: &TaskState, goal_run_id: &str, state: &GoalSidebarState) -> usize {
    rows_for_tab(
        tasks,
        goal_run_id,
        state.active_tab(),
        state.selected_row(),
        80,
        &ThemeTokens::default(),
    )
    .len()
    .max(1)
}

pub fn hit_test(
    area: Rect,
    tasks: &TaskState,
    goal_run_id: &str,
    state: &GoalSidebarState,
    mouse: Position,
) -> Option<GoalSidebarHitTarget> {
    if area.width == 0
        || area.height < 2
        || mouse.x < area.x
        || mouse.x >= area.x.saturating_add(area.width)
        || mouse.y < area.y
        || mouse.y >= area.y.saturating_add(area.height)
    {
        return None;
    }

    let (tab_area, body_area) = split_area(area);
    if mouse.y == tab_area.y {
        return tab_hit_test(tab_area, mouse.x).map(GoalSidebarHitTarget::Tab);
    }

    let rows = rows_for_tab(
        tasks,
        goal_run_id,
        state.active_tab(),
        state.selected_row(),
        body_area.width as usize,
        &ThemeTokens::default(),
    );
    let scroll = resolved_scroll(&rows, state, body_area.height as usize);
    let row_idx = scroll + mouse.y.saturating_sub(body_area.y) as usize;
    rows.get(row_idx).and_then(|row| row.target.clone())
}

fn split_area(area: Rect) -> (Rect, Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(1)])
        .split(area);
    (chunks[0], chunks[1])
}

fn render_tabs(frame: &mut Frame, area: Rect, active_tab: GoalSidebarTab, theme: &ThemeTokens) {
    for (tab, cell) in tab_cells(area) {
        let style = if tab == active_tab {
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
}

fn tab_cells(area: Rect) -> Vec<(GoalSidebarTab, Rect)> {
    let tabs = goal_tabs();
    let percent = 100 / tabs.len() as u16;
    let mut constraints = vec![Constraint::Percentage(percent); tabs.len()];
    if let Some(last) = constraints.last_mut() {
        *last = Constraint::Min(0);
    }
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(constraints)
        .split(area);
    tabs.into_iter().zip(chunks.iter().copied()).collect()
}

fn tab_hit_test(area: Rect, mouse_x: u16) -> Option<GoalSidebarTab> {
    tab_cells(area).into_iter().find_map(|(tab, rect)| {
        (mouse_x >= rect.x && mouse_x < rect.x.saturating_add(rect.width)).then_some(tab)
    })
}

fn goal_tabs() -> [GoalSidebarTab; 4] {
    [
        GoalSidebarTab::Steps,
        GoalSidebarTab::Checkpoints,
        GoalSidebarTab::Tasks,
        GoalSidebarTab::Files,
    ]
}

fn tab_label(tab: GoalSidebarTab) -> &'static str {
    match tab {
        GoalSidebarTab::Steps => "Steps",
        GoalSidebarTab::Checkpoints => "Checkpoints",
        GoalSidebarTab::Tasks => "Tasks",
        GoalSidebarTab::Files => "Files",
    }
}

fn rows_for_tab(
    tasks: &TaskState,
    goal_run_id: &str,
    tab: GoalSidebarTab,
    selected_row: usize,
    width: usize,
    theme: &ThemeTokens,
) -> Vec<GoalSidebarRow> {
    let selected_style = Style::default().bg(Color::Indexed(236));
    let selected = selected_row;

    let rows: Vec<GoalSidebarRow> = match tab {
        GoalSidebarTab::Steps => goal_steps(tasks, goal_run_id)
            .into_iter()
            .enumerate()
            .map(|(idx, step)| {
                let line = Line::from(vec![
                    Span::styled(
                        if idx == selected { "> " } else { "  " },
                        theme.accent_primary,
                    ),
                    Span::styled(format!("[{}] ", step.order + 1), theme.fg_dim),
                    Span::styled(step.title.clone(), theme.fg_active),
                    if let Some(status) = step.status {
                        Span::styled(format!("  {}", goal_run_status_label(status)), theme.fg_dim)
                    } else {
                        Span::raw("")
                    },
                ]);
                GoalSidebarRow {
                    line: if idx == selected {
                        line.style(selected_style)
                    } else {
                        line
                    },
                    target: Some(GoalSidebarHitTarget::Step(idx)),
                }
            })
            .collect(),
        GoalSidebarTab::Checkpoints => goal_checkpoints(tasks, goal_run_id)
            .into_iter()
            .enumerate()
            .map(|(idx, checkpoint)| {
                let label = checkpoint
                    .context_summary_preview
                    .as_deref()
                    .unwrap_or_else(|| checkpoint.checkpoint_type.as_str());
                let line = Line::from(vec![
                    Span::styled(
                        if idx == selected { "> " } else { "  " },
                        theme.accent_primary,
                    ),
                    Span::styled(format!("[{}] ", checkpoint.checkpoint_type), theme.fg_dim),
                    Span::styled(label.to_string(), theme.fg_active),
                ]);
                GoalSidebarRow {
                    line: if idx == selected {
                        line.style(selected_style)
                    } else {
                        line
                    },
                    target: Some(GoalSidebarHitTarget::Checkpoint(idx)),
                }
            })
            .collect(),
        GoalSidebarTab::Tasks => goal_tasks(tasks, goal_run_id)
            .into_iter()
            .enumerate()
            .map(|(idx, task)| {
                let title = truncate_tail(&task.title, width.saturating_sub(10).max(8));
                let line = Line::from(vec![
                    Span::styled(
                        if idx == selected { "> " } else { "  " },
                        theme.accent_primary,
                    ),
                    Span::styled(task_status_label(task.status).to_string(), theme.fg_dim),
                    Span::raw(" "),
                    Span::styled(title, theme.fg_active),
                ]);
                GoalSidebarRow {
                    line: if idx == selected {
                        line.style(selected_style)
                    } else {
                        line
                    },
                    target: Some(GoalSidebarHitTarget::Task(idx)),
                }
            })
            .collect(),
        GoalSidebarTab::Files => goal_files(tasks, goal_run_id)
            .into_iter()
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
                let line = Line::from(vec![
                    Span::styled(
                        if idx == selected { "> " } else { "  " },
                        theme.accent_primary,
                    ),
                    Span::styled(format!("[{}] ", label), theme.fg_dim),
                    Span::styled(
                        truncate_tail(&entry.path, width.saturating_sub(12).max(8)),
                        theme.fg_active,
                    ),
                ]);
                GoalSidebarRow {
                    line: if idx == selected {
                        line.style(selected_style)
                    } else {
                        line
                    },
                    target: Some(GoalSidebarHitTarget::File(idx)),
                }
            })
            .collect(),
    };

    if rows.is_empty() {
        empty_row(theme, tab)
    } else {
        rows
    }
}

fn empty_row(theme: &ThemeTokens, tab: GoalSidebarTab) -> Vec<GoalSidebarRow> {
    let message = match tab {
        GoalSidebarTab::Steps => "No steps",
        GoalSidebarTab::Checkpoints => "No checkpoints",
        GoalSidebarTab::Tasks => "No tasks",
        GoalSidebarTab::Files => "No files",
    };
    vec![GoalSidebarRow {
        line: Line::from(Span::styled(format!("  {}", message), theme.fg_dim)),
        target: None,
    }]
}

fn goal_run<'a>(tasks: &'a TaskState, goal_run_id: &str) -> Option<&'a GoalRun> {
    tasks.goal_run_by_id(goal_run_id)
}

fn goal_steps<'a>(tasks: &'a TaskState, goal_run_id: &'a str) -> Vec<&'a GoalRunStep> {
    goal_run(tasks, goal_run_id)
        .map(|run| {
            let mut steps: Vec<_> = run.steps.iter().collect();
            steps.sort_by_key(|step| step.order);
            steps
        })
        .unwrap_or_default()
}

fn goal_checkpoints(tasks: &TaskState, goal_run_id: &str) -> Vec<GoalRunCheckpointSummary> {
    tasks.checkpoints_for_goal_run(goal_run_id).to_vec()
}

fn goal_tasks<'a>(tasks: &'a TaskState, goal_run_id: &'a str) -> Vec<&'a AgentTask> {
    let Some(run) = goal_run(tasks, goal_run_id) else {
        return Vec::new();
    };

    if !run.child_task_ids.is_empty() {
        return run
            .child_task_ids
            .iter()
            .filter_map(|task_id| tasks.task_by_id(task_id))
            .collect();
    }

    tasks
        .tasks()
        .iter()
        .filter(|task| task.goal_run_id.as_deref() == Some(goal_run_id))
        .collect()
}

fn goal_files<'a>(tasks: &'a TaskState, goal_run_id: &'a str) -> Vec<&'a WorkContextEntry> {
    let Some(run) = goal_run(tasks, goal_run_id) else {
        return Vec::new();
    };
    let Some(thread_id) = run.thread_id.as_deref() else {
        return Vec::new();
    };
    let Some(context) = tasks.work_context_for_thread(thread_id) else {
        return Vec::new();
    };

    context
        .entries
        .iter()
        .filter(|entry| match entry.goal_run_id.as_deref() {
            Some(entry_goal_run_id) => entry_goal_run_id == goal_run_id,
            None => true,
        })
        .collect()
}

fn goal_run_status_label(status: GoalRunStatus) -> &'static str {
    match status {
        GoalRunStatus::Queued => "queued",
        GoalRunStatus::Planning => "planning",
        GoalRunStatus::Running => "running",
        GoalRunStatus::AwaitingApproval => "awaiting approval",
        GoalRunStatus::Paused => "paused",
        GoalRunStatus::Completed => "completed",
        GoalRunStatus::Failed => "failed",
        GoalRunStatus::Cancelled => "cancelled",
    }
}

fn task_status_label(status: Option<crate::state::task::TaskStatus>) -> &'static str {
    match status {
        Some(crate::state::task::TaskStatus::Queued) => "[queued]",
        Some(crate::state::task::TaskStatus::InProgress) => "[running]",
        Some(crate::state::task::TaskStatus::AwaitingApproval) => "[awaiting]",
        Some(crate::state::task::TaskStatus::Blocked) => "[blocked]",
        Some(crate::state::task::TaskStatus::FailedAnalyzing) => "[analyzing]",
        Some(crate::state::task::TaskStatus::BudgetExceeded) => "[budget]",
        Some(crate::state::task::TaskStatus::Completed) => "[done]",
        Some(crate::state::task::TaskStatus::Failed) => "[failed]",
        Some(crate::state::task::TaskStatus::Cancelled) => "[cancelled]",
        None => "[task]",
    }
}

fn truncate_tail(text: &str, max_len: usize) -> String {
    if text.chars().count() <= max_len {
        return text.to_string();
    }

    let take = max_len.saturating_sub(1);
    let mut truncated = text.chars().take(take).collect::<String>();
    truncated.push('…');
    truncated
}

fn resolved_scroll(rows: &[GoalSidebarRow], state: &GoalSidebarState, body_height: usize) -> usize {
    let max_scroll = rows.len().saturating_sub(body_height);
    let mut scroll = state.scroll_offset().min(max_scroll);
    let selected = state.selected_row().min(rows.len().saturating_sub(1));
    if selected < scroll {
        scroll = selected;
    } else if selected >= scroll.saturating_add(body_height) {
        scroll = selected.saturating_add(1).saturating_sub(body_height);
    }
    scroll.min(max_scroll)
}

#[cfg(test)]
#[path = "tests/goal_sidebar.rs"]
mod tests;
