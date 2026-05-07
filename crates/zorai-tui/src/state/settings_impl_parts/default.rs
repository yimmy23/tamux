use super::*;
use super::cursor::*;
use crate::state::config::ConfigState;
use zorai_shared::providers::PROVIDER_ID_OPENROUTER;
impl Default for SettingsState {
    fn default() -> Self {
        Self::new()
    }
}
