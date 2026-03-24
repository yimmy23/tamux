use std::collections::HashSet;

const MAX_AUDIT_ENTRIES: usize = 500;

// ── View Models ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct AuditEntryVm {
    pub id: String,
    pub timestamp: u64,
    pub action_type: String,
    pub summary: String,
    pub explanation: Option<String>,
    pub confidence: Option<f64>,
    pub confidence_band: Option<String>,
    pub causal_trace_id: Option<String>,
    pub thread_id: Option<String>,
    pub dismissed: bool,
}

#[derive(Debug, Clone)]
pub struct EscalationVm {
    pub thread_id: String,
    pub from_level: String,
    pub to_level: String,
    pub reason: String,
    pub attempts: u32,
    pub audit_id: Option<String>,
}

// ── Filters ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimeRange {
    LastHour,
    Today,
    ThisWeek,
    AllTime,
}

impl Default for TimeRange {
    fn default() -> Self {
        Self::Today
    }
}

fn default_type_filter() -> HashSet<String> {
    ["heartbeat", "tool", "escalation", "skill", "subagent"]
        .iter()
        .map(|s| (*s).to_string())
        .collect()
}

// ── Action ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum AuditAction {
    EntryReceived(AuditEntryVm),
    EscalationUpdate(EscalationVm),
    SetTypeFilter(HashSet<String>),
    SetTimeFilter(TimeRange),
    SelectEntry(Option<String>),
    ToggleExpand(String),
    DismissEntry(String),
    ScrollUp,
    ScrollDown,
    ClearAll,
}

// ── State ────────────────────────────────────────────────────────────────────

pub struct AuditState {
    entries: Vec<AuditEntryVm>,
    selected_index: usize,
    expanded_entry: Option<String>,
    type_filter: HashSet<String>,
    time_filter: TimeRange,
    current_escalation: Option<EscalationVm>,
}

impl AuditState {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            selected_index: 0,
            expanded_entry: None,
            type_filter: default_type_filter(),
            time_filter: TimeRange::Today,
            current_escalation: None,
        }
    }

    pub fn reduce(&mut self, action: AuditAction) {
        match action {
            AuditAction::EntryReceived(entry) => {
                self.entries.insert(0, entry);
                self.entries.truncate(MAX_AUDIT_ENTRIES);
            }
            AuditAction::EscalationUpdate(escalation) => {
                self.current_escalation = Some(escalation);
            }
            AuditAction::SetTypeFilter(filter) => {
                self.type_filter = filter;
            }
            AuditAction::SetTimeFilter(range) => {
                self.time_filter = range;
            }
            AuditAction::SelectEntry(Some(id)) => {
                if let Some(pos) = self.filtered_entries().iter().position(|e| e.id == id) {
                    self.selected_index = pos;
                }
            }
            AuditAction::SelectEntry(None) => {
                self.selected_index = 0;
            }
            AuditAction::ToggleExpand(id) => {
                if self.expanded_entry.as_deref() == Some(&id) {
                    self.expanded_entry = None;
                } else {
                    self.expanded_entry = Some(id);
                }
            }
            AuditAction::DismissEntry(id) => {
                if let Some(entry) = self.entries.iter_mut().find(|e| e.id == id) {
                    entry.dismissed = true;
                }
            }
            AuditAction::ScrollUp => {
                if self.selected_index > 0 {
                    self.selected_index -= 1;
                }
            }
            AuditAction::ScrollDown => {
                let max = self.filtered_entries().len().saturating_sub(1);
                if self.selected_index < max {
                    self.selected_index += 1;
                }
            }
            AuditAction::ClearAll => {
                self.entries.clear();
                self.selected_index = 0;
            }
        }
    }

    // ── Accessors ────────────────────────────────────────────────────────────

    pub fn entries(&self) -> &[AuditEntryVm] {
        &self.entries
    }

    pub fn filtered_entries(&self) -> Vec<&AuditEntryVm> {
        self.entries
            .iter()
            .filter(|e| self.type_filter.contains(&e.action_type))
            .filter(|e| self.passes_time_filter(e.timestamp))
            .collect()
    }

    pub fn selected_index(&self) -> usize {
        self.selected_index
    }

    pub fn expanded_entry(&self) -> Option<&str> {
        self.expanded_entry.as_deref()
    }

    pub fn current_escalation(&self) -> Option<&EscalationVm> {
        self.current_escalation.as_ref()
    }

    pub fn type_filter(&self) -> &HashSet<String> {
        &self.type_filter
    }

    pub fn time_filter(&self) -> &TimeRange {
        &self.time_filter
    }

    /// Get the ID of the currently selected entry (if any).
    pub fn selected_entry_id(&self) -> Option<&str> {
        let filtered = self.filtered_entries();
        filtered
            .get(self.selected_index)
            .map(|e| e.id.as_str())
    }

    // ── Helpers ──────────────────────────────────────────────────────────────

    fn passes_time_filter(&self, timestamp: u64) -> bool {
        match self.time_filter {
            TimeRange::AllTime => true,
            TimeRange::LastHour => {
                let now = current_epoch_secs();
                timestamp >= now.saturating_sub(3600)
            }
            TimeRange::Today => {
                let now = current_epoch_secs();
                timestamp >= now.saturating_sub(86400)
            }
            TimeRange::ThisWeek => {
                let now = current_epoch_secs();
                timestamp >= now.saturating_sub(604800)
            }
        }
    }
}

impl Default for AuditState {
    fn default() -> Self {
        Self::new()
    }
}

fn current_epoch_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_entry(id: &str, action_type: &str) -> AuditEntryVm {
        AuditEntryVm {
            id: id.into(),
            timestamp: current_epoch_secs(),
            action_type: action_type.into(),
            summary: format!("Summary for {}", id),
            explanation: None,
            confidence: None,
            confidence_band: None,
            causal_trace_id: None,
            thread_id: None,
            dismissed: false,
        }
    }

    fn make_entry_with_ts(id: &str, action_type: &str, timestamp: u64) -> AuditEntryVm {
        AuditEntryVm {
            id: id.into(),
            timestamp,
            action_type: action_type.into(),
            summary: format!("Summary for {}", id),
            explanation: None,
            confidence: None,
            confidence_band: None,
            causal_trace_id: None,
            thread_id: None,
            dismissed: false,
        }
    }

    #[test]
    fn entry_received_adds_entry_to_front() {
        let mut state = AuditState::new();
        state.reduce(AuditAction::EntryReceived(make_entry("a1", "heartbeat")));
        state.reduce(AuditAction::EntryReceived(make_entry("a2", "tool")));
        assert_eq!(state.entries().len(), 2);
        assert_eq!(state.entries()[0].id, "a2");
        assert_eq!(state.entries()[1].id, "a1");
    }

    #[test]
    fn entries_capped_at_max() {
        let mut state = AuditState::new();
        for i in 0..=MAX_AUDIT_ENTRIES {
            state.reduce(AuditAction::EntryReceived(make_entry(
                &format!("a{}", i),
                "heartbeat",
            )));
        }
        assert_eq!(state.entries().len(), MAX_AUDIT_ENTRIES);
        // newest first
        assert_eq!(
            state.entries()[0].id,
            format!("a{}", MAX_AUDIT_ENTRIES)
        );
    }

    #[test]
    fn set_type_filter_updates_filter_and_filtered_entries() {
        let mut state = AuditState::new();
        state.reduce(AuditAction::EntryReceived(make_entry("a1", "heartbeat")));
        state.reduce(AuditAction::EntryReceived(make_entry("a2", "tool")));
        state.reduce(AuditAction::EntryReceived(make_entry("a3", "escalation")));

        // Filter to only "tool"
        let mut filter = HashSet::new();
        filter.insert("tool".to_string());
        state.reduce(AuditAction::SetTypeFilter(filter));

        let filtered = state.filtered_entries();
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].id, "a2");
    }

    #[test]
    fn select_entry_sets_selected_index() {
        let mut state = AuditState::new();
        state.reduce(AuditAction::EntryReceived(make_entry("a1", "heartbeat")));
        state.reduce(AuditAction::EntryReceived(make_entry("a2", "heartbeat")));
        state.reduce(AuditAction::EntryReceived(make_entry("a3", "heartbeat")));

        // Select a2, which is at filtered_entries index 1 (a3 is at 0, a2 at 1, a1 at 2)
        state.reduce(AuditAction::SelectEntry(Some("a2".into())));
        assert_eq!(state.selected_index(), 1);
    }

    #[test]
    fn toggle_expand_toggles_between_some_and_none() {
        let mut state = AuditState::new();

        state.reduce(AuditAction::ToggleExpand("a1".into()));
        assert_eq!(state.expanded_entry(), Some("a1"));

        // Toggle same id -> should be None
        state.reduce(AuditAction::ToggleExpand("a1".into()));
        assert_eq!(state.expanded_entry(), None);

        // Toggle different id
        state.reduce(AuditAction::ToggleExpand("a2".into()));
        assert_eq!(state.expanded_entry(), Some("a2"));
    }

    #[test]
    fn clear_all_resets_entries() {
        let mut state = AuditState::new();
        state.reduce(AuditAction::EntryReceived(make_entry("a1", "heartbeat")));
        state.reduce(AuditAction::EntryReceived(make_entry("a2", "tool")));
        state.reduce(AuditAction::ClearAll);
        assert!(state.entries().is_empty());
        assert_eq!(state.selected_index(), 0);
    }

    #[test]
    fn filtered_entries_with_time_filter_last_hour() {
        let mut state = AuditState::new();
        let now = current_epoch_secs();

        // Recent entry (within last hour)
        state.reduce(AuditAction::EntryReceived(make_entry_with_ts(
            "recent",
            "heartbeat",
            now - 60,
        )));
        // Old entry (2 hours ago)
        state.reduce(AuditAction::EntryReceived(make_entry_with_ts(
            "old",
            "heartbeat",
            now - 7200,
        )));

        state.reduce(AuditAction::SetTimeFilter(TimeRange::LastHour));
        let filtered = state.filtered_entries();
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].id, "recent");
    }

    #[test]
    fn escalation_update_stores_current_escalation() {
        let mut state = AuditState::new();
        assert!(state.current_escalation().is_none());

        state.reduce(AuditAction::EscalationUpdate(EscalationVm {
            thread_id: "t1".into(),
            from_level: "L0".into(),
            to_level: "L1".into(),
            reason: "Self-correction failed".into(),
            attempts: 3,
            audit_id: Some("audit-1".into()),
        }));

        let esc = state.current_escalation().unwrap();
        assert_eq!(esc.from_level, "L0");
        assert_eq!(esc.to_level, "L1");
        assert_eq!(esc.reason, "Self-correction failed");
        assert_eq!(esc.attempts, 3);
    }

    #[test]
    fn scroll_up_decrements_selected_index() {
        let mut state = AuditState::new();
        state.reduce(AuditAction::EntryReceived(make_entry("a1", "heartbeat")));
        state.reduce(AuditAction::EntryReceived(make_entry("a2", "heartbeat")));
        state.selected_index = 1;

        state.reduce(AuditAction::ScrollUp);
        assert_eq!(state.selected_index(), 0);

        // Can't go below 0
        state.reduce(AuditAction::ScrollUp);
        assert_eq!(state.selected_index(), 0);
    }

    #[test]
    fn scroll_down_increments_selected_index() {
        let mut state = AuditState::new();
        state.reduce(AuditAction::EntryReceived(make_entry("a1", "heartbeat")));
        state.reduce(AuditAction::EntryReceived(make_entry("a2", "heartbeat")));

        state.reduce(AuditAction::ScrollDown);
        assert_eq!(state.selected_index(), 1);

        // Can't go past end
        state.reduce(AuditAction::ScrollDown);
        assert_eq!(state.selected_index(), 1);
    }

    #[test]
    fn dismiss_entry_marks_entry_dismissed() {
        let mut state = AuditState::new();
        state.reduce(AuditAction::EntryReceived(make_entry("a1", "heartbeat")));
        assert!(!state.entries()[0].dismissed);

        state.reduce(AuditAction::DismissEntry("a1".into()));
        assert!(state.entries()[0].dismissed);
    }

    #[test]
    fn selected_entry_id_returns_correct_id() {
        let mut state = AuditState::new();
        state.reduce(AuditAction::EntryReceived(make_entry("a1", "heartbeat")));
        state.reduce(AuditAction::EntryReceived(make_entry("a2", "heartbeat")));
        // a2 is at index 0, a1 at index 1
        assert_eq!(state.selected_entry_id(), Some("a2"));

        state.reduce(AuditAction::ScrollDown);
        assert_eq!(state.selected_entry_id(), Some("a1"));
    }

    #[test]
    fn default_type_filter_has_all_five_types() {
        let state = AuditState::new();
        let filter = state.type_filter();
        assert!(filter.contains("heartbeat"));
        assert!(filter.contains("tool"));
        assert!(filter.contains("escalation"));
        assert!(filter.contains("skill"));
        assert!(filter.contains("subagent"));
        assert_eq!(filter.len(), 5);
    }

    #[test]
    fn default_time_filter_is_today() {
        let state = AuditState::new();
        assert_eq!(*state.time_filter(), TimeRange::Today);
    }
}
