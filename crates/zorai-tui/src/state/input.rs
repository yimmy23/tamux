#![allow(dead_code)]

use ratatui_textarea::{CursorMove, TextArea};
use unicode_width::UnicodeWidthChar;

use crate::state::input_refs;

mod display;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputMode {
    Normal,
    Insert,
}

#[derive(Debug, Clone)]
pub enum InputAction {
    InsertChar(char),
    Backspace,
    DeleteWord, // delete word before cursor (Ctrl+Backspace / Ctrl+W)
    ClearLine,  // clear entire input line (Ctrl+U)
    Submit,
    ToggleMode,
    Clear,
    InsertNewline,
    MoveCursorLeft,
    MoveCursorRight,
    MoveCursorUp,
    MoveCursorDown,
    /// Move cursor up one visual line (accounting for text wrapping at given width)
    MoveCursorUpVisual(usize),
    /// Move cursor down one visual line
    MoveCursorDownVisual(usize),
    MoveCursorHome,
    MoveCursorEnd,
    MoveCursorToPos(usize),
    HistoryPrevious,
    HistoryNext,
    Undo,
    Redo,
}

/// A pasted multi-line block stored out-of-band; a placeholder token is kept
/// in the buffer and expanded to the full content on submit.
#[derive(Debug, Clone)]
pub struct PasteBlock {
    pub id: usize,
    pub content: String,
    pub line_count: usize,
}

pub struct InputState {
    textarea: TextArea<'static>,
    buffer_cache: String,
    mode: InputMode,
    submitted: Option<String>,
    paste_blocks: Vec<PasteBlock>,
    next_paste_id: usize,
    sent_history: Vec<String>,
    history_cursor: Option<usize>,
}

include!("input_parts/history.rs");
include!("input_parts/new_to_reduce.rs");
include!("input_parts/text.rs");

#[cfg(test)]
#[path = "tests/input.rs"]
mod tests;
