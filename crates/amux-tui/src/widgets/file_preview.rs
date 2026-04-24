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
use std::sync::{Arc, Mutex, OnceLock};
use unicode_width::UnicodeWidthChar;

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

struct SelectionSnapshot {
    lines: Arc<Vec<Line<'static>>>,
    scroll: usize,
    area: Rect,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct FilePreviewThemeKey {
    fg_dim: Style,
    fg_active: Style,
    accent_primary: Style,
    accent_danger: Style,
}

impl From<&ThemeTokens> for FilePreviewThemeKey {
    fn from(theme: &ThemeTokens) -> Self {
        Self {
            fg_dim: theme.fg_dim,
            fg_active: theme.fg_active,
            accent_primary: theme.accent_primary,
            accent_danger: theme.accent_danger,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct FilePreviewCacheKey {
    area: Rect,
    path: String,
    repo_root: Option<String>,
    repo_relative_path: Option<String>,
    task_state_id: usize,
    preview_revision: u64,
    image_preview_revision: u64,
    terminal_graphics: bool,
    theme: FilePreviewThemeKey,
}

#[derive(Clone)]
struct CachedFilePreviewLines {
    key: FilePreviewCacheKey,
    lines: Arc<Vec<Line<'static>>>,
    content_area: Rect,
    max_scroll: usize,
}

#[derive(Default)]
struct FilePreviewRenderCache {
    lines: Option<CachedFilePreviewLines>,
}

struct FilePreviewSnapshot {
    lines: Arc<Vec<Line<'static>>>,
    scroll: usize,
    area: Rect,
    layout: Option<FilePreviewScrollbarLayout>,
    max_scroll: usize,
}

fn global_file_preview_cache() -> &'static Mutex<FilePreviewRenderCache> {
    static CACHE: OnceLock<Mutex<FilePreviewRenderCache>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(FilePreviewRenderCache::default()))
}

fn lock_file_preview_cache() -> std::sync::MutexGuard<'static, FilePreviewRenderCache> {
    global_file_preview_cache()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[cfg(test)]
static BUILD_LINES_CALL_COUNT: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);

#[cfg(test)]
fn reset_build_lines_call_count_for_tests() {
    BUILD_LINES_CALL_COUNT.store(0, std::sync::atomic::Ordering::SeqCst);
}

#[cfg(test)]
fn build_lines_call_count_for_tests() -> usize {
    BUILD_LINES_CALL_COUNT.load(std::sync::atomic::Ordering::SeqCst)
}

fn line_plain_text(line: &Line<'static>) -> String {
    line.spans
        .iter()
        .map(|span| span.content.as_ref())
        .collect()
}

fn line_display_width(line: &Line<'static>) -> usize {
    line_plain_text(line)
        .chars()
        .map(|ch| UnicodeWidthChar::width(ch).unwrap_or(0))
        .sum()
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

fn build_lines(
    area: Rect,
    tasks: &TaskState,
    target: &ChatFilePreviewTarget,
    theme: &ThemeTokens,
    scroll: usize,
) -> Vec<Line<'static>> {
    #[cfg(test)]
    BUILD_LINES_CALL_COUNT.fetch_add(1, std::sync::atomic::Ordering::SeqCst);

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

fn file_preview_cache_key(
    area: Rect,
    tasks: &TaskState,
    target: &ChatFilePreviewTarget,
    theme: &ThemeTokens,
    terminal_graphics: bool,
) -> FilePreviewCacheKey {
    let image_preview_revision = if terminal_graphics {
        0
    } else {
        image_preview::resolve_local_image_path(&target.path)
            .as_deref()
            .filter(|path| image_preview::is_previewable_image_path(path))
            .map(|_| image_preview::preview_cache_revision())
            .unwrap_or(0)
    };

    FilePreviewCacheKey {
        area,
        path: target.path.clone(),
        repo_root: target.repo_root.clone(),
        repo_relative_path: target.repo_relative_path.clone(),
        task_state_id: tasks as *const TaskState as usize,
        preview_revision: tasks.preview_revision(),
        image_preview_revision,
        terminal_graphics,
        theme: FilePreviewThemeKey::from(theme),
    }
}

fn build_cached_lines(
    area: Rect,
    tasks: &TaskState,
    target: &ChatFilePreviewTarget,
    theme: &ThemeTokens,
    terminal_graphics: bool,
) -> CachedFilePreviewLines {
    let cache_scroll = if terminal_graphics { 0 } else { 1 };
    let full_lines = build_lines(area, tasks, target, theme, cache_scroll);
    if full_lines.len() <= area.height as usize {
        return CachedFilePreviewLines {
            key: file_preview_cache_key(area, tasks, target, theme, terminal_graphics),
            max_scroll: full_lines.len().saturating_sub(area.height as usize),
            lines: Arc::new(full_lines),
            content_area: area,
        };
    }

    let content_area = Rect::new(
        area.x,
        area.y,
        area.width.saturating_sub(SCROLLBAR_WIDTH),
        area.height,
    );
    let lines = build_lines(content_area, tasks, target, theme, cache_scroll);
    CachedFilePreviewLines {
        key: file_preview_cache_key(area, tasks, target, theme, terminal_graphics),
        max_scroll: lines.len().saturating_sub(content_area.height as usize),
        lines: Arc::new(lines),
        content_area,
    }
}

fn snapshot_with_cache(
    cache: &mut FilePreviewRenderCache,
    area: Rect,
    tasks: &TaskState,
    target: &ChatFilePreviewTarget,
    theme: &ThemeTokens,
    scroll: usize,
) -> Option<FilePreviewSnapshot> {
    if area.width == 0 || area.height == 0 {
        return None;
    }

    let terminal_graphics = uses_terminal_graphics(target, scroll);
    let key = file_preview_cache_key(area, tasks, target, theme, terminal_graphics);
    let cached = match cache.lines.as_ref() {
        Some(cached) if cached.key == key => cached.clone(),
        _ => {
            let cached = build_cached_lines(area, tasks, target, theme, terminal_graphics);
            cache.lines = Some(cached.clone());
            cached
        }
    };
    let layout = scrollbar_layout_from_metrics(area, cached.lines.len(), scroll);
    let resolved_scroll = layout
        .map(|layout| layout.scroll)
        .unwrap_or_else(|| scroll.min(cached.max_scroll));

    Some(FilePreviewSnapshot {
        lines: cached.lines,
        scroll: resolved_scroll,
        area: cached.content_area,
        layout,
        max_scroll: cached.max_scroll,
    })
}

fn snapshot(
    area: Rect,
    tasks: &TaskState,
    target: &ChatFilePreviewTarget,
    theme: &ThemeTokens,
    scroll: usize,
) -> Option<FilePreviewSnapshot> {
    let mut cache = lock_file_preview_cache();
    snapshot_with_cache(&mut cache, area, tasks, target, theme, scroll)
}

fn selection_snapshot(
    area: Rect,
    tasks: &TaskState,
    target: &ChatFilePreviewTarget,
    theme: &ThemeTokens,
    scroll: usize,
) -> Option<SelectionSnapshot> {
    let snapshot = snapshot(area, tasks, target, theme, scroll)?;
    if snapshot.lines.is_empty() || snapshot.area.width == 0 || snapshot.area.height == 0 {
        return None;
    }
    Some(SelectionSnapshot {
        lines: snapshot.lines,
        scroll: snapshot.scroll,
        area: snapshot.area,
    })
}

fn selection_point_from_snapshot(
    snapshot: &SelectionSnapshot,
    mouse: Position,
) -> Option<crate::widgets::chat::SelectionPoint> {
    let area = snapshot.area;
    let clamped_x = mouse
        .x
        .clamp(area.x, area.x.saturating_add(area.width).saturating_sub(1));
    let clamped_y = mouse
        .y
        .clamp(area.y, area.y.saturating_add(area.height).saturating_sub(1));
    let row = snapshot
        .scroll
        .saturating_add(clamped_y.saturating_sub(area.y) as usize)
        .min(snapshot.lines.len().saturating_sub(1));
    let col = clamped_x.saturating_sub(area.x) as usize;
    let width = line_display_width(snapshot.lines.get(row)?);
    Some(crate::widgets::chat::SelectionPoint {
        row,
        col: col.min(width),
    })
}

pub fn selection_point_from_mouse(
    area: Rect,
    tasks: &TaskState,
    target: &ChatFilePreviewTarget,
    theme: &ThemeTokens,
    scroll: usize,
    mouse: Position,
) -> Option<crate::widgets::chat::SelectionPoint> {
    let snapshot = selection_snapshot(area, tasks, target, theme, scroll)?;
    selection_point_from_snapshot(&snapshot, mouse)
}

pub fn selection_points_from_mouse(
    area: Rect,
    tasks: &TaskState,
    target: &ChatFilePreviewTarget,
    theme: &ThemeTokens,
    scroll: usize,
    start: Position,
    end: Position,
) -> Option<(
    crate::widgets::chat::SelectionPoint,
    crate::widgets::chat::SelectionPoint,
)> {
    let snapshot = selection_snapshot(area, tasks, target, theme, scroll)?;
    Some((
        selection_point_from_snapshot(&snapshot, start)?,
        selection_point_from_snapshot(&snapshot, end)?,
    ))
}

pub fn selected_text(
    area: Rect,
    tasks: &TaskState,
    target: &ChatFilePreviewTarget,
    theme: &ThemeTokens,
    scroll: usize,
    start: crate::widgets::chat::SelectionPoint,
    end: crate::widgets::chat::SelectionPoint,
) -> Option<String> {
    let snapshot = selection_snapshot(area, tasks, target, theme, scroll)?;
    let (start_point, end_point) =
        if start.row <= end.row || (start.row == end.row && start.col <= end.col) {
            (start, end)
        } else {
            (end, start)
        };
    if start_point == end_point {
        return None;
    }

    let mut lines = Vec::new();
    for row in start_point.row..=end_point.row {
        let line = snapshot.lines.get(row)?;
        let plain = line_plain_text(line);
        let width = line_display_width(line);
        let from = if row == start_point.row {
            start_point.col.min(width)
        } else {
            0
        };
        let to = if row == end_point.row {
            end_point.col.min(width).max(from)
        } else {
            width
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

fn apply_mouse_selection_highlight_to_line(
    row: usize,
    line: &mut Line<'static>,
    mouse_selection: Option<(
        crate::widgets::chat::SelectionPoint,
        crate::widgets::chat::SelectionPoint,
    )>,
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
    if row < start_point.row || row > end_point.row {
        return;
    }

    let line_width = line_display_width(line);
    let from = if row == start_point.row {
        start_point.col.min(line_width)
    } else {
        0
    };
    let to = if row == end_point.row {
        end_point.col.min(line_width).max(from)
    } else {
        line_width
    };
    highlight_line_range(line, from, to, Style::default().bg(Color::Indexed(31)));
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

    let content = snapshot(area, tasks, target, theme, scroll)?.area;
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
    let snapshot = snapshot(area, tasks, target, theme, scroll)?;
    let content = snapshot.area;
    if !content.contains(mouse) {
        return None;
    }
    if snapshot
        .scroll
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
    let layout = snapshot(area, tasks, target, theme, scroll)?.layout?;
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
    mouse_selection: Option<(
        crate::widgets::chat::SelectionPoint,
        crate::widgets::chat::SelectionPoint,
    )>,
) {
    if area.width == 0 || area.height == 0 {
        return;
    }

    let Some(snapshot) = snapshot(area, tasks, target, theme, scroll) else {
        return;
    };
    let visible = snapshot
        .lines
        .iter()
        .enumerate()
        .skip(snapshot.scroll)
        .take(snapshot.area.height as usize)
        .map(|(row, line)| {
            let mut line = line.clone();
            apply_mouse_selection_highlight_to_line(row, &mut line, mouse_selection);
            line
        })
        .collect::<Vec<_>>();

    if let Some(layout) = snapshot.layout {
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
        frame.render_widget(Paragraph::new(visible), snapshot.area);
    }
}

pub fn max_scroll(
    area: Rect,
    tasks: &TaskState,
    target: &ChatFilePreviewTarget,
    theme: &ThemeTokens,
) -> usize {
    snapshot(area, tasks, target, theme, 0)
        .map(|snapshot| snapshot.max_scroll)
        .unwrap_or(0)
}

pub(crate) fn scrollbar_layout(
    area: Rect,
    tasks: &TaskState,
    target: &ChatFilePreviewTarget,
    theme: &ThemeTokens,
    scroll: usize,
) -> Option<FilePreviewScrollbarLayout> {
    snapshot(area, tasks, target, theme, scroll)?.layout
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cached_snapshot_reuses_built_lines_for_same_preview_input() {
        let mut tasks = TaskState::default();
        tasks.reduce(crate::state::task::TaskAction::FilePreviewReceived(
            crate::state::task::FilePreview {
                path: "/tmp/large-preview.txt".to_string(),
                content: (1..=500)
                    .map(|idx| format!("large preview line {idx}"))
                    .collect::<Vec<_>>()
                    .join("\n"),
                truncated: false,
                is_text: true,
            },
        ));
        let target = ChatFilePreviewTarget {
            path: "/tmp/large-preview.txt".to_string(),
            repo_root: None,
            repo_relative_path: None,
        };
        let area = Rect::new(0, 0, 80, 20);
        let theme = ThemeTokens::default();
        let mut cache = FilePreviewRenderCache::default();

        reset_build_lines_call_count_for_tests();
        let first = snapshot_with_cache(&mut cache, area, &tasks, &target, &theme, 0)
            .expect("first snapshot should build");
        let calls_after_first = build_lines_call_count_for_tests();
        let second = snapshot_with_cache(&mut cache, area, &tasks, &target, &theme, 10)
            .expect("second snapshot should be cached");

        assert_eq!(first.lines.len(), second.lines.len());
        assert_eq!(
            build_lines_call_count_for_tests(),
            calls_after_first,
            "same file preview input should reuse cached rendered lines"
        );
    }

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
