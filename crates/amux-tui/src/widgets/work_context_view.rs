use ratatui::prelude::*;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::state::sidebar::SidebarTab;
use crate::state::task::{TaskState, TodoStatus, WorkContextEntryKind};
use crate::theme::ThemeTokens;
use crate::widgets::message::{render_markdown_pub, wrap_text};

fn push_wrapped(lines: &mut Vec<Line<'static>>, text: &str, style: Style, width: usize, indent: usize) {
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

pub fn render(
    frame: &mut Frame,
    area: Rect,
    tasks: &TaskState,
    thread_id: Option<&str>,
    active_tab: SidebarTab,
    selected_index: usize,
    theme: &ThemeTokens,
    scroll: usize,
) {
    if area.width == 0 || area.height == 0 {
        return;
    }

    let width = area.width as usize;
    let mut lines = Vec::new();

    match active_tab {
        SidebarTab::Files => {
            section(&mut lines, "Files", theme);
            let Some(thread_id) = thread_id else {
                push_wrapped(&mut lines, "No thread selected.", theme.fg_dim, width, 0);
                frame.render_widget(
                    Paragraph::new(lines).scroll((scroll as u16, 0)),
                    area,
                );
                return;
            };
            let Some(context) = tasks.work_context_for_thread(thread_id) else {
                push_wrapped(&mut lines, "No files recorded for this thread.", theme.fg_dim, width, 0);
                frame.render_widget(
                    Paragraph::new(lines).scroll((scroll as u16, 0)),
                    area,
                );
                return;
            };
            let Some(entry) = context.entries.get(selected_index) else {
                push_wrapped(&mut lines, "Select a file from the sidebar.", theme.fg_dim, width, 0);
                frame.render_widget(
                    Paragraph::new(lines).scroll((scroll as u16, 0)),
                    area,
                );
                return;
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
                push_wrapped(&mut lines, "Loading preview...", theme.fg_dim, width, 0);
            }
        }
        SidebarTab::Todos => {
            section(&mut lines, "Todos", theme);
            let Some(thread_id) = thread_id else {
                push_wrapped(&mut lines, "No thread selected.", theme.fg_dim, width, 0);
                frame.render_widget(
                    Paragraph::new(lines).scroll((scroll as u16, 0)),
                    area,
                );
                return;
            };
            let todos = tasks.todos_for_thread(thread_id);
            if todos.is_empty() {
                push_wrapped(&mut lines, "No todos recorded for this thread.", theme.fg_dim, width, 0);
                frame.render_widget(
                    Paragraph::new(lines).scroll((scroll as u16, 0)),
                    area,
                );
                return;
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
                    if idx == index { theme.fg_active } else { theme.fg_dim },
                    width,
                    0,
                );
            }
        }
    }

    frame.render_widget(Paragraph::new(lines).scroll((scroll as u16, 0)), area);
}
