#![allow(dead_code)]

use crate::state::goal_mission_control::GoalMissionControlState;
use crate::theme::ThemeTokens;
use ratatui::prelude::*;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph, Wrap};

const OPEN_ACTIVE_THREAD_LABEL: &str = "[Ctrl+O] Open active thread";
const RETURN_TO_GOAL_LABEL: &str = "[B] Return to goal";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GoalMissionControlHitTarget {
    OpenActiveThread,
}

pub fn render_preflight(
    frame: &mut Frame,
    area: Rect,
    state: &GoalMissionControlState,
    can_open_active_thread: bool,
    theme: &ThemeTokens,
) {
    let block = Block::default()
        .title(" MISSION CONTROL ")
        .borders(Borders::ALL)
        .border_type(BorderType::Double)
        .border_style(theme.accent_primary);

    let inner = block.inner(area);
    frame.render_widget(Clear, area);
    frame.render_widget(block, area);

    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(5),
            Constraint::Length(8),
            Constraint::Min(7),
            Constraint::Length(3),
            Constraint::Length(2),
        ])
        .split(inner);

    render_prompt_section(frame, sections[0], state, theme);
    render_main_section(frame, sections[1], state, theme);
    render_role_assignments_section(frame, sections[2], state, theme);
    render_thread_router_section(frame, sections[3], can_open_active_thread, theme);
    render_footer(frame, sections[4], theme);
}

pub fn hit_test(
    area: Rect,
    mouse: Position,
    can_open_active_thread: bool,
) -> Option<GoalMissionControlHitTarget> {
    if !can_open_active_thread {
        return None;
    }
    if area.width == 0
        || area.height == 0
        || mouse.x < area.x
        || mouse.x >= area.x.saturating_add(area.width)
        || mouse.y < area.y
        || mouse.y >= area.y.saturating_add(area.height)
    {
        return None;
    }

    let router_area = thread_router_area(area)?;
    let button = open_active_thread_button_area(router_area)?;
    point_in_rect(button, mouse).then_some(GoalMissionControlHitTarget::OpenActiveThread)
}

pub fn render_return_to_goal_banner(frame: &mut Frame, area: Rect, theme: &ThemeTokens) {
    let block = Block::default()
        .title(" MISSION CONTROL ")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(theme.accent_secondary);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let button = return_to_goal_button_area(area);
    let button_style = theme.accent_secondary;
    let mut spans = vec![
        Span::styled(RETURN_TO_GOAL_LABEL, button_style),
        Span::styled("  Return to the source goal run", theme.fg_dim),
    ];
    if button.is_none() {
        spans.clear();
        spans.push(Span::styled("Return to goal unavailable", theme.fg_dim));
    }
    frame.render_widget(Paragraph::new(Line::from(spans)), inner);
}

pub fn return_to_goal_button_area(area: Rect) -> Option<Rect> {
    let inner = Block::default().borders(Borders::ALL).inner(area);
    button_area(inner, RETURN_TO_GOAL_LABEL)
}

fn render_prompt_section(
    frame: &mut Frame,
    area: Rect,
    state: &GoalMissionControlState,
    theme: &ThemeTokens,
) {
    let block = Block::default()
        .title(" Prompt ")
        .borders(Borders::ALL)
        .border_style(theme.fg_dim);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let prompt_text = if state.prompt_text.trim().is_empty() {
        "(empty)".to_string()
    } else {
        state.prompt_text.clone()
    };
    let content = vec![
        Line::from(Span::styled("Goal prompt", theme.accent_secondary)),
        Line::from(Span::styled(prompt_text, theme.fg_active)),
    ];
    frame.render_widget(Paragraph::new(content).wrap(Wrap { trim: false }), inner);
}

fn render_main_section(
    frame: &mut Frame,
    area: Rect,
    state: &GoalMissionControlState,
    theme: &ThemeTokens,
) {
    let block = Block::default()
        .title(" Main Agent ")
        .borders(Borders::ALL)
        .border_style(theme.fg_dim);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let reasoning = state
        .main_reasoning_effort()
        .map(str::to_string)
        .unwrap_or_else(|| "none".to_string());
    let save_default = if state.save_as_default_pending {
        "pending"
    } else {
        "off"
    };
    let content = vec![
        Line::from(Span::styled("Main model", theme.accent_secondary)),
        Line::from(vec![
            Span::styled("Provider: ", theme.fg_dim),
            Span::styled(state.main_provider(), theme.fg_active),
        ]),
        Line::from(vec![
            Span::styled("Model: ", theme.fg_dim),
            Span::styled(state.main_model(), theme.fg_active),
            Span::styled("  Reasoning: ", theme.fg_dim),
            Span::styled(reasoning, theme.fg_active),
        ]),
        Line::from(vec![
            Span::styled("Preset source: ", theme.fg_dim),
            Span::styled(state.preset_source_label.as_str(), theme.fg_active),
        ]),
        Line::from(vec![
            Span::styled("Save as default: ", theme.fg_dim),
            Span::styled(save_default, theme.fg_active),
        ]),
    ];
    frame.render_widget(Paragraph::new(content).wrap(Wrap { trim: false }), inner);
}

fn render_role_assignments_section(
    frame: &mut Frame,
    area: Rect,
    state: &GoalMissionControlState,
    theme: &ThemeTokens,
) {
    let block = Block::default()
        .title(" Role Assignments ")
        .borders(Borders::ALL)
        .border_style(theme.fg_dim);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let mut lines = Vec::with_capacity(state.role_assignments.len().saturating_add(2));
    lines.push(Line::from(Span::styled(
        "Role assignments",
        theme.accent_secondary,
    )));

    if state.role_assignments.is_empty() {
        lines.push(Line::from(Span::styled(
            "No role assignments loaded.",
            theme.fg_dim,
        )));
    } else {
        for assignment in &state.role_assignments {
            let reasoning = assignment
                .reasoning_effort
                .as_deref()
                .filter(|value| !value.trim().is_empty())
                .unwrap_or("none");
            let inherit_label = if assignment.inherit_from_main {
                "inherits main"
            } else {
                "custom"
            };
            lines.push(Line::from(vec![
                Span::styled(format!("{}: ", assignment.role_id), theme.fg_active),
                Span::styled(
                    format!(
                        "{} / {} / {} ({})",
                        assignment.provider, assignment.model, reasoning, inherit_label
                    ),
                    theme.fg_dim,
                ),
            ]));
        }
    }

    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
}

fn render_thread_router_section(
    frame: &mut Frame,
    area: Rect,
    can_open_active_thread: bool,
    theme: &ThemeTokens,
) {
    let block = Block::default()
        .title(" Thread Router ")
        .borders(Borders::ALL)
        .border_style(theme.fg_dim);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let button_style = if can_open_active_thread {
        theme.accent_secondary
    } else {
        theme.fg_dim
    };
    let status = if can_open_active_thread {
        "Open the goal's active thread, or fall back to the root goal thread."
    } else {
        "No active or root goal thread is available for this Mission Control source."
    };
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(vec![
                Span::styled(OPEN_ACTIVE_THREAD_LABEL, button_style),
                Span::styled("  Inspect the live goal thread", theme.fg_dim),
            ]),
            Line::from(Span::styled(status, theme.fg_dim)),
        ]),
        inner,
    );
}

fn render_footer(frame: &mut Frame, area: Rect, theme: &ThemeTokens) {
    let footer = Line::from(vec![
        Span::styled("Enter", theme.fg_active),
        Span::styled(" launch  ", theme.fg_dim),
        Span::styled("Ctrl+O", theme.fg_active),
        Span::styled(" open thread  ", theme.fg_dim),
        Span::styled("Esc", theme.fg_active),
        Span::styled(" back to conversation", theme.fg_dim),
    ]);
    frame.render_widget(Paragraph::new(footer), area);
}

fn thread_router_area(area: Rect) -> Option<Rect> {
    if area.width == 0 || area.height == 0 {
        return None;
    }
    let inner = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Double)
        .inner(area);
    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(5),
            Constraint::Length(8),
            Constraint::Min(7),
            Constraint::Length(3),
            Constraint::Length(2),
        ])
        .split(inner);
    sections.get(3).copied()
}

fn open_active_thread_button_area(area: Rect) -> Option<Rect> {
    let inner = Block::default().borders(Borders::ALL).inner(area);
    button_area(inner, OPEN_ACTIVE_THREAD_LABEL)
}

fn button_area(inner: Rect, label: &str) -> Option<Rect> {
    if inner.width == 0 || inner.height == 0 {
        return None;
    }

    Some(Rect::new(
        inner.x,
        inner.y,
        label.chars().count().min(inner.width as usize) as u16,
        1,
    ))
}

fn point_in_rect(rect: Rect, point: Position) -> bool {
    point.x >= rect.x
        && point.x < rect.x.saturating_add(rect.width)
        && point.y >= rect.y
        && point.y < rect.y.saturating_add(rect.height)
}

#[cfg(test)]
#[path = "tests/goal_mission_control.rs"]
mod tests;
