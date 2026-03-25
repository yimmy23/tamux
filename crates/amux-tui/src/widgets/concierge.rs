use ratatui::prelude::*;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::state::chat::{ChatState, MessageAction};
use crate::state::concierge::ConciergeState;
use crate::theme::ThemeTokens;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConciergeHitTarget {
    Action(usize),
}

/// Compute the clickable segments for action buttons.
pub fn action_line_segments(actions: &[MessageAction], area: Rect) -> Vec<(usize, u16, u16)> {
    let mut x = area.x.saturating_add(2);
    let mut segments = Vec::new();
    for (idx, action) in actions.iter().enumerate() {
        let label = format!("[{}]", action.label);
        let width = label.chars().count() as u16;
        segments.push((idx, x, x.saturating_add(width)));
        x = x.saturating_add(width + 1);
    }
    segments
}

/// Render the actions bar widget.
///
/// Shows actions from the active thread's last actionable message (concierge
/// welcome actions, assistant question choices, tool confirmations, etc.).
/// Message content is shown in the chat pane — this widget only shows buttons.
pub fn render(
    frame: &mut Frame,
    area: Rect,
    concierge: &ConciergeState,
    chat: &ChatState,
    theme: &ThemeTokens,
    focused: bool,
) {
    let actions = chat.active_actions();
    let has_actions = !actions.is_empty();

    if area.height == 0
        || area.width < 8
        || (!concierge.loading && !has_actions)
    {
        return;
    }

    let mut lines = Vec::new();

    if concierge.loading {
        // Single line: "Concierge ⣻ working…"
        let spinner_frames = ["\u{28bf}", "\u{28fb}", "\u{28fd}", "\u{28fe}", "\u{28f7}", "\u{28ef}", "\u{28df}", "\u{287f}"];
        let spinner = spinner_frames[(std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_millis() as usize)
            .unwrap_or(0)
            / 120)
            % spinner_frames.len()];
        lines.push(Line::from(vec![
            Span::styled("  Concierge ", theme.accent_primary),
            Span::styled(spinner, theme.accent_secondary),
            Span::raw(" "),
            Span::styled("working\u{2026}", theme.fg_dim),
        ]));
    } else if has_actions {
        // Action buttons row.
        let mut action_spans = vec![Span::raw("  ")];
        for (idx, action) in actions.iter().enumerate() {
            if idx > 0 {
                action_spans.push(Span::raw(" "));
            }
            let label = format!("[{}]", action.label);
            let style = if focused && concierge.selected_action == idx {
                theme.accent_primary
            } else {
                theme.fg_dim
            };
            action_spans.push(Span::styled(label, style));
        }
        lines.push(Line::from(action_spans));
    }

    frame.render_widget(Paragraph::new(lines), area);
}

pub fn hit_test(
    area: Rect,
    actions: &[MessageAction],
    selected_action: usize,
    mouse: Position,
) -> Option<ConciergeHitTarget> {
    if actions.is_empty()
        || mouse.x < area.x
        || mouse.x >= area.x.saturating_add(area.width)
        || mouse.y < area.y
        || mouse.y >= area.y.saturating_add(area.height)
    {
        return None;
    }

    let action_y = area.y.saturating_add(area.height.saturating_sub(1));
    if mouse.y != action_y {
        return None;
    }

    for (idx, start_x, end_x) in action_line_segments(actions, area) {
        if mouse.x >= start_x && mouse.x < end_x {
            return Some(ConciergeHitTarget::Action(idx));
        }
    }

    None
}
