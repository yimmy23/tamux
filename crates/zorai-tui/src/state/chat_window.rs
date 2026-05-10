use super::AgentThread;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ThreadPageRequest {
    pub message_limit: usize,
    pub message_offset: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct MessageWindow {
    pub start: usize,
    pub end: usize,
    pub total: usize,
    pub loaded_count: usize,
}

impl MessageWindow {
    pub(crate) fn from_thread(thread: &AgentThread) -> Self {
        Self::from_parts(
            thread.total_message_count,
            thread.loaded_message_start,
            thread.loaded_message_end,
            thread.messages.len(),
        )
    }

    pub(crate) fn from_parts(
        total_message_count: usize,
        loaded_message_start: usize,
        loaded_message_end: usize,
        loaded_count: usize,
    ) -> Self {
        let total = total_message_count.max(loaded_count);
        let end = if loaded_message_end == 0 && loaded_count > 0 {
            total
        } else {
            loaded_message_end.max(loaded_count)
        };
        let start = if end >= loaded_count {
            loaded_message_start.min(end.saturating_sub(loaded_count))
        } else {
            0
        };
        Self {
            start,
            end,
            total,
            loaded_count,
        }
    }

    pub(crate) fn prepends_into(self, existing: Self) -> bool {
        self.start < existing.start && self.end >= existing.start
    }

    pub(crate) fn has_non_empty_range(self) -> bool {
        self.start < self.end
    }
}
