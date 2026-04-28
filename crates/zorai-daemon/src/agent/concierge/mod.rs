//! Concierge agent — proactive welcome greetings and lightweight ops assistant.

use super::circuit_breaker::CircuitBreakerRegistry;
use super::llm_client::{
    self, ApiContent, ApiMessage, ApiToolCall, ApiToolCallFunction, RetryStrategy,
};
use super::provider_resolution::resolve_provider_config_for;
use super::types::*;
use super::{execute_tool, get_available_tools, CONCIERGE_AGENT_NAME, MAIN_AGENT_NAME};
use anyhow::Result;
use futures::StreamExt;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{broadcast, Mutex, RwLock};

mod context;
mod onboarding;
mod profile;
mod runtime;
mod state;
mod triage;
mod welcome;

pub const CONCIERGE_THREAD_ID: &str = "concierge";
const WELCOME_REUSE_WINDOW_MS: u64 = 2 * 60 * 60 * 1000;
const GATEWAY_TRIAGE_MAX_TOOL_ROUNDS: usize = 3;
const GATEWAY_TRIAGE_SAFE_TOOL_NAMES: &[&str] = &[
    "search_history",
    "fetch_gateway_history",
    "message_agent",
    "session_search",
    "agent_query_memory",
    "onecontext_search",
    "list_guidelines",
    "discover_guidelines",
    "read_guideline",
    "list_skills",
    "semantic_query",
    "discover_skills",
    "read_skill",
    "web_search",
];

pub enum GatewayTriage {
    Simple(String),
    Complex,
}

pub struct ConciergeEngine {
    config: Arc<RwLock<AgentConfig>>,
    event_tx: broadcast::Sender<AgentEvent>,
    http_client: reqwest::Client,
    circuit_breakers: Arc<CircuitBreakerRegistry>,
    welcome_cache: Arc<RwLock<Option<WelcomeCacheEntry>>>,
    recovery_investigations: Arc<Mutex<HashMap<String, String>>>,
}

#[allow(unused_imports)]
pub(crate) use context::*;
#[allow(unused_imports)]
pub(super) use onboarding::*;
pub use profile::*;
#[allow(unused_imports)]
pub(super) use runtime::*;
#[allow(unused_imports)]
pub(super) use state::*;
#[allow(unused_imports)]
pub(super) use triage::*;
#[allow(unused_imports)]
pub(super) use welcome::*;

#[cfg(test)]
mod tests;
