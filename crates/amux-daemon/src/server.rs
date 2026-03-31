use std::collections::HashSet;
use std::path::Path;
use std::sync::Arc;

use amux_protocol::{
    ClientMessage, DaemonMessage, GatewayBootstrapPayload, GatewayConnectionStatus,
    GatewayContinuityState, GatewayCursorState, GatewayHealthState, GatewayIncomingEvent,
    GatewayProviderBootstrap, GatewayRegistration, GatewayRouteMode, GatewayRouteModeState,
    GatewayThreadBindingState, SessionInfo, GATEWAY_IPC_PROTOCOL_VERSION,
};
use anyhow::{Context, Result};
use futures::SinkExt;
use futures::StreamExt;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::sync::{broadcast, mpsc};
use tokio_util::codec::Framed;

use crate::agent::skill_community::{
    export_skill, import_community_skill, prepare_publish, unpack_skill, ImportResult,
};
use crate::agent::skill_registry::{to_community_entry, RegistryClient};
use crate::agent::AgentEngine;
use crate::session_manager::SessionManager;

include!("server/helpers_part1.rs");
include!("server/helpers_part2.rs");

#[cfg(test)]
mod tests {
    include!("server/tests_part1.rs");
    include!("server/tests_part2.rs");
    include!("server/tests_part3.rs");
}

include!("server/post_tests.rs");

async fn handle_connection<S>(
    stream: S,
    manager: Arc<SessionManager>,
    agent: Arc<AgentEngine>,
    plugin_manager: Arc<crate::plugin::PluginManager>,
) -> Result<()>
where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    use amux_protocol::DaemonCodec;
    let mut framed = Framed::new(stream, DaemonCodec);

    let mut attached_rxs: Vec<(amux_protocol::SessionId, broadcast::Receiver<DaemonMessage>)> =
        Vec::new();
    let mut client_agent_threads: HashSet<String> = HashSet::new();
    let mut last_concierge_welcome_fingerprint: Option<String> = None;
    let mut agent_event_rx: Option<broadcast::Receiver<crate::agent::types::AgentEvent>> = None;
    let mut whatsapp_link_rx: Option<
        broadcast::Receiver<crate::agent::types::WhatsAppLinkRuntimeEvent>,
    > = None;
    let mut gateway_ipc_rx: Option<mpsc::UnboundedReceiver<DaemonMessage>> = None;
    let mut whatsapp_link_subscriber_guard = WhatsAppLinkSubscriberGuard::new(agent.clone());
    let mut whatsapp_link_snapshot_replayed = false;
    let mut gateway_connection_state = GatewayConnectionState::Unregistered;

    loop {
        let msg = include!("server/connection_pre_dispatch.rs");

        if let Some(msg) = msg {
            include!("server/dispatch_part1.rs");
            include!("server/dispatch_part2.rs");
            include!("server/dispatch_part3.rs");
            include!("server/dispatch_part4.rs");
            include!("server/dispatch_part5.rs");
            include!("server/dispatch_part6.rs");
            include!("server/dispatch_part7.rs");
            include!("server/dispatch_part8.rs");
            include!("server/dispatch_part9.rs");
            include!("server/dispatch_part10.rs");
        }
    }
}
