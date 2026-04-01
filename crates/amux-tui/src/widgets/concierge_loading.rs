use ratatui::prelude::*;
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph};

use crate::theme::ThemeTokens;

fn lower_centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let popup_width = width.min(area.width);
    let popup_height = height.min(area.height);
    let x = area.x + area.width.saturating_sub(popup_width) / 2;
    let bottom_margin = 4u16.min(area.height.saturating_sub(popup_height));
    let y = area.y + area.height.saturating_sub(popup_height + bottom_margin);
    Rect::new(x, y, popup_width, popup_height)
}

fn morph_phase(tick: u64) -> usize {
    ((tick / 10) % 4) as usize
}

fn stage_label(tick: u64) -> &'static str {
    match (tick / 28) % 4 {
        0 => "Reading the thread surface",
        1 => "Cross-linking recent memory",
        2 => "Merging human and machine cues",
        _ => "Composing the welcome handoff",
    }
}

fn orbit_line(width: usize, tick: u64, reverse: bool) -> String {
    if width == 0 {
        return String::new();
    }

    let mut chars = vec!['.'; width];
    let step = ((tick / 3) as usize) % width;
    let primary = if reverse {
        width.saturating_sub(1).saturating_sub(step)
    } else {
        step
    };
    let secondary = if reverse {
        width
            .saturating_sub(1)
            .saturating_sub((step + width / 3) % width)
    } else {
        (step + width / 3) % width
    };
    chars[primary] = 'o';
    chars[secondary] = 'o';
    chars.into_iter().collect()
}

fn portrait_frame(tick: u64) -> &'static [&'static str] {
    match morph_phase(tick) {
        0 => &[
            "......      .-----.      ......",
            "....      .'  -  '.      ....",
            "...      /   o o   \\      ...",
            "..      |     ^     |      ..",
            "..      |    ---    |      ..",
            "...      \\   ===   /      ...",
            "....      '.___.'      ....",
        ],
        1 => &[
            "......      .--=--.      ......",
            "....      .'  -  '.      ....",
            "...      /   o #   \\      ...",
            "..      |    /_\\    |      ..",
            "..      |   <-=>    |      ..",
            "...      \\   ===   /      ...",
            "....      '._=_.'      ....",
        ],
        2 => &[
            "......      .-===-.      ......",
            "....      .' _=_ '.      ....",
            "...      /  [0 0]  \\      ...",
            "..      |    /_\\    |      ..",
            "..      |   [###]   |      ..",
            "...      \\  _===_  /      ...",
            "....      '._____.'      ....",
        ],
        _ => &[
            "......      .-#=#-.      ......",
            "....      .'_\\^/_'.      ....",
            "...      /  o>#<0  \\      ...",
            "..      |    /#\\    |      ..",
            "..      |   [_#_]   |      ..",
            "...      \\  =#=#=  /      ...",
            "....      '.__#.'      ....",
        ],
    }
}

fn glyph_style(ch: char, theme: &ThemeTokens) -> Style {
    if ch == '.' {
        theme.fg_dim
    } else if ch == ' ' {
        Style::default()
    } else {
        theme.fg_active.add_modifier(Modifier::BOLD)
    }
}

fn styled_glyph_line(text: &str, theme: &ThemeTokens) -> Line<'static> {
    let mut spans = Vec::new();
    let mut current_style = None;
    let mut current_text = String::new();

    for ch in text.chars() {
        let style = glyph_style(ch, theme);
        if current_style != Some(style) && !current_text.is_empty() {
            spans.push(Span::styled(
                std::mem::take(&mut current_text),
                current_style.expect("current style should exist"),
            ));
        }
        current_style = Some(style);
        current_text.push(ch);
    }

    if !current_text.is_empty() {
        spans.push(Span::styled(
            current_text,
            current_style.expect("final style should exist"),
        ));
    }

    Line::from(spans)
}

pub fn render(frame: &mut Frame, area: Rect, theme: &ThemeTokens, tick: u64) {
    if area.width < 56 || area.height < 12 {
        return;
    }

    frame.render_widget(Clear, area);

    let inner = lower_centered_rect(64, 12, area);
    let orbit_width = inner.width.saturating_sub(6) as usize;
    let mut lines = Vec::new();
    lines.push(Line::from(Span::styled(
        "Rarog is threading a welcome",
        theme.fg_active.add_modifier(Modifier::BOLD),
    )));
    lines.push(Line::from(Span::styled(stage_label(tick), theme.fg_dim)));
    lines.push(styled_glyph_line(
        &orbit_line(orbit_width, tick, false),
        theme,
    ));
    for row in portrait_frame(tick) {
        lines.push(styled_glyph_line(row, theme));
    }
    lines.push(styled_glyph_line(
        &orbit_line(orbit_width, tick + 5, true),
        theme,
    ));
    lines.push(Line::from(Span::styled(
        "dotfield sync: human intuition <-> machine recall",
        theme.fg_dim,
    )));

    frame.render_widget(Paragraph::new(lines).centered(), inner);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stage_label_cycles_through_loading_states() {
        assert_eq!(stage_label(0), "Reading the thread surface");
        assert_eq!(stage_label(28), "Cross-linking recent memory");
        assert_eq!(stage_label(56), "Merging human and machine cues");
        assert_eq!(stage_label(84), "Composing the welcome handoff");
    }

    #[test]
    fn portrait_frame_morphs_from_human_to_machine() {
        assert!(portrait_frame(0)[2].contains("o o"));
        assert!(portrait_frame(10)[2].contains("o #"));
        assert!(portrait_frame(20)[2].contains("[0 0]"));
        assert!(portrait_frame(30)[2].contains("o>#<0"));
    }

    #[test]
    fn orbit_line_advances_markers_over_time() {
        assert_ne!(orbit_line(24, 0, false), orbit_line(24, 6, false));
        assert_ne!(orbit_line(24, 0, true), orbit_line(24, 6, true));
    }
}
