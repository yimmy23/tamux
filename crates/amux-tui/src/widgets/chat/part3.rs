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

    if let Some(pin_idx) = chat.pinned_message_top() {
        if let Some(&(pin_start, _)) = message_line_ranges.get(pin_idx) {
            scroll = total_lines
                .saturating_sub(pin_start.saturating_add(inner_height))
                .min(max_scroll);
            return scroll;
        }
    }

    scroll
}

fn content_inner(area: Rect) -> Rect {
    area
}

fn scrollbar_layout_from_metrics(
    area: Rect,
    total_lines: usize,
    scroll: usize,
) -> Option<ChatScrollbarLayout> {
    if area.width <= SCROLLBAR_WIDTH || area.height == 0 || total_lines <= area.height as usize {
        return None;
    }

    let viewport = area.height as usize;
    let max_scroll = total_lines.saturating_sub(viewport);
    let scroll = scroll.min(max_scroll);
    let content = Rect::new(
        area.x,
        area.y,
        area.width.saturating_sub(SCROLLBAR_WIDTH),
        area.height,
    );
    let scrollbar = Rect::new(
        area.x
            .saturating_add(area.width)
            .saturating_sub(SCROLLBAR_WIDTH),
        area.y,
        SCROLLBAR_WIDTH,
        area.height,
    );

    let thumb_height = ((viewport * viewport) / total_lines).max(1).min(viewport) as u16;
    let track_span = scrollbar.height.saturating_sub(thumb_height);
    let thumb_offset = if max_scroll == 0 || track_span == 0 {
        0
    } else {
        (((max_scroll.saturating_sub(scroll)) * track_span as usize) + (max_scroll / 2))
            / max_scroll
    } as u16;
    let thumb = Rect::new(
        scrollbar.x,
        scrollbar.y.saturating_add(thumb_offset),
        scrollbar.width,
        thumb_height,
    );

    Some(ChatScrollbarLayout {
        content,
        scrollbar,
        thumb,
        scroll,
        max_scroll,
    })
}

fn scroll_offset_from_thumb_offset(thumb_offset: u16, track_span: u16, max_scroll: usize) -> usize {
    if max_scroll == 0 || track_span == 0 {
        return 0;
    }

    let toward_bottom =
        ((thumb_offset as usize * max_scroll) + (track_span as usize / 2)) / track_span as usize;
    max_scroll.saturating_sub(toward_bottom.min(max_scroll))
}

pub(crate) fn scrollbar_layout(
    area: Rect,
    chat: &ChatState,
    theme: &ThemeTokens,
    current_tick: u64,
    retry_wait_start_selected: bool,
) -> Option<ChatScrollbarLayout> {
    if area.width <= SCROLLBAR_WIDTH || area.height == 0 {
        return None;
    }

    let (all_lines, message_line_ranges) = build_rendered_lines(
        chat,
        theme,
        area.width as usize,
        current_tick,
        retry_wait_start_selected,
    );
    let scroll = resolved_scroll(
        chat,
        all_lines.len(),
        area.height as usize,
        &message_line_ranges,
    );

    scrollbar_layout_from_metrics(area, all_lines.len(), scroll)
}

pub(crate) fn rendered_line_count(
    area: Rect,
    chat: &ChatState,
    theme: &ThemeTokens,
    current_tick: u64,
    retry_wait_start_selected: bool,
) -> usize {
    if area.width == 0 || area.height == 0 {
        return 0;
    }

    let inner = content_inner(area);
    let (all_lines, _) = build_rendered_lines(
        chat,
        theme,
        inner.width as usize,
        current_tick,
        retry_wait_start_selected,
    );
    all_lines.len()
}

pub(crate) fn scrollbar_scroll_offset_for_pointer(
    area: Rect,
    chat: &ChatState,
    theme: &ThemeTokens,
    current_tick: u64,
    retry_wait_start_selected: bool,
    pointer_row: u16,
    grab_offset: u16,
) -> Option<usize> {
    let layout = scrollbar_layout(area, chat, theme, current_tick, retry_wait_start_selected)?;
    let track_span = layout.scrollbar.height.saturating_sub(layout.thumb.height);
    let clamped_row = pointer_row.clamp(
        layout.scrollbar.y,
        layout
            .scrollbar
            .y
            .saturating_add(layout.scrollbar.height)
            .saturating_sub(1),
    );
    let desired_top = clamped_row
        .saturating_sub(layout.scrollbar.y)
        .saturating_sub(grab_offset)
        .min(track_span);

    Some(scroll_offset_from_thumb_offset(
        desired_top,
        track_span,
        layout.max_scroll,
    ))
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
    current_tick: u64,
    retry_wait_start_selected: bool,
) -> Option<(Rect, Vec<RenderedChatLine>)> {
    if (chat.active_thread().is_none() && chat.streaming_content().is_empty())
        || area.width == 0
        || area.height == 0
    {
        return None;
    }

    let inner = content_inner(area);
    let (all_lines, message_line_ranges) = build_rendered_lines(
        chat,
        theme,
        inner.width as usize,
        current_tick,
        retry_wait_start_selected,
    );
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
    current_tick: u64,
    retry_wait_start_selected: bool,
) -> Option<SelectionSnapshot> {
    let inner = content_inner(area);
    if inner.width == 0 || inner.height == 0 {
        return None;
    }
    let key = render_cache_key(area, chat, current_tick, retry_wait_start_selected);

    let (all_lines, message_line_ranges) = build_rendered_lines(
        chat,
        theme,
        inner.width as usize,
        current_tick,
        retry_wait_start_selected,
    );
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
        key,
        inner,
        all_lines,
        message_line_ranges,
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

fn nearest_message_content_row(all_lines: &[RenderedChatLine], row: usize) -> Option<usize> {
    let current = all_lines.get(row)?;
    let message_index = current.message_index?;
    if !matches!(current.kind, RenderedLineKind::Padding) {
        return Some(row);
    }

    for distance in 1..all_lines.len() {
        if let Some(prev) = row.checked_sub(distance) {
            let line = &all_lines[prev];
            if line.message_index == Some(message_index)
                && !matches!(line.kind, RenderedLineKind::Padding)
            {
                return Some(prev);
            }
        }

        let next = row.saturating_add(distance);
        if let Some(line) = all_lines.get(next) {
            if line.message_index == Some(message_index)
                && !matches!(line.kind, RenderedLineKind::Padding)
            {
                return Some(next);
            }
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
    current_tick: u64,
    start: SelectionPoint,
    end: SelectionPoint,
) -> Option<String> {
    let inner = content_inner(area);
    let (all_lines, message_line_ranges) =
        build_rendered_lines(chat, theme, inner.width as usize, current_tick, false);
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
    current_tick: u64,
    mouse: Position,
) -> Option<SelectionPoint> {
    let snapshot = selection_snapshot(area, chat, theme, current_tick, false)?;
    selection_point_from_snapshot(&snapshot, mouse)
}

pub fn selection_points_from_mouse(
    area: Rect,
    chat: &ChatState,
    theme: &ThemeTokens,
    current_tick: u64,
    start: Position,
    end: Position,
    retry_wait_start_selected: bool,
) -> Option<(SelectionPoint, SelectionPoint)> {
    let snapshot = selection_snapshot(area, chat, theme, current_tick, retry_wait_start_selected)?;
    Some((
        selection_point_from_snapshot(&snapshot, start)?,
        selection_point_from_snapshot(&snapshot, end)?,
    ))
}
