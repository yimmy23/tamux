#![allow(dead_code)]

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
}

impl SubAgentsState {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            selected: 0,
            editing: None,
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
            }
            SubAgentsAction::Added(entry) => {
                self.entries.push(entry);
            }
            SubAgentsAction::Removed(id) => {
                self.entries.retain(|e| e.id != id);
                if self.selected >= self.entries.len() {
                    self.selected = self.entries.len().saturating_sub(1);
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
