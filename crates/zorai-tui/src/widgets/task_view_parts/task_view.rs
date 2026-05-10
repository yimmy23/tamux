use super::sections;
use super::sections::*;
use super::selection;
use super::*;
use crate::state::sidebar::{SidebarItemTarget, SidebarTab};
use crate::state::task::*;
use crate::theme::ThemeTokens;
use ratatui::layout::{Position, Rect};
use ratatui::prelude::*;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::widgets::duration_format::format_duration_ms;
