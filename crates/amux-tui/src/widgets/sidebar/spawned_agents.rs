use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};

use crate::state::spawned_tree::{derive_spawned_agent_tree, SpawnedAgentTreeNode};
use crate::state::task::{AgentTask, TaskState};
use crate::theme::ThemeTokens;

use super::{SidebarHitTarget, SidebarRow};

#[derive(Debug, Clone)]
struct SpawnedSidebarItem {
    depth: usize,
    title: String,
    thread_id: Option<String>,
    is_active: bool,
    openable: bool,
    live: bool,
}

fn push_node_rows(
    rows: &mut Vec<SpawnedSidebarItem>,
    node: &SpawnedAgentTreeNode<AgentTask>,
    depth: usize,
    active_thread_id: &str,
) {
    rows.push(SpawnedSidebarItem {
        depth,
        title: node.item.title.clone(),
        thread_id: node.item.thread_id.clone(),
        is_active: node.item.thread_id.as_deref() == Some(active_thread_id),
        openable: node.openable && node.item.thread_id.as_deref() != Some(active_thread_id),
        live: node.live,
    });

    for child in &node.children {
        push_node_rows(rows, child, depth + 1, active_thread_id);
    }
}

fn flattened_items(tasks: &TaskState, thread_id: Option<&str>) -> Vec<SpawnedSidebarItem> {
    let Some(tree) = derive_spawned_agent_tree(tasks.spawned_tree_items(), thread_id) else {
        return Vec::new();
    };

    let mut rows = Vec::new();
    if let Some(anchor) = tree.anchor.as_ref() {
        push_node_rows(&mut rows, anchor, 0, tree.active_thread_id.as_str());
        for root in &tree.roots {
            push_node_rows(&mut rows, root, 1, tree.active_thread_id.as_str());
        }
    } else {
        for root in &tree.roots {
            push_node_rows(&mut rows, root, 0, tree.active_thread_id.as_str());
        }
    }
    rows
}

fn truncated_title(title: &str, max_len: usize) -> String {
    if title.chars().count() > max_len {
        format!(
            "{}…",
            title
                .chars()
                .take(max_len.saturating_sub(1))
                .collect::<String>()
        )
    } else {
        title.to_string()
    }
}

pub(super) fn has_content(tasks: &TaskState, thread_id: Option<&str>) -> bool {
    !flattened_items(tasks, thread_id).is_empty()
}

pub(super) fn selected_thread_id(
    tasks: &TaskState,
    selected_index: usize,
    thread_id: Option<&str>,
) -> Option<String> {
    let items = flattened_items(tasks, thread_id);
    items
        .get(selected_index)
        .filter(|item| item.openable)
        .and_then(|item| item.thread_id.clone())
        .or_else(|| {
            items
                .into_iter()
                .find(|item| item.openable)
                .and_then(|item| item.thread_id)
        })
}

pub(super) fn first_openable_index(tasks: &TaskState, thread_id: Option<&str>) -> Option<usize> {
    flattened_items(tasks, thread_id)
        .iter()
        .position(|item| item.openable)
}

pub(super) fn item_count(tasks: &TaskState, thread_id: Option<&str>) -> usize {
    flattened_items(tasks, thread_id).len().max(1)
}

pub(super) fn rows(
    tasks: &TaskState,
    selected_index: usize,
    thread_id: Option<&str>,
    theme: &ThemeTokens,
    width: usize,
) -> Vec<SidebarRow> {
    let items = flattened_items(tasks, thread_id);
    if items.is_empty() {
        return vec![SidebarRow {
            line: Line::from(Span::styled(" No spawned agents", theme.fg_dim)),
            target: None,
        }];
    }

    let selected_style = Style::default().bg(Color::Indexed(236));
    items
        .into_iter()
        .enumerate()
        .map(|(idx, item)| {
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
            let line = Line::from(vec![
                Span::styled(
                    if idx == selected_index { "> " } else { "  " },
                    theme.accent_primary,
                ),
                Span::raw(indent),
                Span::styled(format!("[{marker}]"), theme.fg_dim),
                Span::raw(" "),
                Span::styled(truncated_title(&item.title, max_len), theme.fg_active),
                Span::styled(format!(" [{status}]"), theme.fg_dim),
            ]);

            SidebarRow {
                line: if idx == selected_index {
                    line.style(selected_style)
                } else {
                    line
                },
                target: Some(SidebarHitTarget::Spawned(idx)),
            }
        })
        .collect()
}
