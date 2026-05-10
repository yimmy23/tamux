use super::markdown_table;
use super::*;
use crate::state::chat::{AgentMessage, MessageRole, TranscriptMode};
use crate::theme::ThemeTokens;
use crate::widgets::image_preview;
use crate::widgets::message_operator_question::render_operator_question_message;
use crate::widgets::tool_diff::{
    render_tool_edit_diff, render_tool_structured_json, ToolStructuredValueSource,
};
use ratatui::prelude::*;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};
use zorai_protocol::tool_names;
