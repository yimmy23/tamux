use ratatui::prelude::*;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use super::message::wrap_text;
use crate::state::chat::{
    AgentMessage, ChatAction, ChatHitTarget, ChatState, MessageRole, TranscriptMode,
};
use crate::theme::ThemeTokens;

fn render_streaming_markdown(content: &str, width: usize) -> Vec<Line<'static>> {
    super::message::render_markdown_pub(content, width)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RenderedLineKind {
    MessageBody,
    ReasoningToggle,
    ReasoningContent,
    ToolToggle,
    ToolDetail,
    Separator,
    Padding,
    Streaming,
}

#[derive(Debug, Clone)]
struct RenderedChatLine {
    line: Line<'static>,
    message_index: Option<usize>,
    kind: RenderedLineKind,
}

impl RenderedChatLine {
    fn padding() -> Self {
        Self {
            line: Line::raw(""),
            message_index: None,
            kind: RenderedLineKind::Padding,
        }
    }

    fn separator() -> Self {
        Self {
            line: Line::raw(""),
            message_index: None,
            kind: RenderedLineKind::Separator,
        }
    }
}

fn classify_message_lines(
    msg: &AgentMessage,
    msg_index: usize,
    mode: TranscriptMode,
    width: usize,
    expanded: &std::collections::HashSet<usize>,
    expanded_tools: &std::collections::HashSet<usize>,
) -> Vec<RenderedLineKind> {
    let indent = 7;
    let content_width = width.saturating_sub(indent + 1);

    match mode {
        TranscriptMode::Tools => {
            if msg.role != MessageRole::Tool && msg.tool_name.is_none() {
                return Vec::new();
            }
            vec![RenderedLineKind::MessageBody]
        }
        TranscriptMode::Compact | TranscriptMode::Full => {
            let tool_toggle_kind = if matches!(mode, TranscriptMode::Compact) {
                RenderedLineKind::ToolToggle
            } else {
                RenderedLineKind::MessageBody
            };
            let reasoning_toggle_kind = if matches!(mode, TranscriptMode::Compact) {
                RenderedLineKind::ReasoningToggle
            } else {
                RenderedLineKind::MessageBody
            };
            let tools_expanded =
                matches!(mode, TranscriptMode::Full) || expanded_tools.contains(&msg_index);
            let reasoning_expanded =
                matches!(mode, TranscriptMode::Full) || expanded.contains(&msg_index);

            if msg.role == MessageRole::Tool {
                if msg.tool_name.is_none() {
                    return Vec::new();
                }

                let mut kinds = vec![tool_toggle_kind];

                if tools_expanded {
                    if msg
                        .tool_arguments
                        .as_deref()
                        .is_some_and(|args| !args.is_empty())
                    {
                        kinds.push(RenderedLineKind::ToolDetail);
                    }

                    if !msg.content.is_empty() {
                        let result_lines = msg.content.lines().count().min(5);
                        kinds.extend(std::iter::repeat_n(
                            RenderedLineKind::ToolDetail,
                            result_lines,
                        ));
                        if msg.content.lines().count() > 5 {
                            kinds.push(RenderedLineKind::ToolDetail);
                        }
                    }
                }

                return kinds;
            }

            if msg.content.is_empty() && msg.role != MessageRole::Assistant {
                return Vec::new();
            }
            if msg.content.is_empty() && msg.reasoning.is_none() {
                return Vec::new();
            }

            let content_lines = if msg.content.is_empty() {
                0
            } else if msg.role == MessageRole::Assistant {
                super::message::render_markdown_pub(&msg.content, content_width).len()
            } else {
                wrap_text(&msg.content, content_width).len()
            };

            let has_reasoning = msg.role == MessageRole::Assistant
                && msg
                    .reasoning
                    .as_deref()
                    .is_some_and(|reasoning| !reasoning.is_empty());

            if has_reasoning {
                let mut kinds = vec![RenderedLineKind::MessageBody];
                kinds.push(reasoning_toggle_kind);

                if reasoning_expanded {
                    let reasoning_width = width.saturating_sub(indent + 2);
                    let reasoning_line_count = wrap_text(
                        msg.reasoning.as_deref().unwrap_or_default(),
                        reasoning_width,
                    )
                    .len();
                    kinds.extend(std::iter::repeat_n(
                        RenderedLineKind::ReasoningContent,
                        reasoning_line_count.max(1),
                    ));
                }

                kinds.extend(std::iter::repeat_n(
                    RenderedLineKind::MessageBody,
                    content_lines,
                ));
                kinds
            } else {
                vec![RenderedLineKind::MessageBody; content_lines.max(1)]
            }
        }
    }
}

fn build_rendered_lines(
    chat: &ChatState,
    theme: &ThemeTokens,
    inner_width: usize,
) -> (Vec<RenderedChatLine>, Vec<(usize, usize)>) {
    let mut all_lines = Vec::new();
    let mut message_line_ranges = Vec::new();
    let mode = chat.transcript_mode();
    let expanded = chat.expanded_reasoning();
    let expanded_tools = chat.expanded_tools();

    if let Some(thread) = chat.active_thread() {
        let message_count = thread.messages.len();
        for (idx, msg) in thread.messages.iter().enumerate() {
            let start = all_lines.len();
            let msg_lines = super::message::message_to_lines(
                msg,
                idx,
                mode,
                theme,
                inner_width,
                expanded,
                expanded_tools,
            );
            let mut kinds =
                classify_message_lines(msg, idx, mode, inner_width, expanded, expanded_tools);

            if kinds.len() < msg_lines.len() {
                kinds.resize(msg_lines.len(), RenderedLineKind::MessageBody);
            } else if kinds.len() > msg_lines.len() {
                kinds.truncate(msg_lines.len());
            }

            for (line, kind) in msg_lines.into_iter().zip(kinds.into_iter()) {
                all_lines.push(RenderedChatLine {
                    line,
                    message_index: Some(idx),
                    kind,
                });
            }

            let end = all_lines.len();
            message_line_ranges.push((start, end));

            if end > start && idx + 1 < message_count {
                all_lines.push(RenderedChatLine::separator());
            }
        }
    }

    if !chat.streaming_reasoning().is_empty() {
        all_lines.push(RenderedChatLine {
            line: Line::from(vec![
                Span::raw("  "),
                Span::styled(
                    " ASST ",
                    Style::default().bg(Color::Indexed(183)).fg(Color::Black),
                ),
            ]),
            message_index: None,
            kind: RenderedLineKind::Streaming,
        });
        all_lines.push(RenderedChatLine {
            line: Line::from(vec![
                Span::raw("       "),
                Span::styled("\u{25be} Reasoning...", theme.fg_dim),
            ]),
            message_index: None,
            kind: RenderedLineKind::Streaming,
        });

        let dark_blue = Style::default().fg(Color::Indexed(24));
        for reasoning_line in chat.streaming_reasoning().lines() {
            all_lines.push(RenderedChatLine {
                line: Line::from(vec![
                    Span::raw("       "),
                    Span::styled("\u{2502}", dark_blue),
                    Span::raw(" "),
                    Span::styled(reasoning_line.to_string(), theme.fg_dim),
                ]),
                message_index: None,
                kind: RenderedLineKind::Streaming,
            });
        }
    }

    if !chat.streaming_content().is_empty() {
        let content = chat.streaming_content();
        let wrap_width = inner_width.saturating_sub(8);
        let wrapped_lines = render_streaming_markdown(content, wrap_width);
        let show_badge = chat.streaming_reasoning().is_empty();

        for (idx, md_line) in wrapped_lines.into_iter().enumerate() {
            let line = if idx == 0 && show_badge {
                let mut spans = vec![
                    Span::raw("  "),
                    Span::styled(
                        " ASST ",
                        Style::default().bg(Color::Indexed(183)).fg(Color::Black),
                    ),
                    Span::raw(" "),
                ];
                spans.extend(md_line.spans);
                Line::from(spans)
            } else {
                let mut spans = vec![Span::raw("       ")];
                spans.extend(md_line.spans);
                Line::from(spans)
            };

            all_lines.push(RenderedChatLine {
                line,
                message_index: None,
                kind: RenderedLineKind::Streaming,
            });
        }

        if let Some(last) = all_lines.last_mut() {
            last.line.spans.push(Span::raw("\u{2588}"));
        }
    }

    (all_lines, message_line_ranges)
}

fn resolved_scroll(
    chat: &ChatState,
    total_lines: usize,
    inner_height: usize,
    message_line_ranges: &[(usize, usize)],
) -> usize {
    let max_scroll = total_lines.saturating_sub(inner_height);
    let mut scroll = chat.scroll_offset().min(max_scroll);

    if chat.scroll_locked() {
        return scroll;
    }

    if let Some(sel_idx) = chat.selected_message() {
        if let Some(&(sel_start, sel_end)) = message_line_ranges.get(sel_idx) {
            let window_end = total_lines.saturating_sub(scroll);
            let window_start = window_end.saturating_sub(inner_height);

            if sel_start < window_start {
                scroll = total_lines
                    .saturating_sub(sel_start + inner_height)
                    .min(max_scroll);
            } else if sel_end > window_end {
                scroll = total_lines.saturating_sub(sel_end).min(max_scroll);
            }
        }
    }

    scroll
}

fn visible_lines(
    all_lines: &[RenderedChatLine],
    inner_height: usize,
    scroll: usize,
) -> Vec<RenderedChatLine> {
    let total = all_lines.len();

    if total <= inner_height {
        let mut padded = Vec::with_capacity(inner_height);
        for _ in 0..inner_height.saturating_sub(total) {
            padded.push(RenderedChatLine::padding());
        }
        padded.extend_from_slice(all_lines);
        return padded;
    }

    if scroll == 0 {
        return all_lines[total - inner_height..].to_vec();
    }

    let end = total.saturating_sub(scroll);
    let start = end.saturating_sub(inner_height);
    all_lines[start..end].to_vec()
}

fn visible_rendered_lines(
    area: Rect,
    chat: &ChatState,
    theme: &ThemeTokens,
) -> Option<(Rect, Vec<RenderedChatLine>)> {
    if (chat.active_thread().is_none() && chat.streaming_content().is_empty())
        || area.width <= 2
        || area.height <= 2
    {
        return None;
    }

    let inner = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .inner(area);

    let (all_lines, message_line_ranges) = build_rendered_lines(chat, theme, inner.width as usize);
    let scroll = resolved_scroll(
        chat,
        all_lines.len(),
        inner.height as usize,
        &message_line_ranges,
    );
    let visible = visible_lines(&all_lines, inner.height as usize, scroll);

    Some((inner, visible))
}

fn normalize_selection(
    inner: Rect,
    start: Position,
    end: Position,
) -> Option<((usize, usize), (usize, usize))> {
    if inner.width == 0 || inner.height == 0 {
        return None;
    }

    let clamp_x = |x: u16| {
        x.clamp(
            inner.x,
            inner.x.saturating_add(inner.width).saturating_sub(1),
        )
    };
    let clamp_y = |y: u16| {
        y.clamp(
            inner.y,
            inner.y.saturating_add(inner.height).saturating_sub(1),
        )
    };

    let start_col = clamp_x(start.x).saturating_sub(inner.x) as usize;
    let start_row = clamp_y(start.y).saturating_sub(inner.y) as usize;
    let end_col = clamp_x(end.x).saturating_sub(inner.x) as usize;
    let end_row = clamp_y(end.y).saturating_sub(inner.y) as usize;

    let start_norm = (start_row, start_col);
    let end_norm = (end_row, end_col);

    if start_norm <= end_norm {
        Some((start_norm, end_norm))
    } else {
        Some((end_norm, start_norm))
    }
}

fn display_slice(text: &str, start_col: usize, end_col: usize) -> String {
    if start_col >= end_col {
        return String::new();
    }

    let mut result = String::new();
    let mut col = 0usize;

    for ch in text.chars() {
        let width = UnicodeWidthChar::width(ch).unwrap_or(0);
        let next = col + width;
        let overlaps = if width == 0 {
            col >= start_col && col < end_col
        } else {
            next > start_col && col < end_col
        };
        if overlaps {
            result.push(ch);
        }
        col = next;
        if col >= end_col {
            break;
        }
    }

    result
}

fn highlight_line_range(
    line: &mut Line<'static>,
    start_col: usize,
    end_col: usize,
    highlight: Style,
) {
    if start_col >= end_col {
        return;
    }

    let original_spans = std::mem::take(&mut line.spans);
    let mut spans = Vec::new();
    let mut col = 0usize;

    for span in original_spans {
        let mut before = String::new();
        let mut selected = String::new();
        let mut after = String::new();

        for ch in span.content.chars() {
            let width = UnicodeWidthChar::width(ch).unwrap_or(0);
            let next = col + width;
            let overlaps = if width == 0 {
                col >= start_col && col < end_col
            } else {
                next > start_col && col < end_col
            };

            if overlaps {
                selected.push(ch);
            } else if col < start_col {
                before.push(ch);
            } else {
                after.push(ch);
            }

            col = next;
        }

        if !before.is_empty() {
            spans.push(Span::styled(before, span.style));
        }
        if !selected.is_empty() {
            spans.push(Span::styled(selected, span.style.patch(highlight)));
        }
        if !after.is_empty() {
            spans.push(Span::styled(after, span.style));
        }
    }

    line.spans = spans;
}

pub fn selected_text(
    area: Rect,
    chat: &ChatState,
    theme: &ThemeTokens,
    start: Position,
    end: Position,
) -> Option<String> {
    let (inner, visible) = visible_rendered_lines(area, chat, theme)?;
    let ((start_row, start_col), (end_row, end_col)) = normalize_selection(inner, start, end)?;

    if start_row == end_row && start_col == end_col {
        return None;
    }

    let mut lines = Vec::new();

    for row in start_row..=end_row {
        let rendered = visible.get(row)?;
        let plain: String = rendered
            .line
            .spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect();
        let line_width = UnicodeWidthStr::width(plain.as_str());
        let from = if row == start_row { start_col } else { 0 };
        let to = if row == end_row {
            end_col.max(from)
        } else {
            line_width
        };

        lines.push(display_slice(&plain, from, to));
    }

    let text = lines.join("\n");
    if text.is_empty() {
        None
    } else {
        Some(text)
    }
}

pub fn hit_test(
    area: Rect,
    chat: &ChatState,
    theme: &ThemeTokens,
    mouse: Position,
) -> Option<ChatHitTarget> {
    let (inner, visible) = visible_rendered_lines(area, chat, theme)?;

    if mouse.x < inner.x
        || mouse.y < inner.y
        || mouse.x >= inner.x.saturating_add(inner.width)
        || mouse.y >= inner.y.saturating_add(inner.height)
    {
        return None;
    }
    let row = mouse.y.saturating_sub(inner.y) as usize;
    let hit = visible.get(row)?;
    let message_index = hit.message_index?;

    match hit.kind {
        RenderedLineKind::ReasoningToggle => Some(ChatHitTarget::ReasoningToggle(message_index)),
        RenderedLineKind::ToolToggle => Some(ChatHitTarget::ToolToggle(message_index)),
        RenderedLineKind::MessageBody
        | RenderedLineKind::ReasoningContent
        | RenderedLineKind::ToolDetail => Some(ChatHitTarget::Message(message_index)),
        RenderedLineKind::Separator | RenderedLineKind::Padding | RenderedLineKind::Streaming => {
            None
        }
    }
}

pub fn render(
    frame: &mut Frame,
    area: Rect,
    chat: &ChatState,
    theme: &ThemeTokens,
    focused: bool,
    mouse_selection: Option<(Position, Position)>,
) {
    let border_style = if focused {
        theme.accent_primary
    } else {
        theme.fg_dim
    };
    let title_style = if focused {
        theme.fg_active
    } else {
        theme.fg_dim
    };
    let block = Block::default()
        .title(Span::styled(" Conversation ", title_style))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(border_style);

    let inner = block.inner(area);
    frame.render_widget(block, area);

    if chat.active_thread().is_none() && chat.streaming_content().is_empty() {
        // Render splash
        super::splash::render(frame, inner, theme);
        return;
    }

    // Render messages
    let inner_width = inner.width as usize;
    let inner_height = inner.height as usize;
    let selected_msg = chat.selected_message();
    let (mut all_lines, message_line_ranges) = build_rendered_lines(chat, theme, inner_width);

    // Apply selection highlight
    if let Some(sel_idx) = selected_msg {
        if let Some(&(start, _)) = message_line_ranges.get(sel_idx) {
            let sel_style = Style::default()
                .bg(Color::Indexed(236))
                .add_modifier(Modifier::empty());

            for (line_idx, rendered) in all_lines.iter_mut().enumerate() {
                if rendered.message_index != Some(sel_idx) {
                    continue;
                }

                let plain: String = rendered
                    .line
                    .spans
                    .iter()
                    .map(|span| span.content.as_ref())
                    .collect();
                if plain.trim().is_empty() {
                    continue;
                }

                if line_idx == start {
                    let mut new_spans =
                        vec![Span::styled("> ", Style::default().fg(Color::Indexed(178)))];
                    new_spans.extend(rendered.line.spans.iter().cloned());
                    rendered.line = Line::from(new_spans).style(sel_style);
                } else {
                    rendered.line = rendered.line.clone().style(sel_style);
                }
            }
        }
    }

    // Apply scroll offset (0 = following tail = show last `height` lines)
    // Clamp scroll to prevent overscroll past the top of content
    let scroll = resolved_scroll(chat, all_lines.len(), inner_height, &message_line_ranges);
    let mut visible_lines = visible_lines(&all_lines, inner_height, scroll);

    if let Some((start, end)) = mouse_selection {
        if let Some(((start_row, start_col), (end_row, end_col))) =
            normalize_selection(inner, start, end)
        {
            let highlight = Style::default().bg(Color::Indexed(31));

            for row in start_row..=end_row {
                if let Some(line) = visible_lines.get_mut(row) {
                    let plain: String = line
                        .line
                        .spans
                        .iter()
                        .map(|span| span.content.as_ref())
                        .collect();
                    let line_width = UnicodeWidthStr::width(plain.as_str());
                    let from = if row == start_row { start_col } else { 0 };
                    let to = if row == end_row {
                        end_col.max(from)
                    } else {
                        line_width
                    };
                    highlight_line_range(&mut line.line, from, to, highlight);
                }
            }
        }
    }

    let visible_lines = visible_lines
        .into_iter()
        .map(|line| line.line)
        .collect::<Vec<_>>();

    // Do not use .wrap() -- lines are already individual Line objects; wrapping
    // would re-wrap the manually-sliced visible lines and cause double-wrapping.
    let paragraph = Paragraph::new(visible_lines);
    frame.render_widget(paragraph, inner);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::chat::{AgentThread, MessageRole};

    fn chat_with_messages(messages: Vec<AgentMessage>) -> ChatState {
        let mut chat = ChatState::new();
        chat.reduce(ChatAction::ThreadCreated {
            thread_id: "t1".into(),
            title: "Test".into(),
        });
        chat.reduce(ChatAction::ThreadDetailReceived(AgentThread {
            id: "t1".into(),
            title: "Test".into(),
            messages,
            ..Default::default()
        }));
        chat
    }

    #[test]
    fn chat_handles_empty_state() {
        let chat = ChatState::new();
        assert!(chat.active_thread().is_none());
        assert!(chat.streaming_content().is_empty());
    }

    #[test]
    fn hit_test_selects_clicked_message_row() {
        let chat = chat_with_messages(vec![
            AgentMessage {
                role: MessageRole::User,
                content: "first".into(),
                ..Default::default()
            },
            AgentMessage {
                role: MessageRole::User,
                content: "second".into(),
                ..Default::default()
            },
        ]);

        let hit = hit_test(
            Rect::new(0, 0, 80, 6),
            &chat,
            &ThemeTokens::default(),
            Position::new(2, 4),
        );

        assert_eq!(hit, Some(ChatHitTarget::Message(1)));
    }

    #[test]
    fn hit_test_marks_reasoning_header_as_toggle() {
        let chat = chat_with_messages(vec![AgentMessage {
            role: MessageRole::Assistant,
            content: "Answer".into(),
            reasoning: Some("Think".into()),
            ..Default::default()
        }]);

        let hit = hit_test(
            Rect::new(0, 0, 80, 5),
            &chat,
            &ThemeTokens::default(),
            Position::new(2, 2),
        );

        assert_eq!(hit, Some(ChatHitTarget::ReasoningToggle(0)));
    }

    #[test]
    fn hit_test_marks_tool_header_as_toggle() {
        let chat = chat_with_messages(vec![AgentMessage {
            role: MessageRole::Tool,
            tool_name: Some("bash_command".into()),
            tool_status: Some("done".into()),
            content: "ok".into(),
            ..Default::default()
        }]);

        let hit = hit_test(
            Rect::new(0, 0, 80, 4),
            &chat,
            &ThemeTokens::default(),
            Position::new(2, 2),
        );

        assert_eq!(hit, Some(ChatHitTarget::ToolToggle(0)));
    }

    #[test]
    fn hit_test_full_mode_reasoning_header_only_selects_message() {
        let mut chat = chat_with_messages(vec![AgentMessage {
            role: MessageRole::Assistant,
            content: "Answer".into(),
            reasoning: Some("Think".into()),
            ..Default::default()
        }]);
        chat.reduce(ChatAction::SetTranscriptMode(TranscriptMode::Full));

        let hit = hit_test(
            Rect::new(0, 0, 80, 6),
            &chat,
            &ThemeTokens::default(),
            Position::new(2, 2),
        );

        assert_eq!(hit, Some(ChatHitTarget::Message(0)));
    }
}
