use super::super::advanced_single_line_edit_layout_to_subagent_row_action_offsets::*;
use super::super::render_advanced_value_to_render_advanced_tab::*;
use super::super::render_edit_buffer_with_cursor_to_editing_cursor_hit_test_to_content::*;
use super::super::wrap_textarea_visual_line_to_render_wrapped_textarea_buffer_to_render::*;
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
pub(crate) fn render_advanced_toggle<'a>(
    lines: &mut Vec<Line<'a>>,
    settings: &'a SettingsState,
    field_idx: usize,
    label: &'a str,
    enabled: bool,
    theme: &ThemeTokens,
) {
    let is_selected = settings.field_cursor() == field_idx;
    let marker = if is_selected { "> " } else { "  " };
    let marker_style = if is_selected {
        theme.accent_primary
    } else {
        theme.fg_dim
    };
    let _check = if enabled { "[x]" } else { "[ ]" };
    let _check_style = if enabled {
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
        Span::styled(label, label_style),
    ];
    if is_selected {
        spans.push(Span::styled("  [Space: toggle]", theme.fg_dim));
    }
    lines.push(Line::from(spans));
}

pub(crate) fn render_advanced_value<'a>(
    lines: &mut Vec<Line<'a>>,
    settings: &'a SettingsState,
    config: &'a ConfigState,
    field_idx: usize,
    label: &'a str,
    value: String,
    field_name: &'a str,
    hint: &'a str,
    theme: &ThemeTokens,
) {
    let is_selected = settings.field_cursor() == field_idx;
    let is_editing = settings.is_editing() && settings.editing_field() == Some(field_name);
    let marker = if is_selected { "> " } else { "  " };
    let marker_style = if is_selected {
        theme.accent_primary
    } else {
        theme.fg_dim
    };
    let display_value = if is_editing {
        format!("{}\u{2588}", settings.edit_buffer())
    } else {
        value
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
        spans.push(Span::styled(hint, theme.fg_dim));
    }
    if field_name == "context_window_tokens"
        && !providers::model_uses_context_window_override(
            &config.provider,
            &config.auth_source,
            &config.model,
            &config.custom_model_name,
        )
    {
        spans.push(Span::styled("  [derived]", theme.fg_dim));
    }
    lines.push(Line::from(spans));
}
