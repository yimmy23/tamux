use super::*;

// Keep all the items the test parts need at this `tests` mod scope so the
// descendant test-part submodules can pull them in via `use super::*;`.
// (Previously the test parts were textually `include!`d into this same module
// body, so all their `use` declarations were already at this scope.)
use super::{
    build_gateway_bootstrap_payload, build_session_end_episode_payload,
    concierge_welcome_fingerprint, enqueue_gateway_incoming_event, handle_connection,
    is_expected_disconnect_error, persist_gateway_health_update, StartupReadiness,
};
use crate::agent::types::AgentConfig;
use crate::agent::types::{
    AgentEvent, AgentMessage, AgentThread, ConciergeAction, ConciergeActionType,
    ConciergeDetailLevel,
};
use crate::agent::AgentEngine;
use crate::agent::{StreamCancellationEntry, StreamProgressKind};
use crate::history::HistoryStore;
use crate::plugin::PluginManager;
use crate::session_manager::SessionManager;
use futures::{SinkExt, StreamExt};
use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::io::DuplexStream;
use tokio::task::JoinHandle;
use tokio::time::{timeout, Duration};
use zorai_protocol::{
    ClientMessage, DaemonMessage, GatewayConnectionStatus, GatewayHealthState,
    GatewayIncomingEvent, GatewayRegistration, GatewaySendRequest, SessionInfo, ZoraiCodec,
    GATEWAY_IPC_PROTOCOL_VERSION,
};

#[path = "tests_part1.rs"]
mod tests_part1;
#[path = "tests_part2.rs"]
mod tests_part2;
#[path = "tests_part3.rs"]
mod tests_part3;
