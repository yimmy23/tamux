use crate::app::ChatFilePreviewTarget;
use crate::state::task::TaskState;
use crate::terminal_graphics::{active_protocol, TerminalImageOverlaySpec, TerminalImageProtocol};
use crate::theme::ThemeTokens;
use crate::widgets::image_preview;
use crate::widgets::message::{render_markdown_pub, wrap_text};
use ratatui::prelude::*;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

const SCROLLBAR_WIDTH: u16 = 1;
const FILE_PREVIEW_HEADER_LINES: u16 = 5;
const TERMINAL_IMAGE_HEADER_LINES: u16 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FilePreviewScrollbarLayout {
    pub(crate) content: Rect,
    pub(crate) scrollbar: Rect,
    pub(crate) thumb: Rect,
    pub(crate) scroll: usize,
    pub(crate) max_scroll: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilePreviewHitTarget {
    ClosePreview,
}

fn scroll_offset_from_thumb_offset(thumb_offset: u16, track_span: u16, max_scroll: usize) -> usize {
    if max_scroll == 0 || track_span == 0 {
        return 0;
    }

    (((thumb_offset as usize) * max_scroll) + (track_span as usize / 2)) / track_span as usize
}

fn scrollbar_layout_from_metrics(
    area: Rect,
    total_lines: usize,
    scroll: usize,
) -> Option<FilePreviewScrollbarLayout> {
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
        ((scroll * track_span as usize) + (max_scroll / 2)) / max_scroll
    } as u16;
    let thumb = Rect::new(
        scrollbar.x,
        scrollbar.y.saturating_add(thumb_offset),
        scrollbar.width,
        thumb_height,
    );

    Some(FilePreviewScrollbarLayout {
        content,
        scrollbar,
        thumb,
        scroll,
        max_scroll,
    })
}

fn is_markdown_path(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    lower.ends_with(".md") || lower.ends_with(".markdown") || lower.ends_with(".mdx")
}

fn push_wrapped(lines: &mut Vec<Line<'static>>, text: &str, style: Style, width: usize) {
    for line in wrap_text(text, width.max(1)) {
        lines.push(Line::from(Span::styled(line, style)));
    }
}

fn push_preview_content(
    lines: &mut Vec<Line<'static>>,
    path: &str,
    content: &str,
    width: usize,
    theme: &ThemeTokens,
) {
    if is_markdown_path(path) {
        lines.extend(render_markdown_pub(content, width.max(1)));
    } else {
        push_wrapped(lines, content, theme.fg_dim, width);
    }
}

fn build_lines(
    area: Rect,
    tasks: &TaskState,
    target: &ChatFilePreviewTarget,
    theme: &ThemeTokens,
    scroll: usize,
) -> Vec<Line<'static>> {
    let width = area.width as usize;
    let mut lines = vec![Line::from(vec![
        Span::styled("[x]", theme.accent_danger),
        Span::raw(" "),
        Span::styled("Close preview", theme.fg_dim),
    ])];
    lines.push(Line::raw(""));
    lines.push(Line::from(vec![
        Span::styled("Path: ", theme.fg_dim),
        Span::styled(target.path.clone(), theme.fg_active),
    ]));
    lines.push(Line::raw(""));
    lines.push(Line::from(Span::styled(
        "Preview",
        theme.accent_primary.add_modifier(Modifier::BOLD),
    )));
    let image_preview_height = area.height.saturating_sub(lines.len() as u16).max(1) as usize;
    let use_terminal_graphics = uses_terminal_graphics(target, scroll);

    if let Some(repo_root) = target.repo_root.as_deref() {
        let diff_key = target
            .repo_relative_path
            .as_deref()
            .unwrap_or(target.path.as_str());
        if let Some(diff) = tasks.diff_for_path(repo_root, diff_key) {
            if diff.trim().is_empty() {
                push_wrapped(
                    &mut lines,
                    "No diff preview available for this file.",
                    theme.fg_dim,
                    width,
                );
            } else {
                push_wrapped(&mut lines, diff, theme.fg_dim, width);
            }
            return lines;
        }
        if let Some(preview) = tasks.preview_for_path(&target.path) {
            if preview.is_text {
                push_preview_content(&mut lines, &target.path, &preview.content, width, theme);
            } else {
                push_wrapped(
                    &mut lines,
                    "Binary file preview is not available.",
                    theme.fg_dim,
                    width,
                );
            }
        } else {
            push_wrapped(&mut lines, "Loading diff...", theme.fg_dim, width);
        }
        return lines;
    }

    if let Some(preview) = tasks.preview_for_path(&target.path) {
        if preview.is_text {
            push_preview_content(&mut lines, &target.path, &preview.content, width, theme);
        } else if use_terminal_graphics {
            push_terminal_graphics_placeholder(&mut lines, image_preview_height, theme);
        } else if image_preview::is_previewable_image_path(&target.path) {
            lines.extend(image_preview::render_image_preview_lines(
                &target.path,
                width,
                image_preview_height,
                theme,
            ));
        } else {
            push_wrapped(
                &mut lines,
                "Binary file preview is not available.",
                theme.fg_dim,
                width,
            );
        }
    } else {
        if image_preview::is_previewable_image_path(&target.path) {
            if use_terminal_graphics {
                push_terminal_graphics_placeholder(&mut lines, image_preview_height, theme);
            } else {
                lines.extend(image_preview::render_image_preview_lines(
                    &target.path,
                    width,
                    image_preview_height,
                    theme,
                ));
            }
        } else {
            push_wrapped(&mut lines, "Loading preview...", theme.fg_dim, width);
        }
    }

    lines
}

fn uses_terminal_graphics(target: &ChatFilePreviewTarget, scroll: usize) -> bool {
    scroll == 0
        && target.repo_root.is_none()
        && active_protocol() != TerminalImageProtocol::None
        && image_preview::resolve_local_image_path(&target.path)
            .as_deref()
            .is_some_and(image_preview::is_previewable_image_path)
}

fn push_terminal_graphics_placeholder(
    lines: &mut Vec<Line<'static>>,
    image_preview_height: usize,
    theme: &ThemeTokens,
) {
    lines.push(Line::from(vec![
        Span::styled("Image: ", theme.fg_dim),
        Span::styled("high-quality terminal preview", theme.fg_active),
    ]));
    for _ in 1..image_preview_height {
        lines.push(Line::raw(""));
    }
}

pub fn terminal_image_overlay_spec(
    area: Rect,
    tasks: &TaskState,
    target: &ChatFilePreviewTarget,
    theme: &ThemeTokens,
    scroll: usize,
) -> Option<TerminalImageOverlaySpec> {
    if !uses_terminal_graphics(target, scroll) {
        return None;
    }

    let content = scrollbar_layout(area, tasks, target, theme, scroll)
        .map(|layout| layout.content)
        .unwrap_or(area);
    let path = image_preview::resolve_local_image_path(&target.path)?;
    let image_row = content
        .y
        .saturating_add(FILE_PREVIEW_HEADER_LINES)
        .saturating_add(TERMINAL_IMAGE_HEADER_LINES);
    let image_rows = content
        .height
        .saturating_sub(FILE_PREVIEW_HEADER_LINES)
        .saturating_sub(TERMINAL_IMAGE_HEADER_LINES);
    if content.width == 0 || image_rows == 0 {
        return None;
    }

    Some(TerminalImageOverlaySpec {
        path,
        column: content.x,
        row: image_row,
        cols: content.width,
        rows: image_rows,
    })
}

pub fn hit_test(
    area: Rect,
    tasks: &TaskState,
    target: &ChatFilePreviewTarget,
    scroll: usize,
    mouse: Position,
    theme: &ThemeTokens,
) -> Option<FilePreviewHitTarget> {
    let layout = scrollbar_layout(area, tasks, target, theme, scroll);
    let content = layout.map(|layout| layout.content).unwrap_or(area);
    if !content.contains(mouse) {
        return None;
    }
    if layout
        .map(|layout| layout.scroll)
        .unwrap_or(scroll)
        .saturating_add(mouse.y.saturating_sub(content.y) as usize)
        == 0
    {
        Some(FilePreviewHitTarget::ClosePreview)
    } else {
        None
    }
}

pub(crate) fn scrollbar_scroll_offset_for_pointer(
    area: Rect,
    tasks: &TaskState,
    target: &ChatFilePreviewTarget,
    theme: &ThemeTokens,
    scroll: usize,
    pointer_row: u16,
    grab_offset: u16,
) -> Option<usize> {
    let layout = scrollbar_layout(area, tasks, target, theme, scroll)?;
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

pub fn render(
    frame: &mut Frame,
    area: Rect,
    tasks: &TaskState,
    target: &ChatFilePreviewTarget,
    theme: &ThemeTokens,
    scroll: usize,
) {
    if area.width == 0 || area.height == 0 {
        return;
    }

    let layout = scrollbar_layout(area, tasks, target, theme, scroll);
    let content = layout.map(|layout| layout.content).unwrap_or(area);
    let resolved_scroll = layout.map(|layout| layout.scroll).unwrap_or(scroll);
    let visible = build_lines(content, tasks, target, theme, resolved_scroll)
        .into_iter()
        .skip(resolved_scroll)
        .take(content.height as usize)
        .collect::<Vec<_>>();

    if let Some(layout) = layout {
        frame.render_widget(Paragraph::new(visible), layout.content);

        let scrollbar_lines = (0..layout.scrollbar.height)
            .map(|offset| {
                let y = layout.scrollbar.y.saturating_add(offset);
                let (glyph, style) = if y >= layout.thumb.y
                    && y < layout.thumb.y.saturating_add(layout.thumb.height)
                {
                    ("█", theme.accent_primary)
                } else {
                    ("│", theme.fg_dim)
                };
                Line::from(Span::styled(glyph, style))
            })
            .collect::<Vec<_>>();
        frame.render_widget(Paragraph::new(scrollbar_lines), layout.scrollbar);
    } else {
        frame.render_widget(Paragraph::new(visible), content);
    }
}

pub fn max_scroll(
    area: Rect,
    tasks: &TaskState,
    target: &ChatFilePreviewTarget,
    theme: &ThemeTokens,
) -> usize {
    if area.width == 0 || area.height == 0 {
        return 0;
    }

    scrollbar_layout(area, tasks, target, theme, 0)
        .map(|layout| layout.max_scroll)
        .unwrap_or_else(|| {
            build_lines(area, tasks, target, theme, 0)
                .len()
                .saturating_sub(area.height as usize)
        })
}

pub(crate) fn scrollbar_layout(
    area: Rect,
    tasks: &TaskState,
    target: &ChatFilePreviewTarget,
    theme: &ThemeTokens,
    scroll: usize,
) -> Option<FilePreviewScrollbarLayout> {
    if area.width <= SCROLLBAR_WIDTH || area.height == 0 {
        return None;
    }

    let full_lines = build_lines(area, tasks, target, theme, scroll);
    if full_lines.len() <= area.height as usize {
        return None;
    }

    let content = Rect::new(
        area.x,
        area.y,
        area.width.saturating_sub(SCROLLBAR_WIDTH),
        area.height,
    );
    let all_lines = build_lines(content, tasks, target, theme, scroll);
    scrollbar_layout_from_metrics(area, all_lines.len(), scroll)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminal_image_overlay_spec_targets_file_preview_body() {
        crate::terminal_graphics::set_active_protocol_for_tests(TerminalImageProtocol::Kitty);

        let target = ChatFilePreviewTarget {
            path: "/tmp/demo.png".to_string(),
            repo_root: None,
            repo_relative_path: None,
        };
        let spec = terminal_image_overlay_spec(
            Rect::new(0, 0, 80, 30),
            &TaskState::default(),
            &target,
            &ThemeTokens::default(),
            0,
        )
        .expect("expected file preview image overlay spec");

        assert_eq!(spec.column, 0);
        assert_eq!(spec.row, 6);
        assert_eq!(spec.cols, 80);
        assert_eq!(spec.rows, 24);

        crate::terminal_graphics::set_active_protocol_for_tests(TerminalImageProtocol::None);
    }
}
