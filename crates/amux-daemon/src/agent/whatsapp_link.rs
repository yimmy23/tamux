use anyhow::{bail, Context, Result};
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::process::{Child, Command};
use tokio::sync::{broadcast, Mutex};

use crate::history::{HistoryStore, WhatsAppProviderStateRow};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WhatsAppLinkState {
    Disconnected,
    Starting,
    QrReady,
    Connected,
    Error,
}

impl WhatsAppLinkState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Disconnected => "disconnected",
            Self::Starting => "starting",
            Self::QrReady => "qr_ready",
            Self::Connected => "connected",
            Self::Error => "error",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WhatsAppLinkStatusSnapshot {
    pub state: String,
    pub phone: Option<String>,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WhatsAppLinkEvent {
    Status(WhatsAppLinkStatusSnapshot),
    Qr {
        ascii_qr: String,
        expires_at_ms: Option<u64>,
    },
    Linked {
        phone: Option<String>,
    },
    Error {
        message: String,
        recoverable: bool,
    },
    Disconnected {
        reason: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SidecarLaunchSpec {
    pub program: String,
    pub args: Vec<String>,
    pub env: HashMap<String, String>,
}

pub const WHATSAPP_LINK_PROVIDER_ID: &str = "whatsapp_link";

pub mod transport {
    use super::*;

    pub type TransportFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct PersistedState {
        pub linked_phone: Option<String>,
        pub auth_json: Option<String>,
        pub metadata_json: Option<String>,
        pub last_reset_at: Option<u64>,
        pub last_linked_at: Option<u64>,
        pub updated_at: u64,
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct SessionUpdate {
        pub linked_phone: Option<String>,
        pub auth_json: Option<String>,
        pub metadata_json: Option<String>,
        pub linked_at: Option<u64>,
        pub updated_at: u64,
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum WhatsAppTransportEvent {
        Starting,
        Qr {
            ascii_qr: String,
            expires_at_ms: Option<u64>,
        },
        SessionUpdated(SessionUpdate),
        Linked {
            phone: Option<String>,
        },
        Disconnected {
            reason: Option<String>,
        },
        Error {
            message: String,
            recoverable: bool,
        },
    }

    pub trait WhatsAppTransport: Send + Sync {
        fn provider_id(&self) -> &'static str;
        fn subscribe(&self) -> broadcast::Receiver<WhatsAppTransportEvent>;
        fn start<'a>(
            &'a self,
            restored_state: Option<PersistedState>,
        ) -> TransportFuture<'a, Result<()>>;
        fn stop<'a>(&'a self, reason: Option<String>) -> TransportFuture<'a, Result<()>>;
        fn reset<'a>(&'a self) -> TransportFuture<'a, Result<()>>;
    }

    #[cfg(test)]
    pub struct ScriptedTransport {
        tx: broadcast::Sender<WhatsAppTransportEvent>,
        actions: Mutex<Vec<String>>,
        restored_state: Mutex<Option<PersistedState>>,
    }

    #[cfg(test)]
    impl ScriptedTransport {
        pub fn new() -> Self {
            let (tx, _) = broadcast::channel(64);
            Self {
                tx,
                actions: Mutex::new(Vec::new()),
                restored_state: Mutex::new(None),
            }
        }

        pub async fn emit_qr(&self, ascii_qr: &str, expires_at_ms: Option<u64>) {
            let _ = self.tx.send(WhatsAppTransportEvent::Qr {
                ascii_qr: ascii_qr.to_string(),
                expires_at_ms,
            });
        }

        pub async fn emit_linked(&self, update: SessionUpdate) {
            let phone = update.linked_phone.clone();
            let _ = self
                .tx
                .send(WhatsAppTransportEvent::SessionUpdated(update));
            let _ = self.tx.send(WhatsAppTransportEvent::Linked { phone });
        }

        pub async fn actions(&self) -> Vec<String> {
            self.actions.lock().await.clone()
        }

        pub async fn restored_state(&self) -> Option<PersistedState> {
            self.restored_state.lock().await.clone()
        }
    }

    #[cfg(test)]
    impl WhatsAppTransport for ScriptedTransport {
        fn provider_id(&self) -> &'static str {
            WHATSAPP_LINK_PROVIDER_ID
        }

        fn subscribe(&self) -> broadcast::Receiver<WhatsAppTransportEvent> {
            self.tx.subscribe()
        }

        fn start<'a>(
            &'a self,
            restored_state: Option<PersistedState>,
        ) -> TransportFuture<'a, Result<()>> {
            Box::pin(async move {
                *self.restored_state.lock().await = restored_state;
                self.actions.lock().await.push("start".to_string());
                let _ = self.tx.send(WhatsAppTransportEvent::Starting);
                Ok(())
            })
        }

        fn stop<'a>(&'a self, reason: Option<String>) -> TransportFuture<'a, Result<()>> {
            Box::pin(async move {
                self.actions.lock().await.push("stop".to_string());
                let _ = self.tx.send(WhatsAppTransportEvent::Disconnected { reason });
                Ok(())
            })
        }

        fn reset<'a>(&'a self) -> TransportFuture<'a, Result<()>> {
            Box::pin(async move {
                self.actions.lock().await.push("reset".to_string());
                *self.restored_state.lock().await = None;
                let _ = self.tx.send(WhatsAppTransportEvent::Disconnected {
                    reason: Some("operator_reset".to_string()),
                });
                Ok(())
            })
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use tokio::time::{timeout, Duration};

        #[tokio::test]
        async fn scripted_transport_start_restores_persisted_state() {
            let transport = ScriptedTransport::new();
            let persisted = PersistedState {
                linked_phone: Some("+15550000001".to_string()),
                auth_json: Some("{\"token\":true}".to_string()),
                metadata_json: Some("{\"provider\":\"test\"}".to_string()),
                last_reset_at: Some(10),
                last_linked_at: Some(20),
                updated_at: 30,
            };
            let mut rx = transport.subscribe();

            transport
                .start(Some(persisted.clone()))
                .await
                .expect("start should succeed");

            assert_eq!(transport.restored_state().await, Some(persisted));
            assert_eq!(transport.actions().await, vec!["start".to_string()]);
            assert!(matches!(
                timeout(Duration::from_millis(250), rx.recv())
                    .await
                    .expect("starting event should arrive")
                    .expect("broadcast should stay open"),
                WhatsAppTransportEvent::Starting
            ));
        }

        #[tokio::test]
        async fn scripted_transport_emits_session_update_and_linked_events() {
            let transport = ScriptedTransport::new();
            let mut rx = transport.subscribe();

            transport
                .emit_qr("QR-TRANSPORT", Some(42))
                .await;
            transport
                .emit_linked(SessionUpdate {
                    linked_phone: Some("+15550000002".to_string()),
                    auth_json: Some("{\"session\":true}".to_string()),
                    metadata_json: Some("{\"jid\":\"abc\"}".to_string()),
                    linked_at: Some(99),
                    updated_at: 100,
                })
                .await;

            assert!(matches!(
                timeout(Duration::from_millis(250), rx.recv())
                    .await
                    .expect("qr event should arrive")
                    .expect("broadcast should stay open"),
                WhatsAppTransportEvent::Qr { ascii_qr, .. } if ascii_qr == "QR-TRANSPORT"
            ));
            assert!(matches!(
                timeout(Duration::from_millis(250), rx.recv())
                    .await
                    .expect("session event should arrive")
                    .expect("broadcast should stay open"),
                WhatsAppTransportEvent::SessionUpdated(SessionUpdate {
                    linked_phone: Some(phone),
                    ..
                }) if phone == "+15550000002"
            ));
            assert!(matches!(
                timeout(Duration::from_millis(250), rx.recv())
                    .await
                    .expect("linked event should arrive")
                    .expect("broadcast should stay open"),
                WhatsAppTransportEvent::Linked { phone: Some(phone) } if phone == "+15550000002"
            ));
        }

        #[tokio::test]
        async fn scripted_transport_reset_clears_restored_state() {
            let transport = ScriptedTransport::new();
            transport
                .start(Some(PersistedState {
                    linked_phone: Some("+15550000003".to_string()),
                    auth_json: None,
                    metadata_json: None,
                    last_reset_at: None,
                    last_linked_at: Some(15),
                    updated_at: 15,
                }))
                .await
                .expect("start should succeed");

            transport.reset().await.expect("reset should succeed");

            assert!(transport.restored_state().await.is_none());
            assert_eq!(
                transport.actions().await,
                vec!["start".to_string(), "reset".to_string()]
            );
        }
    }
}

pub fn persisted_state_from_history_row(row: WhatsAppProviderStateRow) -> transport::PersistedState {
    transport::PersistedState {
        linked_phone: row.linked_phone,
        auth_json: row.auth_json,
        metadata_json: row.metadata_json,
        last_reset_at: row.last_reset_at,
        last_linked_at: row.last_linked_at,
        updated_at: row.updated_at,
    }
}

pub fn persisted_state_into_history_row(
    provider_id: &str,
    state: transport::PersistedState,
) -> WhatsAppProviderStateRow {
    WhatsAppProviderStateRow {
        provider_id: provider_id.to_string(),
        linked_phone: state.linked_phone,
        auth_json: state.auth_json,
        metadata_json: state.metadata_json,
        last_reset_at: state.last_reset_at,
        last_linked_at: state.last_linked_at,
        updated_at: state.updated_at,
    }
}

pub async fn load_persisted_provider_state(
    history: &HistoryStore,
    provider_id: &str,
) -> Result<Option<transport::PersistedState>> {
    Ok(history
        .get_whatsapp_provider_state(provider_id)
        .await?
        .map(persisted_state_from_history_row))
}

pub async fn save_persisted_provider_state(
    history: &HistoryStore,
    provider_id: &str,
    state: transport::PersistedState,
) -> Result<()> {
    history
        .upsert_whatsapp_provider_state(persisted_state_into_history_row(provider_id, state))
        .await
}

pub async fn clear_persisted_provider_state(
    history: &HistoryStore,
    provider_id: &str,
) -> Result<()> {
    history.delete_whatsapp_provider_state(provider_id).await
}

pub fn merge_persisted_state_update(
    existing: Option<transport::PersistedState>,
    update: transport::SessionUpdate,
) -> transport::PersistedState {
    let existing = existing.unwrap_or(transport::PersistedState {
        linked_phone: None,
        auth_json: None,
        metadata_json: None,
        last_reset_at: None,
        last_linked_at: None,
        updated_at: update.updated_at,
    });

    transport::PersistedState {
        linked_phone: update.linked_phone.or(existing.linked_phone),
        auth_json: update.auth_json.or(existing.auth_json),
        metadata_json: update.metadata_json.or(existing.metadata_json),
        last_reset_at: existing.last_reset_at,
        last_linked_at: update.linked_at.or(existing.last_linked_at),
        updated_at: update.updated_at,
    }
}

pub async fn persist_transport_session_update(
    history: &HistoryStore,
    provider_id: &str,
    update: transport::SessionUpdate,
) -> Result<transport::PersistedState> {
    let merged = merge_persisted_state_update(
        load_persisted_provider_state(history, provider_id).await?,
        update,
    );
    save_persisted_provider_state(history, provider_id, merged.clone()).await?;
    Ok(merged)
}

#[derive(Debug)]
struct RuntimeInner {
    state: WhatsAppLinkState,
    phone: Option<String>,
    last_error: Option<String>,
    active_qr: Option<String>,
    active_qr_expires_at_ms: Option<u64>,
    process: Option<Child>,
    stopping: bool,
    retry_count: u32,
    last_retry_at_ms: Option<u64>,
    #[cfg(test)]
    forced_stop_kill_error: Option<String>,
}

pub struct WhatsAppLinkRuntime {
    inner: Mutex<RuntimeInner>,
    subscribers: Mutex<HashMap<u64, broadcast::Sender<WhatsAppLinkEvent>>>,
    next_subscriber_id: AtomicU64,
}

impl Default for WhatsAppLinkRuntime {
    fn default() -> Self {
        Self::new()
    }
}

impl WhatsAppLinkRuntime {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(RuntimeInner {
                state: WhatsAppLinkState::Disconnected,
                phone: None,
                last_error: None,
                active_qr: None,
                active_qr_expires_at_ms: None,
                process: None,
                stopping: false,
                retry_count: 0,
                last_retry_at_ms: None,
                #[cfg(test)]
                forced_stop_kill_error: None,
            }),
            subscribers: Mutex::new(HashMap::new()),
            next_subscriber_id: AtomicU64::new(1),
        }
    }

    pub async fn start(&self) -> Result<()> {
        let mut inner = self.inner.lock().await;
        inner.state = WhatsAppLinkState::Starting;
        inner.last_error = None;
        drop(inner);
        self.broadcast_status().await;
        Ok(())
    }

    pub async fn stop(&self, reason: Option<String>) -> Result<()> {
        #[cfg(test)]
        let mut forced_stop_kill_error = None::<String>;
        let mut child = {
            let mut inner = self.inner.lock().await;
            inner.stopping = true;
            inner.retry_count = 0;
            inner.last_retry_at_ms = None;
            #[cfg(test)]
            {
                forced_stop_kill_error = inner.forced_stop_kill_error.take();
            }
            inner.process.take()
        };

        let kill_result = if let Some(ref mut proc) = child {
            let forced_stop_kill_error: Option<String> = {
                #[cfg(test)]
                {
                    forced_stop_kill_error
                }
                #[cfg(not(test))]
                {
                    None
                }
            };
            if let Some(message) = forced_stop_kill_error {
                Err(anyhow::Error::msg(message))
            } else {
                proc.kill()
                    .await
                    .context("failed to stop whatsapp link sidecar process")
            }
        } else {
            Ok(())
        };

        match kill_result {
            Ok(()) => {
                {
                    let mut inner = self.inner.lock().await;
                    inner.process = None;
                    inner.stopping = false;
                    inner.last_error = None;
                }
                self.broadcast_disconnected(reason).await;
                self.broadcast_status().await;
                Ok(())
            }
            Err(err) => {
                let message = err.to_string();
                {
                    let mut inner = self.inner.lock().await;
                    inner.process = child;
                    inner.stopping = false;
                    inner.state = WhatsAppLinkState::Error;
                    inner.last_error = Some(message.clone());
                }
                self.broadcast_event(WhatsAppLinkEvent::Error {
                    message,
                    recoverable: false,
                })
                .await;
                self.broadcast_status().await;
                Err(err)
            }
        }
    }

    pub async fn reset(&self) -> Result<()> {
        self.stop(Some("operator_reset".to_string())).await?;
        {
            let mut inner = self.inner.lock().await;
            inner.phone = None;
            inner.last_error = None;
            inner.active_qr = None;
            inner.active_qr_expires_at_ms = None;
            inner.retry_count = 0;
            inner.last_retry_at_ms = None;
            inner.state = WhatsAppLinkState::Disconnected;
        }
        self.broadcast_status().await;
        Ok(())
    }

    pub async fn status_snapshot(&self) -> WhatsAppLinkStatusSnapshot {
        let inner = self.inner.lock().await;
        WhatsAppLinkStatusSnapshot {
            state: inner.state.as_str().to_string(),
            phone: inner.phone.clone(),
            last_error: inner.last_error.clone(),
        }
    }

    pub async fn subscribe_with_id(&self) -> (u64, broadcast::Receiver<WhatsAppLinkEvent>) {
        let (tx, rx) = broadcast::channel(64);
        let id = self.next_subscriber_id.fetch_add(1, Ordering::Relaxed);
        {
            let inner = self.inner.lock().await;
            let snapshot = WhatsAppLinkStatusSnapshot {
                state: inner.state.as_str().to_string(),
                phone: inner.phone.clone(),
                last_error: inner.last_error.clone(),
            };
            let replay_qr = if inner.state == WhatsAppLinkState::QrReady {
                inner.active_qr.clone()
            } else {
                None
            };
            let replay_qr_expires_at_ms = if replay_qr.is_some() {
                inner.active_qr_expires_at_ms
            } else {
                None
            };
            let mut subscribers = self.subscribers.lock().await;
            let _ = tx.send(WhatsAppLinkEvent::Status(snapshot));
            if let Some(ascii_qr) = replay_qr {
                let _ = tx.send(WhatsAppLinkEvent::Qr {
                    ascii_qr,
                    expires_at_ms: replay_qr_expires_at_ms,
                });
            }
            subscribers.insert(id, tx.clone());
        }
        (id, rx)
    }

    pub async fn subscribe(&self) -> broadcast::Receiver<WhatsAppLinkEvent> {
        let (_, rx) = self.subscribe_with_id().await;
        rx
    }

    pub async fn unsubscribe(&self, subscriber_id: u64) {
        let mut subscribers = self.subscribers.lock().await;
        subscribers.remove(&subscriber_id);
    }

    #[cfg(test)]
    pub async fn subscriber_count(&self) -> usize {
        self.subscribers.lock().await.len()
    }

    pub async fn attach_sidecar_process(&self, child: Child) -> Result<()> {
        let (mut incoming, mut previous) = {
            let mut inner = self.inner.lock().await;
            if inner.stopping {
                (Some(child), None)
            } else {
                let previous = inner.process.take();
                inner.process = Some(child);
                (None, previous)
            }
        };

        if let Some(ref mut process) = incoming {
            process
                .kill()
                .await
                .context("failed to discard whatsapp link sidecar while stop is in progress")?;
        }

        if let Some(ref mut process) = previous {
            process
                .kill()
                .await
                .context("failed to replace existing whatsapp link sidecar process")?;
        }

        Ok(())
    }

    pub async fn broadcast_qr(&self, ascii_qr: String, expires_at_ms: Option<u64>) {
        let should_emit = {
            let mut inner = self.inner.lock().await;
            if inner.active_qr.as_deref() == Some(ascii_qr.as_str()) {
                false
            } else {
                inner.active_qr = Some(ascii_qr.clone());
                inner.active_qr_expires_at_ms = expires_at_ms;
                inner.state = WhatsAppLinkState::QrReady;
                inner.last_error = None;
                true
            }
        };
        if should_emit {
            self.broadcast_event(WhatsAppLinkEvent::Qr {
                ascii_qr,
                expires_at_ms,
            })
            .await;
            self.broadcast_status().await;
        }
    }

    pub async fn broadcast_linked(&self, phone: Option<String>) {
        {
            let mut inner = self.inner.lock().await;
            inner.state = WhatsAppLinkState::Connected;
            inner.last_error = None;
            inner.phone = phone.clone();
            inner.active_qr = None;
            inner.active_qr_expires_at_ms = None;
        }
        self.broadcast_event(WhatsAppLinkEvent::Linked { phone })
            .await;
        self.broadcast_status().await;
    }

    pub async fn broadcast_error(&self, message: String, recoverable: bool) {
        {
            let mut inner = self.inner.lock().await;
            inner.state = WhatsAppLinkState::Error;
            inner.last_error = Some(message.clone());
            if recoverable {
                inner.retry_count = inner.retry_count.saturating_add(1);
                inner.last_retry_at_ms = Some(now_millis());
            }
        }
        self.broadcast_event(WhatsAppLinkEvent::Error {
            message,
            recoverable,
        })
        .await;
        self.broadcast_status().await;
    }

    pub async fn broadcast_disconnected(&self, reason: Option<String>) {
        {
            let mut inner = self.inner.lock().await;
            inner.state = WhatsAppLinkState::Disconnected;
            inner.phone = None;
            inner.active_qr = None;
            inner.active_qr_expires_at_ms = None;
        }
        self.broadcast_event(WhatsAppLinkEvent::Disconnected { reason })
            .await;
    }

    async fn broadcast_status(&self) {
        let snapshot = self.status_snapshot().await;
        self.broadcast_event(WhatsAppLinkEvent::Status(snapshot))
            .await;
    }

    async fn broadcast_event(&self, event: WhatsAppLinkEvent) {
        let mut subscribers = self.subscribers.lock().await;
        subscribers.retain(|_, tx| tx.send(event.clone()).is_ok());
    }
}

fn now_millis() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

const GPU_NOISE_PATTERNS: [&str; 6] = [
    "gpu process",
    "gpu_process_host",
    "viz_main_impl",
    "passthrough is not supported",
    "gl_display",
    "angle_platform_impl",
];

pub fn normalize_sidecar_stderr(stderr: &str) -> Option<String> {
    let actionable: Vec<String> = stderr
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .filter(|line| {
            let lower = line.to_ascii_lowercase();
            !GPU_NOISE_PATTERNS
                .iter()
                .any(|pattern| lower.contains(pattern))
        })
        .map(ToString::to_string)
        .collect();
    if actionable.is_empty() {
        None
    } else {
        Some(actionable.join("\n"))
    }
}

pub fn build_sidecar_launch_spec(node_bin: &str, bridge_path: &Path) -> Result<SidecarLaunchSpec> {
    let program = node_bin.trim();
    if program.is_empty() {
        bail!("node binary path is required");
    }
    let launcher = std::path::Path::new(program)
        .file_name()
        .and_then(|name| name.to_str())
        .map(|name| name.to_ascii_lowercase())
        .unwrap_or_default();
    let node_compatible = matches!(
        launcher.as_str(),
        "node" | "node.exe" | "electron" | "electron.exe"
    );
    if !node_compatible {
        bail!("whatsapp sidecar launcher must be node-compatible (node/electron), got: {program}");
    }
    let bridge = bridge_path.to_string_lossy().to_string();
    if bridge.trim().is_empty() {
        bail!("bridge path is required");
    }
    let cjs_bridge = bridge_path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.eq_ignore_ascii_case("cjs"))
        .unwrap_or(false);
    if !cjs_bridge {
        bail!("whatsapp bridge entrypoint must be a .cjs file for ESM-safe startup");
    }
    let mut env = HashMap::new();
    env.insert("ELECTRON_RUN_AS_NODE".to_string(), "1".to_string());
    Ok(SidecarLaunchSpec {
        program: program.to_string(),
        args: vec![bridge],
        env,
    })
}

pub async fn spawn_sidecar(spec: &SidecarLaunchSpec) -> Result<Child> {
    let mut cmd = Command::new(&spec.program);
    cmd.args(&spec.args)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    for (k, v) in &spec.env {
        cmd.env(k, v);
    }
    cmd.spawn()
        .with_context(|| format!("failed to spawn whatsapp sidecar: {}", spec.program))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    use tokio::time::{timeout, Duration};

    async fn recv_until_qr(rx: &mut broadcast::Receiver<WhatsAppLinkEvent>) -> Option<String> {
        for _ in 0..8 {
            if let Ok(Ok(WhatsAppLinkEvent::Qr { ascii_qr, .. })) =
                timeout(Duration::from_millis(250), rx.recv()).await
            {
                return Some(ascii_qr);
            }
        }
        None
    }

    async fn recv_until_linked(
        rx: &mut broadcast::Receiver<WhatsAppLinkEvent>,
    ) -> Option<Option<String>> {
        for _ in 0..8 {
            if let Ok(Ok(WhatsAppLinkEvent::Linked { phone })) =
                timeout(Duration::from_millis(250), rx.recv()).await
            {
                return Some(phone);
            }
        }
        None
    }

    async fn recv_until_disconnected(
        rx: &mut broadcast::Receiver<WhatsAppLinkEvent>,
    ) -> Option<Option<String>> {
        for _ in 0..10 {
            if let Ok(Ok(WhatsAppLinkEvent::Disconnected { reason })) =
                timeout(Duration::from_millis(250), rx.recv()).await
            {
                return Some(reason);
            }
        }
        None
    }

    #[tokio::test]
    async fn start_to_qr_ready_emits_qr_event() {
        let runtime = WhatsAppLinkRuntime::new();
        let mut rx = runtime.subscribe().await;
        runtime.start().await.expect("start should succeed");
        runtime.broadcast_qr("QR-1".to_string(), Some(111)).await;
        assert_eq!(recv_until_qr(&mut rx).await.as_deref(), Some("QR-1"));
    }

    #[tokio::test]
    async fn qr_refresh_replaces_stale_qr_without_duplicate_payload() {
        let runtime = WhatsAppLinkRuntime::new();
        let mut rx = runtime.subscribe().await;
        runtime.start().await.expect("start should succeed");
        runtime.broadcast_qr("QR-1".to_string(), Some(111)).await;
        runtime.broadcast_qr("QR-1".to_string(), Some(111)).await;
        runtime.broadcast_qr("QR-2".to_string(), Some(222)).await;

        let mut payloads = Vec::new();
        for _ in 0..10 {
            if let Ok(Ok(WhatsAppLinkEvent::Qr { ascii_qr, .. })) =
                timeout(Duration::from_millis(150), rx.recv()).await
            {
                payloads.push(ascii_qr);
            }
        }
        assert_eq!(payloads, vec!["QR-1".to_string(), "QR-2".to_string()]);
    }

    #[tokio::test]
    async fn reset_clears_runtime_state() {
        let runtime = WhatsAppLinkRuntime::new();
        runtime.start().await.expect("start should succeed");
        runtime.broadcast_qr("QR-RESET".to_string(), Some(111)).await;
        runtime
            .broadcast_linked(Some("+15551234567".to_string()))
            .await;
        runtime.reset().await.expect("reset should succeed");

        let snapshot = runtime.status_snapshot().await;
        assert_eq!(snapshot.state, "disconnected");
        assert!(snapshot.phone.is_none());
        assert!(snapshot.last_error.is_none());

        let (_, mut rx) = runtime.subscribe_with_id().await;
        assert!(
            recv_until_qr(&mut rx).await.is_none(),
            "reset should clear replayable QR state"
        );
    }

    #[tokio::test]
    async fn persisted_provider_state_round_trips_through_history_helpers() {
        let root = tempdir().expect("tempdir");
        let history = HistoryStore::new_test_store(root.path())
            .await
            .expect("history store");
        let state = transport::PersistedState {
            linked_phone: Some("+15557654321".to_string()),
            auth_json: Some("{\"session\":true}".to_string()),
            metadata_json: Some("{\"jid\":\"123\"}".to_string()),
            last_reset_at: Some(12),
            last_linked_at: Some(34),
            updated_at: 56,
        };

        save_persisted_provider_state(&history, WHATSAPP_LINK_PROVIDER_ID, state.clone())
            .await
            .expect("save state");
        let loaded = load_persisted_provider_state(&history, WHATSAPP_LINK_PROVIDER_ID)
            .await
            .expect("load state");
        assert_eq!(loaded, Some(state));

        clear_persisted_provider_state(&history, WHATSAPP_LINK_PROVIDER_ID)
            .await
            .expect("clear state");
        assert!(
            load_persisted_provider_state(&history, WHATSAPP_LINK_PROVIDER_ID)
                .await
                .expect("load cleared state")
                .is_none()
        );
    }

    #[test]
    fn merge_persisted_state_update_preserves_existing_auth_and_metadata() {
        let merged = merge_persisted_state_update(
            Some(transport::PersistedState {
                linked_phone: Some("+15550000010".to_string()),
                auth_json: Some("{\"existing\":true}".to_string()),
                metadata_json: Some("{\"device\":\"a\"}".to_string()),
                last_reset_at: Some(7),
                last_linked_at: Some(8),
                updated_at: 9,
            }),
            transport::SessionUpdate {
                linked_phone: Some("+15550000011".to_string()),
                auth_json: None,
                metadata_json: Some("{\"device\":\"b\"}".to_string()),
                linked_at: Some(12),
                updated_at: 13,
            },
        );

        assert_eq!(merged.linked_phone.as_deref(), Some("+15550000011"));
        assert_eq!(merged.auth_json.as_deref(), Some("{\"existing\":true}"));
        assert_eq!(merged.metadata_json.as_deref(), Some("{\"device\":\"b\"}"));
        assert_eq!(merged.last_reset_at, Some(7));
        assert_eq!(merged.last_linked_at, Some(12));
        assert_eq!(merged.updated_at, 13);
    }

    #[tokio::test]
    async fn persist_transport_session_update_merges_and_saves_state() {
        let root = tempdir().expect("tempdir");
        let history = HistoryStore::new_test_store(root.path())
            .await
            .expect("history store");

        save_persisted_provider_state(
            &history,
            WHATSAPP_LINK_PROVIDER_ID,
            transport::PersistedState {
                linked_phone: Some("+15550000020".to_string()),
                auth_json: Some("{\"existing\":true}".to_string()),
                metadata_json: Some("{\"device\":\"a\"}".to_string()),
                last_reset_at: Some(1),
                last_linked_at: Some(2),
                updated_at: 3,
            },
        )
        .await
        .expect("seed state");

        let merged = persist_transport_session_update(
            &history,
            WHATSAPP_LINK_PROVIDER_ID,
            transport::SessionUpdate {
                linked_phone: Some("+15550000021".to_string()),
                auth_json: None,
                metadata_json: Some("{\"device\":\"b\"}".to_string()),
                linked_at: Some(22),
                updated_at: 23,
            },
        )
        .await
        .expect("persist merged update");

        assert_eq!(merged.linked_phone.as_deref(), Some("+15550000021"));
        assert_eq!(merged.auth_json.as_deref(), Some("{\"existing\":true}"));
        assert_eq!(merged.metadata_json.as_deref(), Some("{\"device\":\"b\"}"));
        assert_eq!(merged.last_reset_at, Some(1));
        assert_eq!(merged.last_linked_at, Some(22));
        assert_eq!(merged.updated_at, 23);
        assert_eq!(
            load_persisted_provider_state(&history, WHATSAPP_LINK_PROVIDER_ID)
                .await
                .expect("load merged state"),
            Some(merged)
        );
    }

    #[tokio::test]
    async fn connected_emits_linked_and_updates_snapshot() {
        let runtime = WhatsAppLinkRuntime::new();
        let mut rx = runtime.subscribe().await;
        runtime.start().await.expect("start should succeed");
        runtime
            .broadcast_linked(Some("+123456789".to_string()))
            .await;

        assert_eq!(
            recv_until_linked(&mut rx).await.flatten().as_deref(),
            Some("+123456789")
        );
        let snapshot = runtime.status_snapshot().await;
        assert_eq!(snapshot.state, "connected");
        assert_eq!(snapshot.phone.as_deref(), Some("+123456789"));
    }

    #[tokio::test]
    async fn stop_emits_disconnected_and_clears_active_session() {
        let runtime = WhatsAppLinkRuntime::new();
        let mut rx = runtime.subscribe().await;
        runtime.start().await.expect("start should succeed");
        runtime
            .broadcast_linked(Some("+123456789".to_string()))
            .await;
        runtime
            .stop(Some("operator_cancelled".to_string()))
            .await
            .expect("stop should succeed");

        assert_eq!(
            recv_until_disconnected(&mut rx).await.flatten().as_deref(),
            Some("operator_cancelled")
        );
        let snapshot = runtime.status_snapshot().await;
        assert_eq!(snapshot.state, "disconnected");
        assert_eq!(snapshot.phone, None);
    }

    #[tokio::test]
    async fn new_subscriber_gets_immediate_latest_status_snapshot() {
        let runtime = WhatsAppLinkRuntime::new();
        runtime.start().await.expect("start should succeed");
        runtime
            .broadcast_linked(Some("+123456789".to_string()))
            .await;
        let mut rx = runtime.subscribe().await;
        let event = timeout(Duration::from_millis(250), rx.recv())
            .await
            .expect("status snapshot should arrive")
            .expect("broadcast should be open");
        match event {
            WhatsAppLinkEvent::Status(snapshot) => {
                assert_eq!(snapshot.state, "connected");
                assert_eq!(snapshot.phone.as_deref(), Some("+123456789"));
            }
            other => panic!("expected status snapshot, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn subscribe_snapshot_is_not_broadcast_to_existing_subscribers() {
        let runtime = WhatsAppLinkRuntime::new();
        runtime
            .broadcast_linked(Some("+123456789".to_string()))
            .await;

        let mut existing = runtime.subscribe().await;
        let _ = timeout(Duration::from_millis(250), existing.recv())
            .await
            .expect("initial snapshot should arrive for first subscriber")
            .expect("broadcast should be open");

        let mut newcomer = runtime.subscribe().await;
        let newcomer_event = timeout(Duration::from_millis(250), newcomer.recv())
            .await
            .expect("new subscriber should get immediate snapshot")
            .expect("broadcast should be open");
        assert!(matches!(newcomer_event, WhatsAppLinkEvent::Status(_)));

        let duplicate = timeout(Duration::from_millis(75), existing.recv()).await;
        assert!(
            duplicate.is_err(),
            "existing subscriber got duplicate status"
        );
    }

    #[tokio::test]
    async fn new_subscriber_replays_qr_after_status_snapshot() {
        let runtime = WhatsAppLinkRuntime::new();
        runtime.start().await.expect("start should succeed");
        runtime
            .broadcast_qr("QR-REPLAY".to_string(), Some(4242))
            .await;

        let mut rx = runtime.subscribe().await;
        let first = timeout(Duration::from_millis(250), rx.recv())
            .await
            .expect("first replay event should arrive")
            .expect("broadcast should be open");
        match first {
            WhatsAppLinkEvent::Status(snapshot) => assert_eq!(snapshot.state, "qr_ready"),
            other => panic!("expected status replay, got {other:?}"),
        }

        let second = timeout(Duration::from_millis(250), rx.recv())
            .await
            .expect("second replay event should arrive")
            .expect("broadcast should be open");
        match second {
            WhatsAppLinkEvent::Qr {
                ascii_qr,
                expires_at_ms,
            } => {
                assert_eq!(ascii_qr, "QR-REPLAY");
                assert_eq!(expires_at_ms, Some(4242));
            }
            other => panic!("expected qr replay, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn concurrent_broadcasts_do_not_precede_subscribe_replay_status() {
        let runtime = std::sync::Arc::new(WhatsAppLinkRuntime::new());
        runtime.start().await.expect("start should succeed");

        let broadcaster = {
            let runtime = runtime.clone();
            tokio::spawn(async move {
                for i in 0..64 {
                    runtime.broadcast_error(format!("err-{i}"), true).await;
                    tokio::task::yield_now().await;
                }
            })
        };

        let mut rx = runtime.subscribe().await;
        let first = timeout(Duration::from_millis(250), rx.recv())
            .await
            .expect("first replay event should arrive")
            .expect("broadcast should be open");
        assert!(
            matches!(first, WhatsAppLinkEvent::Status(_)),
            "first event for a new subscriber must be status replay"
        );

        broadcaster.await.expect("broadcaster should join");
    }

    #[tokio::test]
    async fn error_event_updates_snapshot_state_and_payload() {
        let runtime = WhatsAppLinkRuntime::new();
        let mut rx = runtime.subscribe().await;
        let _ = timeout(Duration::from_millis(250), rx.recv())
            .await
            .expect("initial snapshot should arrive")
            .expect("broadcast should be open");

        runtime
            .broadcast_error("socket timeout".to_string(), true)
            .await;

        let error_event = timeout(Duration::from_millis(250), rx.recv())
            .await
            .expect("error event should arrive")
            .expect("broadcast should be open");
        match error_event {
            WhatsAppLinkEvent::Error {
                message,
                recoverable,
            } => {
                assert_eq!(message, "socket timeout");
                assert!(recoverable);
            }
            other => panic!("expected error event, got {other:?}"),
        }

        let status_event = timeout(Duration::from_millis(250), rx.recv())
            .await
            .expect("status event should arrive")
            .expect("broadcast should be open");
        match status_event {
            WhatsAppLinkEvent::Status(snapshot) => {
                assert_eq!(snapshot.state, "error");
                assert_eq!(snapshot.last_error.as_deref(), Some("socket timeout"));
            }
            other => panic!("expected status event, got {other:?}"),
        }

        let snapshot = runtime.status_snapshot().await;
        assert_eq!(snapshot.state, "error");
        assert_eq!(snapshot.last_error.as_deref(), Some("socket timeout"));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn attach_sidecar_process_discards_incoming_child_during_stop_window() {
        let runtime = WhatsAppLinkRuntime::new();
        {
            let mut inner = runtime.inner.lock().await;
            inner.stopping = true;
        }

        let child = Command::new("sh")
            .arg("-c")
            .arg("sleep 10")
            .spawn()
            .expect("sleep process should spawn");
        let child_pid = child.id().expect("sleep process pid should be available");

        runtime
            .attach_sidecar_process(child)
            .await
            .expect("attach should discard process while stop is active");

        {
            let inner = runtime.inner.lock().await;
            assert!(
                inner.process.is_none(),
                "process handle should not be retained while stop is active"
            );
        }

        let proc_path = std::path::PathBuf::from(format!("/proc/{child_pid}"));
        for _ in 0..10 {
            if !proc_path.exists() {
                return;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        assert!(
            !proc_path.exists(),
            "discarded child process should be terminated"
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn stop_kill_failure_emits_error_without_disconnected_and_preserves_process() {
        let runtime = WhatsAppLinkRuntime::new();
        runtime.start().await.expect("start should succeed");
        runtime
            .broadcast_linked(Some("+123456789".to_string()))
            .await;
        let mut rx = runtime.subscribe().await;
        let _ = timeout(Duration::from_millis(250), rx.recv())
            .await
            .expect("initial status snapshot should arrive")
            .expect("broadcast should be open");

        let child = Command::new("sh")
            .arg("-c")
            .arg("sleep 10")
            .spawn()
            .expect("sleep process should spawn");
        let expected_pid = child.id().expect("sleep process pid should be available");
        {
            let mut inner = runtime.inner.lock().await;
            inner.process = Some(child);
            inner.forced_stop_kill_error = Some("forced kill failure".to_string());
        }

        let err = runtime
            .stop(Some("operator_cancelled".to_string()))
            .await
            .expect_err("stop should fail when sidecar kill fails");
        assert!(
            err.to_string().contains("forced kill failure"),
            "unexpected stop error: {err}"
        );

        let error_event = timeout(Duration::from_millis(250), rx.recv())
            .await
            .expect("error event should arrive")
            .expect("broadcast should be open");
        match error_event {
            WhatsAppLinkEvent::Error {
                message,
                recoverable,
            } => {
                assert_eq!(message, "forced kill failure");
                assert!(!recoverable);
            }
            other => panic!("expected error event, got {other:?}"),
        }

        let status_event = timeout(Duration::from_millis(250), rx.recv())
            .await
            .expect("status event should arrive")
            .expect("broadcast should be open");
        match status_event {
            WhatsAppLinkEvent::Status(snapshot) => {
                assert_eq!(snapshot.state, "error");
                assert_eq!(snapshot.phone.as_deref(), Some("+123456789"));
                assert_eq!(snapshot.last_error.as_deref(), Some("forced kill failure"));
            }
            other => panic!("expected status event, got {other:?}"),
        }

        let disconnected = timeout(Duration::from_millis(100), rx.recv()).await;
        assert!(
            disconnected.is_err(),
            "disconnected event should not be emitted on kill failure"
        );

        let snapshot = runtime.status_snapshot().await;
        assert_eq!(snapshot.state, "error");
        assert_eq!(snapshot.phone.as_deref(), Some("+123456789"));
        assert_eq!(snapshot.last_error.as_deref(), Some("forced kill failure"));

        let mut retained = {
            let mut inner = runtime.inner.lock().await;
            assert!(!inner.stopping, "runtime should clear stopping flag");
            inner
                .process
                .take()
                .expect("process handle should be retained after kill failure")
        };
        assert_eq!(
            retained
                .id()
                .expect("retained process should still have pid"),
            expected_pid
        );
        retained
            .kill()
            .await
            .expect("retained process should be killable during cleanup");
    }

    #[test]
    fn sidecar_stderr_normalization_strips_gpu_noise_only_lines_and_keeps_actionable_errors() {
        let gpu_noise_only = "[1234:ERROR:gpu_process_host.cc(991)] GPU process launch failed\n";
        assert_eq!(normalize_sidecar_stderr(gpu_noise_only), None);

        let mixed = "[1234:ERROR:gpu_process_host.cc(991)] GPU process launch failed\nERR_REQUIRE_ESM: require() of ES Module not supported\n";
        assert_eq!(
            normalize_sidecar_stderr(mixed),
            Some("ERR_REQUIRE_ESM: require() of ES Module not supported".to_string())
        );
    }

    #[test]
    fn sidecar_launcher_enforces_node_mode_and_esm_safe_bridge_startup_behavior() {
        let spec =
            build_sidecar_launch_spec("node", Path::new("frontend/electron/whatsapp-bridge.cjs"))
                .expect("launch spec should be generated");
        assert_eq!(spec.program, "node");
        assert_eq!(spec.args, vec!["frontend/electron/whatsapp-bridge.cjs"]);
        assert_eq!(spec.env.get("ELECTRON_RUN_AS_NODE"), Some(&"1".to_string()));
    }

    #[test]
    fn sidecar_launcher_rejects_non_node_compatible_programs() {
        let err =
            build_sidecar_launch_spec("python", Path::new("frontend/electron/whatsapp-bridge.cjs"))
                .expect_err("non-node-compatible launchers must be rejected");
        assert!(
            err.to_string().contains("node-compatible"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn sidecar_launcher_rejects_non_cjs_entrypoints() {
        let err =
            build_sidecar_launch_spec("node", Path::new("frontend/electron/whatsapp-bridge.mjs"))
                .expect_err("non-cjs bridge paths must be rejected");
        assert!(err.to_string().contains(".cjs"), "unexpected error: {err}");
    }
}
