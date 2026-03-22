#![allow(dead_code)]

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubAgentEditorField {
    Name,
    Provider,
    Model,
    Role,
    SystemPrompt,
    Save,
    Cancel,
}

impl SubAgentEditorField {
    pub const ALL: [SubAgentEditorField; 7] = [
        SubAgentEditorField::Name,
        SubAgentEditorField::Provider,
        SubAgentEditorField::Model,
        SubAgentEditorField::Role,
        SubAgentEditorField::SystemPrompt,
        SubAgentEditorField::Save,
        SubAgentEditorField::Cancel,
    ];

    pub fn prev(self) -> Self {
        let index = Self::ALL
            .iter()
            .position(|field| *field == self)
            .unwrap_or(0);
        Self::ALL[index.saturating_sub(1)]
    }

    pub fn next(self) -> Self {
        let index = Self::ALL
            .iter()
            .position(|field| *field == self)
            .unwrap_or(0);
        Self::ALL[(index + 1).min(Self::ALL.len().saturating_sub(1))]
    }
}

#[derive(Debug, Clone, Copy)]
pub struct SubAgentRolePreset {
    pub id: &'static str,
    pub label: &'static str,
    pub system_prompt: &'static str,
}

pub const SUBAGENT_ROLE_PRESETS: &[SubAgentRolePreset] = &[
    SubAgentRolePreset {
        id: "code_review",
        label: "Code Review",
        system_prompt: "You are a code review specialist. Focus on correctness, regressions, security, edge cases, missing tests, and actionable fixes. Be concise and precise.",
    },
    SubAgentRolePreset {
        id: "research",
        label: "Research",
        system_prompt: "You are a research specialist. Gather relevant code and runtime context, compare options, identify constraints, and return clear conclusions with supporting evidence.",
    },
    SubAgentRolePreset {
        id: "testing",
        label: "Testing",
        system_prompt: "You are a testing specialist. Design focused verification, find reproducible failure cases, validate fixes, and call out remaining risks or missing coverage.",
    },
    SubAgentRolePreset {
        id: "planning",
        label: "Planning",
        system_prompt: "You are a planning specialist. Break work into durable, ordered steps with clear dependencies, acceptance criteria, and realistic implementation boundaries.",
    },
    SubAgentRolePreset {
        id: "documentation",
        label: "Documentation",
        system_prompt: "You are a documentation specialist. Produce clear developer-facing docs, explain behavior accurately, and keep examples aligned with the current implementation.",
    },
    SubAgentRolePreset {
        id: "refactoring",
        label: "Refactoring",
        system_prompt: "You are a refactoring specialist. Improve structure and maintainability without changing behavior, preserve intent, and keep edits scoped and defensible.",
    },
];

#[derive(Debug, Clone)]
pub struct SubAgentEditorState {
    pub id: Option<String>,
    pub created_at: u64,
    pub name: String,
    pub provider: String,
    pub model: String,
    pub role: String,
    pub system_prompt: String,
    pub enabled: bool,
    pub field: SubAgentEditorField,
    pub previous_role_preset: Option<String>,
}

impl SubAgentEditorState {
    pub fn new(id: Option<String>, created_at: u64, provider: String, model: String) -> Self {
        Self {
            id,
            created_at,
            name: String::new(),
            provider,
            model,
            role: String::new(),
            system_prompt: String::new(),
            enabled: true,
            field: SubAgentEditorField::Name,
            previous_role_preset: None,
        }
    }

    pub fn role_preset_index(&self) -> Option<usize> {
        SUBAGENT_ROLE_PRESETS
            .iter()
            .position(|preset| preset.id == self.role)
    }

    pub fn apply_role_preset_by_index(&mut self, index: usize) {
        if let Some(preset) = SUBAGENT_ROLE_PRESETS.get(index) {
            let previous_prompt = self
                .previous_role_preset
                .as_deref()
                .and_then(find_role_preset)
                .map(|preset| preset.system_prompt)
                .unwrap_or("");
            if self.system_prompt.trim().is_empty() || self.system_prompt == previous_prompt {
                self.system_prompt = preset.system_prompt.to_string();
            }
            self.role = preset.id.to_string();
            self.previous_role_preset = Some(preset.id.to_string());
        }
    }
}

pub fn find_role_preset(id: &str) -> Option<&'static SubAgentRolePreset> {
    SUBAGENT_ROLE_PRESETS.iter().find(|preset| preset.id == id)
}

/// TUI-side sub-agent entry mirroring the daemon's SubAgentDefinition.
#[derive(Debug, Clone)]
pub struct SubAgentEntry {
    pub id: String,
    pub name: String,
    pub provider: String,
    pub model: String,
    pub role: Option<String>,
    pub enabled: bool,
    pub raw_json: Option<serde_json::Value>,
}

/// TUI state for the Sub-Agents settings tab.
pub struct SubAgentsState {
    pub entries: Vec<SubAgentEntry>,
    pub selected: usize,
    pub editing: Option<String>,
    pub actions_focused: bool,
    pub action_cursor: usize,
    pub editor: Option<SubAgentEditorState>,
}

impl SubAgentsState {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            selected: 0,
            editing: None,
            actions_focused: false,
            action_cursor: 0,
            editor: None,
        }
    }
}

#[derive(Debug, Clone)]
pub enum SubAgentsAction {
    ListReceived(Vec<SubAgentEntry>),
    Added(SubAgentEntry),
    Removed(String),
    Updated(SubAgentEntry),
    Select(usize),
    StartEdit(String),
    CancelEdit,
    ToggleEnabled(String),
}

impl SubAgentsState {
    pub fn reduce(&mut self, action: SubAgentsAction) {
        match action {
            SubAgentsAction::ListReceived(entries) => {
                self.entries = entries;
                if self.selected >= self.entries.len() {
                    self.selected = self.entries.len().saturating_sub(1);
                }
                if self.entries.is_empty() {
                    self.actions_focused = false;
                    self.action_cursor = 0;
                }
            }
            SubAgentsAction::Added(entry) => {
                self.entries.push(entry);
                self.selected = self.entries.len().saturating_sub(1);
                self.actions_focused = false;
            }
            SubAgentsAction::Removed(id) => {
                self.entries.retain(|e| e.id != id);
                if self.selected >= self.entries.len() {
                    self.selected = self.entries.len().saturating_sub(1);
                }
                if self.entries.is_empty() {
                    self.actions_focused = false;
                    self.action_cursor = 0;
                }
            }
            SubAgentsAction::Updated(entry) => {
                if let Some(existing) = self.entries.iter_mut().find(|e| e.id == entry.id) {
                    *existing = entry;
                }
            }
            SubAgentsAction::Select(idx) => {
                if idx < self.entries.len() {
                    self.selected = idx;
                }
            }
            SubAgentsAction::StartEdit(id) => {
                self.editing = Some(id);
            }
            SubAgentsAction::CancelEdit => {
                self.editing = None;
            }
            SubAgentsAction::ToggleEnabled(id) => {
                if let Some(entry) = self.entries.iter_mut().find(|e| e.id == id) {
                    entry.enabled = !entry.enabled;
                }
            }
        }
    }
}
