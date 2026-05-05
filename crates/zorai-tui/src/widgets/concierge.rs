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
pub fn action_line_segments(
    actions: &[MessageAction],
    area: Rect,
    owner_label: Option<&str>,
) -> Vec<(usize, u16, u16)> {
    let owner_width = owner_label
        .map(str::trim)
        .filter(|label| !label.is_empty())
        .map(|label| label.chars().count() as u16 + 1)
        .unwrap_or(0);
    let mut x = area.x.saturating_add(2).saturating_add(owner_width);
    let mut segments = Vec::new();
    for (idx, action) in actions.iter().enumerate() {
        let label = format!("[{}]", action.label);
        let width = label.chars().count() as u16;
        segments.push((idx, x, x.saturating_add(width)));
        x = x.saturating_add(width + 1);
    }
    segments
}

/// Render the sticky thread activity/actions bar widget.
///
/// Shows actions from the active thread's last actionable message (concierge
/// welcome actions, assistant question choices, tool confirmations, etc.).
/// Message content is shown in the chat pane; this widget only shows buttons
/// and transient thread activity.
pub fn render(
    frame: &mut Frame,
    area: Rect,
    concierge: &ConciergeState,
    chat: &ChatState,
    theme: &ThemeTokens,
    focused: bool,
    thread_activity: Option<&str>,
    owner_label: &str,
) {
    let actions = chat.active_actions();
    let has_actions = !actions.is_empty();
    let thread_activity = thread_activity.filter(|activity| !activity.trim().is_empty());
    let owner_label = owner_label.trim();

    if area.height == 0
        || area.width < 8
        || (!concierge.loading && !has_actions && thread_activity.is_none())
    {
        return;
    }

    let mut lines = Vec::new();
    if area.height > 1 {
        lines.push(Line::raw(""));
    }
    let spinner_frames = [
        "\u{28bf}", "\u{28fb}", "\u{28fd}", "\u{28fe}", "\u{28f7}", "\u{28ef}", "\u{28df}",
        "\u{287f}",
    ];

    if concierge.loading {
        // Single line: "Concierge ⣻ working…"
        let status_frames = [
            "working",
            "grinding",
            "hustling",
            "thinking",
            "planning",
            "shipping",
            "analyzing",
            "crafting",
            "building",
            "designing",
            "processing",
            "computing",
            "executing",
        ];
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_millis() as usize)
            .unwrap_or(0);
        let spinner = spinner_frames[(now / 120) % spinner_frames.len()];
        let status = status_frames[(now / 2000) % status_frames.len()];
        lines.push(activity_line_spans(
            owner_label,
            spinner,
            &format!("{status}\u{2026}"),
            theme,
        ));
    } else if has_actions {
        // Action buttons row.
        let mut action_spans = owner_prefix_spans(owner_label, theme);
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
    } else if let Some(activity) = thread_activity {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_millis() as usize)
            .unwrap_or(0);
        let spinner = spinner_frames[(now / 120) % spinner_frames.len()];
        lines.push(activity_line_spans(
            owner_label,
            spinner,
            &format!("{activity}\u{2026}"),
            theme,
        ));
    }

    frame.render_widget(Paragraph::new(lines), area);
}

fn owner_prefix_spans<'a>(owner_label: &str, theme: &ThemeTokens) -> Vec<Span<'a>> {
    if owner_label.is_empty() {
        vec![Span::raw("  ")]
    } else {
        vec![
            Span::raw("  "),
            Span::styled(owner_label.to_string(), theme.accent_primary),
            Span::raw(" "),
        ]
    }
}

fn activity_line_spans<'a>(
    owner_label: &str,
    spinner: &str,
    activity: &str,
    theme: &ThemeTokens,
) -> Line<'a> {
    let mut spans = owner_prefix_spans(owner_label, theme);
    spans.push(Span::styled(spinner.to_string(), theme.accent_secondary));
    spans.push(Span::raw(" "));
    spans.push(Span::styled(activity.to_string(), theme.fg_dim));
    Line::from(spans)
}

pub fn hit_test(
    area: Rect,
    actions: &[MessageAction],
    _selected_action: usize,
    owner_label: Option<&str>,
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

    for (idx, start_x, end_x) in action_line_segments(actions, area, owner_label) {
        if mouse.x >= start_x && mouse.x < end_x {
            return Some(ConciergeHitTarget::Action(idx));
        }
    }

    None
}
