use uuid::Uuid;

pub type SessionId = Uuid;
pub type WorkspaceId = String;

pub const AGENT_ID_SWAROG: &str = "swarog";
pub const AGENT_NAME_SWAROG: &str = "Swarog";
pub const AGENT_ID_RAROG: &str = "rarog";
pub const AGENT_NAME_RAROG: &str = "Rarog";
pub const GATEWAY_IPC_PROTOCOL_VERSION: u16 = 1;

mod client;
mod daemon;
mod gateway;
mod support;

pub use client::ClientMessage;
pub use daemon::DaemonMessage;
pub use gateway::*;
pub use support::*;

#[cfg(test)]
mod tests;
