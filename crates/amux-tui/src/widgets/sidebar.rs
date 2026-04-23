use ratatui::prelude::*;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use std::hash::{Hash, Hasher};

use crate::app::RecentActionVm;
use crate::state::chat::GatewayStatusVm;
use crate::state::chat::{ChatState, MessageRole};
use crate::state::sidebar::{SidebarState, SidebarTab};
use crate::state::task::TaskState;
use crate::state::tier::TierState;
use crate::theme::ThemeTokens;

#[path = "sidebar/spawned_agents.rs"]
mod spawned_agents;
#[path = "sidebar/tab_layout.rs"]
mod tab_layout;

use tab_layout::{tab_cells, tab_hit_test, tab_label};

#[derive(Debug, Clone)]
struct SidebarRow {
    line: Line<'static>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SidebarHitTarget {
    Tab(SidebarTab),
    File(String),
    Todo(usize),
    Spawned(usize),
    Pinned(usize),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SidebarSnapshotKey {
    width: u16,
    active_tab: SidebarTab,
    thread_id: Option<String>,
    files_filter: String,
    show_spawned: bool,
    show_pinned: bool,
    body_hash: u64,
}

#[derive(Debug, Clone)]
struct FileSidebarItem {
    path: String,
    label: String,
    display_path: String,
}

#[derive(Debug, Clone)]
struct TodoSidebarItem {
    index: usize,
    marker: &'static str,
    text: String,
}

#[derive(Debug, Clone)]
struct PinnedSidebarItem {
    index: usize,
    metadata: String,
    snippet: String,
}

#[derive(Debug, Clone)]
enum SidebarBodySnapshot {
    Empty { message: String },
    Files(Vec<FileSidebarItem>),
    Todos(Vec<TodoSidebarItem>),
    Spawned(Vec<spawned_agents::SpawnedSidebarItem>),
    Pinned(Vec<PinnedSidebarItem>),
}

#[derive(Debug, Clone)]
pub struct CachedSidebarSnapshot {
    key: SidebarSnapshotKey,
    body: SidebarBodySnapshot,
}

impl CachedSidebarSnapshot {
    fn show_spawned(&self) -> bool {
        self.key.show_spawned
    }

    fn show_pinned(&self) -> bool {
        self.key.show_pinned
    }

    pub fn item_count(&self) -> usize {
        match &self.body {
            SidebarBodySnapshot::Empty { .. } => 1,
            SidebarBodySnapshot::Files(items) => items.len().max(1),
            SidebarBodySnapshot::Todos(items) => items.len().max(1),
            SidebarBodySnapshot::Spawned(items) => items.len().max(1),
            SidebarBodySnapshot::Pinned(items) => items.len().max(1),
        }
    }

    pub fn selected_file_path(&self, selected_index: usize) -> Option<String> {
        let SidebarBodySnapshot::Files(items) = &self.body else {
            return None;
        };
        let selected = selected_index.min(items.len().saturating_sub(1));
        items.get(selected).map(|item| item.path.clone())
    }

    pub fn filtered_file_index(&self, path: &str) -> Option<usize> {
        let SidebarBodySnapshot::Files(items) = &self.body else {
            return None;
        };
        items.iter().position(|item| item.path == path)
    }

    pub fn selected_spawned_thread_id(&self, selected_index: usize) -> Option<String> {
        let SidebarBodySnapshot::Spawned(items) = &self.body else {
            return None;
        };
        items
            .get(selected_index)
            .and_then(|item| item.target_thread_id.clone())
    }

    pub fn first_openable_spawned_index(&self) -> Option<usize> {
        let SidebarBodySnapshot::Spawned(items) = &self.body else {
            return None;
        };
        items.iter().position(|item| item.openable)
    }

    pub fn selected_pinned_message(
        &self,
        chat: &ChatState,
        selected_index: usize,
    ) -> Option<crate::state::chat::PinnedThreadMessage> {
        let SidebarBodySnapshot::Pinned(items) = &self.body else {
            return None;
        };
        let index = items.get(selected_index)?.index;
        chat.active_thread_pinned_messages().into_iter().nth(index)
    }

    fn row_target(&self, row_index: usize) -> Option<SidebarHitTarget> {
        match &self.body {
            SidebarBodySnapshot::Empty { .. } => None,
            SidebarBodySnapshot::Files(items) => items
                .get(row_index)
                .map(|item| SidebarHitTarget::File(item.path.clone())),
            SidebarBodySnapshot::Todos(items) => items
                .get(row_index)
                .map(|item| SidebarHitTarget::Todo(item.index)),
            SidebarBodySnapshot::Spawned(items) => items
                .get(row_index)
                .map(|_| SidebarHitTarget::Spawned(row_index)),
            SidebarBodySnapshot::Pinned(items) => items
                .get(row_index)
                .map(|item| SidebarHitTarget::Pinned(item.index)),
        }
    }
}

#[cfg(test)]
thread_local! {
    static BUILD_CACHED_SNAPSHOT_CALLS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

type PinnedSidebarRows = Vec<crate::state::chat::PinnedThreadMessage>;

fn file_entry_matches(entry: &crate::state::task::WorkContextEntry, filter: &str) -> bool {
    let query = filter.trim();
    if query.is_empty() {
        return true;
    }
    let query = query.to_ascii_lowercase();
    entry.path.to_ascii_lowercase().contains(&query)
        || entry
            .previous_path
            .as_deref()
            .is_some_and(|path| path.to_ascii_lowercase().contains(&query))
        || entry
            .change_kind
            .as_deref()
            .is_some_and(|kind| kind.to_ascii_lowercase().contains(&query))
}

fn filtered_file_entries<'a>(
    tasks: &'a TaskState,
    thread_id: Option<&str>,
    sidebar: &SidebarState,
) -> Vec<&'a crate::state::task::WorkContextEntry> {
    let Some(thread_id) = thread_id else {
        return Vec::new();
    };
    let Some(context) = tasks.work_context_for_thread(thread_id) else {
        return Vec::new();
    };
    context
        .entries
        .iter()
        .filter(|entry| file_entry_matches(entry, sidebar.files_filter()))
        .collect()
}

fn hash_sidebar_tab<H: Hasher>(hasher: &mut H, tab: SidebarTab) {
    match tab {
        SidebarTab::Files => 0u8.hash(hasher),
        SidebarTab::Todos => 1u8.hash(hasher),
        SidebarTab::Spawned => 2u8.hash(hasher),
        SidebarTab::Pinned => 3u8.hash(hasher),
    }
}

fn hash_message_role<H: Hasher>(hasher: &mut H, role: MessageRole) {
    match role {
        MessageRole::User => 0u8.hash(hasher),
        MessageRole::Assistant => 1u8.hash(hasher),
        MessageRole::System => 2u8.hash(hasher),
        MessageRole::Tool => 3u8.hash(hasher),
        MessageRole::Unknown => 4u8.hash(hasher),
    }
}

fn hash_task_status<H: Hasher>(hasher: &mut H, status: Option<crate::state::task::TaskStatus>) {
    match status {
        Some(crate::state::task::TaskStatus::Queued) => 0u8.hash(hasher),
        Some(crate::state::task::TaskStatus::InProgress) => 1u8.hash(hasher),
        Some(crate::state::task::TaskStatus::AwaitingApproval) => 2u8.hash(hasher),
        Some(crate::state::task::TaskStatus::Blocked) => 3u8.hash(hasher),
        Some(crate::state::task::TaskStatus::FailedAnalyzing) => 4u8.hash(hasher),
        Some(crate::state::task::TaskStatus::BudgetExceeded) => 5u8.hash(hasher),
        Some(crate::state::task::TaskStatus::Completed) => 6u8.hash(hasher),
        Some(crate::state::task::TaskStatus::Failed) => 7u8.hash(hasher),
        Some(crate::state::task::TaskStatus::Cancelled) => 8u8.hash(hasher),
        None => 9u8.hash(hasher),
    }
}

fn sidebar_snapshot_key(
    area: Rect,
    chat: &ChatState,
    sidebar: &SidebarState,
    tasks: &TaskState,
    thread_id: Option<&str>,
) -> SidebarSnapshotKey {
    let show_spawned = has_spawned_tab(tasks, chat, thread_id);
    let pinned_rows = active_thread_pinned_rows(chat);
    let show_pinned = !pinned_rows.is_empty();

    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    hash_sidebar_tab(&mut hasher, sidebar.active_tab());
    thread_id.hash(&mut hasher);
    sidebar.files_filter().hash(&mut hasher);
    show_spawned.hash(&mut hasher);
    show_pinned.hash(&mut hasher);

    match sidebar.active_tab() {
        SidebarTab::Files => {
            if let Some(thread_id) = thread_id {
                if let Some(context) = tasks.work_context_for_thread(thread_id) {
                    for entry in context
                        .entries
                        .iter()
                        .filter(|entry| file_entry_matches(entry, sidebar.files_filter()))
                    {
                        entry.path.hash(&mut hasher);
                        entry.previous_path.hash(&mut hasher);
                        entry.change_kind.hash(&mut hasher);
                        entry.is_text.hash(&mut hasher);
                    }
                }
            }
        }
        SidebarTab::Todos => {
            if let Some(thread_id) = thread_id {
                for todo in tasks.todos_for_thread(thread_id) {
                    todo.id.hash(&mut hasher);
                    todo.content.hash(&mut hasher);
                    todo.position.hash(&mut hasher);
                    hash_task_status(
                        &mut hasher,
                        todo.status.map(|status| match status {
                            crate::state::task::TodoStatus::Pending => {
                                crate::state::task::TaskStatus::Queued
                            }
                            crate::state::task::TodoStatus::InProgress => {
                                crate::state::task::TaskStatus::InProgress
                            }
                            crate::state::task::TodoStatus::Completed => {
                                crate::state::task::TaskStatus::Completed
                            }
                            crate::state::task::TodoStatus::Blocked => {
                                crate::state::task::TaskStatus::Blocked
                            }
                        }),
                    );
                }
            }
        }
        SidebarTab::Spawned => {
            tasks.tasks_revision().hash(&mut hasher);
            chat.can_go_back_thread().hash(&mut hasher);
        }
        SidebarTab::Pinned => {
            for message in &pinned_rows {
                message.message_id.hash(&mut hasher);
                message.absolute_index.hash(&mut hasher);
                hash_message_role(&mut hasher, message.role);
                message.content.hash(&mut hasher);
            }
        }
    }

    SidebarSnapshotKey {
        width: area.width,
        active_tab: sidebar.active_tab(),
        thread_id: thread_id.map(str::to_string),
        files_filter: sidebar.files_filter().to_string(),
        show_spawned,
        show_pinned,
        body_hash: hasher.finish(),
    }
}

fn truncate_tail(text: &str, max_len: usize) -> String {
    if text.chars().count() <= max_len {
        return text.to_string();
    }
    if max_len <= 1 {
        return "…".to_string();
    }
    let tail: String = text
        .chars()
        .rev()
        .take(max_len.saturating_sub(1))
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    format!("…{tail}")
}

fn build_body_snapshot(
    area: Rect,
    chat: &ChatState,
    sidebar: &SidebarState,
    tasks: &TaskState,
    thread_id: Option<&str>,
) -> SidebarBodySnapshot {
    #[cfg(test)]
    BUILD_CACHED_SNAPSHOT_CALLS.with(|calls| calls.set(calls.get() + 1));

    let width = area.width as usize;

    match (sidebar.active_tab(), thread_id) {
        (SidebarTab::Files, None) => SidebarBodySnapshot::Empty {
            message: " No thread selected".to_string(),
        },
        (SidebarTab::Files, Some(thread_id)) => {
            let entries = filtered_file_entries(tasks, Some(thread_id), sidebar);
            if entries.is_empty() {
                return SidebarBodySnapshot::Empty {
                    message: if sidebar.files_filter().is_empty() {
                        " No files".to_string()
                    } else {
                        " No files match filter".to_string()
                    },
                };
            }

            SidebarBodySnapshot::Files(
                entries
                    .into_iter()
                    .map(|entry| {
                        let label = entry.change_kind.as_deref().unwrap_or_else(|| {
                            entry
                                .kind
                                .map(|kind| match kind {
                                    crate::state::task::WorkContextEntryKind::RepoChange => "diff",
                                    crate::state::task::WorkContextEntryKind::Artifact => "file",
                                    crate::state::task::WorkContextEntryKind::GeneratedSkill => {
                                        "skill"
                                    }
                                })
                                .unwrap_or("file")
                        });

                        FileSidebarItem {
                            path: entry.path.clone(),
                            label: label.to_string(),
                            display_path: truncate_tail(
                                &entry.path,
                                width.saturating_sub(12).max(8),
                            ),
                        }
                    })
                    .collect(),
            )
        }
        (SidebarTab::Todos, None) => SidebarBodySnapshot::Empty {
            message: " No thread selected".to_string(),
        },
        (SidebarTab::Todos, Some(thread_id)) => {
            let todos = tasks.todos_for_thread(thread_id);
            if todos.is_empty() {
                return SidebarBodySnapshot::Empty {
                    message: " No todos".to_string(),
                };
            }

            SidebarBodySnapshot::Todos(
                todos
                    .iter()
                    .enumerate()
                    .map(|(idx, todo)| TodoSidebarItem {
                        index: idx,
                        marker: match todo.status {
                            Some(crate::state::task::TodoStatus::Completed) => "[x]",
                            Some(crate::state::task::TodoStatus::InProgress) => "[~]",
                            Some(crate::state::task::TodoStatus::Blocked) => "[!]",
                            _ => "[ ]",
                        },
                        text: if todo.content.chars().count() > width.saturating_sub(8).max(8) {
                            format!(
                                "{}…",
                                todo.content
                                    .chars()
                                    .take(width.saturating_sub(9).max(7))
                                    .collect::<String>()
                            )
                        } else {
                            todo.content.clone()
                        },
                    })
                    .collect(),
            )
        }
        (SidebarTab::Spawned, _) => {
            let items = spawned_agents::flattened_items(tasks, thread_id);
            if items.is_empty() {
                SidebarBodySnapshot::Empty {
                    message: " No spawned agents".to_string(),
                }
            } else {
                SidebarBodySnapshot::Spawned(items)
            }
        }
        (SidebarTab::Pinned, _) => {
            let pinned_rows = active_thread_pinned_rows(chat);
            if pinned_rows.is_empty() {
                return SidebarBodySnapshot::Empty {
                    message: " No pinned messages".to_string(),
                };
            }

            SidebarBodySnapshot::Pinned(
                pinned_rows
                    .iter()
                    .enumerate()
                    .map(|(index, message)| PinnedSidebarItem {
                        index,
                        metadata: format!(
                            "[{} {}c]",
                            pinned_message_role_label(message.role),
                            pinned_message_chars(message)
                        ),
                        snippet: pinned_message_snippet(&message.content, width),
                    })
                    .collect(),
            )
        }
    }
}

pub fn build_cached_snapshot(
    area: Rect,
    chat: &ChatState,
    sidebar: &SidebarState,
    tasks: &TaskState,
    thread_id: Option<&str>,
) -> CachedSidebarSnapshot {
    CachedSidebarSnapshot {
        key: sidebar_snapshot_key(area, chat, sidebar, tasks, thread_id),
        body: build_body_snapshot(area, chat, sidebar, tasks, thread_id),
    }
}

pub fn cached_snapshot_matches_render(
    snapshot: &CachedSidebarSnapshot,
    area: Rect,
    chat: &ChatState,
    sidebar: &SidebarState,
    tasks: &TaskState,
    thread_id: Option<&str>,
) -> bool {
    snapshot.key == sidebar_snapshot_key(area, chat, sidebar, tasks, thread_id)
}

pub fn selected_file_path(
    tasks: &TaskState,
    sidebar: &SidebarState,
    thread_id: Option<&str>,
) -> Option<String> {
    build_cached_snapshot(
        Rect::new(0, 0, 80, 0),
        &ChatState::new(),
        sidebar,
        tasks,
        thread_id,
    )
    .selected_file_path(sidebar.selected_item())
}

pub fn filtered_file_index(
    tasks: &TaskState,
    sidebar: &SidebarState,
    thread_id: Option<&str>,
    path: &str,
) -> Option<usize> {
    build_cached_snapshot(
        Rect::new(0, 0, 80, 0),
        &ChatState::new(),
        sidebar,
        tasks,
        thread_id,
    )
    .filtered_file_index(path)
}

pub fn selected_pinned_message(
    chat: &ChatState,
    sidebar: &SidebarState,
) -> Option<crate::state::chat::PinnedThreadMessage> {
    build_cached_snapshot(
        Rect::new(0, 0, 80, 0),
        chat,
        sidebar,
        &TaskState::new(),
        None,
    )
    .selected_pinned_message(chat, sidebar.selected_item())
}

fn pinned_message_chars(message: &crate::state::chat::PinnedThreadMessage) -> usize {
    message.content.chars().count()
}

fn pinned_message_role_label(role: MessageRole) -> &'static str {
    match role {
        MessageRole::User => "user",
        MessageRole::Assistant => "assistant",
        MessageRole::System => "system",
        MessageRole::Tool => "tool",
        MessageRole::Unknown => "unknown",
    }
}

fn pinned_message_snippet(content: &str, width: usize) -> String {
    let compact = content.split_whitespace().collect::<Vec<_>>().join(" ");
    let max_len = width.saturating_sub(18).max(8);
    if compact.chars().count() > max_len {
        format!(
            "{}…",
            compact
                .chars()
                .take(max_len.saturating_sub(1))
                .collect::<String>()
        )
    } else {
        compact
    }
}

fn active_thread_pinned_rows(chat: &ChatState) -> PinnedSidebarRows {
    chat.active_thread_pinned_messages()
}

fn pinned_footer_line(theme: &ThemeTokens) -> Line<'static> {
    Line::from(vec![
        Span::styled(" Ctrl+K J", theme.fg_active),
        Span::styled(" jump  ", theme.fg_dim),
        Span::styled("Ctrl+K U", theme.fg_active),
        Span::styled(" unpin  ", theme.fg_dim),
        Span::styled("Ctrl+C", theme.fg_active),
        Span::styled(" copy", theme.fg_dim),
    ])
}

fn spawned_footer_line(theme: &ThemeTokens) -> Line<'static> {
    Line::from(vec![
        Span::styled(" Enter", theme.fg_active),
        Span::styled(" open child  ", theme.fg_dim),
        Span::styled("↑↓", theme.fg_active),
        Span::styled(" move", theme.fg_dim),
    ])
}

fn thread_history_footer_line(theme: &ThemeTokens, depth: usize) -> Line<'static> {
    Line::from(vec![
        Span::styled(" Backspace", theme.fg_active),
        Span::styled(" previous thread", theme.fg_dim),
        Span::styled(format!(" ({depth})"), theme.fg_dim),
    ])
}

fn row_from_snapshot(
    snapshot: &CachedSidebarSnapshot,
    index: usize,
    selected: usize,
    theme: &ThemeTokens,
    width: usize,
) -> Option<SidebarRow> {
    let selected_style = Style::default().bg(Color::Indexed(236));

    let row = match &snapshot.body {
        SidebarBodySnapshot::Empty { message } => SidebarRow {
            line: Line::from(Span::styled(message.clone(), theme.fg_dim)),
        },
        SidebarBodySnapshot::Files(items) => {
            let item = items.get(index)?;
            let line = Line::from(vec![
                Span::styled(
                    if index == selected { "> " } else { "  " },
                    theme.accent_primary,
                ),
                Span::styled(format!("[{}]", item.label), theme.fg_dim),
                Span::raw(" "),
                Span::styled(item.display_path.clone(), theme.fg_active),
            ]);
            SidebarRow {
                line: if index == selected {
                    line.style(selected_style)
                } else {
                    line
                },
            }
        }
        SidebarBodySnapshot::Todos(items) => {
            let item = items.get(index)?;
            let line = Line::from(vec![
                Span::styled(
                    if index == selected { "> " } else { "  " },
                    theme.accent_primary,
                ),
                Span::styled(item.marker, theme.fg_dim),
                Span::raw(" "),
                Span::styled(item.text.clone(), theme.fg_active),
            ]);
            SidebarRow {
                line: if index == selected {
                    line.style(selected_style)
                } else {
                    line
                },
            }
        }
        SidebarBodySnapshot::Spawned(items) => {
            let item = items.get(index)?;
            let indent = "  ".repeat(item.depth);
            let marker = if item.is_active {
                "@"
            } else if item.openable {
                ">"
            } else {
                "-"
            };
            let status = if item.live { "live" } else { "done" };
            let max_len = width
                .saturating_sub(indent.chars().count())
                .saturating_sub(12)
                .max(8);
            let title_style = if item.target_thread_id.is_none() && !item.is_active {
                theme.fg_dim
            } else {
                theme.fg_active
            };
            let line = Line::from(vec![
                Span::styled(
                    if index == selected { "> " } else { "  " },
                    theme.accent_primary,
                ),
                Span::raw(indent),
                Span::styled(format!("[{marker}]"), theme.fg_dim),
                Span::raw(" "),
                Span::styled(truncate_tail(&item.title, max_len), title_style),
                Span::styled(format!(" [{status}]"), theme.fg_dim),
            ]);
            SidebarRow {
                line: if index == selected {
                    line.style(selected_style)
                } else {
                    line
                },
            }
        }
        SidebarBodySnapshot::Pinned(items) => {
            let item = items.get(index)?;
            let line = Line::from(vec![
                Span::styled(
                    if index == selected { "> " } else { "  " },
                    theme.accent_primary,
                ),
                Span::styled(item.metadata.clone(), theme.fg_dim),
                Span::raw(" "),
                Span::styled(item.snippet.clone(), theme.fg_active),
            ]);
            SidebarRow {
                line: if index == selected {
                    line.style(selected_style)
                } else {
                    line
                },
            }
        }
    };

    Some(row)
}

fn visible_rows(
    snapshot: &CachedSidebarSnapshot,
    sidebar: &SidebarState,
    theme: &ThemeTokens,
    width: usize,
    body_height: usize,
    scroll: usize,
) -> Vec<SidebarRow> {
    let end = scroll
        .saturating_add(body_height)
        .min(snapshot.item_count());
    (scroll..end)
        .filter_map(|index| {
            row_from_snapshot(snapshot, index, sidebar.selected_item(), theme, width)
        })
        .collect()
}

pub fn has_spawned_tab(tasks: &TaskState, chat: &ChatState, thread_id: Option<&str>) -> bool {
    spawned_agents::has_content(tasks, thread_id) || chat.can_go_back_thread()
}

pub fn visible_tabs(
    tasks: &TaskState,
    chat: &ChatState,
    thread_id: Option<&str>,
) -> Vec<SidebarTab> {
    tab_layout::visible_tabs(
        has_spawned_tab(tasks, chat, thread_id),
        chat.active_thread_has_pinned_messages(),
    )
}

pub fn selected_spawned_thread_id(
    tasks: &TaskState,
    sidebar: &SidebarState,
    thread_id: Option<&str>,
) -> Option<String> {
    spawned_agents::selected_thread_id(tasks, sidebar.selected_item(), thread_id)
}

pub fn first_openable_spawned_index(tasks: &TaskState, thread_id: Option<&str>) -> Option<usize> {
    spawned_agents::first_openable_index(tasks, thread_id)
}

fn resolved_scroll(item_count: usize, sidebar: &SidebarState, body_height: usize) -> usize {
    let max_scroll = item_count.saturating_sub(body_height);
    let mut scroll = sidebar.scroll_offset().min(max_scroll);
    let selected = sidebar.selected_item().min(item_count.saturating_sub(1));
    if selected < scroll {
        scroll = selected;
    } else if selected >= scroll.saturating_add(body_height) {
        scroll = selected.saturating_add(1).saturating_sub(body_height);
    }
    scroll.min(max_scroll)
}

fn gateway_status_lines(statuses: &[GatewayStatusVm], theme: &ThemeTokens) -> Vec<Line<'static>> {
    // Only show gateway section if at least one platform is not disconnected
    let active: Vec<&GatewayStatusVm> = statuses
        .iter()
        .filter(|s| s.status != "disconnected")
        .collect();
    if active.is_empty() {
        return Vec::new();
    }

    let mut lines = Vec::new();
    lines.push(Line::from(Span::styled(
        " Gateway",
        Style::default()
            .fg(Color::Indexed(245))
            .add_modifier(ratatui::style::Modifier::BOLD),
    )));

    for gw in &active {
        let (indicator, color) = match gw.status.as_str() {
            "connected" => ("\u{25CF}", Color::Green),
            "error" => ("\u{25CF}", Color::Red),
            _ => ("\u{25CF}", Color::Indexed(245)),
        };
        let platform_label = match gw.platform.as_str() {
            "slack" => "Slack",
            "discord" => "Discord",
            "telegram" => "Telegram",
            other => other,
        };
        let mut spans = vec![
            Span::styled("  ", theme.fg_dim),
            Span::styled(indicator.to_string(), Style::default().fg(color)),
            Span::raw(" "),
            Span::styled(platform_label.to_string(), theme.fg_active),
        ];
        if gw.status == "error" {
            if let Some(ref err) = gw.last_error {
                let truncated: String = err.chars().take(30).collect();
                spans.push(Span::styled(
                    format!(" ({})", truncated),
                    Style::default().fg(Color::Red),
                ));
            }
        }
        lines.push(Line::from(spans));
    }
    lines
}

fn recent_actions_lines(actions: &[RecentActionVm], theme: &ThemeTokens) -> Vec<Line<'static>> {
    if actions.is_empty() {
        return Vec::new();
    }
    let mut lines = Vec::new();
    lines.push(Line::from(Span::styled(
        " Recent Actions",
        Style::default()
            .fg(Color::Indexed(245))
            .add_modifier(ratatui::style::Modifier::BOLD),
    )));
    for action in actions.iter().take(3) {
        let icon = match action.action_type.as_str() {
            "stale_todo" => "\u{2611}",    // ballot box with check
            "stuck_goal" => "\u{26A0}",    // warning
            "morning_brief" => "\u{2600}", // sun
            _ => "\u{25CB}",               // circle
        };
        let mut summary = action.summary.clone();
        if summary.chars().count() > 40 {
            summary = format!("{}...", summary.chars().take(37).collect::<String>());
        }
        lines.push(Line::from(vec![
            Span::styled("  ", theme.fg_dim),
            Span::styled(icon.to_string(), theme.fg_dim),
            Span::raw(" "),
            Span::styled(summary, theme.fg_active),
        ]));
    }
    lines
}

/// Render a dimmed one-line placeholder for a tier-locked sidebar section (D-05).
fn tier_placeholder_line(label: &str, required_tier: &str) -> Line<'static> {
    let dim = Style::default().fg(Color::DarkGray);
    Line::from(vec![
        Span::styled("  \u{25B6} ", dim),
        Span::styled(label.to_string(), dim),
        Span::styled(format!("  [{}]", required_tier.replace('_', " ")), dim),
    ])
}

/// Collect tier-gated placeholder lines for hidden sidebar sections.
fn tier_gated_lines(tier: &TierState) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    if !tier.show_goal_runs {
        lines.push(tier_placeholder_line("Goal Runs", "familiar"));
    }
    if !tier.show_task_queue {
        lines.push(tier_placeholder_line("Task Queue", "familiar"));
    }
    if !tier.show_gateway_config {
        lines.push(tier_placeholder_line("Gateway", "familiar"));
    }
    if !tier.show_subagents {
        lines.push(tier_placeholder_line("Sub-Agents", "power user"));
    }
    if !tier.show_memory_controls {
        lines.push(tier_placeholder_line("Memory", "expert"));
    }
    lines
}

#[cfg(test)]
fn agent_status_line(
    activity: Option<&str>,
    tier: &str,
    weles_health: Option<&crate::client::WelesHealthVm>,
) -> Line<'static> {
    let status_span = match activity {
        Some("thinking" | "reasoning" | "writing") => {
            Span::styled("\u{25CF} Thinking", Style::default().fg(Color::Yellow))
        }
        Some(s) if s.starts_with('\u{2699}') => {
            Span::styled(format!("\u{25CF} {}", s), Style::default().fg(Color::Blue))
        }
        Some("waiting_for_approval") => Span::styled(
            "\u{25CF} Awaiting approval",
            Style::default().fg(Color::Rgb(255, 165, 0)),
        ),
        Some("running_goal" | "goal_running") => {
            Span::styled("\u{25CF} Running goal", Style::default().fg(Color::Green))
        }
        Some("idle") | None => Span::styled("\u{25CF} Idle", Style::default().fg(Color::DarkGray)),
        Some(other) => Span::styled(
            format!("\u{25CF} {}", other),
            Style::default().fg(Color::DarkGray),
        ),
    };

    let tier_label = match tier {
        "newcomer" => "",
        "familiar" => " [familiar]",
        "power_user" => " [power user]",
        "expert" => " [expert]",
        _ => "",
    };

    let mut spans = vec![Span::raw(" "), status_span];
    if !tier_label.is_empty() {
        spans.push(Span::styled(
            tier_label.to_string(),
            Style::default().fg(Color::DarkGray),
        ));
    }
    if weles_health.is_some_and(|health| health.state.eq_ignore_ascii_case("degraded")) {
        spans.push(Span::styled(
            "  [WELES degraded]".to_string(),
            Style::default().fg(Color::LightYellow),
        ));
    }
    Line::from(spans)
}

pub fn render(
    frame: &mut Frame,
    area: Rect,
    chat: &ChatState,
    sidebar: &SidebarState,
    tasks: &TaskState,
    thread_id: Option<&str>,
    theme: &ThemeTokens,
    focused: bool,
    gateway_statuses: &[GatewayStatusVm],
    tier: &TierState,
    _agent_activity: Option<&str>,
    _weles_health: Option<&crate::client::WelesHealthVm>,
    recent_actions: &[RecentActionVm],
) {
    let snapshot = build_cached_snapshot(area, chat, sidebar, tasks, thread_id);
    render_cached(
        frame,
        area,
        chat,
        sidebar,
        theme,
        focused,
        gateway_statuses,
        tier,
        recent_actions,
        &snapshot,
    );
}

pub fn render_cached(
    frame: &mut Frame,
    area: Rect,
    chat: &ChatState,
    sidebar: &SidebarState,
    theme: &ThemeTokens,
    focused: bool,
    gateway_statuses: &[GatewayStatusVm],
    tier: &TierState,
    recent_actions: &[RecentActionVm],
    snapshot: &CachedSidebarSnapshot,
) {
    if area.height < 3 {
        return;
    }

    let gw_lines = if tier.show_gateway_config {
        gateway_status_lines(gateway_statuses, theme)
    } else {
        Vec::new()
    };
    let gw_height = gw_lines.len() as u16;
    let show_spawned = snapshot.show_spawned();
    let show_pinned = snapshot.show_pinned();
    let filter_height = if sidebar.active_tab() == SidebarTab::Files {
        1
    } else {
        0
    };
    let mut footer_lines = Vec::new();
    if chat.can_go_back_thread() {
        footer_lines.push(thread_history_footer_line(
            theme,
            chat.thread_navigation_depth(),
        ));
    }
    if sidebar.active_tab() == SidebarTab::Spawned {
        footer_lines.push(spawned_footer_line(theme));
    }
    if sidebar.active_tab() == SidebarTab::Pinned {
        footer_lines.push(pinned_footer_line(theme));
    }
    let footer_height = footer_lines.len() as u16;

    let ra_lines = recent_actions_lines(recent_actions, theme);
    let ra_height = ra_lines.len() as u16;

    let tier_lines = tier_gated_lines(tier);
    let tier_height = tier_lines.len() as u16;

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // tab bar
            Constraint::Length(filter_height),
            Constraint::Min(1), // body
            Constraint::Length(gw_height),
            Constraint::Length(ra_height),
            Constraint::Length(tier_height),
            Constraint::Length(footer_height),
        ])
        .split(area);

    // Agent status line at the very top

    for (tab, cell) in tab_cells(chunks[0], show_spawned, show_pinned) {
        let style = if sidebar.active_tab() == tab {
            theme.fg_active.bg(Color::Indexed(236))
        } else {
            theme.fg_dim
        };
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(tab_label(tab), style)))
                .alignment(Alignment::Center),
            cell,
        );
    }

    if filter_height > 0 {
        let filter_text = if sidebar.files_filter().is_empty() {
            " Filter: type to search".to_string()
        } else {
            format!(" Filter: {}", sidebar.files_filter())
        };
        let style = if focused && sidebar.active_tab() == SidebarTab::Files {
            theme.fg_active.bg(Color::Indexed(236))
        } else {
            theme.fg_dim
        };
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(filter_text, style))),
            chunks[1],
        );
    }

    let body_idx = 2;
    let scroll = resolved_scroll(
        snapshot.item_count(),
        sidebar,
        chunks[body_idx].height as usize,
    );
    let rows = visible_rows(
        snapshot,
        sidebar,
        theme,
        chunks[body_idx].width as usize,
        chunks[body_idx].height as usize,
        scroll,
    );
    let paragraph = Paragraph::new(rows.into_iter().map(|row| row.line).collect::<Vec<_>>());
    frame.render_widget(paragraph, chunks[body_idx]);

    if !gw_lines.is_empty() {
        frame.render_widget(Paragraph::new(gw_lines), chunks[body_idx + 1]);
    }

    if !ra_lines.is_empty() {
        frame.render_widget(Paragraph::new(ra_lines), chunks[body_idx + 2]);
    }

    if !tier_lines.is_empty() {
        frame.render_widget(Paragraph::new(tier_lines), chunks[body_idx + 3]);
    }

    if footer_height > 0 {
        frame.render_widget(Paragraph::new(footer_lines), chunks[body_idx + 4]);
    }
}

pub fn body_item_count(
    tasks: &TaskState,
    chat: &ChatState,
    sidebar: &SidebarState,
    thread_id: Option<&str>,
) -> usize {
    build_cached_snapshot(Rect::new(0, 0, 80, 0), chat, sidebar, tasks, thread_id).item_count()
}

pub fn hit_test_cached(
    area: Rect,
    sidebar: &SidebarState,
    snapshot: &CachedSidebarSnapshot,
    mouse: Position,
) -> Option<SidebarHitTarget> {
    if area.height < 3
        || mouse.x < area.x
        || mouse.x >= area.x.saturating_add(area.width)
        || mouse.y < area.y
        || mouse.y >= area.y.saturating_add(area.height)
    {
        return None;
    }

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // tab bar
            Constraint::Length(if sidebar.active_tab() == SidebarTab::Files {
                1
            } else {
                0
            }),
            Constraint::Min(1), // body
        ])
        .split(area);

    if mouse.y == chunks[0].y {
        return tab_hit_test(
            chunks[0],
            mouse.x,
            snapshot.show_spawned(),
            snapshot.show_pinned(),
        )
        .map(SidebarHitTarget::Tab);
    }

    if sidebar.active_tab() == SidebarTab::Files && mouse.y == chunks[1].y {
        return None;
    }
    let body_idx = 2;
    let scroll = resolved_scroll(
        snapshot.item_count(),
        sidebar,
        chunks[body_idx].height as usize,
    );
    let row_idx = scroll + mouse.y.saturating_sub(chunks[body_idx].y) as usize;
    snapshot.row_target(row_idx)
}

pub fn hit_test(
    area: Rect,
    chat: &ChatState,
    sidebar: &SidebarState,
    tasks: &TaskState,
    thread_id: Option<&str>,
    mouse: Position,
) -> Option<SidebarHitTarget> {
    let snapshot = build_cached_snapshot(area, chat, sidebar, tasks, thread_id);
    hit_test_cached(area, sidebar, &snapshot, mouse)
}

#[cfg(test)]
pub fn reset_build_cached_snapshot_call_count() {
    BUILD_CACHED_SNAPSHOT_CALLS.with(|calls| calls.set(0));
}

#[cfg(test)]
pub fn build_cached_snapshot_call_count() -> usize {
    BUILD_CACHED_SNAPSHOT_CALLS.with(std::cell::Cell::get)
}

#[cfg(test)]
pub fn reset_spawned_sidebar_flatten_call_count() {
    spawned_agents::reset_flattened_items_call_count();
}

#[cfg(test)]
pub fn spawned_sidebar_flatten_call_count() -> usize {
    spawned_agents::flattened_items_call_count()
}

#[cfg(test)]
#[path = "tests/sidebar.rs"]
mod tests;
