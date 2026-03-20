#![allow(dead_code)]

// ── SettingsTab ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsTab {
    Provider,
    Tools,
    WebSearch,
    Chat,
    Gateway,
    Agent,
    Advanced,
}

impl SettingsTab {
    const ALL: &'static [SettingsTab] = &[
        SettingsTab::Provider,
        SettingsTab::Tools,
        SettingsTab::WebSearch,
        SettingsTab::Chat,
        SettingsTab::Gateway,
        SettingsTab::Agent,
        SettingsTab::Advanced,
    ];

    pub fn all() -> &'static [SettingsTab] {
        Self::ALL
    }
}

// ── SettingsAction ────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum SettingsAction {
    Open,
    Close,
    SwitchTab(SettingsTab),
    NavigateField(i32),
    EditField,
    ConfirmEdit,
    CancelEdit,
    InsertChar(char),
    Backspace,
    MoveCursorLeft,
    MoveCursorRight,
    MoveCursorUp,
    MoveCursorDown,
    MoveCursorHome,
    MoveCursorEnd,
    SetCursor(usize),
    SetCursorLineCol(usize, usize),
    ToggleCheckbox,
    SelectRadio,
    OpenDropdown,
    NavigateDropdown(i32),
    SelectDropdown,
    Save,
}

// ── SettingsState ─────────────────────────────────────────────────────────────

pub struct SettingsState {
    active_tab: SettingsTab,
    field_cursor: usize,
    editing_field: Option<String>,
    edit_buffer: String,
    edit_cursor: usize,
    textarea_mode: bool, // true for multi-line edit (system_prompt)
    dropdown_open: bool,
    dropdown_cursor: usize,
    dirty: bool,
}

impl SettingsState {
    pub fn new() -> Self {
        Self {
            active_tab: SettingsTab::Provider,
            field_cursor: 0,
            editing_field: None,
            edit_buffer: String::new(),
            edit_cursor: 0,
            textarea_mode: false,
            dropdown_open: false,
            dropdown_cursor: 0,
            dirty: false,
        }
    }

    pub fn active_tab(&self) -> SettingsTab {
        self.active_tab
    }

    pub fn field_cursor(&self) -> usize {
        self.field_cursor
    }

    pub fn editing_field(&self) -> Option<&str> {
        self.editing_field.as_deref()
    }

    pub fn is_editing(&self) -> bool {
        self.editing_field.is_some()
    }

    pub fn edit_buffer(&self) -> &str {
        &self.edit_buffer
    }

    pub fn edit_cursor(&self) -> usize {
        self.edit_cursor
    }

    pub fn edit_cursor_line_col(&self) -> (usize, usize) {
        let before = &self.edit_buffer[..self.edit_cursor.min(self.edit_buffer.len())];
        let line = before.matches('\n').count();
        let last_newline = before.rfind('\n').map(|i| i + 1).unwrap_or(0);
        (line, before[last_newline..].chars().count())
    }

    pub fn is_dropdown_open(&self) -> bool {
        self.dropdown_open
    }

    pub fn dropdown_cursor(&self) -> usize {
        self.dropdown_cursor
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    pub fn is_textarea(&self) -> bool {
        self.textarea_mode
    }

    /// Start inline editing for a field, pre-populated with its current value.
    pub fn start_editing(&mut self, field: &str, current_value: &str) {
        self.editing_field = Some(field.to_string());
        self.edit_buffer = current_value.to_string();
        self.edit_cursor = self.edit_buffer.len();
        self.textarea_mode = field == "system_prompt";
    }

    fn line_col_to_offset(&self, target_line: usize, target_col: usize) -> usize {
        let mut offset = 0usize;
        for (line_idx, line) in self.edit_buffer.split('\n').enumerate() {
            if line_idx == target_line {
                let mut col = 0usize;
                for (idx, ch) in line.char_indices() {
                    if col == target_col {
                        return offset + idx;
                    }
                    col += 1;
                    if col > target_col {
                        return offset + idx;
                    }
                    let _ = ch;
                }
                return offset + line.len();
            }
            offset += line.len() + 1;
        }
        self.edit_buffer.len()
    }

    fn move_cursor_left(&mut self) {
        if self.edit_cursor > 0 {
            self.edit_cursor = self.edit_buffer[..self.edit_cursor]
                .char_indices()
                .last()
                .map(|(i, _)| i)
                .unwrap_or(0);
        }
    }

    fn move_cursor_right(&mut self) {
        if self.edit_cursor < self.edit_buffer.len() {
            self.edit_cursor = self.edit_buffer[self.edit_cursor..]
                .char_indices()
                .nth(1)
                .map(|(i, _)| self.edit_cursor + i)
                .unwrap_or(self.edit_buffer.len());
        }
    }

    fn move_cursor_up(&mut self) {
        let (line, col) = self.edit_cursor_line_col();
        if line > 0 {
            self.edit_cursor = self.line_col_to_offset(line - 1, col);
        }
    }

    fn move_cursor_down(&mut self) {
        let (line, col) = self.edit_cursor_line_col();
        let line_count = self.edit_buffer.matches('\n').count() + 1;
        if line + 1 < line_count {
            self.edit_cursor = self.line_col_to_offset(line + 1, col);
        }
    }

    fn move_cursor_home(&mut self) {
        let before = &self.edit_buffer[..self.edit_cursor];
        self.edit_cursor = before.rfind('\n').map(|i| i + 1).unwrap_or(0);
    }

    fn move_cursor_end(&mut self) {
        let after = &self.edit_buffer[self.edit_cursor..];
        if let Some(nl) = after.find('\n') {
            self.edit_cursor += nl;
        } else {
            self.edit_cursor = self.edit_buffer.len();
        }
    }

    /// Map `field_cursor` to the field name for the active tab.
    pub fn current_field_name(&self) -> &str {
        match self.active_tab {
            SettingsTab::Provider => match self.field_cursor {
                0 => "provider",
                1 => "base_url",
                2 => "api_key",
                3 => "auth_source",
                4 => "model",
                5 => "api_transport",
                6 => "assistant_id",
                7 => "reasoning_effort",
                8 => "context_window_tokens",
                _ => "",
            },
            SettingsTab::Tools => match self.field_cursor {
                0 => "tool_bash",
                1 => "tool_file_ops",
                2 => "tool_web_search",
                3 => "tool_web_browse",
                4 => "tool_vision",
                5 => "tool_system_info",
                6 => "tool_gateway",
                _ => "",
            },
            SettingsTab::WebSearch => match self.field_cursor {
                0 => "web_search_enabled",
                1 => "search_provider",
                2 => "firecrawl_api_key",
                3 => "exa_api_key",
                4 => "tavily_api_key",
                5 => "search_max_results",
                6 => "search_timeout",
                _ => "",
            },
            SettingsTab::Chat => match self.field_cursor {
                0 => "enable_streaming",
                1 => "enable_conversation_memory",
                2 => "enable_honcho_memory",
                3 => "honcho_api_key",
                4 => "honcho_base_url",
                5 => "honcho_workspace_id",
                _ => "",
            },
            SettingsTab::Gateway => match self.field_cursor {
                0 => "gateway_enabled",
                1 => "gateway_prefix",
                2 => "slack_token",
                3 => "slack_channel_filter",
                4 => "telegram_token",
                5 => "telegram_allowed_chats",
                6 => "discord_token",
                7 => "discord_channel_filter",
                8 => "discord_allowed_users",
                9 => "whatsapp_allowed_contacts",
                10 => "whatsapp_token",
                11 => "whatsapp_phone_id",
                _ => "",
            },
            SettingsTab::Agent => match self.field_cursor {
                0 => "agent_name",
                1 => "system_prompt",
                2 => "backend",
                _ => "",
            },
            SettingsTab::Advanced => match self.field_cursor {
                0 => "auto_compact_context",
                1 => "max_context_messages",
                2 => "max_tool_loops",
                3 => "max_retries",
                4 => "retry_delay_ms",
                5 => "context_window_tokens",
                6 => "context_budget_tokens",
                7 => "compact_threshold_pct",
                8 => "keep_recent_on_compact",
                9 => "bash_timeout_secs",
                10 => "snapshot_auto_cleanup",
                11 => "snapshot_max_count",
                12 => "snapshot_max_size_mb",
                13 => "snapshot_stats",
                _ => "",
            },
        }
    }

    /// Number of navigable fields in the current tab (for cursor clamping).
    pub fn field_count(&self) -> usize {
        match self.active_tab {
            SettingsTab::Provider => 9,
            SettingsTab::Tools => 7,
            SettingsTab::WebSearch => 7,
            SettingsTab::Chat => 6,
            SettingsTab::Gateway => 12,
            SettingsTab::Agent => 3,
            SettingsTab::Advanced => 14,
        }
    }

    /// Navigate fields within `field_count` items; clamps at both ends.
    pub fn navigate_field(&mut self, delta: i32, field_count: usize) {
        if field_count == 0 {
            self.field_cursor = 0;
            return;
        }
        if delta > 0 {
            self.field_cursor =
                (self.field_cursor + delta as usize).min(field_count.saturating_sub(1));
        } else {
            self.field_cursor = self.field_cursor.saturating_sub((-delta) as usize);
        }
    }

    pub fn reduce(&mut self, action: SettingsAction) {
        match action {
            SettingsAction::Open => {
                self.active_tab = SettingsTab::Provider;
                self.field_cursor = 0;
                self.editing_field = None;
                self.edit_buffer.clear();
                self.edit_cursor = 0;
                self.dropdown_open = false;
                self.dropdown_cursor = 0;
                self.dirty = false;
            }

            SettingsAction::Close => {
                self.editing_field = None;
                self.edit_buffer.clear();
                self.edit_cursor = 0;
                self.dropdown_open = false;
            }

            SettingsAction::SwitchTab(tab) => {
                self.active_tab = tab;
                self.field_cursor = 0;
                self.editing_field = None;
                self.edit_buffer.clear();
                self.edit_cursor = 0;
                self.dropdown_open = false;
                self.dropdown_cursor = 0;
            }

            SettingsAction::NavigateField(delta) => {
                let count = self.field_count();
                if delta > 0 {
                    self.field_cursor =
                        (self.field_cursor + delta as usize).min(count.saturating_sub(1));
                } else {
                    self.field_cursor = self.field_cursor.saturating_sub((-delta) as usize);
                }
            }

            SettingsAction::EditField => {
                let field_name = self.current_field_name().to_string();
                self.editing_field = Some(field_name);
                self.dirty = true;
            }

            SettingsAction::InsertChar(c) => {
                if self.editing_field.is_some() {
                    self.edit_buffer.insert(self.edit_cursor, c);
                    self.edit_cursor += c.len_utf8();
                }
            }

            SettingsAction::Backspace => {
                if self.editing_field.is_some() && self.edit_cursor > 0 {
                    let prev = self.edit_buffer[..self.edit_cursor]
                        .char_indices()
                        .last()
                        .map(|(i, _)| i)
                        .unwrap_or(0);
                    self.edit_buffer.drain(prev..self.edit_cursor);
                    self.edit_cursor = prev;
                }
            }

            SettingsAction::MoveCursorLeft => self.move_cursor_left(),

            SettingsAction::MoveCursorRight => self.move_cursor_right(),

            SettingsAction::MoveCursorUp => self.move_cursor_up(),

            SettingsAction::MoveCursorDown => self.move_cursor_down(),

            SettingsAction::MoveCursorHome => self.move_cursor_home(),

            SettingsAction::MoveCursorEnd => self.move_cursor_end(),

            SettingsAction::SetCursor(pos) => {
                self.edit_cursor = pos.min(self.edit_buffer.len());
            }

            SettingsAction::SetCursorLineCol(line, col) => {
                self.edit_cursor = self.line_col_to_offset(line, col);
            }

            SettingsAction::ConfirmEdit => {
                self.editing_field = None;
                self.textarea_mode = false;
            }

            SettingsAction::CancelEdit => {
                self.editing_field = None;
                self.textarea_mode = false;
                self.edit_buffer.clear();
                self.edit_cursor = 0;
            }

            SettingsAction::ToggleCheckbox => {
                self.dirty = true;
            }

            SettingsAction::SelectRadio => {
                self.dirty = true;
            }

            SettingsAction::OpenDropdown => {
                self.dropdown_open = true;
                self.dropdown_cursor = 0;
            }

            SettingsAction::NavigateDropdown(delta) => {
                if self.dropdown_open {
                    if delta > 0 {
                        self.dropdown_cursor = self.dropdown_cursor.saturating_add(delta as usize);
                    } else {
                        self.dropdown_cursor =
                            self.dropdown_cursor.saturating_sub((-delta) as usize);
                    }
                }
            }

            SettingsAction::SelectDropdown => {
                self.dropdown_open = false;
                self.dirty = true;
            }

            SettingsAction::Save => {
                self.dirty = false;
                self.editing_field = None;
                self.edit_buffer.clear();
                self.edit_cursor = 0;
            }
        }
    }
}

impl Default for SettingsState {
    fn default() -> Self {
        Self::new()
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_resets_to_provider_tab() {
        let mut state = SettingsState::new();
        state.reduce(SettingsAction::SwitchTab(SettingsTab::Agent));
        state.reduce(SettingsAction::NavigateField(3));
        state.reduce(SettingsAction::EditField);
        assert_eq!(state.active_tab(), SettingsTab::Agent);
        assert!(state.is_dirty());

        state.reduce(SettingsAction::Open);
        assert_eq!(state.active_tab(), SettingsTab::Provider);
        assert_eq!(state.field_cursor(), 0);
        assert!(state.editing_field().is_none());
        assert!(!state.is_dirty());
    }

    #[test]
    fn switch_tab_resets_cursor_and_editing() {
        let mut state = SettingsState::new();
        state.reduce(SettingsAction::NavigateField(4));
        state.reduce(SettingsAction::EditField);
        assert!(state.editing_field().is_some());

        state.reduce(SettingsAction::SwitchTab(SettingsTab::Tools));
        assert_eq!(state.active_tab(), SettingsTab::Tools);
        assert_eq!(state.field_cursor(), 0);
        assert!(state.editing_field().is_none());
    }

    #[test]
    fn navigate_field_increases_cursor() {
        let mut state = SettingsState::new();
        state.reduce(SettingsAction::NavigateField(2));
        assert_eq!(state.field_cursor(), 2);
        state.reduce(SettingsAction::NavigateField(1));
        assert_eq!(state.field_cursor(), 3);
    }

    #[test]
    fn navigate_field_clamps_at_zero() {
        let mut state = SettingsState::new();
        state.reduce(SettingsAction::NavigateField(-10));
        assert_eq!(state.field_cursor(), 0);
    }

    #[test]
    fn navigate_field_clamps_at_max() {
        let mut state = SettingsState::new();
        // Provider tab has 9 fields (0..8)
        state.reduce(SettingsAction::NavigateField(100));
        assert_eq!(state.field_cursor(), 8);
    }

    #[test]
    fn navigate_field_method_clamps_at_max() {
        let mut state = SettingsState::new();
        state.navigate_field(100, 5);
        assert_eq!(state.field_cursor(), 4);
    }

    #[test]
    fn edit_field_sets_dirty() {
        let mut state = SettingsState::new();
        assert!(!state.is_dirty());
        state.reduce(SettingsAction::EditField);
        assert!(state.is_dirty());
        assert!(state.editing_field().is_some());
    }

    #[test]
    fn confirm_edit_clears_editing_field() {
        let mut state = SettingsState::new();
        state.reduce(SettingsAction::EditField);
        assert!(state.editing_field().is_some());
        state.reduce(SettingsAction::ConfirmEdit);
        assert!(state.editing_field().is_none());
        // dirty remains true until saved
        assert!(state.is_dirty());
    }

    #[test]
    fn cancel_edit_clears_editing_field() {
        let mut state = SettingsState::new();
        state.reduce(SettingsAction::EditField);
        state.reduce(SettingsAction::CancelEdit);
        assert!(state.editing_field().is_none());
    }

    #[test]
    fn save_clears_dirty_flag() {
        let mut state = SettingsState::new();
        state.reduce(SettingsAction::EditField);
        assert!(state.is_dirty());
        state.reduce(SettingsAction::Save);
        assert!(!state.is_dirty());
    }

    #[test]
    fn dropdown_open_and_navigate() {
        let mut state = SettingsState::new();
        assert!(!state.is_dropdown_open());
        state.reduce(SettingsAction::OpenDropdown);
        assert!(state.is_dropdown_open());
        assert_eq!(state.dropdown_cursor(), 0);

        state.reduce(SettingsAction::NavigateDropdown(2));
        assert_eq!(state.dropdown_cursor(), 2);
        state.reduce(SettingsAction::NavigateDropdown(-1));
        assert_eq!(state.dropdown_cursor(), 1);
    }

    #[test]
    fn select_dropdown_closes_and_sets_dirty() {
        let mut state = SettingsState::new();
        state.reduce(SettingsAction::OpenDropdown);
        state.reduce(SettingsAction::SelectDropdown);
        assert!(!state.is_dropdown_open());
        assert!(state.is_dirty());
    }

    #[test]
    fn close_clears_editing_and_dropdown() {
        let mut state = SettingsState::new();
        state.reduce(SettingsAction::EditField);
        state.reduce(SettingsAction::OpenDropdown);
        state.reduce(SettingsAction::Close);
        assert!(state.editing_field().is_none());
        assert!(!state.is_dropdown_open());
    }

    #[test]
    fn all_tabs_covers_seven_variants() {
        assert_eq!(SettingsTab::all().len(), 7);
    }

    #[test]
    fn tab_cycling_through_all() {
        let mut state = SettingsState::new();
        for &tab in SettingsTab::all() {
            state.reduce(SettingsAction::SwitchTab(tab));
            assert_eq!(state.active_tab(), tab);
        }
    }

    #[test]
    fn insert_char_appends_to_edit_buffer() {
        let mut state = SettingsState::new();
        state.start_editing("base_url", "https://");
        assert!(state.is_editing());
        assert_eq!(state.edit_buffer(), "https://");

        state.reduce(SettingsAction::InsertChar('a'));
        state.reduce(SettingsAction::InsertChar('p'));
        state.reduce(SettingsAction::InsertChar('i'));
        assert_eq!(state.edit_buffer(), "https://api");
    }

    #[test]
    fn backspace_removes_last_char() {
        let mut state = SettingsState::new();
        state.start_editing("api_key", "sk-abc");
        state.reduce(SettingsAction::Backspace);
        assert_eq!(state.edit_buffer(), "sk-ab");
        state.reduce(SettingsAction::Backspace);
        assert_eq!(state.edit_buffer(), "sk-a");
    }

    #[test]
    fn backspace_on_empty_buffer_is_noop() {
        let mut state = SettingsState::new();
        state.start_editing("api_key", "");
        state.reduce(SettingsAction::Backspace);
        assert_eq!(state.edit_buffer(), "");
    }

    #[test]
    fn cancel_edit_clears_buffer() {
        let mut state = SettingsState::new();
        state.start_editing("base_url", "https://example.com");
        state.reduce(SettingsAction::InsertChar('!'));
        state.reduce(SettingsAction::CancelEdit);
        assert!(!state.is_editing());
        assert_eq!(state.edit_buffer(), "");
    }

    #[test]
    fn confirm_edit_keeps_buffer_value() {
        let mut state = SettingsState::new();
        state.start_editing("base_url", "https://");
        state.reduce(SettingsAction::InsertChar('x'));
        state.reduce(SettingsAction::ConfirmEdit);
        assert!(!state.is_editing());
        // Buffer still has value so caller can read it
        assert_eq!(state.edit_buffer(), "https://x");
    }

    #[test]
    fn current_field_name_provider_tab() {
        let mut state = SettingsState::new();
        assert_eq!(state.current_field_name(), "provider");
        state.reduce(SettingsAction::NavigateField(1));
        assert_eq!(state.current_field_name(), "base_url");
        state.reduce(SettingsAction::NavigateField(1));
        assert_eq!(state.current_field_name(), "api_key");
        state.reduce(SettingsAction::NavigateField(1));
        assert_eq!(state.current_field_name(), "auth_source");
        state.reduce(SettingsAction::NavigateField(1));
        assert_eq!(state.current_field_name(), "model");
        state.reduce(SettingsAction::NavigateField(1));
        assert_eq!(state.current_field_name(), "api_transport");
        state.reduce(SettingsAction::NavigateField(1));
        assert_eq!(state.current_field_name(), "assistant_id");
        state.reduce(SettingsAction::NavigateField(1));
        assert_eq!(state.current_field_name(), "reasoning_effort");
        state.reduce(SettingsAction::NavigateField(1));
        assert_eq!(state.current_field_name(), "context_window_tokens");
    }

    #[test]
    fn current_field_name_tools_tab() {
        let mut state = SettingsState::new();
        state.reduce(SettingsAction::SwitchTab(SettingsTab::Tools));
        assert_eq!(state.current_field_name(), "tool_bash");
        state.reduce(SettingsAction::NavigateField(1));
        assert_eq!(state.current_field_name(), "tool_file_ops");
        state.reduce(SettingsAction::NavigateField(1));
        assert_eq!(state.current_field_name(), "tool_web_search");
        state.reduce(SettingsAction::NavigateField(1));
        assert_eq!(state.current_field_name(), "tool_web_browse");
        state.reduce(SettingsAction::NavigateField(1));
        assert_eq!(state.current_field_name(), "tool_vision");
        state.reduce(SettingsAction::NavigateField(1));
        assert_eq!(state.current_field_name(), "tool_system_info");
        state.reduce(SettingsAction::NavigateField(1));
        assert_eq!(state.current_field_name(), "tool_gateway");
    }

    #[test]
    fn current_field_name_gateway_tab() {
        let mut state = SettingsState::new();
        state.reduce(SettingsAction::SwitchTab(SettingsTab::Gateway));
        assert_eq!(state.current_field_name(), "gateway_enabled");
        state.reduce(SettingsAction::NavigateField(1));
        assert_eq!(state.current_field_name(), "gateway_prefix");
        state.reduce(SettingsAction::NavigateField(1));
        assert_eq!(state.current_field_name(), "slack_token");
        state.reduce(SettingsAction::NavigateField(1));
        assert_eq!(state.current_field_name(), "slack_channel_filter");
        state.reduce(SettingsAction::NavigateField(1));
        assert_eq!(state.current_field_name(), "telegram_token");
        state.reduce(SettingsAction::NavigateField(1));
        assert_eq!(state.current_field_name(), "telegram_allowed_chats");
        state.reduce(SettingsAction::NavigateField(1));
        assert_eq!(state.current_field_name(), "discord_token");
        state.reduce(SettingsAction::NavigateField(1));
        assert_eq!(state.current_field_name(), "discord_channel_filter");
        state.reduce(SettingsAction::NavigateField(1));
        assert_eq!(state.current_field_name(), "discord_allowed_users");
        state.reduce(SettingsAction::NavigateField(1));
        assert_eq!(state.current_field_name(), "whatsapp_allowed_contacts");
        state.reduce(SettingsAction::NavigateField(1));
        assert_eq!(state.current_field_name(), "whatsapp_token");
        state.reduce(SettingsAction::NavigateField(1));
        assert_eq!(state.current_field_name(), "whatsapp_phone_id");
    }

    #[test]
    fn current_field_name_agent_tab() {
        let mut state = SettingsState::new();
        state.reduce(SettingsAction::SwitchTab(SettingsTab::Agent));
        assert_eq!(state.current_field_name(), "agent_name");
        state.reduce(SettingsAction::NavigateField(1));
        assert_eq!(state.current_field_name(), "system_prompt");
        state.reduce(SettingsAction::NavigateField(1));
        assert_eq!(state.current_field_name(), "backend");
    }

    #[test]
    fn current_field_name_chat_tab() {
        let mut state = SettingsState::new();
        state.reduce(SettingsAction::SwitchTab(SettingsTab::Chat));
        assert_eq!(state.current_field_name(), "enable_streaming");
        state.reduce(SettingsAction::NavigateField(1));
        assert_eq!(state.current_field_name(), "enable_conversation_memory");
        state.reduce(SettingsAction::NavigateField(1));
        assert_eq!(state.current_field_name(), "enable_honcho_memory");
        state.reduce(SettingsAction::NavigateField(1));
        assert_eq!(state.current_field_name(), "honcho_api_key");
        state.reduce(SettingsAction::NavigateField(1));
        assert_eq!(state.current_field_name(), "honcho_base_url");
        state.reduce(SettingsAction::NavigateField(1));
        assert_eq!(state.current_field_name(), "honcho_workspace_id");
        // Only 6 fields, can't navigate past it
        state.reduce(SettingsAction::NavigateField(5));
        assert_eq!(state.current_field_name(), "honcho_workspace_id");
        assert_eq!(state.field_cursor(), 5);
    }

    #[test]
    fn current_field_name_advanced_tab() {
        let mut state = SettingsState::new();
        state.reduce(SettingsAction::SwitchTab(SettingsTab::Advanced));
        assert_eq!(state.current_field_name(), "auto_compact_context");
        state.reduce(SettingsAction::NavigateField(1));
        assert_eq!(state.current_field_name(), "max_context_messages");
        state.reduce(SettingsAction::NavigateField(1));
        assert_eq!(state.current_field_name(), "max_tool_loops");
        state.reduce(SettingsAction::NavigateField(1));
        assert_eq!(state.current_field_name(), "max_retries");
        state.reduce(SettingsAction::NavigateField(1));
        assert_eq!(state.current_field_name(), "retry_delay_ms");
        state.reduce(SettingsAction::NavigateField(1));
        assert_eq!(state.current_field_name(), "context_window_tokens");
        state.reduce(SettingsAction::NavigateField(1));
        assert_eq!(state.current_field_name(), "context_budget_tokens");
        state.reduce(SettingsAction::NavigateField(1));
        assert_eq!(state.current_field_name(), "compact_threshold_pct");
        state.reduce(SettingsAction::NavigateField(1));
        assert_eq!(state.current_field_name(), "keep_recent_on_compact");
        state.reduce(SettingsAction::NavigateField(1));
        assert_eq!(state.current_field_name(), "bash_timeout_secs");
        state.reduce(SettingsAction::NavigateField(1));
        assert_eq!(state.current_field_name(), "snapshot_auto_cleanup");
        state.reduce(SettingsAction::NavigateField(1));
        assert_eq!(state.current_field_name(), "snapshot_max_count");
        state.reduce(SettingsAction::NavigateField(1));
        assert_eq!(state.current_field_name(), "snapshot_max_size_mb");
        state.reduce(SettingsAction::NavigateField(1));
        assert_eq!(state.current_field_name(), "snapshot_stats");
        // 14 fields total, can't navigate past it
        state.reduce(SettingsAction::NavigateField(5));
        assert_eq!(state.current_field_name(), "snapshot_stats");
        assert_eq!(state.field_cursor(), 13);
    }

    #[test]
    fn field_count_per_tab() {
        let mut state = SettingsState::new();
        assert_eq!(state.field_count(), 9); // Provider + auth + transport + assistant id + context window
        state.reduce(SettingsAction::SwitchTab(SettingsTab::Tools));
        assert_eq!(state.field_count(), 7); // 7 tool checkboxes
        state.reduce(SettingsAction::SwitchTab(SettingsTab::WebSearch));
        assert_eq!(state.field_count(), 7); // enabled, provider, 3 keys, max_results, timeout
        state.reduce(SettingsAction::SwitchTab(SettingsTab::Chat));
        assert_eq!(state.field_count(), 6); // streaming, conv memory, honcho toggle, key, url, workspace
        state.reduce(SettingsAction::SwitchTab(SettingsTab::Gateway));
        assert_eq!(state.field_count(), 12); // enabled, prefix, slack×2, telegram×2, discord×3, whatsapp×3
        state.reduce(SettingsAction::SwitchTab(SettingsTab::Agent));
        assert_eq!(state.field_count(), 3); // name, prompt, backend
        state.reduce(SettingsAction::SwitchTab(SettingsTab::Advanced));
        assert_eq!(state.field_count(), 14); // 10 advanced + 4 snapshot fields
    }

    #[test]
    fn current_field_name_websearch_tab() {
        let mut state = SettingsState::new();
        state.reduce(SettingsAction::SwitchTab(SettingsTab::WebSearch));
        assert_eq!(state.current_field_name(), "web_search_enabled");
        state.reduce(SettingsAction::NavigateField(1));
        assert_eq!(state.current_field_name(), "search_provider");
        state.reduce(SettingsAction::NavigateField(1));
        assert_eq!(state.current_field_name(), "firecrawl_api_key");
        state.reduce(SettingsAction::NavigateField(1));
        assert_eq!(state.current_field_name(), "exa_api_key");
        state.reduce(SettingsAction::NavigateField(1));
        assert_eq!(state.current_field_name(), "tavily_api_key");
        state.reduce(SettingsAction::NavigateField(1));
        assert_eq!(state.current_field_name(), "search_max_results");
        state.reduce(SettingsAction::NavigateField(1));
        assert_eq!(state.current_field_name(), "search_timeout");
    }

    #[test]
    fn insert_char_ignored_when_not_editing() {
        let mut state = SettingsState::new();
        state.reduce(SettingsAction::InsertChar('x'));
        assert_eq!(state.edit_buffer(), "");
    }
}
