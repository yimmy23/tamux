use ratatui::prelude::*;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph};

use crate::providers;
use crate::state::concierge::ConciergeState;
use crate::state::config::ConfigState;
use crate::state::subagents::SubAgentsState;
use crate::state::settings::{SettingsState, SettingsTab};
use crate::theme::ThemeTokens;
use crate::widgets::message::wrap_text;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsHitTarget {
    Tab(SettingsTab),
    Field(usize),
    AuthProviderItem(usize),
    AuthAction { index: usize, action: AuthTabAction },
    SubAgentListItem(usize),
    SubAgentAction(SubAgentTabAction),
    SubAgentRowAction { index: usize, action: SubAgentTabAction },
    EditCursor { line: usize, col: usize },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthTabAction {
    Primary,
    Test,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubAgentTabAction {
    Add,
    Edit,
    Delete,
    Toggle,
}

const TAB_LABELS: [&str; 10] = ["Prov", "Tools", "Search", "Chat", "GW", "Auth", "Agent", "Sub", "Con", "Adv"];
const TAB_DIVIDER: &str = " | ";

#[derive(Debug, Clone, Copy)]
struct VisibleTab {
    tab: SettingsTab,
    index: usize,
    start_x: u16,
    end_x: u16,
}

fn render_edit_buffer_with_cursor(text: &str, cursor: usize) -> String {
    let cursor = cursor.min(text.len());
    let mut out = String::with_capacity(text.len() + 3);
    out.push_str(&text[..cursor]);
    out.push('\u{2588}');
    out.push_str(&text[cursor..]);
    out
}

fn render_edit_line_with_cursor(text: &str, cursor_col: usize) -> String {
    let mut out = String::with_capacity(text.len() + 3);
    let mut inserted = false;
    for (col, ch) in text.chars().enumerate() {
        if col == cursor_col {
            out.push('\u{2588}');
            inserted = true;
        }
        out.push(ch);
    }
    if !inserted {
        out.push('\u{2588}');
    }
    out
}

fn clip_inline_text(text: &str, max_chars: usize) -> String {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() <= max_chars {
        return text.to_string();
    }
    let tail: String = chars[chars.len().saturating_sub(max_chars)..].iter().collect();
    format!("…{}", tail)
}

pub fn render(
    frame: &mut Frame,
    area: Rect,
    settings: &SettingsState,
    config: &ConfigState,
    auth: &crate::state::auth::AuthState,
    subagents: &SubAgentsState,
    concierge: &ConciergeState,
    theme: &ThemeTokens,
) {
    let block = Block::default()
        .title(" SETTINGS ")
        .borders(Borders::ALL)
        .border_type(BorderType::Double)
        .border_style(theme.accent_secondary);

    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.height < 5 {
        return;
    }

    // Split: tab bar (1) + separator (1) + content (flex) + hints (1)
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // tab bar
            Constraint::Length(1), // separator
            Constraint::Min(1),    // content
            Constraint::Length(1), // hints
        ])
        .split(inner);

    // Tab bar
    let active = settings.active_tab();
    let tab_index = match active {
        SettingsTab::Provider => 0,
        SettingsTab::Tools => 1,
        SettingsTab::WebSearch => 2,
        SettingsTab::Chat => 3,
        SettingsTab::Gateway => 4,
        SettingsTab::Auth => 5,
        SettingsTab::Agent => 6,
        SettingsTab::SubAgents => 7,
        SettingsTab::Concierge => 8,
        SettingsTab::Advanced => 9,
    };
    let tabs = visible_tabs(chunks[0], tab_index);
    frame.render_widget(
        Paragraph::new(render_tabs_line(&tabs, settings, theme)),
        chunks[0],
    );

    // Separator
    let sep = Line::from(Span::styled(
        "\u{2500}".repeat(chunks[1].width as usize),
        theme.fg_dim,
    ));
    frame.render_widget(Paragraph::new(sep), chunks[1]);

    // Content
    let content_lines = render_tab_content(
        chunks[2].width,
        settings,
        config,
        auth,
        subagents,
        concierge,
        theme,
    );
    let paragraph = Paragraph::new(content_lines);
    frame.render_widget(paragraph, chunks[2]);

    // Hints — context-sensitive
    let hints = if settings.is_editing() {
        Line::from(vec![
            Span::raw(" "),
            Span::styled("Enter", theme.fg_active),
            Span::styled(" confirm  ", theme.fg_dim),
            Span::styled("Esc", theme.fg_active),
            Span::styled(" cancel", theme.fg_dim),
        ])
    } else {
        Line::from(vec![
            Span::raw(" "),
            Span::styled("↑↓", theme.fg_active),
            Span::styled(" navigate  ", theme.fg_dim),
            Span::styled("Enter", theme.fg_active),
            Span::styled(" edit/select  ", theme.fg_dim),
            Span::styled("Space", theme.fg_active),
            Span::styled(" toggle  ", theme.fg_dim),
            Span::styled("Tab", theme.fg_active),
            Span::styled(" switch tab  ", theme.fg_dim),
            Span::styled("Esc", theme.fg_active),
            Span::styled(" close", theme.fg_dim),
        ])
    };
    frame.render_widget(Paragraph::new(hints), chunks[3]);
}

pub fn hit_test(
    area: Rect,
    settings: &SettingsState,
    _config: &ConfigState,
    auth: &crate::state::auth::AuthState,
    subagents: &SubAgentsState,
    mouse: Position,
) -> Option<SettingsHitTarget> {
    let block = Block::default()
        .title(" SETTINGS ")
        .borders(Borders::ALL)
        .border_type(BorderType::Double);
    let inner = block.inner(area);
    if inner.height < 5
        || mouse.x < inner.x
        || mouse.x >= inner.x.saturating_add(inner.width)
        || mouse.y < inner.y
        || mouse.y >= inner.y.saturating_add(inner.height)
    {
        return None;
    }

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Min(1),
            Constraint::Length(1),
        ])
        .split(inner);

    if mouse.y == chunks[0].y {
        if let Some(tab) = tab_hit_test(chunks[0], settings.active_tab(), mouse.x) {
            return Some(SettingsHitTarget::Tab(tab));
        }
        return None;
    }

    if mouse.y < chunks[2].y || mouse.y >= chunks[2].y.saturating_add(chunks[2].height) {
        return None;
    }

    if let Some((line, col)) = editing_cursor_hit_test(chunks[2], settings, mouse) {
        return Some(SettingsHitTarget::EditCursor { line, col });
    }

    if matches!(settings.active_tab(), SettingsTab::Auth) {
        return auth_hit_test(chunks[2], auth, mouse);
    }

    if matches!(settings.active_tab(), SettingsTab::SubAgents) {
        return subagents_hit_test(chunks[2], subagents, mouse);
    }

    let row = mouse.y.saturating_sub(chunks[2].y) as usize;
    match settings_row_hit(settings, subagents, row) {
        Some((_, Some(subagent_index))) => Some(SettingsHitTarget::SubAgentListItem(subagent_index)),
        Some((field, None)) => Some(SettingsHitTarget::Field(field)),
        None => None,
    }
}

fn tab_hit_test(tab_area: Rect, active_tab: SettingsTab, mouse_x: u16) -> Option<SettingsTab> {
    visible_tabs(tab_area, active_tab_index(active_tab))
        .into_iter()
        .find(|tab| mouse_x >= tab.start_x && mouse_x < tab.end_x)
        .map(|tab| tab.tab)
}

fn active_tab_index(tab: SettingsTab) -> usize {
    match tab {
        SettingsTab::Provider => 0,
        SettingsTab::Tools => 1,
        SettingsTab::WebSearch => 2,
        SettingsTab::Chat => 3,
        SettingsTab::Gateway => 4,
        SettingsTab::Auth => 5,
        SettingsTab::Agent => 6,
        SettingsTab::SubAgents => 7,
        SettingsTab::Concierge => 8,
        SettingsTab::Advanced => 9,
    }
}

fn visible_tabs(tab_area: Rect, active_index: usize) -> Vec<VisibleTab> {
    let all = SettingsTab::all();
    let divider_width = TAB_DIVIDER.chars().count() as u16;
    let total_width = |start: usize, end: usize| -> u16 {
        (start..=end)
            .map(|idx| TAB_LABELS[idx].chars().count() as u16)
            .sum::<u16>()
            .saturating_add(divider_width.saturating_mul((end - start) as u16))
    };

    let mut start = 0usize;
    let mut end = all.len().saturating_sub(1);
    let available = tab_area.width.saturating_sub(2);

    while start < active_index && total_width(start, end) > available {
        start += 1;
    }
    while end > active_index && total_width(start, end) > available {
        end = end.saturating_sub(1);
    }
    while total_width(start, end) > available && start < end {
        if active_index.saturating_sub(start) > end.saturating_sub(active_index) {
            start += 1;
        } else {
            end = end.saturating_sub(1);
        }
    }

    let prefix = if start > 0 { "« " } else { "" };
    let suffix = if end + 1 < all.len() { " »" } else { "" };
    let inner_width = total_width(start, end)
        .saturating_add(prefix.chars().count() as u16)
        .saturating_add(suffix.chars().count() as u16);
    let mut x = tab_area
        .x
        .saturating_add(tab_area.width.saturating_sub(inner_width) / 2)
        .saturating_add(prefix.chars().count() as u16);

    let mut visible = Vec::new();
    for idx in start..=end {
        let width = TAB_LABELS[idx].chars().count() as u16;
        visible.push(VisibleTab {
            tab: all[idx],
            index: idx,
            start_x: x,
            end_x: x.saturating_add(width),
        });
        x = x.saturating_add(width);
        if idx < end {
            x = x.saturating_add(divider_width);
        }
    }
    visible
}

fn render_tabs_line(
    tabs: &[VisibleTab],
    settings: &SettingsState,
    theme: &ThemeTokens,
) -> Line<'static> {
    let active_index = active_tab_index(settings.active_tab());
    let mut spans = Vec::new();
    if tabs.first().is_some_and(|tab| tab.index > 0) {
        spans.push(Span::styled("« ", theme.fg_dim));
    }
    for (idx, tab) in tabs.iter().enumerate() {
        let style = if tab.index == active_index {
            theme.fg_active
        } else {
            theme.fg_dim
        };
        spans.push(Span::styled(TAB_LABELS[tab.index], style));
        if idx + 1 < tabs.len() {
            spans.push(Span::styled(TAB_DIVIDER, theme.fg_dim));
        }
    }
    if tabs.last().is_some_and(|tab| tab.index + 1 < TAB_LABELS.len()) {
        spans.push(Span::styled(" »", theme.fg_dim));
    }
    Line::from(spans)
}

fn editing_cursor_hit_test(
    content_area: Rect,
    settings: &SettingsState,
    mouse: Position,
) -> Option<(usize, usize)> {
    let field = settings.editing_field()?;
    let row = mouse.y.saturating_sub(content_area.y) as usize;
    let rel_x = mouse.x.saturating_sub(content_area.x) as usize;

    if settings.is_textarea() && field == "system_prompt" {
        let text_start_row = 7usize;
        let text_start_col = 4usize;
        let line_count = settings.edit_buffer().split('\n').count().max(1);
        let row_end = text_start_row + line_count;
        if row < text_start_row || row > row_end {
            return None;
        }
        let line = (row - text_start_row).min(line_count.saturating_sub(1));
        let col = rel_x.saturating_sub(text_start_col);
        return Some((line, col));
    }

    let (field_row, start_col) = single_line_edit_layout(settings, field)?;
    if row == field_row {
        return Some((0, rel_x.saturating_sub(start_col)));
    }
    None
}

fn single_line_edit_layout(settings: &SettingsState, field: &str) -> Option<(usize, usize)> {
    match settings.active_tab() {
        SettingsTab::Provider => match field {
            "base_url" => Some((5, 19)),
            "api_key" => Some((6, 19)),
            "assistant_id" => Some((10, 19)),
            "context_window_tokens" => Some((12, 19)),
            _ => None,
        },
        SettingsTab::WebSearch => match field {
            "firecrawl_api_key" => Some((6, 19)),
            "exa_api_key" => Some((7, 19)),
            "tavily_api_key" => Some((8, 19)),
            "search_max_results" => Some((9, 19)),
            "search_timeout" => Some((10, 19)),
            _ => None,
        },
        SettingsTab::Chat => match field {
            "honcho_api_key" => Some((7, 19)),
            "honcho_base_url" => Some((8, 19)),
            "honcho_workspace_id" => Some((9, 19)),
            _ => None,
        },
        SettingsTab::Gateway => match field {
            "gateway_prefix" => Some((5, 19)),
            "slack_token" => Some((8, 19)),
            "slack_channel_filter" => Some((9, 19)),
            "telegram_token" => Some((12, 19)),
            "telegram_allowed_chats" => Some((13, 19)),
            "discord_token" => Some((16, 19)),
            "discord_channel_filter" => Some((17, 19)),
            "discord_allowed_users" => Some((18, 19)),
            "whatsapp_allowed_contacts" => Some((21, 19)),
            "whatsapp_token" => Some((22, 19)),
            "whatsapp_phone_id" => Some((23, 19)),
            _ => None,
        },
        SettingsTab::Auth => None,
        SettingsTab::Agent => match field {
            "agent_name" => Some((4, 19)),
            _ => None,
        },
        SettingsTab::SubAgents => None,
        SettingsTab::Concierge => None,
        SettingsTab::Advanced => match field {
            "max_context_messages" => Some((5, 20)),
            "max_tool_loops" => Some((6, 20)),
            "max_retries" => Some((7, 20)),
            "retry_delay_ms" => Some((8, 20)),
            "context_budget_tokens" => Some((9, 20)),
            "compact_threshold_pct" => Some((10, 20)),
            "keep_recent_on_compact" => Some((11, 20)),
            "bash_timeout_secs" => Some((12, 20)),
            "snapshot_max_count" => Some((15, 20)),
            "snapshot_max_size_mb" => Some((16, 20)),
            _ => None,
        },
        SettingsTab::Tools => None,
    }
}

fn settings_row_hit(
    settings: &SettingsState,
    subagents: &SubAgentsState,
    row: usize,
) -> Option<(usize, Option<usize>)> {
    match settings.active_tab() {
        SettingsTab::Provider => row
            .checked_sub(4)
            .filter(|idx| *idx < 8)
            .map(|idx| (idx, None)),
        SettingsTab::Tools => row
            .checked_sub(4)
            .filter(|idx| *idx < 7)
            .map(|idx| (idx, None)),
        SettingsTab::WebSearch => row
            .checked_sub(4)
            .filter(|idx| *idx < 7)
            .map(|idx| (idx, None)),
        SettingsTab::Chat => row
            .checked_sub(4)
            .filter(|idx| *idx < 6)
            .map(|idx| (idx, None)),
        SettingsTab::Advanced => row
            .checked_sub(4)
            .filter(|idx| *idx < 13)
            .map(|idx| (idx, None)),
        SettingsTab::Gateway => match row {
            4 => Some((0, None)),
            5 => Some((1, None)),
            8 => Some((2, None)),
            9 => Some((3, None)),
            12 => Some((4, None)),
            13 => Some((5, None)),
            16 => Some((6, None)),
            17 => Some((7, None)),
            18 => Some((8, None)),
            21 => Some((9, None)),
            22 => Some((10, None)),
            23 => Some((11, None)),
            _ => None,
        },
        SettingsTab::Auth => row
            .checked_sub(4)
            .filter(|idx| *idx < 3)
            .map(|idx| (idx, None)),
        SettingsTab::Agent => {
            if settings.is_editing()
                && settings.is_textarea()
                && settings.editing_field() == Some("system_prompt")
            {
                let prompt_lines = settings.edit_buffer().lines().count().max(1);
                match row {
                    4 => Some((0, None)),
                    5..=6 => Some((1, None)),
                    r if r <= 8 + prompt_lines => Some((1, None)),
                    r if r == 9 + prompt_lines => Some((2, None)),
                    _ => None,
                }
            } else {
                match row {
                    4 => Some((0, None)),
                    5 => Some((1, None)),
                    6 => Some((2, None)),
                    _ => None,
                }
            }
        }
        SettingsTab::SubAgents => {
            let list_len = subagents.entries.len();
            if list_len > 0 && (4..4 + list_len).contains(&row) {
                Some((0, Some(row - 4)))
            } else {
                match row {
                    r if r == 5 + list_len => Some((1, None)),
                    r if r == 6 + list_len => Some((2, None)),
                    r if r == 7 + list_len => Some((3, None)),
                    r if r == 8 + list_len => Some((4, None)),
                    _ => None,
                }
            }
        }
        SettingsTab::Concierge => row
            .checked_sub(4)
            .filter(|idx| *idx < 4)
            .map(|idx| (idx, None)),
    }
}

fn auth_row_action_offsets(
    content_area: Rect,
    entry: &crate::state::auth::ProviderAuthEntry,
) -> (u16, u16, u16) {
    let primary_label = if entry.authenticated {
        "[Logout]"
    } else {
        "[Login]"
    };
    let test_label = "[Test]";
    let actions_width = primary_label.chars().count() as u16 + 1 + test_label.chars().count() as u16;
    let primary_start = content_area
        .x
        .saturating_add(content_area.width.saturating_sub(actions_width));
    let primary_end = primary_start.saturating_add(primary_label.chars().count() as u16);
    let test_start = primary_end.saturating_add(1);
    (primary_start, primary_end, test_start)
}

fn auth_hit_test(
    content_area: Rect,
    auth: &crate::state::auth::AuthState,
    mouse: Position,
) -> Option<SettingsHitTarget> {
    let row = mouse.y.saturating_sub(content_area.y) as usize;
    let entry_index = row.checked_sub(4)?;
    let entry = auth.entries.get(entry_index)?;
    let (primary_start, primary_end, test_start) = auth_row_action_offsets(content_area, entry);
    if mouse.x >= primary_start && mouse.x < primary_end {
        Some(SettingsHitTarget::AuthAction {
            index: entry_index,
            action: AuthTabAction::Primary,
        })
    } else if mouse.x >= test_start {
        Some(SettingsHitTarget::AuthAction {
            index: entry_index,
            action: AuthTabAction::Test,
        })
    } else {
        Some(SettingsHitTarget::AuthProviderItem(entry_index))
    }
}

fn subagent_row_action_offsets(
    content_area: Rect,
    entry: &crate::state::subagents::SubAgentEntry,
) -> (u16, u16, u16, u16, u16) {
    let edit_label = "[Edit]";
    let delete_label = "[Delete]";
    let toggle_label = if entry.enabled { "[Disable]" } else { "[Enable]" };
    let actions_width = edit_label.chars().count() as u16
        + 1
        + delete_label.chars().count() as u16
        + 1
        + toggle_label.chars().count() as u16;
    let edit_start = content_area
        .x
        .saturating_add(content_area.width.saturating_sub(actions_width));
    let delete_start = edit_start.saturating_add(edit_label.chars().count() as u16 + 1);
    let toggle_start = delete_start.saturating_add(delete_label.chars().count() as u16 + 1);
    (
        edit_start,
        delete_start,
        toggle_start,
        delete_start.saturating_sub(1),
        toggle_start.saturating_add(toggle_label.chars().count() as u16),
    )
}

fn subagents_hit_test(
    content_area: Rect,
    subagents: &SubAgentsState,
    mouse: Position,
) -> Option<SettingsHitTarget> {
    let row = mouse.y.saturating_sub(content_area.y) as usize;
    let list_len = subagents.entries.len();
    if list_len > 0 && (4..4 + list_len).contains(&row) {
        let index = row - 4;
        if let Some(entry) = subagents.entries.get(index) {
            let (edit_start, delete_start, toggle_start, _, toggle_end) =
                subagent_row_action_offsets(content_area, entry);
            if mouse.x >= edit_start && mouse.x < delete_start.saturating_sub(1) {
                return Some(SettingsHitTarget::SubAgentRowAction {
                    index,
                    action: SubAgentTabAction::Edit,
                });
            }
            if mouse.x >= delete_start && mouse.x < toggle_start.saturating_sub(1) {
                return Some(SettingsHitTarget::SubAgentRowAction {
                    index,
                    action: SubAgentTabAction::Delete,
                });
            }
            if mouse.x >= toggle_start && mouse.x < toggle_end {
                return Some(SettingsHitTarget::SubAgentRowAction {
                    index,
                    action: SubAgentTabAction::Toggle,
                });
            }
        }
        return Some(SettingsHitTarget::SubAgentListItem(index));
    }
    match row {
        r if r == 5 + list_len => Some(SettingsHitTarget::SubAgentAction(SubAgentTabAction::Add)),
        _ => None,
    }
}

fn render_tab_content<'a>(
    content_width: u16,
    settings: &'a SettingsState,
    config: &'a ConfigState,
    auth: &'a crate::state::auth::AuthState,
    subagents: &'a SubAgentsState,
    concierge: &'a ConciergeState,
    theme: &ThemeTokens,
) -> Vec<Line<'a>> {
    match settings.active_tab() {
        SettingsTab::Provider => render_provider_tab(settings, config, theme),
        SettingsTab::Tools => render_tools_tab(settings, config, theme),
        SettingsTab::WebSearch => render_websearch_tab(settings, config, theme),
        SettingsTab::Chat => render_chat_tab(settings, config, theme),
        SettingsTab::Gateway => render_gateway_tab(settings, config, theme),
        SettingsTab::Auth => render_auth_tab(content_width, auth, theme),
        SettingsTab::Agent => render_agent_tab(settings, config, theme),
        SettingsTab::SubAgents => render_subagents_tab(content_width, subagents, theme),
        SettingsTab::Concierge => render_concierge_tab(settings, concierge, theme),
        SettingsTab::Advanced => render_advanced_tab(settings, config, theme),
    }
}

fn render_provider_tab<'a>(
    settings: &'a SettingsState,
    config: &'a ConfigState,
    theme: &ThemeTokens,
) -> Vec<Line<'a>> {
    let mut lines = Vec::new();

    lines.push(Line::raw(""));
    lines.push(Line::from(Span::styled("  Provider", theme.fg_active)));
    lines.push(Line::from(Span::styled(
        "  Select your LLM provider and credentials",
        theme.fg_dim,
    )));
    lines.push(Line::raw(""));

    let provider_val = if config.provider().is_empty() {
        "(not set)".to_string()
    } else {
        config.provider().to_string()
    };
    let base_url_val = if config.base_url().is_empty() {
        "(not set)".to_string()
    } else {
        config.base_url().to_string()
    };
    let model_val = if config.model().is_empty() {
        "(not set)".to_string()
    } else {
        config.model().to_string()
    };
    let api_key_label = if config.auth_source == "chatgpt_subscription" {
        "ChatGPT Auth"
    } else {
        "API Key"
    };
    let api_key_val = if config.auth_source == "chatgpt_subscription" {
        if config.chatgpt_auth_available {
            format!(
                "connected{}",
                config
                    .chatgpt_auth_source
                    .as_deref()
                    .map(|source| format!(" ({source})"))
                    .unwrap_or_default()
            )
        } else {
            "not authenticated".to_string()
        }
    } else {
        mask_api_key(config.api_key())
    };
    let auth_source_val = match config.auth_source.as_str() {
        "chatgpt_subscription" => "ChatGPT subscription".to_string(),
        _ => "API key".to_string(),
    };
    let transport_val = if config.api_transport().is_empty() {
        providers::default_transport_for(&config.provider).to_string()
    } else {
        match config.api_transport() {
            "native_assistant" => "native assistant".to_string(),
            "responses" => "responses".to_string(),
            _ => "chat completions".to_string(),
        }
    };
    let assistant_id_val = if config.assistant_id.is_empty() {
        "(not set)".to_string()
    } else {
        config.assistant_id.clone()
    };
    let effort_val = if config.reasoning_effort().is_empty() {
        "off".to_string()
    } else {
        config.reasoning_effort().to_string()
    };
    let context_window_val = format!("{} tok", config.context_window_tokens);
    let api_key_hint = if config.auth_source == "chatgpt_subscription" {
        if config.chatgpt_auth_available {
            " [Enter: logout]"
        } else {
            " [Enter: login]"
        }
    } else {
        " [Enter: edit]"
    };
    let context_hint = if config.provider == "custom" {
        " [Enter: edit]"
    } else {
        ""
    };

    // Field definitions: (index, label, value, field_name, hint)
    let fields: [(usize, &str, String, &str, &str); 9] = [
        (0, "Provider", provider_val, "provider", " [Enter: pick]"),
        (1, "Base URL", base_url_val, "base_url", " [Enter: edit]"),
        (2, api_key_label, api_key_val, "api_key", api_key_hint),
        (3, "Auth", auth_source_val, "auth_source", " [Enter: cycle]"),
        (4, "Model", model_val, "model", " [Enter: pick]"),
        (
            5,
            "Transport",
            transport_val,
            "api_transport",
            " [Enter: cycle]",
        ),
        (
            6,
            "Assistant ID",
            assistant_id_val,
            "assistant_id",
            " [Enter: edit]",
        ),
        (
            7,
            "Effort",
            effort_val,
            "reasoning_effort",
            " [Enter: pick]",
        ),
        (
            8,
            "Ctx Length",
            context_window_val,
            "context_window_tokens",
            context_hint,
        ),
    ];

    for (idx, label, value, field_name, hint) in &fields {
        let is_selected = settings.field_cursor() == *idx;
        let is_editing = settings.is_editing() && settings.editing_field() == Some(*field_name);

        let marker = if is_selected { ">" } else { " " };

        let display_value: String = if is_editing {
            // Show edit buffer with cursor block
            if *field_name == "api_key" {
                // Show raw characters while editing API key
                clip_inline_text(&format!("{}\u{2588}", settings.edit_buffer()), 52)
            } else {
                clip_inline_text(&format!("{}\u{2588}", settings.edit_buffer()), 52)
            }
        } else {
            clip_inline_text(value, 52)
        };

        let marker_style = if is_selected {
            theme.accent_primary
        } else {
            theme.fg_dim
        };
        let value_style = if is_editing {
            theme.fg_active
        } else if is_selected {
            theme.accent_primary
        } else {
            theme.fg_active
        };

        let mut spans = vec![
            Span::styled(format!(" {} ", marker), marker_style),
            Span::styled(format!("{:<15} ", label), theme.fg_dim),
            Span::styled(display_value, value_style),
        ];

        // Show hint on selected but not editing
        if is_selected && !is_editing {
            spans.push(Span::styled(*hint, theme.fg_dim));
        }

        lines.push(Line::from(spans));
    }

    lines
}

fn render_tools_tab<'a>(
    settings: &'a SettingsState,
    config: &'a ConfigState,
    theme: &ThemeTokens,
) -> Vec<Line<'a>> {
    let mut lines = Vec::new();

    lines.push(Line::raw(""));
    lines.push(Line::from(Span::styled("  Tools", theme.fg_active)));
    lines.push(Line::from(Span::styled(
        "  Enable or disable tool categories",
        theme.fg_dim,
    )));
    lines.push(Line::raw(""));

    let tools: [(bool, &str); 7] = [
        (config.tool_bash, "Terminal / Bash"),
        (config.tool_file_ops, "File Operations"),
        (config.tool_web_search, "Web Search"),
        (config.tool_web_browse, "Web Browse"),
        (config.tool_vision, "Vision"),
        (config.tool_system_info, "System Info"),
        (config.tool_gateway, "Gateway Messaging"),
    ];

    for (i, (enabled, name)) in tools.iter().enumerate() {
        let is_selected = settings.field_cursor() == i;
        let check = if *enabled { "[x]" } else { "[ ]" };
        let marker = if is_selected { "> " } else { "  " };

        let marker_style = if is_selected {
            theme.accent_primary
        } else {
            theme.fg_dim
        };
        let check_style = if *enabled {
            theme.accent_success
        } else {
            theme.fg_dim
        };
        let label_style = if is_selected {
            theme.accent_primary
        } else {
            theme.fg_active
        };

        let mut spans = vec![
            Span::styled(marker, marker_style),
            Span::styled(check, check_style),
            Span::raw(" "),
            Span::styled(*name, label_style),
        ];
        if is_selected {
            spans.push(Span::styled("  [Space: toggle]", theme.fg_dim));
        }
        lines.push(Line::from(spans));
    }

    lines
}

fn render_websearch_tab<'a>(
    settings: &'a SettingsState,
    config: &'a ConfigState,
    theme: &ThemeTokens,
) -> Vec<Line<'a>> {
    let mut lines = Vec::new();

    lines.push(Line::raw(""));
    lines.push(Line::from(Span::styled("  Web Search", theme.fg_active)));
    lines.push(Line::from(Span::styled(
        "  Configure web search tool and providers",
        theme.fg_dim,
    )));
    lines.push(Line::raw(""));

    // Field 0: web_search_enabled (checkbox — mirrors tool_web_search)
    {
        let is_selected = settings.field_cursor() == 0;
        let marker = if is_selected { "> " } else { "  " };
        let marker_style = if is_selected {
            theme.accent_primary
        } else {
            theme.fg_dim
        };
        let check = if config.tool_web_search { "[x]" } else { "[ ]" };
        let check_style = if config.tool_web_search {
            theme.accent_success
        } else {
            theme.fg_dim
        };
        let label_style = if is_selected {
            theme.accent_primary
        } else {
            theme.fg_active
        };
        let mut spans = vec![
            Span::styled(marker, marker_style),
            Span::styled(check, check_style),
            Span::raw(" "),
            Span::styled("Enable Web Search", label_style),
        ];
        if is_selected {
            spans.push(Span::styled("  [Space: toggle]", theme.fg_dim));
        }
        lines.push(Line::from(spans));
    }

    // Field 1: search_provider (cycle on Enter)
    {
        let is_selected = settings.field_cursor() == 1;
        let marker = if is_selected { "> " } else { "  " };
        let marker_style = if is_selected {
            theme.accent_primary
        } else {
            theme.fg_dim
        };
        let provider_val = if config.search_provider.is_empty() || config.search_provider == "none"
        {
            "none".to_string()
        } else {
            config.search_provider.clone()
        };
        let value_style = if is_selected {
            theme.accent_primary
        } else {
            theme.fg_active
        };
        let mut spans = vec![
            Span::styled(marker, marker_style),
            Span::styled(format!("{:<16} ", "Provider:"), theme.fg_dim),
            Span::styled(provider_val, value_style),
        ];
        if is_selected {
            spans.push(Span::styled("  [Enter: cycle]", theme.fg_dim));
        }
        lines.push(Line::from(spans));
    }

    // Fields 2–4: API keys (masked, inline edit)
    let api_key_fields: [(usize, &str, &str, &str); 3] = [
        (
            2,
            "Firecrawl Key:  ",
            config.firecrawl_api_key.as_str(),
            "firecrawl_api_key",
        ),
        (
            3,
            "Exa Key:        ",
            config.exa_api_key.as_str(),
            "exa_api_key",
        ),
        (
            4,
            "Tavily Key:     ",
            config.tavily_api_key.as_str(),
            "tavily_api_key",
        ),
    ];

    for (idx, label, value, field_name) in &api_key_fields {
        let is_selected = settings.field_cursor() == *idx;
        let is_editing = settings.is_editing() && settings.editing_field() == Some(field_name);
        let marker = if is_selected { "> " } else { "  " };
        let marker_style = if is_selected {
            theme.accent_primary
        } else {
            theme.fg_dim
        };

        let display_value: String = if is_editing {
            format!("{}\u{2588}", settings.edit_buffer())
        } else if value.is_empty() {
            "(not set)".to_string()
        } else {
            mask_api_key(value)
        };

        let value_style = if is_editing {
            theme.fg_active
        } else if is_selected {
            theme.accent_primary
        } else {
            theme.fg_active
        };

        let mut spans = vec![
            Span::styled(marker, marker_style),
            Span::styled(format!("{:<16} ", label), theme.fg_dim),
            Span::styled(display_value, value_style),
        ];
        if is_selected && !is_editing {
            spans.push(Span::styled("  [Enter: edit]", theme.fg_dim));
        }
        lines.push(Line::from(spans));
    }

    // Field 5: search_max_results (numeric inline edit)
    {
        let is_selected = settings.field_cursor() == 5;
        let is_editing =
            settings.is_editing() && settings.editing_field() == Some("search_max_results");
        let marker = if is_selected { "> " } else { "  " };
        let marker_style = if is_selected {
            theme.accent_primary
        } else {
            theme.fg_dim
        };

        let display_value: String = if is_editing {
            format!("{}\u{2588}", settings.edit_buffer())
        } else {
            config.search_max_results.to_string()
        };

        let value_style = if is_editing {
            theme.fg_active
        } else if is_selected {
            theme.accent_primary
        } else {
            theme.fg_active
        };

        let mut spans = vec![
            Span::styled(marker, marker_style),
            Span::styled(format!("{:<16} ", "Max Results:"), theme.fg_dim),
            Span::styled(display_value, value_style),
        ];
        if is_selected && !is_editing {
            spans.push(Span::styled("  [Enter: edit]", theme.fg_dim));
        }
        lines.push(Line::from(spans));
    }

    // Field 6: search_timeout_secs (numeric inline edit)
    {
        let is_selected = settings.field_cursor() == 6;
        let is_editing =
            settings.is_editing() && settings.editing_field() == Some("search_timeout");
        let marker = if is_selected { "> " } else { "  " };
        let marker_style = if is_selected {
            theme.accent_primary
        } else {
            theme.fg_dim
        };

        let display_value: String = if is_editing {
            format!("{}\u{2588}", settings.edit_buffer())
        } else {
            format!("{}s", config.search_timeout_secs)
        };

        let value_style = if is_editing {
            theme.fg_active
        } else if is_selected {
            theme.accent_primary
        } else {
            theme.fg_active
        };

        let mut spans = vec![
            Span::styled(marker, marker_style),
            Span::styled(format!("{:<16} ", "Timeout:"), theme.fg_dim),
            Span::styled(display_value, value_style),
        ];
        if is_selected && !is_editing {
            spans.push(Span::styled("  [Enter: edit]", theme.fg_dim));
        }
        lines.push(Line::from(spans));
    }

    lines
}

fn render_chat_tab<'a>(
    settings: &'a SettingsState,
    config: &'a ConfigState,
    theme: &ThemeTokens,
) -> Vec<Line<'a>> {
    let mut lines = Vec::new();

    lines.push(Line::raw(""));
    lines.push(Line::from(Span::styled("  Chat", theme.fg_active)));
    lines.push(Line::from(Span::styled(
        "  Configure streaming and memory",
        theme.fg_dim,
    )));
    lines.push(Line::raw(""));

    // Fields 0–2: toggles
    let toggles: [(usize, bool, &str, &str); 3] = [
        (0, config.enable_streaming, "Streaming", "enable_streaming"),
        (
            1,
            config.enable_conversation_memory,
            "Conversation Memory",
            "enable_conversation_memory",
        ),
        (
            2,
            config.enable_honcho_memory,
            "Honcho Memory",
            "enable_honcho_memory",
        ),
    ];
    for (idx, enabled, name, _field_name) in &toggles {
        let is_selected = settings.field_cursor() == *idx;
        let check = if *enabled { "[x]" } else { "[ ]" };
        let marker = if is_selected { "> " } else { "  " };
        let marker_style = if is_selected {
            theme.accent_primary
        } else {
            theme.fg_dim
        };
        let check_style = if *enabled {
            theme.accent_success
        } else {
            theme.fg_dim
        };
        let label_style = if is_selected {
            theme.accent_primary
        } else {
            theme.fg_active
        };
        let mut spans = vec![
            Span::styled(marker, marker_style),
            Span::styled(check, check_style),
            Span::raw(" "),
            Span::styled(*name, label_style),
        ];
        if is_selected {
            spans.push(Span::styled("  [Space: toggle]", theme.fg_dim));
        }
        lines.push(Line::from(spans));
    }

    // Fields 3–5: text / password fields
    let text_fields: [(usize, &str, &str, &str, bool); 3] = [
        (
            3,
            "Honcho API Key:  ",
            config.honcho_api_key.as_str(),
            "honcho_api_key",
            true,
        ),
        (
            4,
            "Honcho Base URL: ",
            config.honcho_base_url.as_str(),
            "honcho_base_url",
            false,
        ),
        (
            5,
            "Honcho Workspace:",
            config.honcho_workspace_id.as_str(),
            "honcho_workspace_id",
            false,
        ),
    ];
    for (idx, label, value, field_name, password) in &text_fields {
        render_gateway_text_field(
            settings, theme, &mut lines, *idx, label, value, field_name, *password,
        );
    }

    lines
}

fn render_subagents_tab<'a>(
    content_width: u16,
    subagents: &'a crate::state::subagents::SubAgentsState,
    theme: &ThemeTokens,
) -> Vec<Line<'a>> {
    let mut lines = Vec::new();

    lines.push(Line::raw(""));
    lines.push(Line::from(Span::styled("  Sub-Agents", theme.fg_active)));
    lines.push(Line::from(Span::styled(
        "  Manage orchestration sub-agent definitions",
        theme.fg_dim,
    )));
    lines.push(Line::raw(""));

    if let Some(editor) = subagents.editor.as_ref() {
        let role_label = crate::state::subagents::find_role_preset(&editor.role)
            .map(|preset| preset.label)
            .unwrap_or_else(|| {
                if editor.role.trim().is_empty() {
                    "None"
                } else {
                    "Custom"
                }
            });
        let field_line = |selected: bool, label: &str, value: String| {
            Line::from(vec![
                Span::styled(if selected { "> " } else { "  " }, if selected { theme.fg_active } else { theme.fg_dim }),
                Span::styled(format!("{label:<14}"), theme.fg_dim),
                Span::styled(value, if selected { theme.fg_active } else { Style::default().fg(Color::White) }),
            ])
        };

        lines.push(field_line(
            matches!(editor.field, crate::state::subagents::SubAgentEditorField::Name),
            "Name",
            editor.name.clone(),
        ));
        lines.push(field_line(
            matches!(editor.field, crate::state::subagents::SubAgentEditorField::Provider),
            "Provider",
            if editor.provider.is_empty() { "Select provider".to_string() } else { editor.provider.clone() },
        ));
        lines.push(field_line(
            matches!(editor.field, crate::state::subagents::SubAgentEditorField::Model),
            "Model",
            if editor.model.is_empty() { "Select model".to_string() } else { editor.model.clone() },
        ));
        lines.push(field_line(
            matches!(editor.field, crate::state::subagents::SubAgentEditorField::Role),
            "Role",
            format!("{role_label} ({})", if editor.role.is_empty() { "none" } else { &editor.role }),
        ));
        lines.push(Line::raw(""));
        lines.push(Line::from(vec![
            Span::styled(
                if matches!(
                    editor.field,
                    crate::state::subagents::SubAgentEditorField::SystemPrompt
                ) {
                    "> "
                } else {
                    "  "
                },
                if matches!(
                    editor.field,
                    crate::state::subagents::SubAgentEditorField::SystemPrompt
                ) {
                    theme.fg_active
                } else {
                    theme.fg_dim
                },
            ),
            Span::styled("System Prompt", theme.fg_dim),
        ]));
        for line in wrap_text(
            if editor.system_prompt.trim().is_empty() {
                "Optional override. Use Enter to edit."
            } else {
                &editor.system_prompt
            },
            (content_width as usize).saturating_sub(4).max(20),
        ) {
            lines.push(Line::from(Span::styled(
                format!("    {line}"),
                if editor.system_prompt.trim().is_empty() {
                    theme.fg_dim
                } else {
                    Style::default().fg(Color::White)
                },
            )));
        }
        lines.push(Line::raw(""));
        lines.push(Line::from(vec![
            Span::styled(
                if matches!(editor.field, crate::state::subagents::SubAgentEditorField::Save) {
                    "> "
                } else {
                    "  "
                },
                if matches!(editor.field, crate::state::subagents::SubAgentEditorField::Save) {
                    theme.fg_active
                } else {
                    theme.fg_dim
                },
            ),
            Span::styled("[Save]", if matches!(editor.field, crate::state::subagents::SubAgentEditorField::Save) { theme.fg_active } else { theme.fg_dim }),
            Span::raw("  "),
            Span::styled(
                "[Cancel]",
                if matches!(
                    editor.field,
                    crate::state::subagents::SubAgentEditorField::Cancel
                ) {
                    theme.fg_active
                } else {
                    theme.fg_dim
                },
            ),
        ]));
        lines.push(Line::raw(""));
        lines.push(Line::from(vec![
            Span::styled("  ↑↓", theme.fg_active),
            Span::styled(" move  ", theme.fg_dim),
            Span::styled("Enter", theme.fg_active),
            Span::styled(" edit/open  ", theme.fg_dim),
            Span::styled("←→", theme.fg_active),
            Span::styled(" role preset  ", theme.fg_dim),
            Span::styled("s", theme.fg_active),
            Span::styled(" save  ", theme.fg_dim),
            Span::styled("Esc", theme.fg_active),
            Span::styled(" cancel", theme.fg_dim),
        ]));
        return lines;
    }

    if subagents.entries.is_empty() {
        lines.push(Line::from(Span::styled(
            "  No sub-agents configured.",
            theme.fg_dim,
        )));
    } else {
        // Field 0: subagent_list
        for (i, entry) in subagents.entries.iter().enumerate() {
            let is_selected = subagents.selected == i;
            let marker = if is_selected { "> " } else { "  " };
            let dot = if entry.enabled { "● " } else { "○ " };
            let dot_style = if entry.enabled {
                Style::default().fg(Color::Green)
            } else {
                theme.fg_dim
            };
            let role_str = entry.role.as_deref().map(|r| format!(" [{}]", r)).unwrap_or_default();
            let edit_label = "[Edit]";
            let delete_label = "[Delete]";
            let toggle_label = if entry.enabled { "[Disable]" } else { "[Enable]" };
            let left_width = marker.chars().count()
                + dot.chars().count()
                + entry.name.chars().count()
                + format!(" ({}/{})", entry.provider, entry.model).chars().count()
                + role_str.chars().count();
            let actions_width = edit_label.chars().count()
                + 1
                + delete_label.chars().count()
                + 1
                + toggle_label.chars().count();
            let spacer = " ".repeat(
                (content_width as usize)
                    .saturating_sub(left_width + actions_width)
                    .max(1),
            );

            let line = Line::from(vec![
                Span::styled(marker, if is_selected { theme.fg_active } else { theme.fg_dim }),
                Span::styled(dot, dot_style),
                Span::styled(
                    entry.name.clone(),
                    if is_selected { theme.fg_active } else { Style::default().fg(Color::White) },
                ),
                Span::styled(
                    format!(" ({}/{})", entry.provider, entry.model),
                    theme.fg_dim,
                ),
                Span::styled(role_str, Style::default().fg(Color::Cyan)),
                Span::raw(spacer),
                Span::styled(
                    edit_label,
                    if is_selected && subagents.actions_focused && subagents.action_cursor == 1 {
                        theme.fg_active
                    } else {
                        theme.fg_dim
                    },
                ),
                Span::raw(" "),
                Span::styled(
                    delete_label,
                    if is_selected && subagents.actions_focused && subagents.action_cursor == 2 {
                        theme.fg_active
                    } else {
                        theme.fg_dim
                    },
                ),
                Span::raw(" "),
                Span::styled(
                    toggle_label,
                    if is_selected && subagents.actions_focused && subagents.action_cursor == 3 {
                        theme.fg_active
                    } else {
                        theme.fg_dim
                    },
                ),
            ]);
            lines.push(line);
        }
    }

    lines.push(Line::raw(""));

    // Field 1: subagent_add
    {
        let is_selected = subagents.actions_focused && subagents.action_cursor == 0;
        let marker = if is_selected { "> " } else { "  " };
        lines.push(Line::from(Span::styled(
            format!("{}[Add Sub-Agent]", marker),
            if is_selected { theme.fg_active } else { theme.fg_dim },
        )));
    }
    lines.push(Line::from(vec![
        Span::styled("  ", theme.fg_dim),
        Span::styled("a", theme.fg_active),
        Span::styled(" add  ", theme.fg_dim),
        Span::styled("e", theme.fg_active),
        Span::styled(" edit  ", theme.fg_dim),
        Span::styled("d", theme.fg_active),
        Span::styled(" delete  ", theme.fg_dim),
        Span::styled("Space", theme.fg_active),
        Span::styled(" toggle  ", theme.fg_dim),
        Span::styled("←→", theme.fg_active),
        Span::styled(" row action", theme.fg_dim),
    ]));

    lines
}

fn render_concierge_tab<'a>(
    settings: &'a SettingsState,
    concierge: &'a ConciergeState,
    theme: &ThemeTokens,
) -> Vec<Line<'a>> {
    let mut lines = Vec::new();
    lines.push(Line::raw(""));
    lines.push(Line::from(Span::styled("  Concierge", theme.fg_active)));
    lines.push(Line::from(Span::styled(
        "  Welcome agent and operational assistant",
        theme.fg_dim,
    )));
    lines.push(Line::raw(""));

    // Field 0: concierge_enabled
    {
        let is_selected = settings.field_cursor() == 0;
        let marker = if is_selected { "> " } else { "  " };
        let check = if concierge.enabled { "[x]" } else { "[ ]" };
        lines.push(Line::from(vec![
            Span::styled(marker, if is_selected { theme.fg_active } else { theme.fg_dim }),
            Span::styled(check, if concierge.enabled { theme.accent_success } else { theme.fg_dim }),
            Span::raw(" "),
            Span::styled("Enabled", if is_selected { theme.fg_active } else { theme.fg_dim }),
        ]));
    }

    // Field 1: concierge_detail_level
    {
        let is_selected = settings.field_cursor() == 1;
        let marker = if is_selected { "> " } else { "  " };
        lines.push(Line::from(vec![
            Span::styled(marker, if is_selected { theme.fg_active } else { theme.fg_dim }),
            Span::styled("Detail Level: ", theme.fg_dim),
            Span::styled(
                concierge.detail_level.clone(),
                if is_selected { theme.fg_active } else { theme.fg_dim },
            ),
        ]));
    }

    // Field 2: concierge_provider
    {
        let is_selected = settings.field_cursor() == 2;
        let marker = if is_selected { "> " } else { "  " };
        lines.push(Line::from(vec![
            Span::styled(marker, if is_selected { theme.fg_active } else { theme.fg_dim }),
            Span::styled("Provider:     ", theme.fg_dim),
            Span::styled(
                concierge.provider.clone().unwrap_or_else(|| "(default)".to_string()),
                if is_selected { theme.fg_active } else { theme.fg_dim },
            ),
        ]));
    }

    // Field 3: concierge_model
    {
        let is_selected = settings.field_cursor() == 3;
        let marker = if is_selected { "> " } else { "  " };
        lines.push(Line::from(vec![
            Span::styled(marker, if is_selected { theme.fg_active } else { theme.fg_dim }),
            Span::styled("Model:        ", theme.fg_dim),
            Span::styled(
                concierge.model.clone().unwrap_or_else(|| "(default)".to_string()),
                if is_selected { theme.fg_active } else { theme.fg_dim },
            ),
        ]));
    }

    lines
}

fn render_advanced_tab<'a>(
    settings: &'a SettingsState,
    config: &'a ConfigState,
    theme: &ThemeTokens,
) -> Vec<Line<'a>> {
    let mut lines = Vec::new();

    lines.push(Line::raw(""));
    lines.push(Line::from(Span::styled("  Advanced", theme.fg_active)));
    lines.push(Line::from(Span::styled(
        "  Context compaction and retry settings",
        theme.fg_dim,
    )));
    lines.push(Line::raw(""));

    // Field 0: auto_compact_context (toggle)
    {
        let is_selected = settings.field_cursor() == 0;
        let marker = if is_selected { "> " } else { "  " };
        let marker_style = if is_selected {
            theme.accent_primary
        } else {
            theme.fg_dim
        };
        let check = if config.auto_compact_context {
            "[x]"
        } else {
            "[ ]"
        };
        let check_style = if config.auto_compact_context {
            theme.accent_success
        } else {
            theme.fg_dim
        };
        let label_style = if is_selected {
            theme.accent_primary
        } else {
            theme.fg_active
        };
        let mut spans = vec![
            Span::styled(marker, marker_style),
            Span::styled(check, check_style),
            Span::raw(" "),
            Span::styled("Auto Compact Context", label_style),
        ];
        if is_selected {
            spans.push(Span::styled("  [Space: toggle]", theme.fg_dim));
        }
        lines.push(Line::from(spans));
    }

    // Fields 1–9: numeric inline-edit fields
    let numeric_fields: [(usize, &str, String, &str); 9] = [
        (
            1,
            "Max Context Msgs:",
            config.max_context_messages.to_string(),
            "max_context_messages",
        ),
        (
            2,
            "Max Tool Loops:  ",
            config.max_tool_loops.to_string(),
            "max_tool_loops",
        ),
        (
            3,
            "Max Retries:     ",
            config.max_retries.to_string(),
            "max_retries",
        ),
        (
            4,
            "Retry Delay (ms):",
            config.retry_delay_ms.to_string(),
            "retry_delay_ms",
        ),
        (
            5,
            "Context Len Tok: ",
            config.context_window_tokens.to_string(),
            "context_window_tokens",
        ),
        (
            6,
            "Budget Tokens:   ",
            config.context_budget_tokens.to_string(),
            "context_budget_tokens",
        ),
        (
            7,
            "Compact Thres %: ",
            config.compact_threshold_pct.to_string(),
            "compact_threshold_pct",
        ),
        (
            8,
            "Keep Recent:     ",
            config.keep_recent_on_compact.to_string(),
            "keep_recent_on_compact",
        ),
        (
            9,
            "Bash Timeout (s):",
            config.bash_timeout_secs.to_string(),
            "bash_timeout_secs",
        ),
    ];
    for (idx, label, value, field_name) in &numeric_fields {
        let is_selected = settings.field_cursor() == *idx;
        let is_editing = settings.is_editing() && settings.editing_field() == Some(field_name);
        let marker = if is_selected { "> " } else { "  " };
        let marker_style = if is_selected {
            theme.accent_primary
        } else {
            theme.fg_dim
        };
        let display_value: String = if is_editing {
            format!("{}\u{2588}", settings.edit_buffer())
        } else {
            value.clone()
        };
        let value_style = if is_editing {
            theme.fg_active
        } else if is_selected {
            theme.accent_primary
        } else {
            theme.fg_active
        };
        let mut spans = vec![
            Span::styled(marker, marker_style),
            Span::styled(format!("{:<17} ", label), theme.fg_dim),
            Span::styled(display_value, value_style),
        ];
        if is_selected && !is_editing {
            spans.push(Span::styled("  [Enter: edit]", theme.fg_dim));
        }
        lines.push(Line::from(spans));
    }

    // ── Snapshot Retention section ───────────────────────────────────────────
    lines.push(Line::raw(""));
    lines.push(Line::from(Span::styled(
        "  \u{2500}\u{2500} Snapshot Retention \u{2500}\u{2500}",
        theme.fg_dim,
    )));

    // Field 10: snapshot_auto_cleanup (toggle)
    {
        let is_selected = settings.field_cursor() == 10;
        let marker = if is_selected { "> " } else { "  " };
        let marker_style = if is_selected {
            theme.accent_primary
        } else {
            theme.fg_dim
        };
        let check = if config.snapshot_auto_cleanup {
            "[x]"
        } else {
            "[ ]"
        };
        let check_style = if config.snapshot_auto_cleanup {
            theme.accent_success
        } else {
            theme.fg_dim
        };
        let label_style = if is_selected {
            theme.accent_primary
        } else {
            theme.fg_active
        };
        let mut spans = vec![
            Span::styled(marker, marker_style),
            Span::styled(check, check_style),
            Span::raw(" "),
            Span::styled("Auto Cleanup", label_style),
        ];
        if is_selected {
            spans.push(Span::styled("  [Space: toggle]", theme.fg_dim));
        }
        lines.push(Line::from(spans));
    }

    // Fields 11-12: snapshot numeric fields
    let snapshot_fields: [(usize, &str, String, &str); 2] = [
        (
            11,
            "Max Snapshots:   ",
            config.snapshot_max_count.to_string(),
            "snapshot_max_count",
        ),
        (
            12,
            "Max Size (MB):   ",
            config.snapshot_max_size_mb.to_string(),
            "snapshot_max_size_mb",
        ),
    ];
    for (idx, label, value, field_name) in &snapshot_fields {
        let is_selected = settings.field_cursor() == *idx;
        let is_editing = settings.is_editing() && settings.editing_field() == Some(field_name);
        let marker = if is_selected { "> " } else { "  " };
        let marker_style = if is_selected {
            theme.accent_primary
        } else {
            theme.fg_dim
        };
        let display_value: String = if is_editing {
            format!("{}\u{2588}", settings.edit_buffer())
        } else {
            value.clone()
        };
        let value_style = if is_editing {
            theme.fg_active
        } else if is_selected {
            theme.accent_primary
        } else {
            theme.fg_active
        };
        let mut spans = vec![
            Span::styled(marker, marker_style),
            Span::styled(format!("{:<17} ", label), theme.fg_dim),
            Span::styled(display_value, value_style),
        ];
        if is_selected && !is_editing {
            spans.push(Span::styled("  [Enter: edit]", theme.fg_dim));
        }
        lines.push(Line::from(spans));
    }

    // Field 13: snapshot_stats (read-only info line)
    {
        let is_selected = settings.field_cursor() == 13;
        let marker = if is_selected { "> " } else { "  " };
        let marker_style = if is_selected {
            theme.accent_primary
        } else {
            theme.fg_dim
        };
        let size_display = if config.snapshot_total_size_bytes >= 1024 * 1024 * 1024 {
            format!(
                "{:.1} GB",
                config.snapshot_total_size_bytes as f64 / (1024.0 * 1024.0 * 1024.0)
            )
        } else {
            format!(
                "{:.1} MB",
                config.snapshot_total_size_bytes as f64 / (1024.0 * 1024.0)
            )
        };
        let info = format!("{} ({})", config.snapshot_count, size_display);
        let info_style = if is_selected {
            theme.accent_primary
        } else {
            theme.fg_active
        };
        let spans = vec![
            Span::styled(marker, marker_style),
            Span::styled("Snapshots:        ", theme.fg_dim),
            Span::styled(info, info_style),
        ];
        lines.push(Line::from(spans));
    }

    lines
}

fn render_gateway_tab<'a>(
    settings: &'a SettingsState,
    config: &'a ConfigState,
    theme: &ThemeTokens,
) -> Vec<Line<'a>> {
    let mut lines = Vec::new();

    lines.push(Line::raw(""));
    lines.push(Line::from(Span::styled("  Gateway", theme.fg_active)));
    lines.push(Line::from(Span::styled(
        "  Messaging platform connections",
        theme.fg_dim,
    )));
    lines.push(Line::raw(""));

    // ── Field 0: gateway_enabled (toggle) ─────────────────────────────────────
    {
        let is_selected = settings.field_cursor() == 0;
        let marker = if is_selected { "> " } else { "  " };
        let marker_style = if is_selected {
            theme.accent_primary
        } else {
            theme.fg_dim
        };
        let check = if config.gateway_enabled { "[x]" } else { "[ ]" };
        let check_style = if config.gateway_enabled {
            theme.accent_success
        } else {
            theme.fg_dim
        };
        let label_style = if is_selected {
            theme.accent_primary
        } else {
            theme.fg_active
        };
        let mut spans = vec![
            Span::styled(marker, marker_style),
            Span::styled(check, check_style),
            Span::raw(" "),
            Span::styled("Enable Gateway", label_style),
        ];
        if is_selected {
            spans.push(Span::styled("  [Space: toggle]", theme.fg_dim));
        }
        lines.push(Line::from(spans));
    }

    // ── Field 1: gateway_prefix (plain text) ──────────────────────────────────
    render_gateway_text_field(
        settings,
        theme,
        &mut lines,
        1,
        "Command Prefix",
        &config.gateway_prefix,
        "gateway_prefix",
        false,
    );

    // ── Slack section ─────────────────────────────────────────────────────────
    lines.push(Line::raw(""));
    lines.push(Line::from(Span::styled(
        "  \u{2500}\u{2500} Slack \u{2500}\u{2500}",
        theme.fg_dim,
    )));
    render_gateway_text_field(
        settings,
        theme,
        &mut lines,
        2,
        "Bot Token",
        &config.slack_token,
        "slack_token",
        true,
    );
    render_gateway_text_field(
        settings,
        theme,
        &mut lines,
        3,
        "Channel Filter",
        &config.slack_channel_filter,
        "slack_channel_filter",
        false,
    );

    // ── Telegram section ──────────────────────────────────────────────────────
    lines.push(Line::raw(""));
    lines.push(Line::from(Span::styled(
        "  \u{2500}\u{2500} Telegram \u{2500}\u{2500}",
        theme.fg_dim,
    )));
    render_gateway_text_field(
        settings,
        theme,
        &mut lines,
        4,
        "Bot Token",
        &config.telegram_token,
        "telegram_token",
        true,
    );
    render_gateway_text_field(
        settings,
        theme,
        &mut lines,
        5,
        "Allowed Chats",
        &config.telegram_allowed_chats,
        "telegram_allowed_chats",
        false,
    );

    // ── Discord section ───────────────────────────────────────────────────────
    lines.push(Line::raw(""));
    lines.push(Line::from(Span::styled(
        "  \u{2500}\u{2500} Discord \u{2500}\u{2500}",
        theme.fg_dim,
    )));
    render_gateway_text_field(
        settings,
        theme,
        &mut lines,
        6,
        "Bot Token",
        &config.discord_token,
        "discord_token",
        true,
    );
    render_gateway_text_field(
        settings,
        theme,
        &mut lines,
        7,
        "Channel Filter",
        &config.discord_channel_filter,
        "discord_channel_filter",
        false,
    );
    render_gateway_text_field(
        settings,
        theme,
        &mut lines,
        8,
        "Allowed Users",
        &config.discord_allowed_users,
        "discord_allowed_users",
        false,
    );

    // ── WhatsApp section ──────────────────────────────────────────────────────
    lines.push(Line::raw(""));
    lines.push(Line::from(Span::styled(
        "  \u{2500}\u{2500} WhatsApp \u{2500}\u{2500}",
        theme.fg_dim,
    )));
    render_gateway_text_field(
        settings,
        theme,
        &mut lines,
        9,
        "Allowed Contacts",
        &config.whatsapp_allowed_contacts,
        "whatsapp_allowed_contacts",
        false,
    );
    render_gateway_text_field(
        settings,
        theme,
        &mut lines,
        10,
        "API Token",
        &config.whatsapp_token,
        "whatsapp_token",
        true,
    );
    render_gateway_text_field(
        settings,
        theme,
        &mut lines,
        11,
        "Phone Number ID",
        &config.whatsapp_phone_id,
        "whatsapp_phone_id",
        false,
    );

    lines
}

/// Render a single editable gateway field row.
/// `password` — if true and value is non-empty, the stored value is masked (dots).
fn render_gateway_text_field<'a>(
    settings: &SettingsState,
    theme: &ThemeTokens,
    lines: &mut Vec<Line<'a>>,
    field_idx: usize,
    label: &'a str,
    value: &str,
    field_name: &'a str,
    password: bool,
) {
    let is_selected = settings.field_cursor() == field_idx;
    let is_editing = settings.is_editing() && settings.editing_field() == Some(field_name);
    let marker = if is_selected { "> " } else { "  " };
    let marker_style = if is_selected {
        theme.accent_primary
    } else {
        theme.fg_dim
    };

    let display_value: String = if is_editing {
        format!("{}\u{2588}", settings.edit_buffer())
    } else if value.is_empty() {
        "(not set)".to_string()
    } else if password {
        mask_api_key(value)
    } else {
        value.to_string()
    };

    let value_style = if is_editing {
        theme.fg_active
    } else if is_selected {
        theme.accent_primary
    } else {
        theme.fg_active
    };

    let mut spans = vec![
        Span::styled(marker, marker_style),
        Span::styled(format!("{:<16} ", label), theme.fg_dim),
        Span::styled(display_value, value_style),
    ];
    if is_selected && !is_editing {
        spans.push(Span::styled("  [Enter: edit]", theme.fg_dim));
    }
    lines.push(Line::from(spans));
}

fn render_auth_tab<'a>(
    content_width: u16,
    auth: &'a crate::state::auth::AuthState,
    theme: &ThemeTokens,
) -> Vec<Line<'a>> {
    let mut lines = Vec::new();

    lines.push(Line::raw(""));
    lines.push(Line::from(Span::styled("  Authentication", theme.fg_active)));
    lines.push(Line::from(Span::styled(
        "  Provider authentication status",
        theme.fg_dim,
    )));
    lines.push(Line::raw(""));

    if auth.entries.is_empty() {
        lines.push(Line::from(Span::styled(
            "  No providers loaded. Connect to daemon to see status.",
            theme.fg_dim,
        )));
        return lines;
    }

    for (i, entry) in auth.entries.iter().enumerate() {
        let is_selected = auth.selected == i;
        let marker = if is_selected { "> " } else { "  " };
        let dot = if entry.authenticated { "● " } else { "○ " };
        let dot_style = if entry.authenticated {
            Style::default().fg(Color::Green)
        } else {
            theme.fg_dim
        };
        let model_info = if entry.authenticated && !entry.model.is_empty() {
            format!(" ({})", entry.model)
        } else {
            String::new()
        };
        let primary_label = if entry.authenticated {
            "[Logout]"
        } else {
            "[Login]"
        };
        let test_label = "[Test]";
        let left_width = marker.chars().count()
            + dot.chars().count()
            + entry.provider_name.chars().count()
            + model_info.chars().count();
        let actions_width = primary_label.chars().count() + 1 + test_label.chars().count();
        let spacer = " ".repeat(
            (content_width as usize).saturating_sub(left_width + actions_width + 1),
        );

        let line = Line::from(vec![
            Span::styled(marker, if is_selected { theme.fg_active } else { theme.fg_dim }),
            Span::styled(dot, dot_style),
            Span::styled(
                entry.provider_name.clone(),
                if is_selected { theme.fg_active } else { Style::default().fg(Color::White) },
            ),
            Span::styled(model_info, theme.fg_dim),
            Span::raw(spacer),
            Span::styled(
                primary_label,
                if is_selected && auth.actions_focused && auth.action_cursor == 0 {
                    theme.fg_active
                } else {
                    theme.fg_dim
                },
            ),
            Span::raw(" "),
            Span::styled(
                test_label,
                if is_selected && auth.actions_focused && auth.action_cursor == 1 {
                    theme.fg_active
                } else {
                    theme.fg_dim
                },
            ),
        ]);
        lines.push(line);
    }

    lines.push(Line::raw(""));
    lines.push(Line::from(vec![
        Span::styled("  ", theme.fg_dim),
        Span::styled("↑↓", theme.fg_active),
        Span::styled(" provider  ", theme.fg_dim),
        Span::styled("←→", theme.fg_active),
        Span::styled(" action  ", theme.fg_dim),
        Span::styled("Enter", theme.fg_active),
        Span::styled(" run", theme.fg_dim),
    ]));

    lines
}

fn render_agent_tab<'a>(
    settings: &'a SettingsState,
    config: &'a ConfigState,
    theme: &ThemeTokens,
) -> Vec<Line<'a>> {
    let mut lines = Vec::new();

    lines.push(Line::raw(""));
    lines.push(Line::from(Span::styled("  Agent", theme.fg_active)));
    lines.push(Line::from(Span::styled(
        "  Agent identity and behavior",
        theme.fg_dim,
    )));
    lines.push(Line::raw(""));

    let agent_name = if let Some(raw) = config.agent_config_raw() {
        raw.get("agent_name")
            .and_then(|v| v.as_str())
            .unwrap_or("Tamux")
            .to_string()
    } else {
        "Tamux".to_string()
    };

    let system_prompt = if let Some(raw) = config.agent_config_raw() {
        raw.get("system_prompt")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string()
    } else {
        String::new()
    };

    // (field_index, label, value, field_name, hint)
    let editable_fields: [(usize, &str, String, &str, &str); 2] = [
        (
            0,
            "Agent Name    ",
            agent_name,
            "agent_name",
            " [Enter: edit]",
        ),
        (
            1,
            "System Prompt ",
            system_prompt,
            "system_prompt",
            " [Enter: edit]",
        ),
    ];

    for (idx, label, value, field_name, hint) in &editable_fields {
        let is_selected = settings.field_cursor() == *idx;
        let is_editing = settings.is_editing() && settings.editing_field() == Some(field_name);
        let marker = if is_selected { "> " } else { "  " };
        let marker_style = if is_selected {
            theme.accent_primary
        } else {
            theme.fg_dim
        };

        // System prompt: textarea mode when editing
        if *field_name == "system_prompt" && is_editing && settings.is_textarea() {
            lines.push(Line::from(vec![
                Span::styled(marker, marker_style),
                Span::styled(*label, theme.fg_dim),
                Span::styled(" [Ctrl+Enter: save, Esc: cancel]", theme.fg_dim),
            ]));
            // Render the edit buffer as a multi-line textarea with border
            lines.push(Line::from(Span::styled(
                "  ╭──────────────────────────────────────────╮",
                theme.fg_dim,
            )));
            for buf_line in settings.edit_buffer().split('\n') {
                lines.push(Line::from(vec![
                    Span::styled("  │ ", theme.fg_dim),
                    Span::styled(buf_line.to_string(), theme.fg_active),
                ]));
            }
            lines.push(Line::from(vec![
                Span::styled("  │ ", theme.fg_dim),
                Span::raw("\u{2588}"),
            ]));
            lines.push(Line::from(Span::styled(
                "  ╰──────────────────────────────────────────╯",
                theme.fg_dim,
            )));
            continue;
        }

        // System prompt: show truncated preview when NOT editing
        if *field_name == "system_prompt" && !is_editing {
            let preview = if value.is_empty() {
                "(not set)".to_string()
            } else {
                // Show first 2 lines, truncated
                let first_lines: Vec<&str> = value.lines().take(2).collect();
                let preview = first_lines.join(" ");
                if preview.chars().count() > 45 {
                    let truncated: String = preview.chars().take(42).collect();
                    format!("{}...", truncated)
                } else if value.lines().count() > 2 {
                    format!("{} ...", preview)
                } else {
                    preview
                }
            };
            let hint_text = if is_selected { " [Enter: edit]" } else { "" };
            lines.push(Line::from(vec![
                Span::styled(marker, marker_style),
                Span::styled(*label, theme.fg_dim),
                Span::styled(
                    preview,
                    if is_selected {
                        theme.fg_active
                    } else {
                        theme.fg_dim
                    },
                ),
                Span::styled(hint_text.to_string(), theme.fg_dim),
            ]));
            continue;
        }

        let display_value: String = if is_editing {
            format!("{}\u{2588}", settings.edit_buffer())
        } else if value.is_empty() {
            "(not set)".to_string()
        } else {
            let v = value.as_str();
            if v.chars().count() > 40 {
                let truncated: String = v.chars().take(37).collect();
                format!("{}...", truncated)
            } else {
                v.to_string()
            }
        };

        let value_style = if is_editing {
            theme.fg_active
        } else if is_selected {
            theme.accent_primary
        } else {
            theme.fg_active
        };

        let mut spans = vec![
            Span::styled(marker, marker_style),
            Span::styled(format!("{:<16} ", label), theme.fg_dim),
            Span::styled(display_value, value_style),
        ];
        if is_selected && !is_editing {
            spans.push(Span::styled(*hint, theme.fg_dim));
        }
        lines.push(Line::from(spans));
    }

    // Field 2: backend (read-only)
    {
        let is_selected = settings.field_cursor() == 2;
        let marker = if is_selected { "> " } else { "  " };
        let marker_style = if is_selected {
            theme.accent_primary
        } else {
            theme.fg_dim
        };
        let value_style = if is_selected {
            theme.accent_primary
        } else {
            theme.fg_dim
        };
        lines.push(Line::from(vec![
            Span::styled(marker, marker_style),
            Span::styled("Backend           ", theme.fg_dim),
            Span::styled("daemon", value_style),
        ]));
    }

    lines
}

fn mask_api_key(key: &str) -> String {
    if key.is_empty() {
        return "(not set)".to_string();
    }
    let chars: Vec<char> = key.chars().collect();
    let len = chars.len();
    if len <= 7 {
        return "\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}".to_string();
    }
    let prefix: String = chars[..3].iter().collect();
    let suffix: String = chars[len - 4..].iter().collect();
    format!(
        "{}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}{}",
        prefix, suffix
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::config::ConfigState;
    use crate::state::settings::SettingsState;

    #[test]
    fn settings_handles_empty_state() {
        let settings = SettingsState::new();
        let config = ConfigState::new();
        let _theme = ThemeTokens::default();
        assert_eq!(settings.active_tab(), SettingsTab::Provider);
        assert_eq!(config.model(), "gpt-5.4");
    }

    #[test]
    fn settings_api_key_is_masked() {
        let masked = mask_api_key("sk-abcdefgh12345678abcd");
        assert!(!masked.contains("abcdefgh"));
        assert!(masked.contains("\u{2022}"));
    }

    #[test]
    fn mask_api_key_short_returns_dots() {
        assert_eq!(
            mask_api_key("short"),
            "\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}"
        );
    }

    #[test]
    fn mask_api_key_empty_returns_not_set() {
        assert_eq!(mask_api_key(""), "(not set)");
    }

    #[test]
    fn tab_hit_test_uses_rendered_label_positions() {
        let area = Rect::new(10, 3, 80, 1);
        let visible = visible_tabs(area, active_tab_index(SettingsTab::Concierge));
        assert!(visible.iter().any(|tab| tab.tab == SettingsTab::Concierge));
        for tab in visible {
            assert_eq!(
                tab_hit_test(area, SettingsTab::Concierge, tab.start_x),
                Some(tab.tab)
            );
        }
    }
}
