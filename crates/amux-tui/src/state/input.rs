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
}

impl InputState {
    pub fn new() -> Self {
        let textarea = TextArea::default();
        let buffer_cache = textarea.lines().join("\n");
        Self {
            textarea,
            buffer_cache,
            mode: InputMode::Insert, // Start in Insert mode
            submitted: None,
            paste_blocks: Vec::new(),
            next_paste_id: 0,
        }
    }

    pub fn buffer(&self) -> &str {
        &self.buffer_cache
    }

    pub fn mode(&self) -> InputMode {
        self.mode
    }

    pub fn multiline(&self) -> bool {
        self.textarea.lines().len() > 1
    }

    pub fn take_submitted(&mut self) -> Option<String> {
        self.submitted.take()
    }

    pub fn set_mode(&mut self, mode: InputMode) {
        self.mode = mode;
    }

    /// Get cursor byte offset
    pub fn cursor_pos(&self) -> usize {
        let (target_row, target_col) = self.textarea.cursor();
        let mut offset = 0usize;

        for (row, line) in self.textarea.lines().iter().enumerate() {
            if row == target_row {
                let byte_offset = line
                    .char_indices()
                    .nth(target_col)
                    .map(|(i, _)| i)
                    .unwrap_or(line.len());
                return offset + byte_offset;
            }
            offset += line.len() + 1;
        }

        self.buffer_cache.len()
    }

    /// Get cursor (line, col) for rendering
    pub fn cursor_line_col_public(&self) -> (usize, usize) {
        self.textarea.cursor()
    }

    /// Convert (line, col) to byte offset (public, for mouse click positioning)
    pub fn line_col_to_offset_public(&self, line: usize, col: usize) -> usize {
        self.line_col_to_offset(line, col)
    }

    /// Return the stored paste blocks (for rendering).
    pub fn paste_blocks(&self) -> &[PasteBlock] {
        &self.paste_blocks
    }

    /// Insert a paste block at the current cursor position.
    ///
    /// Single-line pastes are inserted character-by-character as normal text.
    /// Multi-line pastes store the full content in `paste_blocks` and insert
    /// a NUL-delimited placeholder token (`\x00PASTE:N\x00`) into the buffer
    /// so the display layer can render a collapsed amber preview.
    pub fn insert_paste_block(&mut self, content: String) {
        let line_count = content.lines().count();
        if line_count <= 1 {
            self.textarea.insert_str(content);
            self.sync_buffer_cache();
            return;
        }
        let id = self.next_paste_id;
        self.next_paste_id += 1;
        let placeholder = format!("\x00PASTE:{}\x00", id);
        self.textarea.insert_str(placeholder);
        self.paste_blocks.push(PasteBlock {
            id,
            content,
            line_count,
        });
        self.sync_buffer_cache();
    }

    /// Expand all paste-block placeholder tokens in `text`, replacing them
    /// with their full content.  Called on submit so the message sent to the
    /// daemon contains the real text.
    pub fn expand_paste_blocks(&self, text: &str) -> String {
        let mut result = text.to_string();
        for block in &self.paste_blocks {
            let placeholder = format!("\x00PASTE:{}\x00", block.id);
            result = result.replace(&placeholder, &block.content);
        }
        result
    }

    /// Build the amber display label for a paste-block placeholder.
    /// Returns `None` when no block with the given `id` exists.
    pub fn paste_block_display(id: usize, blocks: &[PasteBlock]) -> Option<String> {
        blocks.iter().find(|b| b.id == id).map(|b| {
            let first_line = b.content.lines().next().unwrap_or("");
            let truncated: String = first_line.chars().take(20).collect();
            let ellipsis = if first_line.chars().count() > 20 {
                "..."
            } else {
                ""
            };
            format!(
                "[Pasted text #{} +{} lines: {}{}]",
                b.id + 1,
                b.line_count,
                truncated,
                ellipsis
            )
        })
    }

    pub fn active_at_token(&self) -> Option<input_refs::ActiveToken> {
        input_refs::active_at_token(self.buffer(), self.cursor_pos())
    }

    pub fn complete_active_at_token(&mut self) -> input_refs::TabCompletionOutcome {
        self.complete_active_at_token_with_agents(&[])
    }

    pub fn complete_active_at_token_with_agents(
        &mut self,
        agent_aliases: &[String],
    ) -> input_refs::TabCompletionOutcome {
        let cursor = self.cursor_pos();
        let buffer = self.buffer_cache.clone();
        let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
        let outcome =
            input_refs::complete_active_at_token_with_agents(&buffer, cursor, &cwd, agent_aliases);

        if let Some(replacement) = &outcome.replacement {
            let mut updated = buffer;
            updated.replace_range(replacement.range.clone(), &replacement.text);
            self.set_buffer_and_cursor(&updated, replacement.range.start + replacement.text.len());
        }

        outcome
    }

    /// Get (line_index, col_index) for current cursor position
    fn cursor_line_col(&self) -> (usize, usize) {
        self.textarea.cursor()
    }

    /// Convert (line, col) to byte offset, clamping col to line length
    fn line_col_to_offset(&self, line: usize, col: usize) -> usize {
        let mut offset = 0;
        for (i, line_str) in self.buffer_cache.split('\n').enumerate() {
            if i == line {
                return offset + col.min(line_str.len());
            }
            offset += line_str.len() + 1; // +1 for \n
        }
        self.buffer_cache.len() // past end
    }

    fn sync_buffer_cache(&mut self) {
        self.buffer_cache = self.textarea.lines().join("\n");
    }

    fn set_buffer_and_cursor(&mut self, buffer: &str, cursor: usize) {
        self.textarea = TextArea::from(buffer.split('\n'));
        self.sync_buffer_cache();
        self.jump_to_offset(cursor.min(self.buffer_cache.len()));
    }

    fn find_placeholder_bounds_at(&self, cursor: usize) -> Option<(usize, usize, usize)> {
        let buffer = &self.buffer_cache;
        let mut search_start = 0usize;

        while let Some(rel_start) = buffer[search_start..].find("\x00PASTE:") {
            let start = search_start + rel_start;
            let rest = &buffer[start + 1..];
            let end_rel = rest.find('\x00')?;
            let end = start + 1 + end_rel + 1;
            if cursor > start && cursor <= end {
                let tag = &rest[..end_rel];
                if let Some(id_str) = tag.strip_prefix("PASTE:") {
                    if let Ok(id) = id_str.parse::<usize>() {
                        return Some((start, end, id));
                    }
                }
            }
            search_start = end;
        }

        None
    }

    fn find_placeholder_bounds_touching_cursor(
        &self,
        cursor: usize,
    ) -> Option<(usize, usize, usize)> {
        self.find_placeholder_bounds_at(cursor).or_else(|| {
            cursor
                .checked_sub(1)
                .and_then(|previous| self.find_placeholder_bounds_at(previous))
        })
    }

    fn remove_paste_block_at_cursor(&mut self) -> bool {
        let cursor = self.cursor_pos();
        let Some((start, end, id)) = self.find_placeholder_bounds_touching_cursor(cursor) else {
            return false;
        };

        let mut new_buffer =
            String::with_capacity(self.buffer_cache.len().saturating_sub(end - start));
        new_buffer.push_str(&self.buffer_cache[..start]);
        new_buffer.push_str(&self.buffer_cache[end..]);
        self.paste_blocks.retain(|block| block.id != id);
        self.set_buffer_and_cursor(&new_buffer, start);
        true
    }

    /// Get cursor position in visual (wrapped) line coordinates.
    /// Each logical line is split into visual lines of `wrap_width` chars.
    fn cursor_visual_line_col(&self, wrap_width: usize) -> (usize, usize) {
        let mut vis_line = 0;
        let mut offset = 0;
        for logical_line in self.buffer_cache.split('\n') {
            let len = logical_line.chars().count();
            let vis_lines_in_this = if wrap_width == 0 || len == 0 {
                1
            } else {
                (len + wrap_width - 1) / wrap_width
            };

            let line_start = offset;
            let line_end = offset + logical_line.len();

            let cursor = self.cursor_pos();
            if cursor >= line_start && cursor <= line_end {
                // Cursor is in this logical line
                let chars_before = self.buffer_cache[line_start..cursor].chars().count();
                let vis_line_in_this = chars_before / wrap_width.max(1);
                let vis_col = chars_before % wrap_width.max(1);
                return (vis_line + vis_line_in_this, vis_col);
            }

            vis_line += vis_lines_in_this;
            offset = line_end + 1; // +1 for \n
        }
        (vis_line, 0)
    }

    /// Total number of visual lines when wrapping at `wrap_width`.
    fn total_visual_lines(&self, wrap_width: usize) -> usize {
        self.buffer_cache
            .split('\n')
            .map(|line| {
                let len = line.chars().count();
                if wrap_width == 0 || len == 0 {
                    1
                } else {
                    (len + wrap_width - 1) / wrap_width
                }
            })
            .sum()
    }

    /// Convert visual (wrapped) line + col to byte offset.
    fn visual_line_col_to_offset(
        &self,
        target_vis_line: usize,
        target_col: usize,
        wrap_width: usize,
    ) -> usize {
        let mut vis_line = 0;
        let mut offset = 0;
        for logical_line in self.buffer_cache.split('\n') {
            let len = logical_line.chars().count();
            let vis_lines_in_this = if wrap_width == 0 || len == 0 {
                1
            } else {
                (len + wrap_width - 1) / wrap_width
            };

            if target_vis_line >= vis_line && target_vis_line < vis_line + vis_lines_in_this {
                // Target is in this logical line
                let vis_line_within = target_vis_line - vis_line;
                let char_offset = vis_line_within * wrap_width + target_col.min(wrap_width - 1);
                let clamped = char_offset.min(len);
                // Convert char offset to byte offset
                let byte_offset: usize = logical_line
                    .chars()
                    .take(clamped)
                    .map(|c| c.len_utf8())
                    .sum();
                return offset + byte_offset;
            }

            vis_line += vis_lines_in_this;
            offset += logical_line.len() + 1; // +1 for \n
        }
        self.buffer_cache.len()
    }

    fn jump_to_line_col(&mut self, row: usize, col: usize) {
        self.textarea
            .move_cursor(CursorMove::Jump(row as u16, col as u16));
    }

    fn jump_to_offset(&mut self, offset: usize) {
        let offset = offset.min(self.buffer_cache.len());
        let mut remaining = offset;

        for (row, line) in self.buffer_cache.split('\n').enumerate() {
            if remaining <= line.len() {
                let col = line[..remaining].chars().count().min(line.chars().count());
                self.jump_to_line_col(row, col);
                return;
            }
            remaining = remaining.saturating_sub(line.len() + 1);
        }

        let last_row = self.textarea.lines().len().saturating_sub(1);
        let last_col = self
            .textarea
            .lines()
            .get(last_row)
            .map(|line| line.chars().count())
            .unwrap_or(0);
        self.jump_to_line_col(last_row, last_col);
    }

    fn clear_text(&mut self) {
        if !self.buffer_cache.is_empty() {
            self.textarea.select_all();
            self.textarea.cut();
        }
        self.sync_buffer_cache();
    }

    /// Replace input buffer with the given text and place cursor at end.
    pub fn set_text(&mut self, text: &str) {
        self.clear_text();
        // Insert char-by-char so the textarea cursor advances with each character.
        // insert_str may not update the cursor position in all tui-textarea versions.
        for ch in text.chars() {
            self.textarea.insert_char(ch);
        }
        self.sync_buffer_cache();
    }

    pub fn reduce(&mut self, action: InputAction) {
        match action {
            InputAction::InsertChar(c) => {
                self.textarea.insert_char(c);
                self.sync_buffer_cache();
            }
            InputAction::Backspace => {
                if !self.remove_paste_block_at_cursor() {
                    self.textarea.delete_char();
                    self.sync_buffer_cache();
                }
            }
            InputAction::DeleteWord => {
                if !self.remove_paste_block_at_cursor() {
                    self.textarea.delete_word();
                    self.sync_buffer_cache();
                }
            }
            InputAction::ClearLine => {
                self.clear_text();
                self.paste_blocks.clear();
            }
            InputAction::Submit => {
                if !self.buffer_cache.trim().is_empty() {
                    let expanded = self.expand_paste_blocks(&self.buffer_cache);
                    self.submitted = Some(expanded);
                    self.textarea = TextArea::default();
                    self.sync_buffer_cache();
                    self.paste_blocks.clear();
                }
            }
            InputAction::ToggleMode => {
                self.mode = match self.mode {
                    InputMode::Normal => InputMode::Insert,
                    InputMode::Insert => InputMode::Normal,
                };
            }
            InputAction::Clear => {
                self.clear_text();
                self.paste_blocks.clear();
            }
            InputAction::InsertNewline => {
                self.textarea.insert_newline();
                self.sync_buffer_cache();
            }
            InputAction::MoveCursorLeft => {
                self.textarea.move_cursor(CursorMove::Back);
            }
            InputAction::MoveCursorRight => {
                self.textarea.move_cursor(CursorMove::Forward);
            }
            InputAction::MoveCursorUp => {
                let (line, col) = self.cursor_line_col();
                if line > 0 {
                    self.jump_to_line_col(line - 1, col);
                }
            }
            InputAction::MoveCursorDown => {
                let (line, col) = self.cursor_line_col();
                let line_count = self.textarea.lines().len();
                if line + 1 < line_count {
                    self.jump_to_line_col(line + 1, col);
                }
            }
            InputAction::MoveCursorUpVisual(wrap_width) => {
                if wrap_width == 0 {
                    return;
                }
                // Find cursor's position in visual (wrapped) coordinates
                let (vis_line, vis_col) = self.cursor_visual_line_col(wrap_width);
                if vis_line > 0 {
                    let offset = self.visual_line_col_to_offset(vis_line - 1, vis_col, wrap_width);
                    self.jump_to_offset(offset);
                }
            }
            InputAction::MoveCursorDownVisual(wrap_width) => {
                if wrap_width == 0 {
                    return;
                }
                let (vis_line, vis_col) = self.cursor_visual_line_col(wrap_width);
                let total_vis_lines = self.total_visual_lines(wrap_width);
                if vis_line + 1 < total_vis_lines {
                    let offset = self.visual_line_col_to_offset(vis_line + 1, vis_col, wrap_width);
                    self.jump_to_offset(offset);
                }
            }
            InputAction::MoveCursorHome => {
                let (row, _) = self.cursor_line_col();
                self.jump_to_line_col(row, 0);
            }
            InputAction::MoveCursorEnd => {
                let (row, _) = self.cursor_line_col();
                let col = self
                    .textarea
                    .lines()
                    .get(row)
                    .map(|line| line.chars().count())
                    .unwrap_or(0);
                self.jump_to_line_col(row, col);
            }
            InputAction::MoveCursorToPos(pos) => {
                self.jump_to_offset(pos);
            }
            InputAction::Undo => {
                self.textarea.undo();
                self.sync_buffer_cache();
            }
            InputAction::Redo => {
                self.textarea.redo();
                self.sync_buffer_cache();
            }
        }
    }
}

#[cfg(test)]
#[path = "tests/input.rs"]
mod tests;
