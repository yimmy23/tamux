use ratatui::prelude::*;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};

use crate::state::approval::{ApprovalFilter, ApprovalState};
use crate::theme::ThemeTokens;

use super::queue_hit_test_to_rule_detail_hit_test::{detail_hit_test, queue_hit_test};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApprovalCenterHitTarget {
    Filter(ApprovalFilter),
    Row(usize),
    RuleRow(usize),
    ThreadJump(String),
    ApproveOnce(String),
    ApproveSession(String),
    AlwaysApprove(String),
    RevokeRule(String),
    Deny(String),
}

pub fn render(
    frame: &mut Frame,
    area: Rect,
    approval: &ApprovalState,
    current_thread_id: Option<&str>,
    current_workspace_id: Option<&str>,
    theme: &ThemeTokens,
) {
    frame.render_widget(Clear, area);
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Approval Center ")
        .border_style(theme.fg_dim);
    frame.render_widget(block.clone(), area);
    let inner = block.inner(area);
    if inner.width < 24 || inner.height < 8 {
        return;
    }

    let header_area = Rect::new(inner.x, inner.y, inner.width, 2);
    let body_area = Rect::new(
        inner.x,
        inner.y.saturating_add(2),
        inner.width,
        inner.height.saturating_sub(2),
    );
    let panes = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
        .split(body_area);
    let queue_area = panes[0];
    let detail_area = panes[1];

    render_header(frame, header_area, approval, theme);
    render_queue(
        frame,
        queue_area,
        approval,
        current_thread_id,
        current_workspace_id,
        theme,
    );
    render_detail(
        frame,
        detail_area,
        approval,
        current_thread_id,
        current_workspace_id,
        theme,
    );
}

pub fn hit_test(
    area: Rect,
    approval: &ApprovalState,
    current_thread_id: Option<&str>,
    current_workspace_id: Option<&str>,
    position: Position,
) -> Option<ApprovalCenterHitTarget> {
    let block = Block::default().borders(Borders::ALL);
    let inner = block.inner(area);
    if inner.width < 24 || inner.height < 8 {
        return None;
    }

    let header_area = Rect::new(inner.x, inner.y, inner.width, 2);
    if let Some(filter_hit) =
        header_hit_test(header_area, approval.pending_approvals().len(), position)
    {
        return Some(filter_hit);
    }

    let body_area = Rect::new(
        inner.x,
        inner.y.saturating_add(2),
        inner.width,
        inner.height.saturating_sub(2),
    );
    let panes = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
        .split(body_area);
    let queue_area = panes[0];
    let detail_area = panes[1];

    if let Some(row_hit) = queue_hit_test(
        queue_area,
        approval,
        current_thread_id,
        current_workspace_id,
        position,
    ) {
        return Some(row_hit);
    }

    detail_hit_test(
        detail_area,
        approval,
        current_thread_id,
        current_workspace_id,
        position,
    )
}

fn render_header(frame: &mut Frame, area: Rect, approval: &ApprovalState, theme: &ThemeTokens) {
    let buttons = [
        (ApprovalFilter::AllPending, "All pending"),
        (ApprovalFilter::CurrentThread, "Current thread"),
        (ApprovalFilter::CurrentWorkspace, "Current workspace"),
        (ApprovalFilter::SavedRules, "Always approved"),
    ];
    let mut spans = vec![Span::styled(
        format!("{} pending  ", approval.pending_approvals().len()),
        theme.fg_dim,
    )];
    for (index, (filter, label)) in buttons.iter().enumerate() {
        let style = if approval.filter() == *filter {
            theme.accent_secondary.add_modifier(Modifier::BOLD)
        } else {
            theme.fg_active
        };
        spans.push(Span::styled(*label, style));
        if index < buttons.len() - 1 {
            spans.push(Span::styled("  |  ", theme.fg_dim));
        }
    }
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
    let help = if approval.filter() == ApprovalFilter::SavedRules {
        vec![
            Span::styled("Ctrl+A", theme.accent_primary),
            Span::styled(" approvals  ", theme.fg_dim),
            Span::styled("r", theme.accent_danger),
            Span::styled(" revoke rule", theme.fg_dim),
        ]
    } else {
        vec![
            Span::styled("Ctrl+A", theme.accent_primary),
            Span::styled(" queue  ", theme.fg_dim),
            Span::styled("a", theme.accent_success),
            Span::styled(" approve once  ", theme.fg_dim),
            Span::styled("s", theme.accent_secondary),
            Span::styled(" approve session  ", theme.fg_dim),
            Span::styled("w", theme.accent_primary),
            Span::styled(" always approve  ", theme.fg_dim),
            Span::styled("d", theme.accent_danger),
            Span::styled(" deny", theme.fg_dim),
        ]
    };
    frame.render_widget(
        Paragraph::new(Line::from(help)),
        Rect::new(area.x, area.y.saturating_add(1), area.width, 1),
    );
}

fn render_queue(
    frame: &mut Frame,
    area: Rect,
    approval: &ApprovalState,
    current_thread_id: Option<&str>,
    current_workspace_id: Option<&str>,
    theme: &ThemeTokens,
) {
    if approval.filter() == ApprovalFilter::SavedRules {
        render_rules(frame, area, approval, theme);
        return;
    }

    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Pending ")
        .border_style(theme.fg_dim);
    frame.render_widget(block.clone(), area);
    let inner = block.inner(area);

    let items = approval.visible_approvals(current_thread_id, current_workspace_id);
    if items.is_empty() {
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                "No pending approvals",
                theme.fg_dim,
            ))),
            inner,
        );
        return;
    }

    let mut lines = Vec::new();
    for (index, item) in items.iter().enumerate() {
        let selected = approval.selected_approval_id() == Some(item.approval_id.as_str());
        let style = if selected {
            theme.accent_secondary.add_modifier(Modifier::BOLD)
        } else {
            theme.fg_active
        };
        let title = item.task_title.as_deref().unwrap_or(item.task_id.as_str());
        let thread = item
            .thread_title
            .as_deref()
            .or(item.thread_id.as_deref())
            .unwrap_or("-");
        lines.push(Line::from(Span::styled(
            format!("{} {}", if selected { ">" } else { " " }, title),
            style,
        )));
        lines.push(Line::from(vec![
            Span::raw("  "),
            Span::styled(thread.to_string(), theme.fg_dim),
            Span::styled("  •  ", theme.fg_dim),
            Span::styled(item.blast_radius.to_string(), theme.fg_dim),
        ]));
        if index < items.len() - 1 {
            lines.push(Line::raw(""));
        }
    }

    frame.render_widget(Paragraph::new(lines), inner);
}

fn render_rules(frame: &mut Frame, area: Rect, approval: &ApprovalState, theme: &ThemeTokens) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Always Approved ")
        .border_style(theme.fg_dim);
    frame.render_widget(block.clone(), area);
    let inner = block.inner(area);

    if approval.saved_rules().is_empty() {
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                "No saved approval rules",
                theme.fg_dim,
            ))),
            inner,
        );
        return;
    }

    let mut lines = Vec::new();
    for (index, rule) in approval.saved_rules().iter().enumerate() {
        let selected = approval.selected_rule_id() == Some(rule.id.as_str());
        let style = if selected {
            theme.accent_secondary.add_modifier(Modifier::BOLD)
        } else {
            theme.fg_active
        };
        lines.push(Line::from(Span::styled(
            format!("{} {}", if selected { ">" } else { " " }, rule.command),
            style,
        )));
        lines.push(Line::from(vec![
            Span::raw("  "),
            Span::styled(format!("used {} time(s)", rule.use_count), theme.fg_dim),
        ]));
        if index < approval.saved_rules().len() - 1 {
            lines.push(Line::raw(""));
        }
    }

    frame.render_widget(Paragraph::new(lines), inner);
}

fn render_detail(
    frame: &mut Frame,
    area: Rect,
    approval: &ApprovalState,
    current_thread_id: Option<&str>,
    current_workspace_id: Option<&str>,
    theme: &ThemeTokens,
) {
    if approval.filter() == ApprovalFilter::SavedRules {
        render_rule_detail(frame, area, approval, theme);
        return;
    }

    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Details ")
        .border_style(theme.fg_dim);
    frame.render_widget(block.clone(), area);
    let inner = block.inner(area);
    let selected = approval.selected_visible_approval(current_thread_id, current_workspace_id);

    let Some(selected) = selected else {
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                "No approval selected",
                theme.fg_dim,
            ))),
            inner,
        );
        return;
    };

    let mut lines = vec![
        Line::from(vec![
            Span::styled("Task: ", theme.fg_dim),
            Span::styled(
                crate::widgets::message::truncate_to_width(
                    selected
                        .task_title
                        .as_deref()
                        .unwrap_or(selected.task_id.as_str()),
                    inner.width.saturating_sub(6) as usize,
                ),
                theme.fg_active,
            ),
        ]),
        Line::from(vec![
            Span::styled("Thread: ", theme.fg_dim),
            Span::styled(
                selected
                    .thread_title
                    .as_deref()
                    .or(selected.thread_id.as_deref())
                    .unwrap_or("-")
                    .to_string(),
                theme.accent_primary,
            ),
        ]),
        Line::from(vec![
            Span::styled("Risk: ", theme.fg_dim),
            Span::styled(format!("{:?}", selected.risk_level), theme.accent_danger),
        ]),
        Line::from(vec![
            Span::styled("Blast radius: ", theme.fg_dim),
            Span::styled(selected.blast_radius.to_string(), theme.fg_active),
        ]),
        Line::raw(""),
        Line::from(vec![
            Span::styled("Command: ", theme.fg_dim),
            Span::styled(selected.command.to_string(), theme.fg_active),
        ]),
    ];
    if let Some(rationale) = selected.rationale.as_deref() {
        lines.push(Line::from(vec![
            Span::styled("Reason: ", theme.fg_dim),
            Span::styled(rationale.to_string(), theme.fg_active),
        ]));
    }
    if !selected.reasons.is_empty() {
        lines.push(Line::raw(""));
        lines.push(Line::from(Span::styled("Reasons:", theme.fg_dim)));
        for reason in &selected.reasons {
            lines.push(Line::from(vec![
                Span::raw("  "),
                Span::styled(format!("- {reason}"), theme.fg_active),
            ]));
        }
    }

    let action_row = Rect::new(
        inner.x.saturating_add(20),
        inner.y.saturating_add(inner.height.saturating_sub(2)),
        inner.width.saturating_sub(22),
        1,
    );
    frame.render_widget(
        Paragraph::new(lines).wrap(Wrap { trim: false }),
        Rect::new(
            inner.x,
            inner.y,
            inner.width,
            inner.height.saturating_sub(3),
        ),
    );
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("[Approve once]", theme.accent_success),
            Span::styled("  ", Style::default()),
            Span::styled("[Approve session]", theme.accent_secondary),
            Span::styled("  ", Style::default()),
            Span::styled("[Always approve]", theme.accent_primary),
            Span::styled("  ", Style::default()),
            Span::styled("[Deny]", theme.accent_danger),
        ])),
        action_row,
    );
}

fn render_rule_detail(
    frame: &mut Frame,
    area: Rect,
    approval: &ApprovalState,
    theme: &ThemeTokens,
) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Rule Details ")
        .border_style(theme.fg_dim);
    frame.render_widget(block.clone(), area);
    let inner = block.inner(area);
    let Some(rule) = approval.selected_rule() else {
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled("No rule selected", theme.fg_dim))),
            inner,
        );
        return;
    };

    let lines = vec![
        Line::from(vec![
            Span::styled("Command: ", theme.fg_dim),
            Span::styled(rule.command.to_string(), theme.fg_active),
        ]),
        Line::from(vec![
            Span::styled("Use count: ", theme.fg_dim),
            Span::styled(rule.use_count.to_string(), theme.fg_active),
        ]),
        Line::from(vec![
            Span::styled("Created: ", theme.fg_dim),
            Span::styled(rule.created_at.to_string(), theme.fg_active),
        ]),
        Line::from(vec![
            Span::styled("Last used: ", theme.fg_dim),
            Span::styled(
                rule.last_used_at
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "never".to_string()),
                theme.fg_active,
            ),
        ]),
    ];

    frame.render_widget(
        Paragraph::new(lines).wrap(Wrap { trim: false }),
        Rect::new(
            inner.x,
            inner.y,
            inner.width,
            inner.height.saturating_sub(3),
        ),
    );
    frame.render_widget(
        Paragraph::new(Line::from(vec![Span::styled(
            "[Revoke rule]",
            theme.accent_danger,
        )])),
        Rect::new(
            inner.x.saturating_add(20),
            inner.y.saturating_add(inner.height.saturating_sub(2)),
            inner.width.saturating_sub(22),
            1,
        ),
    );
}

fn header_hit_test(
    area: Rect,
    pending_count: usize,
    position: Position,
) -> Option<ApprovalCenterHitTarget> {
    if position.y != area.y {
        return None;
    }

    let labels = [
        (ApprovalFilter::AllPending, "All pending"),
        (ApprovalFilter::CurrentThread, "Current thread"),
        (ApprovalFilter::CurrentWorkspace, "Current workspace"),
        (ApprovalFilter::SavedRules, "Always approved"),
    ];
    let mut x = area
        .x
        .saturating_add(format!("{} pending  ", pending_count).chars().count() as u16);
    for (filter, label) in labels {
        let width = label.chars().count() as u16;
        if position.x >= x && position.x < x.saturating_add(width) {
            return Some(ApprovalCenterHitTarget::Filter(filter));
        }
        x = x.saturating_add(width + 5);
    }
    None
}
