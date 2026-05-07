use super::render_provider_tab_to_render_tools_tab::*;
use super::render_websearch_tab::*;
use super::render_chat_tab_to_render_honcho_editor_actions::*;
use super::render_gateway_text_field::*;
use super::render_concierge_tab_to_render_feature_toggle_line::*;
use super::render_features_tab::*;
use super::render_auth_tab_to_render_agent_tab::*;
use super::render_plugins_tab_to_connector_readiness_style::*;
use super::render_advanced_value_to_render_advanced_tab::*;
use super::*;
use crate::providers;
use crate::state::concierge::ConciergeState;
use crate::state::config::ConfigState;
use crate::state::modal::{ModalState, WhatsAppLinkPhase};
use crate::state::settings::{PluginListItem, PluginSettingsState, SettingsState, SettingsTab};
use crate::state::subagents::SubAgentsState;
use crate::theme::ThemeTokens;
use crate::widgets::message::wrap_text;
use ratatui::prelude::*;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph};
use zorai_protocol::has_whatsapp_allowed_contacts;
pub(crate) fn render_about_tab(theme: &ThemeTokens) -> Vec<Line<'static>> {
    vec![
        Line::from(Span::styled("zorai", theme.fg_active)),
        Line::raw(""),
        Line::from(vec![
            Span::styled("Version:   ", theme.fg_dim),
            Span::raw(env!("CARGO_PKG_VERSION")),
        ]),
        Line::from(vec![
            Span::styled("Author:    ", theme.fg_dim),
            Span::raw("Mariusz Kurman"),
        ]),
        Line::from(vec![
            Span::styled("GitHub:    ", theme.fg_dim),
            Span::raw("mkurman/zorai"),
        ]),
        Line::from(vec![
            Span::styled("Homepage:  ", theme.fg_dim),
            Span::raw("zorai.app"),
        ]),
    ]
}