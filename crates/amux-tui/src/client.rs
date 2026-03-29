#![allow(dead_code)]

use anyhow::Result;
use futures::{SinkExt, StreamExt};
use serde_json::Value;
use std::sync::Mutex;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::{Instant, MissedTickBehavior};
use tokio_util::codec::Framed;
use tracing::{debug, error, info, warn};

#[cfg(not(unix))]
use amux_protocol::default_tcp_addr;
use amux_protocol::{AmuxCodec, ClientMessage, DaemonMessage};

use crate::wire::{
    AgentConfigSnapshot, AgentTask, AgentThread, AnticipatoryItem, CheckpointSummary, FetchedModel,
    GoalRun, GoalRunStatus, HeartbeatItem, RestoreOutcome, TaskStatus, ThreadWorkContext,
};

#[cfg(unix)]
use tokio::net::UnixStream;

#[derive(Debug, Clone)]
pub enum ClientEvent {
    Connected,
    Disconnected,
    Reconnecting {
        delay_secs: u64,
    },
    SessionSpawned {
        session_id: String,
    },

    ThreadList(Vec<AgentThread>),
    ThreadDetail(Option<AgentThread>),
    ThreadCreated {
        thread_id: String,
        title: String,
    },
    TaskList(Vec<AgentTask>),
    TaskUpdate(AgentTask),
    GoalRunList(Vec<GoalRun>),
    GoalRunStarted(GoalRun),
    GoalRunDetail(Option<GoalRun>),
    GoalRunUpdate(GoalRun),
    GoalRunCheckpoints {
        goal_run_id: String,
        checkpoints: Vec<CheckpointSummary>,
    },
    AgentExplanation(serde_json::Value),
    DivergentSessionStarted(serde_json::Value),
    DivergentSession(serde_json::Value),
    ThreadTodos {
        thread_id: String,
        items: Vec<crate::wire::TodoItem>,
    },
    WorkContext(ThreadWorkContext),
    GitDiff {
        repo_path: String,
        file_path: Option<String>,
        diff: String,
    },
    FilePreview {
        path: String,
        content: String,
        truncated: bool,
        is_text: bool,
    },
    AgentConfig(AgentConfigSnapshot),
    AgentConfigRaw(Value),
    ModelsFetched(Vec<FetchedModel>),
    HeartbeatItems(Vec<HeartbeatItem>),
    HeartbeatDigest {
        cycle_id: String,
        actionable: bool,
        digest: String,
        items: Vec<(u8, String, String, String)>,
        checked_at: u64,
        explanation: Option<String>,
    },
    AuditEntry {
        id: String,
        timestamp: u64,
        action_type: String,
        summary: String,
        explanation: Option<String>,
        confidence: Option<f64>,
        confidence_band: Option<String>,
        causal_trace_id: Option<String>,
        thread_id: Option<String>,
    },
    EscalationUpdate {
        thread_id: String,
        from_level: String,
        to_level: String,
        reason: String,
        attempts: u32,
        audit_id: Option<String>,
    },
    AnticipatoryItems(Vec<AnticipatoryItem>),
    GatewayStatus {
        platform: String,
        status: String,
        last_error: Option<String>,
        consecutive_failures: u32,
    },
    WhatsAppLinkStatus {
        state: String,
        phone: Option<String>,
        last_error: Option<String>,
    },
    WhatsAppLinkQr {
        ascii_qr: String,
        expires_at_ms: Option<u64>,
    },
    WhatsAppLinked {
        phone: Option<String>,
    },
    WhatsAppLinkError {
        message: String,
        recoverable: bool,
    },
    WhatsAppLinkDisconnected {
        reason: Option<String>,
    },

    Delta {
        thread_id: String,
        content: String,
    },
    Reasoning {
        thread_id: String,
        content: String,
    },
    ToolCall {
        thread_id: String,
        call_id: String,
        name: String,
        arguments: String,
    },
    ToolResult {
        thread_id: String,
        call_id: String,
        name: String,
        content: String,
        is_error: bool,
    },
    Done {
        thread_id: String,
        input_tokens: u64,
        output_tokens: u64,
        cost: Option<f64>,
        provider: Option<String>,
        model: Option<String>,
        tps: Option<f64>,
        generation_ms: Option<u64>,
    },
    WorkflowNotice {
        kind: String,
        message: String,
        details: Option<String>,
    },
    RetryStatus {
        thread_id: String,
        phase: String,
        attempt: u32,
        max_retries: u32,
        delay_ms: u64,
        failure_class: String,
        message: String,
    },
    ApprovalRequired {
        approval_id: String,
        command: String,
        risk_level: String,
        blast_radius: String,
    },
    ApprovalResolved {
        approval_id: String,
        decision: String,
    },

    ProviderAuthStates(Vec<crate::state::ProviderAuthEntry>),
    ProviderValidation {
        provider_id: String,
        valid: bool,
        error: Option<String>,
    },
    SubAgentList(Vec<crate::state::SubAgentEntry>),
    SubAgentUpdated(crate::state::SubAgentEntry),
    SubAgentRemoved {
        sub_agent_id: String,
    },
    ConciergeConfig(Value),

    ConciergeWelcome {
        content: String,
        actions: Vec<crate::state::ConciergeActionVm>,
    },
    ConciergeWelcomeDismissed,
    OperatorProfileSessionStarted {
        session_id: String,
        kind: String,
    },
    OperatorProfileQuestion {
        session_id: String,
        question_id: String,
        field_key: String,
        prompt: String,
        input_kind: String,
        optional: bool,
    },
    OperatorProfileProgress {
        session_id: String,
        answered: u32,
        remaining: u32,
        completion_ratio: f64,
    },
    OperatorProfileSummary {
        summary_json: String,
    },
    OperatorProfileSessionCompleted {
        session_id: String,
        updated_fields: Vec<String>,
    },
    StatusDiagnostics {
        operator_profile_sync_state: String,
        operator_profile_sync_dirty: bool,
        operator_profile_scheduler_fallback: bool,
    },

    TierChanged {
        new_tier: String,
    },

    // Plugin settings events (Plan 16-03)
    PluginList(Vec<amux_protocol::PluginInfo>),
    PluginGet {
        plugin: Option<amux_protocol::PluginInfo>,
        settings_schema: Option<String>,
    },
    PluginSettings {
        plugin_name: String,
        settings: Vec<(String, String, bool)>,
    },
    PluginTestConnection {
        plugin_name: String,
        success: bool,
        message: String,
    },
    PluginAction {
        success: bool,
        message: String,
    },
    PluginCommands(Vec<amux_protocol::PluginCommandInfo>),
    PluginOAuthUrl {
        name: String,
        url: String,
    },
    PluginOAuthComplete {
        name: String,
        success: bool,
        error: Option<String>,
    },

    Error(String),
}

pub struct DaemonClient {
    event_tx: mpsc::Sender<ClientEvent>,
    request_tx: mpsc::UnboundedSender<ClientMessage>,
    request_rx: Mutex<Option<mpsc::UnboundedReceiver<ClientMessage>>>,
}

impl DaemonClient {
    pub fn new(event_tx: mpsc::Sender<ClientEvent>) -> Self {
        let (request_tx, request_rx) = mpsc::unbounded_channel();
        Self {
            event_tx,
            request_tx,
            request_rx: Mutex::new(Some(request_rx)),
        }
    }

    pub async fn connect(&self) -> Result<()> {
        let event_tx = self.event_tx.clone();
        let Some(mut request_rx) = self
            .request_rx
            .lock()
            .expect("request mutex poisoned")
            .take()
        else {
            return Ok(());
        };

        tokio::spawn(async move {
            let retry_delay = Duration::from_secs(5);

            loop {
                let mut connected = false;
                #[cfg(unix)]
                {
                    for socket_path in Self::unix_socket_candidates() {
                        info!(path = %socket_path.display(), "Attempting daemon unix socket");
                        match UnixStream::connect(&socket_path).await {
                            Ok(stream) => {
                                info!("Connected to daemon via unix socket");
                                let _ = event_tx.send(ClientEvent::Connected).await;
                                let framed = Framed::new(stream, AmuxCodec);
                                Self::handle_connection(framed, event_tx.clone(), &mut request_rx)
                                    .await;
                                connected = true;
                                break;
                            }
                            Err(err) => {
                                debug!(path = %socket_path.display(), error = %err, "Unix socket connect failed");
                            }
                        }
                    }
                }

                #[cfg(not(unix))]
                {
                    let addr = Self::resolve_daemon_addr(&default_tcp_addr());
                    info!(%addr, "Attempting daemon tcp socket");
                    match tokio::net::TcpStream::connect(&addr).await {
                        Ok(stream) => {
                            info!("Connected to daemon via tcp {}", addr);
                            let _ = event_tx.send(ClientEvent::Connected).await;
                            let framed = Framed::new(stream, AmuxCodec);
                            Self::handle_connection(framed, event_tx.clone(), &mut request_rx)
                                .await;
                            connected = true;
                        }
                        Err(err) => {
                            warn!("Cannot connect to daemon at {} ({})", addr, err);
                            let _ = event_tx.send(ClientEvent::Disconnected).await;
                        }
                    }
                }

                let _ = event_tx
                    .send(ClientEvent::Reconnecting {
                        delay_secs: retry_delay.as_secs(),
                    })
                    .await;

                if connected {
                    info!(
                        "Daemon connection closed; retrying in {}s",
                        retry_delay.as_secs()
                    );
                }
                tokio::time::sleep(retry_delay).await;
            }
        });

        Ok(())
    }

    #[cfg(not(unix))]
    fn resolve_daemon_addr(default_addr: &str) -> String {
        #[cfg(target_os = "linux")]
        {
            if std::path::Path::new("/proc/sys/fs/binfmt_misc/WSLInterop").exists()
                || std::path::Path::new("/run/WSL").exists()
            {
                if let Ok(contents) = std::fs::read_to_string("/etc/resolv.conf") {
                    for line in contents.lines() {
                        if line.starts_with("nameserver") {
                            if let Some(host_ip) = line.split_whitespace().nth(1) {
                                let port = default_addr.split(':').nth(1).unwrap_or("17563");
                                return format!("{}:{}", host_ip, port);
                            }
                        }
                    }
                }
            }
        }

        default_addr.to_string()
    }

    #[cfg(unix)]
    fn unix_socket_candidates() -> Vec<std::path::PathBuf> {
        let mut candidates = Vec::new();

        if let Ok(runtime_dir) = std::env::var("XDG_RUNTIME_DIR") {
            candidates.push(std::path::PathBuf::from(runtime_dir).join("tamux-daemon.sock"));
        }

        candidates.push(std::path::PathBuf::from("/tmp").join("tamux-daemon.sock"));
        candidates.dedup();
        candidates
    }

    async fn handle_connection<S>(
        framed: Framed<S, AmuxCodec>,
        event_tx: mpsc::Sender<ClientEvent>,
        request_rx: &mut mpsc::UnboundedReceiver<ClientMessage>,
    ) where
        S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + 'static,
    {
        let (mut sink, mut stream) = framed.split();
        let keepalive_interval = Duration::from_secs(5);
        let keepalive_timeout = Duration::from_secs(10);
        let mut ping_tick = tokio::time::interval(keepalive_interval);
        ping_tick.set_missed_tick_behavior(MissedTickBehavior::Delay);
        let mut last_inbound_at = Instant::now();
        let mut awaiting_pong_since: Option<Instant> = None;

        for request in [
            ClientMessage::AgentSubscribe,
            ClientMessage::AgentListThreads,
        ] {
            if let Err(err) = sink.send(request).await {
                error!("Failed initial daemon request: {}", err);
                let _ = event_tx
                    .send(ClientEvent::Error(format!("Protocol error: {}", err)))
                    .await;
                let _ = event_tx.send(ClientEvent::Disconnected).await;
                return;
            }
        }

        loop {
            tokio::select! {
                inbound = stream.next() => {
                    match inbound {
                        Some(Ok(message)) => {
                            last_inbound_at = Instant::now();
                            awaiting_pong_since = None;
                            if !Self::handle_daemon_message(message, &event_tx).await {
                                break;
                            }
                        }
                        Some(Err(err)) => {
                            let _ = event_tx.send(ClientEvent::Error(format!("Connection error: {}", err))).await;
                            break;
                        }
                        None => break,
                    }
                }
                _ = ping_tick.tick() => {
                    let now = Instant::now();
                    if let Some(pending_since) = awaiting_pong_since {
                        if now.duration_since(pending_since) >= keepalive_timeout {
                            let _ = event_tx
                                .send(ClientEvent::Error(
                                    "Connection lost: daemon health-check timed out".to_string(),
                                ))
                                .await;
                            break;
                        }
                    }

                    if now.duration_since(last_inbound_at) >= keepalive_interval {
                        if let Err(err) = sink.send(ClientMessage::Ping).await {
                            let _ = event_tx
                                .send(ClientEvent::Error(format!("Keepalive send error: {}", err)))
                                .await;
                            break;
                        }
                        awaiting_pong_since = Some(now);
                    }
                }
                outbound = request_rx.recv() => {
                    match outbound {
                        Some(request) => {
                            if let Err(err) = sink.send(request).await {
                                let _ = event_tx.send(ClientEvent::Error(format!("Send error: {}", err))).await;
                                break;
                            }
                        }
                        None => break,
                    }
                }
            }
        }

        let _ = event_tx.send(ClientEvent::Disconnected).await;
    }

    async fn handle_daemon_message(
        message: DaemonMessage,
        event_tx: &mpsc::Sender<ClientEvent>,
    ) -> bool {
        match message {
            DaemonMessage::AgentEvent { event_json } => {
                match serde_json::from_str::<Value>(&event_json) {
                    Ok(event) => Self::dispatch_agent_event(event, event_tx).await,
                    Err(err) => warn!("Failed to parse agent event: {}", err),
                }
            }
            DaemonMessage::AgentThreadList { threads_json } => {
                match serde_json::from_str::<Vec<AgentThread>>(&threads_json) {
                    Ok(threads) => {
                        let _ = event_tx.send(ClientEvent::ThreadList(threads)).await;
                    }
                    Err(err) => warn!("Failed to parse thread list: {}", err),
                }
            }
            DaemonMessage::AgentThreadDetail { thread_json } => {
                match serde_json::from_str::<Option<AgentThread>>(&thread_json) {
                    Ok(thread) => {
                        let _ = event_tx.send(ClientEvent::ThreadDetail(thread)).await;
                    }
                    Err(err) => warn!("Failed to parse thread detail: {}", err),
                }
            }
            DaemonMessage::AgentTaskList { tasks_json } => {
                match serde_json::from_str::<Vec<AgentTask>>(&tasks_json) {
                    Ok(tasks) => {
                        let _ = event_tx.send(ClientEvent::TaskList(tasks)).await;
                    }
                    Err(err) => warn!("Failed to parse task list: {}", err),
                }
            }
            DaemonMessage::AgentGoalRunList { goal_runs_json } => {
                match serde_json::from_str::<Vec<GoalRun>>(&goal_runs_json) {
                    Ok(goal_runs) => {
                        let _ = event_tx.send(ClientEvent::GoalRunList(goal_runs)).await;
                    }
                    Err(err) => warn!("Failed to parse goal run list: {}", err),
                }
            }
            DaemonMessage::AgentGoalRunStarted { goal_run_json } => {
                match serde_json::from_str::<GoalRun>(&goal_run_json) {
                    Ok(goal_run) => {
                        let _ = event_tx.send(ClientEvent::GoalRunStarted(goal_run)).await;
                    }
                    Err(err) => warn!("Failed to parse goal run started payload: {}", err),
                }
            }
            DaemonMessage::AgentGoalRunDetail { goal_run_json } => {
                match serde_json::from_str::<Option<GoalRun>>(&goal_run_json) {
                    Ok(goal_run) => {
                        let _ = event_tx.send(ClientEvent::GoalRunDetail(goal_run)).await;
                    }
                    Err(err) => warn!("Failed to parse goal run detail: {}", err),
                }
            }
            DaemonMessage::AgentCheckpointList { checkpoints_json } => {
                match serde_json::from_str::<Vec<CheckpointSummary>>(&checkpoints_json) {
                    Ok(checkpoints) => {
                        let goal_run_id = checkpoints
                            .first()
                            .map(|checkpoint| checkpoint.goal_run_id.clone())
                            .unwrap_or_default();
                        let _ = event_tx
                            .send(ClientEvent::GoalRunCheckpoints {
                                goal_run_id,
                                checkpoints,
                            })
                            .await;
                    }
                    Err(err) => warn!("Failed to parse checkpoint list: {}", err),
                }
            }
            DaemonMessage::AgentCheckpointRestored { outcome_json } => {
                match serde_json::from_str::<RestoreOutcome>(&outcome_json) {
                    Ok(outcome) => {
                        let details = format!(
                            "goal {} at step {} • restored {} task(s)",
                            outcome.goal_run_id,
                            outcome.restored_step_index + 1,
                            outcome.tasks_restored
                        );
                        let _ = event_tx
                            .send(ClientEvent::WorkflowNotice {
                                kind: "checkpoint-restored".to_string(),
                                message: "Checkpoint restored".to_string(),
                                details: Some(details),
                            })
                            .await;
                    }
                    Err(err) => warn!("Failed to parse checkpoint restore outcome: {}", err),
                }
            }
            DaemonMessage::AgentTodoDetail {
                thread_id,
                todos_json,
            } => match serde_json::from_str::<Vec<crate::wire::TodoItem>>(&todos_json) {
                Ok(items) => {
                    let _ = event_tx
                        .send(ClientEvent::ThreadTodos { thread_id, items })
                        .await;
                }
                Err(err) => warn!("Failed to parse todo detail: {}", err),
            },
            DaemonMessage::AgentWorkContextDetail {
                thread_id: _,
                context_json,
            } => match serde_json::from_str::<ThreadWorkContext>(&context_json) {
                Ok(context) => {
                    let _ = event_tx.send(ClientEvent::WorkContext(context)).await;
                }
                Err(err) => warn!("Failed to parse work context detail: {}", err),
            },
            DaemonMessage::GitDiff {
                repo_path,
                file_path,
                diff,
            } => {
                let _ = event_tx
                    .send(ClientEvent::GitDiff {
                        repo_path,
                        file_path,
                        diff,
                    })
                    .await;
            }
            DaemonMessage::FilePreview {
                path,
                content,
                truncated,
                is_text,
            } => {
                let _ = event_tx
                    .send(ClientEvent::FilePreview {
                        path,
                        content,
                        truncated,
                        is_text,
                    })
                    .await;
            }
            DaemonMessage::AgentConfigResponse { config_json } => {
                match serde_json::from_str::<Value>(&config_json) {
                    Ok(raw) => {
                        if let Ok(config) =
                            serde_json::from_value::<AgentConfigSnapshot>(raw.clone())
                        {
                            let _ = event_tx.send(ClientEvent::AgentConfig(config)).await;
                        }
                        let _ = event_tx.send(ClientEvent::AgentConfigRaw(raw)).await;
                    }
                    Err(err) => warn!("Failed to parse agent config response: {}", err),
                }
            }
            DaemonMessage::AgentModelsResponse { models_json } => {
                match serde_json::from_str::<Vec<FetchedModel>>(&models_json) {
                    Ok(models) => {
                        let _ = event_tx.send(ClientEvent::ModelsFetched(models)).await;
                    }
                    Err(err) => warn!("Failed to parse models response: {}", err),
                }
            }
            DaemonMessage::AgentHeartbeatItems { items_json } => {
                match serde_json::from_str::<Vec<HeartbeatItem>>(&items_json) {
                    Ok(items) => {
                        let _ = event_tx.send(ClientEvent::HeartbeatItems(items)).await;
                    }
                    Err(err) => warn!("Failed to parse heartbeat items: {}", err),
                }
            }
            DaemonMessage::SessionSpawned { id } => {
                let _ = event_tx
                    .send(ClientEvent::SessionSpawned {
                        session_id: id.to_string(),
                    })
                    .await;
            }
            DaemonMessage::ApprovalRequired { approval, .. } => {
                let _ = event_tx
                    .send(ClientEvent::ApprovalRequired {
                        approval_id: approval.approval_id,
                        command: approval.command,
                        risk_level: approval.risk_level,
                        blast_radius: approval.blast_radius,
                    })
                    .await;
            }
            DaemonMessage::ApprovalResolved {
                approval_id,
                decision,
                ..
            } => {
                let _ = event_tx
                    .send(ClientEvent::ApprovalResolved {
                        approval_id,
                        decision: format!("{decision:?}").to_lowercase(),
                    })
                    .await;
            }
            DaemonMessage::AgentProviderAuthStates { states_json } => {
                let states: Vec<serde_json::Value> =
                    serde_json::from_str(&states_json).unwrap_or_default();
                let entries = states
                    .iter()
                    .filter_map(|v| {
                        Some(crate::state::ProviderAuthEntry {
                            provider_id: v.get("provider_id")?.as_str()?.to_string(),
                            provider_name: v.get("provider_name")?.as_str()?.to_string(),
                            authenticated: v.get("authenticated")?.as_bool()?,
                            auth_source: v
                                .get("auth_source")
                                .and_then(|s| s.as_str())
                                .unwrap_or("api_key")
                                .to_string(),
                            model: v
                                .get("model")
                                .and_then(|s| s.as_str())
                                .unwrap_or("")
                                .to_string(),
                        })
                    })
                    .collect();
                let _ = event_tx
                    .send(ClientEvent::ProviderAuthStates(entries))
                    .await;
            }
            DaemonMessage::AgentProviderValidation {
                provider_id,
                valid,
                error,
                ..
            } => {
                let _ = event_tx
                    .send(ClientEvent::ProviderValidation {
                        provider_id,
                        valid,
                        error,
                    })
                    .await;
            }
            DaemonMessage::AgentSubAgentList { sub_agents_json } => {
                let items: Vec<serde_json::Value> =
                    serde_json::from_str(&sub_agents_json).unwrap_or_default();
                let entries = items
                    .iter()
                    .filter_map(|v| {
                        Some(crate::state::SubAgentEntry {
                            id: v.get("id")?.as_str()?.to_string(),
                            name: v.get("name")?.as_str()?.to_string(),
                            provider: v.get("provider")?.as_str()?.to_string(),
                            model: v.get("model")?.as_str()?.to_string(),
                            role: v.get("role").and_then(|s| s.as_str()).map(String::from),
                            enabled: v.get("enabled").and_then(|b| b.as_bool()).unwrap_or(true),
                            raw_json: Some(v.clone()),
                        })
                    })
                    .collect();
                let _ = event_tx.send(ClientEvent::SubAgentList(entries)).await;
            }
            DaemonMessage::AgentSubAgentUpdated { sub_agent_json } => {
                let v: serde_json::Value =
                    serde_json::from_str(&sub_agent_json).unwrap_or_default();
                let entry = crate::state::SubAgentEntry {
                    id: v
                        .get("id")
                        .and_then(|s| s.as_str())
                        .unwrap_or("")
                        .to_string(),
                    name: v
                        .get("name")
                        .and_then(|s| s.as_str())
                        .unwrap_or("")
                        .to_string(),
                    provider: v
                        .get("provider")
                        .and_then(|s| s.as_str())
                        .unwrap_or("")
                        .to_string(),
                    model: v
                        .get("model")
                        .and_then(|s| s.as_str())
                        .unwrap_or("")
                        .to_string(),
                    role: v.get("role").and_then(|s| s.as_str()).map(String::from),
                    enabled: v.get("enabled").and_then(|b| b.as_bool()).unwrap_or(true),
                    raw_json: Some(v),
                };
                let _ = event_tx.send(ClientEvent::SubAgentUpdated(entry)).await;
            }
            DaemonMessage::AgentSubAgentRemoved { sub_agent_id } => {
                let _ = event_tx
                    .send(ClientEvent::SubAgentRemoved { sub_agent_id })
                    .await;
            }
            DaemonMessage::AgentConciergeConfig { config_json } => {
                match serde_json::from_str::<Value>(&config_json) {
                    Ok(raw) => {
                        let _ = event_tx.send(ClientEvent::ConciergeConfig(raw)).await;
                    }
                    Err(err) => warn!("Failed to parse concierge config response: {}", err),
                }
            }
            // Plugin response handlers (Plan 16-03)
            DaemonMessage::PluginListResult { plugins } => {
                let _ = event_tx.send(ClientEvent::PluginList(plugins)).await;
            }
            DaemonMessage::PluginGetResult {
                plugin,
                settings_schema,
            } => {
                let _ = event_tx
                    .send(ClientEvent::PluginGet {
                        plugin,
                        settings_schema,
                    })
                    .await;
            }
            DaemonMessage::PluginSettingsResult {
                plugin_name,
                settings,
            } => {
                let _ = event_tx
                    .send(ClientEvent::PluginSettings {
                        plugin_name,
                        settings,
                    })
                    .await;
            }
            DaemonMessage::PluginTestConnectionResult {
                plugin_name,
                success,
                message,
            } => {
                let _ = event_tx
                    .send(ClientEvent::PluginTestConnection {
                        plugin_name,
                        success,
                        message,
                    })
                    .await;
            }
            DaemonMessage::PluginActionResult { success, message } => {
                let _ = event_tx
                    .send(ClientEvent::PluginAction { success, message })
                    .await;
            }
            DaemonMessage::PluginCommandsResult { commands } => {
                let _ = event_tx.send(ClientEvent::PluginCommands(commands)).await;
            }
            DaemonMessage::PluginOAuthUrl { name, url } => {
                let _ = event_tx
                    .send(ClientEvent::PluginOAuthUrl { name, url })
                    .await;
            }
            DaemonMessage::PluginOAuthComplete {
                name,
                success,
                error,
            } => {
                let _ = event_tx
                    .send(ClientEvent::PluginOAuthComplete {
                        name,
                        success,
                        error,
                    })
                    .await;
            }
            DaemonMessage::AgentWhatsAppLinkStatus {
                state,
                phone,
                last_error,
            } => {
                let _ = event_tx
                    .send(ClientEvent::WhatsAppLinkStatus {
                        state,
                        phone,
                        last_error,
                    })
                    .await;
            }
            DaemonMessage::AgentWhatsAppLinkQr {
                ascii_qr,
                expires_at_ms,
            } => {
                let _ = event_tx
                    .send(ClientEvent::WhatsAppLinkQr {
                        ascii_qr,
                        expires_at_ms,
                    })
                    .await;
            }
            DaemonMessage::AgentWhatsAppLinked { phone } => {
                let _ = event_tx.send(ClientEvent::WhatsAppLinked { phone }).await;
            }
            DaemonMessage::AgentWhatsAppLinkError {
                message,
                recoverable,
            } => {
                let _ = event_tx
                    .send(ClientEvent::WhatsAppLinkError {
                        message,
                        recoverable,
                    })
                    .await;
            }
            DaemonMessage::AgentWhatsAppLinkDisconnected { reason } => {
                let _ = event_tx
                    .send(ClientEvent::WhatsAppLinkDisconnected { reason })
                    .await;
            }
            DaemonMessage::AgentExplanation { explanation_json } => {
                let payload = serde_json::from_str::<serde_json::Value>(&explanation_json)
                    .unwrap_or_else(|_| serde_json::json!({}));
                let _ = event_tx.send(ClientEvent::AgentExplanation(payload)).await;
            }
            DaemonMessage::AgentDivergentSessionStarted { session_json } => {
                let payload = serde_json::from_str::<serde_json::Value>(&session_json)
                    .unwrap_or_else(|_| serde_json::json!({}));
                let _ = event_tx
                    .send(ClientEvent::DivergentSessionStarted(payload))
                    .await;
            }
            DaemonMessage::AgentDivergentSession { session_json } => {
                let payload = serde_json::from_str::<serde_json::Value>(&session_json)
                    .unwrap_or_else(|_| serde_json::json!({}));
                let _ = event_tx.send(ClientEvent::DivergentSession(payload)).await;
            }
            DaemonMessage::AgentStatusResponse {
                diagnostics_json, ..
            } => {
                if let Ok(diagnostics) =
                    serde_json::from_str::<serde_json::Value>(&diagnostics_json)
                {
                    let _ = event_tx
                        .send(ClientEvent::StatusDiagnostics {
                            operator_profile_sync_state: diagnostics
                                .get("operator_profile_sync_state")
                                .and_then(Value::as_str)
                                .unwrap_or("unknown")
                                .to_string(),
                            operator_profile_sync_dirty: diagnostics
                                .get("operator_profile_sync_dirty")
                                .and_then(Value::as_bool)
                                .unwrap_or(false),
                            operator_profile_scheduler_fallback: diagnostics
                                .get("operator_profile_scheduler_fallback")
                                .and_then(Value::as_bool)
                                .unwrap_or(false),
                        })
                        .await;
                }
            }
            DaemonMessage::AgentOperatorProfileSessionStarted { session_id, kind } => {
                let _ = event_tx
                    .send(ClientEvent::OperatorProfileSessionStarted { session_id, kind })
                    .await;
            }
            DaemonMessage::AgentOperatorProfileQuestion {
                session_id,
                question_id,
                field_key,
                prompt,
                input_kind,
                optional,
            } => {
                let _ = event_tx
                    .send(ClientEvent::OperatorProfileQuestion {
                        session_id,
                        question_id,
                        field_key,
                        prompt,
                        input_kind,
                        optional,
                    })
                    .await;
            }
            DaemonMessage::AgentOperatorProfileProgress {
                session_id,
                answered,
                remaining,
                completion_ratio,
            } => {
                let _ = event_tx
                    .send(ClientEvent::OperatorProfileProgress {
                        session_id,
                        answered,
                        remaining,
                        completion_ratio,
                    })
                    .await;
            }
            DaemonMessage::AgentOperatorProfileSummary { summary_json } => {
                let _ = event_tx
                    .send(ClientEvent::OperatorProfileSummary { summary_json })
                    .await;
            }
            DaemonMessage::AgentOperatorProfileSessionCompleted {
                session_id,
                updated_fields,
            } => {
                let _ = event_tx
                    .send(ClientEvent::OperatorProfileSessionCompleted {
                        session_id,
                        updated_fields,
                    })
                    .await;
            }
            DaemonMessage::GatewayBootstrap { .. }
            | DaemonMessage::GatewaySendRequest { .. }
            | DaemonMessage::GatewayReloadCommand { .. }
            | DaemonMessage::GatewayShutdownCommand { .. } => {
                debug!("Ignoring gateway runtime daemon message in TUI client");
            }
            DaemonMessage::Error { message } => {
                let _ = event_tx.send(ClientEvent::Error(message)).await;
            }
            other => {
                debug!("Ignoring daemon message: {:?}", other);
            }
        }

        true
    }

    async fn dispatch_agent_event(event: Value, event_tx: &mpsc::Sender<ClientEvent>) {
        let Some(kind) = event.get("type").and_then(Value::as_str) else {
            return;
        };

        match kind {
            "thread_created" => {
                let _ = event_tx
                    .send(ClientEvent::ThreadCreated {
                        thread_id: get_string(&event, "thread_id").unwrap_or_default(),
                        title: get_string(&event, "title")
                            .unwrap_or_else(|| "New Conversation".to_string()),
                    })
                    .await;
            }
            "delta" => {
                let _ = event_tx
                    .send(ClientEvent::Delta {
                        thread_id: get_string(&event, "thread_id").unwrap_or_default(),
                        content: get_string(&event, "content").unwrap_or_default(),
                    })
                    .await;
            }
            "reasoning" => {
                let _ = event_tx
                    .send(ClientEvent::Reasoning {
                        thread_id: get_string(&event, "thread_id").unwrap_or_default(),
                        content: get_string(&event, "content").unwrap_or_default(),
                    })
                    .await;
            }
            "tool_call" => {
                let _ = event_tx
                    .send(ClientEvent::ToolCall {
                        thread_id: get_string(&event, "thread_id").unwrap_or_default(),
                        call_id: get_string(&event, "call_id").unwrap_or_default(),
                        name: get_string(&event, "name").unwrap_or_default(),
                        arguments: get_string_lossy(&event, "arguments"),
                    })
                    .await;
            }
            "tool_result" => {
                let _ = event_tx
                    .send(ClientEvent::ToolResult {
                        thread_id: get_string(&event, "thread_id").unwrap_or_default(),
                        call_id: get_string(&event, "call_id").unwrap_or_default(),
                        name: get_string(&event, "name").unwrap_or_default(),
                        content: get_string_lossy(&event, "content"),
                        is_error: event
                            .get("is_error")
                            .and_then(Value::as_bool)
                            .unwrap_or(false),
                    })
                    .await;
            }
            "done" => {
                let _ = event_tx
                    .send(ClientEvent::Done {
                        thread_id: get_string(&event, "thread_id").unwrap_or_default(),
                        input_tokens: event
                            .get("input_tokens")
                            .and_then(Value::as_u64)
                            .unwrap_or(0),
                        output_tokens: event
                            .get("output_tokens")
                            .and_then(Value::as_u64)
                            .unwrap_or(0),
                        cost: event.get("cost").and_then(Value::as_f64),
                        provider: get_string(&event, "provider"),
                        model: get_string(&event, "model"),
                        tps: event.get("tps").and_then(Value::as_f64),
                        generation_ms: event.get("generation_ms").and_then(Value::as_u64),
                    })
                    .await;
            }
            "error" => {
                let _ = event_tx
                    .send(ClientEvent::Error(
                        get_string(&event, "message")
                            .unwrap_or_else(|| "Unknown agent error".to_string()),
                    ))
                    .await;
            }
            "workflow_notice" => {
                let _ = event_tx
                    .send(ClientEvent::WorkflowNotice {
                        kind: get_string(&event, "kind").unwrap_or_default(),
                        message: get_string(&event, "message").unwrap_or_default(),
                        details: get_string(&event, "details"),
                    })
                    .await;
            }
            "retry_status" => {
                let _ = event_tx
                    .send(ClientEvent::RetryStatus {
                        thread_id: get_string(&event, "thread_id").unwrap_or_default(),
                        phase: get_string(&event, "phase")
                            .unwrap_or_else(|| "retrying".to_string()),
                        attempt: event.get("attempt").and_then(Value::as_u64).unwrap_or(0) as u32,
                        max_retries: event
                            .get("max_retries")
                            .and_then(Value::as_u64)
                            .unwrap_or(0) as u32,
                        delay_ms: event.get("delay_ms").and_then(Value::as_u64).unwrap_or(0),
                        failure_class: get_string(&event, "failure_class")
                            .unwrap_or_else(|| "transient".to_string()),
                        message: get_string(&event, "message").unwrap_or_default(),
                    })
                    .await;
            }
            "anticipatory_update" => {
                let items = event
                    .get("items")
                    .cloned()
                    .and_then(|raw| serde_json::from_value::<Vec<AnticipatoryItem>>(raw).ok())
                    .unwrap_or_default();
                let _ = event_tx.send(ClientEvent::AnticipatoryItems(items)).await;
            }
            "task_update" => {
                let task = event
                    .get("task")
                    .cloned()
                    .and_then(|raw| serde_json::from_value::<AgentTask>(raw).ok())
                    .unwrap_or_else(|| AgentTask {
                        id: get_string(&event, "task_id").unwrap_or_default(),
                        title: get_string(&event, "message")
                            .unwrap_or_else(|| "Task update".to_string()),
                        thread_id: None,
                        status: event
                            .get("status")
                            .cloned()
                            .and_then(|raw| serde_json::from_value::<TaskStatus>(raw).ok()),
                        progress: event.get("progress").and_then(Value::as_u64).unwrap_or(0) as u8,
                        session_id: None,
                        goal_run_id: None,
                        goal_step_title: None,
                        awaiting_approval_id: None,
                        blocked_reason: get_string(&event, "message"),
                    });

                let _ = event_tx.send(ClientEvent::TaskUpdate(task)).await;
            }
            "goal_run_update" => {
                let goal_run = event
                    .get("goal_run")
                    .cloned()
                    .and_then(|raw| serde_json::from_value::<GoalRun>(raw).ok())
                    .unwrap_or_else(|| GoalRun {
                        id: get_string(&event, "goal_run_id").unwrap_or_default(),
                        title: get_string(&event, "message")
                            .unwrap_or_else(|| "Goal run update".to_string()),
                        status: event
                            .get("status")
                            .cloned()
                            .and_then(|raw| serde_json::from_value::<GoalRunStatus>(raw).ok()),
                        last_error: get_string(&event, "message"),
                        ..GoalRun::default()
                    });

                let _ = event_tx.send(ClientEvent::GoalRunUpdate(goal_run)).await;
            }
            "todo_update" => {
                let thread_id = get_string(&event, "thread_id").unwrap_or_default();
                let items = event
                    .get("items")
                    .cloned()
                    .and_then(|raw| serde_json::from_value::<Vec<crate::wire::TodoItem>>(raw).ok())
                    .unwrap_or_default();
                let _ = event_tx
                    .send(ClientEvent::ThreadTodos { thread_id, items })
                    .await;
            }
            "work_context_update" => {
                if let Some(context) = event
                    .get("context")
                    .cloned()
                    .and_then(|raw| serde_json::from_value::<ThreadWorkContext>(raw).ok())
                {
                    let _ = event_tx.send(ClientEvent::WorkContext(context)).await;
                }
            }
            "concierge_welcome" => {
                let content = event
                    .get("content")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let actions = event
                    .get("actions")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|a| {
                                Some(crate::state::ConciergeActionVm {
                                    label: a.get("label")?.as_str()?.to_string(),
                                    action_type: a.get("action_type")?.as_str()?.to_string(),
                                    thread_id: a
                                        .get("thread_id")
                                        .and_then(|v| v.as_str())
                                        .map(String::from),
                                })
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                let _ = event_tx
                    .send(ClientEvent::ConciergeWelcome { content, actions })
                    .await;
            }
            "heartbeat_digest" => {
                let items: Vec<(u8, String, String, String)> = event
                    .get("items")
                    .and_then(Value::as_array)
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|item| {
                                Some((
                                    item.get("priority")?.as_u64()? as u8,
                                    item.get("check_type")?.as_str()?.to_string(),
                                    item.get("title")?.as_str()?.to_string(),
                                    item.get("suggestion")?.as_str()?.to_string(),
                                ))
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                let explanation = get_string(&event, "explanation");
                let _ = event_tx
                    .send(ClientEvent::HeartbeatDigest {
                        cycle_id: get_string(&event, "cycle_id").unwrap_or_default(),
                        actionable: event
                            .get("actionable")
                            .and_then(Value::as_bool)
                            .unwrap_or(false),
                        digest: get_string(&event, "digest").unwrap_or_default(),
                        items,
                        checked_at: event.get("checked_at").and_then(Value::as_u64).unwrap_or(0),
                        explanation,
                    })
                    .await;
            }
            "audit_action" => {
                let id = get_string(&event, "id").unwrap_or_default();
                let timestamp = event.get("timestamp").and_then(Value::as_u64).unwrap_or(0);
                let action_type = get_string(&event, "action_type").unwrap_or_default();
                let summary = get_string(&event, "summary").unwrap_or_default();
                let explanation = get_string(&event, "explanation");
                let confidence = event.get("confidence").and_then(Value::as_f64);
                let confidence_band = get_string(&event, "confidence_band");
                let causal_trace_id = get_string(&event, "causal_trace_id");
                let thread_id = get_string(&event, "thread_id");
                let _ = event_tx
                    .send(ClientEvent::AuditEntry {
                        id,
                        timestamp,
                        action_type,
                        summary,
                        explanation,
                        confidence,
                        confidence_band,
                        causal_trace_id,
                        thread_id,
                    })
                    .await;
            }
            "escalation_update" => {
                let thread_id = get_string(&event, "thread_id").unwrap_or_default();
                let from_level = get_string(&event, "from_level").unwrap_or_default();
                let to_level = get_string(&event, "to_level").unwrap_or_default();
                let reason = get_string(&event, "reason").unwrap_or_default();
                let attempts = event.get("attempts").and_then(Value::as_u64).unwrap_or(0) as u32;
                let audit_id = get_string(&event, "audit_id");
                let _ = event_tx
                    .send(ClientEvent::EscalationUpdate {
                        thread_id,
                        from_level,
                        to_level,
                        reason,
                        attempts,
                        audit_id,
                    })
                    .await;
            }
            "gateway_status" => {
                let platform = get_string(&event, "platform").unwrap_or_default();
                let status = get_string(&event, "status").unwrap_or_default();
                let last_error = get_string(&event, "last_error");
                let consecutive_failures = event
                    .get("consecutive_failures")
                    .and_then(Value::as_u64)
                    .unwrap_or(0) as u32;
                let _ = event_tx
                    .send(ClientEvent::GatewayStatus {
                        platform,
                        status,
                        last_error,
                        consecutive_failures,
                    })
                    .await;
            }
            "tier_changed" | "tier-changed" => {
                let data = event.get("data").cloned().unwrap_or_else(|| event.clone());
                let new_tier = data
                    .get("new_tier")
                    .or_else(|| data.get("newTier"))
                    .and_then(Value::as_str)
                    .unwrap_or("newcomer")
                    .to_string();
                let _ = event_tx.send(ClientEvent::TierChanged { new_tier }).await;
            }
            _ => {}
        }
    }

    fn send(&self, request: ClientMessage) -> Result<()> {
        self.request_tx.send(request)?;
        Ok(())
    }

    pub fn refresh(&self) -> Result<()> {
        self.send(ClientMessage::AgentListThreads)
    }

    pub fn refresh_services(&self) -> Result<()> {
        for request in [
            ClientMessage::AgentListTasks,
            ClientMessage::AgentListGoalRuns,
            ClientMessage::AgentGetConfig,
            ClientMessage::AgentHeartbeatGetItems,
        ] {
            self.send(request)?;
        }
        Ok(())
    }

    pub fn request_goal_run(&self, goal_run_id: impl Into<String>) -> Result<()> {
        self.send(ClientMessage::AgentGetGoalRun {
            goal_run_id: goal_run_id.into(),
        })
    }

    pub fn start_goal_run(
        &self,
        goal: String,
        thread_id: Option<String>,
        session_id: Option<String>,
    ) -> Result<()> {
        self.send(ClientMessage::AgentStartGoalRun {
            goal,
            title: None,
            thread_id,
            session_id,
            priority: None,
            client_request_id: None,
            autonomy_level: None,
        })
    }

    pub fn explain_action(&self, action_id: String, step_index: Option<usize>) -> Result<()> {
        self.send(ClientMessage::AgentExplainAction {
            action_id,
            step_index,
        })
    }

    pub fn start_divergent_session(
        &self,
        problem_statement: String,
        thread_id: String,
        goal_run_id: Option<String>,
    ) -> Result<()> {
        self.send(ClientMessage::AgentStartDivergentSession {
            problem_statement,
            thread_id,
            goal_run_id,
            custom_framings_json: None,
        })
    }

    pub fn get_divergent_session(&self, session_id: String) -> Result<()> {
        self.send(ClientMessage::AgentGetDivergentSession { session_id })
    }

    pub fn request_thread(&self, thread_id: impl Into<String>) -> Result<()> {
        self.send(ClientMessage::AgentGetThread {
            thread_id: thread_id.into(),
        })
    }

    pub fn request_todos(&self, thread_id: impl Into<String>) -> Result<()> {
        self.send(ClientMessage::AgentGetTodos {
            thread_id: thread_id.into(),
        })
    }

    pub fn request_work_context(&self, thread_id: impl Into<String>) -> Result<()> {
        self.send(ClientMessage::AgentGetWorkContext {
            thread_id: thread_id.into(),
        })
    }

    pub fn request_git_diff(
        &self,
        repo_path: impl Into<String>,
        file_path: Option<String>,
    ) -> Result<()> {
        self.send(ClientMessage::GetGitDiff {
            repo_path: repo_path.into(),
            file_path,
        })
    }

    pub fn request_file_preview(
        &self,
        path: impl Into<String>,
        max_bytes: Option<usize>,
    ) -> Result<()> {
        self.send(ClientMessage::GetFilePreview {
            path: path.into(),
            max_bytes,
        })
    }

    pub fn send_message(
        &self,
        thread_id: Option<String>,
        content: String,
        session_id: Option<String>,
    ) -> Result<()> {
        self.send(ClientMessage::AgentSendMessage {
            thread_id,
            content,
            session_id,
            context_messages_json: None,
        })
    }

    pub fn stop_stream(&self, thread_id: String) -> Result<()> {
        self.send(ClientMessage::AgentStopStream { thread_id })
    }

    pub fn delete_messages(&self, thread_id: String, message_ids: Vec<String>) -> Result<()> {
        self.send(ClientMessage::DeleteAgentMessages {
            thread_id,
            message_ids,
        })
    }

    pub fn spawn_session(
        &self,
        shell: Option<String>,
        cwd: Option<String>,
        cols: u16,
        rows: u16,
    ) -> Result<()> {
        self.send(ClientMessage::SpawnSession {
            shell,
            cwd,
            env: None,
            workspace_id: None,
            cols,
            rows,
        })
    }

    pub fn control_goal_run(&self, goal_run_id: String, action: String) -> Result<()> {
        self.send(ClientMessage::AgentControlGoalRun {
            goal_run_id,
            action,
            step_index: None,
        })
    }

    pub fn fetch_models(
        &self,
        provider_id: String,
        base_url: String,
        api_key: String,
    ) -> Result<()> {
        self.send(ClientMessage::AgentFetchModels {
            provider_id,
            base_url,
            api_key,
        })
    }

    pub fn set_config_item_json(&self, key_path: String, value_json: String) -> Result<()> {
        self.send(ClientMessage::AgentSetConfigItem {
            key_path,
            value_json,
        })
    }

    pub fn get_provider_auth_states(&self) -> Result<()> {
        self.send(ClientMessage::AgentGetProviderAuthStates)
    }

    pub fn validate_provider(
        &self,
        provider_id: String,
        base_url: String,
        api_key: String,
        auth_source: String,
    ) -> Result<()> {
        self.send(ClientMessage::AgentValidateProvider {
            provider_id,
            base_url,
            api_key,
            auth_source,
        })
    }

    pub fn set_sub_agent(&self, sub_agent_json: String) -> Result<()> {
        self.send(ClientMessage::AgentSetSubAgent { sub_agent_json })
    }

    pub fn remove_sub_agent(&self, sub_agent_id: String) -> Result<()> {
        self.send(ClientMessage::AgentRemoveSubAgent { sub_agent_id })
    }

    pub fn list_sub_agents(&self) -> Result<()> {
        self.send(ClientMessage::AgentListSubAgents)
    }

    pub fn get_concierge_config(&self) -> Result<()> {
        self.send(ClientMessage::AgentGetConciergeConfig)
    }

    pub fn set_concierge_config(&self, config_json: String) -> Result<()> {
        self.send(ClientMessage::AgentSetConciergeConfig { config_json })
    }

    pub fn request_concierge_welcome(&self) -> Result<()> {
        self.send(ClientMessage::AgentRequestConciergeWelcome)
    }

    pub fn list_checkpoints(&self, goal_run_id: String) -> Result<()> {
        self.send(ClientMessage::AgentListCheckpoints { goal_run_id })
    }

    pub fn dismiss_concierge_welcome(&self) -> Result<()> {
        self.send(ClientMessage::AgentDismissConciergeWelcome)
    }

    pub fn record_attention(
        &self,
        surface: String,
        thread_id: Option<String>,
        goal_run_id: Option<String>,
    ) -> Result<()> {
        self.send(ClientMessage::AgentRecordAttention {
            surface,
            thread_id,
            goal_run_id,
        })
    }

    pub fn start_operator_profile_session(&self, kind: String) -> Result<()> {
        self.send(ClientMessage::AgentStartOperatorProfileSession { kind })
    }

    pub fn next_operator_profile_question(&self, session_id: String) -> Result<()> {
        self.send(ClientMessage::AgentNextOperatorProfileQuestion { session_id })
    }

    pub fn submit_operator_profile_answer(
        &self,
        session_id: String,
        question_id: String,
        answer_json: String,
    ) -> Result<()> {
        self.send(ClientMessage::AgentSubmitOperatorProfileAnswer {
            session_id,
            question_id,
            answer_json,
        })
    }

    pub fn skip_operator_profile_question(
        &self,
        session_id: String,
        question_id: String,
        reason: Option<String>,
    ) -> Result<()> {
        self.send(ClientMessage::AgentSkipOperatorProfileQuestion {
            session_id,
            question_id,
            reason,
        })
    }

    pub fn defer_operator_profile_question(
        &self,
        session_id: String,
        question_id: String,
        defer_until_unix_ms: Option<u64>,
    ) -> Result<()> {
        self.send(ClientMessage::AgentDeferOperatorProfileQuestion {
            session_id,
            question_id,
            defer_until_unix_ms,
        })
    }

    pub fn get_operator_profile_summary(&self) -> Result<()> {
        self.send(ClientMessage::AgentGetOperatorProfileSummary)
    }

    pub fn set_operator_profile_consent(&self, consent_key: String, granted: bool) -> Result<()> {
        self.send(ClientMessage::AgentSetOperatorProfileConsent {
            consent_key,
            granted,
        })
    }

    pub fn dismiss_audit_entry(&self, entry_id: String) -> Result<()> {
        self.send(ClientMessage::AuditDismiss { entry_id })
    }

    // Plugin IPC methods (Plan 16-01)
    pub fn plugin_list(&self) -> Result<()> {
        self.send(ClientMessage::PluginList {})
    }

    pub fn plugin_list_commands(&self) -> Result<()> {
        self.send(ClientMessage::PluginListCommands {})
    }

    pub fn plugin_get(&self, name: String) -> Result<()> {
        self.send(ClientMessage::PluginGet { name })
    }

    pub fn plugin_enable(&self, name: String) -> Result<()> {
        self.send(ClientMessage::PluginEnable { name })
    }

    pub fn plugin_disable(&self, name: String) -> Result<()> {
        self.send(ClientMessage::PluginDisable { name })
    }

    pub fn plugin_get_settings(&self, name: String) -> Result<()> {
        self.send(ClientMessage::PluginGetSettings { name })
    }

    pub fn plugin_update_setting(
        &self,
        plugin_name: String,
        key: String,
        value: String,
        is_secret: bool,
    ) -> Result<()> {
        self.send(ClientMessage::PluginUpdateSettings {
            plugin_name,
            key,
            value,
            is_secret,
        })
    }

    pub fn plugin_test_connection(&self, name: String) -> Result<()> {
        self.send(ClientMessage::PluginTestConnection { name })
    }

    pub fn plugin_oauth_start(&self, name: String) -> Result<()> {
        self.send(ClientMessage::PluginOAuthStart { name })
    }

    pub fn whatsapp_link_start(&self) -> Result<()> {
        self.send(ClientMessage::AgentWhatsAppLinkStart)
    }

    pub fn whatsapp_link_stop(&self) -> Result<()> {
        self.send(ClientMessage::AgentWhatsAppLinkStop)
    }

    pub fn whatsapp_link_status(&self) -> Result<()> {
        self.send(ClientMessage::AgentWhatsAppLinkStatus)
    }

    pub fn whatsapp_link_subscribe(&self) -> Result<()> {
        self.send(ClientMessage::AgentWhatsAppLinkSubscribe)
    }

    pub fn whatsapp_link_unsubscribe(&self) -> Result<()> {
        self.send(ClientMessage::AgentWhatsAppLinkUnsubscribe)
    }

    pub fn whatsapp_link_reset(&self) -> Result<()> {
        self.send(ClientMessage::AgentWhatsAppLinkReset)
    }

    pub fn resolve_task_approval(&self, approval_id: String, decision: String) -> Result<()> {
        use amux_protocol::ApprovalDecision;
        let decision = match decision.as_str() {
            "allow_once" | "approve_once" => ApprovalDecision::ApproveOnce,
            "allow_session" | "approve_session" => ApprovalDecision::ApproveSession,
            _ => ApprovalDecision::Deny,
        };
        // ResolveApproval requires a SessionId; we use a nil UUID as placeholder
        // since the daemon routes by approval_id.
        self.send(ClientMessage::ResolveApproval {
            id: uuid::Uuid::nil(),
            approval_id,
            decision,
        })
    }
}

fn get_string(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
}

fn get_string_lossy(value: &Value, key: &str) -> String {
    match value.get(key) {
        Some(Value::String(inner)) => inner.clone(),
        Some(other) => other.to_string(),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use amux_protocol::ClientMessage;
    use tokio::sync::mpsc;

    fn drain_request(rx: &mut mpsc::UnboundedReceiver<ClientMessage>) -> ClientMessage {
        rx.try_recv().expect("expected queued client message")
    }

    #[test]
    fn whatsapp_link_methods_send_expected_protocol_messages() {
        let (event_tx, _event_rx) = mpsc::channel(8);
        let client = DaemonClient::new(event_tx);
        let mut rx = client.request_rx.lock().unwrap().take().unwrap();

        client.whatsapp_link_start().unwrap();
        assert!(matches!(
            drain_request(&mut rx),
            ClientMessage::AgentWhatsAppLinkStart
        ));

        client.whatsapp_link_status().unwrap();
        assert!(matches!(
            drain_request(&mut rx),
            ClientMessage::AgentWhatsAppLinkStatus
        ));

        client.whatsapp_link_subscribe().unwrap();
        assert!(matches!(
            drain_request(&mut rx),
            ClientMessage::AgentWhatsAppLinkSubscribe
        ));

        client.whatsapp_link_unsubscribe().unwrap();
        assert!(matches!(
            drain_request(&mut rx),
            ClientMessage::AgentWhatsAppLinkUnsubscribe
        ));

        client.whatsapp_link_reset().unwrap();
        assert!(matches!(
            drain_request(&mut rx),
            ClientMessage::AgentWhatsAppLinkReset
        ));

        client.whatsapp_link_stop().unwrap();
        assert!(matches!(
            drain_request(&mut rx),
            ClientMessage::AgentWhatsAppLinkStop
        ));
    }
}
