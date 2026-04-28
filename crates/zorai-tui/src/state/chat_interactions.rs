use super::*;

impl ChatState {
    pub fn expanded_reasoning(&self) -> std::collections::HashSet<usize> {
        self.resolve_active_message_ref_set(&self.expanded_reasoning)
    }

    pub fn toggle_reasoning(&mut self, msg_index: usize) {
        let Some(message_ref) = self.message_ref_for_active_index(msg_index) else {
            return;
        };
        if self.expanded_reasoning.contains(&message_ref) {
            self.expanded_reasoning.remove(&message_ref);
        } else {
            self.expanded_reasoning.insert(message_ref);
        }
        self.bump_render_revision();
    }

    pub fn selected_message(&self) -> Option<usize> {
        self.selected_message
            .as_ref()
            .and_then(|message_ref| self.resolve_active_message_ref(message_ref))
    }

    pub fn selected_message_action(&self) -> usize {
        self.selected_message_action
    }

    pub fn select_message_action(&mut self, index: usize) {
        self.selected_message_action = index;
        self.bump_render_revision();
    }

    pub fn navigate_selected_message_action(&mut self, delta: i32, action_count: usize) {
        if action_count == 0 {
            self.selected_message_action = 0;
        } else if delta > 0 {
            self.selected_message_action =
                (self.selected_message_action + delta as usize).min(action_count - 1);
        } else {
            self.selected_message_action = self
                .selected_message_action
                .saturating_sub((-delta) as usize);
        }
        self.bump_render_revision();
    }

    pub fn select_message(&mut self, index: Option<usize>) {
        self.selected_message = index.and_then(|index| self.message_ref_for_active_index(index));
        self.selected_message_action = 0;
        self.bump_render_revision();
    }

    pub fn toggle_message_selection(&mut self, index: usize) {
        if self.selected_message() == Some(index) {
            self.selected_message = None;
            self.selected_message_action = 0;
        } else {
            self.selected_message = self.message_ref_for_active_index(index);
            self.selected_message_action = 0;
        }
        self.bump_render_revision();
    }

    pub fn select_next_message(&mut self) {
        let count = self
            .active_thread()
            .map(|thread| thread.messages.len())
            .unwrap_or(0);
        if count == 0 {
            self.selected_message = None;
            self.selected_message_action = 0;
            return;
        }
        match self.selected_message() {
            None => {
                self.selected_message = self.message_ref_for_active_index(0);
                self.selected_message_action = 0;
            }
            Some(index) => {
                if index + 1 < count {
                    self.selected_message = self.message_ref_for_active_index(index + 1);
                    self.selected_message_action = 0;
                }
            }
        }
        self.bump_render_revision();
    }

    pub fn select_prev_message(&mut self) {
        match self.selected_message() {
            None => {
                let count = self
                    .active_thread()
                    .map(|thread| thread.messages.len())
                    .unwrap_or(0);
                if count > 0 {
                    self.selected_message = self.message_ref_for_active_index(count - 1);
                    self.selected_message_action = 0;
                }
            }
            Some(0) => {}
            Some(index) => {
                self.selected_message = self.message_ref_for_active_index(index - 1);
                self.selected_message_action = 0;
            }
        }
        self.bump_render_revision();
    }

    pub fn expanded_tools(&self) -> std::collections::HashSet<usize> {
        self.resolve_active_message_ref_set(&self.expanded_tools)
    }

    pub fn toggle_tool_expansion(&mut self, msg_index: usize) {
        let Some(message_ref) = self.message_ref_for_active_index(msg_index) else {
            return;
        };
        if self.expanded_tools.contains(&message_ref) {
            self.expanded_tools.remove(&message_ref);
        } else {
            self.expanded_tools.insert(message_ref);
        }
        self.bump_render_revision();
    }

    pub fn toggle_last_reasoning(&mut self) {
        if let Some(thread) = self.active_thread() {
            for (index, message) in thread.messages.iter().enumerate().rev() {
                if message.role == MessageRole::Assistant && message.reasoning.is_some() {
                    let Some(message_ref) = stored_message_ref(thread, index) else {
                        return;
                    };
                    if self.expanded_reasoning.contains(&message_ref) {
                        self.expanded_reasoning.remove(&message_ref);
                    } else {
                        self.expanded_reasoning.insert(message_ref);
                    }
                    self.bump_render_revision();
                    return;
                }
            }
        }
    }

    pub fn mark_message_copied(&mut self, message_index: usize, expires_at_tick: u64) {
        let Some(message_ref) = self.message_ref_for_active_index(message_index) else {
            return;
        };
        self.copied_message_feedback = Some(CopiedMessageFeedback {
            message_ref,
            expires_at_tick,
        });
        self.bump_render_revision();
    }

    pub fn clear_expired_copy_feedback(&mut self, current_tick: u64) {
        if self
            .copied_message_feedback
            .as_ref()
            .is_some_and(|feedback| current_tick >= feedback.expires_at_tick)
        {
            self.copied_message_feedback = None;
            self.bump_render_revision();
        }
    }

    pub fn is_message_recently_copied(&self, message_index: usize, current_tick: u64) -> bool {
        self.copied_message_feedback
            .as_ref()
            .is_some_and(|feedback| {
                current_tick < feedback.expires_at_tick
                    && self.resolve_active_message_ref(&feedback.message_ref) == Some(message_index)
            })
    }
}
