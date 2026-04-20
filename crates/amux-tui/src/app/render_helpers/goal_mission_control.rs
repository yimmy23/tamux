use super::*;

pub(super) fn render_goal_mission_control_preflight(
    frame: &mut Frame,
    area: Rect,
    state: &crate::state::goal_mission_control::GoalMissionControlState,
    theme: &ThemeTokens,
) {
    use ratatui::widgets::{Block, BorderType, Borders};

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
            Constraint::Length(2),
        ])
        .split(inner);

    render_prompt_section(frame, sections[0], state, theme);
    render_main_section(frame, sections[1], state, theme);
    render_role_assignments_section(frame, sections[2], state, theme);
    render_footer(frame, sections[3], theme);
}

fn render_prompt_section(
    frame: &mut Frame,
    area: Rect,
    state: &crate::state::goal_mission_control::GoalMissionControlState,
    theme: &ThemeTokens,
) {
    use ratatui::text::{Line, Span};
    use ratatui::widgets::{Block, Borders, Paragraph, Wrap};

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
    state: &crate::state::goal_mission_control::GoalMissionControlState,
    theme: &ThemeTokens,
) {
    use ratatui::text::{Line, Span};
    use ratatui::widgets::{Block, Borders, Paragraph, Wrap};

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
    state: &crate::state::goal_mission_control::GoalMissionControlState,
    theme: &ThemeTokens,
) {
    use ratatui::text::{Line, Span};
    use ratatui::widgets::{Block, Borders, Paragraph, Wrap};

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

fn render_footer(frame: &mut Frame, area: Rect, theme: &ThemeTokens) {
    use ratatui::text::{Line, Span};
    use ratatui::widgets::Paragraph;

    let footer = Line::from(vec![
        Span::styled("Enter", theme.fg_active),
        Span::styled(" launch  ", theme.fg_dim),
        Span::styled("Esc", theme.fg_active),
        Span::styled(" back to conversation", theme.fg_dim),
    ]);
    frame.render_widget(Paragraph::new(footer), area);
}
