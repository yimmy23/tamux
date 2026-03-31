use super::*;

#[derive(Debug, Clone, serde::Deserialize)]
pub(super) struct ProviderAuthState {
    pub provider_id: String,
    pub provider_name: String,
    #[allow(dead_code)]
    pub authenticated: bool,
    pub auth_source: String,
    pub model: String,
    pub base_url: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum WhatsAppAllowlistPromptOutcome {
    Submitted(String),
    Cancelled,
    EndOfInput,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum WhatsAppAllowlistPromptResolution<'a> {
    Accept(String),
    Retry(&'a str),
    Cancel,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ConfigWrite {
    pub key_path: String,
    pub value_json: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum WhatsAppLinkAttemptOutcome {
    Linked(Option<String>),
    TimedOut,
    CancelledByUser,
}

pub(super) struct RawModeGuard;

impl RawModeGuard {
    pub(super) fn new() -> Result<Self> {
        terminal::enable_raw_mode().context("Failed to enable raw mode")?;
        Ok(Self)
    }
}

impl Drop for RawModeGuard {
    fn drop(&mut self) {
        let _ = terminal::disable_raw_mode();
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PostSetupAction {
    LaunchTui,
    LaunchElectron,
    NotNow,
}

#[derive(Debug, Clone)]
pub(super) struct ProviderSelection {
    pub provider_id: String,
    pub provider_name: String,
    pub base_url: String,
    pub default_model: String,
    pub auth_source: String,
}

#[derive(Debug, Default)]
pub(super) struct SetupSummary {
    pub model: Option<String>,
    pub web_search: Option<String>,
    pub gateway: Option<String>,
    pub whatsapp_linked: bool,
}
