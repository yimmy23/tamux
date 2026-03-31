use amux_protocol::{SecurityLevel, AGENT_NAME_RAROG, AGENT_NAME_SWAROG};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

use super::capability_tier::TierConfig;

pub type WhatsAppLinkRuntimeEvent = super::whatsapp_link::WhatsAppLinkEvent;

include!("provider_basics.rs");
include!("provider_catalog_a.rs");
include!("provider_catalog_b.rs");
include!("provider_registry.rs");
include!("config_core.rs");
include!("config_skill.rs");
include!("runtime_config.rs");
include!("agent_event.rs");
include!("thread_message_types.rs");
include!("task_types.rs");
include!("goal_types.rs");
include!("heartbeat_misc.rs");

#[cfg(test)]
mod tests;
