use crate::app::ChatFilePreviewTarget;
use crate::state::task::TaskState;
use crate::theme::ThemeTokens;
use crate::widgets::message::{render_markdown_pub, wrap_text};
use ratatui::prelude::*;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

const SCROLLBAR_WIDTH: u16 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FilePreviewScrollbarLayout {
    content: Rect,
    scrollbar: Rect,
    thumb: Rect,
    scroll: usize,
    max_scroll: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilePreviewHitTarget {
    ClosePreview,
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
        } else {
            push_wrapped(
                &mut lines,
                "Binary file preview is not available.",
                theme.fg_dim,
                width,
            );
        }
    } else {
        push_wrapped(&mut lines, "Loading preview...", theme.fg_dim, width);
    }

    lines
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
    let visible = build_lines(content, tasks, target, theme)
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
            build_lines(area, tasks, target, theme)
                .len()
                .saturating_sub(area.height as usize)
        })
}

fn scrollbar_layout(
    area: Rect,
    tasks: &TaskState,
    target: &ChatFilePreviewTarget,
    theme: &ThemeTokens,
    scroll: usize,
) -> Option<FilePreviewScrollbarLayout> {
    if area.width <= SCROLLBAR_WIDTH || area.height == 0 {
        return None;
    }

    let full_lines = build_lines(area, tasks, target, theme);
    if full_lines.len() <= area.height as usize {
        return None;
    }

    let content = Rect::new(
        area.x,
        area.y,
        area.width.saturating_sub(SCROLLBAR_WIDTH),
        area.height,
    );
    let all_lines = build_lines(content, tasks, target, theme);
    scrollbar_layout_from_metrics(area, all_lines.len(), scroll)
}
