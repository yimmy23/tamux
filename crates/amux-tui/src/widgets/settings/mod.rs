use ratatui::prelude::*;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph};

use amux_protocol::has_whatsapp_allowed_contacts;

use crate::providers;
use crate::state::concierge::ConciergeState;
use crate::state::config::ConfigState;
use crate::state::modal::{ModalState, WhatsAppLinkPhase};
use crate::state::settings::{PluginSettingsState, SettingsState, SettingsTab};
use crate::state::subagents::SubAgentsState;
use crate::theme::ThemeTokens;
use crate::widgets::message::wrap_text;

include!("part1.rs");
include!("part2.rs");
include!("part3.rs");
include!("part4.rs");
include!("part5.rs");
include!("part6.rs");
include!("part7.rs");
include!("part8.rs");
include!("part9.rs");
include!("part10.rs");
include!("part11.rs");
include!("part12.rs");

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::config::ConfigState;
    use crate::state::modal::ModalState;
    use crate::state::settings::SettingsState;

    include!("tests/tests_part1.rs");
}
