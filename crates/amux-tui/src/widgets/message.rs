use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

#[path = "message_markdown_table.rs"]
mod markdown_table;

use markdown_table::{is_markdown_table_row, is_markdown_table_start, render_markdown_table};

use crate::state::chat::{AgentMessage, MessageRole, TranscriptMode};
use crate::theme::ThemeTokens;
use crate::widgets::message_operator_question::render_operator_question_message;
use crate::widgets::tool_diff::{
    render_tool_edit_diff, render_tool_structured_json, ToolStructuredValueSource,
};

fn format_weles_review_badge(
    review: &crate::state::chat::WelesReviewMetaVm,
    theme: &ThemeTokens,
) -> (String, Style) {
    match review.verdict.as_str() {
        "block" => ("blocked".to_string(), theme.accent_danger),
        "flag_only" => ("flagged".to_string(), theme.accent_secondary),
        _ if review.weles_reviewed => ("reviewed".to_string(), theme.fg_dim),
        _ => ("unreviewed".to_string(), theme.fg_dim),
    }
}

fn render_weles_review_details(
    review: &crate::state::chat::WelesReviewMetaVm,
    theme: &ThemeTokens,
    width: usize,
    lines: &mut Vec<Line<'static>>,
) {
    let detail_width = width.max(1);
    let (badge, badge_style) = format_weles_review_badge(review, theme);
    let mut meta_spans = vec![
        Span::styled("weles: ".to_string(), theme.fg_dim),
        Span::styled(badge, badge_style),
    ];

    if let Some(mode) = review.security_override_mode.as_deref() {
        if !mode.is_empty() {
            meta_spans.push(Span::raw(" "));
            meta_spans.push(Span::styled(
                format!("override={mode}"),
                theme.accent_secondary,
            ));
        }
    }

    if !review.weles_reviewed {
        meta_spans.push(Span::raw(" "));
        meta_spans.push(Span::styled("degraded", theme.accent_secondary));
    }

    if let Some(audit_id) = review.audit_id.as_deref() {
        if !audit_id.is_empty() {
            meta_spans.push(Span::raw(" "));
            meta_spans.push(Span::styled(
                format!(
                    "#{}",
                    audit_id
                        .chars()
                        .rev()
                        .take(8)
                        .collect::<String>()
                        .chars()
                        .rev()
                        .collect::<String>()
                ),
                theme.fg_dim,
            ));
        }
    }
    lines.push(Line::from(meta_spans));

    for reason in &review.reasons {
        for line in wrap_text(reason, detail_width) {
            lines.push(Line::from(vec![
                Span::styled("reason: ".to_string(), theme.fg_dim),
                Span::styled(line, theme.fg_active),
            ]));
        }
    }
}

/// Render markdown content into Lines using tui-markdown.
/// Converts from ratatui_core types to ratatui types.
pub(crate) fn render_markdown_pub(content: &str, width: usize) -> Vec<Line<'static>> {
    render_markdown(content, width)
}

fn normalize_markdown_for_tui(content: &str) -> String {
    let mut normalized = String::with_capacity(content.len());
    let mut active_fence: Option<(char, usize)> = None;

    for segment in content.split_inclusive('\n') {
        let (line, newline) = match segment.strip_suffix('\n') {
            Some(line) => (line, "\n"),
            None => (segment, ""),
        };
        let trimmed = line.trim_start_matches([' ', '\t']);
        let leading_len = line.len().saturating_sub(trimmed.len());
        let leading = &line[..leading_len];

        if let Some((marker, marker_len)) = fence_marker(trimmed) {
            let rest = &trimmed[marker_len..];

            match active_fence {
                Some((active_marker, active_len))
                    if marker == active_marker
                        && marker_len >= active_len
                        && rest.trim().is_empty() =>
                {
                    active_fence = None;
                    normalized.push_str(line);
                    normalized.push_str(newline);
                    continue;
                }
                None => {
                    active_fence = Some((marker, marker_len));
                    if rest.trim().is_empty() {
                        normalized.push_str(leading);
                        normalized.extend(std::iter::repeat_n(marker, marker_len));
                        normalized.push_str("text");
                        normalized.push_str(newline);
                        continue;
                    }
                }
                _ => {}
            }
        }

        normalized.push_str(line);
        normalized.push_str(newline);
    }

    normalized
}

fn fence_marker(line: &str) -> Option<(char, usize)> {
    let mut chars = line.chars();
    let marker = chars.next()?;
    if marker != '`' && marker != '~' {
        return None;
    }

    let marker_len = line.chars().take_while(|ch| *ch == marker).count();
    if marker_len < 3 {
        return None;
    }

    Some((marker, marker_len))
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
    let normalized = normalize_markdown_for_tui(content);
    let md_text = tui_markdown::from_str(&normalized);
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

fn toggle_glyph(expanded: bool) -> &'static str {
    if expanded {
        "\u{25be}"
    } else {
        "\u{25b6}"
    }
}

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

    if msg.message_kind == "compaction_artifact" {
        let compaction_content = {
            let visible_header = msg.content.trim();
            let payload = msg
                .compaction_payload
                .as_deref()
                .map(str::trim)
                .filter(|payload| !payload.is_empty());

            match payload {
                Some(payload) if visible_header.is_empty() => payload.to_string(),
                Some(payload) if visible_header.contains(payload) => visible_header.to_string(),
                Some(payload) => format!("{visible_header}\n\nContent:\n{payload}"),
                None => msg.content.clone(),
            }
        };
        lines.push(Line::from(Span::styled(
            "---- auto compaction ----",
            theme.fg_dim,
        )));
        for line in wrap_text(&compaction_content, content_width) {
            lines.push(Line::from(Span::styled(line, theme.fg_active)));
        }
        lines.push(Line::from(Span::styled(
            "------------------------",
            theme.fg_dim,
        )));
        return;
    }

    if let Some(operator_question_lines) =
        render_operator_question_message(msg, theme, content_width)
    {
        lines.extend(operator_question_lines);
        return;
    }

    // TOOL messages: compact one-liner or expanded with args + result
    if msg.role == MessageRole::Tool {
        if let Some(name) = &msg.tool_name {
            let status = msg.tool_status.as_deref().unwrap_or("done");
            let (status_text, status_style) = format_tool_status(status, theme);
            let is_expanded = expanded_tools.contains(&msg_index);
            let mut header_spans = vec![
                Span::styled(toggle_glyph(is_expanded), theme.fg_dim),
                Span::raw(" "),
                Span::styled("\u{2699}", theme.accent_assistant),
                Span::raw("  "),
                Span::styled(name.clone(), theme.fg_dim),
                Span::raw(" "),
                Span::styled(status_text, status_style),
            ];

            if let Some(review) = msg.weles_review.as_ref() {
                let (badge, badge_style) = format_weles_review_badge(review, theme);
                header_spans.push(Span::raw(" "));
                header_spans.push(Span::styled(badge, badge_style));
            }
            lines.push(Line::from(header_spans));

            // Expanded tool details
            if is_expanded {
                let detail_indent = 4;
                let detail_width = width.saturating_sub(detail_indent + 1);

                if let Some(review) = msg.weles_review.as_ref() {
                    render_weles_review_details(review, theme, detail_width, lines);
                }

                // Show arguments
                if let Some(args) = &msg.tool_arguments {
                    if !args.is_empty() {
                        if let Some(diff_lines) =
                            render_tool_edit_diff(name, args, theme, detail_width.max(1))
                        {
                            lines.push(Line::from(vec![Span::styled("changes:", theme.fg_dim)]));
                            lines.extend(diff_lines);
                        } else if let Some(structured_args) = render_tool_structured_json(
                            name,
                            ToolStructuredValueSource::Arguments,
                            args,
                            theme,
                            detail_width.max(1),
                        ) {
                            lines.push(Line::from(vec![Span::styled("args:", theme.fg_dim)]));
                            lines.extend(structured_args);
                        } else {
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
                }

                // Show full result
                let result_text = &msg.content;
                if !result_text.is_empty() {
                    if let Some(structured_result) = render_tool_structured_json(
                        name,
                        ToolStructuredValueSource::Result,
                        result_text,
                        theme,
                        detail_width.max(1),
                    ) {
                        lines.push(Line::from(vec![Span::styled("result:", theme.fg_dim)]));
                        lines.extend(structured_result);
                    } else {
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
                format!("{} Reasoning", toggle_glyph(true)),
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
                format!("{} Reasoning", toggle_glyph(false)),
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
#[path = "tests/message.rs"]
mod tests;
