use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::state::chat::{AgentMessage, MessageRole, TranscriptMode};
use crate::theme::ThemeTokens;

/// Render markdown content into Lines using tui-markdown.
/// Converts from ratatui_core types to ratatui types.
pub(crate) fn render_markdown_pub(content: &str, width: usize) -> Vec<Line<'static>> {
    render_markdown(content, width)
}

fn render_markdown(content: &str, width: usize) -> Vec<Line<'static>> {
    if content.is_empty() {
        return vec![];
    }

    let raw_lines: Vec<&str> = content.lines().collect();
    let mut result = Vec::new();
    let mut markdown_buffer = String::new();
    let mut idx = 0usize;

    while idx < raw_lines.len() {
        if is_markdown_table_start(&raw_lines, idx) {
            if !markdown_buffer.is_empty() {
                result.extend(render_markdown_segment(&markdown_buffer, width));
                markdown_buffer.clear();
            }
            let start = idx;
            idx += 2;
            while idx < raw_lines.len() && is_markdown_table_row(raw_lines[idx]) {
                idx += 1;
            }
            result.extend(render_markdown_table(&raw_lines[start..idx], width));
            continue;
        }

        markdown_buffer.push_str(raw_lines[idx]);
        if idx + 1 < raw_lines.len() {
            markdown_buffer.push('\n');
        }
        idx += 1;
    }

    if !markdown_buffer.is_empty() {
        result.extend(render_markdown_segment(&markdown_buffer, width));
    }

    result
}

fn render_markdown_segment(content: &str, width: usize) -> Vec<Line<'static>> {
    let md_text = tui_markdown::from_str(content);
    // Convert ratatui_core::Line to ratatui::Line via plain text + styles
    let mut result = Vec::new();
    for md_line in md_text.lines {
        let mut spans: Vec<Span<'static>> = Vec::new();
        for md_span in md_line.spans {
            let s = convert_style(md_span.style);
            spans.push(Span::styled(md_span.content.to_string(), s));
        }
        result.push(Line::from(spans).style(convert_style(md_line.style)));
    }
    if result.is_empty() {
        // Fallback to plain wrap
        wrap_text(content, width)
            .into_iter()
            .map(|s| Line::from(Span::raw(s)))
            .collect()
    } else {
        wrap_styled_lines(result, width)
    }
}

fn is_markdown_table_row(line: &str) -> bool {
    line.contains('|')
}

fn is_markdown_table_separator(line: &str) -> bool {
    let trimmed = line.trim();
    !trimmed.is_empty()
        && trimmed.contains('|')
        && trimmed
            .chars()
            .all(|ch| matches!(ch, '|' | '-' | ':' | ' '))
}

fn is_markdown_table_start(lines: &[&str], idx: usize) -> bool {
    idx + 1 < lines.len()
        && is_markdown_table_row(lines[idx])
        && is_markdown_table_separator(lines[idx + 1])
}

fn parse_markdown_table_row(line: &str) -> Vec<String> {
    line.trim()
        .trim_matches('|')
        .split('|')
        .map(|cell| cell.trim().to_string())
        .collect()
}

fn pad_cell(text: &str, width: usize) -> String {
    let current = UnicodeWidthStr::width(text);
    if current >= width {
        return text.to_string();
    }
    format!("{text}{}", " ".repeat(width - current))
}

fn wrap_table_cell(text: &str, width: usize) -> Vec<String> {
    wrap_styled_line(Line::from(text.to_string()), width.max(1))
        .into_iter()
        .map(|line| line.spans.into_iter().map(|span| span.content).collect())
        .collect()
}

fn fit_table_widths(rows: &[Vec<String>], width: usize) -> Vec<usize> {
    let cols = rows.iter().map(Vec::len).max().unwrap_or(0);
    if cols == 0 {
        return Vec::new();
    }

    let gutter_width = cols.saturating_sub(1) * 3;
    let available = width.saturating_sub(gutter_width).max(cols);
    let mut widths = vec![1usize; cols];

    for row in rows {
        for (idx, cell) in row.iter().enumerate() {
            widths[idx] = widths[idx].max(UnicodeWidthStr::width(cell.as_str()));
        }
    }

    let total: usize = widths.iter().sum();
    if total <= available {
        return widths;
    }

    let mut assigned = vec![3usize; cols];
    let mut remaining = available.saturating_sub(assigned.iter().sum::<usize>());
    let mut growable: Vec<(usize, usize)> = widths
        .iter()
        .enumerate()
        .map(|(idx, natural)| (idx, natural.saturating_sub(3)))
        .collect();

    while remaining > 0 {
        let mut progressed = false;
        for (idx, extra) in &mut growable {
            if *extra > 0 && remaining > 0 {
                assigned[*idx] += 1;
                *extra -= 1;
                remaining -= 1;
                progressed = true;
            }
        }
        if !progressed {
            break;
        }
    }

    assigned
}

fn render_markdown_table(lines: &[&str], width: usize) -> Vec<Line<'static>> {
    if lines.is_empty() {
        return vec![];
    }

    let mut rows = Vec::new();
    for (idx, line) in lines.iter().enumerate() {
        if idx == 1 && is_markdown_table_separator(line) {
            continue;
        }
        rows.push(parse_markdown_table_row(line));
    }

    let col_widths = fit_table_widths(&rows, width.max(1));
    if col_widths.is_empty() {
        return vec![];
    }

    let header_style = Style::default().add_modifier(Modifier::BOLD);
    let separator = col_widths
        .iter()
        .map(|col| "─".repeat(*col))
        .collect::<Vec<_>>()
        .join("─┼─");

    let mut rendered = Vec::new();
    for (row_idx, row) in rows.iter().enumerate() {
        let wrapped_cells = col_widths
            .iter()
            .enumerate()
            .map(|(col_idx, col_width)| {
                let cell = row.get(col_idx).map(String::as_str).unwrap_or("");
                wrap_table_cell(cell, *col_width)
            })
            .collect::<Vec<_>>();
        let row_height = wrapped_cells.iter().map(Vec::len).max().unwrap_or(1);

        for visual_row in 0..row_height {
            let mut spans = Vec::new();
            for (col_idx, col_width) in col_widths.iter().enumerate() {
                if col_idx > 0 {
                    spans.push(Span::styled(" │ ", Style::default().fg(Color::DarkGray)));
                }
                let cell_line = wrapped_cells
                    .get(col_idx)
                    .and_then(|lines| lines.get(visual_row))
                    .map(String::as_str)
                    .unwrap_or("");
                spans.push(Span::styled(
                    pad_cell(cell_line, *col_width),
                    if row_idx == 0 {
                        header_style
                    } else {
                        Style::default()
                    },
                ));
            }
            rendered.push(Line::from(spans));
        }
        if row_idx == 0 {
            rendered.push(Line::from(Span::styled(
                separator.clone(),
                Style::default().fg(Color::DarkGray),
            )));
        }
    }
    rendered
}

fn convert_style(style: ratatui_core::style::Style) -> Style {
    let mut result = Style::default();

    if let Some(fg) = style.fg {
        result = result.fg(convert_color(fg));
    }
    if let Some(bg) = style.bg {
        result = result.bg(convert_color(bg));
    }

    let add = convert_modifier(style.add_modifier);
    if !add.is_empty() {
        result = result.add_modifier(add);
    }

    let remove = convert_modifier(style.sub_modifier);
    if !remove.is_empty() {
        result = result.remove_modifier(remove);
    }

    result
}

fn convert_modifier(modifier: ratatui_core::style::Modifier) -> Modifier {
    let mut result = Modifier::empty();

    if modifier.contains(ratatui_core::style::Modifier::BOLD) {
        result |= Modifier::BOLD;
    }
    if modifier.contains(ratatui_core::style::Modifier::DIM) {
        result |= Modifier::DIM;
    }
    if modifier.contains(ratatui_core::style::Modifier::ITALIC) {
        result |= Modifier::ITALIC;
    }
    if modifier.contains(ratatui_core::style::Modifier::UNDERLINED) {
        result |= Modifier::UNDERLINED;
    }
    if modifier.contains(ratatui_core::style::Modifier::SLOW_BLINK) {
        result |= Modifier::SLOW_BLINK;
    }
    if modifier.contains(ratatui_core::style::Modifier::RAPID_BLINK) {
        result |= Modifier::RAPID_BLINK;
    }
    if modifier.contains(ratatui_core::style::Modifier::REVERSED) {
        result |= Modifier::REVERSED;
    }
    if modifier.contains(ratatui_core::style::Modifier::HIDDEN) {
        result |= Modifier::HIDDEN;
    }
    if modifier.contains(ratatui_core::style::Modifier::CROSSED_OUT) {
        result |= Modifier::CROSSED_OUT;
    }

    result
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
        TranscriptMode::Compact => render_compact(
            msg,
            msg_index,
            theme,
            width,
            expanded,
            expanded_tools,
            &mut lines,
        ),
        TranscriptMode::Tools => render_tools_only(msg, theme, width, &mut lines),
        TranscriptMode::Full => render_full(
            msg,
            msg_index,
            theme,
            width,
            expanded,
            expanded_tools,
            &mut lines,
        ),
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
    let content_width = width.max(1);

    // TOOL messages: compact one-liner or expanded with args + result
    if msg.role == MessageRole::Tool {
        if let Some(name) = &msg.tool_name {
            let status = msg.tool_status.as_deref().unwrap_or("done");
            let (status_text, status_style) = format_tool_status(status, theme);
            lines.push(Line::from(vec![
                Span::styled("\u{2699}", theme.accent_assistant),
                Span::raw("  "),
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
                        let mut rendered_arg_lines =
                            wrap_text(args, detail_width.max(1)).into_iter();
                        if let Some(first_line) = rendered_arg_lines.next() {
                            lines.push(Line::from(vec![
                                Span::styled("args: ", theme.fg_dim),
                                Span::styled(first_line, theme.fg_active),
                            ]));
                            for line in rendered_arg_lines {
                                lines.push(Line::from(vec![
                                    Span::styled("      ", theme.fg_dim),
                                    Span::styled(line, theme.fg_active),
                                ]));
                            }
                        }
                    }
                }

                // Show full result
                let result_text = &msg.content;
                if !result_text.is_empty() {
                    let mut result_line_index = 0usize;
                    for result_line in result_text.lines() {
                        let wrapped_result = wrap_text(result_line, detail_width.max(1));
                        for wrapped_line in wrapped_result {
                            let prefix = if result_line_index == 0 {
                                "result: "
                            } else {
                                "        "
                            };
                            lines.push(Line::from(vec![
                                Span::styled(prefix.to_string(), theme.fg_dim),
                                Span::styled(wrapped_line, theme.fg_active),
                            ]));
                            result_line_index += 1;
                        }
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

    let md_lines: Vec<Line<'static>> = if msg.role == MessageRole::Assistant {
        render_markdown(content, content_width)
    } else if msg.role == MessageRole::User {
        wrap_text(content, content_width)
            .into_iter()
            .map(|s| Line::from(Span::styled(s, theme.fg_active)))
            .collect()
    } else if msg.role == MessageRole::System {
        wrap_text(content, content_width)
            .into_iter()
            .map(|s| Line::from(Span::styled(s, theme.fg_dim)))
            .collect()
    } else {
        wrap_text(content, content_width)
            .into_iter()
            .map(|s| Line::from(Span::styled(s, theme.fg_active)))
            .collect()
    };
    let has_reasoning = msg.role == MessageRole::Assistant
        && msg
            .reasoning
            .as_deref()
            .is_some_and(|reasoning| !reasoning.is_empty());

    if has_reasoning {
        let reasoning = msg.reasoning.as_deref().unwrap_or_default();
        let is_expanded = expanded.contains(&msg_index);
        if is_expanded {
            lines.push(Line::from(vec![Span::styled(
                "\u{25be} [-] Reasoning",
                theme.fg_dim,
            )]));
            let reasoning_width = width.saturating_sub(2).max(1);
            let dark_blue = Style::default().fg(Color::Indexed(24));
            for rline in wrap_text(reasoning, reasoning_width) {
                lines.push(Line::from(vec![
                    Span::styled("\u{2502}", dark_blue),
                    Span::raw(" "),
                    Span::styled(rline, theme.fg_dim),
                ]));
            }
        } else {
            lines.push(Line::from(vec![Span::styled(
                "\u{25b6} [+] Reasoning",
                theme.fg_dim,
            )]));
        }

        lines.extend(md_lines);
    } else {
        lines.extend(md_lines);
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
            Span::styled("\u{2699}", theme.accent_assistant),
            Span::raw("  "),
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
    render_compact(
        msg,
        msg_index,
        theme,
        width,
        &full_expanded,
        &full_tools,
        lines,
    );
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
            } else if UnicodeWidthStr::width(current_line.as_str())
                + 1
                + UnicodeWidthStr::width(word)
                <= width
            {
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

fn wrap_styled_lines(lines: Vec<Line<'static>>, width: usize) -> Vec<Line<'static>> {
    lines
        .into_iter()
        .flat_map(|line| wrap_styled_line(line, width))
        .collect()
}

fn wrap_styled_line(line: Line<'static>, width: usize) -> Vec<Line<'static>> {
    if width == 0 || line.spans.is_empty() {
        return vec![line];
    }

    let line_style = line.style;
    let tokens = line
        .spans
        .into_iter()
        .flat_map(|span| tokenize_styled_text(span.content.to_string(), span.style))
        .collect::<Vec<_>>();

    if tokens.is_empty() {
        return vec![Line::default().style(line_style)];
    }

    let mut wrapped = Vec::new();
    let mut current_spans: Vec<Span<'static>> = Vec::new();
    let mut current_width = 0usize;

    for (token_text, token_style) in tokens {
        let token_width = UnicodeWidthStr::width(token_text.as_str());

        if token_width == 0 {
            current_spans.push(Span::styled(token_text, token_style));
            continue;
        }

        if current_width > 0 && current_width + token_width > width {
            wrapped.push(Line::from(std::mem::take(&mut current_spans)).style(line_style));
            current_width = 0;
            if token_text.chars().all(char::is_whitespace) {
                continue;
            }
        }

        if token_width <= width {
            current_width += token_width;
            current_spans.push(Span::styled(token_text, token_style));
            continue;
        }

        for chunk in split_text_by_width(&token_text, width) {
            let chunk_width = UnicodeWidthStr::width(chunk.as_str());
            if current_width > 0 {
                wrapped.push(Line::from(std::mem::take(&mut current_spans)).style(line_style));
            }

            current_spans.push(Span::styled(chunk, token_style));
            current_width = chunk_width;
            if current_width >= width {
                wrapped.push(Line::from(std::mem::take(&mut current_spans)).style(line_style));
                current_width = 0;
            }
        }
    }

    if !current_spans.is_empty() {
        wrapped.push(Line::from(current_spans).style(line_style));
    }

    if wrapped.is_empty() {
        wrapped.push(Line::default().style(line_style));
    }

    wrapped
}

fn tokenize_styled_text(text: String, style: Style) -> Vec<(String, Style)> {
    if text.is_empty() {
        return Vec::new();
    }

    let mut tokens = Vec::new();
    let mut start = 0usize;
    let mut chars = text.char_indices();
    let Some((_, first)) = chars.next() else {
        return tokens;
    };
    let mut in_whitespace = first.is_whitespace();

    for (idx, ch) in chars {
        if ch.is_whitespace() != in_whitespace {
            tokens.push((text[start..idx].to_string(), style));
            start = idx;
            in_whitespace = ch.is_whitespace();
        }
    }

    tokens.push((text[start..].to_string(), style));
    tokens
}

fn split_text_by_width(text: &str, width: usize) -> Vec<String> {
    if width == 0 {
        return vec![text.to_string()];
    }

    let mut chunks = Vec::new();
    let mut current = String::new();
    let mut current_width = 0usize;

    for ch in text.chars() {
        let ch_width = UnicodeWidthChar::width(ch).unwrap_or(0);
        if current_width > 0 && current_width + ch_width > width {
            chunks.push(std::mem::take(&mut current));
            current_width = 0;
        }
        current.push(ch);
        current_width += ch_width;
    }

    if !current.is_empty() {
        chunks.push(current);
    }

    if chunks.is_empty() {
        chunks.push(String::new());
    }

    chunks
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
                span.style
                    .add_modifier
                    .contains(ratatui::style::Modifier::BOLD)
            })
        });
        let debug: Vec<Vec<String>> = lines
            .iter()
            .map(|l| {
                l.spans
                    .iter()
                    .map(|s| format!("'{}' mods={:?}", s.content, s.style.add_modifier))
                    .collect()
            })
            .collect();
        assert!(has_bold, "Expected BOLD in markdown output: {:?}", debug);
    }

    #[test]
    fn markdown_heading_keeps_line_style() {
        let lines = render_markdown("## Heading", 80);
        assert!(!lines.is_empty());
        assert!(
            lines[0].style.add_modifier.contains(Modifier::BOLD),
            "Expected heading line style to keep bold modifier, got {:?}",
            lines[0].style
        );
    }

    #[test]
    fn markdown_wraps_to_requested_width() {
        let lines = render_markdown("**alpha beta gamma delta**", 10);
        assert!(
            lines.len() > 1,
            "Expected markdown to wrap, got {:?}",
            lines
        );
    }

    #[test]
    fn markdown_tables_render_as_columns() {
        let lines = render_markdown(
            "| Skill | Size | Purpose |\n|---|---|---|\n| tamux-rust-dev.md | 3.4KB | Build and test Rust crates |",
            80,
        );
        let plain = lines
            .iter()
            .map(|line| {
                line.spans
                    .iter()
                    .map(|span| span.content.as_ref())
                    .collect::<String>()
            })
            .collect::<Vec<_>>();
        assert!(
            plain.iter().any(|line| line.contains("│")),
            "Expected rendered column separators, got {:?}",
            plain
        );
        assert!(
            plain.iter().all(|line| !line.contains("|---")),
            "Expected markdown separator row to be rendered, got {:?}",
            plain
        );
    }

    #[test]
    fn markdown_tables_wrap_long_cells_instead_of_truncating() {
        let lines = render_markdown(
            "| Spec | Idea | Why |\n|---|---|---|\n| NEGATIVE_KNOWLEDGE | The agent should track negative knowledge explicitly instead of compressing it into binary success and failure states | This preserves the actual content for the operator |",
            40,
        );
        let plain = lines
            .iter()
            .map(|line| {
                line.spans
                    .iter()
                    .map(|span| span.content.as_ref())
                    .collect::<String>()
            })
            .collect::<Vec<_>>();

        assert!(
            plain.len() > 3,
            "Expected wrapped multi-line table rows, got {:?}",
            plain
        );
        assert!(
            plain.iter().all(|line| !line.contains('…')),
            "Expected wrapped cells without truncation, got {:?}",
            plain
        );
        let joined = plain.join("\n");
        assert!(
            joined.contains("The agent")
                && joined.contains("negative")
                && joined.contains("states"),
            "Expected wrapped table output to preserve the long cell content, got {:?}",
            plain
        );
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
        let lines = message_to_lines(
            &msg,
            0,
            TranscriptMode::Compact,
            &ThemeTokens::default(),
            80,
            &empty_expanded(),
            &empty_tools(),
        );
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
        let lines = message_to_lines(
            &msg,
            0,
            TranscriptMode::Compact,
            &ThemeTokens::default(),
            80,
            &empty_expanded(),
            &empty_tools(),
        );
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
        let lines = message_to_lines(
            &msg,
            0,
            TranscriptMode::Compact,
            &ThemeTokens::default(),
            80,
            &empty_expanded(),
            &exp_tools,
        );
        assert!(
            lines.len() > 1,
            "Expanded tool should have more than 1 line, got {}",
            lines.len()
        );
    }

    #[test]
    fn tool_message_expanded_preserves_full_arguments_and_result() {
        let long_args = serde_json::json!({
            "command": "python - <<'PY'\n".to_string() + &"x".repeat(120) + "\nPY",
        })
        .to_string();
        let long_result = (0..8)
            .map(|index| format!("line-{index}: {}", "y".repeat(40)))
            .collect::<Vec<_>>()
            .join("\n");
        let msg = AgentMessage {
            role: MessageRole::Tool,
            tool_name: Some("bash_command".into()),
            tool_status: Some("done".into()),
            tool_arguments: Some(long_args.clone()),
            content: long_result.clone(),
            ..Default::default()
        };

        let mut exp_tools = empty_tools();
        exp_tools.insert(0);
        let lines = message_to_lines(
            &msg,
            0,
            TranscriptMode::Compact,
            &ThemeTokens::default(),
            50,
            &empty_expanded(),
            &exp_tools,
        );

        let plain = lines
            .iter()
            .map(|line| {
                line.spans
                    .iter()
                    .map(|span| span.content.as_ref())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n");

        assert!(
            plain.contains("python -"),
            "missing argument prefix: {plain}"
        );
        assert!(plain.contains("<<'PY'"), "missing heredoc marker: {plain}");
        assert!(
            plain.contains(&"x".repeat(80)),
            "missing long argument body: {plain}"
        );
        assert!(
            plain.contains("line-7:"),
            "missing later result lines: {plain}"
        );
        assert!(
            plain.contains(&"y".repeat(30)),
            "missing long result body: {plain}"
        );
        assert!(
            !plain.contains("..."),
            "expanded tool output should not be truncated: {plain}"
        );
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
        let lines = message_to_lines(
            &msg,
            0,
            TranscriptMode::Compact,
            &ThemeTokens::default(),
            80,
            &empty_expanded(),
            &empty_tools(),
        );
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
        let lines = message_to_lines(
            &msg,
            0,
            TranscriptMode::Compact,
            &ThemeTokens::default(),
            80,
            &empty_expanded(),
            &empty_tools(),
        );
        assert!(!lines.is_empty());
        let first_text: String = lines[0]
            .spans
            .iter()
            .map(|s| s.content.to_string())
            .collect();
        assert!(
            first_text.contains("Reasoning"),
            "First line should be reasoning hint, got: {}",
            first_text
        );
    }

    #[test]
    fn reasoning_renders_before_multiline_content() {
        let msg = AgentMessage {
            role: MessageRole::Assistant,
            content: "First line that wraps a bit for the test".into(),
            reasoning: Some("Let me think...".into()),
            ..Default::default()
        };
        let lines = message_to_lines(
            &msg,
            0,
            TranscriptMode::Compact,
            &ThemeTokens::default(),
            20,
            &empty_expanded(),
            &empty_tools(),
        );
        let first_text: String = lines[0]
            .spans
            .iter()
            .map(|s| s.content.to_string())
            .collect();
        let second_text: String = lines[1]
            .spans
            .iter()
            .map(|s| s.content.to_string())
            .collect();
        assert!(
            first_text.contains("Reasoning"),
            "First line should be reasoning, got: {}",
            first_text
        );
        assert!(
            !second_text.contains("Reasoning"),
            "Content should start after reasoning, got: {}",
            second_text
        );
    }

    #[test]
    fn reasoning_expandable() {
        let msg = AgentMessage {
            role: MessageRole::Assistant,
            content: "Answer".into(),
            reasoning: Some("Thinking step by step".into()),
            ..Default::default()
        };
        let collapsed = message_to_lines(
            &msg,
            0,
            TranscriptMode::Compact,
            &ThemeTokens::default(),
            80,
            &empty_expanded(),
            &empty_tools(),
        );
        let mut exp = empty_expanded();
        exp.insert(0);
        let expanded = message_to_lines(
            &msg,
            0,
            TranscriptMode::Compact,
            &ThemeTokens::default(),
            80,
            &exp,
            &empty_tools(),
        );
        assert!(
            expanded.len() > collapsed.len(),
            "Expanded should have more lines"
        );
    }

    #[test]
    fn tools_mode_skips_non_tool_messages() {
        let msg = AgentMessage {
            role: MessageRole::User,
            content: "Hello".into(),
            ..Default::default()
        };
        let lines = message_to_lines(
            &msg,
            0,
            TranscriptMode::Tools,
            &ThemeTokens::default(),
            80,
            &empty_expanded(),
            &empty_tools(),
        );
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
