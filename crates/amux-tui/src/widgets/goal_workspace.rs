#![allow(dead_code)]

use crate::state::goal_workspace::{GoalWorkspaceMode, GoalWorkspacePane, GoalWorkspaceState};
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
    ModeTab(GoalWorkspaceMode),
    PlanPromptToggle,
    PlanMainThread(String),
    PlanStep(String),
    PlanTodo { step_id: String, todo_id: String },
    TimelineRow(usize),
    ThreadRow(String),
    FooterAction(GoalWorkspaceAction),
    DetailFile(String),
    DetailCheckpoint(String),
    DetailTask(String),
    DetailThread(String),
    DetailAction(GoalWorkspaceAction),
    DetailTimelineDetails(usize),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GoalWorkspaceAction {
    ToggleGoalRun,
    OpenActions,
    RetryStep,
    RerunFromStep,
}

#[derive(Clone, Copy)]
struct GoalWorkspaceLayoutRects {
    summary: Rect,
    footer: Rect,
    plan: Rect,
    timeline: Rect,
    details: Rect,
}

fn workspace_layout(area: Rect) -> Option<GoalWorkspaceLayoutRects> {
    if area.width < 3 || area.height < 8 {
        return None;
    }

    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(4),
            Constraint::Min(1),
            Constraint::Length(3),
        ])
        .split(area);
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(40),
            Constraint::Percentage(32),
            Constraint::Min(24),
        ])
        .split(sections[1]);
    Some(GoalWorkspaceLayoutRects {
        summary: sections[0],
        footer: sections[2],
        plan: columns[0],
        timeline: columns[1],
        details: columns[2],
    })
}

pub fn render(
    frame: &mut Frame,
    area: Rect,
    tasks: &TaskState,
    goal_run_id: &str,
    state: &GoalWorkspaceState,
    theme: &ThemeTokens,
    tick_counter: u64,
) {
    let Some(layout) = workspace_layout(area) else {
        return;
    };

    render_summary(frame, layout.summary, state, theme);
    render_plan(frame, layout.plan, tasks, goal_run_id, state, theme);
    render_center_pane(
        frame,
        layout.timeline,
        tasks,
        goal_run_id,
        state,
        theme,
        tick_counter,
    );
    render_details(frame, layout.details, tasks, goal_run_id, state, theme);
    render_step_footer(frame, layout.footer, tasks, goal_run_id, state, theme);
}

pub fn pane_at(area: Rect, mouse: Position) -> Option<GoalWorkspacePane> {
    let Some(layout) = workspace_layout(area) else {
        return None;
    };
    if mouse.x < area.x
        || mouse.x >= area.x.saturating_add(area.width)
        || mouse.y < area.y
        || mouse.y >= area.y.saturating_add(area.height)
    {
        return None;
    }

    if rect_contains(layout.summary, mouse) {
        return Some(GoalWorkspacePane::CommandBar);
    }

    if rect_contains(layout.plan, mouse) {
        Some(GoalWorkspacePane::Plan)
    } else if rect_contains(layout.timeline, mouse) {
        Some(GoalWorkspacePane::Timeline)
    } else if rect_contains(layout.details, mouse) {
        Some(GoalWorkspacePane::Details)
    } else {
        None
    }
}

pub fn hit_test(
    area: Rect,
    tasks: &TaskState,
    goal_run_id: &str,
    state: &GoalWorkspaceState,
    mouse: Position,
) -> Option<GoalWorkspaceHitTarget> {
    let Some(layout) = workspace_layout(area) else {
        return None;
    };
    if mouse.x < area.x
        || mouse.x >= area.x.saturating_add(area.width)
        || mouse.y < area.y
        || mouse.y >= area.y.saturating_add(area.height)
    {
        return None;
    }

    if let Some(tab_hit) = summary_hit_test(layout.summary, mouse) {
        return Some(tab_hit);
    }
    if let Some(footer_hit) = footer_hit_test(layout.footer, tasks, goal_run_id, state, mouse) {
        return Some(footer_hit);
    }

    match pane_at(area, mouse)? {
        GoalWorkspacePane::Plan => {
            let plan_area = layout.plan;
            let inner = Block::default().borders(Borders::ALL).inner(plan_area);
            if !rect_contains(inner, mouse) {
                return None;
            }

            let rows = plan::build_rows(tasks, goal_run_id, state);
            let row_index = resolved_plan_scroll(rows.len(), inner.height as usize, state)
                .saturating_add(mouse.y.saturating_sub(inner.y) as usize);
            rows.get(row_index).and_then(|row| row.target.clone())
        }
        GoalWorkspacePane::Timeline => {
            let inner = Block::default().borders(Borders::ALL).inner(layout.timeline);
            if !rect_contains(inner, mouse) {
                return None;
            }
            let row_index = resolved_timeline_scroll(
                center_rows(
                    tasks,
                    goal_run_id,
                    state,
                    inner.width as usize,
                    &ThemeTokens::default(),
                    0,
                )
                .len(),
                inner.height as usize,
                state,
            )
            .saturating_add(mouse.y.saturating_sub(inner.y) as usize);
            center_rows(
                tasks,
                goal_run_id,
                state,
                inner.width as usize,
                &ThemeTokens::default(),
                0,
            )
                .into_iter()
                .nth(row_index)
                .and_then(|row| row.target)
        }
        GoalWorkspacePane::Details => {
            let inner = Block::default().borders(Borders::ALL).inner(layout.details);
            if !rect_contains(inner, mouse) {
                return None;
            }
            let row_index = resolved_detail_scroll(
                detail_lines(tasks, goal_run_id, state, inner.width as usize, &ThemeTokens::default())
                    .len(),
                inner.height as usize,
                state,
            )
            .saturating_add(mouse.y.saturating_sub(inner.y) as usize);
            detail_lines(tasks, goal_run_id, state, inner.width as usize, &ThemeTokens::default())
                .into_iter()
                .find_map(|(index, target, _)| (index == row_index).then_some(target))
                .flatten()
        }
        GoalWorkspacePane::CommandBar => None,
    }
}

pub fn max_plan_scroll(
    area: Rect,
    tasks: &TaskState,
    goal_run_id: &str,
    state: &GoalWorkspaceState,
) -> usize {
    let Some(layout) = workspace_layout(area) else {
        return 0;
    };
    let inner = Block::default().borders(Borders::ALL).inner(layout.plan);
    let rows = plan::build_rows(tasks, goal_run_id, state);
    rows.len().saturating_sub(inner.height as usize)
}

pub fn timeline_row_count(
    tasks: &TaskState,
    goal_run_id: &str,
    state: &GoalWorkspaceState,
) -> usize {
    center_targets(tasks, goal_run_id, state).len()
}

pub fn max_timeline_scroll(
    area: Rect,
    tasks: &TaskState,
    goal_run_id: &str,
    state: &GoalWorkspaceState,
) -> usize {
    let Some(layout) = workspace_layout(area) else {
        return 0;
    };
    let inner = Block::default().borders(Borders::ALL).inner(layout.timeline);
    center_rows(
        tasks,
        goal_run_id,
        state,
        inner.width as usize,
        &ThemeTokens::default(),
        0,
    )
        .len()
        .saturating_sub(inner.height as usize)
}

pub fn detail_target_count(tasks: &TaskState, goal_run_id: &str, state: &GoalWorkspaceState) -> usize {
    detail_targets(tasks, goal_run_id, state).len()
}

pub fn max_detail_scroll(
    area: Rect,
    tasks: &TaskState,
    goal_run_id: &str,
    state: &GoalWorkspaceState,
) -> usize {
    let Some(layout) = workspace_layout(area) else {
        return 0;
    };
    let inner = Block::default().borders(Borders::ALL).inner(layout.details);
    detail_lines(tasks, goal_run_id, state, inner.width as usize, &ThemeTokens::default())
        .len()
        .saturating_sub(inner.height as usize)
}

pub fn timeline_viewport_height(area: Rect) -> usize {
    let Some(layout) = workspace_layout(area) else {
        return 0;
    };
    Block::default().borders(Borders::ALL).inner(layout.timeline).height as usize
}

pub fn detail_viewport_height(area: Rect) -> usize {
    let Some(layout) = workspace_layout(area) else {
        return 0;
    };
    Block::default().borders(Borders::ALL).inner(layout.details).height as usize
}

pub fn detail_row_for_target(
    tasks: &TaskState,
    goal_run_id: &str,
    state: &GoalWorkspaceState,
    target: &GoalWorkspaceHitTarget,
) -> Option<usize> {
    detail_targets(tasks, goal_run_id, state)
        .into_iter()
        .position(|(_, candidate)| candidate == *target)
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

const MODE_TABS: &[(GoalWorkspaceMode, &str)] = &[
    (GoalWorkspaceMode::Goal, "Goal"),
    (GoalWorkspaceMode::Progress, "Progress"),
    (GoalWorkspaceMode::ActiveAgent, "Active agent"),
    (GoalWorkspaceMode::Threads, "Threads"),
    (GoalWorkspaceMode::NeedsAttention, "Needs attention"),
];

fn render_summary(
    frame: &mut Frame,
    area: Rect,
    state: &GoalWorkspaceState,
    theme: &ThemeTokens,
) {
    let block = Block::default()
        .title(" Goal Mission Control ")
        .borders(Borders::ALL)
        .border_style(if state.focused_pane() == GoalWorkspacePane::CommandBar {
            theme.accent_primary
        } else {
            theme.fg_dim
        });
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let mut spans = Vec::new();
    for (index, (mode, label)) in MODE_TABS.iter().enumerate() {
        if index > 0 {
            spans.push(Span::raw("  "));
        }
        let style = if state.mode() == *mode {
            theme.accent_secondary
        } else {
            theme.fg_dim
        };
        spans.push(Span::styled(*label, style));
    }
    let text = Line::from(spans);
    frame.render_widget(Paragraph::new(text), inner);
}

fn summary_hit_test(area: Rect, mouse: Position) -> Option<GoalWorkspaceHitTarget> {
    let inner = Block::default().borders(Borders::ALL).inner(area);
    if !rect_contains(inner, mouse) || mouse.y != inner.y {
        return None;
    }

    let mut x = inner.x;
    for (index, (mode, label)) in MODE_TABS.iter().enumerate() {
        if index > 0 {
            x = x.saturating_add(2);
        }
        let width = label.chars().count() as u16;
        if mouse.x >= x && mouse.x < x.saturating_add(width) {
            return Some(GoalWorkspaceHitTarget::ModeTab(*mode));
        }
        x = x.saturating_add(width);
    }
    None
}

fn render_plan(
    frame: &mut Frame,
    area: Rect,
    tasks: &TaskState,
    goal_run_id: &str,
    state: &GoalWorkspaceState,
    _theme: &ThemeTokens,
) {
    let block = Block::default()
        .title(" Plan ")
        .borders(Borders::ALL)
        .border_style(if state.focused_pane() == GoalWorkspacePane::Plan {
            _theme.accent_primary
        } else {
            _theme.fg_dim
        });
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let selected_style = selected_row_style(state.focused_pane() == GoalWorkspacePane::Plan);
    let selected_visual_row = plan_visual_row_for_selection(tasks, goal_run_id, state);
    let lines = plan::build_rows(tasks, goal_run_id, state)
        .into_iter()
        .enumerate()
        .map(|(index, row)| {
            if Some(index) == selected_visual_row {
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

fn render_center_pane(
    frame: &mut Frame,
    area: Rect,
    tasks: &TaskState,
    goal_run_id: &str,
    state: &GoalWorkspaceState,
    theme: &ThemeTokens,
    tick_counter: u64,
) {
    let block = Block::default()
        .title(center_pane_title(state.mode()))
        .borders(Borders::ALL)
        .border_style(if state.focused_pane() == GoalWorkspacePane::Timeline {
            theme.accent_primary
        } else {
            theme.fg_dim
        });
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let selected_target = center_targets(tasks, goal_run_id, state)
        .get(state.selected_timeline_row())
        .cloned();
    let center_rows = center_rows(
        tasks,
        goal_run_id,
        state,
        inner.width as usize,
        theme,
        tick_counter,
    );
    let mut lines = center_rows.iter().map(|row| row.line.clone()).collect::<Vec<_>>();
    if lines.is_empty() {
        lines.push(Line::from(Span::styled("No data available.", theme.fg_dim)));
    }
    let selected_style = selected_row_style(state.focused_pane() == GoalWorkspacePane::Timeline);
    let scroll = resolved_timeline_scroll(lines.len(), inner.height as usize, state);
    for (line, row) in lines.iter_mut().zip(center_rows.iter()) {
        if selected_target.is_some() && row.target == selected_target {
            *line = line.clone().style(selected_style);
        }
    }
    frame.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .scroll((scroll.min(u16::MAX as usize) as u16, 0)),
        inner,
    );
}

fn render_details(
    frame: &mut Frame,
    area: Rect,
    tasks: &TaskState,
    goal_run_id: &str,
    state: &GoalWorkspaceState,
    theme: &ThemeTokens,
) {
    let block = Block::default()
        .title(detail_pane_title(state.mode()))
        .borders(Borders::ALL)
        .border_style(if state.focused_pane() == GoalWorkspacePane::Details {
            theme.accent_primary
        } else {
            theme.fg_dim
        });
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let mut target_index = 0usize;
    let mut lines = detail_lines(tasks, goal_run_id, state, inner.width as usize, theme)
        .into_iter()
        .map(|(_, target, line)| {
            if target.is_some() {
                let current_target_index = target_index;
                target_index = target_index.saturating_add(1);
                if current_target_index == state.selected_detail_row() {
                    line.style(selected_row_style(
                        state.focused_pane() == GoalWorkspacePane::Details,
                    ))
                } else {
                    line
                }
            } else {
                line
            }
        })
        .collect::<Vec<_>>();

    if lines.is_empty() {
        lines.push(Line::from(Span::styled(
            "No details available.",
            theme.fg_dim,
        )));
    }
    let scroll = resolved_detail_scroll(lines.len(), inner.height as usize, state);
    frame.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .scroll((scroll.min(u16::MAX as usize) as u16, 0)),
        inner,
    );
}

fn render_step_footer(
    frame: &mut Frame,
    area: Rect,
    tasks: &TaskState,
    goal_run_id: &str,
    state: &GoalWorkspaceState,
    theme: &ThemeTokens,
) {
    let block = Block::default()
        .title(" Step Actions ")
        .borders(Borders::ALL)
        .border_style(theme.fg_dim);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let spans = footer_segments(tasks, goal_run_id, state, theme)
        .into_iter()
        .map(|segment| Span::styled(segment.text, segment.style))
        .collect::<Vec<_>>();
    let line = if spans.is_empty() {
        Line::from(Span::styled("No step selected.", theme.fg_dim))
    } else {
        Line::from(spans)
    };
    frame.render_widget(Paragraph::new(line), inner);
}

#[derive(Clone)]
struct WorkspaceVisualRow {
    target: Option<GoalWorkspaceHitTarget>,
    line: Line<'static>,
}

fn center_pane_title(mode: GoalWorkspaceMode) -> &'static str {
    match mode {
        GoalWorkspaceMode::Goal => " Run timeline ",
        GoalWorkspaceMode::Progress => " Progress ",
        GoalWorkspaceMode::ActiveAgent => " Active agent ",
        GoalWorkspaceMode::Threads => " Threads ",
        GoalWorkspaceMode::NeedsAttention => " Needs attention ",
    }
}

#[derive(Clone)]
struct FooterSegment {
    text: String,
    style: Style,
    target: Option<GoalWorkspaceHitTarget>,
}

fn footer_segments(
    tasks: &TaskState,
    goal_run_id: &str,
    state: &GoalWorkspaceState,
    theme: &ThemeTokens,
) -> Vec<FooterSegment> {
    let mut segments = Vec::new();
    let Some(step) = selected_goal_step(tasks, goal_run_id, state) else {
        return segments;
    };

    segments.push(FooterSegment {
        text: format!("{}.", step.order + 1),
        style: theme.fg_dim,
        target: None,
    });
    segments.push(FooterSegment {
        text: format!(" {}", step.title),
        style: theme.fg_active,
        target: None,
    });
    segments.push(FooterSegment {
        text: "  ".to_string(),
        style: theme.fg_dim,
        target: None,
    });

    if let Some(run) = tasks.goal_run_by_id(goal_run_id) {
        if let Some((label, style)) = goal_toggle_action_label(run, theme) {
            segments.push(FooterSegment {
                text: format!("[{label}] Ctrl+S"),
                style,
                target: Some(GoalWorkspaceHitTarget::FooterAction(
                    GoalWorkspaceAction::ToggleGoalRun,
                )),
            });
            segments.push(FooterSegment {
                text: "  ".to_string(),
                style: theme.fg_dim,
                target: None,
            });
        }
    }

    for (action, label, hotkey, style) in [
        (
            GoalWorkspaceAction::OpenActions,
            "[Actions]",
            "A",
            theme.accent_primary,
        ),
        (
            GoalWorkspaceAction::RetryStep,
            "[Retry step]",
            "R",
            theme.accent_secondary,
        ),
        (
            GoalWorkspaceAction::RerunFromStep,
            "[Rerun from here]",
            "Shift+R",
            theme.accent_primary,
        ),
    ] {
        segments.push(FooterSegment {
            text: format!("{label} {hotkey}"),
            style,
            target: Some(GoalWorkspaceHitTarget::FooterAction(action)),
        });
        segments.push(FooterSegment {
            text: "  ".to_string(),
            style: theme.fg_dim,
            target: None,
        });
    }

    while segments
        .last()
        .is_some_and(|segment| segment.target.is_none() && segment.text.trim().is_empty())
    {
        segments.pop();
    }
    segments
}

fn footer_hit_test(
    area: Rect,
    tasks: &TaskState,
    goal_run_id: &str,
    state: &GoalWorkspaceState,
    mouse: Position,
) -> Option<GoalWorkspaceHitTarget> {
    let inner = Block::default().borders(Borders::ALL).inner(area);
    if !rect_contains(inner, mouse) || mouse.y != inner.y {
        return None;
    }

    let mut x = inner.x;
    for segment in footer_segments(tasks, goal_run_id, state, &ThemeTokens::default()) {
        let width = segment
            .text
            .chars()
            .map(|ch| ch.width().unwrap_or(0) as u16)
            .sum::<u16>();
        if mouse.x >= x && mouse.x < x.saturating_add(width) {
            return segment.target;
        }
        x = x.saturating_add(width);
    }
    None
}

fn detail_pane_title(mode: GoalWorkspaceMode) -> &'static str {
    match mode {
        GoalWorkspaceMode::Goal => " Goal ",
        GoalWorkspaceMode::Progress => " Progress details ",
        GoalWorkspaceMode::ActiveAgent => " Runtime details ",
        GoalWorkspaceMode::Threads => " Thread details ",
        GoalWorkspaceMode::NeedsAttention => " Attention details ",
    }
}

fn center_rows(
    tasks: &TaskState,
    goal_run_id: &str,
    state: &GoalWorkspaceState,
    width: usize,
    theme: &ThemeTokens,
    tick_counter: u64,
) -> Vec<WorkspaceVisualRow> {
    match state.mode() {
        GoalWorkspaceMode::Goal => timeline_rows(tasks, goal_run_id, width, theme, tick_counter),
        GoalWorkspaceMode::Progress => progress_rows(tasks, goal_run_id, theme),
        GoalWorkspaceMode::ActiveAgent => active_agent_rows(tasks, goal_run_id, theme),
        GoalWorkspaceMode::Threads => thread_rows(tasks, goal_run_id, theme),
        GoalWorkspaceMode::NeedsAttention => attention_rows(tasks, goal_run_id, theme),
    }
}

fn center_targets(
    tasks: &TaskState,
    goal_run_id: &str,
    state: &GoalWorkspaceState,
) -> Vec<GoalWorkspaceHitTarget> {
    let mut targets = Vec::new();
    for row in center_rows(tasks, goal_run_id, state, 80, &ThemeTokens::default(), 0) {
        let Some(target) = row.target else {
            continue;
        };
        if targets.last() != Some(&target) {
            targets.push(target);
        }
    }
    targets
}

#[derive(Clone)]
enum ProgressItem {
    DossierSummary,
    ResumeDecision,
    DeliveryUnit(usize),
    Checkpoint(String),
}

#[derive(Clone)]
enum ActiveAgentItem {
    CurrentOwner,
    PlannerOwner,
    Assignment(usize),
    Thread(String),
}

#[derive(Clone)]
enum ThreadItem {
    Entry(String),
}

#[derive(Clone)]
enum AttentionItem {
    Approvals,
    Status,
    LastError,
    ProjectionError,
}

fn timeline_rows(
    tasks: &TaskState,
    goal_run_id: &str,
    width: usize,
    theme: &ThemeTokens,
    tick_counter: u64,
) -> Vec<WorkspaceVisualRow> {
    let Some(run) = tasks.goal_run_by_id(goal_run_id) else {
        return vec![WorkspaceVisualRow {
            target: None,
            line: Line::from(Span::styled("No timeline available.", theme.fg_dim)),
        }];
    };
    if run.events.is_empty() {
        return vec![WorkspaceVisualRow {
            target: None,
            line: Line::from(Span::styled("Waiting for run events.", theme.fg_dim)),
        }];
    }

    let usable_width = width.saturating_sub(2).max(8);
    let mut rows = Vec::new();
    for (event_index, event) in run.events.iter().rev().enumerate() {
        let (indicator, indicator_style, body_style) =
            timeline_event_visuals(run, event, event_index, theme, tick_counter);
        let label = if event.message.trim().is_empty() {
            "event".to_string()
        } else {
            event.message.clone()
        };
        for (wrapped_index, segment) in wrap_plain_text(&label, usable_width).into_iter().enumerate()
        {
            rows.push(WorkspaceVisualRow {
                target: Some(GoalWorkspaceHitTarget::TimelineRow(event_index)),
                line: if wrapped_index == 0 {
                    Line::from(vec![
                        Span::styled(format!("{indicator} "), indicator_style),
                        Span::styled(segment, body_style),
                    ])
                } else {
                    Line::from(vec![
                        Span::raw("  "),
                        Span::styled(segment, body_style),
                    ])
                },
            });
        }
        if let Some(details) = event.details.as_deref() {
            for segment in wrap_plain_text(details, usable_width.saturating_sub(2).max(8)) {
                rows.push(WorkspaceVisualRow {
                    target: Some(GoalWorkspaceHitTarget::TimelineRow(event_index)),
                    line: Line::from(vec![
                        Span::raw("  "),
                        Span::styled(segment, theme.fg_dim),
                    ]),
                });
            }
        }
        for todo in &event.todo_snapshot {
            rows.push(WorkspaceVisualRow {
                target: Some(GoalWorkspaceHitTarget::TimelineRow(event_index)),
                line: Line::from(vec![
                    Span::raw("  "),
                    Span::styled(todo_status_chip(todo.status), theme.fg_dim),
                    Span::raw(" "),
                    Span::styled(todo.content.clone(), body_style),
                ]),
            });
        }
    }
    rows
}

fn progress_rows(
    tasks: &TaskState,
    goal_run_id: &str,
    theme: &ThemeTokens,
) -> Vec<WorkspaceVisualRow> {
    let items = progress_items(tasks, goal_run_id);
    if items.is_empty() {
        return vec![WorkspaceVisualRow {
            target: None,
            line: Line::from(Span::styled("No progress data available.", theme.fg_dim)),
        }];
    }
    items
        .into_iter()
        .enumerate()
        .map(|(index, item)| {
            let line = match item {
                ProgressItem::DossierSummary => Line::from(vec![
                    Span::styled("[dossier] ", theme.fg_dim),
                    Span::styled("Execution Dossier", theme.fg_active),
                ]),
                ProgressItem::ResumeDecision => Line::from(vec![
                    Span::styled("[resume] ", theme.fg_dim),
                    Span::styled("Resume Decision", theme.fg_active),
                ]),
                ProgressItem::DeliveryUnit(unit_index) => {
                    let unit = tasks
                        .goal_run_by_id(goal_run_id)
                        .and_then(|run| run.dossier.as_ref())
                        .and_then(|dossier| dossier.units.get(unit_index));
                    if let Some(unit) = unit {
                        Line::from(vec![
                            Span::styled(format!("[{}] ", unit.status), theme.fg_dim),
                            Span::styled(unit.title.clone(), theme.fg_active),
                        ])
                    } else {
                        Line::from(Span::styled("Missing delivery unit", theme.fg_dim))
                    }
                }
                ProgressItem::Checkpoint(checkpoint_id) => {
                    let checkpoint = tasks
                        .checkpoints_for_goal_run(goal_run_id)
                        .iter()
                        .find(|checkpoint| checkpoint.id == checkpoint_id);
                    if let Some(checkpoint) = checkpoint {
                        Line::from(vec![
                            Span::styled(
                                format!("[{}] ", checkpoint.checkpoint_type),
                                theme.fg_dim,
                            ),
                            Span::styled(
                                checkpoint
                                    .step_index
                                    .map(|idx| format!("step {}", idx + 1))
                                    .unwrap_or_else(|| "goal".to_string()),
                                theme.fg_active,
                            ),
                        ])
                    } else {
                        Line::from(Span::styled("Missing checkpoint", theme.fg_dim))
                    }
                }
            };
            WorkspaceVisualRow {
                target: Some(GoalWorkspaceHitTarget::TimelineRow(index)),
                line,
            }
        })
        .collect()
}

fn active_agent_rows(
    tasks: &TaskState,
    goal_run_id: &str,
    theme: &ThemeTokens,
) -> Vec<WorkspaceVisualRow> {
    let items = active_agent_items(tasks, goal_run_id);
    if items.is_empty() {
        return vec![WorkspaceVisualRow {
            target: None,
            line: Line::from(Span::styled("No runtime owner metadata.", theme.fg_dim)),
        }];
    }
    let Some(run) = tasks.goal_run_by_id(goal_run_id) else {
        return Vec::new();
    };
    let mut rows = Vec::new();
    for (index, item) in items.into_iter().enumerate() {
        let line = match item {
            ActiveAgentItem::CurrentOwner => Line::from(vec![
                Span::styled("Current ", theme.fg_dim),
                Span::styled(
                    run.current_step_owner_profile
                        .as_ref()
                        .map(|owner| owner.agent_label.clone())
                        .unwrap_or_else(|| "unknown".to_string()),
                    theme.fg_active,
                ),
            ]),
            ActiveAgentItem::PlannerOwner => Line::from(vec![
                Span::styled("Planner ", theme.fg_dim),
                Span::styled(
                    run.planner_owner_profile
                        .as_ref()
                        .map(|owner| owner.agent_label.clone())
                        .unwrap_or_else(|| "unknown".to_string()),
                    theme.fg_active,
                ),
            ]),
            ActiveAgentItem::Assignment(assignment_index) => {
                let assignment = runtime_assignments(run).get(assignment_index).cloned();
                if let Some(assignment) = assignment {
                    Line::from(vec![
                        Span::styled(format!("[{}] ", assignment.role_id), theme.fg_dim),
                        Span::styled(assignment.model, theme.fg_active),
                    ])
                } else {
                    Line::from(Span::styled("Missing assignment", theme.fg_dim))
                }
            }
            ActiveAgentItem::Thread(thread_id) => Line::from(vec![
                Span::styled("[thread] ", theme.fg_dim),
                Span::styled(thread_id, theme.fg_active),
            ]),
        };
        rows.push(WorkspaceVisualRow {
            target: Some(GoalWorkspaceHitTarget::TimelineRow(index)),
            line,
        });
    }
    rows
}

fn thread_rows(
    tasks: &TaskState,
    goal_run_id: &str,
    theme: &ThemeTokens,
) -> Vec<WorkspaceVisualRow> {
    let Some(run) = tasks.goal_run_by_id(goal_run_id) else {
        return Vec::new();
    };
    let entries = goal_thread_entries(tasks, run);
    if entries.is_empty() {
        return vec![WorkspaceVisualRow {
            target: None,
            line: Line::from(Span::styled("No linked threads available.", theme.fg_dim)),
        }];
    }
    entries
        .into_iter()
        .map(|entry| WorkspaceVisualRow {
            target: Some(GoalWorkspaceHitTarget::ThreadRow(entry.thread_id.clone())),
            line: Line::from(vec![
                Span::styled("[thread] ", theme.fg_dim),
                Span::styled(entry.label, theme.fg_active),
                Span::raw("  "),
                Span::styled(entry.thread_id, theme.accent_primary),
            ]),
        })
        .collect()
}

fn attention_rows(
    tasks: &TaskState,
    goal_run_id: &str,
    theme: &ThemeTokens,
) -> Vec<WorkspaceVisualRow> {
    let Some(run) = tasks.goal_run_by_id(goal_run_id) else {
        return Vec::new();
    };
    let items = attention_items(run);
    if items.is_empty() {
        return vec![WorkspaceVisualRow {
            target: None,
            line: Line::from(Span::styled("No blockers or review items.", theme.fg_dim)),
        }];
    }
    items
        .into_iter()
        .enumerate()
        .map(|(index, item)| {
            let line = match item {
                AttentionItem::Approvals => Line::from(vec![
                    Span::styled("Approvals ", theme.fg_dim),
                    Span::styled(run.approval_count.to_string(), theme.fg_active),
                ]),
                AttentionItem::Status => Line::from(vec![
                    Span::styled("Status ", theme.fg_dim),
                    Span::styled(format!("{:?}", run.status).to_ascii_lowercase(), theme.fg_active),
                ]),
                AttentionItem::LastError => Line::from(vec![
                    Span::styled("Last error ", theme.fg_dim),
                    Span::styled("available", theme.accent_danger),
                ]),
                AttentionItem::ProjectionError => Line::from(vec![
                    Span::styled("Projection error ", theme.fg_dim),
                    Span::styled("available", theme.accent_danger),
                ]),
            };
            WorkspaceVisualRow {
                target: Some(GoalWorkspaceHitTarget::TimelineRow(index)),
                line,
            }
        })
        .collect()
}

pub fn detail_targets(
    tasks: &TaskState,
    goal_run_id: &str,
    state: &GoalWorkspaceState,
) -> Vec<(usize, GoalWorkspaceHitTarget)> {
    detail_lines(tasks, goal_run_id, state, 80, &ThemeTokens::default())
        .into_iter()
        .filter_map(|(_, target, _)| target)
        .enumerate()
        .collect()
}

fn selected_goal_step(
    tasks: &TaskState,
    goal_run_id: &str,
    state: &GoalWorkspaceState,
) -> Option<crate::state::task::GoalRunStep> {
    state
        .selected_plan_item()
        .and_then(|selection| match selection {
            crate::state::goal_workspace::GoalPlanSelection::Step { step_id }
            | crate::state::goal_workspace::GoalPlanSelection::Todo { step_id, .. } => {
                Some(step_id.as_str())
            }
            crate::state::goal_workspace::GoalPlanSelection::PromptToggle
            | crate::state::goal_workspace::GoalPlanSelection::MainThread { .. } => None,
        })
        .and_then(|step_id| {
            tasks
                .goal_steps_in_display_order(goal_run_id)
                .into_iter()
                .find(|step| step.id == step_id)
                .cloned()
        })
        .or_else(|| {
            tasks
                .goal_steps_in_display_order(goal_run_id)
                .into_iter()
                .next()
                .cloned()
        })
}

fn detail_lines(
    tasks: &TaskState,
    goal_run_id: &str,
    state: &GoalWorkspaceState,
    width: usize,
    theme: &ThemeTokens,
) -> Vec<(usize, Option<GoalWorkspaceHitTarget>, Line<'static>)> {
    let selected_step = selected_goal_step(tasks, goal_run_id, state);
    let mut rows = Vec::new();
    let mut visual_row = 0usize;
    let run = tasks.goal_run_by_id(goal_run_id);
    match state.mode() {
        GoalWorkspaceMode::Goal => {
            let Some(step) = selected_step else {
                return Vec::new();
            };
            if let Some(run) = run {
                push_detail_header(&mut rows, &mut visual_row, "Selected Step", theme);
                push_detail_line(
                    &mut rows,
                    &mut visual_row,
                    None,
                    Line::from(vec![
                        Span::styled(
                            format!("{}.", step.order + 1),
                            theme.fg_dim,
                        ),
                        Span::raw(" "),
                        Span::styled(step.title.clone(), theme.fg_active),
                    ]),
                );
                if !step.instructions.is_empty() {
                    push_detail_wrapped(
                        &mut rows,
                        &mut visual_row,
                        &step.instructions,
                        theme.fg_dim,
                        width,
                    );
                }
                if let Some(summary) = step.summary.as_deref() {
                    push_detail_wrapped(
                        &mut rows,
                        &mut visual_row,
                        summary,
                        theme.fg_active,
                        width,
                    );
                }

                push_detail_header(&mut rows, &mut visual_row, "Files", theme);
                let file_entries =
                    goal_files_for_selected_step(tasks, goal_run_id, step.order as usize);
                if file_entries.is_empty() {
                    push_detail_line(
                        &mut rows,
                        &mut visual_row,
                        None,
                        Line::from(Span::styled("No goal files yet.", theme.fg_dim)),
                    );
                } else {
                    for entry in file_entries {
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
                        push_detail_line(
                            &mut rows,
                            &mut visual_row,
                            Some(GoalWorkspaceHitTarget::DetailFile(entry.path.clone())),
                            Line::from(vec![
                                Span::styled("• ", theme.accent_secondary),
                                Span::styled(format!("[{label}] "), theme.fg_dim),
                                Span::styled(entry.path.clone(), theme.fg_active),
                            ]),
                        );
                    }
                }

                if let Some(run) = run.dossier.as_ref() {
                    push_detail_blank(&mut rows, &mut visual_row);
                    push_detail_header(&mut rows, &mut visual_row, "Execution Dossier", theme);
                    push_detail_line(
                        &mut rows,
                        &mut visual_row,
                        None,
                        Line::from(vec![
                            Span::styled("Projection ", theme.fg_dim),
                            Span::styled(run.projection_state.clone(), theme.fg_active),
                        ]),
                    );
                    if let Some(summary) = run.summary.as_deref() {
                        push_detail_wrapped(
                            &mut rows,
                            &mut visual_row,
                            summary,
                            theme.fg_active,
                            width,
                        );
                    }
                }

                push_detail_blank(&mut rows, &mut visual_row);
                push_detail_header(&mut rows, &mut visual_row, "Related Tasks", theme);
                let related_tasks = related_tasks_for_step(tasks, run, &step);
                if related_tasks.is_empty() {
                    push_detail_line(
                        &mut rows,
                        &mut visual_row,
                        None,
                        Line::from(Span::styled("No related tasks.", theme.fg_dim)),
                    );
                } else {
                    for task in related_tasks {
                        push_detail_line(
                            &mut rows,
                            &mut visual_row,
                            Some(GoalWorkspaceHitTarget::DetailTask(task.id.clone())),
                            Line::from(vec![
                                Span::styled(todo_task_chip(task.status), theme.fg_dim),
                                Span::raw(" "),
                                Span::styled(task.title.clone(), theme.fg_active),
                            ]),
                        );
                        if let Some(thread_id) = task.thread_id.as_deref() {
                            push_detail_line(
                                &mut rows,
                                &mut visual_row,
                                Some(GoalWorkspaceHitTarget::DetailThread(thread_id.to_string())),
                                Line::from(vec![
                                    Span::styled("  [thread] ", theme.fg_dim),
                                    Span::styled(thread_id.to_string(), theme.accent_primary),
                                ]),
                            );
                        }
                    }
                }

                push_detail_blank(&mut rows, &mut visual_row);
                push_detail_header(&mut rows, &mut visual_row, "Checkpoints", theme);
                let checkpoints = tasks.goal_step_checkpoints(goal_run_id, step.order as usize);
                if checkpoints.is_empty() {
                    push_detail_line(
                        &mut rows,
                        &mut visual_row,
                        None,
                        Line::from(Span::styled("No checkpoints for this step.", theme.fg_dim)),
                    );
                } else {
                    for checkpoint in checkpoints {
                        push_detail_line(
                            &mut rows,
                            &mut visual_row,
                            Some(GoalWorkspaceHitTarget::DetailCheckpoint(
                                checkpoint.id.clone(),
                            )),
                            Line::from(vec![
                                Span::styled("• ", theme.accent_secondary),
                                Span::styled(
                                    checkpoint.checkpoint_type.clone(),
                                    theme.fg_active,
                                ),
                                Span::raw("  "),
                                Span::styled(short_checkpoint_id(&checkpoint.id), theme.fg_dim),
                            ]),
                        );
                    }
                }
            }

            if let Some(run) = run.and_then(|run| run.dossier.as_ref()) {
                push_detail_blank(&mut rows, &mut visual_row);
                push_detail_header(&mut rows, &mut visual_row, "Execution Dossier", theme);
                push_detail_line(
                    &mut rows,
                    &mut visual_row,
                    None,
                    Line::from(vec![
                        Span::styled("Projection ", theme.fg_dim),
                        Span::styled(run.projection_state.clone(), theme.fg_active),
                    ]),
                );
                if let Some(summary) = run.summary.as_deref() {
                    push_detail_wrapped(
                        &mut rows,
                        &mut visual_row,
                        summary,
                        theme.fg_active,
                        width,
                    );
                }
            }

            if let Some((selected_event_index, event)) = selected_event(tasks, goal_run_id, state) {
                push_detail_blank(&mut rows, &mut visual_row);
                push_detail_header(&mut rows, &mut visual_row, "Selected Timeline Item", theme);
                push_detail_line(
                    &mut rows,
                    &mut visual_row,
                    None,
                    Line::from(Span::styled(event.message.clone(), theme.fg_active)),
                );
                if let Some(details) = event.details.as_deref() {
                    push_detail_wrapped(
                        &mut rows,
                        &mut visual_row,
                        details,
                        theme.fg_dim,
                        width,
                    );
                }
                for todo in &event.todo_snapshot {
                    push_detail_line(
                        &mut rows,
                        &mut visual_row,
                        None,
                        Line::from(vec![
                            Span::styled(todo_status_chip(todo.status), theme.fg_dim),
                            Span::raw(" "),
                            Span::styled(todo.content.clone(), theme.fg_active),
                        ]),
                    );
                }
                push_detail_line(
                    &mut rows,
                    &mut visual_row,
                    Some(GoalWorkspaceHitTarget::DetailTimelineDetails(selected_event_index)),
                    Line::from(vec![
                        Span::styled("[details]", theme.accent_primary),
                        Span::raw("  "),
                        Span::styled("show timeline item context", theme.fg_dim),
                    ]),
                );
                if let Some(run) = run {
                    for thread_id in goal_thread_targets(run) {
                        push_detail_line(
                            &mut rows,
                            &mut visual_row,
                            Some(GoalWorkspaceHitTarget::DetailThread(thread_id.clone())),
                            Line::from(vec![
                                Span::styled("[thread] ", theme.fg_dim),
                                Span::styled(thread_id, theme.accent_primary),
                            ]),
                        );
                    }
                }
            }
        }
        GoalWorkspaceMode::Progress => {
            let items = progress_items(tasks, goal_run_id);
            let selected = items.get(state.selected_timeline_row());
            if let Some(item) = selected {
                match item {
                    ProgressItem::DossierSummary => {
                        push_detail_header(&mut rows, &mut visual_row, "Execution Dossier", theme);
                        if let Some(dossier) = run.and_then(|run| run.dossier.as_ref()) {
                            push_detail_line(
                                &mut rows,
                                &mut visual_row,
                                None,
                                Line::from(vec![
                                    Span::styled("Projection ", theme.fg_dim),
                                    Span::styled(dossier.projection_state.clone(), theme.fg_active),
                                ]),
                            );
                            if let Some(summary) = dossier.summary.as_deref() {
                                push_detail_wrapped(
                                    &mut rows,
                                    &mut visual_row,
                                    summary,
                                    theme.fg_active,
                                    width,
                                );
                            }
                            if let Some(error) = dossier.projection_error.as_deref() {
                                push_detail_wrapped(
                                    &mut rows,
                                    &mut visual_row,
                                    error,
                                    theme.accent_danger,
                                    width,
                                );
                            }
                        }
                    }
                    ProgressItem::ResumeDecision => {
                        push_detail_header(&mut rows, &mut visual_row, "Resume Decision", theme);
                        if let Some(decision) = run
                            .and_then(|run| run.dossier.as_ref())
                            .and_then(|dossier| dossier.latest_resume_decision.as_ref())
                        {
                            push_detail_line(
                                &mut rows,
                                &mut visual_row,
                                None,
                                Line::from(Span::styled(
                                    format!(
                                        "{} via {} ({})",
                                        decision.action,
                                        decision.reason_code,
                                        decision.projection_state
                                    ),
                                    theme.fg_active,
                                )),
                            );
                            if let Some(reason) = decision.reason.as_deref() {
                                push_detail_wrapped(
                                    &mut rows,
                                    &mut visual_row,
                                    reason,
                                    theme.fg_dim,
                                    width,
                                );
                            }
                            for detail in &decision.details {
                                push_detail_wrapped(
                                    &mut rows,
                                    &mut visual_row,
                                    detail,
                                    theme.fg_dim,
                                    width,
                                );
                            }
                        }
                    }
                    ProgressItem::DeliveryUnit(unit_index) => {
                        push_detail_header(&mut rows, &mut visual_row, "Delivery Unit", theme);
                        if let Some(unit) = run
                            .and_then(|run| run.dossier.as_ref())
                            .and_then(|dossier| dossier.units.get(*unit_index))
                        {
                            push_detail_line(
                                &mut rows,
                                &mut visual_row,
                                None,
                                Line::from(Span::styled(unit.title.clone(), theme.fg_active)),
                            );
                            push_detail_wrapped(
                                &mut rows,
                                &mut visual_row,
                                &format!(
                                    "execute via {}  verify via {}",
                                    unit.execution_binding, unit.verification_binding
                                ),
                                theme.fg_dim,
                                width,
                            );
                            if let Some(summary) = unit.summary.as_deref() {
                                push_detail_wrapped(
                                    &mut rows,
                                    &mut visual_row,
                                    summary,
                                    theme.fg_active,
                                    width,
                                );
                            }
                            for proof in &unit.proof_checks {
                                push_detail_line(
                                    &mut rows,
                                    &mut visual_row,
                                    None,
                                    Line::from(vec![
                                        Span::styled("[proof] ", theme.fg_dim),
                                        Span::styled(proof.title.clone(), theme.fg_active),
                                        Span::raw(" "),
                                        Span::styled(proof.state.clone(), theme.fg_dim),
                                    ]),
                                );
                            }
                            if let Some(report) = unit.report.as_ref() {
                                push_detail_wrapped(
                                    &mut rows,
                                    &mut visual_row,
                                    &format!("report [{}] {}", report.state, report.summary),
                                    theme.fg_active,
                                    width,
                                );
                            }
                        }
                    }
                    ProgressItem::Checkpoint(checkpoint_id) => {
                        push_detail_header(&mut rows, &mut visual_row, "Checkpoints", theme);
                        if let Some(checkpoint) = tasks
                            .checkpoints_for_goal_run(goal_run_id)
                            .iter()
                            .find(|checkpoint| checkpoint.id == *checkpoint_id)
                        {
                            push_detail_line(
                                &mut rows,
                                &mut visual_row,
                                Some(GoalWorkspaceHitTarget::DetailCheckpoint(
                                    checkpoint.id.clone(),
                                )),
                                Line::from(vec![
                                    Span::styled(checkpoint.checkpoint_type.clone(), theme.fg_active),
                                    Span::raw("  "),
                                    Span::styled(short_checkpoint_id(&checkpoint.id), theme.fg_dim),
                                ]),
                            );
                            if let Some(preview) = checkpoint.context_summary_preview.as_deref() {
                                push_detail_wrapped(
                                    &mut rows,
                                    &mut visual_row,
                                    preview,
                                    theme.fg_dim,
                                    width,
                                );
                            }
                        }
                    }
                }
            }
        }
        GoalWorkspaceMode::ActiveAgent => {
            if let Some(run) = run {
                let items = active_agent_items(tasks, goal_run_id);
                if let Some(item) = items.get(state.selected_timeline_row()) {
                    match item {
                        ActiveAgentItem::CurrentOwner => {
                            push_detail_header(&mut rows, &mut visual_row, "Current Owner", theme);
                            if let Some(owner) = run.current_step_owner_profile.as_ref() {
                                push_owner_profile(
                                    &mut rows,
                                    &mut visual_row,
                                    owner,
                                    theme,
                                    width,
                                );
                            }
                        }
                        ActiveAgentItem::PlannerOwner => {
                            push_detail_header(&mut rows, &mut visual_row, "Planner Owner", theme);
                            if let Some(owner) = run.planner_owner_profile.as_ref() {
                                push_owner_profile(
                                    &mut rows,
                                    &mut visual_row,
                                    owner,
                                    theme,
                                    width,
                                );
                            }
                        }
                        ActiveAgentItem::Assignment(index) => {
                            push_detail_header(
                                &mut rows,
                                &mut visual_row,
                                "Runtime Assignment",
                                theme,
                            );
                            if let Some(assignment) = runtime_assignments(run).get(*index) {
                                push_assignment(
                                    &mut rows,
                                    &mut visual_row,
                                    assignment,
                                    theme,
                                    width,
                                );
                            }
                        }
                        ActiveAgentItem::Thread(thread_id) => {
                            push_detail_header(&mut rows, &mut visual_row, "Thread", theme);
                            push_detail_line(
                                &mut rows,
                                &mut visual_row,
                                Some(GoalWorkspaceHitTarget::DetailThread(thread_id.clone())),
                                Line::from(vec![
                                    Span::styled("[open] ", theme.accent_primary),
                                    Span::styled(thread_id.clone(), theme.fg_active),
                                ]),
                            );
                            push_detail_wrapped(
                                &mut rows,
                                &mut visual_row,
                                "Opens the linked thread and keeps a return-to-goal path.",
                                theme.fg_dim,
                                width,
                            );
                        }
                    }
                }
            }
        }
        GoalWorkspaceMode::Threads => {
            if let Some(run) = run {
                let items = thread_items(tasks, run);
                if let Some(ThreadItem::Entry(thread_id)) = items.get(state.selected_timeline_row()) {
                    push_detail_header(&mut rows, &mut visual_row, "Thread", theme);
                    if let Some(entry) = goal_thread_entries(tasks, run)
                        .into_iter()
                        .find(|entry| &entry.thread_id == thread_id)
                    {
                        push_detail_line(
                            &mut rows,
                            &mut visual_row,
                            None,
                            Line::from(vec![
                                Span::styled(entry.label, theme.fg_active),
                                Span::raw("  "),
                                Span::styled(entry.thread_id.clone(), theme.fg_dim),
                            ]),
                        );
                        push_detail_wrapped(
                            &mut rows,
                            &mut visual_row,
                            &entry.summary,
                            theme.fg_dim,
                            width,
                        );
                        push_detail_line(
                            &mut rows,
                            &mut visual_row,
                            Some(GoalWorkspaceHitTarget::DetailThread(entry.thread_id.clone())),
                            Line::from(vec![
                                Span::styled("[open] ", theme.accent_primary),
                                Span::styled(entry.thread_id, theme.fg_active),
                            ]),
                        );
                    }
                }
            }
        }
        GoalWorkspaceMode::NeedsAttention => {
            if let Some(run) = run {
                let items = attention_items(run);
                if let Some(item) = items.get(state.selected_timeline_row()) {
                    match item {
                        AttentionItem::Approvals => {
                            push_detail_header(&mut rows, &mut visual_row, "Approvals", theme);
                            push_detail_line(
                                &mut rows,
                                &mut visual_row,
                                None,
                                Line::from(Span::styled(
                                    run.approval_count.to_string(),
                                    theme.fg_active,
                                )),
                            );
                        }
                        AttentionItem::Status => {
                            push_detail_header(&mut rows, &mut visual_row, "Status", theme);
                            push_detail_line(
                                &mut rows,
                                &mut visual_row,
                                None,
                                Line::from(Span::styled(
                                    goal_status_label(run.status),
                                    theme.fg_active,
                                )),
                            );
                            if let Some(awaiting_id) = run.awaiting_approval_id.as_deref() {
                                push_detail_wrapped(
                                    &mut rows,
                                    &mut visual_row,
                                    &format!("Awaiting approval {awaiting_id}"),
                                    theme.fg_dim,
                                    width,
                                );
                            }
                        }
                        AttentionItem::LastError => {
                            push_detail_header(&mut rows, &mut visual_row, "Last Error", theme);
                            if let Some(last_error) = run.last_error.as_deref() {
                                push_detail_wrapped(
                                    &mut rows,
                                    &mut visual_row,
                                    last_error,
                                    theme.accent_danger,
                                    width,
                                );
                            }
                        }
                        AttentionItem::ProjectionError => {
                            push_detail_header(
                                &mut rows,
                                &mut visual_row,
                                "Projection Error",
                                theme,
                            );
                            if let Some(error) = run
                                .dossier
                                .as_ref()
                                .and_then(|dossier| dossier.projection_error.as_deref())
                            {
                                push_detail_wrapped(
                                    &mut rows,
                                    &mut visual_row,
                                    error,
                                    theme.accent_danger,
                                    width,
                                );
                            }
                        }
                    }
                }
            }
        }
    }
    rows
}

fn progress_items(tasks: &TaskState, goal_run_id: &str) -> Vec<ProgressItem> {
    let Some(run) = tasks.goal_run_by_id(goal_run_id) else {
        return Vec::new();
    };
    let mut items = Vec::new();
    for checkpoint in tasks.checkpoints_for_goal_run(goal_run_id) {
        items.push(ProgressItem::Checkpoint(checkpoint.id.clone()));
    }
    if run.dossier.is_some() {
        items.push(ProgressItem::DossierSummary);
    }
    if run
        .dossier
        .as_ref()
        .and_then(|dossier| dossier.latest_resume_decision.as_ref())
        .is_some()
    {
        items.push(ProgressItem::ResumeDecision);
    }
    if let Some(dossier) = run.dossier.as_ref() {
        for index in 0..dossier.units.len() {
            items.push(ProgressItem::DeliveryUnit(index));
        }
    }
    items
}

fn active_agent_items(tasks: &TaskState, goal_run_id: &str) -> Vec<ActiveAgentItem> {
    let Some(run) = tasks.goal_run_by_id(goal_run_id) else {
        return Vec::new();
    };
    let mut items = Vec::new();
    if run.current_step_owner_profile.is_some() {
        items.push(ActiveAgentItem::CurrentOwner);
    }
    if run.planner_owner_profile.is_some() {
        items.push(ActiveAgentItem::PlannerOwner);
    }
    for index in 0..runtime_assignments(run).len() {
        items.push(ActiveAgentItem::Assignment(index));
    }
    for thread_id in goal_thread_targets(run) {
        items.push(ActiveAgentItem::Thread(thread_id));
    }
    items
}

fn thread_items(tasks: &TaskState, run: &crate::state::task::GoalRun) -> Vec<ThreadItem> {
    goal_thread_entries(tasks, run)
        .into_iter()
        .map(|entry| ThreadItem::Entry(entry.thread_id))
        .collect()
}

fn attention_items(run: &crate::state::task::GoalRun) -> Vec<AttentionItem> {
    let mut items = Vec::new();
    if run.last_error.is_some() {
        items.push(AttentionItem::LastError);
    }
    if run
        .dossier
        .as_ref()
        .and_then(|dossier| dossier.projection_error.as_deref())
        .is_some()
    {
        items.push(AttentionItem::ProjectionError);
    }
    items.push(AttentionItem::Approvals);
    items.push(AttentionItem::Status);
    items
}

fn runtime_assignments(run: &crate::state::task::GoalRun) -> Vec<crate::state::task::GoalAgentAssignment> {
    if !run.runtime_assignment_list.is_empty() {
        run.runtime_assignment_list.clone()
    } else {
        run.launch_assignment_snapshot.clone()
    }
}

fn selected_event<'a>(
    tasks: &'a TaskState,
    goal_run_id: &str,
    state: &GoalWorkspaceState,
) -> Option<(usize, &'a crate::state::task::GoalRunEvent)> {
    let selected_target = center_targets(tasks, goal_run_id, state)
        .get(state.selected_timeline_row())
        .cloned()?;
    let GoalWorkspaceHitTarget::TimelineRow(event_index) = selected_target else {
        return None;
    };
    let run = tasks.goal_run_by_id(goal_run_id)?;
    run.events.iter().rev().nth(event_index).map(|event| (event_index, event))
}

fn related_tasks_for_step<'a>(
    tasks: &'a TaskState,
    run: &crate::state::task::GoalRun,
    step: &crate::state::task::GoalRunStep,
) -> Vec<&'a crate::state::task::AgentTask> {
    let related: Vec<_> = tasks
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
        .collect();
    if !related.is_empty() {
        return related;
    }
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
        .filter(|task| task.goal_run_id.as_deref() == Some(run.id.as_str()))
        .collect()
}

fn goal_thread_targets(run: &crate::state::task::GoalRun) -> Vec<String> {
    let mut threads = Vec::new();
    for thread_id in run
        .active_thread_id
        .iter()
        .chain(run.root_thread_id.iter())
        .chain(run.thread_id.iter())
        .cloned()
    {
        if !threads.contains(&thread_id) {
            threads.push(thread_id);
        }
    }
    for thread_id in &run.execution_thread_ids {
        if !threads.contains(thread_id) {
            threads.push(thread_id.clone());
        }
    }
    threads
}

#[derive(Clone)]
struct GoalThreadEntry {
    label: String,
    thread_id: String,
    summary: String,
}

fn goal_thread_entries(
    tasks: &TaskState,
    run: &crate::state::task::GoalRun,
) -> Vec<GoalThreadEntry> {
    let mut entries = Vec::new();
    let mut push_entry = |label: String, thread_id: String, summary: String| {
        if !entries
            .iter()
            .any(|entry: &GoalThreadEntry| entry.thread_id == thread_id)
        {
            entries.push(GoalThreadEntry {
                label,
                thread_id,
                summary,
            });
        }
    };

    if let Some(thread_id) = run.active_thread_id.clone() {
        let label = run
            .current_step_owner_profile
            .as_ref()
            .map(|owner| owner.agent_label.clone())
            .unwrap_or_else(|| "Main agent".to_string());
        push_entry(
            label,
            thread_id,
            "Active thread for the current goal step.".to_string(),
        );
    }
    if let Some(thread_id) = run.root_thread_id.clone() {
        let label = run
            .planner_owner_profile
            .as_ref()
            .map(|owner| owner.agent_label.clone())
            .unwrap_or_else(|| "Planner".to_string());
        push_entry(label, thread_id, "Root goal-planning thread.".to_string());
    }
    if let Some(thread_id) = run.thread_id.clone() {
        push_entry(
            "Goal thread".to_string(),
            thread_id,
            "Primary thread attached to the goal run.".to_string(),
        );
    }
    for (index, thread_id) in run.execution_thread_ids.iter().cloned().enumerate() {
        let label = run
            .current_step_owner_profile
            .as_ref()
            .filter(|_| index == 0)
            .map(|owner| owner.agent_label.clone())
            .unwrap_or_else(|| format!("Execution {}", index + 1));
        push_entry(
            label,
            thread_id,
            "Execution thread spawned by the goal run.".to_string(),
        );
    }
    for task in tasks
        .tasks()
        .iter()
        .filter(|task| task.goal_run_id.as_deref() == Some(run.id.as_str()))
    {
        if let Some(thread_id) = task.thread_id.clone() {
            push_entry(
                task.title.clone(),
                thread_id,
                "Task-linked thread related to this goal.".to_string(),
            );
        }
    }
    entries
}

fn push_detail_header(
    rows: &mut Vec<(usize, Option<GoalWorkspaceHitTarget>, Line<'static>)>,
    visual_row: &mut usize,
    label: &str,
    theme: &ThemeTokens,
) {
    push_detail_line(
        rows,
        visual_row,
        None,
        Line::from(Span::styled(label.to_string(), theme.accent_secondary)),
    );
}

fn push_detail_blank(
    rows: &mut Vec<(usize, Option<GoalWorkspaceHitTarget>, Line<'static>)>,
    visual_row: &mut usize,
) {
    push_detail_line(rows, visual_row, None, Line::from(Span::raw("")));
}

fn push_detail_line(
    rows: &mut Vec<(usize, Option<GoalWorkspaceHitTarget>, Line<'static>)>,
    visual_row: &mut usize,
    target: Option<GoalWorkspaceHitTarget>,
    line: Line<'static>,
) {
    rows.push((*visual_row, target, line));
    *visual_row = visual_row.saturating_add(1);
}

fn push_detail_wrapped(
    rows: &mut Vec<(usize, Option<GoalWorkspaceHitTarget>, Line<'static>)>,
    visual_row: &mut usize,
    text: &str,
    style: Style,
    width: usize,
) {
    for line in wrap_plain_text(text, width.max(8)) {
        push_detail_line(rows, visual_row, None, Line::from(Span::styled(line, style)));
    }
}

fn push_owner_profile(
    rows: &mut Vec<(usize, Option<GoalWorkspaceHitTarget>, Line<'static>)>,
    visual_row: &mut usize,
    owner: &crate::state::task::GoalRuntimeOwnerProfile,
    theme: &ThemeTokens,
    width: usize,
) {
    push_detail_line(
        rows,
        visual_row,
        None,
        Line::from(Span::styled(owner.agent_label.clone(), theme.fg_active)),
    );
    push_detail_wrapped(
        rows,
        visual_row,
        &format!(
            "{} / {} / {}",
            owner.provider,
            owner.model,
            owner.reasoning_effort.clone().unwrap_or_else(|| "default".to_string())
        ),
        theme.fg_dim,
        width,
    );
}

fn push_assignment(
    rows: &mut Vec<(usize, Option<GoalWorkspaceHitTarget>, Line<'static>)>,
    visual_row: &mut usize,
    assignment: &crate::state::task::GoalAgentAssignment,
    theme: &ThemeTokens,
    width: usize,
) {
    push_detail_line(
        rows,
        visual_row,
        None,
        Line::from(vec![
            Span::styled(assignment.role_id.clone(), theme.fg_active),
            Span::raw(" "),
            Span::styled(
                if assignment.enabled { "[enabled]" } else { "[disabled]" },
                theme.fg_dim,
            ),
        ]),
    );
    push_detail_wrapped(
        rows,
        visual_row,
        &format!(
            "{} / {} / {}",
            assignment.provider,
            assignment.model,
            assignment
                .reasoning_effort
                .clone()
                .unwrap_or_else(|| "default".to_string())
        ),
        theme.fg_dim,
        width,
    );
}

fn goal_toggle_action_label(run: &crate::state::task::GoalRun, theme: &ThemeTokens) -> Option<(String, Style)> {
    match run.status {
        Some(crate::state::task::GoalRunStatus::Paused) => {
            Some(("[Resume]".to_string(), theme.accent_success))
        }
        Some(crate::state::task::GoalRunStatus::Queued)
        | Some(crate::state::task::GoalRunStatus::Planning)
        | Some(crate::state::task::GoalRunStatus::Running)
        | Some(crate::state::task::GoalRunStatus::AwaitingApproval) => {
            Some(("[Pause]".to_string(), theme.accent_secondary))
        }
        _ => None,
    }
}

fn goal_status_label(status: Option<crate::state::task::GoalRunStatus>) -> &'static str {
    match status {
        Some(crate::state::task::GoalRunStatus::Queued) => "queued",
        Some(crate::state::task::GoalRunStatus::Planning) => "planning",
        Some(crate::state::task::GoalRunStatus::Running) => "running",
        Some(crate::state::task::GoalRunStatus::AwaitingApproval) => "awaiting approval",
        Some(crate::state::task::GoalRunStatus::Paused) => "paused",
        Some(crate::state::task::GoalRunStatus::Completed) => "completed",
        Some(crate::state::task::GoalRunStatus::Failed) => "failed",
        Some(crate::state::task::GoalRunStatus::Cancelled) => "cancelled",
        None => "queued",
    }
}

fn short_checkpoint_id(id: &str) -> String {
    if id.chars().count() <= 18 {
        return id.to_string();
    }
    let tail: String = id
        .chars()
        .rev()
        .take(12)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    format!("…{tail}")
}

fn todo_status_chip(status: Option<crate::state::task::TodoStatus>) -> &'static str {
    match status {
        Some(crate::state::task::TodoStatus::InProgress) => "[~]",
        Some(crate::state::task::TodoStatus::Completed) => "[x]",
        Some(crate::state::task::TodoStatus::Blocked) => "[!]",
        _ => "[ ]",
    }
}

fn todo_task_chip(status: Option<crate::state::task::TaskStatus>) -> &'static str {
    match status {
        Some(crate::state::task::TaskStatus::InProgress) => "[~]",
        Some(crate::state::task::TaskStatus::Completed) => "[x]",
        Some(crate::state::task::TaskStatus::Blocked)
        | Some(crate::state::task::TaskStatus::Failed)
        | Some(crate::state::task::TaskStatus::FailedAnalyzing)
        | Some(crate::state::task::TaskStatus::BudgetExceeded) => "[!]",
        _ => "[ ]",
    }
}

fn goal_files_for_selected_step<'a>(
    tasks: &'a TaskState,
    goal_run_id: &str,
    step_index: usize,
) -> Vec<&'a crate::state::task::WorkContextEntry> {
    let Some(run) = tasks.goal_run_by_id(goal_run_id) else {
        return Vec::new();
    };
    let Some(thread_id) = run.thread_id.as_deref() else {
        return Vec::new();
    };

    let step_files = tasks.goal_step_files(goal_run_id, thread_id, step_index);
    if !step_files.is_empty() {
        return step_files;
    }

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

fn rect_contains(area: Rect, mouse: Position) -> bool {
    mouse.x >= area.x
        && mouse.x < area.x.saturating_add(area.width)
        && mouse.y >= area.y
        && mouse.y < area.y.saturating_add(area.height)
}

fn wrap_plain_text(text: &str, width: usize) -> Vec<String> {
    if width == 0 {
        return vec![text.to_string()];
    }
    let mut wrapped = Vec::new();
    for raw_line in text.lines() {
        let mut current = String::new();
        for word in raw_line.split_whitespace() {
            let candidate = if current.is_empty() {
                word.to_string()
            } else {
                format!("{current} {word}")
            };
            if candidate.chars().count() > width && !current.is_empty() {
                wrapped.push(current);
                current = word.to_string();
            } else {
                current = candidate;
            }
        }
        if current.is_empty() {
            wrapped.push(String::new());
        } else {
            wrapped.push(current);
        }
    }
    if wrapped.is_empty() {
        wrapped.push(String::new());
    }
    wrapped
}

fn timeline_event_visuals(
    run: &crate::state::task::GoalRun,
    event: &crate::state::task::GoalRunEvent,
    event_index: usize,
    theme: &ThemeTokens,
    tick_counter: u64,
) -> (char, Style, Style) {
    let is_current_step_event = event
        .step_index
        .is_some_and(|index| index == run.current_step_index);
    let is_latest_event = event_index == 0;
    let is_live_row = matches!(
        run.status,
        Some(crate::state::task::GoalRunStatus::Planning | crate::state::task::GoalRunStatus::Running)
    ) && (is_current_step_event || is_latest_event);

    if is_live_row {
        return (
            spinner_frame(tick_counter),
            theme.accent_primary,
            theme.fg_active,
        );
    }

    match run.status {
        Some(crate::state::task::GoalRunStatus::AwaitingApproval)
        | Some(crate::state::task::GoalRunStatus::Paused) if is_latest_event => {
            ('‖', theme.accent_secondary, theme.fg_active)
        }
        Some(crate::state::task::GoalRunStatus::Completed) if is_latest_event => {
            ('✓', theme.accent_success, theme.fg_active)
        }
        Some(crate::state::task::GoalRunStatus::Failed)
        | Some(crate::state::task::GoalRunStatus::Cancelled) if is_latest_event => {
            ('✕', theme.accent_danger, theme.fg_active)
        }
        _ => ('•', theme.accent_secondary, theme.fg_active),
    }
}

fn spinner_frame(tick_counter: u64) -> char {
    match tick_counter % 4 {
        0 => '⠋',
        1 => '⠙',
        2 => '⠹',
        _ => '⠸',
    }
}

fn selected_row_style(selected: bool) -> Style {
    if selected {
        Style::default().bg(Color::Indexed(236))
    } else {
        Style::default()
    }
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

fn resolved_timeline_scroll(
    row_count: usize,
    viewport_height: usize,
    state: &GoalWorkspaceState,
) -> usize {
    row_count
        .saturating_sub(viewport_height)
        .min(state.timeline_scroll())
}

fn resolved_detail_scroll(
    row_count: usize,
    viewport_height: usize,
    state: &GoalWorkspaceState,
) -> usize {
    row_count
        .saturating_sub(viewport_height)
        .min(state.detail_scroll())
}

pub fn timeline_visual_row_for_selection(
    tasks: &TaskState,
    goal_run_id: &str,
    state: &GoalWorkspaceState,
) -> Option<usize> {
    let selected_target = center_targets(tasks, goal_run_id, state)
        .get(state.selected_timeline_row())
        .cloned()?;
    center_rows(tasks, goal_run_id, state, 80, &ThemeTokens::default(), 0)
        .iter()
        .position(|row| row.target == Some(selected_target.clone()))
}

pub fn timeline_targets(
    tasks: &TaskState,
    goal_run_id: &str,
    state: &GoalWorkspaceState,
) -> Vec<(usize, GoalWorkspaceHitTarget)> {
    center_targets(tasks, goal_run_id, state)
        .into_iter()
        .enumerate()
        .collect()
}

pub fn plan_selection_rows(
    tasks: &TaskState,
    goal_run_id: &str,
    state: &GoalWorkspaceState,
) -> Vec<(usize, crate::state::goal_workspace::GoalPlanSelection)> {
    plan::build_rows(tasks, goal_run_id, state)
        .into_iter()
        .enumerate()
        .filter_map(|(index, row)| row.selection.map(|selection| (index, selection)))
        .collect()
}

pub fn plan_visual_row_for_selection(
    tasks: &TaskState,
    goal_run_id: &str,
    state: &GoalWorkspaceState,
) -> Option<usize> {
    let selected = plan_selection_rows(tasks, goal_run_id, state)
        .into_iter()
        .find_map(|(visual_row, selection)| {
            (state.selected_plan_item() == Some(&selection)).then_some(visual_row)
        });
    selected.or_else(|| {
        plan_selection_rows(tasks, goal_run_id, state)
            .get(state.selected_plan_row())
            .map(|(visual_row, _)| *visual_row)
    })
}

pub fn detail_visual_row_for_selection(
    tasks: &TaskState,
    goal_run_id: &str,
    state: &GoalWorkspaceState,
) -> Option<usize> {
    let selected_target = detail_targets(tasks, goal_run_id, state)
        .into_iter()
        .find_map(|(index, target)| (index == state.selected_detail_row()).then_some(target))?;
    detail_lines(tasks, goal_run_id, state, 80, &ThemeTokens::default())
        .iter()
        .position(|(_, target, _)| *target == Some(selected_target.clone()))
}

fn plan_inner_row_index(
    area: Rect,
    tasks: &TaskState,
    goal_run_id: &str,
    state: &GoalWorkspaceState,
    mouse: Position,
) -> Option<(Rect, usize)> {
    let Some(layout) = workspace_layout(area) else {
        return None;
    };
    if mouse.x < area.x
        || mouse.x >= area.x.saturating_add(area.width)
        || mouse.y < area.y
        || mouse.y >= area.y.saturating_add(area.height)
    {
        return None;
    }

    let plan_area = layout.plan;
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
