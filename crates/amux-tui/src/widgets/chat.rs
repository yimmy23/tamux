use ratatui::prelude::*;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use super::message::wrap_text;
use crate::state::chat::{AgentMessage, ChatHitTarget, ChatState, MessageRole, TranscriptMode};
use crate::theme::ThemeTokens;

const MESSAGE_PADDING_X: usize = 2;
const MESSAGE_PADDING_Y: usize = 1;

fn render_streaming_markdown(content: &str, width: usize) -> Vec<Line<'static>> {
    super::message::render_markdown_pub(content, width)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SelectionPoint {
    pub row: usize,
    pub col: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RenderedLineKind {
    MessageBody,
    ReasoningToggle,
    ReasoningContent,
    ToolToggle,
    ToolDetail,
    ActionBar,
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

struct SelectionSnapshot {
    inner: Rect,
    all_lines: Vec<RenderedChatLine>,
    start_idx: usize,
    end_idx: usize,
    padding: usize,
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

fn padded_content_width(inner_width: usize) -> usize {
    inner_width.saturating_sub(MESSAGE_PADDING_X * 2).max(1)
}

fn line_display_width(line: &Line<'_>) -> usize {
    line.spans
        .iter()
        .map(|span| UnicodeWidthStr::width(span.content.as_ref()))
        .sum()
}

fn blank_message_line(width: usize, style: Style) -> Line<'static> {
    Line::from(Span::styled(" ".repeat(width.max(1)), style))
}

fn rendered_line_plain_text(rendered: &RenderedChatLine) -> String {
    rendered
        .line
        .spans
        .iter()
        .map(|span| span.content.as_ref())
        .collect()
}

fn rendered_line_content_bounds(rendered: &RenderedChatLine) -> (String, usize, usize) {
    let plain = rendered_line_plain_text(rendered);
    let trimmed = plain.trim_end_matches(' ');
    let trimmed_width = UnicodeWidthStr::width(trimmed);
    let content_start = MESSAGE_PADDING_X.min(trimmed_width);
    let content_end = trimmed_width.max(content_start);
    (plain, content_start, content_end)
}

fn pad_message_line(mut line: Line<'static>, width: usize, style: Style) -> Line<'static> {
    let mut spans = Vec::new();
    let left = " ".repeat(MESSAGE_PADDING_X);
    spans.push(Span::styled(left, style));

    for span in line.spans.drain(..) {
        spans.push(Span::styled(
            span.content.to_string(),
            span.style.patch(style),
        ));
    }

    let content_width = line_display_width(&Line::from(spans.clone()));
    let right_width = width.saturating_sub(content_width).max(MESSAGE_PADDING_X);
    spans.push(Span::styled(" ".repeat(right_width), style));

    Line::from(spans).style(line.style.patch(style))
}

fn message_block_style(msg: &AgentMessage, theme: &ThemeTokens) -> Style {
    match msg.role {
        MessageRole::User => theme.fg_active.bg(Color::Indexed(236)),
        _ => Style::default(),
    }
}

fn message_action_targets(
    msg_index: usize,
    msg: &AgentMessage,
) -> Vec<(&'static str, ChatHitTarget)> {
    let mut actions = vec![("[Copy]", ChatHitTarget::CopyMessage(msg_index))];
    match msg.role {
        MessageRole::User => {
            actions.push(("[Resend]", ChatHitTarget::ResendMessage(msg_index)));
        }
        MessageRole::Assistant => {
            actions.push(("[Regenerate]", ChatHitTarget::RegenerateMessage(msg_index)));
        }
        _ => {}
    }
    actions.push(("[Delete]", ChatHitTarget::DeleteMessage(msg_index)));
    actions
}

fn message_action_line(
    msg_index: usize,
    msg: &AgentMessage,
    theme: &ThemeTokens,
) -> Option<Line<'static>> {
    let actions = message_action_targets(msg_index, msg);
    if actions.is_empty() {
        return None;
    }

    let mut spans = Vec::new();
    for (idx, (label, _)) in actions.into_iter().enumerate() {
        if idx > 0 {
            spans.push(Span::raw(" "));
        }
        spans.push(Span::styled(label, theme.accent_primary));
    }

    Some(Line::from(spans))
}

fn action_hit_target(
    msg_index: usize,
    msg: &AgentMessage,
    content_col: usize,
) -> Option<ChatHitTarget> {
    let mut col = 0usize;
    for (idx, (label, target)) in message_action_targets(msg_index, msg)
        .into_iter()
        .enumerate()
    {
        let width = UnicodeWidthStr::width(label);
        if content_col >= col && content_col < col.saturating_add(width) {
            return Some(target);
        }
        col = col.saturating_add(width);
        if idx + 1 < message_action_targets(msg_index, msg).len() {
            col = col.saturating_add(1);
        }
    }
    None
}

fn classify_message_lines(
    msg: &AgentMessage,
    msg_index: usize,
    mode: TranscriptMode,
    width: usize,
    expanded: &std::collections::HashSet<usize>,
    expanded_tools: &std::collections::HashSet<usize>,
) -> Vec<RenderedLineKind> {
    let content_width = padded_content_width(width);

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
                let mut kinds = vec![reasoning_toggle_kind];

                if reasoning_expanded {
                    let reasoning_width = content_width.saturating_sub(2).max(1);
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
    let content_width = padded_content_width(inner_width);

    if let Some(thread) = chat.active_thread() {
        for (idx, msg) in thread.messages.iter().enumerate() {
            let start = all_lines.len();
            let msg_lines = super::message::message_to_lines(
                msg,
                idx,
                mode,
                theme,
                content_width,
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

            let block_style = message_block_style(msg, theme);
            for _ in 0..MESSAGE_PADDING_Y {
                all_lines.push(RenderedChatLine {
                    line: blank_message_line(inner_width, block_style),
                    message_index: Some(idx),
                    kind: RenderedLineKind::Padding,
                });
            }

            for (line, kind) in msg_lines.into_iter().zip(kinds.into_iter()) {
                all_lines.push(RenderedChatLine {
                    line: pad_message_line(line, inner_width, block_style),
                    message_index: Some(idx),
                    kind,
                });
            }

            for _ in 0..MESSAGE_PADDING_Y {
                all_lines.push(RenderedChatLine {
                    line: blank_message_line(inner_width, block_style),
                    message_index: Some(idx),
                    kind: RenderedLineKind::Padding,
                });
            }

            if chat.selected_message() == Some(idx) {
                if let Some(action_line) = message_action_line(idx, msg, theme) {
                    all_lines.push(RenderedChatLine {
                        line: pad_message_line(action_line, inner_width, block_style),
                        message_index: Some(idx),
                        kind: RenderedLineKind::ActionBar,
                    });
                }
            }

            let end = all_lines.len();
            message_line_ranges.push((start, end));
        }
    }

    let assistant_style = Style::default();
    if !chat.streaming_reasoning().is_empty() {
        all_lines.push(RenderedChatLine {
            line: blank_message_line(inner_width, assistant_style),
            message_index: None,
            kind: RenderedLineKind::Streaming,
        });
        all_lines.push(RenderedChatLine {
            line: pad_message_line(
                Line::from(vec![Span::styled("\u{25be} Reasoning...", theme.fg_dim)]),
                inner_width,
                assistant_style,
            ),
            message_index: None,
            kind: RenderedLineKind::Streaming,
        });

        let dark_blue = Style::default().fg(Color::Indexed(24));
        let wrap_width = content_width.saturating_sub(2).max(1);
        for reasoning_line in chat.streaming_reasoning().lines() {
            let wrapped_lines = wrap_text(reasoning_line, wrap_width);
            let wrapped_lines = if wrapped_lines.is_empty() {
                vec![String::new()]
            } else {
                wrapped_lines
            };
            for wrapped in wrapped_lines {
                all_lines.push(RenderedChatLine {
                    line: pad_message_line(
                        Line::from(vec![
                            Span::styled("\u{2502}", dark_blue),
                            Span::raw(" "),
                            Span::styled(wrapped, theme.fg_dim),
                        ]),
                        inner_width,
                        assistant_style,
                    ),
                    message_index: None,
                    kind: RenderedLineKind::Streaming,
                });
            }
        }
    }

    if !chat.streaming_content().is_empty() {
        let content = chat.streaming_content();
        if chat.streaming_reasoning().is_empty() {
            all_lines.push(RenderedChatLine {
                line: blank_message_line(inner_width, assistant_style),
                message_index: None,
                kind: RenderedLineKind::Streaming,
            });
        }
        let wrap_width = content_width;
        let wrapped_lines = render_streaming_markdown(content, wrap_width);

        for md_line in wrapped_lines.into_iter() {
            all_lines.push(RenderedChatLine {
                line: pad_message_line(md_line, inner_width, assistant_style),
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

    if !chat.is_streaming() {
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
    }

    scroll
}

fn content_inner(area: Rect) -> Rect {
    area
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

fn visible_window_bounds(
    total: usize,
    inner_height: usize,
    scroll: usize,
) -> (usize, usize, usize) {
    if total <= inner_height {
        let padding = inner_height.saturating_sub(total);
        return (padding, 0, total);
    }

    if scroll == 0 {
        return (0, total - inner_height, total);
    }

    let end = total.saturating_sub(scroll);
    let start = end.saturating_sub(inner_height);
    (0, start, end)
}

fn visible_rendered_lines(
    area: Rect,
    chat: &ChatState,
    theme: &ThemeTokens,
) -> Option<(Rect, Vec<RenderedChatLine>)> {
    if (chat.active_thread().is_none() && chat.streaming_content().is_empty())
        || area.width == 0
        || area.height == 0
    {
        return None;
    }

    let inner = content_inner(area);
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

fn selection_snapshot(
    area: Rect,
    chat: &ChatState,
    theme: &ThemeTokens,
) -> Option<SelectionSnapshot> {
    let inner = content_inner(area);
    if inner.width == 0 || inner.height == 0 {
        return None;
    }

    let (all_lines, message_line_ranges) = build_rendered_lines(chat, theme, inner.width as usize);
    if all_lines.is_empty() {
        return None;
    }

    let scroll = resolved_scroll(
        chat,
        all_lines.len(),
        inner.height as usize,
        &message_line_ranges,
    );
    let (padding, start_idx, end_idx) =
        visible_window_bounds(all_lines.len(), inner.height as usize, scroll);

    Some(SelectionSnapshot {
        inner,
        all_lines,
        start_idx,
        end_idx,
        padding,
    })
}

fn nearest_content_row(all_lines: &[RenderedChatLine], row: usize) -> Option<usize> {
    let current = all_lines.get(row)?;
    let current_has_content = !matches!(current.kind, RenderedLineKind::Padding)
        && rendered_line_content_bounds(current).2 > rendered_line_content_bounds(current).1;
    if current_has_content {
        return Some(row);
    }

    if let Some(message_index) = current.message_index {
        for distance in 1..all_lines.len() {
            if let Some(prev) = row.checked_sub(distance) {
                let line = &all_lines[prev];
                if line.message_index == Some(message_index)
                    && !matches!(line.kind, RenderedLineKind::Padding)
                    && rendered_line_content_bounds(line).2 > rendered_line_content_bounds(line).1
                {
                    return Some(prev);
                }
            }

            let next = row.saturating_add(distance);
            if let Some(line) = all_lines.get(next) {
                if line.message_index == Some(message_index)
                    && !matches!(line.kind, RenderedLineKind::Padding)
                    && rendered_line_content_bounds(line).2 > rendered_line_content_bounds(line).1
                {
                    return Some(next);
                }
            }
        }
    }

    for next in row + 1..all_lines.len() {
        let line = &all_lines[next];
        if !matches!(line.kind, RenderedLineKind::Padding)
            && rendered_line_content_bounds(line).2 > rendered_line_content_bounds(line).1
        {
            return Some(next);
        }
    }

    for prev in (0..row).rev() {
        let line = &all_lines[prev];
        if !matches!(line.kind, RenderedLineKind::Padding)
            && rendered_line_content_bounds(line).2 > rendered_line_content_bounds(line).1
        {
            return Some(prev);
        }
    }

    None
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
    start: SelectionPoint,
    end: SelectionPoint,
) -> Option<String> {
    let inner = content_inner(area);
    let (all_lines, message_line_ranges) = build_rendered_lines(chat, theme, inner.width as usize);
    if all_lines.is_empty() {
        return None;
    }
    let _scroll = resolved_scroll(
        chat,
        all_lines.len(),
        inner.height as usize,
        &message_line_ranges,
    );

    let (start_point, end_point) =
        if start.row <= end.row || (start.row == end.row && start.col <= end.col) {
            (start, end)
        } else {
            (end, start)
        };
    let start_row = start_point.row.min(all_lines.len().saturating_sub(1));
    let end_row = end_point.row.min(all_lines.len().saturating_sub(1));
    let start_col = start_point.col;
    let end_col = end_point.col;

    if start_row == end_row && start_col == end_col {
        return None;
    }

    let mut lines = Vec::new();

    for row in start_row..=end_row {
        let rendered = all_lines.get(row)?;
        let (plain, content_start, content_end) = rendered_line_content_bounds(rendered);
        let content_width = content_end.saturating_sub(content_start);
        let from = if row == start_row {
            start_col.min(content_width)
        } else {
            0
        };
        let to = if row == end_row {
            end_col.min(content_width).max(from)
        } else {
            content_width
        };

        lines.push(display_slice(
            &plain,
            content_start.saturating_add(from),
            content_start.saturating_add(to),
        ));
    }

    let text = lines.join("\n");
    if text.is_empty() {
        None
    } else {
        Some(text)
    }
}

pub fn selection_point_from_mouse(
    area: Rect,
    chat: &ChatState,
    theme: &ThemeTokens,
    mouse: Position,
) -> Option<SelectionPoint> {
    let snapshot = selection_snapshot(area, chat, theme)?;
    selection_point_from_snapshot(&snapshot, mouse)
}

pub fn selection_points_from_mouse(
    area: Rect,
    chat: &ChatState,
    theme: &ThemeTokens,
    start: Position,
    end: Position,
) -> Option<(SelectionPoint, SelectionPoint)> {
    let snapshot = selection_snapshot(area, chat, theme)?;
    Some((
        selection_point_from_snapshot(&snapshot, start)?,
        selection_point_from_snapshot(&snapshot, end)?,
    ))
}

fn selection_point_from_snapshot(
    snapshot: &SelectionSnapshot,
    mouse: Position,
) -> Option<SelectionPoint> {
    let inner = snapshot.inner;

    let clamped_x = mouse.x.clamp(
        inner.x,
        inner.x.saturating_add(inner.width).saturating_sub(1),
    );
    let clamped_y = mouse.y.clamp(
        inner.y,
        inner.y.saturating_add(inner.height).saturating_sub(1),
    );
    let rel_row = clamped_y.saturating_sub(inner.y) as usize;
    let rel_col = clamped_x.saturating_sub(inner.x) as usize;

    if rel_row < snapshot.padding {
        return None;
    }

    let row = {
        let visible_count = snapshot.end_idx.saturating_sub(snapshot.start_idx).max(1);
        snapshot.start_idx
            + rel_row
                .saturating_sub(snapshot.padding)
                .min(visible_count.saturating_sub(1))
    };
    let row = nearest_content_row(&snapshot.all_lines, row)?;
    let rendered = snapshot.all_lines.get(row)?;
    let (_, content_start, content_end) = rendered_line_content_bounds(rendered);
    let content_width = content_end.saturating_sub(content_start);
    let content_col = rel_col.saturating_sub(content_start).min(content_width);

    Some(SelectionPoint {
        row,
        col: content_col,
    })
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
    let mut resolved_row = row;
    if visible
        .get(resolved_row)
        .is_some_and(|line| matches!(line.kind, RenderedLineKind::Padding))
    {
        if let Some(next) = (resolved_row + 1..visible.len()).find(|idx| {
            !matches!(visible[*idx].kind, RenderedLineKind::Padding)
                && visible[*idx].message_index.is_some()
        }) {
            resolved_row = next;
        } else if let Some(prev) = (0..resolved_row).rev().find(|idx| {
            !matches!(visible[*idx].kind, RenderedLineKind::Padding)
                && visible[*idx].message_index.is_some()
        }) {
            resolved_row = prev;
        }
    }
    let hit = visible.get(resolved_row)?;
    let message_index = hit.message_index?;

    match hit.kind {
        RenderedLineKind::ReasoningToggle => Some(ChatHitTarget::ReasoningToggle(message_index)),
        RenderedLineKind::ToolToggle => Some(ChatHitTarget::ToolToggle(message_index)),
        RenderedLineKind::ActionBar => {
            let content_col = mouse.x.saturating_sub(inner.x) as usize;
            let (_, content_start, _) = rendered_line_content_bounds(hit);
            let message = chat
                .active_thread()
                .and_then(|thread| thread.messages.get(message_index))?;
            action_hit_target(
                message_index,
                message,
                content_col.saturating_sub(content_start),
            )
        }
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
    _focused: bool,
    mouse_selection: Option<(SelectionPoint, SelectionPoint)>,
) {
    let inner = content_inner(area);

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
        if let Some(&(start, end)) = message_line_ranges.get(sel_idx) {
            let sel_style = Style::default().bg(Color::Indexed(238));

            for rendered in all_lines.iter_mut().take(end).skip(start) {
                rendered.line.style = rendered.line.style.patch(sel_style);
                for span in &mut rendered.line.spans {
                    span.style = span.style.patch(sel_style);
                }
            }
        }
    }

    // Apply scroll offset (0 = following tail = show last `height` lines)
    // Clamp scroll to prevent overscroll past the top of content
    let scroll = resolved_scroll(chat, all_lines.len(), inner_height, &message_line_ranges);
    let (padding_rows, start_idx, end_idx) =
        visible_window_bounds(all_lines.len(), inner_height, scroll);
    let mut visible_lines = visible_lines(&all_lines, inner_height, scroll);

    if let Some((start, end)) = mouse_selection {
        let (start_point, end_point) =
            if start.row <= end.row || (start.row == end.row && start.col <= end.col) {
                (start, end)
            } else {
                (end, start)
            };
        let highlight = Style::default().bg(Color::Indexed(31));
        let visible_last = end_idx.saturating_sub(1);
        let range_start = start_point.row.max(start_idx);
        let range_end = end_point.row.min(visible_last);

        if range_start <= range_end {
            for absolute_row in range_start..=range_end {
                let visible_row = padding_rows + absolute_row.saturating_sub(start_idx);
                if let Some(line) = visible_lines.get_mut(visible_row) {
                    let rendered = RenderedChatLine {
                        line: line.line.clone(),
                        message_index: line.message_index,
                        kind: line.kind,
                    };
                    let (_, content_start, content_end) = rendered_line_content_bounds(&rendered);
                    let content_width = content_end.saturating_sub(content_start);
                    let from = if absolute_row == start_point.row {
                        content_start.saturating_add(start_point.col.min(content_width))
                    } else {
                        content_start
                    };
                    let to = if absolute_row == end_point.row {
                        content_start
                            .saturating_add(end_point.col.min(content_width))
                            .max(from)
                    } else {
                        content_end
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
    use crate::state::chat::{AgentThread, ChatAction, MessageRole};

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
