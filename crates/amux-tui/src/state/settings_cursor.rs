use super::*;

impl SettingsState {
    pub(super) fn line_col_to_offset(&self, target_line: usize, target_col: usize) -> usize {
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

    pub(super) fn move_cursor_left(&mut self) {
        if self.edit_cursor > 0 {
            self.edit_cursor = self.edit_buffer[..self.edit_cursor]
                .char_indices()
                .last()
                .map(|(i, _)| i)
                .unwrap_or(0);
        }
    }

    pub(super) fn move_cursor_right(&mut self) {
        if self.edit_cursor < self.edit_buffer.len() {
            self.edit_cursor = self.edit_buffer[self.edit_cursor..]
                .char_indices()
                .nth(1)
                .map(|(i, _)| self.edit_cursor + i)
                .unwrap_or(self.edit_buffer.len());
        }
    }

    pub(super) fn move_cursor_up(&mut self) {
        let (line, col) = self.edit_cursor_line_col();
        if line > 0 {
            self.edit_cursor = self.line_col_to_offset(line - 1, col);
        }
    }

    pub(super) fn move_cursor_down(&mut self) {
        let (line, col) = self.edit_cursor_line_col();
        let line_count = self.edit_buffer.matches('\n').count() + 1;
        if line + 1 < line_count {
            self.edit_cursor = self.line_col_to_offset(line + 1, col);
        }
    }

    pub(super) fn move_cursor_home(&mut self) {
        let before = &self.edit_buffer[..self.edit_cursor];
        self.edit_cursor = before.rfind('\n').map(|i| i + 1).unwrap_or(0);
    }

    pub(super) fn move_cursor_end(&mut self) {
        let after = &self.edit_buffer[self.edit_cursor..];
        if let Some(newline) = after.find('\n') {
            self.edit_cursor += newline;
        } else {
            self.edit_cursor = self.edit_buffer.len();
        }
    }
}
