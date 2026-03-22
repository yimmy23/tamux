#![allow(dead_code)]

/// Entry describing one provider's authentication status (TUI-side).
#[derive(Debug, Clone)]
pub struct ProviderAuthEntry {
    pub provider_id: String,
    pub provider_name: String,
    pub authenticated: bool,
    pub auth_source: String,
    pub model: String,
}

/// TUI state for the Auth settings tab.
pub struct AuthState {
    pub entries: Vec<ProviderAuthEntry>,
    pub selected: usize,
    pub validating: Option<String>,
    pub login_buffer: String,
    pub login_cursor: usize,
    pub login_target: Option<String>,
    pub actions_focused: bool,
    pub action_cursor: usize,
}

impl AuthState {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            selected: 0,
            validating: None,
            login_buffer: String::new(),
            login_cursor: 0,
            login_target: None,
            actions_focused: false,
            action_cursor: 0,
        }
    }
}

#[derive(Debug, Clone)]
pub enum AuthAction {
    Received(Vec<ProviderAuthEntry>),
    ValidationResult { provider_id: String, valid: bool, error: Option<String> },
    Select(usize),
    StartLogin(String),
    CancelLogin,
    LoginKeyChar(char),
    LoginKeyBackspace,
    ConfirmLogin,
    Logout(String),
}

impl AuthState {
    pub fn reduce(&mut self, action: AuthAction) {
        match action {
            AuthAction::Received(entries) => {
                self.entries = entries;
                if self.selected >= self.entries.len() {
                    self.selected = self.entries.len().saturating_sub(1);
                }
            }
            AuthAction::ValidationResult { provider_id, .. } => {
                if self.validating.as_deref() == Some(&provider_id) {
                    self.validating = None;
                }
            }
            AuthAction::Select(idx) => {
                if idx < self.entries.len() {
                    self.selected = idx;
                }
            }
            AuthAction::StartLogin(provider_id) => {
                self.login_target = Some(provider_id);
                self.login_buffer.clear();
                self.login_cursor = 0;
            }
            AuthAction::CancelLogin => {
                self.login_target = None;
                self.login_buffer.clear();
                self.login_cursor = 0;
            }
            AuthAction::LoginKeyChar(c) => {
                self.login_buffer.insert(self.login_cursor, c);
                self.login_cursor += c.len_utf8();
            }
            AuthAction::LoginKeyBackspace => {
                if self.login_cursor > 0 {
                    let prev = self.login_buffer[..self.login_cursor]
                        .char_indices()
                        .last()
                        .map(|(i, _)| i)
                        .unwrap_or(0);
                    self.login_buffer.drain(prev..self.login_cursor);
                    self.login_cursor = prev;
                }
            }
            AuthAction::ConfirmLogin => {
                // Handled externally — the handler sends the key to the daemon.
                self.login_target = None;
                self.login_buffer.clear();
                self.login_cursor = 0;
            }
            AuthAction::Logout(_) => {
                // Handled externally — the handler sends logout to the daemon.
            }
        }
    }
}
