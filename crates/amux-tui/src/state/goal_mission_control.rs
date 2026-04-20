#![allow(dead_code)]

use crate::state::task::GoalAgentAssignment;

const MAIN_AGENT_ROLE_ID: &str = amux_protocol::AGENT_ID_SWAROG;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GoalMissionControlState {
    pub prompt_text: String,
    pub main_assignment: GoalAgentAssignment,
    pub role_assignments: Vec<GoalAgentAssignment>,
    pub preset_source_label: String,
    pub save_as_default_pending: bool,
}

impl GoalMissionControlState {
    pub fn new() -> Self {
        Self {
            prompt_text: String::new(),
            main_assignment: GoalAgentAssignment::default(),
            role_assignments: Vec::new(),
            preset_source_label: "Main agent inheritance".to_string(),
            save_as_default_pending: false,
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
}

impl Default for GoalMissionControlState {
    fn default() -> Self {
        Self::new()
    }
}
