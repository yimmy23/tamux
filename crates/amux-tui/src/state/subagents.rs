#![allow(dead_code)]

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubAgentEditorField {
    Name,
    Provider,
    Model,
    ReasoningEffort,
    Role,
    SystemPrompt,
    Save,
    Cancel,
}

impl SubAgentEditorField {
    pub const ALL: [SubAgentEditorField; 8] = [
        SubAgentEditorField::Name,
        SubAgentEditorField::Provider,
        SubAgentEditorField::Model,
        SubAgentEditorField::ReasoningEffort,
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
        Self::ALL[if index == 0 {
            Self::ALL.len().saturating_sub(1)
        } else {
            index - 1
        }]
    }

    pub fn next(self) -> Self {
        let index = Self::ALL
            .iter()
            .position(|field| *field == self)
            .unwrap_or(0);
        Self::ALL[(index + 1) % Self::ALL.len()]
    }
}

#[derive(Debug, Clone, Copy)]
pub struct SubAgentRolePreset {
    pub id: &'static str,
    pub label: &'static str,
    pub system_prompt: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RolePickerChoiceKind {
    Preset,
    Persona,
}

#[derive(Debug, Clone, Copy)]
pub struct RolePickerChoice {
    pub id: &'static str,
    pub label: &'static str,
    pub kind: RolePickerChoiceKind,
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

pub const BUILTIN_PERSONA_ROLE_CHOICES: &[RolePickerChoice] = &[
    RolePickerChoice {
        id: amux_protocol::AGENT_ID_SWAROG,
        label: "Svarog",
        kind: RolePickerChoiceKind::Persona,
    },
    RolePickerChoice {
        id: amux_protocol::AGENT_ID_RAROG,
        label: "Rarog",
        kind: RolePickerChoiceKind::Persona,
    },
    RolePickerChoice {
        id: "weles",
        label: "Weles",
        kind: RolePickerChoiceKind::Persona,
    },
    RolePickerChoice {
        id: "swarozyc",
        label: "Swarozyc",
        kind: RolePickerChoiceKind::Persona,
    },
    RolePickerChoice {
        id: "radogost",
        label: "Radogost",
        kind: RolePickerChoiceKind::Persona,
    },
    RolePickerChoice {
        id: "domowoj",
        label: "Domowoj",
        kind: RolePickerChoiceKind::Persona,
    },
    RolePickerChoice {
        id: "swietowit",
        label: "Swietowit",
        kind: RolePickerChoiceKind::Persona,
    },
    RolePickerChoice {
        id: "perun",
        label: "Perun",
        kind: RolePickerChoiceKind::Persona,
    },
    RolePickerChoice {
        id: "mokosh",
        label: "Mokosh",
        kind: RolePickerChoiceKind::Persona,
    },
    RolePickerChoice {
        id: "dazhbog",
        label: "Dazhbog",
        kind: RolePickerChoiceKind::Persona,
    },
    RolePickerChoice {
        id: "rod",
        label: "Rod",
        kind: RolePickerChoiceKind::Persona,
    },
];

pub fn role_picker_custom_index() -> usize {
    SUBAGENT_ROLE_PRESETS.len() + BUILTIN_PERSONA_ROLE_CHOICES.len()
}

pub fn role_picker_item_count() -> usize {
    role_picker_custom_index() + 1
}

pub fn role_picker_choice(index: usize) -> Option<RolePickerChoice> {
    if let Some(preset) = SUBAGENT_ROLE_PRESETS.get(index) {
        return Some(RolePickerChoice {
            id: preset.id,
            label: preset.label,
            kind: RolePickerChoiceKind::Preset,
        });
    }
    BUILTIN_PERSONA_ROLE_CHOICES
        .get(index.saturating_sub(SUBAGENT_ROLE_PRESETS.len()))
        .copied()
}

pub fn role_picker_index_for_id(id: &str) -> Option<usize> {
    let normalized = id.trim();
    SUBAGENT_ROLE_PRESETS
        .iter()
        .position(|preset| preset.id.eq_ignore_ascii_case(normalized))
        .or_else(|| {
            BUILTIN_PERSONA_ROLE_CHOICES
                .iter()
                .position(|choice| choice.id.eq_ignore_ascii_case(normalized))
                .map(|index| SUBAGENT_ROLE_PRESETS.len() + index)
        })
}

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
    pub builtin: bool,
    pub immutable_identity: bool,
    pub disable_allowed: bool,
    pub delete_allowed: bool,
    pub protected_reason: Option<String>,
    pub reasoning_effort: Option<String>,
    pub raw_json: Option<serde_json::Value>,
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
            builtin: false,
            immutable_identity: false,
            disable_allowed: true,
            delete_allowed: true,
            protected_reason: None,
            reasoning_effort: None,
            raw_json: None,
            field: SubAgentEditorField::Name,
            previous_role_preset: None,
        }
    }

    pub fn identity_is_mutable(&self) -> bool {
        self.id.is_none() || !self.immutable_identity
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
    pub builtin: bool,
    pub immutable_identity: bool,
    pub disable_allowed: bool,
    pub delete_allowed: bool,
    pub protected_reason: Option<String>,
    pub reasoning_effort: Option<String>,
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
