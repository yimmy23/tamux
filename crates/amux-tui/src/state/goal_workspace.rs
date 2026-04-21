#![allow(dead_code)]

use std::collections::BTreeSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GoalWorkspacePane {
    Plan,
    Timeline,
    Details,
    CommandBar,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GoalPlanSelection {
    Step { step_id: String },
    Todo { step_id: String, todo_id: String },
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GoalWorkspaceState {
    focused_pane: GoalWorkspacePane,
    selected_plan_row: usize,
    expanded_step_ids: BTreeSet<String>,
    selected_timeline_row: usize,
    selected_detail_row: usize,
    selected_plan_item: Option<GoalPlanSelection>,
}

impl GoalWorkspaceState {
    pub fn new() -> Self {
        Self {
            focused_pane: GoalWorkspacePane::Plan,
            selected_plan_row: 0,
            expanded_step_ids: BTreeSet::new(),
            selected_timeline_row: 0,
            selected_detail_row: 0,
            selected_plan_item: None,
        }
    }

    pub fn focused_pane(&self) -> GoalWorkspacePane {
        self.focused_pane
    }

    pub fn set_focused_pane(&mut self, pane: GoalWorkspacePane) {
        self.focused_pane = pane;
    }

    pub fn selected_plan_row(&self) -> usize {
        self.selected_plan_row
    }

    pub fn set_selected_plan_row(&mut self, row: usize) {
        self.selected_plan_row = row;
    }

    pub fn selected_timeline_row(&self) -> usize {
        self.selected_timeline_row
    }

    pub fn set_selected_timeline_row(&mut self, row: usize) {
        self.selected_timeline_row = row;
    }

    pub fn selected_detail_row(&self) -> usize {
        self.selected_detail_row
    }

    pub fn set_selected_detail_row(&mut self, row: usize) {
        self.selected_detail_row = row;
    }

    pub fn selected_plan_item(&self) -> Option<&GoalPlanSelection> {
        self.selected_plan_item.as_ref()
    }

    pub fn set_selected_plan_item(&mut self, item: Option<GoalPlanSelection>) {
        self.selected_plan_item = item;
    }

    pub fn is_step_expanded(&self, step_id: &str) -> bool {
        self.expanded_step_ids.contains(step_id)
    }

    pub fn set_step_expanded(&mut self, step_id: impl Into<String>, expanded: bool) {
        let step_id = step_id.into();
        if expanded {
            self.expanded_step_ids.insert(step_id);
        } else {
            self.expanded_step_ids.remove(&step_id);
        }
    }

    pub fn toggle_step_expanded(&mut self, step_id: impl Into<String>) -> bool {
        let step_id = step_id.into();
        if self.expanded_step_ids.remove(&step_id) {
            false
        } else {
            self.expanded_step_ids.insert(step_id);
            true
        }
    }
}

impl Default for GoalWorkspacePane {
    fn default() -> Self {
        Self::Plan
    }
}
