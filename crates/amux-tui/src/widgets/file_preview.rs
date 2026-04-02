use crate::app::ChatFilePreviewTarget;
use crate::state::task::TaskState;
use crate::theme::ThemeTokens;
use crate::widgets::message::{render_markdown_pub, wrap_text};
use ratatui::prelude::*;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilePreviewHitTarget {
    ClosePreview,
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
    _tasks: &TaskState,
    _target: &ChatFilePreviewTarget,
    mouse: Position,
    _theme: &ThemeTokens,
) -> Option<FilePreviewHitTarget> {
    if !area.contains(mouse) {
        return None;
    }
    if mouse.y == area.y {
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

    let visible = build_lines(area, tasks, target, theme)
        .into_iter()
        .skip(scroll)
        .take(area.height as usize)
        .collect::<Vec<_>>();

    frame.render_widget(Paragraph::new(visible), area);
}
