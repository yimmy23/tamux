impl InputState {
    pub fn can_browse_sent_history(&self) -> bool {
        self.history_cursor.is_some() || self.buffer_cache.is_empty()
    }

    fn remember_submitted_prompt(&mut self, prompt: &str) {
        self.sent_history.push(prompt.to_string());
        self.history_cursor = None;
    }

    fn commit_history_selection(&mut self) {
        self.history_cursor = None;
    }

    fn browse_history_previous(&mut self) {
        if self.sent_history.is_empty() {
            return;
        }
        if self.history_cursor.is_none() && !self.buffer_cache.is_empty() {
            return;
        }

        let next_cursor = match self.history_cursor {
            Some(0) => 0,
            Some(cursor) => cursor.saturating_sub(1),
            None => self.sent_history.len() - 1,
        };

        self.history_cursor = Some(next_cursor);
        let prompt = self.sent_history[next_cursor].clone();
        self.set_text(&prompt);
    }

    fn browse_history_next(&mut self) {
        let Some(cursor) = self.history_cursor else {
            return;
        };

        let next_cursor = cursor + 1;
        if next_cursor >= self.sent_history.len() {
            self.history_cursor = Some(self.sent_history.len());
            self.clear_text();
            return;
        }

        self.history_cursor = Some(next_cursor);
        let prompt = self.sent_history[next_cursor].clone();
        self.set_text(&prompt);
    }
}
