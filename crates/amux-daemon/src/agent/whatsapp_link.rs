use anyhow::{bail, Context, Result};
use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::io::AsyncWriteExt;
use tokio::process::{Child, Command};
use tokio::sync::{broadcast, Mutex};

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
    next_rpc_id: u64,
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
                next_rpc_id: 1,
                #[cfg(test)]
                forced_stop_kill_error: None,
            }),
            subscribers: Mutex::new(HashMap::new()),
            next_subscriber_id: AtomicU64::new(1),
        }
    }

    pub async fn start(&self) -> Result<()> {
        let should_broadcast = {
            let mut inner = self.inner.lock().await;
            if inner.stopping {
                bail!("whatsapp link sidecar is stopping");
            }
            if matches!(
                inner.state,
                WhatsAppLinkState::Starting
                    | WhatsAppLinkState::QrReady
                    | WhatsAppLinkState::Connected
            ) {
                false
            } else {
                inner.state = WhatsAppLinkState::Starting;
                inner.last_error = None;
                true
            }
        };
        if should_broadcast {
            self.broadcast_status().await;
        }
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

    pub async fn has_sidecar_process(&self) -> bool {
        let inner = self.inner.lock().await;
        inner.process.is_some()
    }

    pub async fn connect_sidecar(&self) -> Result<()> {
        self.send_sidecar_command("connect", serde_json::json!({}))
            .await
    }

    pub async fn send_message(&self, jid: &str, text: &str) -> Result<()> {
        let jid = jid.trim();
        if jid.is_empty() {
            bail!("whatsapp send target is empty");
        }
        if text.trim().is_empty() {
            bail!("whatsapp message body is empty");
        }
        self.send_sidecar_command("send", serde_json::json!({"jid": jid, "text": text}))
            .await
    }

    async fn send_sidecar_command(&self, method: &str, params: serde_json::Value) -> Result<()> {
        let mut inner = self.inner.lock().await;
        if inner.stopping {
            bail!("whatsapp link sidecar is stopping");
        }
        if method == "send" && inner.state != WhatsAppLinkState::Connected {
            bail!("whatsapp link is not connected");
        }
        let rpc_id = inner.next_rpc_id;
        inner.next_rpc_id = inner.next_rpc_id.saturating_add(1);
        let payload = serde_json::json!({
            "id": rpc_id,
            "method": method,
            "params": params,
        });
        let line = format!("{payload}\n");
        let process = inner
            .process
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("whatsapp link sidecar is not running"))?;
        let stdin = process
            .stdin
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("whatsapp link sidecar stdin unavailable"))?;
        stdin
            .write_all(line.as_bytes())
            .await
            .context("failed to write command to whatsapp link sidecar stdin")?;
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
            inner.last_error = Some(message.clone());
            if recoverable {
                // Recoverable sidecar errors (e.g. transient decrypt/session warnings)
                // should not tear down a live connected transport.
                if inner.state != WhatsAppLinkState::Connected {
                    inner.state = WhatsAppLinkState::Error;
                }
                inner.retry_count = inner.retry_count.saturating_add(1);
                inner.last_retry_at_ms = Some(now_millis());
            } else {
                inner.state = WhatsAppLinkState::Error;
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

const SENSITIVE_SIDECAR_PATTERNS: [&str; 21] = [
    "closing session: sessionentry",
    "_chains:",
    "currentratchet:",
    "ephemeralkeypair:",
    "lastremoteephemeralkey:",
    "rootkey:",
    "indexinfo:",
    "pendingprekey:",
    "remoteidentitykey:",
    "privkey:",
    "pubkey:",
    "registrationid:",
    "basekey:",
    "basekeytype:",
    "signedkeyid:",
    "prekeyid:",
    "previouscounter:",
    "used:",
    "created:",
    "chainkey:",
    "<buffer ",
];

pub fn normalize_sidecar_stderr(stderr: &str) -> Option<String> {
    let actionable: Vec<String> = stderr
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .filter(|line| {
            let lower = line.to_ascii_lowercase();
            let gpu_noise = GPU_NOISE_PATTERNS
                .iter()
                .any(|pattern| lower.contains(pattern));
            let sensitive_dump = SENSITIVE_SIDECAR_PATTERNS
                .iter()
                .any(|pattern| lower.contains(pattern));
            let structural_only = matches!(lower.as_str(), "{" | "}" | "},");
            !(gpu_noise || sensitive_dump || structural_only)
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

    #[tokio::test]
    async fn recoverable_error_while_connected_keeps_connected_state() {
        let runtime = WhatsAppLinkRuntime::new();
        let mut rx = runtime.subscribe().await;
        let _ = timeout(Duration::from_millis(250), rx.recv())
            .await
            .expect("initial snapshot should arrive")
            .expect("broadcast should be open");

        runtime
            .broadcast_linked(Some("+123456789".to_string()))
            .await;
        let _ = timeout(Duration::from_millis(250), rx.recv())
            .await
            .expect("linked event should arrive")
            .expect("broadcast should be open");
        let _ = timeout(Duration::from_millis(250), rx.recv())
            .await
            .expect("status event should arrive")
            .expect("broadcast should be open");

        runtime
            .broadcast_error("transient decrypt warning".to_string(), true)
            .await;

        let error_event = timeout(Duration::from_millis(250), rx.recv())
            .await
            .expect("error event should arrive")
            .expect("broadcast should be open");
        assert!(matches!(error_event, WhatsAppLinkEvent::Error { recoverable: true, .. }));

        let status_event = timeout(Duration::from_millis(250), rx.recv())
            .await
            .expect("status event should arrive")
            .expect("broadcast should be open");
        match status_event {
            WhatsAppLinkEvent::Status(snapshot) => {
                assert_eq!(snapshot.state, "connected");
                assert_eq!(
                    snapshot.last_error.as_deref(),
                    Some("transient decrypt warning")
                );
            }
            other => panic!("expected status event, got {other:?}"),
        }
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
    fn sidecar_stderr_normalization_drops_sensitive_session_dump_lines() {
        let sensitive = "[wa-sidecar:info] Closing session: SessionEntry {\ncurrentRatchet: {\nprivKey: <Buffer 01 02>\n}\n";
        assert_eq!(normalize_sidecar_stderr(sensitive), None);

        let mixed = "[wa-sidecar:warn] Decrypted message with closed session.\ncurrentRatchet: {\n";
        assert_eq!(
            normalize_sidecar_stderr(mixed),
            Some("[wa-sidecar:warn] Decrypted message with closed session.".to_string())
        );

        let noisy = "registrationId: 769524623,\nbaseKey: <Buffer 05 aa>,\n}\n";
        assert_eq!(normalize_sidecar_stderr(noisy), None);
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
