#![allow(dead_code)]

use std::collections::HashMap;

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
    pub loaded: bool,
    pub selected: usize,
    pub validating: Option<String>,
    pub validation_results: HashMap<String, (bool, String)>,
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
            loaded: false,
            selected: 0,
            validating: None,
            validation_results: HashMap::new(),
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
    ValidationResult {
        provider_id: String,
        valid: bool,
        error: Option<String>,
    },
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
                self.loaded = true;
                self.entries = entries;
                self.validation_results.retain(|provider_id, _| {
                    self.entries
                        .iter()
                        .any(|entry| &entry.provider_id == provider_id)
                });
                if self.selected >= self.entries.len() {
                    self.selected = self.entries.len().saturating_sub(1);
                }
            }
            AuthAction::ValidationResult {
                provider_id,
                valid,
                error,
            } => {
                if self.validating.as_deref() == Some(&provider_id) {
                    self.validating = None;
                }
                let message = if valid {
                    "Connection OK".to_string()
                } else {
                    format!("Error: {}", error.unwrap_or_else(|| "unknown".to_string()))
                };
                self.validation_results
                    .insert(provider_id, (valid, message));
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
                let byte_idx = self
                    .login_buffer
                    .char_indices()
                    .nth(self.login_cursor)
                    .map(|(idx, _)| idx)
                    .unwrap_or(self.login_buffer.len());
                self.login_buffer.insert(byte_idx, c);
                self.login_cursor += 1;
            }
            AuthAction::LoginKeyBackspace => {
                if self.login_cursor > 0 {
                    let start_char = self.login_cursor.saturating_sub(1);
                    let start_byte = self
                        .login_buffer
                        .char_indices()
                        .nth(start_char)
                        .map(|(idx, _)| idx)
                        .unwrap_or(0);
                    let end_byte = self
                        .login_buffer
                        .char_indices()
                        .nth(self.login_cursor)
                        .map(|(idx, _)| idx)
                        .unwrap_or(self.login_buffer.len());
                    self.login_buffer.drain(start_byte..end_byte);
                    self.login_cursor = start_char;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn received_marks_auth_state_loaded() {
        let mut state = AuthState::new();
        assert!(!state.loaded);

        state.reduce(AuthAction::Received(Vec::new()));

        assert!(state.loaded);
    }
}
