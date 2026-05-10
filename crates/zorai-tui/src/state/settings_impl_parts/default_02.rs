use super::cursor::*;
use super::*;
use crate::state::config::ConfigState;
use zorai_shared::providers::PROVIDER_ID_OPENROUTER;
impl Default for PluginSettingsState {
    fn default() -> Self {
        Self::new()
    }
}
