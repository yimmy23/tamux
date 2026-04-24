use ratatui::prelude::*;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, List, ListItem, Paragraph};

use amux_protocol::{AGENT_NAME_RAROG, AGENT_NAME_SWAROG};

use crate::state::chat::{AgentThread, ChatState};
use crate::state::modal::{ModalState, ThreadPickerTab};
use crate::state::subagents::SubAgentsState;
use crate::state::task::TaskState;
use crate::theme::ThemeTokens;
use crate::widgets::token_format::format_token_count;

const TAB_GAP: u16 = 1;
const INTERNAL_DM_THREAD_PREFIX: &str = "dm:";
const INTERNAL_DM_TITLE_PREFIX: &str = "Internal DM";
const HIDDEN_HANDOFF_THREAD_PREFIX: &str = "handoff:";
const GOAL_THREAD_PREFIX: &str = "goal:";
const PLAYGROUND_THREAD_PREFIX: &str = "playground:";
const PLAYGROUND_THREAD_TITLE_PREFIX: &str = "Participant Playground";
const WELES_THREAD_TITLE: &str = "WELES";
const GATEWAY_THREAD_TITLE_PREFIXES: [&str; 4] = ["slack ", "discord ", "telegram ", "whatsapp "];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ThreadPickerHitTarget {
    Tab(ThreadPickerTab),
    Item(usize),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ThreadPickerTabSpec {
    pub(crate) tab: ThreadPickerTab,
    pub(crate) label: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ThreadPickerTabCell {
    tab: ThreadPickerTab,
    label: String,
    start: u16,
}

#[derive(Debug, Clone, Default)]
struct GoalThreadIndex {
    ids: std::collections::BTreeSet<String>,
}

impl GoalThreadIndex {
    fn from_tasks(tasks: &TaskState) -> Self {
        let ids = tasks.all_goal_thread_ids().into_iter().collect();
        Self { ids }
    }

    fn contains_id(&self, thread_id: &str) -> bool {
        self.ids.contains(thread_id)
    }

    fn contains_thread(&self, thread: &AgentThread) -> bool {
        is_goal_thread(thread) || self.contains_id(&thread.id)
    }
}

fn thread_picker_layout(inner: Rect) -> [Rect; 5] {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // tabs
            Constraint::Length(1), // search
            Constraint::Length(1), // separator
            Constraint::Min(1),    // list
            Constraint::Length(1), // hints
        ])
        .split(inner);
    [chunks[0], chunks[1], chunks[2], chunks[3], chunks[4]]
}

fn fixed_tab_specs() -> Vec<ThreadPickerTabSpec> {
    vec![
        ThreadPickerTabSpec {
            tab: ThreadPickerTab::Swarog,
            label: format!("[{AGENT_NAME_SWAROG}]"),
        },
        ThreadPickerTabSpec {
            tab: ThreadPickerTab::Rarog,
            label: format!("[{AGENT_NAME_RAROG}]"),
        },
        ThreadPickerTabSpec {
            tab: ThreadPickerTab::Weles,
            label: "[Weles]".to_string(),
        },
        ThreadPickerTabSpec {
            tab: ThreadPickerTab::Goals,
            label: "[Goals]".to_string(),
        },
        ThreadPickerTabSpec {
            tab: ThreadPickerTab::Playgrounds,
            label: "[Playgrounds]".to_string(),
        },
        ThreadPickerTabSpec {
            tab: ThreadPickerTab::Internal,
            label: "[Internal]".to_string(),
        },
        ThreadPickerTabSpec {
            tab: ThreadPickerTab::Gateway,
            label: "[Gateway]".to_string(),
        },
    ]
}

fn normalize_agent_tab_id(value: &str) -> Option<String> {
    let normalized = value.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "" | "svarog" | "swarog" | "main" | "rarog" | "concierge" | "weles" => None,
        other => Some(other.to_string()),
    }
}

fn display_name_for_agent_id(
    agent_id: &str,
    chat: &ChatState,
    subagents: &SubAgentsState,
) -> String {
    if let Some(entry) = subagents.entries.iter().find(|entry| {
        entry.id.eq_ignore_ascii_case(agent_id)
            || entry
                .id
                .strip_suffix("_builtin")
                .is_some_and(|alias| alias.eq_ignore_ascii_case(agent_id))
    }) {
        return entry.name.clone();
    }

    chat.threads()
        .iter()
        .filter_map(|thread| thread.agent_name.as_deref())
        .find(|name| name.eq_ignore_ascii_case(agent_id))
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| {
            let mut chars = agent_id.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => agent_id.to_string(),
            }
        })
}

fn tab_specs_inner(
    chat: &ChatState,
    subagents: &SubAgentsState,
    goal_index: Option<&GoalThreadIndex>,
) -> Vec<ThreadPickerTabSpec> {
    let mut specs = fixed_tab_specs();
    let mut dynamic_agents = std::collections::BTreeSet::new();

    for entry in &subagents.entries {
        if !entry.builtin {
            if let Some(agent_id) = normalize_agent_tab_id(&entry.id) {
                dynamic_agents.insert(agent_id);
            }
        }
    }

    for thread in chat.threads() {
        if is_hidden_handoff_thread(thread)
            || is_internal_thread(thread)
            || is_gateway_thread(thread)
            || is_goal_thread_with_index(thread, goal_index)
            || is_playground_thread(thread)
            || is_rarog_thread(thread)
            || is_weles_thread(thread)
        {
            continue;
        }

        if let Some(agent_name) = thread.agent_name.as_deref() {
            if let Some(agent_id) = normalize_agent_tab_id(agent_name) {
                dynamic_agents.insert(agent_id);
            }
        }
    }

    specs.extend(
        dynamic_agents
            .into_iter()
            .map(|agent_id| ThreadPickerTabSpec {
                label: format!(
                    "[{}]",
                    display_name_for_agent_id(&agent_id, chat, subagents)
                ),
                tab: ThreadPickerTab::Agent(agent_id),
            }),
    );
    specs
}

pub(crate) fn tab_specs(chat: &ChatState, subagents: &SubAgentsState) -> Vec<ThreadPickerTabSpec> {
    tab_specs_inner(chat, subagents, None)
}

pub(crate) fn tab_specs_for_tasks(
    chat: &ChatState,
    subagents: &SubAgentsState,
    tasks: &TaskState,
) -> Vec<ThreadPickerTabSpec> {
    let goal_index = GoalThreadIndex::from_tasks(tasks);
    tab_specs_inner(chat, subagents, Some(&goal_index))
}

fn thread_matches_agent_tab(
    thread: &AgentThread,
    agent_id: &str,
    subagents: &SubAgentsState,
    goal_index: Option<&GoalThreadIndex>,
) -> bool {
    if is_hidden_handoff_thread(thread)
        || is_internal_thread(thread)
        || is_gateway_thread(thread)
        || is_goal_thread_with_index(thread, goal_index)
        || is_playground_thread(thread)
        || is_rarog_thread(thread)
        || is_weles_thread(thread)
    {
        return false;
    }

    let normalized_agent = agent_id.trim().to_ascii_lowercase();
    thread
        .agent_name
        .as_deref()
        .and_then(normalize_agent_tab_id)
        .is_some_and(|thread_agent| thread_agent == normalized_agent)
        || subagents.entries.iter().any(|entry| {
            normalize_agent_tab_id(&entry.id).is_some_and(|entry_id| entry_id == normalized_agent)
                && thread
                    .agent_name
                    .as_deref()
                    .is_some_and(|name| name.eq_ignore_ascii_case(&entry.name))
        })
}

pub(crate) fn resolve_thread_picker_tab(
    agent_alias: &str,
    chat: &ChatState,
    subagents: &SubAgentsState,
) -> Option<ThreadPickerTab> {
    let normalized = agent_alias.trim().to_ascii_lowercase();
    let fixed = match normalized.as_str() {
        "svarog" | "swarog" | "main" => Some(ThreadPickerTab::Swarog),
        "rarog" | "concierge" => Some(ThreadPickerTab::Rarog),
        "weles" => Some(ThreadPickerTab::Weles),
        "goals" | "goal" => Some(ThreadPickerTab::Goals),
        "playgrounds" | "playground" => Some(ThreadPickerTab::Playgrounds),
        "internal" => Some(ThreadPickerTab::Internal),
        "gateway" => Some(ThreadPickerTab::Gateway),
        _ => None,
    };
    if fixed.is_some() {
        return fixed;
    }

    if let Some(entry) = subagents.entries.iter().find(|entry| {
        entry.id.eq_ignore_ascii_case(agent_alias)
            || entry.name.eq_ignore_ascii_case(agent_alias)
            || entry
                .id
                .strip_suffix("_builtin")
                .is_some_and(|alias| alias.eq_ignore_ascii_case(agent_alias))
    }) {
        if let Some(agent_id) =
            normalize_agent_tab_id(entry.id.strip_suffix("_builtin").unwrap_or(&entry.id))
        {
            return Some(ThreadPickerTab::Agent(agent_id));
        }
    }

    chat.threads()
        .iter()
        .filter_map(|thread| thread.agent_name.as_deref())
        .find(|name| name.eq_ignore_ascii_case(agent_alias))
        .and_then(normalize_agent_tab_id)
        .map(ThreadPickerTab::Agent)
}

pub(crate) fn adjacent_thread_picker_tab_for_tasks(
    current: &ThreadPickerTab,
    chat: &ChatState,
    subagents: &SubAgentsState,
    tasks: &TaskState,
    direction: i32,
) -> ThreadPickerTab {
    let goal_index = GoalThreadIndex::from_tasks(tasks);
    adjacent_thread_picker_tab_inner(current, chat, subagents, Some(&goal_index), direction)
}

fn adjacent_thread_picker_tab_inner(
    current: &ThreadPickerTab,
    chat: &ChatState,
    subagents: &SubAgentsState,
    goal_index: Option<&GoalThreadIndex>,
    direction: i32,
) -> ThreadPickerTab {
    let specs = tab_specs_inner(chat, subagents, goal_index);
    let current_index = specs
        .iter()
        .position(|spec| &spec.tab == current)
        .unwrap_or(0);
    let next_index = if direction < 0 {
        current_index
            .checked_sub(1)
            .unwrap_or(specs.len().saturating_sub(1))
    } else {
        (current_index + 1) % specs.len().max(1)
    };
    specs
        .get(next_index)
        .map(|spec| spec.tab.clone())
        .unwrap_or_default()
}

fn thread_matches_query(
    thread: &AgentThread,
    query: &str,
    goal_index: Option<&GoalThreadIndex>,
) -> bool {
    if query.is_empty() {
        return true;
    }
    let lower = query.to_lowercase();
    thread.title.to_lowercase().contains(&lower)
        || thread_display_title_inner(thread, goal_index)
            .to_lowercase()
            .contains(&lower)
        || thread
            .agent_name
            .as_deref()
            .is_some_and(|name| name.to_lowercase().contains(&lower))
}

pub(crate) fn is_rarog_thread(thread: &AgentThread) -> bool {
    thread.id == "concierge"
        || thread
            .agent_name
            .as_deref()
            .is_some_and(|name| name.eq_ignore_ascii_case(AGENT_NAME_RAROG))
        || thread.title.eq_ignore_ascii_case("concierge")
        || thread.title.starts_with("HEARTBEAT SYNTHESIS")
        || thread.title.starts_with("Heartbeat check:")
}

pub(crate) fn is_internal_thread(thread: &AgentThread) -> bool {
    thread.id.starts_with(INTERNAL_DM_THREAD_PREFIX)
        || thread.title.starts_with(INTERNAL_DM_TITLE_PREFIX)
}

pub(crate) fn is_gateway_thread(thread: &AgentThread) -> bool {
    !is_internal_thread(thread)
        && GATEWAY_THREAD_TITLE_PREFIXES
            .iter()
            .any(|prefix| thread.title.trim().to_ascii_lowercase().starts_with(prefix))
}

pub(crate) fn is_playground_thread(thread: &AgentThread) -> bool {
    thread.id.starts_with(PLAYGROUND_THREAD_PREFIX)
        || thread.title.starts_with(PLAYGROUND_THREAD_TITLE_PREFIX)
}

pub(crate) fn is_goal_thread(thread: &AgentThread) -> bool {
    thread.id.starts_with(GOAL_THREAD_PREFIX)
}

fn is_goal_thread_with_index(thread: &AgentThread, goal_index: Option<&GoalThreadIndex>) -> bool {
    is_goal_thread(thread) || goal_index.is_some_and(|index| index.contains_thread(thread))
}

fn is_hidden_handoff_thread(thread: &AgentThread) -> bool {
    thread.id.starts_with(HIDDEN_HANDOFF_THREAD_PREFIX)
        || thread
            .title
            .trim()
            .to_ascii_lowercase()
            .starts_with("handoff ")
}

pub(crate) fn is_weles_thread(thread: &AgentThread) -> bool {
    !is_internal_thread(thread)
        && (thread
            .agent_name
            .as_deref()
            .is_some_and(|name| name.eq_ignore_ascii_case("weles"))
            || thread.title.contains(WELES_THREAD_TITLE)
            || thread.messages.iter().any(|message| {
                message.content.lines().any(|line| {
                    let Some((marker, value)) = line.split_once(':') else {
                        return false;
                    };
                    if marker.trim() != "Agent persona id" {
                        return false;
                    }
                    matches!(
                        value.trim().to_ascii_lowercase().as_str(),
                        "weles" | "governance" | "vitality"
                    )
                })
            }))
}

fn is_svarog_thread(thread: &AgentThread, subagents: &SubAgentsState) -> bool {
    let Some(agent_name) = thread.agent_name.as_deref() else {
        return true;
    };
    let Some(agent_id) = normalize_agent_tab_id(agent_name) else {
        return true;
    };

    !subagents.entries.iter().any(|entry| {
        normalize_agent_tab_id(&entry.id).is_some_and(|entry_id| entry_id == agent_id)
            || entry.name.eq_ignore_ascii_case(agent_name)
    })
}

pub(crate) fn thread_display_title(thread: &AgentThread) -> String {
    thread_display_title_inner(thread, None)
}

pub(crate) fn thread_display_title_for_tasks(thread: &AgentThread, tasks: &TaskState) -> String {
    let goal_index = GoalThreadIndex::from_tasks(tasks);
    thread_display_title_inner(thread, Some(&goal_index))
}

fn thread_display_title_inner(
    thread: &AgentThread,
    goal_index: Option<&GoalThreadIndex>,
) -> String {
    if thread.id == "concierge" || thread.title.eq_ignore_ascii_case("concierge") {
        AGENT_NAME_RAROG.to_string()
    } else if is_goal_thread_with_index(thread, goal_index) {
        let role = thread
            .agent_name
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or("Goal");
        format!("goal: {role} · {}", thread.title)
    } else {
        thread.title.clone()
    }
}

pub(crate) fn filtered_threads<'a>(
    chat: &'a ChatState,
    modal: &ModalState,
    subagents: &SubAgentsState,
) -> Vec<&'a AgentThread> {
    filtered_threads_inner(chat, modal, subagents, None)
}

pub(crate) fn filtered_threads_for_tasks<'a>(
    chat: &'a ChatState,
    modal: &ModalState,
    subagents: &SubAgentsState,
    tasks: &TaskState,
) -> Vec<&'a AgentThread> {
    let goal_index = GoalThreadIndex::from_tasks(tasks);
    filtered_threads_inner(chat, modal, subagents, Some(&goal_index))
}

fn filtered_threads_inner<'a>(
    chat: &'a ChatState,
    modal: &ModalState,
    subagents: &SubAgentsState,
    goal_index: Option<&GoalThreadIndex>,
) -> Vec<&'a AgentThread> {
    let query = modal.command_query();
    chat.threads()
        .iter()
        .filter(|thread| !is_hidden_handoff_thread(thread))
        .filter(|thread| match modal.thread_picker_tab() {
            ThreadPickerTab::Swarog => {
                !is_rarog_thread(thread)
                    && !is_internal_thread(thread)
                    && !is_gateway_thread(thread)
                    && !is_weles_thread(thread)
                    && !is_goal_thread_with_index(thread, goal_index)
                    && !is_playground_thread(thread)
                    && is_svarog_thread(thread, subagents)
            }
            ThreadPickerTab::Rarog => is_rarog_thread(thread),
            ThreadPickerTab::Weles => !is_playground_thread(thread) && is_weles_thread(thread),
            ThreadPickerTab::Goals => is_goal_thread_with_index(thread, goal_index),
            ThreadPickerTab::Playgrounds => is_playground_thread(thread),
            ThreadPickerTab::Internal => is_internal_thread(thread),
            ThreadPickerTab::Gateway => is_gateway_thread(thread),
            ThreadPickerTab::Agent(agent_id) => {
                thread_matches_agent_tab(thread, &agent_id, subagents, goal_index)
            }
        })
        .filter(|thread| thread_matches_query(thread, query, goal_index))
        .collect()
}

fn tab_cells(chat: &ChatState, subagents: &SubAgentsState) -> Vec<ThreadPickerTabCell> {
    tab_cells_inner(chat, subagents, None)
}

fn tab_cells_inner(
    chat: &ChatState,
    subagents: &SubAgentsState,
    goal_index: Option<&GoalThreadIndex>,
) -> Vec<ThreadPickerTabCell> {
    let mut x = 0;
    tab_specs_inner(chat, subagents, goal_index)
        .into_iter()
        .map(|spec| {
            let tab = spec.tab;
            let label = spec.label;
            let cell = ThreadPickerTabCell {
                tab,
                label,
                start: x,
            };
            x = x.saturating_add(cell.label.chars().count() as u16 + TAB_GAP);
            cell
        })
        .collect()
}

fn tab_scroll_offset(
    area_width: u16,
    cells: &[ThreadPickerTabCell],
    selected: &ThreadPickerTab,
) -> u16 {
    if area_width == 0 {
        return 0;
    }

    let Some(selected_cell) = cells.iter().find(|cell| &cell.tab == selected) else {
        return 0;
    };
    let total_width = cells
        .last()
        .map(|cell| cell.start.saturating_add(cell.label.chars().count() as u16))
        .unwrap_or(0);
    let max_offset = total_width.saturating_sub(area_width);
    let selected_width = selected_cell.label.chars().count() as u16;
    let desired_offset = if selected_width >= area_width {
        selected_cell.start
    } else {
        selected_cell
            .start
            .saturating_add(selected_width)
            .saturating_sub(area_width)
    };

    desired_offset.min(max_offset)
}

fn visible_tab_cells(
    area: Rect,
    cells: &[ThreadPickerTabCell],
    scroll: u16,
) -> Vec<(ThreadPickerTab, Rect, String)> {
    let viewport_end = scroll.saturating_add(area.width);

    cells
        .iter()
        .filter_map(|cell| {
            let cell_width = cell.label.chars().count() as u16;
            let cell_end = cell.start.saturating_add(cell_width);
            let visible_start = cell.start.max(scroll);
            let visible_end = cell_end.min(viewport_end);

            if visible_start >= visible_end {
                return None;
            }

            let skip = visible_start.saturating_sub(cell.start) as usize;
            let width = visible_end.saturating_sub(visible_start);
            let label = cell.label.chars().skip(skip).take(width as usize).collect();
            let rect = Rect::new(
                area.x.saturating_add(visible_start.saturating_sub(scroll)),
                area.y,
                width,
                area.height,
            );

            Some((cell.tab.clone(), rect, label))
        })
        .collect()
}

pub(crate) fn visible_window(
    cursor: usize,
    item_count: usize,
    list_height: usize,
) -> (usize, usize) {
    if item_count == 0 || list_height == 0 {
        return (0, 0);
    }

    let height = list_height.min(item_count);
    let max_start = item_count.saturating_sub(height);
    let start = cursor
        .saturating_sub(height.saturating_sub(1))
        .min(max_start);
    (start, height)
}

fn synthetic_row_label(tab: ThreadPickerTab) -> &'static str {
    match tab {
        ThreadPickerTab::Goals => "Goal threads are created automatically",
        ThreadPickerTab::Playgrounds => "Playgrounds are created automatically",
        ThreadPickerTab::Gateway => "Gateway threads are created automatically",
        ThreadPickerTab::Agent(_) => "+ New conversation",
        _ => "+ New conversation",
    }
}

pub fn render_for_tasks(
    frame: &mut Frame,
    area: Rect,
    chat: &ChatState,
    modal: &ModalState,
    subagents: &SubAgentsState,
    tasks: &TaskState,
    theme: &ThemeTokens,
) {
    let block = Block::default()
        .title(" THREADS ")
        .borders(Borders::ALL)
        .border_type(BorderType::Double)
        .border_style(theme.accent_secondary);

    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.height < 5 {
        return;
    }

    let [tabs_row, search_row, separator_row, list_row, hints_row] = thread_picker_layout(inner);
    let goal_index = GoalThreadIndex::from_tasks(tasks);

    let tab_cells = tab_cells_inner(chat, subagents, Some(&goal_index));
    let selected_tab = modal.thread_picker_tab();
    let tab_scroll = tab_scroll_offset(tabs_row.width, &tab_cells, &selected_tab);
    let tab_line = Line::from(
        tab_cells
            .iter()
            .enumerate()
            .flat_map(|(index, cell)| {
                let style = if cell.tab == selected_tab {
                    theme.accent_primary
                } else {
                    theme.fg_dim
                };
                let mut spans = vec![Span::styled(cell.label.clone(), style)];
                if index + 1 < tab_cells.len() {
                    spans.push(Span::raw(" "));
                }
                spans
            })
            .collect::<Vec<_>>(),
    );
    frame.render_widget(Paragraph::new(tab_line).scroll((0, tab_scroll)), tabs_row);

    // Search input
    let query = modal.command_query();
    let input_line = Line::from(vec![
        Span::raw(" "),
        Span::styled(
            if query.is_empty() {
                "Search threads..."
            } else {
                query
            },
            theme.fg_active,
        ),
        if query.is_empty() {
            Span::raw("")
        } else {
            Span::raw("\u{2588}")
        },
    ]);
    frame.render_widget(Paragraph::new(input_line), search_row);

    // Separator
    let sep = Line::from(Span::styled(
        "\u{2500}".repeat(separator_row.width as usize),
        theme.fg_dim,
    ));
    frame.render_widget(Paragraph::new(sep), separator_row);

    // Build thread list
    let active_id = chat.active_thread_id();
    let filtered_threads = filtered_threads_inner(chat, modal, subagents, Some(&goal_index));

    let cursor = modal.picker_cursor();
    let list_h = list_row.height as usize;
    let inner_w = inner.width as usize;
    let total_items = filtered_threads.len() + 1;
    let (visible_start, visible_len) = visible_window(cursor, total_items, list_h);

    let list_items: Vec<ListItem> = (0..list_h)
        .map(|i| {
            if i < visible_len {
                let absolute_index = visible_start + i;
                if absolute_index == 0 {
                    let row_label = synthetic_row_label(modal.thread_picker_tab());
                    let is_selected = cursor == 0;
                    if is_selected {
                        ListItem::new(Line::from(vec![Span::raw(format!("  {row_label}"))]))
                            .style(Style::default().bg(Color::Indexed(178)).fg(Color::Black))
                    } else {
                        ListItem::new(Line::from(vec![
                            Span::raw("  "),
                            Span::styled(row_label, theme.fg_dim),
                        ]))
                    }
                } else {
                    let thread_idx = absolute_index - 1;
                    if thread_idx < filtered_threads.len() {
                        let thread = filtered_threads[thread_idx];
                        let is_selected = cursor == absolute_index;
                        let is_active = active_id == Some(thread.id.as_str());

                        let dot_style = if is_active {
                            theme.accent_success
                        } else {
                            theme.fg_dim
                        };

                        let time_str = format_time_ago(thread.updated_at);
                        let tokens = thread.total_input_tokens + thread.total_output_tokens;
                        let token_str = format_tokens(tokens);

                        let display_title = thread_display_title_inner(thread, Some(&goal_index));
                        let max_title = inner_w.saturating_sub(25);
                        let title = if display_title.chars().count() > max_title && max_title > 3 {
                            format!(
                                "{}...",
                                display_title
                                    .chars()
                                    .take(max_title - 3)
                                    .collect::<String>()
                            )
                        } else {
                            display_title
                        };

                        if is_selected {
                            ListItem::new(Line::from(vec![
                                Span::styled("\u{25cf}", dot_style),
                                Span::raw(" "),
                                Span::raw(title),
                                Span::raw("  "),
                                Span::raw(time_str),
                                Span::raw("  "),
                                Span::raw(token_str),
                            ]))
                            .style(Style::default().bg(Color::Indexed(178)).fg(Color::Black))
                        } else {
                            ListItem::new(Line::from(vec![
                                Span::raw("  "),
                                Span::styled("\u{25cf}", dot_style),
                                Span::raw(" "),
                                Span::styled(title, theme.fg_active),
                                Span::raw("  "),
                                Span::styled(time_str, theme.fg_dim),
                                Span::raw("  "),
                                Span::styled(token_str, theme.fg_dim),
                            ]))
                        }
                    } else {
                        ListItem::new(Line::raw(""))
                    }
                }
            } else {
                ListItem::new(Line::raw(""))
            }
        })
        .collect();

    let list = List::new(list_items);
    frame.render_widget(list, list_row);

    // Hints
    let mut hints = vec![
        Span::raw(" "),
        Span::styled("↑↓", theme.fg_active),
        Span::styled(" navigate  ", theme.fg_dim),
        Span::styled("←→", theme.fg_active),
        Span::styled(" source  ", theme.fg_dim),
        Span::styled("Enter", theme.fg_active),
        Span::styled(" select  ", theme.fg_dim),
    ];
    if cursor > 0 {
        hints.push(Span::styled("Del", theme.fg_active));
        hints.push(Span::styled(" delete  ", theme.fg_dim));
        hints.push(Span::styled("Ctrl+S", theme.fg_active));
        hints.push(Span::styled(" stop/resume  ", theme.fg_dim));
    }
    hints.push(Span::styled("Shift+R", theme.fg_active));
    hints.push(Span::styled(" refresh  ", theme.fg_dim));
    hints.push(Span::styled("Esc", theme.fg_active));
    hints.push(Span::styled(" close", theme.fg_dim));
    let hints = Line::from(hints);
    frame.render_widget(Paragraph::new(hints), hints_row);
}

pub fn hit_test(
    area: Rect,
    chat: &ChatState,
    modal: &ModalState,
    subagents: &SubAgentsState,
    mouse: Position,
) -> Option<ThreadPickerHitTarget> {
    let tasks = TaskState::default();
    hit_test_for_tasks(area, chat, modal, subagents, &tasks, mouse)
}

pub fn hit_test_for_tasks(
    area: Rect,
    chat: &ChatState,
    modal: &ModalState,
    subagents: &SubAgentsState,
    tasks: &TaskState,
    mouse: Position,
) -> Option<ThreadPickerHitTarget> {
    if !area.contains(mouse) {
        return None;
    }

    let inner = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Double)
        .inner(area);
    if inner.height < 5 {
        return None;
    }
    let [tabs_row, _, _, list_row, _] = thread_picker_layout(inner);
    let goal_index = GoalThreadIndex::from_tasks(tasks);

    if tabs_row.contains(mouse) {
        let tab_cells = tab_cells_inner(chat, subagents, Some(&goal_index));
        let selected_tab = modal.thread_picker_tab();
        let tab_scroll = tab_scroll_offset(tabs_row.width, &tab_cells, &selected_tab);
        for (tab, rect, _) in visible_tab_cells(tabs_row, &tab_cells, tab_scroll) {
            if rect.contains(mouse) {
                return Some(ThreadPickerHitTarget::Tab(tab));
            }
        }
    }

    if list_row.contains(mouse) {
        let total_items =
            filtered_threads_inner(chat, modal, subagents, Some(&goal_index)).len() + 1;
        let row_idx = mouse.y.saturating_sub(list_row.y) as usize;
        let (visible_start, visible_len) =
            visible_window(modal.picker_cursor(), total_items, list_row.height as usize);
        if row_idx < visible_len {
            return Some(ThreadPickerHitTarget::Item(visible_start + row_idx));
        }
    }

    None
}

/// Format millisecond timestamp as "Xm ago" or "Xh ago" etc.
fn format_time_ago(updated_at: u64) -> String {
    if updated_at == 0 {
        return String::new();
    }
    let now = now_millis();
    if now < updated_at {
        return "just now".to_string();
    }
    let diff_secs = (now - updated_at) / 1000;
    if diff_secs < 60 {
        format!("{}s ago", diff_secs)
    } else if diff_secs < 3600 {
        format!("{}m ago", diff_secs / 60)
    } else if diff_secs < 86400 {
        format!("{}h ago", diff_secs / 3600)
    } else {
        format!("{}d ago", diff_secs / 86400)
    }
}

/// Format token count compactly
fn format_tokens(tokens: u64) -> String {
    if tokens == 0 {
        return String::new();
    }
    format_token_count(tokens)
}

fn now_millis() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::chat::{AgentThread, ChatAction};
    use crate::state::task::{GoalRun, TaskAction, TaskState};
    use crate::state::ModalAction;
    use crate::state::{SubAgentEntry, SubAgentsState};

    #[test]
    fn format_time_ago_zero_returns_empty() {
        assert_eq!(format_time_ago(0), "");
    }

    #[test]
    fn format_tokens_zero_returns_empty() {
        assert_eq!(format_tokens(0), "");
    }

    #[test]
    fn format_tokens_thousands() {
        let s = format_tokens(1500);
        assert!(s.contains("k tok"));
    }

    #[test]
    fn format_tokens_billions() {
        assert_eq!(format_tokens(1_500_000_000), "1.5B tok");
    }

    #[test]
    fn format_tokens_small() {
        let s = format_tokens(500);
        assert_eq!(s, "500 tok");
    }

    fn make_chat(threads: Vec<AgentThread>) -> ChatState {
        let mut chat = ChatState::new();
        chat.reduce(ChatAction::ThreadListReceived(threads));
        chat
    }

    fn make_subagents(entries: Vec<SubAgentEntry>) -> SubAgentsState {
        let mut state = SubAgentsState::new();
        state.entries = entries;
        state
    }

    fn make_tasks_with_goal_thread(thread_id: &str) -> TaskState {
        let mut tasks = TaskState::new();
        tasks.reduce(TaskAction::GoalRunDetailReceived(GoalRun {
            id: "goal-1".into(),
            title: "Goal one".into(),
            thread_id: Some(thread_id.into()),
            active_thread_id: Some(thread_id.into()),
            ..Default::default()
        }));
        tasks
    }

    #[test]
    fn goal_thread_index_collects_goal_run_thread_ids_once_for_picker_use() {
        let tasks = make_tasks_with_goal_thread("thread-existing-goal");
        let index = GoalThreadIndex::from_tasks(&tasks);

        assert!(index.contains_id("thread-existing-goal"));
        assert!(!index.contains_id("thread-normal"));
    }

    fn sample_subagent(id: &str, name: &str, builtin: bool) -> SubAgentEntry {
        SubAgentEntry {
            id: id.to_string(),
            name: name.to_string(),
            provider: "openai".to_string(),
            model: "gpt-5.4-mini".to_string(),
            role: Some("testing".to_string()),
            enabled: true,
            builtin,
            immutable_identity: builtin,
            disable_allowed: !builtin,
            delete_allowed: !builtin,
            protected_reason: builtin.then(|| "builtin".to_string()),
            reasoning_effort: Some("medium".to_string()),
            raw_json: None,
        }
    }

    #[test]
    fn filtered_threads_default_to_swarog_and_exclude_rarog_threads() {
        let chat = make_chat(vec![
            AgentThread {
                id: "regular-thread".into(),
                title: "Regular work".into(),
                ..Default::default()
            },
            AgentThread {
                id: "concierge".into(),
                title: "Concierge".into(),
                ..Default::default()
            },
            AgentThread {
                id: "heartbeat-1".into(),
                title: "HEARTBEAT SYNTHESIS".into(),
                ..Default::default()
            },
            AgentThread {
                id: "dm:rarog:swarog".into(),
                title: "Internal DM · Rarog ↔ Svarog".into(),
                ..Default::default()
            },
            AgentThread {
                id: "weles-thread".into(),
                title: "WELES governance review".into(),
                ..Default::default()
            },
        ]);
        let modal = ModalState::new();

        let threads = filtered_threads(&chat, &modal, &make_subagents(Vec::new()));

        assert_eq!(threads.len(), 1);
        assert_eq!(threads[0].id, "regular-thread");
    }

    #[test]
    fn tab_specs_include_playgrounds_tab() {
        let chat = make_chat(Vec::new());
        let subagents = make_subagents(Vec::new());
        let tabs = tab_specs(&chat, &subagents);

        assert_eq!(tabs.len(), 7);
        assert_eq!(tabs[3].label, "[Goals]");
        assert!(
            tabs.iter()
                .any(|spec| spec.label.as_str() == "[Playgrounds]"),
            "expected thread picker tabs to expose a Playgrounds tab"
        );
    }

    #[test]
    fn tab_specs_include_user_defined_subagent_without_duplicating_weles() {
        let chat = make_chat(vec![
            AgentThread {
                id: "thread-domowoj".into(),
                agent_name: Some("Domowoj".into()),
                title: "Domowoj helps".into(),
                ..Default::default()
            },
            AgentThread {
                id: "thread-weles".into(),
                agent_name: Some("Weles".into()),
                title: "Weles helps".into(),
                ..Default::default()
            },
        ]);
        let subagents = make_subagents(vec![
            sample_subagent("domowoj", "Domowoj", false),
            sample_subagent("weles_builtin", "Weles", true),
        ]);

        let tabs = tab_specs(&chat, &subagents);
        let labels = tabs
            .iter()
            .map(|spec| spec.label.as_str())
            .collect::<Vec<_>>();

        assert!(labels.contains(&"[Domowoj]"));
        assert_eq!(
            labels.iter().filter(|label| **label == "[Weles]").count(),
            1,
            "Weles should remain a dedicated single tab"
        );
    }

    #[test]
    fn tab_specs_include_builtin_persona_when_threads_exist() {
        let chat = make_chat(vec![AgentThread {
            id: "thread-perun".into(),
            agent_name: Some("Perun".into()),
            title: "Perun triage".into(),
            ..Default::default()
        }]);
        let subagents = make_subagents(Vec::new());

        let tabs = tab_specs(&chat, &subagents);

        assert!(tabs.iter().any(|spec| spec.label == "[Perun]"));
    }

    #[test]
    fn gateway_tab_is_inserted_after_internal_and_before_subagents() {
        let chat = make_chat(vec![AgentThread {
            id: "thread-domowoj".into(),
            agent_name: Some("Domowoj".into()),
            title: "Domowoj triage".into(),
            ..Default::default()
        }]);
        let subagents = make_subagents(vec![sample_subagent("domowoj", "Domowoj", false)]);

        let labels = tab_specs(&chat, &subagents)
            .into_iter()
            .map(|spec| spec.label)
            .collect::<Vec<_>>();

        assert_eq!(
            labels,
            vec![
                "[Svarog]".to_string(),
                "[Rarog]".to_string(),
                "[Weles]".to_string(),
                "[Goals]".to_string(),
                "[Playgrounds]".to_string(),
                "[Internal]".to_string(),
                "[Gateway]".to_string(),
                "[Domowoj]".to_string(),
            ]
        );
    }

    #[test]
    fn thread_picker_tabs_auto_scroll_to_keep_selected_agent_visible() {
        let chat = make_chat(Vec::new());
        let subagents = make_subagents(vec![
            sample_subagent("radogost", "Radogost", false),
            sample_subagent("rod", "Rod", false),
            sample_subagent("dola", "dola", false),
            sample_subagent("swarozyc", "Swarozyc", false),
            sample_subagent("swietowit", "Swietowit", false),
        ]);
        let selected = ThreadPickerTab::Agent("dola".to_string());
        let tabs_area = Rect::new(0, 0, 24, 1);
        let cells = tab_cells(&chat, &subagents);
        let scroll = tab_scroll_offset(tabs_area.width, &cells, &selected);

        let visible_labels = visible_tab_cells(tabs_area, &cells, scroll)
            .into_iter()
            .map(|(_, _, label)| label)
            .collect::<Vec<_>>();

        assert!(scroll > 0, "expected overflow to produce horizontal scroll");
        assert!(
            visible_labels.iter().any(|label| label.contains("dola")),
            "expected selected tab to remain visible after auto-scroll, got {visible_labels:?}"
        );
    }

    #[test]
    fn thread_picker_hit_test_tracks_scrolled_tab_positions() {
        let chat = make_chat(Vec::new());
        let subagents = make_subagents(vec![
            sample_subagent("radogost", "Radogost", false),
            sample_subagent("rod", "Rod", false),
            sample_subagent("dola", "dola", false),
            sample_subagent("swarozyc", "Swarozyc", false),
        ]);
        let selected = ThreadPickerTab::Agent("dola".to_string());
        let mut modal = ModalState::new();
        modal.set_thread_picker_tab(selected.clone());
        let area = Rect::new(0, 0, 28, 8);
        let inner = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Double)
            .inner(area);
        let [tabs_row, _, _, _, _] = thread_picker_layout(inner);
        let cells = tab_cells(&chat, &subagents);
        let scroll = tab_scroll_offset(tabs_row.width, &cells, &selected);
        let (_, selected_rect, _) = visible_tab_cells(tabs_row, &cells, scroll)
            .into_iter()
            .find(|(tab, _, _)| *tab == selected)
            .expect("selected tab should stay visible");
        let mouse = Position::new(selected_rect.x, selected_rect.y);

        let hit = hit_test(area, &chat, &modal, &subagents, mouse);

        assert_eq!(hit, Some(ThreadPickerHitTarget::Tab(selected)));
    }

    #[test]
    fn filtered_threads_default_tab_excludes_playground_threads() {
        let chat = make_chat(vec![
            AgentThread {
                id: "regular-thread".into(),
                title: "Regular work".into(),
                ..Default::default()
            },
            AgentThread {
                id: "playground:domowoj:thread-user".into(),
                title: "Participant Playground · Domowoj @ thread-user".into(),
                ..Default::default()
            },
            AgentThread {
                id: "goal:goal_1".into(),
                agent_name: Some("Domowoj".into()),
                title: "Run concrete moat pass".into(),
                ..Default::default()
            },
        ]);
        let modal = ModalState::new();

        let threads = filtered_threads(&chat, &modal, &make_subagents(Vec::new()));

        assert_eq!(threads.len(), 1);
        assert_eq!(threads[0].id, "regular-thread");
    }

    #[test]
    fn filtered_threads_dynamic_agent_tab_excludes_goal_threads() {
        let chat = make_chat(vec![
            AgentThread {
                id: "thread-domowoj".into(),
                agent_name: Some("Domowoj".into()),
                title: "Normal agent conversation".into(),
                ..Default::default()
            },
            AgentThread {
                id: "goal:goal_1".into(),
                agent_name: Some("Domowoj".into()),
                title: "Run concrete moat pass".into(),
                ..Default::default()
            },
        ]);
        let subagents = make_subagents(vec![sample_subagent("domowoj", "Domowoj", false)]);
        let mut modal = ModalState::new();
        modal.set_thread_picker_tab(ThreadPickerTab::Agent("domowoj".to_string()));

        let threads = filtered_threads(&chat, &modal, &subagents);

        assert_eq!(threads.len(), 1);
        assert_eq!(threads[0].id, "thread-domowoj");
    }

    #[test]
    fn filtered_threads_swarog_tab_excludes_dynamic_subagent_threads() {
        let chat = make_chat(vec![
            AgentThread {
                id: "thread-svarog".into(),
                agent_name: Some("Svarog".into()),
                title: "Root planning".into(),
                ..Default::default()
            },
            AgentThread {
                id: "thread-domowoj".into(),
                agent_name: Some("Domowoj".into()),
                title: "Spawned child execution".into(),
                ..Default::default()
            },
        ]);
        let subagents = make_subagents(vec![sample_subagent("domowoj", "Domowoj", false)]);
        let modal = ModalState::new();

        let threads = filtered_threads(&chat, &modal, &subagents);

        assert_eq!(threads.len(), 1);
        assert_eq!(threads[0].id, "thread-svarog");
    }

    #[test]
    fn rarog_tab_filters_threads_and_searches_within_tab() {
        let chat = make_chat(vec![
            AgentThread {
                id: "regular-thread".into(),
                title: "Regular work".into(),
                ..Default::default()
            },
            AgentThread {
                id: "concierge".into(),
                title: "Concierge".into(),
                ..Default::default()
            },
            AgentThread {
                id: "heartbeat-1".into(),
                title: "HEARTBEAT SYNTHESIS".into(),
                ..Default::default()
            },
        ]);
        let mut modal = ModalState::new();
        modal.set_thread_picker_tab(ThreadPickerTab::Rarog);
        modal.reduce(ModalAction::SetQuery("heart".into()));

        let threads = filtered_threads(&chat, &modal, &make_subagents(Vec::new()));

        assert_eq!(threads.len(), 1);
        assert_eq!(threads[0].id, "heartbeat-1");
    }

    #[test]
    fn playgrounds_tab_filters_only_playground_threads() {
        let chat = make_chat(vec![
            AgentThread {
                id: "regular-thread".into(),
                title: "Regular work".into(),
                ..Default::default()
            },
            AgentThread {
                id: "playground:domowoj:thread-user".into(),
                title: "Participant Playground · Domowoj @ thread-user".into(),
                ..Default::default()
            },
        ]);
        let mut modal = ModalState::new();
        modal.set_thread_picker_tab(ThreadPickerTab::Playgrounds);

        let threads = filtered_threads(&chat, &modal, &make_subagents(Vec::new()));

        assert_eq!(threads.len(), 1);
        assert_eq!(threads[0].id, "playground:domowoj:thread-user");
    }

    #[test]
    fn goals_tab_filters_goal_threads() {
        let chat = make_chat(vec![
            AgentThread {
                id: "regular-thread".into(),
                title: "Regular work".into(),
                ..Default::default()
            },
            AgentThread {
                id: "goal:goal_1".into(),
                agent_name: Some("Domowoj".into()),
                title: "Run concrete moat pass".into(),
                ..Default::default()
            },
        ]);
        let mut modal = ModalState::new();
        modal.set_thread_picker_tab(ThreadPickerTab::Goals);

        let threads = filtered_threads(&chat, &modal, &make_subagents(Vec::new()));

        assert_eq!(threads.len(), 1);
        assert_eq!(threads[0].id, "goal:goal_1");
    }

    #[test]
    fn goal_run_thread_ids_route_plain_threads_to_goals_tab() {
        let chat = make_chat(vec![AgentThread {
            id: "thread-existing-goal".into(),
            agent_name: Some("Domowoj".into()),
            title: "Run concrete moat pass".into(),
            ..Default::default()
        }]);
        let subagents = make_subagents(Vec::new());
        let tasks = make_tasks_with_goal_thread("thread-existing-goal");
        let mut modal = ModalState::new();

        let labels = tab_specs_for_tasks(&chat, &subagents, &tasks)
            .into_iter()
            .map(|spec| spec.label)
            .collect::<Vec<_>>();
        assert!(
            !labels.iter().any(|label| label == "[Domowoj]"),
            "goal-linked threads should not create normal agent tabs"
        );

        assert!(
            filtered_threads_for_tasks(&chat, &modal, &subagents, &tasks).is_empty(),
            "goal-linked threads should be excluded from the default Swarog tab"
        );

        modal.set_thread_picker_tab(ThreadPickerTab::Agent("domowoj".into()));
        assert!(
            filtered_threads_for_tasks(&chat, &modal, &subagents, &tasks).is_empty(),
            "goal-linked threads should be excluded from normal agent tabs"
        );

        modal.set_thread_picker_tab(ThreadPickerTab::Goals);
        let threads = filtered_threads_for_tasks(&chat, &modal, &subagents, &tasks);
        assert_eq!(threads.len(), 1);
        assert_eq!(threads[0].id, "thread-existing-goal");
        assert_eq!(
            thread_display_title_for_tasks(threads[0], &tasks),
            "goal: Domowoj · Run concrete moat pass"
        );
    }

    #[test]
    fn goal_thread_display_title_shows_prefix_role_and_title() {
        let thread = AgentThread {
            id: "goal:goal_1".into(),
            agent_name: Some("Domowoj".into()),
            title: "Run concrete moat pass".into(),
            ..Default::default()
        };

        assert_eq!(
            thread_display_title(&thread),
            "goal: Domowoj · Run concrete moat pass"
        );
    }

    #[test]
    fn search_matches_thread_responder_name() {
        let chat = make_chat(vec![AgentThread {
            id: "regular-thread".into(),
            agent_name: Some("Domowoj".into()),
            title: "Needs review".into(),
            ..Default::default()
        }]);
        let mut modal = ModalState::new();
        modal.reduce(ModalAction::SetQuery("domowoj".into()));

        let threads = filtered_threads(&chat, &modal, &make_subagents(Vec::new()));

        assert_eq!(threads.len(), 1);
        assert_eq!(threads[0].id, "regular-thread");
    }

    #[test]
    fn weles_tab_filters_weles_threads_without_internal_dms() {
        let chat = make_chat(vec![
            AgentThread {
                id: "regular-thread".into(),
                title: "Regular work".into(),
                ..Default::default()
            },
            AgentThread {
                id: "weles-thread".into(),
                title: "WELES governance review".into(),
                ..Default::default()
            },
            AgentThread {
                id: "dm:svarog:weles".into(),
                title: "Internal DM · Svarog ↔ Weles".into(),
                ..Default::default()
            },
        ]);
        let mut modal = ModalState::new();
        modal.set_thread_picker_tab(ThreadPickerTab::Weles);

        let threads = filtered_threads(&chat, &modal, &make_subagents(Vec::new()));

        assert_eq!(threads.len(), 1);
        assert_eq!(threads[0].id, "weles-thread");
    }

    #[test]
    fn weles_tab_uses_agent_name_for_new_targeted_threads() {
        let chat = make_chat(vec![
            AgentThread {
                id: "thread-weles".into(),
                agent_name: Some("Weles".into()),
                title: "Review pending changes".into(),
                ..Default::default()
            },
            AgentThread {
                id: "thread-svarog".into(),
                agent_name: Some("Svarog".into()),
                title: "Review pending changes".into(),
                ..Default::default()
            },
        ]);
        let mut modal = ModalState::new();
        modal.set_thread_picker_tab(ThreadPickerTab::Weles);

        let threads = filtered_threads(&chat, &modal, &make_subagents(Vec::new()));

        assert_eq!(threads.len(), 1);
        assert_eq!(threads[0].id, "thread-weles");
    }

    #[test]
    fn rarog_tab_uses_agent_name_for_new_targeted_threads() {
        let chat = make_chat(vec![
            AgentThread {
                id: "thread-rarog".into(),
                agent_name: Some("Rarog".into()),
                title: "Operator triage".into(),
                ..Default::default()
            },
            AgentThread {
                id: "thread-svarog".into(),
                agent_name: Some("Svarog".into()),
                title: "Operator triage".into(),
                ..Default::default()
            },
        ]);
        let mut modal = ModalState::new();
        modal.set_thread_picker_tab(ThreadPickerTab::Rarog);

        let threads = filtered_threads(&chat, &modal, &make_subagents(Vec::new()));

        assert_eq!(threads.len(), 1);
        assert_eq!(threads[0].id, "thread-rarog");
    }

    #[test]
    fn internal_tab_filters_internal_dm_threads() {
        let chat = make_chat(vec![
            AgentThread {
                id: "regular-thread".into(),
                title: "Regular work".into(),
                ..Default::default()
            },
            AgentThread {
                id: "dm:svarog:weles".into(),
                title: "Internal DM · Svarog ↔ Weles".into(),
                ..Default::default()
            },
        ]);
        let mut modal = ModalState::new();
        modal.set_thread_picker_tab(ThreadPickerTab::Internal);

        let threads = filtered_threads(&chat, &modal, &make_subagents(Vec::new()));

        assert_eq!(threads.len(), 1);
        assert_eq!(threads[0].id, "dm:svarog:weles");
    }

    #[test]
    fn gateway_tab_filters_gateway_threads() {
        let chat = make_chat(vec![
            AgentThread {
                id: "regular-thread".into(),
                title: "Regular work".into(),
                ..Default::default()
            },
            AgentThread {
                id: "thread-slack-alice".into(),
                title: "slack Alice".into(),
                ..Default::default()
            },
            AgentThread {
                id: "dm:svarog:weles".into(),
                title: "Internal DM · Svarog ↔ Weles".into(),
                ..Default::default()
            },
        ]);
        let mut modal = ModalState::new();
        modal.set_thread_picker_tab(ThreadPickerTab::Gateway);

        let threads = filtered_threads(&chat, &modal, &make_subagents(Vec::new()));

        assert_eq!(threads.len(), 1);
        assert_eq!(threads[0].id, "thread-slack-alice");
    }

    #[test]
    fn filtered_threads_exclude_hidden_handoff_threads() {
        let chat = make_chat(vec![
            AgentThread {
                id: "regular-thread".into(),
                title: "Regular work".into(),
                ..Default::default()
            },
            AgentThread {
                id: "handoff:regular-thread:handoff-1".into(),
                title: "Handoff · Svarog -> Weles".into(),
                ..Default::default()
            },
        ]);
        let modal = ModalState::new();

        let threads = filtered_threads(&chat, &modal, &make_subagents(Vec::new()));

        assert_eq!(threads.len(), 1);
        assert_eq!(threads[0].id, "regular-thread");
    }

    #[test]
    fn dynamic_agent_tab_filters_matching_threads() {
        let chat = make_chat(vec![
            AgentThread {
                id: "thread-domowoj".into(),
                agent_name: Some("Domowoj".into()),
                title: "Workspace cleanup".into(),
                ..Default::default()
            },
            AgentThread {
                id: "thread-svarog".into(),
                agent_name: Some("Svarog".into()),
                title: "Workspace cleanup".into(),
                ..Default::default()
            },
        ]);
        let subagents = make_subagents(vec![sample_subagent("domowoj", "Domowoj", false)]);
        let mut modal = ModalState::new();
        modal.set_thread_picker_tab(ThreadPickerTab::Agent("domowoj".to_string()));

        let threads = filtered_threads(&chat, &modal, &subagents);

        assert_eq!(threads.len(), 1);
        assert_eq!(threads[0].id, "thread-domowoj");
    }

    #[test]
    fn thread_display_title_renames_concierge_to_rarog() {
        let thread = AgentThread {
            id: "concierge".into(),
            title: "Concierge".into(),
            ..Default::default()
        };

        assert_eq!(thread_display_title(&thread), AGENT_NAME_RAROG);
    }
}
