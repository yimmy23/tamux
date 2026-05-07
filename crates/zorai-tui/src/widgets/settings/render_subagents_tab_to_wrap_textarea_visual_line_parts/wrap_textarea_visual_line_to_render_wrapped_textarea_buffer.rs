use super::*;
use super::super::advanced_single_line_edit_layout_to_subagent_row_action_offsets::*;
use super::super::render_edit_buffer_with_cursor_to_editing_cursor_hit_test_to_content::*;
use super::super::wrap_textarea_visual_line_to_render_wrapped_textarea_buffer_to_render::*;
use super::super::render_advanced_value_to_render_advanced_tab::*;
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
pub(crate) fn wrap_textarea_visual_line(text: &str, width: usize) -> Vec<String> {
    if width == 0 {
        return vec![text.to_string()];
    }
    if text.is_empty() {
        return vec![String::new()];
    }

    let mut lines = Vec::new();
    let mut current = String::new();
    let mut current_width = 0usize;

    for ch in text.chars() {
        let ch_width = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0);
        let next_width = current_width + ch_width;
        if !current.is_empty() && next_width > width {
            lines.push(std::mem::take(&mut current));
            current_width = 0;
        }
        current.push(ch);
        current_width += ch_width;
    }

    if current.is_empty() {
        lines.push(String::new());
    } else {
        lines.push(current);
    }

    lines
}

pub(crate) fn pad_visual_width(text: &str, width: usize) -> String {
    let visible_width = unicode_width::UnicodeWidthStr::width(text);
    if visible_width >= width {
        return text.to_string();
    }

    let mut padded = String::with_capacity(text.len() + (width - visible_width));
    padded.push_str(text);
    padded.push_str(&" ".repeat(width - visible_width));
    padded
}

pub(crate) fn render_wrapped_textarea_buffer(
    buffer: &str,
    cursor_line: usize,
    cursor_col: usize,
    width: usize,
) -> Vec<String> {
    let mut visual_lines = Vec::new();

    for (idx, raw_line) in buffer.split('\n').enumerate() {
        let rendered = if idx == cursor_line {
            render_edit_line_with_cursor(raw_line, cursor_col)
        } else {
            raw_line.to_string()
        };
        visual_lines.extend(wrap_textarea_visual_line(&rendered, width.max(1)));
    }

    if visual_lines.is_empty() {
        visual_lines.push(render_edit_line_with_cursor("", 0));
    }

    visual_lines
}
