use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use zorai_protocol::{SecurityLevel, AGENT_NAME_RAROG, AGENT_NAME_SWAROG};

use super::capability_tier::TierConfig;

pub type WhatsAppLinkRuntimeEvent = super::whatsapp_link::WhatsAppLinkEvent;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WelesVerdict {
    Allow,
    Block,
    FlagOnly,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WelesReviewMeta {
    pub weles_reviewed: bool,
    pub verdict: WelesVerdict,
    #[serde(default)]
    pub reasons: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audit_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub security_override_mode: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WelesHealthState {
    Healthy,
    Degraded,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WelesHealthStatus {
    pub state: WelesHealthState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    pub checked_at: u64,
}

include!("provider_basics.rs");
include!("provider_catalog_a.rs");
include!("provider_catalog_b.rs");
include!("custom_provider_catalog.rs");
include!("custom_provider_runtime.rs");
include!("provider_registry.rs");
include!("config_core.rs");
include!("config_skill.rs");
include!("runtime_config.rs");
include!("runtime_profiles.rs");
include!("browser_profiles.rs");
include!("agent_event.rs");
include!("thread_message_types.rs");
include!("task_types.rs");
include!("goal_dossier.rs");
include!("goal_types.rs");
include!("heartbeat_misc.rs");

#[cfg(test)]
mod tests;
