use ratatui::prelude::*;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};

use crate::state::{CollaborationPaneFocus, CollaborationRowVm, CollaborationState};
use crate::theme::ThemeTokens;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CollaborationHitTarget {
    Row(usize),
    DetailAction(usize),
}

pub fn render(
    frame: &mut Frame,
    area: Rect,
    collaboration: &CollaborationState,
    theme: &ThemeTokens,
    focused: bool,
) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Collaboration ")
        .border_style(if focused {
            theme.accent_primary
        } else {
            theme.fg_dim
        });
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.width < 20 || inner.height < 4 {
        return;
    }

    let panes = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(42), Constraint::Percentage(58)])
        .split(inner);
    render_rows(frame, panes[0], collaboration, theme);
    render_detail(frame, panes[1], collaboration, theme);
}

pub fn hit_test(
    area: Rect,
    collaboration: &CollaborationState,
    position: Position,
) -> Option<CollaborationHitTarget> {
    let block = Block::default().borders(Borders::ALL);
    let inner = block.inner(area);
    if inner.width < 20 || inner.height < 4 {
        return None;
    }
    let panes = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(42), Constraint::Percentage(58)])
        .split(inner);
    let rows_area = panes[0];
    let detail_area = panes[1];

    if position.x >= rows_area.x
        && position.x < rows_area.x + rows_area.width
        && position.y > rows_area.y
        && position.y < rows_area.y + rows_area.height
    {
        let row_index = (position.y - rows_area.y - 1) as usize;
        if row_index < collaboration.rows().len() {
            return Some(CollaborationHitTarget::Row(row_index));
        }
    }

    if let Some(disagreement) = collaboration.selected_disagreement() {
        let action_y = detail_area.y + 5;
        if position.y == action_y
            && position.x >= detail_area.x
            && position.x < detail_area.x + detail_area.width
        {
            let mut current_x = detail_area.x + 1;
            for (index, position_label) in disagreement.positions.iter().enumerate() {
                let width = position_label.len() as u16 + 8;
                if position.x >= current_x && position.x < current_x + width {
                    return Some(CollaborationHitTarget::DetailAction(index));
                }
                current_x = current_x.saturating_add(width + 1);
            }
        }
    }

    None
}

fn render_rows(
    frame: &mut Frame,
    area: Rect,
    collaboration: &CollaborationState,
    theme: &ThemeTokens,
) {
    let selected_style = Style::default().bg(Color::Indexed(236));
    let mut lines = vec![Line::from(vec![
        Span::styled("Sessions ", theme.fg_dim),
        Span::styled(collaboration.rows().len().to_string(), theme.fg_active),
    ])];

    for (index, row) in collaboration.rows().iter().enumerate() {
        let is_selected = collaboration.selected_row_index() == index
            && collaboration.focus() == CollaborationPaneFocus::Navigator;
        let style = if is_selected {
            selected_style
        } else {
            Style::default()
        };
        let prefix = match row {
            CollaborationRowVm::Session { .. } => "● ",
            CollaborationRowVm::Disagreement { .. } => "  ↳ ",
        };
        let label = row.disagreement_id().unwrap_or_else(|| row.session_id());
        lines.push(Line::from(vec![
            Span::styled(prefix, theme.accent_primary.patch(style)),
            Span::styled(label.to_string(), theme.fg_active.patch(style)),
        ]));
    }

    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), area);
}

fn render_detail(
    frame: &mut Frame,
    area: Rect,
    collaboration: &CollaborationState,
    theme: &ThemeTokens,
) {
    let mut lines = Vec::new();
    if let Some(session) = collaboration.selected_session() {
        lines.push(Line::from(vec![
            Span::styled("Session ", theme.fg_dim),
            Span::styled(session.id.clone(), theme.fg_active),
        ]));
        if let Some(escalation) = &session.escalation {
            lines.push(Line::from(Span::styled(
                format!(
                    "Escalation {}->{}: {}",
                    escalation.from_level, escalation.to_level, escalation.reason
                ),
                theme.accent_secondary,
            )));
        }
    }
    if let Some(disagreement) = collaboration.selected_disagreement() {
        lines.push(Line::from(Span::styled(
            disagreement.topic.clone(),
            theme.fg_active.add_modifier(Modifier::BOLD),
        )));
        lines.push(Line::from(Span::styled(
            format!("Votes: {}", disagreement.vote_count),
            theme.fg_dim,
        )));
        lines.push(Line::from(Span::styled("Positions", theme.fg_dim)));

        let action_style = |selected: bool| {
            if selected && collaboration.focus() == CollaborationPaneFocus::Detail {
                theme.accent_secondary.add_modifier(Modifier::BOLD)
            } else {
                theme.fg_active
            }
        };
        let actions = disagreement
            .positions
            .iter()
            .enumerate()
            .flat_map(|(index, label)| {
                let mut spans = vec![Span::styled(
                    format!("[Vote {label}]"),
                    action_style(collaboration.selected_detail_action_index() == index),
                )];
                spans.push(Span::raw(" "));
                spans
            })
            .collect::<Vec<_>>();
        lines.push(Line::from(actions));
    } else {
        lines.push(Line::from(Span::styled(
            "Select a disagreement to inspect details",
            theme.fg_dim,
        )));
    }

    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), area);
}
