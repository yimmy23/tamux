use ratatui::prelude::*;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph};

use zorai_protocol::has_whatsapp_allowed_contacts;

use crate::providers;
use crate::state::concierge::ConciergeState;
use crate::state::config::ConfigState;
use crate::state::modal::{ModalState, WhatsAppLinkPhase};
use crate::state::settings::{PluginListItem, PluginSettingsState, SettingsState, SettingsTab};
use crate::state::subagents::SubAgentsState;
use crate::theme::ThemeTokens;
use crate::widgets::message::wrap_text;

#[path = "advanced_single_line_edit_layout_to_subagent_row_action_offsets.rs"]
mod advanced_single_line_edit_layout_to_subagent_row_action_offsets;
#[path = "render_about_tab.rs"]
mod render_about_tab;
#[path = "render_advanced_value_to_render_advanced_tab.rs"]
mod render_advanced_value_to_render_advanced_tab;
#[path = "render_auth_tab_to_render_agent_tab.rs"]
mod render_auth_tab_to_render_agent_tab;
#[path = "render_chat_tab_to_render_honcho_editor_actions.rs"]
mod render_chat_tab_to_render_honcho_editor_actions;
#[path = "render_concierge_tab_to_render_feature_toggle_line.rs"]
mod render_concierge_tab_to_render_feature_toggle_line;
#[path = "render_edit_buffer_with_cursor_to_editing_cursor_hit_test_to_content.rs"]
mod render_edit_buffer_with_cursor_to_editing_cursor_hit_test_to_content;
#[path = "render_features_tab.rs"]
mod render_features_tab;
#[path = "render_gateway_text_field.rs"]
mod render_gateway_text_field;
#[path = "render_plugins_tab_to_connector_readiness_style.rs"]
mod render_plugins_tab_to_connector_readiness_style;
#[path = "render_provider_tab_to_render_tools_tab.rs"]
mod render_provider_tab_to_render_tools_tab;
#[path = "render_websearch_tab.rs"]
mod render_websearch_tab;
#[path = "wrap_textarea_visual_line_to_render_wrapped_textarea_buffer_to_render.rs"]
mod wrap_textarea_visual_line_to_render_wrapped_textarea_buffer_to_render;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::config::ConfigState;
    use crate::state::modal::ModalState;
    use crate::state::settings::SettingsState;

    #[path = "advanced_tab_shows_repo_monitor_checkbox.rs"]
    mod advanced_tab_shows_repo_monitor_checkbox;
    #[path = "settings_handles_empty_state_to_auth_tab_shows_chatgpt_logout.rs"]
    mod settings_handles_empty_state_to_auth_tab_shows_chatgpt_logout;
}

pub(crate) use advanced_single_line_edit_layout_to_subagent_row_action_offsets::*;
pub(crate) use render_about_tab::*;
pub(crate) use render_advanced_value_to_render_advanced_tab::*;
pub(crate) use render_auth_tab_to_render_agent_tab::*;
pub(crate) use render_chat_tab_to_render_honcho_editor_actions::*;
pub(crate) use render_concierge_tab_to_render_feature_toggle_line::*;
pub(crate) use render_edit_buffer_with_cursor_to_editing_cursor_hit_test_to_content::*;
pub(crate) use render_features_tab::*;
pub(crate) use render_gateway_text_field::*;
pub(crate) use render_plugins_tab_to_connector_readiness_style::*;
pub(crate) use render_provider_tab_to_render_tools_tab::*;
pub(crate) use render_websearch_tab::*;
pub(crate) use wrap_textarea_visual_line_to_render_wrapped_textarea_buffer_to_render::*;
