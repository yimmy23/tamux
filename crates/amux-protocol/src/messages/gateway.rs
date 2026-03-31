use serde::{Deserialize, Serialize};

use super::{SessionId, WorkspaceId};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GatewayRegistration {
    pub gateway_id: String,
    pub instance_id: String,
    pub protocol_version: u16,
    pub supported_platforms: Vec<String>,
    #[serde(default)]
    pub process_id: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GatewayAck {
    pub correlation_id: String,
    pub accepted: bool,
    #[serde(default)]
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GatewayProviderBootstrap {
    pub platform: String,
    pub enabled: bool,
    pub credentials_json: String,
    pub config_json: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GatewayCursorState {
    pub platform: String,
    pub channel_id: String,
    pub cursor_value: String,
    pub cursor_type: String,
    pub updated_at_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GatewayThreadBindingState {
    pub channel_key: String,
    #[serde(default)]
    pub thread_id: Option<String>,
    pub updated_at_ms: u64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum GatewayRouteMode {
    Rarog,
    Swarog,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GatewayRouteModeState {
    pub channel_key: String,
    pub route_mode: GatewayRouteMode,
    pub updated_at_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct GatewayContinuityState {
    #[serde(default)]
    pub cursors: Vec<GatewayCursorState>,
    #[serde(default)]
    pub thread_bindings: Vec<GatewayThreadBindingState>,
    #[serde(default)]
    pub route_modes: Vec<GatewayRouteModeState>,
    #[serde(default)]
    pub health_snapshots: Vec<GatewayHealthState>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GatewayBootstrapPayload {
    pub bootstrap_correlation_id: String,
    #[serde(default)]
    pub feature_flags: Vec<String>,
    #[serde(default)]
    pub providers: Vec<GatewayProviderBootstrap>,
    #[serde(default)]
    pub continuity: GatewayContinuityState,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GatewayIncomingEvent {
    pub platform: String,
    pub channel_id: String,
    pub sender_id: String,
    #[serde(default)]
    pub sender_display: Option<String>,
    pub content: String,
    #[serde(default)]
    pub message_id: Option<String>,
    #[serde(default)]
    pub thread_id: Option<String>,
    pub received_at_ms: u64,
    #[serde(default)]
    pub raw_event_json: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GatewaySendRequest {
    pub correlation_id: String,
    pub platform: String,
    pub channel_id: String,
    #[serde(default)]
    pub thread_id: Option<String>,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GatewaySendResult {
    pub correlation_id: String,
    pub platform: String,
    pub channel_id: String,
    #[serde(default)]
    pub requested_channel_id: Option<String>,
    #[serde(default)]
    pub delivery_id: Option<String>,
    pub ok: bool,
    #[serde(default)]
    pub error: Option<String>,
    pub completed_at_ms: u64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum GatewayConnectionStatus {
    Connected,
    Disconnected,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GatewayHealthState {
    pub platform: String,
    pub status: GatewayConnectionStatus,
    #[serde(default)]
    pub last_success_at_ms: Option<u64>,
    #[serde(default)]
    pub last_error_at_ms: Option<u64>,
    pub consecutive_failure_count: u32,
    #[serde(default)]
    pub last_error: Option<String>,
    pub current_backoff_secs: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GatewayReloadCommand {
    pub correlation_id: String,
    #[serde(default)]
    pub reason: Option<String>,
    pub requested_at_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GatewayShutdownCommand {
    pub correlation_id: String,
    #[serde(default)]
    pub reason: Option<String>,
    pub requested_at_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionInfo {
    pub id: SessionId,
    pub title: Option<String>,
    pub cwd: Option<String>,
    pub cols: u16,
    pub rows: u16,
    pub created_at: u64,
    pub workspace_id: Option<WorkspaceId>,
    pub exit_code: Option<i32>,
    pub is_alive: bool,
    pub active_command: Option<String>,
}
