#![allow(dead_code)]

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModalKind {
    CommandPalette,
    ThreadPicker,
    GoalPicker,
    ProviderPicker,
    ModelPicker,
    OpenAIAuth,
    ErrorViewer,
    ApprovalOverlay,
    ChatActionConfirm,
    Settings,
    EffortPicker,
    ToolsPicker,
    ViewPicker,
    Help,
    WhatsAppLink,
}

#[derive(Debug, Clone)]
pub struct CommandItem {
    pub command: String,
    pub description: String,
}

#[derive(Debug, Clone)]
pub enum ModalAction {
    Push(ModalKind),
    Pop,
    SetQuery(String),
    Navigate(i32), // +1 = down, -1 = up
    Execute,
    FuzzyFilter,
}

pub struct ModalState {
    stack: Vec<ModalKind>,
    command_query: String,
    command_items: Vec<CommandItem>,
    filtered_indices: Vec<usize>,
    picker_cursor: usize,
    /// Override item count for non-command-palette pickers (providers, models, etc.)
    picker_item_count: Option<usize>,
    whatsapp_link: WhatsAppLinkState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WhatsAppLinkPhase {
    Idle,
    Starting,
    AwaitingScan,
    Connected,
    Error,
    Disconnected,
}

#[derive(Debug, Clone)]
pub struct WhatsAppLinkState {
    phase: WhatsAppLinkPhase,
    status_text: String,
    ascii_qr: Option<String>,
    expires_at_ms: Option<u64>,
    phone: Option<String>,
    last_error: Option<String>,
}

impl Default for WhatsAppLinkState {
    fn default() -> Self {
        Self {
            phase: WhatsAppLinkPhase::Idle,
            status_text: "Ready to start WhatsApp device linking".to_string(),
            ascii_qr: None,
            expires_at_ms: None,
            phone: None,
            last_error: None,
        }
    }
}

impl ModalState {
    pub fn new() -> Self {
        let items = default_command_items();
        let filtered = (0..items.len()).collect();
        Self {
            stack: Vec::new(),
            command_query: String::new(),
            command_items: items,
            filtered_indices: filtered,
            picker_cursor: 0,
            picker_item_count: None,
            whatsapp_link: WhatsAppLinkState::default(),
        }
    }

    // Accessors
    pub fn top(&self) -> Option<ModalKind> {
        self.stack.last().copied()
    }
    pub fn is_empty(&self) -> bool {
        self.stack.is_empty()
    }
    pub fn command_query(&self) -> &str {
        &self.command_query
    }
    pub fn command_items(&self) -> &[CommandItem] {
        &self.command_items
    }
    pub fn filtered_items(&self) -> &[usize] {
        &self.filtered_indices
    }
    pub fn picker_cursor(&self) -> usize {
        self.picker_cursor
    }
    pub fn set_picker_item_count(&mut self, count: usize) {
        self.picker_item_count = Some(count);
        self.picker_cursor = if count == 0 {
            0
        } else {
            self.picker_cursor.min(count - 1)
        };
    }
    pub fn whatsapp_link(&self) -> &WhatsAppLinkState {
        &self.whatsapp_link
    }
    pub fn reset_whatsapp_link(&mut self) {
        self.whatsapp_link = WhatsAppLinkState::default();
    }
    pub fn set_whatsapp_link_starting(&mut self) {
        self.whatsapp_link.phase = WhatsAppLinkPhase::Starting;
        self.whatsapp_link.status_text = "Starting WhatsApp linking…".to_string();
        self.whatsapp_link.ascii_qr = None;
        self.whatsapp_link.expires_at_ms = None;
        self.whatsapp_link.phone = None;
        self.whatsapp_link.last_error = None;
    }
    pub fn set_whatsapp_link_status(
        &mut self,
        state: &str,
        phone: Option<String>,
        last_error: Option<String>,
    ) {
        self.whatsapp_link.phone = phone;
        self.whatsapp_link.last_error = last_error;
        match state {
            "starting" => {
                self.whatsapp_link.phase = WhatsAppLinkPhase::Starting;
                self.whatsapp_link.status_text = "Starting WhatsApp linking…".to_string();
                self.whatsapp_link.ascii_qr = None;
                self.whatsapp_link.expires_at_ms = None;
            }
            "qr_ready" | "awaiting_qr" => {
                self.whatsapp_link.phase = WhatsAppLinkPhase::AwaitingScan;
                self.whatsapp_link.status_text =
                    "Scan the QR code in WhatsApp on your phone".to_string();
            }
            "connected" => {
                self.whatsapp_link.phase = WhatsAppLinkPhase::Connected;
                self.whatsapp_link.ascii_qr = None;
                self.whatsapp_link.expires_at_ms = None;
                let phone_display = self.whatsapp_link.phone.as_deref().unwrap_or("device");
                self.whatsapp_link.status_text = format!("Connected: {phone_display}");
            }
            "error" => {
                self.whatsapp_link.phase = WhatsAppLinkPhase::Error;
                self.whatsapp_link.ascii_qr = None;
                self.whatsapp_link.expires_at_ms = None;
                let message = self
                    .whatsapp_link
                    .last_error
                    .as_deref()
                    .unwrap_or("Unknown WhatsApp linking error");
                self.whatsapp_link.status_text = format!("Error: {message}");
            }
            "disconnected" => {
                self.whatsapp_link.phase = WhatsAppLinkPhase::Disconnected;
                self.whatsapp_link.ascii_qr = None;
                self.whatsapp_link.expires_at_ms = None;
                let reason = self
                    .whatsapp_link
                    .last_error
                    .as_deref()
                    .unwrap_or("Disconnected");
                self.whatsapp_link.status_text = format!("Disconnected: {reason}");
            }
            _ => {}
        }
    }
    pub fn set_whatsapp_link_qr(&mut self, ascii_qr: String, expires_at_ms: Option<u64>) {
        self.whatsapp_link.phase = WhatsAppLinkPhase::AwaitingScan;
        self.whatsapp_link.status_text = "Scan the QR code in WhatsApp on your phone".to_string();
        self.whatsapp_link.ascii_qr = Some(ascii_qr);
        self.whatsapp_link.expires_at_ms = expires_at_ms;
        self.whatsapp_link.last_error = None;
    }
    pub fn set_whatsapp_link_connected(&mut self, phone: Option<String>) {
        self.whatsapp_link.phone = phone;
        self.set_whatsapp_link_status("connected", self.whatsapp_link.phone.clone(), None);
    }
    pub fn set_whatsapp_link_error(&mut self, message: String) {
        self.set_whatsapp_link_status("error", self.whatsapp_link.phone.clone(), Some(message));
    }
    pub fn set_whatsapp_link_disconnected(&mut self, reason: Option<String>) {
        let preserved_reason = reason.or_else(|| self.whatsapp_link.last_error.clone());
        self.set_whatsapp_link_status(
            "disconnected",
            self.whatsapp_link.phone.clone(),
            preserved_reason,
        );
    }

    /// Merge plugin commands into the command palette.
    /// Removes any previously added plugin commands, then appends the new ones.
    pub fn set_plugin_commands(&mut self, commands: Vec<CommandItem>) {
        // Remove old plugin commands (marked by command containing '.')
        self.command_items
            .retain(|item| !item.command.contains('.'));
        // Append new plugin commands
        self.command_items.extend(commands);
        // Rebuild filter
        self.filtered_indices = (0..self.command_items.len()).collect();
    }

    pub fn reduce(&mut self, action: ModalAction) {
        match action {
            ModalAction::Push(kind) => {
                self.stack.push(kind);
                self.command_query.clear();
                self.picker_cursor = 0;
                self.picker_item_count = None;
                self.refilter();
            }
            ModalAction::Pop => {
                self.stack.pop();
                self.command_query.clear();
                self.picker_cursor = 0;
                self.refilter();
            }
            ModalAction::SetQuery(query) => {
                self.command_query = query;
                self.refilter();
                self.picker_cursor = 0;
            }
            ModalAction::Navigate(delta) => {
                let len = self
                    .picker_item_count
                    .unwrap_or(self.filtered_indices.len());
                if len == 0 {
                    return;
                }
                if delta > 0 {
                    self.picker_cursor = (self.picker_cursor + delta as usize).min(len - 1);
                } else {
                    self.picker_cursor = self.picker_cursor.saturating_sub((-delta) as usize);
                }
            }
            ModalAction::Execute => {
                // Execution is handled by the app layer — this just marks intent
            }
            ModalAction::FuzzyFilter => {
                self.refilter();
            }
        }
    }

    /// Get the currently selected command (if any)
    pub fn selected_command(&self) -> Option<&CommandItem> {
        self.filtered_indices
            .get(self.picker_cursor)
            .and_then(|&idx| self.command_items.get(idx))
    }

    fn refilter(&mut self) {
        let query = self.command_query.to_lowercase();
        if query.is_empty() {
            self.filtered_indices = (0..self.command_items.len()).collect();
        } else {
            // Strip leading '/' for matching
            let q = query.strip_prefix('/').unwrap_or(&query);
            self.filtered_indices = self
                .command_items
                .iter()
                .enumerate()
                .filter(|(_, item)| {
                    item.command.to_lowercase().contains(q)
                        || item.description.to_lowercase().contains(q)
                })
                .map(|(idx, _)| idx)
                .collect();
        }
    }
}

impl WhatsAppLinkState {
    pub fn phase(&self) -> WhatsAppLinkPhase {
        self.phase
    }
    pub fn status_text(&self) -> &str {
        &self.status_text
    }
    pub fn ascii_qr(&self) -> Option<&str> {
        self.ascii_qr.as_deref()
    }
    pub fn expires_at_ms(&self) -> Option<u64> {
        self.expires_at_ms
    }
    pub fn phone(&self) -> Option<&str> {
        self.phone.as_deref()
    }
    pub fn last_error(&self) -> Option<&str> {
        self.last_error.as_deref()
    }
}

fn default_command_items() -> Vec<CommandItem> {
    vec![
        CommandItem {
            command: "provider".into(),
            description: "Switch LLM backend".into(),
        },
        CommandItem {
            command: "model".into(),
            description: "Switch active model".into(),
        },
        CommandItem {
            command: "tools".into(),
            description: "Toggle tool categories".into(),
        },
        CommandItem {
            command: "effort".into(),
            description: "Set reasoning effort".into(),
        },
        CommandItem {
            command: "thread".into(),
            description: "Pick conversation thread".into(),
        },
        CommandItem {
            command: "new".into(),
            description: "New conversation".into(),
        },
        CommandItem {
            command: "goal".into(),
            description: "Open Goal Runner view".into(),
        },
        CommandItem {
            command: "goals".into(),
            description: "Open Goal Runner view".into(),
        },
        CommandItem {
            command: "conversation".into(),
            description: "Return to conversation view".into(),
        },
        CommandItem {
            command: "view".into(),
            description: "Switch transcript mode".into(),
        },
        CommandItem {
            command: "settings".into(),
            description: "Open settings panel".into(),
        },
        CommandItem {
            command: "prompt".into(),
            description: "Edit system prompt".into(),
        },
        CommandItem {
            command: "attach".into(),
            description: "Attach a file to the message".into(),
        },
        CommandItem {
            command: "quit".into(),
            description: "Exit TUI".into(),
        },
        CommandItem {
            command: "help".into(),
            description: "Show keyboard shortcuts".into(),
        },
        CommandItem {
            command: "explain".into(),
            description: "Explain latest goal-run decision".into(),
        },
        CommandItem {
            command: "diverge".into(),
            description: "Seed divergent session command".into(),
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_and_pop_modal() {
        let mut state = ModalState::new();
        assert!(state.top().is_none());
        state.reduce(ModalAction::Push(ModalKind::CommandPalette));
        assert_eq!(state.top(), Some(ModalKind::CommandPalette));
        state.reduce(ModalAction::Pop);
        assert!(state.top().is_none());
    }

    #[test]
    fn stacked_modals_pop_in_order() {
        let mut state = ModalState::new();
        state.reduce(ModalAction::Push(ModalKind::CommandPalette));
        state.reduce(ModalAction::Push(ModalKind::ProviderPicker));
        assert_eq!(state.top(), Some(ModalKind::ProviderPicker));
        state.reduce(ModalAction::Pop);
        assert_eq!(state.top(), Some(ModalKind::CommandPalette));
    }

    #[test]
    fn fuzzy_filter_narrows_items() {
        let mut state = ModalState::new();
        state.reduce(ModalAction::SetQuery("pro".into()));
        // "provider" and "prompt" should match "pro"
        let filtered_commands: Vec<&str> = state
            .filtered_items()
            .iter()
            .map(|&idx| state.command_items()[idx].command.as_str())
            .collect();
        assert!(filtered_commands.contains(&"provider"));
        assert!(filtered_commands.contains(&"prompt"));
        assert!(!filtered_commands.contains(&"model"));
    }

    #[test]
    fn slash_prefix_stripped_for_matching() {
        let mut state = ModalState::new();
        state.reduce(ModalAction::SetQuery("/mod".into()));
        let filtered_commands: Vec<&str> = state
            .filtered_items()
            .iter()
            .map(|&idx| state.command_items()[idx].command.as_str())
            .collect();
        assert!(filtered_commands.contains(&"model"));
    }

    #[test]
    fn navigation_clamps_to_bounds() {
        let mut state = ModalState::new();
        state.reduce(ModalAction::Navigate(-1));
        assert_eq!(state.picker_cursor(), 0);
        for _ in 0..100 {
            state.reduce(ModalAction::Navigate(1));
        }
        assert!(state.picker_cursor() < state.command_items().len());
    }

    #[test]
    fn selected_command_returns_correct_item() {
        let mut state = ModalState::new();
        state.reduce(ModalAction::Navigate(2));
        let selected = state.selected_command().unwrap();
        assert_eq!(selected.command, "tools");
    }

    #[test]
    fn push_resets_query_and_cursor() {
        let mut state = ModalState::new();
        state.reduce(ModalAction::SetQuery("test".into()));
        state.reduce(ModalAction::Navigate(3));
        state.reduce(ModalAction::Push(ModalKind::CommandPalette));
        assert_eq!(state.command_query(), "");
        assert_eq!(state.picker_cursor(), 0);
    }

    #[test]
    fn empty_filter_shows_all_items() {
        let state = ModalState::new();
        assert_eq!(state.filtered_items().len(), state.command_items().len());
    }

    #[test]
    fn whatsapp_status_maps_to_connected_error_and_disconnected() {
        let mut state = ModalState::new();
        state.set_whatsapp_link_status("qr_ready", None, None);
        assert_eq!(
            state.whatsapp_link().phase(),
            WhatsAppLinkPhase::AwaitingScan
        );
        assert!(state
            .whatsapp_link()
            .status_text()
            .contains("Scan the QR code"));

        state.set_whatsapp_link_status("connected", Some("+12065550123".to_string()), None);
        assert_eq!(state.whatsapp_link().phase(), WhatsAppLinkPhase::Connected);
        assert!(state.whatsapp_link().status_text().contains("Connected"));

        state.set_whatsapp_link_error("pairing failed".to_string());
        assert_eq!(state.whatsapp_link().phase(), WhatsAppLinkPhase::Error);
        assert!(state
            .whatsapp_link()
            .status_text()
            .contains("pairing failed"));

        state.set_whatsapp_link_disconnected(Some("socket closed".to_string()));
        assert_eq!(
            state.whatsapp_link().phase(),
            WhatsAppLinkPhase::Disconnected
        );
        assert!(state
            .whatsapp_link()
            .status_text()
            .contains("socket closed"));
    }

    #[test]
    fn whatsapp_qr_updates_ascii_payload() {
        let mut state = ModalState::new();
        state.set_whatsapp_link_qr("██ QR".to_string(), Some(42));
        assert_eq!(
            state.whatsapp_link().phase(),
            WhatsAppLinkPhase::AwaitingScan
        );
        assert_eq!(state.whatsapp_link().ascii_qr(), Some("██ QR"));
        assert_eq!(state.whatsapp_link().expires_at_ms(), Some(42));
    }

    #[test]
    fn whatsapp_terminal_states_clear_qr_expiry() {
        let mut state = ModalState::new();
        state.set_whatsapp_link_qr("██ QR".to_string(), Some(42));
        assert_eq!(state.whatsapp_link().expires_at_ms(), Some(42));

        state.set_whatsapp_link_connected(Some("+12065550123".to_string()));
        assert_eq!(state.whatsapp_link().expires_at_ms(), None);

        state.set_whatsapp_link_qr("██ QR".to_string(), Some(77));
        state.set_whatsapp_link_error("pairing failed".to_string());
        assert_eq!(state.whatsapp_link().expires_at_ms(), None);

        state.set_whatsapp_link_qr("██ QR".to_string(), Some(99));
        state.set_whatsapp_link_disconnected(Some("socket closed".to_string()));
        assert_eq!(state.whatsapp_link().expires_at_ms(), None);
    }
}
