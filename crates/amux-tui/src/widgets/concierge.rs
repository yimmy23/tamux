use ratatui::prelude::*;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use super::message::wrap_text;
use crate::state::concierge::ConciergeState;
use crate::theme::ThemeTokens;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConciergeHitTarget {
    Action(usize),
}

fn action_line_segments(state: &ConciergeState, area: Rect) -> Vec<(usize, u16, u16)> {
    let mut x = area.x.saturating_add(2);
    let mut segments = Vec::new();
    for (idx, action) in state.welcome_actions.iter().enumerate() {
        let label = format!("[{}]", action.label);
        let width = label.chars().count() as u16;
        segments.push((idx, x, x.saturating_add(width)));
        x = x.saturating_add(width + 1);
    }
    segments
}

pub fn render(
    frame: &mut Frame,
    area: Rect,
    state: &ConciergeState,
    theme: &ThemeTokens,
    focused: bool,
) {
    if area.height == 0 || area.width < 8 || (!state.loading && !state.welcome_visible) {
        return;
    }

    let inner_width = area.width.saturating_sub(4) as usize;
    let mut lines = Vec::new();
    lines.push(Line::from(vec![
        Span::styled("  Concierge", theme.accent_primary),
        Span::styled("  ", theme.fg_dim),
        if state.loading {
            Span::styled("working…", theme.accent_secondary)
        } else {
            Span::styled("ready", theme.accent_success)
        },
    ]));

    if state.loading {
        let spinner_frames = ["⢿", "⣻", "⣽", "⣾", "⣷", "⣯", "⣟", "⡿"];
        let spinner = spinner_frames[0];
        lines.push(Line::from(vec![
            Span::raw("  "),
            Span::styled(spinner, theme.accent_secondary),
            Span::raw(" "),
            Span::styled(
                "Preparing your welcome and suggested next actions",
                theme.fg_dim,
            ),
        ]));
    } else if let Some(content) = state.welcome_content.as_deref() {
        let wrapped = wrap_text(content, inner_width.max(24));
        for line in wrapped.into_iter().take(2) {
            lines.push(Line::from(vec![
                Span::raw("  "),
                Span::styled(line, theme.fg_active),
            ]));
        }
    }

    while lines.len() < area.height.saturating_sub(1) as usize {
        lines.push(Line::raw(""));
    }

    let mut action_spans = vec![Span::raw("  ")];
    if state.loading || state.welcome_actions.is_empty() {
        action_spans.push(Span::styled(
            "Wait for the concierge response…",
            theme.fg_dim,
        ));
    } else {
        for (idx, action) in state.welcome_actions.iter().enumerate() {
            if idx > 0 {
                action_spans.push(Span::raw(" "));
            }
            let label = format!("[{}]", action.label);
            let style = if focused && state.selected_action == idx {
                theme.accent_primary
            } else {
                theme.fg_dim
            };
            action_spans.push(Span::styled(label, style));
        }
    }
    lines.push(Line::from(action_spans));

    frame.render_widget(Paragraph::new(lines), area);
}

pub fn hit_test(area: Rect, state: &ConciergeState, mouse: Position) -> Option<ConciergeHitTarget> {
    if !state.welcome_visible
        || state.welcome_actions.is_empty()
        || mouse.x < area.x
        || mouse.x >= area.x.saturating_add(area.width)
        || mouse.y != area.y.saturating_add(area.height.saturating_sub(1))
    {
        return None;
    }

    action_line_segments(state, area)
        .into_iter()
        .find(|(_, start, end)| mouse.x >= *start && mouse.x < *end)
        .map(|(idx, _, _)| ConciergeHitTarget::Action(idx))
}
