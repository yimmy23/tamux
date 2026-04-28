#![allow(dead_code)]

use ratatui::prelude::*;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::state::audit::AuditState;
use crate::theme::ThemeTokens;

/// Render the audit feed as a scrollable list of audit entries.
pub fn render(frame: &mut Frame, area: Rect, state: &AuditState, theme: &ThemeTokens) {
    let lines = build_lines(state, theme, area.width as usize);
    let scroll = resolved_scroll(state, &lines, area.height as usize);
    let paragraph = Paragraph::new(lines).scroll((scroll as u16, 0));
    frame.render_widget(paragraph, area);
}

fn build_lines(state: &AuditState, theme: &ThemeTokens, width: usize) -> Vec<Line<'static>> {
    let filtered = state.filtered_entries();
    let mut lines = Vec::new();

    // Header
    lines.push(Line::from(vec![
        Span::styled("Audit Feed", theme.accent_primary),
        Span::styled(format!(" ({})", filtered.len()), theme.fg_dim),
        Span::styled("  d=dismiss", theme.fg_dim),
    ]));
    lines.push(Line::from(Span::styled(
        "\u{2500}".repeat(width.min(40)),
        theme.fg_dim,
    )));

    if filtered.is_empty() {
        lines.push(Line::from(Span::styled(
            " No actions recorded",
            theme.fg_dim,
        )));
        return lines;
    }

    let selected = state.selected_index();
    let sel_bg = Style::default().bg(Color::Indexed(236));

    for (idx, entry) in filtered.iter().enumerate() {
        let is_selected = idx == selected;
        let is_dismissed = entry.dismissed;
        let base_style = if is_dismissed {
            Style::default()
                .fg(Color::DarkGray)
                .add_modifier(ratatui::style::Modifier::CROSSED_OUT)
        } else if is_selected {
            sel_bg
        } else {
            Style::default()
        };

        // Type icon and color
        let (icon, type_style) = if is_dismissed {
            (
                action_type_icon(&entry.action_type),
                Style::default().fg(Color::DarkGray),
            )
        } else {
            action_type_style(&entry.action_type, theme)
        };

        // Build main line: [icon] summary
        let mut spans = vec![
            Span::raw(" "),
            Span::styled(icon.to_string(), type_style),
            Span::raw(" "),
            Span::styled(entry.summary.clone(), base_style),
        ];

        // Confidence dot + band (only if band != "confident")
        if let Some(band) = &entry.confidence_band {
            if band != "confident" {
                let dot_style = confidence_band_style(band, theme);
                let pct = entry
                    .confidence
                    .map(|c| format!(" {}%", (c * 100.0) as u32))
                    .unwrap_or_default();
                spans.push(Span::raw(" "));
                spans.push(Span::styled("\u{25cf}", dot_style));
                spans.push(Span::styled(format!(" {}{}", band, pct), theme.fg_dim));
            }
        }

        // Relative timestamp
        let age = format_relative_time(entry.timestamp);
        spans.push(Span::styled(format!("  {}", age), theme.fg_dim));

        lines.push(Line::from(spans));

        // Expanded details
        let is_expanded = state.expanded_entry() == Some(&entry.id);
        if is_expanded {
            if let Some(explanation) = &entry.explanation {
                if !explanation.is_empty() {
                    let max_text_width = width.saturating_sub(6);
                    for chunk in wrap_text(explanation, max_text_width) {
                        lines.push(Line::from(vec![
                            Span::raw("     "),
                            Span::styled(chunk, theme.fg_dim),
                        ]));
                    }
                }
            }
            if let Some(trace_id) = &entry.causal_trace_id {
                lines.push(Line::from(vec![
                    Span::raw("     "),
                    Span::styled(format!("trace: {}", trace_id), theme.fg_dim),
                ]));
            }
        }
    }

    lines
}

/// Returns the visible scroll offset so the selected entry remains in view.
fn resolved_scroll(state: &AuditState, lines: &[Line], body_height: usize) -> usize {
    let max_scroll = lines.len().saturating_sub(body_height);
    // Approximate: selected_index + 2 (header lines) maps to a line position
    let selected_line = state.selected_index().saturating_add(2);
    let mut scroll = 0;
    if selected_line >= body_height {
        scroll = selected_line.saturating_add(1).saturating_sub(body_height);
    }
    scroll.min(max_scroll)
}

/// Return only the icon character for an action type (used for dismissed entries).
fn action_type_icon(action_type: &str) -> &'static str {
    match action_type {
        "heartbeat" => "\u{2665}",          // heart
        "tool" => "\u{2699}",               // gear
        "escalation" => "\u{2191}",         // up arrow
        "skill" | "subagent" => "\u{2726}", // star
        _ => "\u{2022}",                    // bullet
    }
}

fn action_type_style(action_type: &str, theme: &ThemeTokens) -> (&'static str, Style) {
    match action_type {
        "heartbeat" => ("\u{2665}", theme.accent_primary), // heart
        "tool" => ("\u{2699}", theme.accent_primary),      // gear
        "escalation" => ("\u{2191}", theme.accent_secondary), // up arrow
        "skill" => ("\u{2726}", theme.accent_assistant),   // star
        "subagent" => ("\u{2726}", theme.accent_assistant), // star
        _ => ("\u{2022}", theme.fg_dim),                   // bullet
    }
}

fn confidence_band_style(band: &str, theme: &ThemeTokens) -> Style {
    match band {
        "confident" => theme.accent_success,
        "likely" => theme.accent_primary,
        "uncertain" => theme.accent_secondary,
        "guessing" => theme.accent_danger,
        _ => theme.fg_dim,
    }
}

fn format_relative_time(timestamp: u64) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let elapsed = now.saturating_sub(timestamp);
    if elapsed < 60 {
        format!("{}s ago", elapsed)
    } else if elapsed < 3600 {
        format!("{}m ago", elapsed / 60)
    } else if elapsed < 86400 {
        format!("{}h ago", elapsed / 3600)
    } else {
        format!("{}d ago", elapsed / 86400)
    }
}

/// Simple word-aware text wrapping for expanded explanation text.
fn wrap_text(text: &str, max_width: usize) -> Vec<String> {
    if max_width == 0 {
        return vec![text.to_string()];
    }
    let mut lines = Vec::new();
    let mut current = String::new();
    for word in text.split_whitespace() {
        if current.is_empty() {
            current = word.to_string();
        } else if current.len() + 1 + word.len() <= max_width {
            current.push(' ');
            current.push_str(word);
        } else {
            lines.push(current);
            current = word.to_string();
        }
    }
    if !current.is_empty() {
        lines.push(current);
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

/// Render escalation status as a formatted line for the footer/status area.
pub fn escalation_status_spans(state: &AuditState, theme: &ThemeTokens) -> Vec<Span<'static>> {
    let Some(esc) = state.current_escalation() else {
        return vec![];
    };
    let level_style = escalation_level_style(&esc.to_level, theme);
    vec![
        Span::styled(" Esc: ", theme.fg_dim),
        Span::styled(format!("{}->{}", esc.from_level, esc.to_level), level_style),
    ]
}

fn escalation_level_style(level: &str, theme: &ThemeTokens) -> Style {
    match level {
        "L0" => theme.accent_primary,
        "L1" => theme.accent_assistant,
        "L2" => theme.accent_secondary,
        "L3" => theme.accent_danger,
        _ => theme.fg_dim,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_relative_time_seconds() {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let result = format_relative_time(now - 30);
        assert!(result.contains("s ago"));
    }

    #[test]
    fn format_relative_time_minutes() {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let result = format_relative_time(now - 300);
        assert!(result.contains("m ago"));
    }

    #[test]
    fn format_relative_time_hours() {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let result = format_relative_time(now - 7200);
        assert!(result.contains("h ago"));
    }

    #[test]
    fn wrap_text_wraps_long_text() {
        let text = "This is a long text that should be wrapped across multiple lines for display";
        let wrapped = wrap_text(text, 20);
        assert!(wrapped.len() > 1);
        for line in &wrapped {
            assert!(line.len() <= 20 + 20); // word boundaries may slightly exceed
        }
    }

    #[test]
    fn wrap_text_empty_returns_one_empty_line() {
        let wrapped = wrap_text("", 40);
        assert_eq!(wrapped.len(), 1);
    }

    #[test]
    fn action_type_style_returns_correct_icon() {
        let theme = ThemeTokens::default();
        let (icon, _) = action_type_style("heartbeat", &theme);
        assert_eq!(icon, "\u{2665}");
        let (icon, _) = action_type_style("tool", &theme);
        assert_eq!(icon, "\u{2699}");
    }

    #[test]
    fn escalation_status_spans_empty_when_no_escalation() {
        let state = AuditState::new();
        let theme = ThemeTokens::default();
        assert!(escalation_status_spans(&state, &theme).is_empty());
    }

    #[test]
    fn escalation_status_spans_present_when_escalation_active() {
        let mut state = AuditState::new();
        state.reduce(crate::state::audit::AuditAction::EscalationUpdate(
            crate::state::audit::EscalationVm {
                thread_id: "t1".into(),
                from_level: "L0".into(),
                to_level: "L1".into(),
                reason: "test".into(),
                attempts: 1,
                audit_id: None,
            },
        ));
        let theme = ThemeTokens::default();
        let spans = escalation_status_spans(&state, &theme);
        assert!(!spans.is_empty());
    }
}
