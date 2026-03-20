use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span, Text};

use crate::state::chat::{AgentMessage, MessageRole, TranscriptMode};
use crate::theme::ThemeTokens;

/// Render markdown content into Lines using tui-markdown.
/// Converts from ratatui_core types to ratatui types.
pub(crate) fn render_markdown_pub(content: &str, width: usize) -> Vec<Line<'static>> {
    render_markdown(content, width)
}

fn render_markdown(content: &str, width: usize) -> Vec<Line<'static>> {
    let md_text = tui_markdown::from_str(content);
    // Convert ratatui_core::Line to ratatui::Line via plain text + styles
    let mut result = Vec::new();
    for md_line in md_text.lines {
        let mut spans: Vec<Span<'static>> = Vec::new();
        for md_span in md_line.spans {
            let style = Style::default();
            // Map ratatui_core style to ratatui style
            let mut s = style;
            if let Some(fg) = md_span.style.fg {
                s = s.fg(convert_color(fg));
            }
            if let Some(bg) = md_span.style.bg {
                s = s.bg(convert_color(bg));
            }
            if md_span.style.add_modifier.contains(ratatui_core::style::Modifier::BOLD) {
                s = s.add_modifier(ratatui::style::Modifier::BOLD);
            }
            if md_span.style.add_modifier.contains(ratatui_core::style::Modifier::ITALIC) {
                s = s.add_modifier(ratatui::style::Modifier::ITALIC);
            }
            if md_span.style.add_modifier.contains(ratatui_core::style::Modifier::UNDERLINED) {
                s = s.add_modifier(ratatui::style::Modifier::UNDERLINED);
            }
            spans.push(Span::styled(md_span.content.to_string(), s));
        }
        result.push(Line::from(spans));
    }
    if result.is_empty() {
        // Fallback to plain wrap
        wrap_text(content, width).into_iter()
            .map(|s| Line::from(Span::raw(s)))
            .collect()
    } else {
        result
    }
}

fn convert_color(c: ratatui_core::style::Color) -> Color {
    match c {
        ratatui_core::style::Color::Reset => Color::Reset,
        ratatui_core::style::Color::Black => Color::Black,
        ratatui_core::style::Color::Red => Color::Red,
        ratatui_core::style::Color::Green => Color::Green,
        ratatui_core::style::Color::Yellow => Color::Yellow,
        ratatui_core::style::Color::Blue => Color::Blue,
        ratatui_core::style::Color::Magenta => Color::Magenta,
        ratatui_core::style::Color::Cyan => Color::Cyan,
        ratatui_core::style::Color::Gray => Color::Gray,
        ratatui_core::style::Color::White => Color::White,
        ratatui_core::style::Color::Indexed(i) => Color::Indexed(i),
        ratatui_core::style::Color::Rgb(r, g, b) => Color::Rgb(r, g, b),
        _ => Color::Reset,
    }
}

/// Set of message indices whose reasoning blocks are expanded
pub type ExpandedReasoning = std::collections::HashSet<usize>;
/// Set of message indices whose tool details are expanded
pub type ExpandedTools = std::collections::HashSet<usize>;

/// Convert a message into ratatui Lines (all owned/static)
pub fn message_to_lines(
    msg: &AgentMessage,
    msg_index: usize,
    mode: TranscriptMode,
    theme: &ThemeTokens,
    width: usize,
    expanded: &ExpandedReasoning,
    expanded_tools: &ExpandedTools,
) -> Vec<Line<'static>> {
    let mut lines = Vec::new();

    match mode {
        TranscriptMode::Compact => render_compact(msg, msg_index, theme, width, expanded, expanded_tools, &mut lines),
        TranscriptMode::Tools => render_tools_only(msg, theme, width, &mut lines),
        TranscriptMode::Full => render_full(msg, msg_index, theme, width, expanded, expanded_tools, &mut lines),
    }

    lines
}

fn render_compact(
    msg: &AgentMessage,
    msg_index: usize,
    theme: &ThemeTokens,
    width: usize,
    expanded: &ExpandedReasoning,
    expanded_tools: &ExpandedTools,
    lines: &mut Vec<Line<'static>>,
) {
    let indent = 7;
    let content_width = width.saturating_sub(indent + 1);

    // TOOL messages: compact one-liner or expanded with args + result
    if msg.role == MessageRole::Tool {
        if let Some(name) = &msg.tool_name {
            let status = msg.tool_status.as_deref().unwrap_or("done");
            let (status_text, status_style) = format_tool_status(status, theme);
            lines.push(Line::from(vec![
                Span::raw("  "),
                Span::styled("\u{2699}", theme.accent_assistant),
                Span::raw(" "),
                Span::styled(name.clone(), theme.fg_dim),
                Span::raw(" "),
                Span::styled(status_text, status_style),
            ]));

            // Expanded tool details
            if expanded_tools.contains(&msg_index) {
                let detail_indent = 4;
                let detail_width = width.saturating_sub(detail_indent + 1);

                // Show arguments
                if let Some(args) = &msg.tool_arguments {
                    if !args.is_empty() {
                        let args_preview = if args.len() > detail_width.saturating_sub(6) {
                            format!("{}...", &args[..detail_width.saturating_sub(9).min(args.len())])
                        } else {
                            args.clone()
                        };
                        lines.push(Line::from(vec![
                            Span::raw(" ".repeat(detail_indent)),
                            Span::styled("args: ", theme.fg_dim),
                            Span::styled(args_preview, theme.fg_active),
                        ]));
                    }
                }

                // Show result (truncated to 5 lines)
                let result_text = &msg.content;
                if !result_text.is_empty() {
                    let result_lines: Vec<&str> = result_text.lines().collect();
                    let show_lines = result_lines.len().min(5);
                    let has_more = result_lines.len() > 5;

                    for (i, rline) in result_lines[..show_lines].iter().enumerate() {
                        let prefix = if i == 0 { "result: " } else { "        " };
                        let truncated = if rline.len() > detail_width.saturating_sub(prefix.len()) {
                            format!("{}...", &rline[..detail_width.saturating_sub(prefix.len() + 3).min(rline.len())])
                        } else {
                            rline.to_string()
                        };
                        lines.push(Line::from(vec![
                            Span::raw(" ".repeat(detail_indent)),
                            Span::styled(prefix.to_string(), theme.fg_dim),
                            Span::styled(truncated, theme.fg_active),
                        ]));
                    }
                    if has_more {
                        lines.push(Line::from(vec![
                            Span::raw(" ".repeat(detail_indent)),
                            Span::styled("        ...", theme.fg_dim),
                        ]));
                    }
                }
            }
        }
        return;
    }

    let content = &msg.content;
    // Skip truly empty non-assistant messages (no content, no reasoning)
    if content.is_empty() && msg.role != MessageRole::Assistant {
        return;
    }
    if content.is_empty() && msg.reasoning.is_none() {
        return;
    }

    let (badge, badge_style) = role_badge(msg.role);

    // Render content — use markdown for assistant, plain wrap for others
    let md_lines: Vec<Line<'static>> = if msg.role == MessageRole::Assistant {
        render_markdown(content, content_width)
    } else {
        wrap_text(content, content_width).into_iter()
            .map(|s| Line::from(Span::styled(s, theme.fg_active)))
            .collect()
    };

    // Badge + first line of content
    let first_line = md_lines.first().cloned().unwrap_or_default();
    let mut badge_line_spans = vec![
        Span::raw("  "),
        Span::styled(
            badge,
            Style::default()
                .bg(badge_style.fg.unwrap_or(Color::Indexed(245)))
                .fg(Color::Black),
        ),
        Span::raw(" "),
    ];
    badge_line_spans.extend(first_line.spans);
    lines.push(Line::from(badge_line_spans));

    // Continuation content lines
    for line in md_lines.iter().skip(1) {
        let mut spans = vec![Span::raw(" ".repeat(indent))];
        spans.extend(line.spans.iter().cloned());
        lines.push(Line::from(spans));
    }

    // Reasoning block AFTER all content (collapsible)
    if msg.role == MessageRole::Assistant {
        if let Some(reasoning) = &msg.reasoning {
            if !reasoning.is_empty() {
                let is_expanded = expanded.contains(&msg_index);
                if is_expanded {
                    lines.push(Line::from(vec![
                        Span::raw(" ".repeat(indent)),
                        Span::styled("\u{25be} [-] Reasoning", theme.fg_dim),
                    ]));
                    let reasoning_width = width.saturating_sub(indent + 2);
                    let dark_blue = Style::default().fg(Color::Indexed(24));
                    for rline in wrap_text(reasoning, reasoning_width) {
                        lines.push(Line::from(vec![
                            Span::raw(" ".repeat(indent)),
                            Span::styled("\u{2502}", dark_blue),
                            Span::raw(" "),
                            Span::styled(rline, theme.fg_dim),
                        ]));
                    }
                } else {
                    lines.push(Line::from(vec![
                        Span::raw(" ".repeat(indent)),
                        Span::styled("\u{25b6} [+] Reasoning", theme.fg_dim),
                    ]));
                }
            }
        }
    }
}

fn render_tools_only(
    msg: &AgentMessage,
    theme: &ThemeTokens,
    width: usize,
    lines: &mut Vec<Line<'static>>,
) {
    if msg.role != MessageRole::Tool && msg.tool_name.is_none() {
        return;
    }

    if let Some(name) = &msg.tool_name {
        let status = msg.tool_status.as_deref().unwrap_or("done");
        let (status_text, status_style) = format_tool_status(status, theme);
        let args_preview = msg.tool_arguments.as_deref().unwrap_or("");
        let max_args = width.saturating_sub(30);
        let args_short = if args_preview.len() > max_args {
            &args_preview[..max_args]
        } else {
            args_preview
        };

        let mut spans = vec![
            Span::raw("  "),
            Span::styled("\u{2699}", theme.accent_assistant),
            Span::raw(" "),
            Span::styled(name.clone(), theme.fg_active),
            Span::raw(" "),
            Span::styled(status_text, status_style),
        ];

        if !args_short.is_empty() {
            spans.push(Span::raw("  "));
            spans.push(Span::styled(args_short.to_string(), theme.fg_dim));
        }

        lines.push(Line::from(spans));
    }
}

fn render_full(
    msg: &AgentMessage,
    msg_index: usize,
    theme: &ThemeTokens,
    width: usize,
    expanded: &ExpandedReasoning,
    expanded_tools: &ExpandedTools,
    lines: &mut Vec<Line<'static>>,
) {
    // Full mode: always expand reasoning and tools
    let mut full_expanded = expanded.clone();
    full_expanded.insert(msg_index);
    let mut full_tools = expanded_tools.clone();
    full_tools.insert(msg_index);
    render_compact(msg, msg_index, theme, width, &full_expanded, &full_tools, lines);
}

fn role_badge(role: MessageRole) -> (&'static str, Style) {
    match role {
        MessageRole::User => ("USER", Style::default().fg(Color::Indexed(75))),
        MessageRole::Assistant => ("ASST", Style::default().fg(Color::Indexed(183))),
        MessageRole::System => ("SYS ", Style::default().fg(Color::Indexed(245))),
        MessageRole::Tool => ("TOOL", Style::default().fg(Color::Indexed(183))),
        MessageRole::Unknown => ("??? ", Style::default().fg(Color::Indexed(245))),
    }
}

fn format_tool_status(status: &str, theme: &ThemeTokens) -> (&'static str, Style) {
    match status {
        "completed" | "done" | "success" => ("\u{2713} done", theme.accent_success),
        "error" | "failed" => ("\u{2717} error", theme.accent_danger),
        _ => ("\u{25cf} running", theme.accent_secondary),
    }
}

/// Word-wrap text to fit within a given width
pub(crate) fn wrap_text(text: &str, width: usize) -> Vec<String> {
    if width == 0 {
        return vec![text.to_string()];
    }
    let mut lines = Vec::new();
    for paragraph in text.split('\n') {
        if paragraph.is_empty() {
            lines.push(String::new());
            continue;
        }
        let mut current_line = String::new();
        for word in paragraph.split_whitespace() {
            if current_line.is_empty() {
                current_line = word.to_string();
            } else if current_line.len() + 1 + word.len() <= width {
                current_line.push(' ');
                current_line.push_str(word);
            } else {
                lines.push(current_line);
                current_line = word.to_string();
            }
        }
        if !current_line.is_empty() {
            lines.push(current_line);
        }
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_expanded() -> ExpandedReasoning {
        ExpandedReasoning::new()
    }

    fn empty_tools() -> ExpandedTools {
        ExpandedTools::new()
    }

    #[test]
    fn markdown_renders_bold() {
        let lines = render_markdown("**bold text** normal", 80);
        assert!(!lines.is_empty(), "Markdown should produce lines");
        let has_bold = lines.iter().any(|line| {
            line.spans.iter().any(|span| {
                span.style.add_modifier.contains(ratatui::style::Modifier::BOLD)
            })
        });
        let debug: Vec<Vec<String>> = lines.iter()
            .map(|l| l.spans.iter().map(|s| format!("'{}' mods={:?}", s.content, s.style.add_modifier)).collect())
            .collect();
        assert!(has_bold, "Expected BOLD in markdown output: {:?}", debug);
    }

    #[test]
    fn wrap_text_basic() {
        let lines = wrap_text("hello world foo bar", 12);
        assert_eq!(lines, vec!["hello world", "foo bar"]);
    }

    #[test]
    fn wrap_text_preserves_newlines() {
        let lines = wrap_text("line1\nline2", 80);
        assert_eq!(lines, vec!["line1", "line2"]);
    }

    #[test]
    fn user_message_has_badge() {
        let msg = AgentMessage {
            role: MessageRole::User,
            content: "Hello".into(),
            ..Default::default()
        };
        let lines = message_to_lines(&msg, 0, TranscriptMode::Compact, &ThemeTokens::default(), 80, &empty_expanded(), &empty_tools());
        assert!(!lines.is_empty());
    }

    #[test]
    fn tool_message_shows_gear_icon() {
        let msg = AgentMessage {
            role: MessageRole::Tool,
            tool_name: Some("bash_command".into()),
            tool_status: Some("done".into()),
            content: "some output here".into(),
            ..Default::default()
        };
        let lines = message_to_lines(&msg, 0, TranscriptMode::Compact, &ThemeTokens::default(), 80, &empty_expanded(), &empty_tools());
        assert_eq!(lines.len(), 1); // single compact line
    }

    #[test]
    fn tool_message_expanded_shows_details() {
        let msg = AgentMessage {
            role: MessageRole::Tool,
            tool_name: Some("bash_command".into()),
            tool_status: Some("done".into()),
            tool_arguments: Some("ls -la /home/user".into()),
            content: "total 208\ndrwxr-xr-x 15 user user 4096 Jan 1 00:00 .".into(),
            ..Default::default()
        };
        let mut exp_tools = empty_tools();
        exp_tools.insert(0);
        let lines = message_to_lines(&msg, 0, TranscriptMode::Compact, &ThemeTokens::default(), 80, &empty_expanded(), &exp_tools);
        assert!(lines.len() > 1, "Expanded tool should have more than 1 line, got {}", lines.len());
    }

    #[test]
    fn tool_message_with_content_renders_compact() {
        let msg = AgentMessage {
            role: MessageRole::Tool,
            tool_name: Some("list_workspaces".into()),
            tool_status: Some("done".into()),
            content: "Workspace Default:\n  Surface: Infinite Canvas".into(),
            ..Default::default()
        };
        let lines = message_to_lines(&msg, 0, TranscriptMode::Compact, &ThemeTokens::default(), 80, &empty_expanded(), &empty_tools());
        // Should be 1 compact line, not the full content
        assert_eq!(lines.len(), 1);
    }

    #[test]
    fn reasoning_before_content() {
        let msg = AgentMessage {
            role: MessageRole::Assistant,
            content: "Here is my answer".into(),
            reasoning: Some("Let me think...".into()),
            ..Default::default()
        };
        let lines = message_to_lines(&msg, 0, TranscriptMode::Compact, &ThemeTokens::default(), 80, &empty_expanded(), &empty_tools());
        // First line = ASST badge, second line = reasoning hint
        assert!(lines.len() >= 2);
        let first_text: String = lines[0].spans.iter().map(|s| s.content.to_string()).collect();
        assert!(first_text.contains("ASST"), "First line should have ASST badge, got: {}", first_text);
        let second_text: String = lines[1].spans.iter().map(|s| s.content.to_string()).collect();
        assert!(second_text.contains("Reasoning"), "Second line should be reasoning hint, got: {}", second_text);
    }

    #[test]
    fn reasoning_expandable() {
        let msg = AgentMessage {
            role: MessageRole::Assistant,
            content: "Answer".into(),
            reasoning: Some("Thinking step by step".into()),
            ..Default::default()
        };
        let collapsed = message_to_lines(&msg, 0, TranscriptMode::Compact, &ThemeTokens::default(), 80, &empty_expanded(), &empty_tools());
        let mut exp = empty_expanded();
        exp.insert(0);
        let expanded = message_to_lines(&msg, 0, TranscriptMode::Compact, &ThemeTokens::default(), 80, &exp, &empty_tools());
        assert!(expanded.len() > collapsed.len(), "Expanded should have more lines");
    }

    #[test]
    fn tools_mode_skips_non_tool_messages() {
        let msg = AgentMessage {
            role: MessageRole::User,
            content: "Hello".into(),
            ..Default::default()
        };
        let lines = message_to_lines(&msg, 0, TranscriptMode::Tools, &ThemeTokens::default(), 80, &empty_expanded(), &empty_tools());
        assert!(lines.is_empty());
    }

    #[test]
    fn wrap_text_empty_string() {
        let lines = wrap_text("", 80);
        assert_eq!(lines, vec![""]);
    }

    #[test]
    fn wrap_text_zero_width() {
        let lines = wrap_text("hello", 0);
        assert_eq!(lines, vec!["hello"]);
    }
}
