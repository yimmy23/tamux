#![allow(dead_code)]

use std::collections::BTreeSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GoalWorkspacePane {
    Plan,
    Timeline,
    Details,
    CommandBar,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GoalWorkspaceMode {
    Goal,
    Files,
    Progress,
    Usage,
    ActiveAgent,
    Threads,
    NeedsAttention,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GoalPlanSelection {
    PromptToggle,
    MainThread { thread_id: String },
    Step { step_id: String },
    Todo { step_id: String, todo_id: String },
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GoalWorkspaceState {
    mode: GoalWorkspaceMode,
    focused_pane: GoalWorkspacePane,
    plan_scroll: usize,
    timeline_scroll: usize,
    detail_scroll: usize,
    selected_plan_row: usize,
    expanded_step_ids: BTreeSet<String>,
    prompt_expanded: bool,
    selected_timeline_row: usize,
    selected_detail_row: usize,
    selected_plan_item: Option<GoalPlanSelection>,
}

impl GoalWorkspaceState {
    pub fn new() -> Self {
        Self {
            mode: GoalWorkspaceMode::Goal,
            focused_pane: GoalWorkspacePane::Plan,
            plan_scroll: 0,
            timeline_scroll: 0,
            detail_scroll: 0,
            selected_plan_row: 0,
            expanded_step_ids: BTreeSet::new(),
            prompt_expanded: false,
            selected_timeline_row: 0,
            selected_detail_row: 0,
            selected_plan_item: None,
        }
    }

    pub fn mode(&self) -> GoalWorkspaceMode {
        self.mode
    }

    pub fn set_mode(&mut self, mode: GoalWorkspaceMode) {
        self.mode = mode;
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

    pub fn plan_scroll(&self) -> usize {
        self.plan_scroll
    }

    pub fn set_plan_scroll(&mut self, scroll: usize) {
        self.plan_scroll = scroll;
    }

    pub fn timeline_scroll(&self) -> usize {
        self.timeline_scroll
    }

    pub fn set_timeline_scroll(&mut self, scroll: usize) {
        self.timeline_scroll = scroll;
    }

    pub fn detail_scroll(&self) -> usize {
        self.detail_scroll
    }

    pub fn set_detail_scroll(&mut self, scroll: usize) {
        self.detail_scroll = scroll;
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

    pub fn prompt_expanded(&self) -> bool {
        self.prompt_expanded
    }

    pub fn set_prompt_expanded(&mut self, expanded: bool) {
        self.prompt_expanded = expanded;
    }

    pub fn toggle_prompt_expanded(&mut self) -> bool {
        self.prompt_expanded = !self.prompt_expanded;
        self.prompt_expanded
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

impl Default for GoalWorkspaceMode {
    fn default() -> Self {
        Self::Goal
    }
}
