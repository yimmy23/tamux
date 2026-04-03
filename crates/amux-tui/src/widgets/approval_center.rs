use ratatui::prelude::*;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};

use crate::state::approval::{ApprovalFilter, ApprovalState};
use crate::theme::ThemeTokens;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApprovalCenterHitTarget {
    Filter(ApprovalFilter),
    Row(usize),
    ThreadJump(String),
    ApproveOnce(String),
    ApproveSession(String),
    Deny(String),
    Close,
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
    if let Some(filter_hit) = header_hit_test(header_area, approval.pending_approvals().len(), position) {
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
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("Ctrl+A", theme.accent_primary),
            Span::styled(" queue  ", theme.fg_dim),
            Span::styled("a", theme.accent_success),
            Span::styled(" approve once  ", theme.fg_dim),
            Span::styled("s", theme.accent_secondary),
            Span::styled(" approve session  ", theme.fg_dim),
            Span::styled("d", theme.accent_danger),
            Span::styled(" deny", theme.fg_dim),
        ])),
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

fn render_detail(
    frame: &mut Frame,
    area: Rect,
    approval: &ApprovalState,
    current_thread_id: Option<&str>,
    current_workspace_id: Option<&str>,
    theme: &ThemeTokens,
) {
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
                selected
                    .task_title
                    .as_deref()
                    .unwrap_or(selected.task_id.as_str())
                    .to_string(),
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
            Span::styled("[Deny]", theme.accent_danger),
        ])),
        action_row,
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

fn queue_hit_test(
    area: Rect,
    approval: &ApprovalState,
    current_thread_id: Option<&str>,
    current_workspace_id: Option<&str>,
    position: Position,
) -> Option<ApprovalCenterHitTarget> {
    let block = Block::default().borders(Borders::ALL);
    let inner = block.inner(area);
    if position.x < inner.x
        || position.x >= inner.x.saturating_add(inner.width)
        || position.y < inner.y
        || position.y >= inner.y.saturating_add(inner.height)
    {
        return None;
    }
    let row = position.y.saturating_sub(inner.y) as usize;
    let item_index = row / 3;
    approval
        .visible_approvals(current_thread_id, current_workspace_id)
        .get(item_index)
        .map(|_| ApprovalCenterHitTarget::Row(item_index))
}

fn detail_hit_test(
    area: Rect,
    approval: &ApprovalState,
    current_thread_id: Option<&str>,
    current_workspace_id: Option<&str>,
    position: Position,
) -> Option<ApprovalCenterHitTarget> {
    let block = Block::default().borders(Borders::ALL);
    let inner = block.inner(area);
    if position.x < inner.x
        || position.x >= inner.x.saturating_add(inner.width)
        || position.y < inner.y
        || position.y >= inner.y.saturating_add(inner.height)
    {
        return None;
    }

    let selected = approval.selected_visible_approval(current_thread_id, current_workspace_id)?;

    if position.y == inner.y.saturating_add(1)
        && position.x >= inner.x.saturating_add(8)
        && position.x < inner.x.saturating_add(32)
    {
        return selected
            .thread_id
            .clone()
            .map(ApprovalCenterHitTarget::ThreadJump);
    }

    let action_y = inner.y.saturating_add(inner.height.saturating_sub(2));
    if position.y != action_y {
        return None;
    }
    let approve_once_x = inner.x.saturating_add(20);
    let approve_once_w = 14;
    let approve_session_x = approve_once_x.saturating_add(16);
    let approve_session_w = 17;
    let deny_x = approve_session_x.saturating_add(19);
    let deny_w = 6;

    if position.x >= approve_once_x && position.x < approve_once_x.saturating_add(approve_once_w) {
        return Some(ApprovalCenterHitTarget::ApproveOnce(
            selected.approval_id.clone(),
        ));
    }
    if position.x >= approve_session_x
        && position.x < approve_session_x.saturating_add(approve_session_w)
    {
        return Some(ApprovalCenterHitTarget::ApproveSession(
            selected.approval_id.clone(),
        ));
    }
    if position.x >= deny_x && position.x < deny_x.saturating_add(deny_w) {
        return Some(ApprovalCenterHitTarget::Deny(selected.approval_id.clone()));
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{ApprovalAction, ApprovalState, PendingApproval, RiskLevel};
    use crate::theme::ThemeTokens;
    use ratatui::backend::TestBackend;
    use ratatui::layout::{Position, Rect};
    use ratatui::Terminal;

    fn make_approval(
        approval_id: &str,
        task_title: &str,
        thread_id: &str,
        workspace_id: &str,
    ) -> PendingApproval {
        PendingApproval {
            approval_id: approval_id.into(),
            task_id: format!("task-{approval_id}"),
            task_title: Some(task_title.into()),
            thread_id: Some(thread_id.into()),
            thread_title: Some(format!("Thread {thread_id}")),
            workspace_id: Some(workspace_id.into()),
            rationale: Some("Needed to continue execution".into()),
            reasons: vec!["network access requested".into()],
            command: "git clone https://example.com/repo.git".into(),
            risk_level: RiskLevel::High,
            blast_radius: "workspace".into(),
            received_at: 1,
            seen_at: None,
        }
    }

    #[test]
    fn approval_center_renders_without_panicking() {
        let mut approvals = ApprovalState::new();
        approvals.reduce(ApprovalAction::ApprovalRequired(make_approval(
            "a1", "WELES", "thread-1", "ws-1",
        )));
        approvals.reduce(ApprovalAction::SetFilter(ApprovalFilter::AllPending));

        let backend = TestBackend::new(100, 30);
        let mut terminal = Terminal::new(backend).expect("test terminal should initialize");

        terminal
            .draw(|frame| {
                render(
                    frame,
                    Rect::new(0, 0, 100, 30),
                    &approvals,
                    Some("thread-1"),
                    Some("ws-1"),
                    &ThemeTokens::default(),
                )
            })
            .expect("approval center render should succeed");
    }

    #[test]
    fn hit_test_targets_queue_row() {
        let mut approvals = ApprovalState::new();
        approvals.reduce(ApprovalAction::ApprovalRequired(make_approval(
            "a1", "WELES", "thread-1", "ws-1",
        )));

        let hit = hit_test(
            Rect::new(0, 0, 100, 30),
            &approvals,
            Some("thread-1"),
            Some("ws-1"),
            Position::new(3, 5),
        );

        assert_eq!(hit, Some(ApprovalCenterHitTarget::Row(0)));
    }

    #[test]
    fn hit_test_targets_filter_after_double_digit_pending_prefix() {
        let mut approvals = ApprovalState::new();
        for index in 0..10 {
            approvals.reduce(ApprovalAction::ApprovalRequired(make_approval(
                &format!("a{index}"),
                "WELES",
                "thread-1",
                "ws-1",
            )));
        }

        let prefix_width = format!("{} pending  ", approvals.pending_approvals().len())
            .chars()
            .count() as u16;
        let hit = hit_test(
            Rect::new(0, 0, 100, 30),
            &approvals,
            Some("thread-1"),
            Some("ws-1"),
            Position::new(1 + prefix_width + 1, 1),
        );

        assert_eq!(
            hit,
            Some(ApprovalCenterHitTarget::Filter(ApprovalFilter::AllPending))
        );
    }

    #[test]
    fn hit_test_targets_approve_action() {
        let mut approvals = ApprovalState::new();
        approvals.reduce(ApprovalAction::ApprovalRequired(make_approval(
            "a1", "WELES", "thread-1", "ws-1",
        )));

        let hit = hit_test(
            Rect::new(0, 0, 100, 30),
            &approvals,
            Some("thread-1"),
            Some("ws-1"),
            Position::new(64, 26),
        );

        assert_eq!(
            hit,
            Some(ApprovalCenterHitTarget::ApproveOnce("a1".to_string()))
        );
    }

    #[test]
    fn approval_center_wraps_long_detail_text() {
        let mut approvals = ApprovalState::new();
        let mut approval = make_approval("a1", "WELES", "thread-1", "ws-1");
        approval.rationale = Some(
            "Cloning scientific skills repository from GitHub as part of WELES governance review task TAILTOKEN"
                .to_string(),
        );
        approval.command = "git clone https://github.com/example/scientific-skills.git".to_string();
        approvals.reduce(ApprovalAction::ApprovalRequired(approval));

        let backend = TestBackend::new(70, 20);
        let mut terminal = Terminal::new(backend).expect("test terminal should initialize");

        terminal
            .draw(|frame| {
                render(
                    frame,
                    Rect::new(0, 0, 70, 20),
                    &approvals,
                    Some("thread-1"),
                    Some("ws-1"),
                    &ThemeTokens::default(),
                )
            })
            .expect("approval center render should succeed");

        let buffer = terminal.backend().buffer();
        let rendered = (0..20)
            .map(|y| {
                (0..70)
                    .filter_map(|x| buffer.cell((x, y)).map(|cell| cell.symbol()))
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            rendered.contains("TAILTOKEN"),
            "long approval rationale should wrap instead of truncating: {rendered}"
        );
    }
}
