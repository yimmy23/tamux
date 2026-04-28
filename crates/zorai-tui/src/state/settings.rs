#![allow(dead_code)]

#[path = "settings_cursor.rs"]
mod cursor;

use crate::state::config::ConfigState;

// ── SettingsTab ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsTab {
    Auth,
    Provider,
    Tools,
    WebSearch,
    Chat,
    Gateway,
    Agent,
    SubAgents,
    Concierge,
    Features,
    Advanced,
    Plugins,
    About,
}

impl SettingsTab {
    const ALL: &'static [SettingsTab] = &[
        SettingsTab::Auth,
        SettingsTab::Agent,
        SettingsTab::Concierge,
        SettingsTab::Tools,
        SettingsTab::WebSearch,
        SettingsTab::Chat,
        SettingsTab::Gateway,
        SettingsTab::SubAgents,
        SettingsTab::Features,
        SettingsTab::Advanced,
        SettingsTab::Plugins,
        SettingsTab::About,
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

fn field_uses_textarea(field: &str) -> bool {
    matches!(
        field,
        "system_prompt" | "subagent_system_prompt" | "whatsapp_allowed_contacts"
    )
}

impl SettingsState {
    fn advanced_field_names_for_strategy(strategy: &str) -> &'static [&'static str] {
        const HEURISTIC_FIELDS: &[&str] = &[
            "managed_sandbox_enabled",
            "managed_security_level",
            "auto_compact_context",
            "compaction_strategy",
            "max_context_messages",
            "max_tool_loops",
            "max_retries",
            "retry_delay_ms",
            "message_loop_delay_ms",
            "tool_call_delay_ms",
            "llm_stream_chunk_timeout_secs",
            "auto_retry",
            "context_window_tokens",
            "compact_threshold_pct",
            "keep_recent_on_compact",
            "bash_timeout_secs",
            "weles_max_concurrent_reviews",
            "snapshot_auto_cleanup",
            "snapshot_max_count",
            "snapshot_max_size_mb",
            "snapshot_stats",
        ];
        const WELES_FIELDS: &[&str] = &[
            "managed_sandbox_enabled",
            "managed_security_level",
            "auto_compact_context",
            "compaction_strategy",
            "max_context_messages",
            "max_tool_loops",
            "max_retries",
            "retry_delay_ms",
            "message_loop_delay_ms",
            "tool_call_delay_ms",
            "llm_stream_chunk_timeout_secs",
            "auto_retry",
            "context_window_tokens",
            "compact_threshold_pct",
            "keep_recent_on_compact",
            "bash_timeout_secs",
            "weles_max_concurrent_reviews",
            "compaction_weles_provider",
            "compaction_weles_model",
            "compaction_weles_reasoning_effort",
            "snapshot_auto_cleanup",
            "snapshot_max_count",
            "snapshot_max_size_mb",
            "snapshot_stats",
        ];
        const CUSTOM_FIELDS: &[&str] = &[
            "managed_sandbox_enabled",
            "managed_security_level",
            "auto_compact_context",
            "compaction_strategy",
            "max_context_messages",
            "max_tool_loops",
            "max_retries",
            "retry_delay_ms",
            "message_loop_delay_ms",
            "tool_call_delay_ms",
            "llm_stream_chunk_timeout_secs",
            "auto_retry",
            "context_window_tokens",
            "compact_threshold_pct",
            "keep_recent_on_compact",
            "bash_timeout_secs",
            "weles_max_concurrent_reviews",
            "compaction_custom_provider",
            "compaction_custom_base_url",
            "compaction_custom_auth_source",
            "compaction_custom_model",
            "compaction_custom_api_transport",
            "compaction_custom_api_key",
            "compaction_custom_assistant_id",
            "compaction_custom_reasoning_effort",
            "compaction_custom_context_window_tokens",
            "snapshot_auto_cleanup",
            "snapshot_max_count",
            "snapshot_max_size_mb",
            "snapshot_stats",
        ];

        match strategy {
            "weles" => WELES_FIELDS,
            "custom_model" => CUSTOM_FIELDS,
            _ => HEURISTIC_FIELDS,
        }
    }

    pub fn new() -> Self {
        Self {
            active_tab: SettingsTab::Auth,
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
        self.textarea_mode = field_uses_textarea(field);
    }

    /// Map `field_cursor` to the field name for the active tab.
    pub fn current_field_name(&self) -> &str {
        match self.active_tab {
            SettingsTab::Provider => match self.field_cursor {
                0 => "provider",
                1 => "base_url",
                2 => "auth_source",
                3 => "model",
                4 => "api_transport",
                5 => "assistant_id",
                6 => "reasoning_effort",
                7 => "context_window_tokens",
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
                7 => "browse_provider",
                _ => "",
            },
            SettingsTab::Chat => match self.field_cursor {
                0 => "enable_streaming",
                1 => "enable_conversation_memory",
                2 => "enable_honcho_memory",
                3 => "anticipatory_enabled",
                4 => "anticipatory_morning_brief",
                5 => "anticipatory_predictive_hydration",
                6 => "anticipatory_stuck_detection",
                7 => "operator_model_enabled",
                8 => "operator_model_allow_message_statistics",
                9 => "operator_model_allow_approval_learning",
                10 => "operator_model_allow_attention_tracking",
                11 => "operator_model_allow_implicit_feedback",
                12 => "collaboration_enabled",
                13 => "compliance_mode",
                14 => "compliance_retention_days",
                15 => "compliance_sign_all_events",
                16 => "tool_synthesis_enabled",
                17 => "tool_synthesis_require_activation",
                18 => "tool_synthesis_max_generated_tools",
                19 => "tui_chat_history_page_size",
                20 => "operator_model_inspect",
                21 => "operator_model_reset",
                22 => "collaboration_sessions_inspect",
                23 => "generated_tools_inspect",
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
                12 => "whatsapp_link_device",
                13 => "whatsapp_relink_device",
                _ => "",
            },
            SettingsTab::Auth => match self.field_cursor {
                0 => "auth_provider_list",
                _ => "",
            },
            SettingsTab::Agent => match self.field_cursor {
                0 => "provider",
                1 => "base_url",
                2 => "auth_source",
                3 => "model",
                4 => "api_transport",
                5 => "assistant_id",
                6 => "reasoning_effort",
                7 => "context_window_tokens",
                8 => "system_prompt",
                9 => "backend",
                _ => "",
            },
            SettingsTab::SubAgents => match self.field_cursor {
                0 => "subagent_list",
                _ => "",
            },
            SettingsTab::Concierge => match self.field_cursor {
                0 => "concierge_enabled",
                1 => "concierge_detail_level",
                2 => "concierge_provider",
                3 => "concierge_model",
                4 => "concierge_reasoning_effort",
                _ => "",
            },
            SettingsTab::Features => match self.field_cursor {
                0 => "feat_tier_override",
                1 => "feat_security_level",
                2 => "feat_heartbeat_cron",
                3 => "feat_heartbeat_quiet_start",
                4 => "feat_heartbeat_quiet_end",
                5 => "feat_check_stale_todos",
                6 => "feat_check_stuck_goals",
                7 => "feat_check_unreplied_messages",
                8 => "feat_check_repo_changes",
                9 => "feat_consolidation_enabled",
                10 => "feat_decay_half_life_hours",
                11 => "feat_heuristic_promotion_threshold",
                12 => "feat_skill_recommendation_enabled",
                13 => "feat_skill_background_community_search",
                14 => "feat_skill_community_preapprove_timeout_secs",
                15 => "feat_skill_suggest_global_enable_after_approvals",
                16 => "feat_audio_stt_enabled",
                17 => "feat_audio_stt_provider",
                18 => "feat_audio_stt_model",
                19 => "feat_audio_tts_enabled",
                20 => "feat_audio_tts_provider",
                21 => "feat_audio_tts_model",
                22 => "feat_audio_tts_voice",
                23 => "feat_image_generation_provider",
                24 => "feat_image_generation_model",
                _ => "",
            },
            SettingsTab::Advanced => match self.field_cursor {
                0 => "managed_sandbox_enabled",
                1 => "managed_security_level",
                2 => "auto_compact_context",
                3 => "compaction_strategy",
                4 => "max_context_messages",
                5 => "max_tool_loops",
                6 => "max_retries",
                7 => "retry_delay_ms",
                8 => "message_loop_delay_ms",
                9 => "tool_call_delay_ms",
                10 => "llm_stream_chunk_timeout_secs",
                11 => "auto_retry",
                12 => "context_window_tokens",
                13 => "compact_threshold_pct",
                14 => "keep_recent_on_compact",
                15 => "bash_timeout_secs",
                16 => "weles_max_concurrent_reviews",
                17 => "snapshot_auto_cleanup",
                18 => "snapshot_max_count",
                19 => "snapshot_max_size_mb",
                20 => "snapshot_stats",
                _ => "",
            },
            SettingsTab::Plugins => {
                // In list mode, field_cursor indexes into plugin list.
                // In detail mode, field_cursor indexes into schema fields + actions.
                "plugin_field"
            }
            SettingsTab::About => "",
        }
    }

    pub fn current_field_name_with_config<'a>(&'a self, config: &'a ConfigState) -> &'a str {
        if self.active_tab == SettingsTab::Advanced {
            return Self::advanced_field_names_for_strategy(&config.compaction_strategy)
                .get(self.field_cursor)
                .copied()
                .unwrap_or("snapshot_stats");
        }
        self.current_field_name()
    }

    /// Number of navigable fields in the current tab (for cursor clamping).
    pub fn field_count(&self) -> usize {
        match self.active_tab {
            SettingsTab::Provider => 8,
            SettingsTab::Tools => 7,
            SettingsTab::WebSearch => 8,
            SettingsTab::Chat => 24,
            SettingsTab::Gateway => 14,
            SettingsTab::Auth => 1,
            SettingsTab::Agent => 10,
            SettingsTab::SubAgents => 1,
            SettingsTab::Concierge => 5,
            SettingsTab::Features => 25,
            SettingsTab::Advanced => 21,
            SettingsTab::Plugins => 1,
            SettingsTab::About => 0,
        }
    }

    pub fn field_count_with_config(&self, config: &ConfigState) -> usize {
        if self.active_tab == SettingsTab::Advanced {
            return Self::advanced_field_names_for_strategy(&config.compaction_strategy).len();
        }
        self.field_count()
    }

    pub fn clamp_field_cursor(&mut self, field_count: usize) {
        if field_count == 0 {
            self.field_cursor = 0;
        } else {
            self.field_cursor = self.field_cursor.min(field_count.saturating_sub(1));
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
                self.active_tab = SettingsTab::Auth;
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
                if !field_name.is_empty() {
                    self.editing_field = Some(field_name);
                    self.dirty = true;
                }
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

// ── PluginSettingsState ──────────────────────────────────────────────────────

/// State for the Plugins settings tab. Stored separately from SettingsState
/// because plugin data is dynamic (varies by installed plugins).
#[derive(Debug, Clone)]
pub struct PluginSettingsState {
    /// List of installed plugins (from daemon PluginListResult).
    pub plugins: Vec<PluginListItem>,
    /// Index of selected plugin in the list.
    pub selected_index: usize,
    /// Settings schema fields for the selected plugin (parsed from manifest JSON).
    pub schema_fields: Vec<PluginSchemaField>,
    /// Current setting values for the selected plugin (from daemon).
    pub settings_values: Vec<(String, String, bool)>, // (key, value, is_secret)
    /// Whether we're in plugin list mode (true) or plugin detail mode (false).
    pub list_mode: bool,
    /// Test connection result message (None = not tested yet).
    pub test_result: Option<(bool, String)>,
    /// Loading flag.
    pub loading: bool,
    /// Field cursor in detail mode (indexes into schema_fields + action buttons).
    pub detail_cursor: usize,
}

#[derive(Debug, Clone)]
pub struct PluginListItem {
    pub name: String,
    pub version: String,
    pub enabled: bool,
    pub has_api: bool,
    pub has_auth: bool,
    pub settings_count: u32,
    pub description: Option<String>,
    pub install_source: String,
    pub auth_status: String,
}

#[derive(Debug, Clone)]
pub struct PluginSchemaField {
    pub key: String,
    pub field_type: String,
    pub label: String,
    pub required: bool,
    pub secret: bool,
    pub options: Option<Vec<String>>,
    pub description: Option<String>,
}

impl PluginSettingsState {
    pub fn new() -> Self {
        Self {
            plugins: Vec::new(),
            selected_index: 0,
            schema_fields: Vec::new(),
            settings_values: Vec::new(),
            list_mode: true,
            test_result: None,
            loading: false,
            detail_cursor: 0,
        }
    }

    pub fn selected_plugin(&self) -> Option<&PluginListItem> {
        self.plugins.get(self.selected_index)
    }

    /// field_count in detail mode = number of schema fields + action buttons
    pub fn detail_field_count(&self) -> usize {
        self.schema_fields.len()
            + if self.selected_plugin().map_or(false, |p| p.has_api) {
                1
            } else {
                0
            } // test connection
            + if self.selected_plugin().map_or(false, |p| p.has_auth) {
                1
            } else {
                0
            } // connect button
    }

    /// Get the current value for a schema field key.
    pub fn value_for_key(&self, key: &str) -> Option<&str> {
        self.settings_values
            .iter()
            .find(|(k, _, _)| k == key)
            .map(|(_, v, _)| v.as_str())
    }

    /// Check if a key is a secret field.
    pub fn is_key_secret(&self, key: &str) -> bool {
        self.settings_values
            .iter()
            .find(|(k, _, _)| k == key)
            .map_or(false, |(_, _, is_secret)| *is_secret)
    }
}

impl Default for PluginSettingsState {
    fn default() -> Self {
        Self::new()
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[path = "tests/settings.rs"]
mod tests;
