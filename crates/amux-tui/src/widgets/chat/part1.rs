fn render_streaming_markdown(content: &str, width: usize) -> Vec<Line<'static>> {
    super::message::render_markdown_pub(content, width)
}

use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ToolFileChip {
    pub path: String,
    pub label: String,
    pub tool_name: String,
}

pub(crate) fn tool_file_chip(message: &AgentMessage) -> Option<ToolFileChip> {
    let tool_name = message.tool_name.as_deref()?;
    if let Some(path) = message
        .tool_output_preview_path
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        let label = file_name_label(path);
        return Some(ToolFileChip {
            path: path.to_string(),
            label,
            tool_name: tool_name.to_string(),
        });
    }
    if tool_name == "generate_image" {
        let path = generated_image_preview_path(message)?;
        let label = file_name_label(&path);
        return Some(ToolFileChip {
            path,
            label,
            tool_name: tool_name.to_string(),
        });
    }
    if tool_name == "read_skill" {
        let path = read_skill_preview_path(message)?;
        let label = file_name_label(&path);
        return Some(ToolFileChip {
            path,
            label,
            tool_name: tool_name.to_string(),
        });
    }

    if !matches!(
        tool_name,
        "read_file"
            | "write_file"
            | "create_file"
            | "append_to_file"
            | "replace_in_file"
            | "apply_file_patch"
            | "apply_patch"
    ) {
        return None;
    }

    let arguments = message.tool_arguments.as_deref()?;
    let value: serde_json::Value = serde_json::from_str(arguments).ok()?;
    let path = direct_tool_path(&value)
        .or_else(|| tool_path_from_cwd_and_filename(&value))
        .or_else(|| {
            if tool_name == "apply_patch" {
                first_apply_patch_path(arguments)
            } else {
                None
            }
        })?;
    let label = file_name_label(&path);

    Some(ToolFileChip {
        path,
        label,
        tool_name: tool_name.to_string(),
    })
}

pub(crate) fn message_image_preview_path(message: &AgentMessage) -> Option<String> {
    message.content_blocks.iter().find_map(|block| match block {
        crate::state::chat::AgentContentBlock::Image { url, data_url, .. } => url
            .as_deref()
            .and_then(crate::widgets::image_preview::resolve_local_image_path)
            .or_else(|| {
                data_url
                    .as_deref()
                    .and_then(crate::widgets::image_preview::resolve_local_image_path)
            }),
        _ => None,
    })
}

fn generated_image_preview_path(message: &AgentMessage) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(&message.content).ok()?;
    non_empty_string_field(&value, "path").or_else(|| {
        non_empty_string_field(&value, "file_url")
            .and_then(|value| crate::widgets::image_preview::resolve_local_image_path(&value))
    })
}

pub(crate) fn tool_skill_chip(message: &AgentMessage) -> Option<String> {
    let tool_name = message.tool_name.as_deref()?;
    if tool_name != "read_skill" {
        return None;
    }

    let arguments = message.tool_arguments.as_deref()?;
    let value: serde_json::Value = serde_json::from_str(arguments).ok()?;
    non_empty_string_field(&value, "skill")
}

fn direct_tool_path(value: &serde_json::Value) -> Option<String> {
    ["path", "filePath", "file_path"]
        .iter()
        .find_map(|key| non_empty_string_field(value, key))
}

fn tool_path_from_cwd_and_filename(value: &serde_json::Value) -> Option<String> {
    let filename = non_empty_string_field(value, "filename")?;
    if let Some(cwd) = non_empty_string_field(value, "cwd") {
        Some(Path::new(&cwd).join(&filename).display().to_string())
    } else {
        Some(filename)
    }
}

fn non_empty_string_field(value: &serde_json::Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn read_skill_preview_path(message: &AgentMessage) -> Option<String> {
    skill_path_from_result_json(&message.content)
        .or_else(|| skill_path_from_result_header(&message.content))
}

fn skill_path_from_result_json(content: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(content).ok()?;
    let path = non_empty_string_field(&value, "path")?;
    Some(resolve_skill_path_for_preview(
        non_empty_string_field(&value, "skills_root"),
        &path,
    ))
}

fn skill_path_from_result_header(content: &str) -> Option<String> {
    let first_line = content.lines().next()?.trim();
    let raw_path = first_line.strip_prefix("Skill ")?;
    let relative_path = if let Some((path, _)) = raw_path.split_once(" [") {
        path.trim()
    } else {
        raw_path.strip_suffix(':').unwrap_or(raw_path).trim()
    };
    if relative_path.is_empty() {
        return None;
    }

    Some(resolve_skill_path_for_preview(None, relative_path))
}

fn resolve_skill_path_for_preview(skills_root: Option<String>, path: &str) -> String {
    if Path::new(path).is_absolute() {
        return path.to_string();
    }

    skills_root
        .map(std::path::PathBuf::from)
        .unwrap_or_else(amux_protocol::tamux_skills_dir)
        .join(path)
        .display()
        .to_string()
}

fn file_name_label(path: &str) -> String {
    Path::new(path)
        .file_name()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .unwrap_or(path)
        .to_string()
}

pub(crate) fn append_tool_file_chip(
    line: &mut Line<'static>,
    message: &AgentMessage,
    theme: &ThemeTokens,
) {
    let Some(chip) = tool_file_chip(message) else {
        return;
    };

    line.spans.push(Span::raw(" "));
    line.spans.push(Span::styled(
        format!("[{}]", chip.label),
        theme.accent_primary,
    ));
}

pub(crate) fn append_tool_skill_chip(
    line: &mut Line<'static>,
    message: &AgentMessage,
    theme: &ThemeTokens,
) {
    if tool_file_chip(message).is_some() {
        return;
    }

    let Some(skill_name) = tool_skill_chip(message) else {
        return;
    };

    line.spans.push(Span::raw(" "));
    line.spans.push(Span::styled(
        format!("[{skill_name}]"),
        theme.accent_primary,
    ));
}

fn first_apply_patch_path(arguments: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(arguments).ok()?;
    let input = value
        .get("input")
        .or_else(|| value.get("patch"))
        .and_then(|raw| raw.as_str())?;
    for line in input.lines() {
        if let Some(path) = line
            .strip_prefix("*** Update File: ")
            .or_else(|| line.strip_prefix("*** Add File: "))
            .or_else(|| line.strip_prefix("*** Delete File: "))
        {
            let path = path.split(" -> ").next().unwrap_or(path).trim();
            if !path.is_empty() {
                return Some(path.to_string());
            }
        }
    }
    None
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SelectionPoint {
    pub row: usize,
    pub col: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RenderedLineKind {
    MessageBody,
    ImageAttachment,
    ReasoningToggle,
    ReasoningContent,
    ToolToggle,
    ToolDetail,
    ActionBar,
    Padding,
    Streaming,
    RetryStatus,
    RetryAction,
}

#[derive(Debug, Clone)]
struct RenderedChatLine {
    line: Line<'static>,
    message_index: Option<usize>,
    kind: RenderedLineKind,
}

struct SelectionSnapshot {
    key: RenderCacheKey,
    inner: Rect,
    all_lines: Vec<RenderedChatLine>,
    message_line_ranges: Vec<(usize, usize)>,
    start_idx: usize,
    end_idx: usize,
    padding: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ChatScrollbarLayout {
    pub(crate) content: Rect,
    pub(crate) scrollbar: Rect,
    pub(crate) thumb: Rect,
    pub(crate) scroll: usize,
    pub(crate) max_scroll: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct RenderCacheKey {
    inner: Rect,
    render_revision: u64,
    render_epoch: u64,
    retry_wait_start_selected: bool,
}

fn render_cache_key(
    area: Rect,
    chat: &ChatState,
    current_tick: u64,
    retry_wait_start_selected: bool,
) -> RenderCacheKey {
    let inner = content_inner(area);
    RenderCacheKey {
        inner,
        render_revision: chat.render_revision(),
        render_epoch: chat.render_cache_epoch(current_tick),
        retry_wait_start_selected,
    }
}

impl RenderedChatLine {
    fn padding() -> Self {
        Self {
            line: Line::raw(""),
            message_index: None,
            kind: RenderedLineKind::Padding,
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
            style.patch(span.style),
        ));
    }

    let content_width = line_display_width(&Line::from(spans.clone()));
    let right_width = width.saturating_sub(content_width).max(MESSAGE_PADDING_X);
    spans.push(Span::styled(" ".repeat(right_width), style));

    Line::from(spans).style(style.patch(line.style))
}

fn message_block_style(msg: &AgentMessage, theme: &ThemeTokens) -> Style {
    match msg.role {
        MessageRole::User => theme.fg_active.bg(Color::Indexed(236)),
        _ => Style::default(),
    }
}

pub(crate) fn message_action_targets(
    chat: &ChatState,
    msg_index: usize,
    msg: &AgentMessage,
    current_tick: u64,
) -> Vec<(String, ChatHitTarget)> {
    if !msg.actions.is_empty() {
        return msg
            .actions
            .iter()
            .enumerate()
            .map(|(action_index, action)| {
                (
                    format!("[{}]", action.label),
                    ChatHitTarget::MessageAction {
                        message_index: msg_index,
                        action_index,
                    },
                )
            })
            .collect();
    }

    let copy_label = if chat.is_message_recently_copied(msg_index, current_tick) {
        "[Copied]".to_string()
    } else {
        "[Copy]".to_string()
    };
    let mut actions = Vec::new();

    if msg.role == MessageRole::Assistant
        && msg
            .reasoning
            .as_deref()
            .is_some_and(|reasoning| !reasoning.is_empty())
        && matches!(chat.transcript_mode(), TranscriptMode::Compact)
    {
        let toggle_label = if chat.expanded_reasoning().contains(&msg_index) {
            "[Collapse]"
        } else {
            "[Expand]"
        };
        actions.push((
            toggle_label.to_string(),
            ChatHitTarget::ReasoningToggle(msg_index),
        ));
    }

    if msg.role == MessageRole::Tool
        && msg.tool_name.is_some()
        && matches!(chat.transcript_mode(), TranscriptMode::Compact)
    {
        let toggle_label = if chat.expanded_tools().contains(&msg_index) {
            "[Collapse]"
        } else {
            "[Expand]"
        };
        actions.push((
            toggle_label.to_string(),
            ChatHitTarget::ToolToggle(msg_index),
        ));
    }

    actions.push((copy_label, ChatHitTarget::CopyMessage(msg_index)));
    match msg.role {
        MessageRole::User => {
            actions.push((
                "[Resend]".to_string(),
                ChatHitTarget::ResendMessage(msg_index),
            ));
            actions.push((
                if msg.pinned_for_compaction {
                    "[Unpin]"
                } else {
                    "[Pin]"
                }
                .to_string(),
                if msg.pinned_for_compaction {
                    ChatHitTarget::UnpinMessage(msg_index)
                } else {
                    ChatHitTarget::PinMessage(msg_index)
                },
            ));
        }
        MessageRole::Assistant => {
            actions.push((
                "[Regenerate]".to_string(),
                ChatHitTarget::RegenerateMessage(msg_index),
            ));
            actions.push((
                if msg.pinned_for_compaction {
                    "[Unpin]"
                } else {
                    "[Pin]"
                }
                .to_string(),
                if msg.pinned_for_compaction {
                    ChatHitTarget::UnpinMessage(msg_index)
                } else {
                    ChatHitTarget::PinMessage(msg_index)
                },
            ));
        }
        _ => {}
    }
    actions.push((
        "[Delete]".to_string(),
        ChatHitTarget::DeleteMessage(msg_index),
    ));
    actions
}

fn message_action_line(
    chat: &ChatState,
    msg_index: usize,
    msg: &AgentMessage,
    selected_action: usize,
    theme: &ThemeTokens,
    current_tick: u64,
) -> Option<Line<'static>> {
    let actions = message_action_targets(chat, msg_index, msg, current_tick);
    if actions.is_empty() {
        return None;
    }

    let mut spans = Vec::new();
    for (idx, (label, _)) in actions.into_iter().enumerate() {
        let style = if idx == selected_action {
            theme.accent_primary
        } else {
            theme.fg_dim
        };
        spans.push(Span::styled(format!(" {label} "), style));
    }

    Some(Line::from(spans))
}

fn action_hit_target(
    chat: &ChatState,
    msg_index: usize,
    msg: &AgentMessage,
    content_col: usize,
    current_tick: u64,
) -> Option<ChatHitTarget> {
    let actions = message_action_targets(chat, msg_index, msg, current_tick);
    let mut col = 0usize;
    for (label, target) in actions {
        let width = UnicodeWidthStr::width(label.as_str()).saturating_add(2);
        if content_col >= col && content_col < col.saturating_add(width) {
            return Some(target);
        }
        col = col.saturating_add(width);
    }
    None
}

fn retry_wait_remaining_secs(status: &crate::state::chat::RetryStatusVm, current_tick: u64) -> u64 {
    let elapsed_ticks = current_tick.saturating_sub(status.received_at_tick);
    let elapsed_ms = elapsed_ticks.saturating_mul(crate::app::TUI_TICK_RATE_MS);
    status
        .delay_ms
        .saturating_sub(elapsed_ms)
        .div_ceil(1_000)
        .max(1)
}

fn retry_action_line(
    status: &crate::state::chat::RetryStatusVm,
    theme: &ThemeTokens,
    current_tick: u64,
    retry_wait_start_selected: bool,
) -> Line<'static> {
    match status.phase {
        RetryPhase::Retrying => Line::from(vec![Span::styled("[Stop]", theme.accent_primary)]),
        RetryPhase::Waiting => {
            let yes_label = format!("[Yes {}s]", retry_wait_remaining_secs(status, current_tick));
            let yes_style = if retry_wait_start_selected {
                theme.accent_primary
            } else {
                theme.fg_dim
            };
            let no_style = if retry_wait_start_selected {
                theme.fg_dim
            } else {
                theme.accent_primary
            };
            Line::from(vec![
                Span::styled(yes_label, yes_style),
                Span::raw(" "),
                Span::styled("[No]", no_style),
            ])
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
    let content_width = padded_content_width(width);
    let image_line_count = message_image_preview_path(msg)
        .map(|path| {
            crate::widgets::image_preview::render_image_preview_lines(&path, content_width, 12, &ThemeTokens::default())
                .len()
        })
        .unwrap_or(0);

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

            if msg.content.is_empty() && image_line_count == 0 && msg.role != MessageRole::Assistant {
                return Vec::new();
            }
            if msg.content.is_empty() && image_line_count == 0 && msg.reasoning.is_none() {
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
                kinds.extend(std::iter::repeat_n(
                    RenderedLineKind::ImageAttachment,
                    image_line_count,
                ));
                kinds
            } else {
                let mut kinds = vec![RenderedLineKind::MessageBody; content_lines.max(1)];
                kinds.extend(std::iter::repeat_n(
                    RenderedLineKind::ImageAttachment,
                    image_line_count,
                ));
                kinds
            }
        }
    }
}
