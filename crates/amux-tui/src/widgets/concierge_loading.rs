use ratatui::prelude::*;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph};

use crate::theme::ThemeTokens;

fn lower_centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let popup_width = width.min(area.width);
    let popup_height = height.min(area.height);
    let x = area.x + area.width.saturating_sub(popup_width) / 2;
    let bottom_margin = 5u16.min(area.height.saturating_sub(popup_height));
    let y = area.y + area.height.saturating_sub(popup_height + bottom_margin);
    Rect::new(x, y, popup_width, popup_height)
}

fn pulse_frame(tick: u64) -> usize {
    ((tick / 6) % 4) as usize
}

fn stage_label(tick: u64) -> &'static str {
    match (tick / 24) % 4 {
        0 => "Checking your latest thread",
        1 => "Looking for unfinished work",
        2 => "Writing a short recap",
        _ => "Welcome almost ready",
    }
}

fn bar_cell_style(index: usize, pulse: usize, theme: &ThemeTokens) -> Style {
    match index.abs_diff(pulse) {
        0 => theme.accent_primary,
        1 => theme.accent_assistant,
        _ => theme.fg_dim,
    }
}

pub fn render(frame: &mut Frame, area: Rect, theme: &ThemeTokens, tick: u64) {
    if area.width < 48 || area.height < 10 {
        return;
    }

    frame.render_widget(Clear, area);

    let inner = lower_centered_rect(58, 7, area);

    let pulse = pulse_frame(tick);
    let mut lines = Vec::new();
    lines.push(Line::from(vec![Span::styled(
        "Concierge is getting your welcome ready",
        theme.accent_secondary,
    )]));
    lines.push(Line::from(vec![Span::styled(
        stage_label(tick),
        theme.fg_active,
    )]));

    let mut bars = Vec::new();
    for index in 0..4 {
        if index > 0 {
            bars.push(Span::raw(" "));
        }
        bars.push(Span::styled("■", bar_cell_style(index, pulse, theme)));
    }
    lines.push(Line::from(bars));
    lines.push(Line::from(vec![Span::styled(
        "Short recap only. Full triage stays available when you ask for it.",
        theme.fg_dim,
    )]));

    frame.render_widget(Paragraph::new(lines).centered(), inner);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stage_label_cycles_through_quieter_loading_states() {
        assert_eq!(stage_label(0), "Checking your latest thread");
        assert_eq!(stage_label(24), "Looking for unfinished work");
        assert_eq!(stage_label(48), "Writing a short recap");
        assert_eq!(stage_label(72), "Welcome almost ready");
    }
}
