fn build_rendered_lines(
    chat: &ChatState,
    theme: &ThemeTokens,
    inner_width: usize,
    current_tick: u64,
    retry_wait_start_selected: bool,
) -> (Vec<RenderedChatLine>, Vec<(usize, usize)>) {
    #[cfg(test)]
    BUILD_RENDERED_LINES_CALLS.with(|calls| calls.set(calls.get() + 1));

    let mut all_lines = Vec::new();
    let mut message_line_ranges = Vec::new();
    let mode = chat.transcript_mode();
    let expanded = chat.expanded_reasoning();
    let expanded_tools = chat.expanded_tools();
    let content_width = padded_content_width(inner_width);

    if let Some(thread) = chat.active_thread() {
        let responder_labels = assistant_responder_labels(thread);
        for (idx, msg) in thread.messages.iter().enumerate() {
            let start = all_lines.len();
            let mut msg_lines = super::message::message_to_lines(
                msg,
                idx,
                mode,
                theme,
                content_width,
                expanded,
                expanded_tools,
            );
            if msg.role == MessageRole::Assistant {
                if let Some(label) = responder_labels.get(idx).and_then(|value| value.as_deref()) {
                    msg_lines.push(Line::from(vec![Span::styled(
                        format!("Responder: {label}"),
                        theme.fg_dim,
                    )]));
                }
            }
            if let Some(first_line) = msg_lines.first_mut() {
                append_tool_file_chip(first_line, msg, theme);
            }
            let mut kinds =
                classify_message_lines(msg, idx, mode, inner_width, expanded, expanded_tools);

            if kinds.len() < msg_lines.len() {
                kinds.resize(msg_lines.len(), RenderedLineKind::MessageBody);
            } else if kinds.len() > msg_lines.len() {
                kinds.truncate(msg_lines.len());
            }

            let block_style = message_block_style(msg, theme);
            let render_compaction_artifact = msg.message_kind == "compaction_artifact";
            if render_compaction_artifact {
                for (line, kind) in msg_lines.into_iter().zip(kinds.into_iter()) {
                    all_lines.push(RenderedChatLine {
                        line,
                        message_index: Some(idx),
                        kind,
                    });
                }
                let end = all_lines.len();
                message_line_ranges.push((start, end));
                continue;
            }
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

            let is_last_actionable = !msg.actions.is_empty()
                && chat.active_actions().first().map(|a| &a.label)
                    == msg.actions.first().map(|a| &a.label);
            let inline_action_line = if chat.selected_message() == Some(idx) && !is_last_actionable
            {
                message_action_line(
                    chat,
                    idx,
                    msg,
                    chat.selected_message_action(),
                    theme,
                    current_tick,
                )
            } else {
                None
            };

            if let Some(action_line) = inline_action_line {
                all_lines.push(RenderedChatLine {
                    line: pad_message_line(action_line, inner_width, block_style),
                    message_index: Some(idx),
                    kind: RenderedLineKind::ActionBar,
                });
                all_lines.push(RenderedChatLine {
                    line: blank_message_line(inner_width, block_style),
                    message_index: Some(idx),
                    kind: RenderedLineKind::Padding,
                });
            } else {
                for _ in 0..MESSAGE_PADDING_Y {
                    all_lines.push(RenderedChatLine {
                        line: blank_message_line(inner_width, block_style),
                        message_index: Some(idx),
                        kind: RenderedLineKind::Padding,
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

    if let Some(status) = chat.retry_status() {
        let summary = match status.phase {
            RetryPhase::Retrying => format!(
                "[tamux] retry {}/{} in {}s · {}",
                status.attempt,
                if status.max_retries == 0 {
                    "∞".to_string()
                } else {
                    status.max_retries.to_string()
                },
                status.delay_ms.div_ceil(1000).max(1),
                status.failure_class.replace('_', " ")
            ),
            RetryPhase::Waiting => format!(
                "[tamux] retrying automatically in {}s · {}",
                status.delay_ms.div_ceil(1000).max(1),
                status.failure_class.replace('_', " ")
            ),
        };
        all_lines.push(RenderedChatLine {
            line: blank_message_line(inner_width, assistant_style),
            message_index: None,
            kind: RenderedLineKind::RetryStatus,
        });
        all_lines.push(RenderedChatLine {
            line: pad_message_line(
                Line::from(vec![Span::styled(summary, theme.fg_dim)]),
                inner_width,
                assistant_style,
            ),
            message_index: None,
            kind: RenderedLineKind::RetryStatus,
        });
        all_lines.push(RenderedChatLine {
            line: pad_message_line(
                Line::from(vec![Span::raw(status.message.clone())]),
                inner_width,
                assistant_style,
            ),
            message_index: None,
            kind: RenderedLineKind::RetryStatus,
        });
        all_lines.push(RenderedChatLine {
            line: pad_message_line(
                retry_action_line(status, theme, current_tick, retry_wait_start_selected),
                inner_width,
                assistant_style,
            ),
            message_index: None,
            kind: RenderedLineKind::RetryAction,
        });
    }

    (all_lines, message_line_ranges)
}

const THREAD_HANDOFF_SYSTEM_MARKER: &str = "[[handoff_event]]";

#[derive(serde::Deserialize)]
struct HandoffResponderEvent {
    #[serde(default)]
    from_agent_name: Option<String>,
    #[serde(default)]
    to_agent_name: Option<String>,
}

fn assistant_responder_labels(
    thread: &crate::state::chat::AgentThread,
) -> Vec<Option<String>> {
    let mut labels = vec![None; thread.messages.len()];
    let mut responder = initial_responder_name(thread);

    for (idx, msg) in thread.messages.iter().enumerate() {
        if msg.role == MessageRole::Assistant {
            labels[idx] = responder.clone();
        }
        if let Some(event) = parse_handoff_responder_event(&msg.content) {
            if event.to_agent_name.is_some() {
                responder = event.to_agent_name;
            }
        }
    }

    labels
}

fn initial_responder_name(thread: &crate::state::chat::AgentThread) -> Option<String> {
    thread
        .messages
        .iter()
        .find_map(|msg| parse_handoff_responder_event(&msg.content).and_then(|event| event.from_agent_name))
    .or_else(|| thread.agent_name.clone())
        .or_else(|| Some(amux_protocol::AGENT_NAME_SWAROG.to_string()))
}

fn parse_handoff_responder_event(content: &str) -> Option<HandoffResponderEvent> {
    let payload = content.strip_prefix(THREAD_HANDOFF_SYSTEM_MARKER)?;
    let json = payload.lines().next()?.trim();
    serde_json::from_str(json).ok()
}

pub struct CachedSelectionSnapshot(SelectionSnapshot);

pub fn build_selection_snapshot(
    area: Rect,
    chat: &ChatState,
    theme: &ThemeTokens,
    current_tick: u64,
    retry_wait_start_selected: bool,
) -> Option<CachedSelectionSnapshot> {
    selection_snapshot(area, chat, theme, current_tick, retry_wait_start_selected)
        .map(CachedSelectionSnapshot)
}

pub fn cached_snapshot_matches_area(snapshot: &CachedSelectionSnapshot, area: Rect) -> bool {
    snapshot.0.inner == content_inner(area)
}

pub fn selection_point_from_cached_snapshot(
    snapshot: &CachedSelectionSnapshot,
    mouse: Position,
) -> Option<SelectionPoint> {
    selection_point_from_snapshot(&snapshot.0, mouse)
}

pub fn selected_text_from_cached_snapshot(
    snapshot: &CachedSelectionSnapshot,
    start: SelectionPoint,
    end: SelectionPoint,
) -> Option<String> {
    let all_lines = &snapshot.0.all_lines;
    if all_lines.is_empty() {
        return None;
    }

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

fn apply_selected_message_highlight(
    all_lines: &mut [RenderedChatLine],
    selected_msg: Option<usize>,
) {
    let Some(sel_idx) = selected_msg else {
        return;
    };
    let sel_style = Style::default().bg(Color::Indexed(238));
    for rendered in all_lines
        .iter_mut()
        .filter(|line| line.message_index == Some(sel_idx))
    {
        rendered.line.style = rendered.line.style.patch(sel_style);
        for span in &mut rendered.line.spans {
            span.style = span.style.patch(sel_style);
        }
    }
}

fn build_visible_window_from_snapshot(
    snapshot: &SelectionSnapshot,
    all_lines: &[RenderedChatLine],
) -> Vec<RenderedChatLine> {
    let mut visible = Vec::with_capacity(snapshot.inner.height as usize);
    for _ in 0..snapshot.padding {
        visible.push(RenderedChatLine::padding());
    }
    visible.extend_from_slice(&all_lines[snapshot.start_idx..snapshot.end_idx]);
    visible
}

fn apply_mouse_selection_highlight(
    snapshot: &SelectionSnapshot,
    visible_lines: &mut [RenderedChatLine],
    mouse_selection: Option<(SelectionPoint, SelectionPoint)>,
) {
    let Some((start, end)) = mouse_selection else {
        return;
    };
    let (start_point, end_point) =
        if start.row <= end.row || (start.row == end.row && start.col <= end.col) {
            (start, end)
        } else {
            (end, start)
        };
    let highlight = Style::default().bg(Color::Indexed(31));
    let visible_last = snapshot.end_idx.saturating_sub(1);
    let range_start = start_point.row.max(snapshot.start_idx);
    let range_end = end_point.row.min(visible_last);

    if range_start > range_end {
        return;
    }

    for absolute_row in range_start..=range_end {
        let visible_row = snapshot.padding + absolute_row.saturating_sub(snapshot.start_idx);
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

fn render_snapshot(
    frame: &mut Frame,
    snapshot: &SelectionSnapshot,
    chat: &ChatState,
    mouse_selection: Option<(SelectionPoint, SelectionPoint)>,
) {
    let mut all_lines = snapshot.all_lines.clone();
    apply_selected_message_highlight(&mut all_lines, chat.selected_message());
    let mut visible_lines = build_visible_window_from_snapshot(snapshot, &all_lines);
    apply_mouse_selection_highlight(snapshot, &mut visible_lines, mouse_selection);

    let visible_lines = visible_lines
        .into_iter()
        .map(|line| line.line)
        .collect::<Vec<_>>();
    frame.render_widget(Paragraph::new(visible_lines), snapshot.inner);
}

pub fn render_cached(
    frame: &mut Frame,
    _area: Rect,
    chat: &ChatState,
    snapshot: &CachedSelectionSnapshot,
    mouse_selection: Option<(SelectionPoint, SelectionPoint)>,
) {
    render_snapshot(frame, &snapshot.0, chat, mouse_selection);
}

#[cfg(test)]
pub fn reset_build_rendered_lines_call_count() {
    BUILD_RENDERED_LINES_CALLS.with(|calls| calls.set(0));
}

#[cfg(test)]
pub fn build_rendered_lines_call_count() -> usize {
    BUILD_RENDERED_LINES_CALLS.with(std::cell::Cell::get)
}
