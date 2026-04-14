use ratatui::prelude::*;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, List, ListItem, Paragraph};

use amux_protocol::{AGENT_NAME_RAROG, AGENT_NAME_SWAROG};

use crate::state::chat::{AgentThread, ChatState};
use crate::state::modal::{ModalState, ThreadPickerTab};
use crate::theme::ThemeTokens;
use crate::widgets::token_format::format_token_count;

const TAB_GAP: u16 = 1;
const INTERNAL_DM_THREAD_PREFIX: &str = "dm:";
const INTERNAL_DM_TITLE_PREFIX: &str = "Internal DM";
const HIDDEN_HANDOFF_THREAD_PREFIX: &str = "handoff:";
const PLAYGROUND_THREAD_PREFIX: &str = "playground:";
const PLAYGROUND_THREAD_TITLE_PREFIX: &str = "Participant Playground";
const WELES_THREAD_TITLE: &str = "WELES";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThreadPickerHitTarget {
    Tab(ThreadPickerTab),
    Item(usize),
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

fn tab_specs() -> [(ThreadPickerTab, String); 5] {
    [
        (ThreadPickerTab::Swarog, format!("[{AGENT_NAME_SWAROG}]")),
        (ThreadPickerTab::Rarog, format!("[{AGENT_NAME_RAROG}]")),
        (ThreadPickerTab::Weles, "[Weles]".to_string()),
        (ThreadPickerTab::Playgrounds, "[Playgrounds]".to_string()),
        (ThreadPickerTab::Internal, "[Internal]".to_string()),
    ]
}

fn thread_matches_query(thread: &AgentThread, query: &str) -> bool {
    if query.is_empty() {
        return true;
    }
    let lower = query.to_lowercase();
    thread.title.to_lowercase().contains(&lower)
        || thread_display_title(thread).to_lowercase().contains(&lower)
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

pub(crate) fn is_playground_thread(thread: &AgentThread) -> bool {
    thread.id.starts_with(PLAYGROUND_THREAD_PREFIX)
        || thread.title.starts_with(PLAYGROUND_THREAD_TITLE_PREFIX)
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

pub(crate) fn thread_display_title(thread: &AgentThread) -> String {
    if thread.id == "concierge" || thread.title.eq_ignore_ascii_case("concierge") {
        AGENT_NAME_RAROG.to_string()
    } else {
        thread.title.clone()
    }
}

pub(crate) fn filtered_threads<'a>(
    chat: &'a ChatState,
    modal: &ModalState,
) -> Vec<&'a AgentThread> {
    let query = modal.command_query();
    chat.threads()
        .iter()
        .filter(|thread| !is_hidden_handoff_thread(thread))
        .filter(|thread| match modal.thread_picker_tab() {
            ThreadPickerTab::Swarog => {
                !is_rarog_thread(thread)
                    && !is_internal_thread(thread)
                    && !is_weles_thread(thread)
                    && !is_playground_thread(thread)
            }
            ThreadPickerTab::Rarog => is_rarog_thread(thread),
            ThreadPickerTab::Weles => !is_playground_thread(thread) && is_weles_thread(thread),
            ThreadPickerTab::Playgrounds => is_playground_thread(thread),
            ThreadPickerTab::Internal => is_internal_thread(thread),
        })
        .filter(|thread| thread_matches_query(thread, query))
        .collect()
}

fn tab_cells(area: Rect) -> Vec<(ThreadPickerTab, Rect, String)> {
    let mut x = area.x;
    tab_specs()
        .into_iter()
        .map(|(tab, label)| {
            let width = label.chars().count() as u16;
            let rect = Rect::new(x, area.y, width, area.height);
            x = x.saturating_add(width + TAB_GAP);
            (tab, rect, label)
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
        ThreadPickerTab::Playgrounds => "Playgrounds are created automatically",
        _ => "+ New conversation",
    }
}

pub fn render(
    frame: &mut Frame,
    area: Rect,
    chat: &ChatState,
    modal: &ModalState,
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

    let tab_line = Line::from(
        tab_cells(tabs_row)
            .into_iter()
            .enumerate()
            .flat_map(|(index, (tab, _, label))| {
                let style = if tab == modal.thread_picker_tab() {
                    theme.accent_primary
                } else {
                    theme.fg_dim
                };
                let mut spans = vec![Span::styled(label, style)];
                if index + 1 < tab_specs().len() {
                    spans.push(Span::raw(" "));
                }
                spans
            })
            .collect::<Vec<_>>(),
    );
    frame.render_widget(Paragraph::new(tab_line), tabs_row);

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
    let filtered_threads = filtered_threads(chat, modal);

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

                        let display_title = thread_display_title(thread);
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
    let hints = Line::from(vec![
        Span::raw(" "),
        Span::styled("↑↓", theme.fg_active),
        Span::styled(" navigate  ", theme.fg_dim),
        Span::styled("←→", theme.fg_active),
        Span::styled(" source  ", theme.fg_dim),
        Span::styled("Enter", theme.fg_active),
        Span::styled(" select  ", theme.fg_dim),
        Span::styled("Esc", theme.fg_active),
        Span::styled(" close", theme.fg_dim),
    ]);
    frame.render_widget(Paragraph::new(hints), hints_row);
}

pub fn hit_test(
    area: Rect,
    chat: &ChatState,
    modal: &ModalState,
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

    if tabs_row.contains(mouse) {
        for (tab, rect, _) in tab_cells(tabs_row) {
            if rect.contains(mouse) {
                return Some(ThreadPickerHitTarget::Tab(tab));
            }
        }
    }

    if list_row.contains(mouse) {
        let total_items = filtered_threads(chat, modal).len() + 1;
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
    use crate::state::ModalAction;

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

        let threads = filtered_threads(&chat, &modal);

        assert_eq!(threads.len(), 1);
        assert_eq!(threads[0].id, "regular-thread");
    }

    #[test]
    fn tab_specs_include_playgrounds_tab() {
        let tabs = tab_specs();

        assert_eq!(tabs.len(), 5);
        assert!(
            tabs.iter()
                .any(|(_, label)| label.as_str() == "[Playgrounds]"),
            "expected thread picker tabs to expose a Playgrounds tab"
        );
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
        ]);
        let modal = ModalState::new();

        let threads = filtered_threads(&chat, &modal);

        assert_eq!(threads.len(), 1);
        assert_eq!(threads[0].id, "regular-thread");
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

        let threads = filtered_threads(&chat, &modal);

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

        let threads = filtered_threads(&chat, &modal);

        assert_eq!(threads.len(), 1);
        assert_eq!(threads[0].id, "playground:domowoj:thread-user");
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

        let threads = filtered_threads(&chat, &modal);

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

        let threads = filtered_threads(&chat, &modal);

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

        let threads = filtered_threads(&chat, &modal);

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

        let threads = filtered_threads(&chat, &modal);

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

        let threads = filtered_threads(&chat, &modal);

        assert_eq!(threads.len(), 1);
        assert_eq!(threads[0].id, "dm:svarog:weles");
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

        let threads = filtered_threads(&chat, &modal);

        assert_eq!(threads.len(), 1);
        assert_eq!(threads[0].id, "regular-thread");
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
