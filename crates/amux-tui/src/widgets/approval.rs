use ratatui::prelude::*;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph, Wrap};

use crate::state::approval::{ApprovalState, RiskLevel};
use crate::theme::ThemeTokens;

pub fn render(frame: &mut Frame, area: Rect, approval: &ApprovalState, theme: &ThemeTokens) {
    let ap = approval.selected_approval();

    // Determine border color from risk level
    let (border_style, risk_label, risk_style) = match ap.map(|a| a.risk_level) {
        Some(RiskLevel::Critical) => (
            theme.accent_danger,
            "\u{2588} CRITICAL RISK \u{2588}",
            theme.accent_danger,
        ),
        Some(RiskLevel::High) => (
            theme.accent_danger,
            "\u{2588} HIGH RISK \u{2588}",
            theme.accent_danger,
        ),
        Some(RiskLevel::Medium) => (
            theme.accent_secondary,
            "\u{25b2} MEDIUM RISK \u{25b2}",
            theme.accent_secondary,
        ),
        Some(RiskLevel::Low) | None => (theme.fg_dim, "\u{25c6} LOW RISK \u{25c6}", theme.fg_dim),
    };

    let block = Block::default()
        .title(" APPROVAL REQUIRED ")
        .borders(Borders::ALL)
        .border_type(BorderType::Double)
        .border_style(border_style);

    let inner = block.inner(area);
    frame.render_widget(block, area);
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(2)])
        .split(inner);

    let mut lines: Vec<Line> = Vec::new();

    // Empty line
    lines.push(Line::raw(""));

    // Risk badge
    lines.push(Line::from(vec![
        Span::raw("  "),
        Span::styled(risk_label, risk_style),
    ]));

    // Empty line
    lines.push(Line::raw(""));

    if let Some(ap) = ap {
        // Command text
        lines.push(Line::from(vec![
            Span::raw("  "),
            Span::styled("Command: ", theme.fg_dim),
            Span::styled(&ap.command, theme.fg_active),
        ]));

        // Blast radius
        if !ap.blast_radius.is_empty() {
            lines.push(Line::from(vec![
                Span::raw("  "),
                Span::styled("Blast radius: ", theme.fg_dim),
                Span::styled(&ap.blast_radius, theme.fg_active),
            ]));
        }

        // Task title
        if let Some(task_title) = &ap.task_title {
            lines.push(Line::from(vec![
                Span::raw("  "),
                Span::styled("Task: ", theme.fg_dim),
                Span::styled(task_title.as_str(), theme.fg_dim),
            ]));
        }

        if let Some(rationale) = &ap.rationale {
            lines.push(Line::from(vec![
                Span::raw("  "),
                Span::styled("Reason: ", theme.fg_dim),
                Span::styled(rationale.as_str(), theme.fg_active),
            ]));
        }

        if !ap.reasons.is_empty() {
            lines.push(Line::from(vec![
                Span::raw("  "),
                Span::styled("Signals: ", theme.fg_dim),
                Span::styled(ap.reasons.join("; "), theme.fg_active),
            ]));
        }
    } else {
        lines.push(Line::from(Span::styled(
            "  No pending approvals.",
            theme.fg_dim,
        )));
    }

    let paragraph = Paragraph::new(lines).wrap(Wrap { trim: false });
    frame.render_widget(paragraph, layout[0]);

    let footer = Paragraph::new(vec![
        Line::from(Span::styled(
            "\u{2500}".repeat(inner.width as usize),
            theme.fg_dim,
        )),
        Line::from(vec![
            Span::raw("  "),
            Span::styled("[Y]", theme.accent_success),
            Span::styled(" Allow once  ", theme.fg_active),
            Span::styled("[A]", theme.accent_secondary),
            Span::styled(" Allow for session  ", theme.fg_active),
            Span::styled("[W]", theme.accent_primary),
            Span::styled(" Always approve  ", theme.fg_active),
            Span::styled("[N]", theme.accent_danger),
            Span::styled(" Reject", theme.fg_active),
        ]),
    ])
    .wrap(Wrap { trim: false });
    frame.render_widget(footer, layout[1]);
}

#[cfg(test)]
mod tests {
    use crate::state::approval::{ApprovalAction, ApprovalState, PendingApproval, RiskLevel};
    use crate::theme::ThemeTokens;
    use ratatui::backend::TestBackend;
    use ratatui::prelude::Rect;
    use ratatui::Terminal;

    fn render_lines(approval: &ApprovalState, width: u16, height: u16) -> Vec<String> {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).expect("test terminal should initialize");
        terminal
            .draw(|frame| {
                super::render(
                    frame,
                    Rect::new(0, 0, width, height),
                    approval,
                    &ThemeTokens::default(),
                )
            })
            .expect("approval render should succeed");

        let buffer = terminal.backend().buffer();
        (0..height)
            .map(|y| {
                (0..width)
                    .filter_map(|x| buffer.cell((x, y)).map(|cell| cell.symbol()))
                    .collect::<String>()
            })
            .collect()
    }

    fn make_approval(risk: RiskLevel, command: &str) -> PendingApproval {
        PendingApproval {
            approval_id: "ap1".into(),
            task_id: "t1".into(),
            task_title: Some("Deploy to production".into()),
            thread_id: None,
            thread_title: None,
            workspace_id: None,
            rationale: None,
            reasons: Vec::new(),
            command: command.into(),
            risk_level: risk,
            blast_radius: "production cluster".into(),
            received_at: 0,
            seen_at: None,
        }
    }

    #[test]
    fn approval_handles_empty_state() {
        let approval = ApprovalState::new();
        assert!(approval.current_approval().is_none());
    }

    #[test]
    fn approval_handles_pending_state() {
        let mut approval = ApprovalState::new();
        approval.reduce(ApprovalAction::ApprovalRequired(make_approval(
            RiskLevel::High,
            "kubectl delete pod",
        )));
        assert!(approval.selected_approval().is_some());
    }

    #[test]
    fn approval_keeps_actions_visible_when_details_wrap() {
        let mut approval = ApprovalState::new();
        let mut pending = make_approval(
            RiskLevel::Medium,
            "review low-confidence goal plan with a very long command that wraps heavily",
        );
        pending.task_title = Some(
            "Review plan: Meta-cognition in tamux: dynamic skill injection from self-reflection"
                .to_string(),
        );
        pending.rationale = Some(
            "Low-confidence steps require operator approval before execution: Step 1: [LOW] Find the minimal integration seam; Step 2: [LOW] Choose the smallest safe design; Step 3: [LOW] Implement dynamic skill injection; Step 4: [LOW] Run targeted verification and record results".to_string(),
        );
        approval.reduce(ApprovalAction::ApprovalRequired(pending));

        let lines = render_lines(&approval, 80, 14);
        assert!(
            lines.iter().any(|line| line.contains("Allow once")),
            "approval footer should remain visible even when body content wraps: {lines:?}"
        );
        assert!(
            lines.iter().any(|line| line.contains("Always approve")),
            "approval footer should include all actions: {lines:?}"
        );
    }
}
