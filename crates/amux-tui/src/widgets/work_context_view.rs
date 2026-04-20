use crate::state::sidebar::SidebarTab;
use crate::state::task::{TaskState, TodoStatus, WorkContextEntryKind};
use crate::terminal_graphics::{active_protocol, TerminalImageOverlaySpec, TerminalImageProtocol};
use crate::theme::ThemeTokens;
use crate::widgets::chat::SelectionPoint;
use crate::widgets::image_preview;
use crate::widgets::message::{render_markdown_pub, wrap_text};
use ratatui::prelude::*;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

#[path = "work_context_view_selection.rs"]
mod selection;

use selection::{display_slice, highlight_line_range, line_display_width, line_plain_text};

const SCROLLBAR_WIDTH: u16 = 1;
const WORK_CONTEXT_IMAGE_HEADER_LINES: u16 = 6;
const TERMINAL_IMAGE_HEADER_LINES: u16 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct WorkContextScrollbarLayout {
    content: Rect,
    scrollbar: Rect,
    thumb: Rect,
    scroll: usize,
    max_scroll: usize,
}

#[derive(Clone)]
struct RenderedWorkLine {
    line: Line<'static>,
    close_preview: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkContextHitTarget {
    ClosePreview,
}

struct SelectionSnapshot {
    all_lines: Vec<RenderedWorkLine>,
    scroll: usize,
    area: Rect,
}

fn scrollbar_layout_from_metrics(
    area: Rect,
    total_lines: usize,
    scroll: usize,
) -> Option<WorkContextScrollbarLayout> {
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

    Some(WorkContextScrollbarLayout {
        content,
        scrollbar,
        thumb,
        scroll,
        max_scroll,
    })
}

fn push_wrapped(
    lines: &mut Vec<Line<'static>>,
    text: &str,
    style: Style,
    width: usize,
    indent: usize,
) {
    let available = width.saturating_sub(indent).max(1);
    for line in wrap_text(text, available) {
        lines.push(Line::from(vec![
            Span::raw(" ".repeat(indent)),
            Span::styled(line, style),
        ]));
    }
}

fn section(lines: &mut Vec<Line<'static>>, title: &str, theme: &ThemeTokens) {
    if !lines.is_empty() {
        lines.push(Line::raw(""));
    }
    lines.push(Line::from(Span::styled(
        title.to_string(),
        theme.accent_primary.add_modifier(Modifier::BOLD),
    )));
}

fn is_markdown_path(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    lower.ends_with(".md") || lower.ends_with(".markdown") || lower.ends_with(".mdx")
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
        push_wrapped(lines, content, theme.fg_dim, width, 0);
    }
}

fn uses_terminal_graphics(
    entry_path: &str,
    repo_root: Option<&str>,
    active_tab: SidebarTab,
    scroll: usize,
) -> bool {
    active_tab == SidebarTab::Files
        && scroll == 0
        && repo_root.is_none()
        && active_protocol() != TerminalImageProtocol::None
        && image_preview::resolve_local_image_path(entry_path)
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

fn build_lines(
    area: Rect,
    tasks: &TaskState,
    thread_id: Option<&str>,
    active_tab: SidebarTab,
    selected_index: usize,
    theme: &ThemeTokens,
    scroll: usize,
) -> Vec<RenderedWorkLine> {
    if area.width == 0 || area.height == 0 {
        return Vec::new();
    }

    let width = area.width as usize;
    let mut lines = Vec::new();

    match active_tab {
        SidebarTab::Files => {
            section(&mut lines, "Files", theme);
            let Some(thread_id) = thread_id else {
                push_wrapped(&mut lines, "No thread selected.", theme.fg_dim, width, 0);
                return lines
                    .into_iter()
                    .map(|line| RenderedWorkLine {
                        line,
                        close_preview: false,
                    })
                    .collect();
            };
            lines.push(Line::from(vec![
                Span::styled("[x]", theme.accent_danger),
                Span::raw(" "),
                Span::styled("Close preview", theme.fg_dim),
            ]));
            let Some(context) = tasks.work_context_for_thread(thread_id) else {
                push_wrapped(
                    &mut lines,
                    "No files recorded for this thread.",
                    theme.fg_dim,
                    width,
                    0,
                );
                return lines
                    .into_iter()
                    .map(|line| RenderedWorkLine {
                        line,
                        close_preview: false,
                    })
                    .collect();
            };
            let Some(entry) = context.entries.get(selected_index) else {
                push_wrapped(
                    &mut lines,
                    "Select a file from the sidebar.",
                    theme.fg_dim,
                    width,
                    0,
                );
                return lines
                    .into_iter()
                    .map(|line| RenderedWorkLine {
                        line,
                        close_preview: false,
                    })
                    .collect();
            };

            lines.push(Line::from(vec![
                Span::styled("Path: ", theme.fg_dim),
                Span::styled(entry.path.clone(), theme.fg_active),
            ]));
            let kind = match entry.kind {
                Some(WorkContextEntryKind::RepoChange) => "repo change",
                Some(WorkContextEntryKind::Artifact) => "artifact",
                Some(WorkContextEntryKind::GeneratedSkill) => "generated skill",
                None => "file",
            };
            lines.push(Line::from(vec![
                Span::styled("Type: ", theme.fg_dim),
                Span::styled(kind, theme.fg_active),
            ]));
            if let Some(change_kind) = &entry.change_kind {
                lines.push(Line::from(vec![
                    Span::styled("Change: ", theme.fg_dim),
                    Span::styled(change_kind.clone(), theme.fg_active),
                ]));
            }
            if let Some(previous_path) = &entry.previous_path {
                lines.push(Line::from(vec![
                    Span::styled("From: ", theme.fg_dim),
                    Span::styled(previous_path.clone(), theme.fg_active),
                ]));
            }

            section(&mut lines, "Preview", theme);
            let image_preview_height =
                area.height.saturating_sub(lines.len() as u16).max(1) as usize;
            let use_terminal_image =
                uses_terminal_graphics(&entry.path, entry.repo_root.as_deref(), active_tab, scroll);
            if let Some(repo_root) = entry.repo_root.as_deref() {
                if let Some(diff) = tasks.diff_for_path(repo_root, &entry.path) {
                    if diff.trim().is_empty() {
                        push_wrapped(
                            &mut lines,
                            "No diff preview available for this file.",
                            theme.fg_dim,
                            width,
                            0,
                        );
                    } else {
                        push_wrapped(&mut lines, diff, theme.fg_dim, width, 0);
                    }
                } else {
                    push_wrapped(&mut lines, "Loading diff...", theme.fg_dim, width, 0);
                }
            } else if let Some(preview) = tasks.preview_for_path(&entry.path) {
                if preview.is_text {
                    push_preview_content(&mut lines, &entry.path, &preview.content, width, theme);
                } else if use_terminal_image {
                    push_terminal_graphics_placeholder(&mut lines, image_preview_height, theme);
                } else if image_preview::is_previewable_image_path(&entry.path) {
                    lines.extend(image_preview::render_image_preview_lines(
                        &entry.path,
                        width,
                        14,
                        theme,
                    ));
                } else {
                    push_wrapped(
                        &mut lines,
                        "Binary file preview is not available.",
                        theme.fg_dim,
                        width,
                        0,
                    );
                }
            } else {
                if image_preview::is_previewable_image_path(&entry.path) {
                    if use_terminal_image {
                        push_terminal_graphics_placeholder(&mut lines, image_preview_height, theme);
                    } else {
                        lines.extend(image_preview::render_image_preview_lines(
                            &entry.path,
                            width,
                            14,
                            theme,
                        ));
                    }
                } else {
                    push_wrapped(&mut lines, "Loading preview...", theme.fg_dim, width, 0);
                }
            }
        }
        SidebarTab::Todos => {
            section(&mut lines, "Todos", theme);
            let Some(thread_id) = thread_id else {
                push_wrapped(&mut lines, "No thread selected.", theme.fg_dim, width, 0);
                return lines
                    .into_iter()
                    .map(|line| RenderedWorkLine {
                        line,
                        close_preview: false,
                    })
                    .collect();
            };
            lines.push(Line::from(vec![
                Span::styled("[x]", theme.accent_danger),
                Span::raw(" "),
                Span::styled("Close preview", theme.fg_dim),
            ]));
            let todos = tasks.todos_for_thread(thread_id);
            if todos.is_empty() {
                push_wrapped(
                    &mut lines,
                    "No todos recorded for this thread.",
                    theme.fg_dim,
                    width,
                    0,
                );
                return lines
                    .into_iter()
                    .map(|line| RenderedWorkLine {
                        line,
                        close_preview: false,
                    })
                    .collect();
            }

            let index = selected_index.min(todos.len().saturating_sub(1));
            let todo = &todos[index];
            let status = match todo.status {
                Some(TodoStatus::Completed) => "completed",
                Some(TodoStatus::InProgress) => "in progress",
                Some(TodoStatus::Blocked) => "blocked",
                _ => "pending",
            };
            lines.push(Line::from(vec![
                Span::styled("Status: ", theme.fg_dim),
                Span::styled(status, theme.fg_active),
            ]));
            if let Some(step_index) = todo.step_index {
                lines.push(Line::from(vec![
                    Span::styled("Step: ", theme.fg_dim),
                    Span::styled((step_index + 1).to_string(), theme.fg_active),
                ]));
            }
            section(&mut lines, "Selected Todo", theme);
            push_wrapped(&mut lines, &todo.content, theme.fg_active, width, 0);

            section(&mut lines, "All Todos", theme);
            for (idx, item) in todos.iter().enumerate() {
                let marker = if idx == index { ">" } else { " " };
                let chip = match item.status {
                    Some(TodoStatus::Completed) => "[x]",
                    Some(TodoStatus::InProgress) => "[~]",
                    Some(TodoStatus::Blocked) => "[!]",
                    _ => "[ ]",
                };
                push_wrapped(
                    &mut lines,
                    &format!("{marker} {chip} {}", item.content),
                    if idx == index {
                        theme.fg_active
                    } else {
                        theme.fg_dim
                    },
                    width,
                    0,
                );
            }
        }
        SidebarTab::Spawned => {
            section(&mut lines, "Spawned", theme);
            lines.push(Line::from(vec![
                Span::styled("[x]", theme.accent_danger),
                Span::raw(" "),
                Span::styled("Close preview", theme.fg_dim),
            ]));
            push_wrapped(
                &mut lines,
                "Spawned agent navigation stays in the conversation view.",
                theme.fg_dim,
                width,
                0,
            );
        }
        SidebarTab::Pinned => {
            section(&mut lines, "Pinned", theme);
            lines.push(Line::from(vec![
                Span::styled("[x]", theme.accent_danger),
                Span::raw(" "),
                Span::styled("Close preview", theme.fg_dim),
            ]));
            push_wrapped(
                &mut lines,
                "Pinned messages jump back into the conversation view.",
                theme.fg_dim,
                width,
                0,
            );
        }
    }

    lines
        .into_iter()
        .enumerate()
        .map(|(index, line)| RenderedWorkLine {
            line,
            close_preview: index == 1,
        })
        .collect()
}

fn selection_snapshot(
    area: Rect,
    tasks: &TaskState,
    thread_id: Option<&str>,
    active_tab: SidebarTab,
    selected_index: usize,
    theme: &ThemeTokens,
    scroll: usize,
) -> Option<SelectionSnapshot> {
    let layout = scrollbar_layout(
        area,
        tasks,
        thread_id,
        active_tab,
        selected_index,
        theme,
        scroll,
    );
    let content = layout.map(|layout| layout.content).unwrap_or(area);
    let resolved_scroll = layout.map(|layout| layout.scroll).unwrap_or(scroll);
    let all_lines = build_lines(
        content,
        tasks,
        thread_id,
        active_tab,
        selected_index,
        theme,
        resolved_scroll,
    );
    if all_lines.is_empty() || content.width == 0 || content.height == 0 {
        return None;
    }
    Some(SelectionSnapshot {
        all_lines,
        scroll: resolved_scroll,
        area: content,
    })
}

fn selection_point_from_snapshot(
    snapshot: &SelectionSnapshot,
    mouse: Position,
) -> Option<SelectionPoint> {
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
        .min(snapshot.all_lines.len().saturating_sub(1));
    let col = clamped_x.saturating_sub(area.x) as usize;
    let width = line_display_width(&snapshot.all_lines.get(row)?.line);
    Some(SelectionPoint {
        row,
        col: col.min(width),
    })
}

pub fn selection_points_from_mouse(
    area: Rect,
    tasks: &TaskState,
    thread_id: Option<&str>,
    active_tab: SidebarTab,
    selected_index: usize,
    theme: &ThemeTokens,
    scroll: usize,
    start: Position,
    end: Position,
) -> Option<(SelectionPoint, SelectionPoint)> {
    let snapshot = selection_snapshot(
        area,
        tasks,
        thread_id,
        active_tab,
        selected_index,
        theme,
        scroll,
    )?;
    Some((
        selection_point_from_snapshot(&snapshot, start)?,
        selection_point_from_snapshot(&snapshot, end)?,
    ))
}

pub fn selection_point_from_mouse(
    area: Rect,
    tasks: &TaskState,
    thread_id: Option<&str>,
    active_tab: SidebarTab,
    selected_index: usize,
    theme: &ThemeTokens,
    scroll: usize,
    mouse: Position,
) -> Option<SelectionPoint> {
    let snapshot = selection_snapshot(
        area,
        tasks,
        thread_id,
        active_tab,
        selected_index,
        theme,
        scroll,
    )?;
    selection_point_from_snapshot(&snapshot, mouse)
}

pub fn selected_text(
    area: Rect,
    tasks: &TaskState,
    thread_id: Option<&str>,
    active_tab: SidebarTab,
    selected_index: usize,
    theme: &ThemeTokens,
    scroll: usize,
    start: SelectionPoint,
    end: SelectionPoint,
) -> Option<String> {
    let snapshot = selection_snapshot(
        area,
        tasks,
        thread_id,
        active_tab,
        selected_index,
        theme,
        scroll,
    )?;
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
        let rendered = snapshot.all_lines.get(row)?;
        let plain = line_plain_text(&rendered.line);
        let width = line_display_width(&rendered.line);
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

pub fn hit_test(
    area: Rect,
    tasks: &TaskState,
    thread_id: Option<&str>,
    active_tab: SidebarTab,
    selected_index: usize,
    scroll: usize,
    mouse: Position,
    theme: &ThemeTokens,
) -> Option<WorkContextHitTarget> {
    let layout = scrollbar_layout(
        area,
        tasks,
        thread_id,
        active_tab,
        selected_index,
        theme,
        scroll,
    );
    let content = layout.map(|layout| layout.content).unwrap_or(area);
    if !content.contains(mouse) {
        return None;
    }

    let row_index = layout
        .map(|layout| layout.scroll)
        .unwrap_or(scroll)
        .saturating_add(mouse.y.saturating_sub(content.y) as usize);
    build_lines(
        content,
        tasks,
        thread_id,
        active_tab,
        selected_index,
        theme,
        scroll,
    )
    .get(row_index)
    .and_then(|line| {
        if line.close_preview {
            Some(WorkContextHitTarget::ClosePreview)
        } else {
            None
        }
    })
}

pub fn render(
    frame: &mut Frame,
    area: Rect,
    tasks: &TaskState,
    thread_id: Option<&str>,
    active_tab: SidebarTab,
    selected_index: usize,
    theme: &ThemeTokens,
    scroll: usize,
    mouse_selection: Option<(SelectionPoint, SelectionPoint)>,
) {
    if area.width == 0 || area.height == 0 {
        return;
    }

    let layout = scrollbar_layout(
        area,
        tasks,
        thread_id,
        active_tab,
        selected_index,
        theme,
        scroll,
    );
    let content = layout.map(|layout| layout.content).unwrap_or(area);
    let resolved_scroll = layout.map(|layout| layout.scroll).unwrap_or(scroll);
    let mut all_lines = build_lines(
        content,
        tasks,
        thread_id,
        active_tab,
        selected_index,
        theme,
        resolved_scroll,
    );

    if let Some((start, end)) = mouse_selection {
        let (start_point, end_point) =
            if start.row <= end.row || (start.row == end.row && start.col <= end.col) {
                (start, end)
            } else {
                (end, start)
            };
        let highlight = Style::default().bg(Color::Indexed(31));
        for row in start_point.row..=end_point.row {
            if let Some(line) = all_lines.get_mut(row) {
                let line_width = line_display_width(&line.line);
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
                highlight_line_range(&mut line.line, from, to, highlight);
            }
        }
    }

    let visible = all_lines
        .into_iter()
        .skip(resolved_scroll)
        .take(content.height as usize)
        .map(|line| line.line)
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
    thread_id: Option<&str>,
    active_tab: SidebarTab,
    selected_index: usize,
    theme: &ThemeTokens,
) -> usize {
    if area.width == 0 || area.height == 0 {
        return 0;
    }

    scrollbar_layout(area, tasks, thread_id, active_tab, selected_index, theme, 0)
        .map(|layout| layout.max_scroll)
        .unwrap_or_else(|| {
            build_lines(area, tasks, thread_id, active_tab, selected_index, theme, 0)
                .len()
                .saturating_sub(area.height as usize)
        })
}

fn scrollbar_layout(
    area: Rect,
    tasks: &TaskState,
    thread_id: Option<&str>,
    active_tab: SidebarTab,
    selected_index: usize,
    theme: &ThemeTokens,
    scroll: usize,
) -> Option<WorkContextScrollbarLayout> {
    if area.width <= SCROLLBAR_WIDTH || area.height == 0 {
        return None;
    }

    let full_lines = build_lines(
        area,
        tasks,
        thread_id,
        active_tab,
        selected_index,
        theme,
        scroll,
    );
    if full_lines.len() <= area.height as usize {
        return None;
    }

    let content = Rect::new(
        area.x,
        area.y,
        area.width.saturating_sub(SCROLLBAR_WIDTH),
        area.height,
    );
    let all_lines = build_lines(
        content,
        tasks,
        thread_id,
        active_tab,
        selected_index,
        theme,
        scroll,
    );
    scrollbar_layout_from_metrics(area, all_lines.len(), scroll)
}

pub fn terminal_image_overlay_spec(
    area: Rect,
    tasks: &TaskState,
    thread_id: Option<&str>,
    active_tab: SidebarTab,
    selected_index: usize,
    theme: &ThemeTokens,
    scroll: usize,
) -> Option<TerminalImageOverlaySpec> {
    if active_tab != SidebarTab::Files || scroll != 0 {
        return None;
    }

    let thread_id = thread_id?;
    let context = tasks.work_context_for_thread(thread_id)?;
    let entry = context.entries.get(selected_index)?;
    if !uses_terminal_graphics(&entry.path, entry.repo_root.as_deref(), active_tab, scroll) {
        return None;
    }

    let content = scrollbar_layout(
        area,
        tasks,
        Some(thread_id),
        active_tab,
        selected_index,
        theme,
        scroll,
    )
    .map(|layout| layout.content)
    .unwrap_or(area);
    let path = image_preview::resolve_local_image_path(&entry.path)?;
    let image_row = content
        .y
        .saturating_add(WORK_CONTEXT_IMAGE_HEADER_LINES)
        .saturating_add(TERMINAL_IMAGE_HEADER_LINES);
    let image_rows = content
        .height
        .saturating_sub(WORK_CONTEXT_IMAGE_HEADER_LINES)
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

#[cfg(test)]
#[path = "tests/work_context_view.rs"]
mod tests;
