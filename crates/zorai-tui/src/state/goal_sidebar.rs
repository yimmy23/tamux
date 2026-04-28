#![allow(dead_code)]

// ── GoalSidebarTab ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GoalSidebarTab {
    Steps,
    Checkpoints,
    Tasks,
    Files,
}

// ── GoalSidebarState ─────────────────────────────────────────────────────────

pub struct GoalSidebarState {
    active_tab: GoalSidebarTab,
    selected_row: usize,
    scroll_offset: usize,
}

impl GoalSidebarState {
    pub fn new() -> Self {
        Self {
            active_tab: GoalSidebarTab::Steps,
            selected_row: 0,
            scroll_offset: 0,
        }
    }

    pub fn active_tab(&self) -> GoalSidebarTab {
        self.active_tab
    }

    pub fn selected_row(&self) -> usize {
        self.selected_row
    }

    pub fn scroll_offset(&self) -> usize {
        self.scroll_offset
    }

    pub fn cycle_tab_left(&mut self) {
        self.active_tab = match self.active_tab {
            GoalSidebarTab::Steps => GoalSidebarTab::Steps,
            GoalSidebarTab::Checkpoints => GoalSidebarTab::Steps,
            GoalSidebarTab::Tasks => GoalSidebarTab::Checkpoints,
            GoalSidebarTab::Files => GoalSidebarTab::Tasks,
        };
        self.selected_row = 0;
        self.scroll_offset = 0;
    }

    pub fn cycle_tab_right(&mut self) {
        self.active_tab = match self.active_tab {
            GoalSidebarTab::Steps => GoalSidebarTab::Checkpoints,
            GoalSidebarTab::Checkpoints => GoalSidebarTab::Tasks,
            GoalSidebarTab::Tasks => GoalSidebarTab::Files,
            GoalSidebarTab::Files => GoalSidebarTab::Files,
        };
        self.selected_row = 0;
        self.scroll_offset = 0;
    }

    pub fn select_row(&mut self, index: usize, item_count: usize) {
        self.selected_row = if item_count == 0 {
            0
        } else {
            index.min(item_count.saturating_sub(1))
        };
    }

    pub fn scroll(&mut self, delta: i32) {
        if delta >= 0 {
            self.scroll_offset = self.scroll_offset.saturating_add(delta as usize);
        } else {
            self.scroll_offset = self.scroll_offset.saturating_sub((-delta) as usize);
        }
    }

    pub fn navigate(&mut self, delta: i32, item_count: usize) {
        if item_count == 0 {
            self.selected_row = 0;
            return;
        }

        if delta >= 0 {
            self.selected_row = self
                .selected_row
                .saturating_add(delta as usize)
                .min(item_count.saturating_sub(1));
        } else {
            self.selected_row = self.selected_row.saturating_sub((-delta) as usize);
        }
    }
}

impl Default for GoalSidebarState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tab_switch_resets_selection_and_scroll() {
        let mut state = GoalSidebarState::new();
        state.select_row(2, 5);
        state.scroll(3);

        state.cycle_tab_right();

        assert_eq!(state.active_tab(), GoalSidebarTab::Checkpoints);
        assert_eq!(state.selected_row(), 0);
        assert_eq!(state.scroll_offset(), 0);
    }

    #[test]
    fn scroll_clamps_at_zero() {
        let mut state = GoalSidebarState::new();
        state.scroll(4);
        state.scroll(-2);
        state.scroll(-10);

        assert_eq!(state.scroll_offset(), 0);
    }

    #[test]
    fn navigate_clamps_for_empty_and_non_empty_lists() {
        let mut state = GoalSidebarState::new();

        state.navigate(3, 0);
        assert_eq!(state.selected_row(), 0);

        state.navigate(3, 2);
        assert_eq!(state.selected_row(), 1);

        state.navigate(-10, 2);
        assert_eq!(state.selected_row(), 0);
    }
}
