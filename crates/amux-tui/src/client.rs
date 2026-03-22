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
    AnticipatoryItems(Vec<AnticipatoryItem>),

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
        })
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

    pub fn set_config_json(&self, config_json: String) -> Result<()> {
        self.send(ClientMessage::AgentSetConfig { config_json })
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
