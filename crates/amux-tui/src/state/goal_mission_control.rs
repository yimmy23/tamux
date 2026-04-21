#![allow(dead_code)]

use crate::state::task::GoalAgentAssignment;

const MAIN_AGENT_ROLE_ID: &str = amux_protocol::AGENT_ID_SWAROG;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeAssignmentApplyMode {
    NextTurn,
    ReassignActiveStep,
    RestartActiveStep,
}

impl RuntimeAssignmentApplyMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::NextTurn => "Apply Next Turn",
            Self::ReassignActiveStep => "Reassign Active Step",
            Self::RestartActiveStep => "Restart Active Step",
        }
    }

    pub fn roster_status_label(self) -> &'static str {
        match self {
            Self::NextTurn => "pending next turn",
            Self::ReassignActiveStep => "pending active-step reassignment",
            Self::RestartActiveStep => "pending active-step restart",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeAssignmentEditField {
    Provider,
    Model,
    ReasoningEffort,
    Role,
    Enabled,
    InheritFromMain,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeAssignmentEditRequest {
    pub row_index: usize,
    pub field: RuntimeAssignmentEditField,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingRuntimeAssignmentChange {
    pub goal_run_id: String,
    pub row_index: usize,
    pub next_assignment: GoalAgentAssignment,
    pub apply_mode: RuntimeAssignmentApplyMode,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GoalMissionControlState {
    pub prompt_text: String,
    pub main_assignment: GoalAgentAssignment,
    pub role_assignments: Vec<GoalAgentAssignment>,
    pub preset_source_label: String,
    pub save_as_default_pending: bool,
    pub runtime_goal_run_id: Option<String>,
    pub pending_role_assignments: Option<Vec<GoalAgentAssignment>>,
    pub pending_runtime_apply_modes: Vec<Option<RuntimeAssignmentApplyMode>>,
    pub selected_runtime_assignment_index: usize,
    pub active_runtime_assignment_index: Option<usize>,
    pub runtime_roster_uses_fallback: bool,
    pub pending_runtime_edit: Option<RuntimeAssignmentEditRequest>,
    pub pending_runtime_change: Option<PendingRuntimeAssignmentChange>,
}

impl GoalMissionControlState {
    pub fn new() -> Self {
        Self {
            prompt_text: String::new(),
            main_assignment: GoalAgentAssignment::default(),
            role_assignments: Vec::new(),
            preset_source_label: "Main agent inheritance".to_string(),
            save_as_default_pending: false,
            runtime_goal_run_id: None,
            pending_role_assignments: None,
            pending_runtime_apply_modes: Vec::new(),
            selected_runtime_assignment_index: 0,
            active_runtime_assignment_index: None,
            runtime_roster_uses_fallback: false,
            pending_runtime_edit: None,
            pending_runtime_change: None,
        }
    }

    pub fn from_main_assignment(
        main_assignment: GoalAgentAssignment,
        role_assignments: Vec<GoalAgentAssignment>,
        preset_source_label: impl Into<String>,
    ) -> Self {
        Self {
            prompt_text: String::new(),
            main_assignment,
            role_assignments,
            preset_source_label: preset_source_label.into(),
            save_as_default_pending: false,
            runtime_goal_run_id: None,
            pending_role_assignments: None,
            pending_runtime_apply_modes: Vec::new(),
            selected_runtime_assignment_index: 0,
            active_runtime_assignment_index: None,
            runtime_roster_uses_fallback: false,
            pending_runtime_edit: None,
            pending_runtime_change: None,
        }
    }

    pub fn from_goal_snapshot(
        snapshot: Vec<GoalAgentAssignment>,
        fallback_main_assignment: GoalAgentAssignment,
        preset_source_label: impl Into<String>,
    ) -> Self {
        let main_assignment = snapshot
            .iter()
            .find(|assignment| assignment.role_id == MAIN_AGENT_ROLE_ID)
            .cloned()
            .or_else(|| snapshot.first().cloned())
            .unwrap_or(fallback_main_assignment.clone());
        let role_assignments = if snapshot.is_empty() {
            vec![fallback_main_assignment]
        } else {
            snapshot
        };

        Self::from_main_assignment(main_assignment, role_assignments, preset_source_label)
    }

    pub fn prompt_text(&self) -> &str {
        &self.prompt_text
    }

    pub fn set_prompt_text(&mut self, prompt_text: impl Into<String>) {
        self.prompt_text = prompt_text.into();
    }

    pub fn main_provider(&self) -> &str {
        self.main_assignment.provider.as_str()
    }

    pub fn main_model(&self) -> &str {
        self.main_assignment.model.as_str()
    }

    pub fn main_reasoning_effort(&self) -> Option<&str> {
        self.main_assignment.reasoning_effort.as_deref()
    }

    pub fn set_save_as_default_pending(&mut self, pending: bool) {
        self.save_as_default_pending = pending;
    }

    pub fn toggle_save_as_default_pending(&mut self) {
        self.save_as_default_pending = !self.save_as_default_pending;
    }

    pub fn runtime_mode(&self) -> bool {
        self.runtime_goal_run_id.is_some()
    }

    pub fn display_role_assignments(&self) -> &[GoalAgentAssignment] {
        self.pending_role_assignments
            .as_deref()
            .unwrap_or(self.role_assignments.as_slice())
    }

    pub fn selected_runtime_assignment(&self) -> Option<&GoalAgentAssignment> {
        self.display_role_assignments()
            .get(self.selected_runtime_assignment_index)
    }

    pub fn configure_runtime_assignments(
        &mut self,
        goal_run_id: impl Into<String>,
        assignments: Vec<GoalAgentAssignment>,
        active_runtime_assignment_index: Option<usize>,
        runtime_roster_uses_fallback: bool,
    ) {
        self.runtime_goal_run_id = Some(goal_run_id.into());
        self.role_assignments = assignments;
        self.pending_runtime_apply_modes = vec![None; self.role_assignments.len()];
        self.pending_role_assignments = None;
        self.active_runtime_assignment_index =
            active_runtime_assignment_index.filter(|index| *index < self.role_assignments.len());
        self.selected_runtime_assignment_index = self
            .active_runtime_assignment_index
            .unwrap_or(0)
            .min(self.role_assignments.len().saturating_sub(1));
        self.runtime_roster_uses_fallback = runtime_roster_uses_fallback;
        self.pending_runtime_edit = None;
        self.pending_runtime_change = None;
        self.refresh_main_assignment_from_display();
    }

    pub fn clear_runtime_state(&mut self) {
        self.runtime_goal_run_id = None;
        self.pending_role_assignments = None;
        self.pending_runtime_apply_modes.clear();
        self.selected_runtime_assignment_index = 0;
        self.active_runtime_assignment_index = None;
        self.runtime_roster_uses_fallback = false;
        self.pending_runtime_edit = None;
        self.pending_runtime_change = None;
    }

    pub fn set_selected_runtime_assignment_index(&mut self, index: usize) {
        let max_index = self.display_role_assignments().len().saturating_sub(1);
        self.selected_runtime_assignment_index = index.min(max_index);
    }

    pub fn cycle_selected_runtime_assignment(&mut self, delta: i32) -> bool {
        let len = self.display_role_assignments().len();
        if len <= 1 {
            return false;
        }
        let current = self.selected_runtime_assignment_index.min(len - 1);
        let next = if delta >= 0 {
            (current + delta as usize).min(len - 1)
        } else {
            current.saturating_sub((-delta) as usize)
        };
        if next == current {
            return false;
        }
        self.selected_runtime_assignment_index = next;
        true
    }

    pub fn append_preflight_assignment(&mut self) {
        let role_id = next_preflight_role_id(&self.role_assignments);
        self.role_assignments.push(GoalAgentAssignment {
            role_id,
            enabled: true,
            provider: self.main_assignment.provider.clone(),
            model: self.main_assignment.model.clone(),
            reasoning_effort: self.main_assignment.reasoning_effort.clone(),
            inherit_from_main: false,
        });
        self.selected_runtime_assignment_index = self.role_assignments.len().saturating_sub(1);
        self.pending_role_assignments = None;
        self.pending_runtime_edit = None;
        self.pending_runtime_change = None;
    }

    pub fn stage_runtime_edit(&mut self, row_index: usize, field: RuntimeAssignmentEditField) {
        self.pending_runtime_edit = Some(RuntimeAssignmentEditRequest { row_index, field });
    }

    pub fn clear_runtime_edit(&mut self) {
        self.pending_runtime_edit = None;
    }

    pub fn stage_runtime_change(
        &mut self,
        goal_run_id: impl Into<String>,
        row_index: usize,
        next_assignment: GoalAgentAssignment,
        apply_mode: RuntimeAssignmentApplyMode,
    ) {
        self.pending_runtime_change = Some(PendingRuntimeAssignmentChange {
            goal_run_id: goal_run_id.into(),
            row_index,
            next_assignment,
            apply_mode,
        });
    }

    pub fn clear_runtime_change(&mut self) {
        self.pending_runtime_change = None;
    }

    pub fn selected_runtime_row_label(&self) -> Option<&str> {
        self.selected_runtime_assignment()
            .map(|assignment| assignment.role_id.as_str())
    }

    pub fn runtime_assignment_status_label(&self, index: usize) -> &'static str {
        if !self.runtime_mode() {
            return "launch setting";
        }
        let has_pending_row = self
            .pending_role_assignments
            .as_ref()
            .and_then(|assignments| assignments.get(index))
            .zip(self.role_assignments.get(index))
            .is_some_and(|(pending, live)| pending != live);
        if has_pending_row {
            return self
                .pending_runtime_apply_modes
                .get(index)
                .and_then(|mode| *mode)
                .unwrap_or(RuntimeAssignmentApplyMode::NextTurn)
                .roster_status_label();
        }
        if self.runtime_roster_uses_fallback {
            "stale / fallback"
        } else {
            "live now"
        }
    }

    pub fn apply_runtime_assignment_change(
        &mut self,
        row_index: usize,
        apply_mode: RuntimeAssignmentApplyMode,
    ) {
        let Some(change) = self.pending_runtime_change.clone() else {
            return;
        };
        let mut assignments = self
            .pending_role_assignments
            .clone()
            .unwrap_or_else(|| self.role_assignments.clone());
        if let Some(slot) = assignments.get_mut(row_index) {
            *slot = change.next_assignment;
        }
        self.pending_role_assignments = Some(assignments);
        if self.pending_runtime_apply_modes.len() < self.role_assignments.len() {
            self.pending_runtime_apply_modes
                .resize(self.role_assignments.len(), None);
        }
        if let Some(slot) = self.pending_runtime_apply_modes.get_mut(row_index) {
            *slot = Some(apply_mode);
        }
        self.pending_runtime_change = None;
        self.pending_runtime_edit = None;
        self.refresh_main_assignment_from_display();
    }

    pub fn update_selected_preflight_assignment(
        &mut self,
        update: impl FnOnce(&mut GoalAgentAssignment),
    ) -> bool {
        let len = self.role_assignments.len();
        if len == 0 {
            return false;
        }
        let index = self.selected_runtime_assignment_index.min(len - 1);
        let Some(assignment) = self.role_assignments.get_mut(index) else {
            return false;
        };
        update(assignment);
        self.refresh_main_assignment_from_display();
        true
    }

    pub fn selected_assignment_matches_active_step(&self) -> bool {
        self.active_runtime_assignment_index == Some(self.selected_runtime_assignment_index)
    }

    fn refresh_main_assignment_from_display(&mut self) {
        if let Some(main_assignment) = self
            .display_role_assignments()
            .iter()
            .find(|assignment| assignment.role_id == MAIN_AGENT_ROLE_ID)
            .cloned()
            .or_else(|| self.display_role_assignments().first().cloned())
        {
            self.main_assignment = main_assignment;
        }
    }
}

impl Default for GoalMissionControlState {
    fn default() -> Self {
        Self::new()
    }
}

fn next_preflight_role_id(assignments: &[GoalAgentAssignment]) -> String {
    for preset in crate::state::subagents::SUBAGENT_ROLE_PRESETS {
        if assignments.iter().all(|assignment| assignment.role_id != preset.id) {
            return preset.id.to_string();
        }
    }

    let mut suffix = assignments.len().saturating_add(1);
    loop {
        let candidate = format!("specialist_{suffix}");
        if assignments
            .iter()
            .all(|assignment| assignment.role_id != candidate)
        {
            return candidate;
        }
        suffix += 1;
    }
}
