use std::collections::HashMap;

use anyhow::Result;
use tokio::sync::mpsc;
use zorai_protocol::{
    ClientMessage, DaemonMessage, GatewayAck, GatewayConnectionStatus, GatewayCursorState,
    GatewayHealthState, GatewayIncomingEvent, GatewayRouteModeState, GatewaySendRequest,
    GatewaySendResult, GatewayThreadBindingState,
};

use crate::router::GatewayMessage;
use crate::state::GatewayRuntimeState;

#[derive(Debug, Clone)]
pub enum GatewayProviderEvent {
    Incoming(GatewayMessage),
    CursorUpdate(GatewayCursorState),
    ThreadBindingUpdate(GatewayThreadBindingState),
    RouteModeUpdate(GatewayRouteModeState),
    SendResult(GatewaySendResult),
    HealthUpdate(GatewayHealthState),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GatewaySendOutcome {
    pub channel_id: String,
    pub delivery_id: Option<String>,
}

pub trait GatewayProvider: Send + 'static {
    fn platform(&self) -> &str;

    fn connect(
        &mut self,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send + '_>>;

    fn recv(
        &mut self,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<Option<GatewayProviderEvent>>> + Send + '_>,
    >;

    fn send(
        &mut self,
        request: GatewaySendRequest,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<GatewaySendOutcome>> + Send + '_>>;
}

pub trait DaemonConnection: Send + 'static {
    fn send(
        &mut self,
        msg: ClientMessage,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send + '_>>;

    fn recv(
        &mut self,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<Option<DaemonMessage>>> + Send + '_>,
    >;
}

pub struct GatewayRuntimeCore {
    state: GatewayRuntimeState,
    daemon_tx: mpsc::UnboundedSender<ClientMessage>,
    provider_queue_tx: mpsc::UnboundedSender<GatewaySendRequest>,
}

impl GatewayRuntimeCore {
    #[cfg(test)]
    pub fn new(
        daemon_tx: mpsc::UnboundedSender<ClientMessage>,
        provider_queue_tx: mpsc::UnboundedSender<GatewaySendRequest>,
    ) -> Self {
        Self {
            state: GatewayRuntimeState::default(),
            daemon_tx,
            provider_queue_tx,
        }
    }

    pub fn with_state(
        state: GatewayRuntimeState,
        daemon_tx: mpsc::UnboundedSender<ClientMessage>,
        provider_queue_tx: mpsc::UnboundedSender<GatewaySendRequest>,
    ) -> Self {
        Self {
            state,
            daemon_tx,
            provider_queue_tx,
        }
    }

    #[cfg(test)]
    pub fn state(&self) -> &GatewayRuntimeState {
        &self.state
    }

    #[cfg(test)]
    pub fn bootstrap_from_daemon_message(&mut self, msg: DaemonMessage) -> Result<()> {
        let DaemonMessage::GatewayBootstrap { payload } = msg else {
            anyhow::bail!("expected gateway bootstrap message");
        };
        self.state = GatewayRuntimeState::from_bootstrap(&payload);
        Ok(())
    }

    pub fn send_bootstrap_ready_ack(&self) -> Result<()> {
        let correlation_id = self.state.bootstrap_correlation_id().trim();
        if correlation_id.is_empty() {
            anyhow::bail!("cannot acknowledge bootstrap without correlation id");
        }
        self.send_to_daemon(ClientMessage::GatewayAck {
            ack: GatewayAck {
                correlation_id: correlation_id.to_string(),
                accepted: true,
                detail: Some("gateway runtime ready".to_string()),
            },
        })
    }

    pub fn route_incoming_provider_event(&mut self, msg: GatewayMessage) -> Result<()> {
        self.send_to_daemon(ClientMessage::GatewayIncomingEvent {
            event: GatewayIncomingEvent {
                platform: msg.platform,
                channel_id: msg.channel_id,
                sender_id: msg.user_id,
                sender_display: msg.sender_display,
                content: msg.text,
                message_id: msg.message_id,
                thread_id: msg.thread_id,
                received_at_ms: msg.timestamp.saturating_mul(1000),
                raw_event_json: msg.raw_event_json,
            },
        })
    }

    pub fn apply_daemon_message(&mut self, msg: DaemonMessage) -> Result<()> {
        match msg {
            DaemonMessage::GatewayBootstrap { payload } => {
                self.state = GatewayRuntimeState::from_bootstrap(&payload);
            }
            DaemonMessage::GatewaySendRequest { request } => {
                self.provider_queue_tx
                    .send(request)
                    .map_err(|_| anyhow::anyhow!("provider queue is closed"))?;
            }
            DaemonMessage::GatewayReloadCommand { command } => {
                self.send_to_daemon(ClientMessage::GatewayAck {
                    ack: GatewayAck {
                        correlation_id: command.correlation_id,
                        accepted: true,
                        detail: Some("gateway runtime reload acknowledged".to_string()),
                    },
                })?;
            }
            DaemonMessage::GatewayShutdownCommand { command } => {
                self.send_to_daemon(ClientMessage::GatewayAck {
                    ack: GatewayAck {
                        correlation_id: command.correlation_id,
                        accepted: true,
                        detail: Some("gateway runtime shutdown acknowledged".to_string()),
                    },
                })?;
            }
            _ => {}
        }
        Ok(())
    }

    pub fn emit_live_cursor_update(&mut self, update: GatewayCursorState) -> Result<()> {
        self.state.apply_cursor_update(update.clone());
        self.send_to_daemon(ClientMessage::GatewayCursorUpdate { update })
    }

    pub fn emit_live_thread_binding_update(
        &mut self,
        update: GatewayThreadBindingState,
    ) -> Result<()> {
        self.state.apply_thread_binding_update(update.clone());
        self.send_to_daemon(ClientMessage::GatewayThreadBindingUpdate { update })
    }

    pub fn emit_live_route_mode_update(&mut self, update: GatewayRouteModeState) -> Result<()> {
        self.state.apply_route_mode_update(update.clone());
        self.send_to_daemon(ClientMessage::GatewayRouteModeUpdate { update })
    }

    pub fn emit_health_update(&mut self, update: GatewayHealthState) -> Result<()> {
        self.state.apply_health_update(update.clone());
        self.send_to_daemon(ClientMessage::GatewayHealthUpdate { update })
    }

    pub fn emit_send_result(&mut self, result: GatewaySendResult) -> Result<()> {
        self.send_to_daemon(ClientMessage::GatewaySendResult { result })
    }

    fn send_to_daemon(&self, msg: ClientMessage) -> Result<()> {
        self.daemon_tx
            .send(msg)
            .map_err(|_| anyhow::anyhow!("daemon outbound channel is closed"))
    }
}

pub struct GatewayRuntime<D> {
    daemon: D,
    core: GatewayRuntimeCore,
    providers: Vec<Box<dyn GatewayProvider>>,
    daemon_out_rx: mpsc::UnboundedReceiver<ClientMessage>,
    provider_queue_rx: mpsc::UnboundedReceiver<GatewaySendRequest>,
    provider_events_tx: mpsc::UnboundedSender<GatewayProviderEvent>,
    provider_events_rx: mpsc::UnboundedReceiver<GatewayProviderEvent>,
    provider_senders: HashMap<String, mpsc::UnboundedSender<GatewaySendRequest>>,
    provider_tasks: Vec<tokio::task::JoinHandle<()>>,
}

impl<D: DaemonConnection> GatewayRuntime<D> {
    pub fn new(
        daemon: D,
        state: GatewayRuntimeState,
        providers: Vec<Box<dyn GatewayProvider>>,
    ) -> Self {
        let (daemon_out_tx, daemon_out_rx) = mpsc::unbounded_channel();
        let (provider_queue_tx, provider_queue_rx) = mpsc::unbounded_channel();
        let (provider_events_tx, provider_events_rx) = mpsc::unbounded_channel();

        Self {
            daemon,
            core: GatewayRuntimeCore::with_state(state, daemon_out_tx, provider_queue_tx),
            providers,
            daemon_out_rx,
            provider_queue_rx,
            provider_events_tx,
            provider_events_rx,
            provider_senders: HashMap::new(),
            provider_tasks: Vec::new(),
        }
    }

    pub async fn run(mut self) -> Result<()> {
        self.core.send_bootstrap_ready_ack()?;
        self.start_providers();

        let mut daemon = self.daemon;
        let mut core = self.core;
        let mut daemon_out_rx = self.daemon_out_rx;
        let mut provider_queue_rx = self.provider_queue_rx;
        let mut provider_events_rx = self.provider_events_rx;
        let mut provider_senders = self.provider_senders;
        let provider_tasks = self.provider_tasks;

        let shutdown = tokio::signal::ctrl_c();
        tokio::pin!(shutdown);

        loop {
            tokio::select! {
                _ = &mut shutdown => break,
                maybe_outbound = daemon_out_rx.recv() => {
                    if let Some(message) = maybe_outbound {
                        daemon.send(message).await?;
                    } else {
                        break;
                    }
                }
                daemon_message = daemon.recv() => {
                    let Some(message) = daemon_message? else {
                        break;
                    };
                    let should_shutdown = matches!(message, DaemonMessage::GatewayShutdownCommand { .. });
                    core.apply_daemon_message(message)?;
                    if should_shutdown {
                        break;
                    }
                }
                maybe_event = provider_events_rx.recv() => {
                    if let Some(event) = maybe_event {
                        apply_provider_event(&mut core, event)?;
                    }
                }
                maybe_request = provider_queue_rx.recv() => {
                    if let Some(request) = maybe_request {
                        dispatch_send_request(&mut core, &mut provider_senders, request)?;
                    }
                }
            }
        }

        for task in provider_tasks {
            task.abort();
        }

        Ok(())
    }

    fn start_providers(&mut self) {
        for mut provider in self.providers.drain(..) {
            let platform = provider.platform().to_ascii_lowercase();
            let provider_label = provider.platform().to_string();
            let (send_tx, mut send_rx) = mpsc::unbounded_channel::<GatewaySendRequest>();
            self.provider_senders.insert(platform.clone(), send_tx);
            let event_tx = self.provider_events_tx.clone();

            let task = tokio::spawn(async move {
                if let Err(error) = provider.connect().await {
                    let _ = event_tx.send(GatewayProviderEvent::HealthUpdate(GatewayHealthState {
                        platform: platform.clone(),
                        status: GatewayConnectionStatus::Error,
                        last_success_at_ms: None,
                        last_error_at_ms: Some(now_ms()),
                        consecutive_failure_count: 1,
                        last_error: Some(error.to_string()),
                        current_backoff_secs: 0,
                    }));
                    return;
                }

                let _ = event_tx.send(GatewayProviderEvent::HealthUpdate(GatewayHealthState {
                    platform: platform.clone(),
                    status: GatewayConnectionStatus::Connected,
                    last_success_at_ms: Some(now_ms()),
                    last_error_at_ms: None,
                    consecutive_failure_count: 0,
                    last_error: None,
                    current_backoff_secs: 0,
                }));

                let mut poll_timer = tokio::time::interval(std::time::Duration::from_millis(150));
                loop {
                    tokio::select! {
                        maybe_request = send_rx.recv() => {
                            let Some(request) = maybe_request else {
                                break;
                            };
                            let correlation_id = request.correlation_id.clone();
                            let requested_channel_id = request.channel_id.clone();
                            let send_result = provider.send(request).await;

                            let result = match send_result {
                                Ok(outcome) => GatewaySendResult {
                                    correlation_id,
                                    platform: platform.clone(),
                                    channel_id: outcome.channel_id,
                                    requested_channel_id: Some(requested_channel_id),
                                    delivery_id: outcome.delivery_id,
                                    ok: true,
                                    error: None,
                                    completed_at_ms: now_ms(),
                                },
                                Err(error) => GatewaySendResult {
                                    correlation_id,
                                    platform: platform.clone(),
                                    channel_id: requested_channel_id.clone(),
                                    requested_channel_id: Some(requested_channel_id.clone()),
                                    delivery_id: None,
                                    ok: false,
                                    error: Some(error.to_string()),
                                    completed_at_ms: now_ms(),
                                },
                            };
                            let _ = event_tx.send(GatewayProviderEvent::SendResult(result));
                        }
                        _ = poll_timer.tick() => {
                            match provider.recv().await {
                                Ok(Some(event)) => {
                                    let _ = event_tx.send(event);
                                }
                                Ok(None) => {}
                                Err(error) => {
                                    tracing::error!(
                                        provider = provider_label,
                                        error = %error,
                                        "provider receive loop failed"
                                    );
                                }
                            }
                        }
                    }
                }
            });
            self.provider_tasks.push(task);
        }
    }
}

fn apply_provider_event(core: &mut GatewayRuntimeCore, event: GatewayProviderEvent) -> Result<()> {
    match event {
        GatewayProviderEvent::Incoming(message) => core.route_incoming_provider_event(message)?,
        GatewayProviderEvent::CursorUpdate(update) => core.emit_live_cursor_update(update)?,
        GatewayProviderEvent::ThreadBindingUpdate(update) => {
            core.emit_live_thread_binding_update(update)?
        }
        GatewayProviderEvent::RouteModeUpdate(update) => {
            core.emit_live_route_mode_update(update)?
        }
        GatewayProviderEvent::SendResult(result) => core.emit_send_result(result)?,
        GatewayProviderEvent::HealthUpdate(update) => core.emit_health_update(update)?,
    }
    Ok(())
}

fn dispatch_send_request(
    core: &mut GatewayRuntimeCore,
    provider_senders: &mut HashMap<String, mpsc::UnboundedSender<GatewaySendRequest>>,
    request: GatewaySendRequest,
) -> Result<()> {
    let platform = request.platform.to_ascii_lowercase();
    if let Some(send_tx) = provider_senders.get(&platform) {
        if send_tx.send(request.clone()).is_ok() {
            return Ok(());
        }
    }
    let requested_channel_id = request.channel_id.clone();

    core.emit_send_result(GatewaySendResult {
        correlation_id: request.correlation_id,
        platform: request.platform,
        channel_id: request.channel_id,
        requested_channel_id: Some(requested_channel_id),
        delivery_id: None,
        ok: false,
        error: Some("provider queue unavailable".to_string()),
        completed_at_ms: now_ms(),
    })
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests;
