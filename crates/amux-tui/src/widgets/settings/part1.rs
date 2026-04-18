#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsHitTarget {
    Tab(SettingsTab),
    Field(usize),
    AuthProviderItem(usize),
    AuthAction {
        index: usize,
        action: AuthTabAction,
    },
    SubAgentListItem(usize),
    SubAgentAction(SubAgentTabAction),
    SubAgentRowAction {
        index: usize,
        action: SubAgentTabAction,
    },
    EditCursor {
        line: usize,
        col: usize,
    },
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

const TAB_LABELS: [&str; 12] = [
    "Auth", "Svar", "Rar", "Tools", "Search", "Chat", "GW", "Sub", "Feat", "Adv", "Plug", "About",
];
const TAB_DIVIDER: &str = " | ";

#[derive(Debug, Clone, Copy)]
struct VisibleTab {
    tab: SettingsTab,
    index: usize,
    start_x: u16,
    end_x: u16,
}

fn render_edit_buffer_with_cursor(text: &str, cursor: usize) -> String {
    let cursor = cursor.min(text.chars().count());
    let mut out = String::with_capacity(text.len() + 3);
    let byte_cursor = text
        .char_indices()
        .nth(cursor)
        .map(|(idx, _)| idx)
        .unwrap_or(text.len());
    out.push_str(&text[..byte_cursor]);
    out.push('\u{2588}');
    out.push_str(&text[byte_cursor..]);
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
    let tail: String = chars[chars.len().saturating_sub(max_chars)..]
        .iter()
        .collect();
    format!("…{}", tail)
}

pub fn render(
    frame: &mut Frame,
    area: Rect,
    settings: &SettingsState,
    config: &ConfigState,
    modal: &ModalState,
    auth: &crate::state::auth::AuthState,
    subagents: &SubAgentsState,
    concierge: &ConciergeState,
    tier: &crate::state::tier::TierState,
    plugin_settings: &PluginSettingsState,
    scroll: usize,
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
    let tab_index = active_tab_index(active);
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
        modal,
        auth,
        subagents,
        concierge,
        tier,
        plugin_settings,
        theme,
    );
    let paragraph = Paragraph::new(content_lines).scroll((scroll.min(u16::MAX as usize) as u16, 0));
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
    config: &ConfigState,
    auth: &crate::state::auth::AuthState,
    subagents: &SubAgentsState,
    scroll: usize,
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

    if let Some((line, col)) = editing_cursor_hit_test(chunks[2], settings, config, scroll, mouse) {
        return Some(SettingsHitTarget::EditCursor { line, col });
    }

    if matches!(settings.active_tab(), SettingsTab::Auth) {
        return auth_hit_test(chunks[2], auth, scroll, mouse);
    }

    if matches!(settings.active_tab(), SettingsTab::SubAgents) {
        return subagents_hit_test(chunks[2], subagents, scroll, mouse);
    }

    let row = mouse.y.saturating_sub(chunks[2].y) as usize + scroll;
    match settings_row_hit(settings, config, subagents, row) {
        Some((_, Some(subagent_index))) => {
            Some(SettingsHitTarget::SubAgentListItem(subagent_index))
        }
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
        SettingsTab::Auth => 0,
        SettingsTab::Provider | SettingsTab::Agent => 1,
        SettingsTab::Concierge => 2,
        SettingsTab::Tools => 3,
        SettingsTab::WebSearch => 4,
        SettingsTab::Chat => 5,
        SettingsTab::Gateway => 6,
        SettingsTab::SubAgents => 7,
        SettingsTab::Features => 8,
        SettingsTab::Advanced => 9,
        SettingsTab::Plugins => 10,
        SettingsTab::About => 11,
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
    let mut x = tab_area.x.saturating_add(prefix.chars().count() as u16);

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
    if tabs
        .last()
        .is_some_and(|tab| tab.index + 1 < TAB_LABELS.len())
    {
        spans.push(Span::styled(" »", theme.fg_dim));
    }
    Line::from(spans)
}

fn editing_cursor_hit_test(
    content_area: Rect,
    settings: &SettingsState,
    config: &ConfigState,
    scroll: usize,
    mouse: Position,
) -> Option<(usize, usize)> {
    let field = settings.editing_field()?;
    let row = mouse.y.saturating_sub(content_area.y) as usize + scroll;
    let rel_x = mouse.x.saturating_sub(content_area.x) as usize;

    if settings.is_textarea() {
        let (text_start_row, text_start_col) = textarea_edit_layout(settings, field)?;
        let line_count = settings.edit_buffer().split('\n').count().max(1);
        let row_end = text_start_row + line_count;
        if row < text_start_row || row > row_end {
            return None;
        }
        let line = (row - text_start_row).min(line_count.saturating_sub(1));
        let col = rel_x.saturating_sub(text_start_col);
        return Some((line, col));
    }

    let (field_row, start_col) = single_line_edit_layout(settings, config, field)?;
    if row == field_row {
        return Some((0, rel_x.saturating_sub(start_col)));
    }
    None
}

fn content_area(area: Rect) -> Option<Rect> {
    let block = Block::default()
        .title(" SETTINGS ")
        .borders(Borders::ALL)
        .border_type(BorderType::Double);
    let inner = block.inner(area);
    if inner.height < 5 {
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
    Some(chunks[2])
}

pub fn max_scroll(
    area: Rect,
    settings: &SettingsState,
    config: &ConfigState,
    modal: &ModalState,
    auth: &crate::state::auth::AuthState,
    subagents: &SubAgentsState,
    concierge: &ConciergeState,
    tier: &crate::state::tier::TierState,
    plugin_settings: &PluginSettingsState,
    theme: &ThemeTokens,
) -> usize {
    let Some(content_area) = content_area(area) else {
        return 0;
    };
    let line_count = render_tab_content(
        content_area.width,
        settings,
        config,
        modal,
        auth,
        subagents,
        concierge,
        tier,
        plugin_settings,
        theme,
    )
    .len();
    line_count.saturating_sub(content_area.height as usize)
}
