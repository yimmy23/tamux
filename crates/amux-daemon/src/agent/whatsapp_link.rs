use anyhow::{bail, Context, Result};
use std::collections::{HashMap, HashSet, VecDeque};
use std::future::Future;
use std::path::Path;
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::io::AsyncWriteExt;
use tokio::process::{Child, Command};
use tokio::sync::{broadcast, Mutex};
use tokio::task::JoinHandle;

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

pub fn normalize_jid_user(jid: &str) -> String {
    let trimmed = jid.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    let with_device = match trimmed.split_once('@') {
        Some((user, _)) => user,
        None => trimmed,
    };
    match with_device.split_once(':') {
        Some((user, _)) => user.trim().to_string(),
        None => with_device.trim().to_string(),
    }
}

pub fn normalize_identifier(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    let jid_user = normalize_jid_user(trimmed);
    if !jid_user.is_empty() {
        return jid_user.trim_start_matches('+').to_string();
    }
    trimmed.trim_start_matches('+').to_string()
}

pub fn collect_normalized_identifiers(values: &[&str]) -> HashSet<String> {
    let mut ids = HashSet::new();
    for value in values {
        let normalized = normalize_identifier(value);
        if !normalized.is_empty() {
            ids.insert(normalized);
        }
    }
    ids
}

pub fn collect_exact_jid_candidates(values: &[&str]) -> Vec<String> {
    let mut targets = Vec::new();
    for value in values {
        push_unique_target(&mut targets, value);
    }
    targets
}

pub fn resolve_send_target_candidates(
    requested: &str,
    own_identifiers: &HashSet<String>,
    own_phone: Option<&str>,
    own_exact_jids: &[String],
) -> Vec<String> {
    let requested = requested.trim();
    if requested.is_empty() {
        return Vec::new();
    }

    let requested_user = normalize_identifier(requested);
    let own_phone = own_phone.map(normalize_identifier).unwrap_or_default();
    let is_self_target = !requested_user.is_empty() && own_identifiers.contains(&requested_user);
    let mut targets = Vec::new();

    if is_self_target {
        for own_jid in own_exact_jids {
            push_unique_target(&mut targets, own_jid);
        }
    }

    push_unique_target(&mut targets, requested);

    if is_self_target && !own_phone.is_empty() {
        push_unique_target(&mut targets, &format!("{own_phone}@s.whatsapp.net"));
    }

    if requested.ends_with("@lid") {
        let lid_user = normalize_jid_user(requested);
        if lid_user.chars().all(|ch| ch.is_ascii_digit()) && lid_user.len() >= 6 {
            push_unique_target(&mut targets, &format!("{lid_user}@s.whatsapp.net"));
        }
    }

    if requested.ends_with("@s.whatsapp.net") {
        let pn_user = normalize_jid_user(requested);
        if pn_user.chars().all(|ch| ch.is_ascii_digit()) && pn_user.len() >= 6 {
            push_unique_target(&mut targets, &format!("{pn_user}@lid"));
        }
    }

    targets
}

fn resolve_native_send_plan(requested: &str, own_pn: &str, own_lid: &str) -> (Vec<String>, bool) {
    let own_identifiers = collect_normalized_identifiers(&[own_pn, own_lid]);
    let requested_user = normalize_identifier(requested);
    let prefix_self_chat = !requested_user.is_empty() && own_identifiers.contains(&requested_user);
    let targets = resolve_send_target_candidates(requested, &HashSet::new(), None, &[]);
    (targets, prefix_self_chat)
}

fn push_unique_target(targets: &mut Vec<String>, value: &str) {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return;
    }
    if !targets.iter().any(|target| target == trimmed) {
        targets.push(trimmed.to_string());
    }
}

fn looks_like_raw_pairing_qr_payload(payload: &str) -> bool {
    let trimmed = payload.trim();
    if trimmed.is_empty() || trimmed.contains('\n') || trimmed.contains('\r') {
        return false;
    }

    let mut parts = trimmed.split(',');
    let Some(first) = parts.next() else {
        return false;
    };
    let Some(second) = parts.next() else {
        return false;
    };
    let Some(third) = parts.next() else {
        return false;
    };
    let Some(fourth) = parts.next() else {
        return false;
    };

    parts.next().is_none()
        && !first.trim().is_empty()
        && !second.trim().is_empty()
        && !third.trim().is_empty()
        && !fourth.trim().is_empty()
}

fn render_pairing_qr_payload(payload: &str) -> Result<String> {
    let code = qrcode::QrCode::new(payload.as_bytes())
        .context("failed to encode whatsapp pairing payload as QR")?;
    Ok(code
        .render::<qrcode::render::unicode::Dense1x2>()
        .quiet_zone(false)
        .build())
}

#[path = "whatsapp_link/persistence.rs"]
mod persistence_impl;
#[path = "whatsapp_link/runtime_core.rs"]
mod runtime_core;
#[path = "whatsapp_link/runtime_events.rs"]
mod runtime_events;
#[path = "whatsapp_link/sidecar.rs"]
mod sidecar_impl;

pub use persistence_impl::{
    apply_transport_event, clear_persisted_provider_state, load_persisted_provider_state,
    merge_persisted_state_update, persist_transport_session_update, save_persisted_provider_state,
    spawn_transport_event_bridge, start_transport_bridge,
};
pub(super) use sidecar_impl::now_millis;
pub use sidecar_impl::{build_sidecar_launch_spec, normalize_sidecar_stderr};

#[allow(dead_code)]
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
            let _ = self.tx.send(WhatsAppTransportEvent::SessionUpdated(update));
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
                let _ = self
                    .tx
                    .send(WhatsAppTransportEvent::Disconnected { reason });
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

            transport.emit_qr("QR-TRANSPORT", Some(42)).await;
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

struct RuntimeInner {
    state: WhatsAppLinkState,
    phone: Option<String>,
    last_error: Option<String>,
    active_qr: Option<String>,
    active_qr_expires_at_ms: Option<u64>,
    process: Option<Child>,
    native_client: Option<Arc<wa_rs::Client>>,
    native_task: Option<JoinHandle<()>>,
    stopping: bool,
    retry_count: u32,
    last_retry_at_ms: Option<u64>,
    recent_outbound_message_ids: VecDeque<(String, u64)>,
    next_rpc_id: u64,
    #[cfg(test)]
    forced_stop_kill_error: Option<String>,
}

pub struct WhatsAppLinkRuntime {
    inner: Mutex<RuntimeInner>,
    subscribers: Mutex<HashMap<u64, broadcast::Sender<WhatsAppLinkEvent>>>,
    next_subscriber_id: AtomicU64,
}

#[cfg(test)]
#[path = "tests/whatsapp_link/mod.rs"]
mod tests;
