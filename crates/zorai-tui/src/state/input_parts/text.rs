impl InputState {
    /// Replace input buffer with the given text and place cursor at end.
    pub fn set_text(&mut self, text: &str) {
        self.clear_text();
        self.paste_blocks.clear();
        // Insert char-by-char so the textarea cursor advances with each character.
        // insert_str may not update the cursor position in all tui-textarea versions.
        for ch in text.chars() {
            self.textarea.insert_char(ch);
        }
        self.sync_buffer_cache();
    }
}
