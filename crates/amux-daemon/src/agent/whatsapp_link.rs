use anyhow::{bail, Context, Result};
use std::collections::HashMap;
use std::path::Path;
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
    process: Option<Child>,
    retry_count: u32,
    last_retry_at_ms: Option<u64>,
}

pub struct WhatsAppLinkRuntime {
    inner: Mutex<RuntimeInner>,
    events_tx: broadcast::Sender<WhatsAppLinkEvent>,
}

impl Default for WhatsAppLinkRuntime {
    fn default() -> Self {
        Self::new()
    }
}

impl WhatsAppLinkRuntime {
    pub fn new() -> Self {
        let (events_tx, _) = broadcast::channel(64);
        Self {
            inner: Mutex::new(RuntimeInner {
                state: WhatsAppLinkState::Disconnected,
                phone: None,
                last_error: None,
                active_qr: None,
                process: None,
                retry_count: 0,
                last_retry_at_ms: None,
            }),
            events_tx,
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
        let mut child = {
            let mut inner = self.inner.lock().await;
            inner.retry_count = 0;
            inner.last_retry_at_ms = None;
            inner.last_error = None;
            inner.active_qr = None;
            inner.phone = None;
            inner.process.take()
        };

        if let Some(ref mut proc) = child {
            proc.kill()
                .await
                .context("failed to stop whatsapp link sidecar process")?;
        }

        {
            let mut inner = self.inner.lock().await;
            inner.state = WhatsAppLinkState::Disconnected;
            inner.process = None;
        }

        self.broadcast_disconnected(reason).await;
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

    pub async fn subscribe(&self) -> broadcast::Receiver<WhatsAppLinkEvent> {
        let rx = self.events_tx.subscribe();
        self.broadcast_status().await;
        rx
    }

    pub fn unsubscribe(&self, rx: broadcast::Receiver<WhatsAppLinkEvent>) {
        drop(rx);
    }

    pub async fn attach_sidecar_process(&self, child: Child) {
        let mut inner = self.inner.lock().await;
        inner.process = Some(child);
    }

    pub async fn broadcast_qr(&self, ascii_qr: String, expires_at_ms: Option<u64>) {
        let should_emit = {
            let mut inner = self.inner.lock().await;
            if inner.active_qr.as_deref() == Some(ascii_qr.as_str()) {
                false
            } else {
                inner.active_qr = Some(ascii_qr.clone());
                inner.state = WhatsAppLinkState::QrReady;
                inner.last_error = None;
                true
            }
        };
        if should_emit {
            let _ = self.events_tx.send(WhatsAppLinkEvent::Qr {
                ascii_qr,
                expires_at_ms,
            });
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
        }
        let _ = self.events_tx.send(WhatsAppLinkEvent::Linked { phone });
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
        let _ = self.events_tx.send(WhatsAppLinkEvent::Error {
            message,
            recoverable,
        });
        self.broadcast_status().await;
    }

    pub async fn broadcast_disconnected(&self, reason: Option<String>) {
        {
            let mut inner = self.inner.lock().await;
            inner.state = WhatsAppLinkState::Disconnected;
            inner.phone = None;
            inner.active_qr = None;
        }
        let _ = self
            .events_tx
            .send(WhatsAppLinkEvent::Disconnected { reason });
    }

    async fn broadcast_status(&self) {
        let snapshot = self.status_snapshot().await;
        let _ = self.events_tx.send(WhatsAppLinkEvent::Status(snapshot));
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
    let bridge = bridge_path.to_string_lossy().to_string();
    if bridge.trim().is_empty() {
        bail!("bridge path is required");
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
        runtime.broadcast_linked(Some("+123456789".to_string())).await;

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
        runtime.broadcast_linked(Some("+123456789".to_string())).await;
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
        runtime.broadcast_linked(Some("+123456789".to_string())).await;
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
        let spec = build_sidecar_launch_spec(
            "node",
            Path::new("frontend/electron/whatsapp-bridge.cjs"),
        )
        .expect("launch spec should be generated");
        assert_eq!(spec.program, "node");
        assert_eq!(spec.args, vec!["frontend/electron/whatsapp-bridge.cjs"]);
        assert_eq!(spec.env.get("ELECTRON_RUN_AS_NODE"), Some(&"1".to_string()));
    }
}
