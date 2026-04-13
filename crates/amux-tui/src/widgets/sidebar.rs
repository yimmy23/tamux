use ratatui::prelude::*;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::app::RecentActionVm;
use crate::state::chat::GatewayStatusVm;
use crate::state::sidebar::{SidebarState, SidebarTab};
use crate::state::task::TaskState;
use crate::state::tier::TierState;
use crate::theme::ThemeTokens;

#[path = "sidebar/tab_layout.rs"]
mod tab_layout;

use tab_layout::{tab_cells, tab_hit_test, tab_label};

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
                        entry
                            .kind
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
                        Span::styled(
                            if idx == selected { "> " } else { "  " },
                            theme.accent_primary,
                        ),
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

            todos
                .iter()
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
                            text.chars()
                                .take(max_len.saturating_sub(1))
                                .collect::<String>()
                        );
                    }
                    let line = Line::from(vec![
                        Span::styled(
                            if idx == selected { "> " } else { "  " },
                            theme.accent_primary,
                        ),
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

fn gateway_status_lines(statuses: &[GatewayStatusVm], theme: &ThemeTokens) -> Vec<Line<'static>> {
    // Only show gateway section if at least one platform is not disconnected
    let active: Vec<&GatewayStatusVm> = statuses
        .iter()
        .filter(|s| s.status != "disconnected")
        .collect();
    if active.is_empty() {
        return Vec::new();
    }

    let mut lines = Vec::new();
    lines.push(Line::from(Span::styled(
        " Gateway",
        Style::default()
            .fg(Color::Indexed(245))
            .add_modifier(ratatui::style::Modifier::BOLD),
    )));

    for gw in &active {
        let (indicator, color) = match gw.status.as_str() {
            "connected" => ("\u{25CF}", Color::Green),
            "error" => ("\u{25CF}", Color::Red),
            _ => ("\u{25CF}", Color::Indexed(245)),
        };
        let platform_label = match gw.platform.as_str() {
            "slack" => "Slack",
            "discord" => "Discord",
            "telegram" => "Telegram",
            other => other,
        };
        let mut spans = vec![
            Span::styled("  ", theme.fg_dim),
            Span::styled(indicator.to_string(), Style::default().fg(color)),
            Span::raw(" "),
            Span::styled(platform_label.to_string(), theme.fg_active),
        ];
        if gw.status == "error" {
            if let Some(ref err) = gw.last_error {
                let truncated: String = err.chars().take(30).collect();
                spans.push(Span::styled(
                    format!(" ({})", truncated),
                    Style::default().fg(Color::Red),
                ));
            }
        }
        lines.push(Line::from(spans));
    }
    lines
}

fn recent_actions_lines(actions: &[RecentActionVm], theme: &ThemeTokens) -> Vec<Line<'static>> {
    if actions.is_empty() {
        return Vec::new();
    }
    let mut lines = Vec::new();
    lines.push(Line::from(Span::styled(
        " Recent Actions",
        Style::default()
            .fg(Color::Indexed(245))
            .add_modifier(ratatui::style::Modifier::BOLD),
    )));
    for action in actions.iter().take(3) {
        let icon = match action.action_type.as_str() {
            "stale_todo" => "\u{2611}",    // ballot box with check
            "stuck_goal" => "\u{26A0}",    // warning
            "morning_brief" => "\u{2600}", // sun
            _ => "\u{25CB}",               // circle
        };
        let mut summary = action.summary.clone();
        if summary.chars().count() > 40 {
            summary = format!("{}...", summary.chars().take(37).collect::<String>());
        }
        lines.push(Line::from(vec![
            Span::styled("  ", theme.fg_dim),
            Span::styled(icon.to_string(), theme.fg_dim),
            Span::raw(" "),
            Span::styled(summary, theme.fg_active),
        ]));
    }
    lines
}

/// Render a dimmed one-line placeholder for a tier-locked sidebar section (D-05).
fn tier_placeholder_line(label: &str, required_tier: &str) -> Line<'static> {
    let dim = Style::default().fg(Color::DarkGray);
    Line::from(vec![
        Span::styled("  \u{25B6} ", dim),
        Span::styled(label.to_string(), dim),
        Span::styled(format!("  [{}]", required_tier.replace('_', " ")), dim),
    ])
}

/// Collect tier-gated placeholder lines for hidden sidebar sections.
fn tier_gated_lines(tier: &TierState) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    if !tier.show_goal_runs {
        lines.push(tier_placeholder_line("Goal Runs", "familiar"));
    }
    if !tier.show_task_queue {
        lines.push(tier_placeholder_line("Task Queue", "familiar"));
    }
    if !tier.show_gateway_config {
        lines.push(tier_placeholder_line("Gateway", "familiar"));
    }
    if !tier.show_subagents {
        lines.push(tier_placeholder_line("Sub-Agents", "power user"));
    }
    if !tier.show_memory_controls {
        lines.push(tier_placeholder_line("Memory", "expert"));
    }
    lines
}

#[cfg(test)]
fn agent_status_line(
    activity: Option<&str>,
    tier: &str,
    weles_health: Option<&crate::client::WelesHealthVm>,
) -> Line<'static> {
    let status_span = match activity {
        Some("thinking" | "reasoning" | "writing") => {
            Span::styled("\u{25CF} Thinking", Style::default().fg(Color::Yellow))
        }
        Some(s) if s.starts_with('\u{2699}') => {
            Span::styled(format!("\u{25CF} {}", s), Style::default().fg(Color::Blue))
        }
        Some("waiting_for_approval") => Span::styled(
            "\u{25CF} Awaiting approval",
            Style::default().fg(Color::Rgb(255, 165, 0)),
        ),
        Some("running_goal" | "goal_running") => {
            Span::styled("\u{25CF} Running goal", Style::default().fg(Color::Green))
        }
        Some("idle") | None => Span::styled("\u{25CF} Idle", Style::default().fg(Color::DarkGray)),
        Some(other) => Span::styled(
            format!("\u{25CF} {}", other),
            Style::default().fg(Color::DarkGray),
        ),
    };

    let tier_label = match tier {
        "newcomer" => "",
        "familiar" => " [familiar]",
        "power_user" => " [power user]",
        "expert" => " [expert]",
        _ => "",
    };

    let mut spans = vec![Span::raw(" "), status_span];
    if !tier_label.is_empty() {
        spans.push(Span::styled(
            tier_label.to_string(),
            Style::default().fg(Color::DarkGray),
        ));
    }
    if weles_health.is_some_and(|health| health.state.eq_ignore_ascii_case("degraded")) {
        spans.push(Span::styled(
            "  [WELES degraded]".to_string(),
            Style::default().fg(Color::LightYellow),
        ));
    }
    Line::from(spans)
}

pub fn render(
    frame: &mut Frame,
    area: Rect,
    sidebar: &SidebarState,
    tasks: &TaskState,
    thread_id: Option<&str>,
    theme: &ThemeTokens,
    _focused: bool,
    gateway_statuses: &[GatewayStatusVm],
    tier: &TierState,
    _agent_activity: Option<&str>,
    _weles_health: Option<&crate::client::WelesHealthVm>,
    recent_actions: &[RecentActionVm],
) {
    if area.height < 3 {
        return;
    }

    let gw_lines = if tier.show_gateway_config {
        gateway_status_lines(gateway_statuses, theme)
    } else {
        Vec::new()
    };
    let gw_height = gw_lines.len() as u16;

    let ra_lines = recent_actions_lines(recent_actions, theme);
    let ra_height = ra_lines.len() as u16;

    let tier_lines = tier_gated_lines(tier);
    let tier_height = tier_lines.len() as u16;

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // tab bar
            Constraint::Min(1),    // body
            Constraint::Length(gw_height),
            Constraint::Length(ra_height),
            Constraint::Length(tier_height),
        ])
        .split(area);

    // Agent status line at the very top

    for (tab, cell) in [
        (SidebarTab::Todos, tab_cells(chunks[0])[0]),
        (SidebarTab::Files, tab_cells(chunks[0])[1]),
    ] {
        let style = if sidebar.active_tab() == tab {
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

    let rows = rows_for_thread(tasks, sidebar, thread_id, theme, chunks[1].width as usize);
    let scroll = resolved_scroll(&rows, sidebar, chunks[1].height as usize);
    let paragraph = Paragraph::new(rows.into_iter().map(|row| row.line).collect::<Vec<_>>())
        .scroll((scroll as u16, 0));
    frame.render_widget(paragraph, chunks[1]);

    if !gw_lines.is_empty() {
        frame.render_widget(Paragraph::new(gw_lines), chunks[2]);
    }

    if !ra_lines.is_empty() {
        frame.render_widget(Paragraph::new(ra_lines), chunks[3]);
    }

    if !tier_lines.is_empty() {
        frame.render_widget(Paragraph::new(tier_lines), chunks[4]);
    }
}

pub fn body_item_count(
    tasks: &TaskState,
    sidebar: &SidebarState,
    thread_id: Option<&str>,
) -> usize {
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
    if area.height < 3
        || mouse.x < area.x
        || mouse.x >= area.x.saturating_add(area.width)
        || mouse.y < area.y
        || mouse.y >= area.y.saturating_add(area.height)
    {
        return None;
    }

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // tab bar
            Constraint::Min(1),    // body
        ])
        .split(area);

    if mouse.y == chunks[0].y {
        return tab_hit_test(chunks[0], mouse.x).map(SidebarHitTarget::Tab);
    }

    let rows = rows_for_thread(
        tasks,
        sidebar,
        thread_id,
        &ThemeTokens::default(),
        chunks[1].width as usize,
    );
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
#[path = "tests/sidebar.rs"]
mod tests;
