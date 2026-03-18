//! Always-on autonomous agent engine.
//!
//! The agent lives in the daemon process and handles:
//! - LLM inference with streaming (OpenAI-compatible + Anthropic)
//! - Tool execution via SessionManager
//! - Persistent task queue with automatic processing
//! - Heartbeat system for periodic checks
//! - Persistent identity (SOUL.md, MEMORY.md, USER.md)

pub mod external_runner;
pub mod gateway;
pub mod llm_client;
pub mod tool_executor;
pub mod types;

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use futures::StreamExt;
use tokio::sync::{broadcast, Mutex, RwLock};
use tokio::time::{Duration, Instant};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::history::HistoryStore;
use crate::session_manager::SessionManager;

use self::llm_client::{messages_to_api_format, send_chat_completion, ApiContent, ApiMessage};
use self::tool_executor::{execute_tool, get_available_tools};
use self::types::*;

struct SendMessageOutcome {
    thread_id: String,
    interrupted_for_approval: bool,
}

#[derive(Clone)]
struct StreamCancellationEntry {
    generation: u64,
    token: CancellationToken,
}

const ONECONTEXT_BOOTSTRAP_QUERY_MAX_CHARS: usize = 180;
const ONECONTEXT_BOOTSTRAP_OUTPUT_MAX_CHARS: usize = 5000;

/// Cached check for `aline` CLI availability (checked once per process).
pub(crate) fn aline_available() -> bool {
    static AVAILABLE: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *AVAILABLE.get_or_init(|| which::which("aline").is_ok())
}

// ---------------------------------------------------------------------------
// AgentEngine
// ---------------------------------------------------------------------------

pub struct AgentEngine {
    config: RwLock<AgentConfig>,
    http_client: reqwest::Client,
    session_manager: Arc<SessionManager>,
    history: HistoryStore,
    threads: RwLock<HashMap<String, AgentThread>>,
    thread_todos: RwLock<HashMap<String, Vec<TodoItem>>>,
    tasks: Mutex<VecDeque<AgentTask>>,
    goal_runs: Mutex<VecDeque<GoalRun>>,
    inflight_goal_runs: Mutex<HashSet<String>>,
    heartbeat_items: RwLock<Vec<HeartbeatItem>>,
    event_tx: broadcast::Sender<AgentEvent>,
    memory: RwLock<AgentMemory>,
    data_dir: PathBuf,
    gateway_process: Mutex<Option<tokio::process::Child>>,
    gateway_state: Mutex<Option<gateway::GatewayState>>,
    /// Discord channel IDs to poll (parsed from config).
    gateway_discord_channels: RwLock<Vec<String>>,
    /// Slack channel IDs to poll (parsed from config).
    gateway_slack_channels: RwLock<Vec<String>>,
    /// Maps gateway channel IDs to daemon thread IDs for conversation continuity.
    gateway_threads: RwLock<HashMap<String, String>>,
    /// External agent runners for openclaw/hermes backends.
    external_runners: RwLock<HashMap<String, external_runner::ExternalAgentRunner>>,
    /// Active cancellation tokens per thread for stop-stream behavior.
    stream_cancellations: Mutex<HashMap<String, StreamCancellationEntry>>,
    stream_generation: AtomicU64,
}

impl AgentEngine {
    pub fn new(session_manager: Arc<SessionManager>, config: AgentConfig) -> Arc<Self> {
        let (event_tx, _) = broadcast::channel(256);
        let data_dir = agent_data_dir();

        // Pre-initialize external agent runners for discovery
        let mut runners = HashMap::new();
        for agent_type in &["openclaw", "hermes"] {
            runners.insert(
                agent_type.to_string(),
                external_runner::ExternalAgentRunner::new(agent_type, event_tx.clone()),
            );
        }

        Arc::new(Self {
            config: RwLock::new(config),
            http_client: reqwest::Client::new(),
            session_manager,
            history: HistoryStore::new().expect("history store initialization failed"),
            threads: RwLock::new(HashMap::new()),
            thread_todos: RwLock::new(HashMap::new()),
            tasks: Mutex::new(VecDeque::new()),
            goal_runs: Mutex::new(VecDeque::new()),
            inflight_goal_runs: Mutex::new(HashSet::new()),
            heartbeat_items: RwLock::new(Vec::new()),
            event_tx,
            memory: RwLock::new(AgentMemory::default()),
            data_dir,
            gateway_process: Mutex::new(None),
            gateway_state: Mutex::new(None),
            gateway_discord_channels: RwLock::new(Vec::new()),
            gateway_slack_channels: RwLock::new(Vec::new()),
            gateway_threads: RwLock::new(HashMap::new()),
            external_runners: RwLock::new(runners),
            stream_cancellations: Mutex::new(HashMap::new()),
            stream_generation: AtomicU64::new(1),
        })
    }

    /// Subscribe to agent events (for IPC forwarding to frontend).
    pub fn subscribe(&self) -> broadcast::Receiver<AgentEvent> {
        self.event_tx.subscribe()
    }

    /// Get a reference to the event sender (for server.rs integration).
    pub fn event_sender(&self) -> broadcast::Sender<AgentEvent> {
        self.event_tx.clone()
    }

    async fn begin_stream_cancellation(&self, thread_id: &str) -> (u64, CancellationToken) {
        let generation = self.stream_generation.fetch_add(1, Ordering::Relaxed);
        let token = CancellationToken::new();
        let mut streams = self.stream_cancellations.lock().await;
        if let Some(previous) = streams.insert(
            thread_id.to_string(),
            StreamCancellationEntry {
                generation,
                token: token.clone(),
            },
        ) {
            previous.token.cancel();
        }
        (generation, token)
    }

    async fn finish_stream_cancellation(&self, thread_id: &str, generation: u64) {
        let mut streams = self.stream_cancellations.lock().await;
        let should_remove = streams
            .get(thread_id)
            .map(|entry| entry.generation == generation)
            .unwrap_or(false);
        if should_remove {
            streams.remove(thread_id);
        }
    }

    pub async fn stop_stream(&self, thread_id: &str) -> bool {
        let token = {
            let streams = self.stream_cancellations.lock().await;
            streams.get(thread_id).map(|entry| entry.token.clone())
        };
        if let Some(token) = token {
            token.cancel();
            true
        } else {
            false
        }
    }

    async fn refresh_memory_cache(&self) {
        let mut memory = AgentMemory::default();
        let memory_dirs = ordered_memory_dirs(&self.data_dir);
        for dir in &memory_dirs {
            if let Ok(soul) = tokio::fs::read_to_string(dir.join("SOUL.md")).await {
                memory.soul = soul;
                break;
            }
        }
        for dir in &memory_dirs {
            if let Ok(mem) = tokio::fs::read_to_string(dir.join("MEMORY.md")).await {
                memory.memory = mem;
                break;
            }
        }
        for dir in &memory_dirs {
            if let Ok(user) = tokio::fs::read_to_string(dir.join("USER.md")).await {
                memory.user_profile = user;
                break;
            }
        }
        *self.memory.write().await = memory;
    }

    async fn onecontext_bootstrap_for_new_thread(&self, initial_message: &str) -> Option<String> {
        let trimmed = initial_message.trim();
        if trimmed.is_empty() {
            return None;
        }
        if !aline_available() {
            return None;
        }

        let query = trimmed
            .chars()
            .take(ONECONTEXT_BOOTSTRAP_QUERY_MAX_CHARS)
            .collect::<String>();

        let mut cmd = tokio::process::Command::new("aline");
        cmd.arg("search")
            .arg(&query)
            .arg("-t")
            .arg("session")
            .arg("--no-regex")
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .stdin(std::process::Stdio::null());

        let output = match tokio::time::timeout(Duration::from_secs(4), cmd.output()).await {
            Ok(Ok(output)) if output.status.success() => output,
            _ => return None,
        };

        let raw = String::from_utf8_lossy(&output.stdout);
        let normalized = raw.trim();
        if normalized.is_empty() {
            return None;
        }

        Some(
            normalized
                .chars()
                .take(ONECONTEXT_BOOTSTRAP_OUTPUT_MAX_CHARS)
                .collect(),
        )
    }

    // -----------------------------------------------------------------------
    // Lifecycle
    // -----------------------------------------------------------------------

    /// Load persisted state (threads, tasks, heartbeat, memory, config).
    pub async fn hydrate(&self) -> Result<()> {
        // Load config
        let config_path = self.data_dir.join("config.json");
        if config_path.exists() {
            match tokio::fs::read_to_string(&config_path).await {
                Ok(raw) => {
                    if let Ok(cfg) = serde_json::from_str::<AgentConfig>(&raw) {
                        *self.config.write().await = cfg;
                    }
                }
                Err(e) => tracing::warn!("failed to load agent config: {e}"),
            }
        }

        // Load threads
        match self.history.list_threads() {
            Ok(thread_rows) if !thread_rows.is_empty() => {
                let mut threads = HashMap::new();
                for thread_row in thread_rows {
                    let messages = self
                        .history
                        .list_messages(&thread_row.id, None)
                        .unwrap_or_default()
                        .into_iter()
                        .map(|message| {
                            let metadata = message.metadata_json.as_deref().and_then(|json| {
                                serde_json::from_str::<serde_json::Value>(json).ok()
                            });
                            let get_str = |keys: &[&str]| -> Option<String> {
                                metadata.as_ref().and_then(|v| {
                                    keys.iter()
                                        .find_map(|k| v.get(*k).and_then(|val| val.as_str()))
                                        .map(ToOwned::to_owned)
                                })
                            };
                            AgentMessage {
                                role: match message.role.as_str() {
                                    "system" => MessageRole::System,
                                    "assistant" => MessageRole::Assistant,
                                    "tool" => MessageRole::Tool,
                                    _ => MessageRole::User,
                                },
                                content: message.content,
                                tool_calls: message
                                    .tool_calls_json
                                    .as_deref()
                                    .and_then(|json| serde_json::from_str(json).ok()),
                                tool_call_id: get_str(&["tool_call_id", "toolCallId"]),
                                tool_name: get_str(&["tool_name", "toolName"]),
                                tool_arguments: get_str(&["tool_arguments", "toolArguments"]),
                                tool_status: get_str(&["tool_status", "toolStatus"]),
                                input_tokens: message.input_tokens.unwrap_or(0) as u64,
                                output_tokens: message.output_tokens.unwrap_or(0) as u64,
                                reasoning: message.reasoning,
                                timestamp: message.created_at as u64,
                            }
                        })
                        .collect::<Vec<_>>();

                    threads.insert(
                        thread_row.id.clone(),
                        AgentThread {
                            id: thread_row.id,
                            title: thread_row.title,
                            messages,
                            created_at: thread_row.created_at as u64,
                            updated_at: thread_row.updated_at as u64,
                            total_input_tokens: 0,
                            total_output_tokens: 0,
                        },
                    );
                }
                *self.threads.write().await = threads;
            }
            Ok(_) => {}
            Err(e) => tracing::warn!("failed to load agent threads from sqlite: {e}"),
        }

        // Load AJQ tasks from SQLite first; fall back to legacy JSON migration.
        match self.history.list_agent_tasks() {
            Ok(mut tasks) if !tasks.is_empty() => {
                for task in &mut tasks {
                    if task.status == TaskStatus::InProgress {
                        task.status = TaskStatus::Queued;
                        task.started_at = None;
                        task.lane_id = None;
                        task.logs.push(make_task_log_entry(
                            task.retry_count,
                            TaskLogLevel::Warn,
                            "hydrate",
                            "daemon restarted while task was in progress; task re-queued",
                            None,
                        ));
                    }
                }
                *self.tasks.lock().await = tasks.into_iter().collect();
                self.persist_tasks().await;
            }
            Ok(_) => {
                let tasks_path = self.data_dir.join("tasks.json");
                if tasks_path.exists() {
                    match tokio::fs::read_to_string(&tasks_path).await {
                        Ok(raw) => {
                            if let Ok(mut tasks) = serde_json::from_str::<VecDeque<AgentTask>>(&raw)
                            {
                                for task in tasks.iter_mut() {
                                    if task.status == TaskStatus::InProgress {
                                        task.status = TaskStatus::Queued;
                                        task.started_at = None;
                                    }
                                    task.max_retries = task.max_retries.max(1);
                                }
                                *self.tasks.lock().await = tasks;
                                self.persist_tasks().await;
                            }
                        }
                        Err(e) => tracing::warn!("failed to migrate legacy agent tasks: {e}"),
                    }
                }
            }
            Err(e) => tracing::warn!("failed to load agent tasks from sqlite: {e}"),
        }

        match self.history.list_goal_runs() {
            Ok(goal_runs) if !goal_runs.is_empty() => {
                *self.goal_runs.lock().await = goal_runs.into_iter().collect();
            }
            Ok(_) => {
                let goal_runs_path = self.data_dir.join("goal-runs.json");
                if goal_runs_path.exists() {
                    match tokio::fs::read_to_string(&goal_runs_path).await {
                        Ok(raw) => {
                            if let Ok(goal_runs) = serde_json::from_str::<VecDeque<GoalRun>>(&raw) {
                                *self.goal_runs.lock().await = goal_runs;
                                self.persist_goal_runs().await;
                            }
                        }
                        Err(e) => tracing::warn!("failed to migrate legacy goal runs: {e}"),
                    }
                }
            }
            Err(e) => tracing::warn!("failed to load goal runs from sqlite: {e}"),
        }

        let todos_path = self.data_dir.join("todos.json");
        if todos_path.exists() {
            match tokio::fs::read_to_string(&todos_path).await {
                Ok(raw) => {
                    if let Ok(items) = serde_json::from_str::<HashMap<String, Vec<TodoItem>>>(&raw)
                    {
                        *self.thread_todos.write().await = items;
                    }
                }
                Err(e) => tracing::warn!("failed to load thread todos: {e}"),
            }
        }

        // Load heartbeat items
        let heartbeat_path = self.data_dir.join("heartbeat.json");
        if heartbeat_path.exists() {
            match tokio::fs::read_to_string(&heartbeat_path).await {
                Ok(raw) => {
                    if let Ok(items) = serde_json::from_str::<Vec<HeartbeatItem>>(&raw) {
                        *self.heartbeat_items.write().await = items;
                    }
                }
                Err(e) => tracing::warn!("failed to load heartbeat items: {e}"),
            }
        }

        // Load memory files
        self.refresh_memory_cache().await;

        // Seed built-in skill documents into ~/.tamux/skills/builtin/
        seed_builtin_skills(&self.data_dir);

        tracing::info!("agent engine hydrated from {:?}", self.data_dir);

        // Initialize gateway polling
        self.init_gateway().await;

        Ok(())
    }

    /// Initialize gateway connections for receiving messages.
    async fn init_gateway(&self) {
        let config = self.config.read().await.clone();
        let gw = &config.gateway;

        // Read settings.json once and extract all gateway-related values
        let (
            slack_token,
            telegram_token,
            discord_token,
            discord_channel_filter,
            slack_channel_filter,
        ) = if !gw.slack_token.is_empty()
            || !gw.telegram_token.is_empty()
            || !gw.discord_token.is_empty()
        {
            (
                gw.slack_token.clone(),
                gw.telegram_token.clone(),
                gw.discord_token.clone(),
                String::new(),
                String::new(),
            )
        } else {
            let settings_path = self
                .data_dir
                .parent()
                .unwrap_or(std::path::Path::new("."))
                .join("settings.json");
            match tokio::fs::read_to_string(&settings_path).await {
                Ok(raw) => {
                    let v: serde_json::Value = serde_json::from_str(&raw).unwrap_or_default();
                    (
                        read_setting_str(&v, "slackToken"),
                        read_setting_str(&v, "telegramToken"),
                        read_setting_str(&v, "discordToken"),
                        read_setting_str(&v, "discordChannelFilter"),
                        read_setting_str(&v, "slackChannelFilter"),
                    )
                }
                Err(_) => (
                    String::new(),
                    String::new(),
                    String::new(),
                    String::new(),
                    String::new(),
                ),
            }
        };

        let has_any =
            !slack_token.is_empty() || !telegram_token.is_empty() || !discord_token.is_empty();
        if !has_any {
            tracing::info!("gateway: no platform tokens, polling disabled");
            return;
        }

        // Parse channel lists from the already-read settings
        if !discord_channel_filter.is_empty() {
            tracing::info!(discord_filter = %discord_channel_filter, "gateway: discordChannelFilter");
            let channels: Vec<String> = discord_channel_filter
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
            *self.gateway_discord_channels.write().await = channels;
        }
        if !slack_channel_filter.is_empty() {
            let channels: Vec<String> = slack_channel_filter
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
            *self.gateway_slack_channels.write().await = channels;
        }

        let gw_config = GatewayConfig {
            enabled: true,
            slack_token,
            telegram_token,
            discord_token,
            command_prefix: gw.command_prefix.clone(),
        };

        let dc = self.gateway_discord_channels.read().await.clone();
        let sc = self.gateway_slack_channels.read().await.clone();

        tracing::info!(
            has_slack = !gw_config.slack_token.is_empty(),
            has_telegram = !gw_config.telegram_token.is_empty(),
            has_discord = !gw_config.discord_token.is_empty(),
            discord_channels = ?dc,
            slack_channels = ?sc,
            "gateway: config loaded"
        );

        *self.gateway_state.lock().await = Some(gateway::GatewayState::new(
            gw_config,
            self.http_client.clone(),
        ));

        tracing::info!("gateway: polling initialized in daemon");
    }

    /// Spawn the tamux-gateway process if gateway tokens are configured.
    pub async fn maybe_spawn_gateway(&self) {
        let config = self.config.read().await.clone();
        let gw = &config.gateway;

        // Also try reading tokens from the frontend settings.json as fallback
        let (slack_token, telegram_token, discord_token) = if !gw.slack_token.is_empty()
            || !gw.telegram_token.is_empty()
            || !gw.discord_token.is_empty()
        {
            (
                gw.slack_token.clone(),
                gw.telegram_token.clone(),
                gw.discord_token.clone(),
            )
        } else {
            // Read from ~/.tamux/settings.json (frontend persistence)
            let settings_path = self
                .data_dir
                .parent()
                .unwrap_or(std::path::Path::new("."))
                .join("settings.json");
            match tokio::fs::read_to_string(&settings_path).await {
                Ok(raw) => {
                    let v: serde_json::Value = serde_json::from_str(&raw).unwrap_or_default();
                    (
                        read_setting_str(&v, "slackToken"),
                        read_setting_str(&v, "telegramToken"),
                        read_setting_str(&v, "discordToken"),
                    )
                }
                Err(_) => (String::new(), String::new(), String::new()),
            }
        };

        if slack_token.is_empty() && telegram_token.is_empty() && discord_token.is_empty() {
            tracing::info!("gateway: no platform tokens configured, skipping");
            return;
        }

        // Find the gateway binary next to the daemon binary
        let gateway_path = std::env::current_exe().ok().and_then(|p| {
            let dir = p.parent()?;
            let name = if cfg!(windows) {
                "tamux-gateway.exe"
            } else {
                "tamux-gateway"
            };
            let path = dir.join(name);
            if path.exists() {
                Some(path)
            } else {
                None
            }
        });

        let gateway_path = match gateway_path {
            Some(p) => p,
            None => {
                tracing::warn!("gateway binary not found next to daemon executable");
                return;
            }
        };

        // Kill existing gateway process if any
        {
            let mut proc = self.gateway_process.lock().await;
            if let Some(ref mut child) = *proc {
                let _ = child.kill().await;
            }
            *proc = None;
        }

        tracing::info!(?gateway_path, "spawning gateway process");

        let mut cmd = tokio::process::Command::new(&gateway_path);
        cmd.stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());

        if !slack_token.is_empty() {
            cmd.env("AMUX_SLACK_TOKEN", &slack_token);
        }
        if !telegram_token.is_empty() {
            cmd.env("AMUX_TELEGRAM_TOKEN", &telegram_token);
        }
        if !discord_token.is_empty() {
            cmd.env("AMUX_DISCORD_TOKEN", &discord_token);
        }

        match cmd.spawn() {
            Ok(child) => {
                tracing::info!(pid = ?child.id(), "gateway process started");
                *self.gateway_process.lock().await = Some(child);
            }
            Err(e) => {
                tracing::error!(error = %e, "failed to spawn gateway process");
            }
        }
    }

    /// Stop the gateway process.
    pub async fn stop_gateway(&self) {
        let mut proc = self.gateway_process.lock().await;
        if let Some(ref mut child) = *proc {
            tracing::info!("stopping gateway process");
            let _ = child.kill().await;
        }
        *proc = None;
    }

    /// Main background loop — processes tasks, runs heartbeats, polls gateway.
    pub async fn run_loop(self: Arc<Self>, mut shutdown: tokio::sync::watch::Receiver<bool>) {
        let config = self.config.read().await.clone();

        let task_interval = std::time::Duration::from_secs(config.task_poll_interval_secs);
        let heartbeat_interval =
            std::time::Duration::from_secs(config.heartbeat_interval_mins * 60);
        let gateway_poll_interval = std::time::Duration::from_secs(3);

        let mut task_tick = tokio::time::interval(task_interval);
        let mut heartbeat_tick = tokio::time::interval(heartbeat_interval);
        let mut gateway_tick = tokio::time::interval(gateway_poll_interval);

        tracing::info!(
            task_poll_secs = config.task_poll_interval_secs,
            heartbeat_mins = config.heartbeat_interval_mins,
            "agent background loop started"
        );

        loop {
            tokio::select! {
                _ = task_tick.tick() => {
                    self.clone().dispatch_goal_runs().await;
                    if let Err(e) = self.clone().dispatch_ready_tasks().await {
                        tracing::error!("agent task error: {e}");
                    }
                }
                _ = heartbeat_tick.tick() => {
                    if let Err(e) = self.run_heartbeat().await {
                        tracing::error!("agent heartbeat error: {e}");
                    }
                }
                _ = gateway_tick.tick() => {
                    // Skip built-in gateway polling when using an external agent
                    // — the external agent handles its own gateway connections
                    let backend = self.config.read().await.agent_backend.clone();
                    if backend != "openclaw" && backend != "hermes" {
                        self.poll_gateway_messages().await;
                    }
                }
                _ = shutdown.changed() => {
                    tracing::info!("agent background loop shutting down");
                    self.stop_gateway().await;
                    self.stop_external_agents().await;
                    break;
                }
            }
        }
    }

    /// Poll all gateway platforms for incoming messages and route to agent.
    async fn poll_gateway_messages(&self) {
        let mut gw_guard = self.gateway_state.lock().await;
        let gw = match gw_guard.as_mut() {
            Some(g) => g,
            None => return,
        };

        // Re-read channel lists from settings.json every cycle
        // so we pick up changes without restart
        let settings_path = self
            .data_dir
            .parent()
            .unwrap_or(std::path::Path::new("."))
            .join("settings.json");
        let (discord_channels, slack_channels) =
            match tokio::fs::read_to_string(&settings_path).await {
                Ok(raw) => {
                    let v: serde_json::Value = serde_json::from_str(&raw).unwrap_or_default();
                    let dc = parse_channel_filter(&read_setting_str(&v, "discordChannelFilter"));
                    let sc = parse_channel_filter(&read_setting_str(&v, "slackChannelFilter"));
                    (dc, sc)
                }
                Err(_) => (Vec::new(), Vec::new()),
            };

        // Collect messages from all platforms
        let mut incoming = Vec::new();

        if !gw.config.telegram_token.is_empty() {
            let telegram_msgs = gateway::poll_telegram(gw).await;
            if !telegram_msgs.is_empty() {
                tracing::info!(
                    count = telegram_msgs.len(),
                    "gateway: telegram messages received"
                );
            }
            incoming.extend(telegram_msgs);
        }

        if !slack_channels.is_empty() && !gw.config.slack_token.is_empty() {
            let slack_msgs = gateway::poll_slack(gw, &slack_channels).await;
            if !slack_msgs.is_empty() {
                tracing::info!(count = slack_msgs.len(), "gateway: slack messages received");
            }
            incoming.extend(slack_msgs);
        }

        if !discord_channels.is_empty() && !gw.config.discord_token.is_empty() {
            let discord_msgs = gateway::poll_discord(gw, &discord_channels).await;
            if !discord_msgs.is_empty() {
                tracing::info!(
                    count = discord_msgs.len(),
                    "gateway: discord messages received"
                );
            }
            incoming.extend(discord_msgs);
        }

        // Drop the mutex before processing (send_message needs it indirectly)
        drop(gw_guard);

        // Route each message to the agent
        for msg in incoming {
            tracing::info!(
                platform = %msg.platform,
                sender = %msg.sender,
                channel = %msg.channel,
                content = %msg.content,
                "gateway: incoming message"
            );

            // Handle control commands (reset/new conversation)
            let trimmed = msg.content.trim().to_lowercase();
            if trimmed == "!reset" || trimmed == "!new" || trimmed == "reset" || trimmed == "new" {
                let channel_key = format!("{}:{}", msg.platform, msg.channel);
                self.gateway_threads.write().await.remove(&channel_key);
                tracing::info!(channel_key = %channel_key, "gateway: conversation reset");

                // Send confirmation back
                let prompt = format!(
                    "The user typed '{}' in {} channel {}. \
                     This means they want to start a fresh conversation. \
                     Send a brief confirmation back using {} saying the conversation has been reset.",
                    msg.content, msg.platform, msg.channel,
                    match msg.platform.as_str() {
                        "Discord" => format!("send_discord_message with channel_id=\"{}\"", msg.channel),
                        "Slack" => format!("send_slack_message with channel=\"{}\"", msg.channel),
                        "Telegram" => format!("send_telegram_message with chat_id=\"{}\"", msg.channel),
                        _ => "the appropriate gateway tool".to_string(),
                    }
                );

                if let Err(e) = self.send_message(None, &prompt).await {
                    tracing::error!(error = %e, "gateway: failed to send reset confirmation");
                }
                continue;
            }

            let (reply_tool, reply_tool_name) = match msg.platform.as_str() {
                "Discord" => (
                    format!("send_discord_message with channel_id=\"{}\"", msg.channel),
                    "send_discord_message",
                ),
                "Slack" => (
                    format!("send_slack_message with channel=\"{}\"", msg.channel),
                    "send_slack_message",
                ),
                "Telegram" => (
                    format!("send_telegram_message with chat_id=\"{}\"", msg.channel),
                    "send_telegram_message",
                ),
                "WhatsApp" => (
                    format!("send_whatsapp_message with phone=\"{}\"", msg.channel),
                    "send_whatsapp_message",
                ),
                _ => (
                    "the appropriate gateway tool".to_string(),
                    "send_discord_message",
                ),
            };

            let prompt = format!(
                "[{platform} message from {sender}]: {content}\n\n\
                 YOU MUST CALL {reply_tool} to reply. Do NOT just write a text response — \
                 the user is on {platform} and will ONLY see messages sent via the tool. \
                 Your text response here is invisible to them. \
                 If you use other tools first (bash, read_file, etc), that's fine, \
                 but your FINAL action MUST be calling {reply_tool_short} to send the reply.",
                platform = msg.platform,
                sender = msg.sender,
                content = msg.content,
                reply_tool = reply_tool,
                reply_tool_short = reply_tool_name,
            );

            // Notify frontend about the incoming message (full content)
            let _ = self.event_tx.send(AgentEvent::GatewayIncoming {
                platform: msg.platform.clone(),
                sender: msg.sender.clone(),
                content: msg.content.clone(),
                channel: msg.channel.clone(),
            });

            // Use persistent thread per channel for conversation continuity
            let channel_key = format!("{}:{}", msg.platform, msg.channel);
            let existing_thread = self.gateway_threads.read().await.get(&channel_key).cloned();

            match self.send_message(existing_thread.as_deref(), &prompt).await {
                Ok(thread_id) => {
                    // Store the mapping so follow-up messages use the same thread
                    self.gateway_threads
                        .write()
                        .await
                        .insert(channel_key, thread_id.clone());

                    // Safety net: if the agent didn't call the gateway send tool,
                    // auto-send the last assistant message to the platform
                    let threads = self.threads.read().await;
                    if let Some(thread) = threads.get(&thread_id) {
                        let used_gateway_tool = thread.messages.iter().any(|m| {
                            m.role == MessageRole::Tool
                                && m.tool_name
                                    .as_deref()
                                    .map(|n| n.starts_with("send_"))
                                    .unwrap_or(false)
                        });

                        if !used_gateway_tool {
                            // Find the last assistant text response
                            let last_response = thread
                                .messages
                                .iter()
                                .rev()
                                .find(|m| m.role == MessageRole::Assistant && !m.content.is_empty())
                                .map(|m| m.content.clone());

                            if let Some(response_text) = last_response {
                                tracing::info!(
                                    platform = %msg.platform,
                                    "gateway: agent forgot to call send tool, auto-sending response"
                                );
                                drop(threads);

                                // Auto-send via the gateway tool
                                let auto_args = match msg.platform.as_str() {
                                    "Discord" => {
                                        serde_json::json!({"channel_id": msg.channel, "message": response_text})
                                    }
                                    "Slack" => {
                                        serde_json::json!({"channel": msg.channel, "message": response_text})
                                    }
                                    "Telegram" => {
                                        serde_json::json!({"chat_id": msg.channel, "message": response_text})
                                    }
                                    "WhatsApp" => {
                                        serde_json::json!({"phone": msg.channel, "message": response_text})
                                    }
                                    _ => serde_json::json!({"message": response_text}),
                                };

                                let auto_tool = ToolCall {
                                    id: format!("auto_{}", uuid::Uuid::new_v4()),
                                    function: ToolFunction {
                                        name: reply_tool_name.to_string(),
                                        arguments: auto_args.to_string(),
                                    },
                                };

                                let _ = tool_executor::execute_tool(
                                    &auto_tool,
                                    self,
                                    "",
                                    None,
                                    &self.session_manager,
                                    None,
                                    &self.event_tx,
                                    &self.data_dir,
                                    &self.http_client,
                                )
                                .await;
                            }
                        }
                    }
                }
                Err(e) => {
                    tracing::error!(
                        platform = %msg.platform,
                        error = %e,
                        "gateway: failed to process incoming message"
                    );
                }
            }
        }
    }

    /// Get or create a thread, returning the thread ID and whether it was newly created.
    async fn get_or_create_thread(&self, thread_id: Option<&str>, content: &str) -> (String, bool) {
        let given_id = thread_id.map(|s| s.to_string());
        let id = given_id.unwrap_or_else(|| format!("thread_{}", Uuid::new_v4()));
        let title = content.chars().take(50).collect::<String>();
        let mut created = false;

        let mut threads = self.threads.write().await;
        if !threads.contains_key(&id) {
            created = true;
            threads.insert(
                id.clone(),
                AgentThread {
                    id: id.clone(),
                    title: title.clone(),
                    messages: Vec::new(),
                    created_at: now_millis(),
                    updated_at: now_millis(),
                    total_input_tokens: 0,
                    total_output_tokens: 0,
                },
            );
            let _ = self.event_tx.send(AgentEvent::ThreadCreated {
                thread_id: id.clone(),
                title,
            });
        }
        drop(threads);
        (id, created)
    }

    // -----------------------------------------------------------------------
    // Agent turn (send message → LLM → tool loop → done)
    // -----------------------------------------------------------------------

    /// Run a complete agent turn in a thread.
    pub async fn send_message(&self, thread_id: Option<&str>, content: &str) -> Result<String> {
        Ok(self
            .send_message_inner(thread_id, content, None, None)
            .await?
            .thread_id)
    }

    async fn send_task_message(
        &self,
        task_id: &str,
        thread_id: Option<&str>,
        preferred_session_hint: Option<&str>,
        content: &str,
    ) -> Result<SendMessageOutcome> {
        self.send_message_inner(thread_id, content, Some(task_id), preferred_session_hint)
            .await
    }

    async fn send_message_inner(
        &self,
        thread_id: Option<&str>,
        content: &str,
        task_id: Option<&str>,
        preferred_session_hint: Option<&str>,
    ) -> Result<SendMessageOutcome> {
        let config = self.config.read().await.clone();

        // Route through external agent if backend is "openclaw" or "hermes"
        match config.agent_backend.as_str() {
            "openclaw" | "hermes" => {
                return self
                    .send_message_external(&config, thread_id, content)
                    .await
                    .map(|thread_id| SendMessageOutcome {
                        thread_id,
                        interrupted_for_approval: false,
                    });
            }
            _ => {} // Fall through to built-in daemon LLM client
        }

        // Resolve provider config
        let provider_config = self.resolve_provider_config(&config)?;

        // Get or create thread
        let (tid, is_new_thread) = self.get_or_create_thread(thread_id, content).await;

        // Add user message
        {
            let mut threads = self.threads.write().await;
            if let Some(thread) = threads.get_mut(&tid) {
                thread.messages.push(AgentMessage {
                    role: MessageRole::User,
                    content: content.into(),
                    tool_calls: None,
                    tool_call_id: None,
                    tool_name: None,
                    tool_arguments: None,
                    tool_status: None,
                    input_tokens: 0,
                    output_tokens: 0,
                    reasoning: None,
                    timestamp: now_millis(),
                });
                thread.updated_at = now_millis();
            }
        }

        let (stream_generation, stream_cancel_token) = self.begin_stream_cancellation(&tid).await;

        let onecontext_bootstrap = if is_new_thread {
            self.onecontext_bootstrap_for_new_thread(content).await
        } else {
            None
        };

        // Build system prompt with memory
        let memory = self.memory.read().await;
        let memory_dir = active_memory_dir(&self.data_dir);
        let mut system_prompt = build_system_prompt(&config.system_prompt, &memory, &memory_dir);
        drop(memory);
        if let Some(recall) = onecontext_bootstrap {
            system_prompt.push_str("\n\n## OneContext Recall\n");
            system_prompt
                .push_str("Use this as historical context from prior sessions when relevant:\n");
            system_prompt.push_str(&recall);
        }
        self.emit_workflow_notice(
            &tid,
            "memory-consulted",
            "Loaded persistent memory, user profile, and local skill paths for this turn.",
            Some(format!(
                "memory_dir={}; skills_dir={}",
                memory_dir.display(),
                skills_dir(&self.data_dir).display()
            )),
        );

        // Get tools
        let tools = get_available_tools(&config);
        let preferred_session_id =
            resolve_preferred_session_id(&self.session_manager, preferred_session_hint).await;

        // Run the agent loop
        let max_loops = config.max_tool_loops;
        let mut loop_count = 0u32;
        let mut was_cancelled = false;
        let mut interrupted_for_approval = false;

        'agent_loop: while loop_count < max_loops {
            if stream_cancel_token.is_cancelled() {
                was_cancelled = true;
                break;
            }
            loop_count += 1;

            // Build API messages from thread history
            let api_messages = {
                let threads = self.threads.read().await;
                let thread = match threads.get(&tid) {
                    Some(thread) => thread,
                    None => {
                        self.finish_stream_cancellation(&tid, stream_generation)
                            .await;
                        anyhow::bail!("thread not found");
                    }
                };
                messages_to_api_format(&thread.messages)
            };

            // Call LLM
            let llm_started_at = Instant::now();
            let mut first_token_at: Option<Instant> = None;
            let mut stream = send_chat_completion(
                &self.http_client,
                &config.provider,
                &provider_config,
                &system_prompt,
                &api_messages,
                &tools,
                config.max_retries,
                config.retry_delay_ms,
            );

            let mut accumulated_content = String::new();
            let mut accumulated_reasoning = String::new();
            let mut final_chunk: Option<CompletionChunk> = None;

            loop {
                tokio::select! {
                    _ = stream_cancel_token.cancelled() => {
                        was_cancelled = true;
                        break;
                    }
                    maybe_chunk = stream.next() => {
                        let Some(chunk_result) = maybe_chunk else {
                            break;
                        };

                        let chunk = match chunk_result {
                            Ok(chunk) => chunk,
                            Err(e) => {
                                self.finish_stream_cancellation(&tid, stream_generation).await;
                                return Err(e);
                            }
                        };

                        match chunk {
                            CompletionChunk::Delta { content, reasoning } => {
                                if first_token_at.is_none()
                                    && (!content.is_empty()
                                        || reasoning
                                            .as_ref()
                                            .map(|s| !s.is_empty())
                                            .unwrap_or(false))
                                {
                                    first_token_at = Some(Instant::now());
                                }
                                if !content.is_empty() {
                                    accumulated_content.push_str(&content);
                                    let _ = self.event_tx.send(AgentEvent::Delta {
                                        thread_id: tid.clone(),
                                        content,
                                    });
                                }
                                if let Some(r) = reasoning {
                                    accumulated_reasoning.push_str(&r);
                                    let _ = self.event_tx.send(AgentEvent::Reasoning {
                                        thread_id: tid.clone(),
                                        content: r,
                                    });
                                }
                            }
                            CompletionChunk::Retry {
                                attempt,
                                max_retries,
                                delay_ms,
                            } => {
                                let _ = self.event_tx.send(AgentEvent::Delta {
                                    thread_id: tid.clone(),
                                    content: format!(
                                        "\n[tamux] rate limited, running retry {attempt}/{max_retries} in {delay_ms}ms...\n"
                                    ),
                                });
                            }
                            chunk @ CompletionChunk::Done { .. } => {
                                final_chunk = Some(chunk);
                                break;
                            }
                            chunk @ CompletionChunk::ToolCalls { .. } => {
                                final_chunk = Some(chunk);
                                break;
                            }
                            CompletionChunk::Error { message } => {
                                let _ = self.event_tx.send(AgentEvent::Error {
                                    thread_id: tid.clone(),
                                    message: message.clone(),
                                });
                                // Add error as assistant message
                                self.add_assistant_message(&tid, &format!("Error: {message}"), 0, 0, None)
                                    .await;
                                self.persist_threads().await;
                                self.finish_stream_cancellation(&tid, stream_generation).await;
                                return Err(anyhow::anyhow!("LLM error: {message}"));
                            }
                        }
                    }
                }
            }

            if was_cancelled {
                break 'agent_loop;
            }

            match final_chunk {
                Some(CompletionChunk::Done {
                    content,
                    reasoning,
                    input_tokens,
                    output_tokens,
                }) => {
                    let final_content = if content.is_empty() {
                        accumulated_content
                    } else {
                        content
                    };
                    let final_reasoning = reasoning.or(if accumulated_reasoning.is_empty() {
                        None
                    } else {
                        Some(accumulated_reasoning)
                    });

                    self.add_assistant_message(
                        &tid,
                        &final_content,
                        input_tokens,
                        output_tokens,
                        final_reasoning,
                    )
                    .await;

                    let generation_secs = first_token_at
                        .unwrap_or(llm_started_at)
                        .elapsed()
                        .as_secs_f64();
                    let (generation_ms, tps) =
                        compute_generation_stats(generation_secs, output_tokens);

                    let _ = self.event_tx.send(AgentEvent::Done {
                        thread_id: tid.clone(),
                        input_tokens,
                        output_tokens,
                        cost: None,
                        provider: Some(config.provider.clone()),
                        model: Some(provider_config.model.clone()),
                        tps,
                        generation_ms,
                    });
                    break; // No tool calls, conversation turn is done
                }
                Some(CompletionChunk::ToolCalls {
                    tool_calls,
                    content,
                    reasoning,
                    input_tokens,
                    output_tokens,
                }) => {
                    // Add assistant message with tool calls
                    let msg_content = content.unwrap_or(accumulated_content.clone());
                    let msg_reasoning = reasoning.or(if accumulated_reasoning.is_empty() {
                        None
                    } else {
                        Some(accumulated_reasoning.clone())
                    });

                    {
                        let mut threads = self.threads.write().await;
                        if let Some(thread) = threads.get_mut(&tid) {
                            thread.messages.push(AgentMessage {
                                role: MessageRole::Assistant,
                                content: msg_content,
                                tool_calls: Some(tool_calls.clone()),
                                tool_call_id: None,
                                tool_name: None,
                                tool_arguments: None,
                                tool_status: None,
                                input_tokens: input_tokens.unwrap_or(0),
                                output_tokens: output_tokens.unwrap_or(0),
                                reasoning: msg_reasoning,
                                timestamp: now_millis(),
                            });
                            thread.total_input_tokens += input_tokens.unwrap_or(0);
                            thread.total_output_tokens += output_tokens.unwrap_or(0);
                        }
                    }

                    // Execute each tool call
                    for tc in &tool_calls {
                        if stream_cancel_token.is_cancelled() {
                            was_cancelled = true;
                            break;
                        }

                        let _ = self.event_tx.send(AgentEvent::ToolCall {
                            thread_id: tid.clone(),
                            call_id: tc.id.clone(),
                            name: tc.function.name.clone(),
                            arguments: tc.function.arguments.clone(),
                        });

                        let result = execute_tool(
                            tc,
                            self,
                            &tid,
                            task_id,
                            &self.session_manager,
                            preferred_session_id,
                            &self.event_tx,
                            &self.data_dir,
                            &self.http_client,
                        )
                        .await;

                        if tc.function.name == "update_memory" && !result.is_error {
                            self.refresh_memory_cache().await;
                        }

                        let _ = self.event_tx.send(AgentEvent::ToolResult {
                            thread_id: tid.clone(),
                            call_id: tc.id.clone(),
                            name: result.name.clone(),
                            content: result.content.clone(),
                            is_error: result.is_error,
                        });

                        // Add tool result message
                        {
                            let tool_status = if result.is_error { "error" } else { "done" };
                            let mut threads = self.threads.write().await;
                            if let Some(thread) = threads.get_mut(&tid) {
                                thread.messages.push(AgentMessage {
                                    role: MessageRole::Tool,
                                    content: result.content,
                                    tool_calls: None,
                                    tool_call_id: Some(result.tool_call_id),
                                    tool_name: Some(result.name),
                                    tool_arguments: Some(tc.function.arguments.clone()),
                                    tool_status: Some(tool_status.to_string()),
                                    input_tokens: 0,
                                    output_tokens: 0,
                                    reasoning: None,
                                    timestamp: now_millis(),
                                });
                            }
                        }

                        if let Some(pending_approval) = result.pending_approval.as_ref() {
                            interrupted_for_approval = true;
                            if let Some(task_id) = task_id {
                                self.mark_task_awaiting_approval(task_id, &tid, pending_approval)
                                    .await;
                            }
                            break 'agent_loop;
                        }

                        if stream_cancel_token.is_cancelled() {
                            was_cancelled = true;
                            break;
                        }
                    }

                    if was_cancelled {
                        break 'agent_loop;
                    }

                    // Loop continues — next iteration will include tool results in context
                }
                _ => {
                    // Stream ended unexpectedly
                    self.add_assistant_message(&tid, &accumulated_content, 0, 0, None)
                        .await;
                    break;
                }
            }
        }

        if !was_cancelled && loop_count >= max_loops {
            let _ = self.event_tx.send(AgentEvent::Error {
                thread_id: tid.clone(),
                message: "Tool execution limit reached".into(),
            });
        }

        self.persist_threads().await;
        self.finish_stream_cancellation(&tid, stream_generation)
            .await;
        Ok(SendMessageOutcome {
            thread_id: tid,
            interrupted_for_approval,
        })
    }

    // -----------------------------------------------------------------------
    // Task queue
    // -----------------------------------------------------------------------

    pub async fn start_goal_run(
        &self,
        goal: String,
        title: Option<String>,
        thread_id: Option<String>,
        session_id: Option<String>,
        priority: Option<&str>,
    ) -> GoalRun {
        let now = now_millis();
        let normalized_title = title
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| summarize_goal_title(&goal));
        let goal_run = GoalRun {
            id: format!("goal_{}", Uuid::new_v4()),
            title: normalized_title,
            goal,
            status: GoalRunStatus::Queued,
            priority: parse_priority_str(priority.unwrap_or("normal")),
            created_at: now,
            updated_at: now,
            started_at: None,
            completed_at: None,
            thread_id,
            session_id,
            current_step_index: 0,
            current_step_title: None,
            current_step_kind: None,
            replan_count: 0,
            max_replans: 2,
            plan_summary: None,
            reflection_summary: None,
            memory_updates: Vec::new(),
            generated_skill_path: None,
            last_error: None,
            failure_cause: None,
            awaiting_approval_id: None,
            active_task_id: None,
            duration_ms: None,
            child_task_ids: Vec::new(),
            child_task_count: 0,
            approval_count: 0,
            steps: Vec::new(),
            events: vec![make_goal_run_event("queue", "goal run created", None)],
        };

        self.goal_runs.lock().await.push_back(goal_run.clone());
        self.persist_goal_runs().await;
        self.emit_goal_run_update(&goal_run, Some("Goal queued".into()));
        self.project_goal_run(goal_run).await
    }

    pub async fn list_goal_runs(&self) -> Vec<GoalRun> {
        let goal_runs = self.goal_runs.lock().await;
        let mut items: Vec<GoalRun> = goal_runs.iter().cloned().collect();
        drop(goal_runs);
        let mut projected = Vec::with_capacity(items.len());
        for goal_run in items.drain(..) {
            projected.push(self.project_goal_run(goal_run).await);
        }
        projected.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        projected
    }

    pub async fn get_goal_run(&self, goal_run_id: &str) -> Option<GoalRun> {
        let goal_run = self
            .goal_runs
            .lock()
            .await
            .iter()
            .find(|goal_run| goal_run.id == goal_run_id)
            .cloned()?;
        Some(self.project_goal_run(goal_run).await)
    }

    pub async fn list_todos(&self) -> HashMap<String, Vec<TodoItem>> {
        self.thread_todos.read().await.clone()
    }

    pub async fn get_todos(&self, thread_id: &str) -> Vec<TodoItem> {
        self.thread_todos
            .read()
            .await
            .get(thread_id)
            .cloned()
            .unwrap_or_default()
    }

    async fn project_goal_run(&self, goal_run: GoalRun) -> GoalRun {
        let tasks = self.tasks.lock().await;
        let related_tasks = tasks
            .iter()
            .filter(|task| task.goal_run_id.as_deref() == Some(goal_run.id.as_str()))
            .cloned()
            .collect::<Vec<_>>();
        project_goal_run_snapshot(goal_run, &related_tasks, now_millis())
    }

    pub async fn control_goal_run(
        &self,
        goal_run_id: &str,
        action: &str,
        step_index: Option<usize>,
    ) -> bool {
        let mut changed_goal: Option<GoalRun> = None;
        let mut task_to_cancel: Option<String> = None;
        {
            let mut goal_runs = self.goal_runs.lock().await;
            let Some(goal_run) = goal_runs.iter_mut().find(|item| item.id == goal_run_id) else {
                return false;
            };

            match action {
                "pause" => {
                    if matches!(
                        goal_run.status,
                        GoalRunStatus::Queued
                            | GoalRunStatus::Planning
                            | GoalRunStatus::Running
                            | GoalRunStatus::AwaitingApproval
                    ) {
                        goal_run.status = GoalRunStatus::Paused;
                        goal_run.updated_at = now_millis();
                        goal_run.events.push(make_goal_run_event(
                            "control",
                            "goal run paused",
                            None,
                        ));
                        changed_goal = Some(goal_run.clone());
                    }
                }
                "resume" => {
                    if goal_run.status == GoalRunStatus::Paused {
                        goal_run.status = if goal_run.steps.is_empty() {
                            GoalRunStatus::Queued
                        } else {
                            GoalRunStatus::Running
                        };
                        goal_run.updated_at = now_millis();
                        goal_run.events.push(make_goal_run_event(
                            "control",
                            "goal run resumed",
                            None,
                        ));
                        changed_goal = Some(goal_run.clone());
                    }
                }
                "retry_step" | "retry-step" => {
                    let target_index = resolve_goal_run_control_step(goal_run, step_index);
                    task_to_cancel = goal_run
                        .steps
                        .get(target_index)
                        .and_then(|step| step.task_id.clone());
                    if retry_goal_run_step(goal_run, step_index).is_ok() {
                        goal_run.updated_at = now_millis();
                        goal_run.awaiting_approval_id = None;
                        goal_run.active_task_id = None;
                        goal_run.events.push(make_goal_run_event(
                            "control",
                            "goal run step retry requested",
                            step_index.map(|value| format!("step {value}")),
                        ));
                        changed_goal = Some(goal_run.clone());
                    }
                }
                "rerun_from_step" | "rerun-from-step" => {
                    let target_index = resolve_goal_run_control_step(goal_run, step_index);
                    task_to_cancel = goal_run
                        .steps
                        .get(target_index)
                        .and_then(|step| step.task_id.clone());
                    if rerun_goal_run_from_step(goal_run, step_index).is_ok() {
                        goal_run.updated_at = now_millis();
                        goal_run.awaiting_approval_id = None;
                        goal_run.active_task_id = None;
                        goal_run.events.push(make_goal_run_event(
                            "control",
                            "goal run rerun requested",
                            step_index.map(|value| format!("step {value}")),
                        ));
                        changed_goal = Some(goal_run.clone());
                    }
                }
                "cancel" => {
                    if !matches!(
                        goal_run.status,
                        GoalRunStatus::Completed | GoalRunStatus::Failed | GoalRunStatus::Cancelled
                    ) {
                        goal_run.status = GoalRunStatus::Cancelled;
                        goal_run.completed_at = Some(now_millis());
                        goal_run.updated_at = now_millis();
                        goal_run.events.push(make_goal_run_event(
                            "control",
                            "goal run cancelled",
                            None,
                        ));
                        goal_run.awaiting_approval_id = None;
                        goal_run.active_task_id = None;
                        task_to_cancel = goal_run
                            .steps
                            .get(goal_run.current_step_index)
                            .and_then(|step| step.task_id.clone());
                        changed_goal = Some(goal_run.clone());
                    }
                }
                _ => {}
            }
        }

        if let Some(task_id) = task_to_cancel {
            let _ = self.cancel_task(&task_id).await;
        }

        if let Some(goal_run) = changed_goal {
            self.persist_goal_runs().await;
            self.emit_goal_run_update(&goal_run, Some(goal_run_status_message(&goal_run).into()));
            return true;
        }

        false
    }

    pub async fn add_task(
        &self,
        title: String,
        description: String,
        priority: &str,
        command: Option<String>,
        session_id: Option<String>,
        dependencies: Vec<String>,
    ) -> String {
        self.enqueue_task(
            title,
            description,
            priority,
            command,
            session_id,
            dependencies,
            None,
            "user",
            None,
        )
        .await
        .id
    }

    pub async fn enqueue_task(
        &self,
        title: String,
        description: String,
        priority: &str,
        command: Option<String>,
        session_id: Option<String>,
        dependencies: Vec<String>,
        scheduled_at: Option<u64>,
        source: &str,
        goal_run_id: Option<String>,
    ) -> AgentTask {
        let id = format!("task_{}", Uuid::new_v4());
        let now = now_millis();
        let initial_schedule_reason = scheduled_at
            .filter(|deadline| *deadline > now)
            .map(describe_scheduled_time);
        let task = AgentTask {
            id: id.clone(),
            title,
            description,
            status: if initial_schedule_reason.is_some() {
                TaskStatus::Blocked
            } else {
                TaskStatus::Queued
            },
            priority: parse_priority_str(priority),
            progress: 0,
            created_at: now,
            started_at: None,
            completed_at: None,
            error: None,
            result: None,
            thread_id: None,
            source: source.into(),
            notify_on_complete: true,
            notify_channels: vec!["in-app".into()],
            dependencies,
            command,
            session_id,
            goal_run_id,
            retry_count: 0,
            max_retries: self.config.read().await.max_retries.max(1),
            next_retry_at: None,
            scheduled_at,
            blocked_reason: initial_schedule_reason.clone(),
            awaiting_approval_id: None,
            lane_id: None,
            last_error: None,
            logs: vec![make_task_log_entry(
                0,
                TaskLogLevel::Info,
                "queue",
                if initial_schedule_reason.is_some() {
                    "task scheduled"
                } else {
                    "task enqueued"
                },
                initial_schedule_reason,
            )],
        };

        self.tasks.lock().await.push_back(task);
        self.persist_tasks().await;

        let task = self
            .tasks
            .lock()
            .await
            .iter()
            .find(|task| task.id == id)
            .cloned()
            .expect("enqueued task missing from queue");
        self.emit_task_update(&task, Some(status_message(&task).into()));

        task
    }

    pub async fn cancel_task(&self, task_id: &str) -> bool {
        let mut tasks = self.tasks.lock().await;
        if let Some(task) = tasks.iter_mut().find(|t| t.id == task_id) {
            if matches!(
                task.status,
                TaskStatus::Queued
                    | TaskStatus::InProgress
                    | TaskStatus::Blocked
                    | TaskStatus::FailedAnalyzing
                    | TaskStatus::AwaitingApproval
            ) {
                let thread_to_stop = task.thread_id.clone();
                let session_to_interrupt = task.session_id.clone();
                task.status = TaskStatus::Cancelled;
                task.completed_at = Some(now_millis());
                task.lane_id = None;
                task.blocked_reason = None;
                task.awaiting_approval_id = None;
                task.logs.push(make_task_log_entry(
                    task.retry_count,
                    TaskLogLevel::Warn,
                    "queue",
                    "task cancelled by user",
                    None,
                ));
                let updated = task.clone();
                drop(tasks);
                self.persist_tasks().await;
                if let Some(thread_id) = thread_to_stop {
                    let _ = self.stop_stream(&thread_id).await;
                }
                if let Some(session_id) =
                    session_to_interrupt.and_then(|value| Uuid::parse_str(&value).ok())
                {
                    let _ = self.session_manager.write_input(session_id, &[3]).await;
                }
                self.emit_task_update(&updated, Some("Cancelled by user".into()));
                return true;
            }
        }
        false
    }

    pub async fn handle_task_approval_resolution(
        &self,
        approval_id: &str,
        decision: amux_protocol::ApprovalDecision,
    ) -> bool {
        let updated = {
            let mut tasks = self.tasks.lock().await;
            let Some(task) = tasks
                .iter_mut()
                .find(|task| task.awaiting_approval_id.as_deref() == Some(approval_id))
            else {
                return false;
            };

            match decision {
                amux_protocol::ApprovalDecision::ApproveOnce
                | amux_protocol::ApprovalDecision::ApproveSession => {
                    task.status = TaskStatus::Queued;
                    task.started_at = None;
                    task.awaiting_approval_id = None;
                    task.blocked_reason = None;
                    task.logs.push(make_task_log_entry(
                        task.retry_count,
                        TaskLogLevel::Info,
                        "approval",
                        "operator approved managed command; task re-queued",
                        None,
                    ));
                }
                amux_protocol::ApprovalDecision::Deny => {
                    let reason = "operator denied managed command approval".to_string();
                    task.status = TaskStatus::Failed;
                    task.started_at = None;
                    task.completed_at = Some(now_millis());
                    task.awaiting_approval_id = None;
                    task.blocked_reason = Some(reason.clone());
                    task.error = Some(reason.clone());
                    task.last_error = Some(reason.clone());
                    task.logs.push(make_task_log_entry(
                        task.retry_count,
                        TaskLogLevel::Error,
                        "approval",
                        "operator denied managed command; task failed",
                        Some(reason),
                    ));
                }
            }

            task.clone()
        };

        self.persist_tasks().await;
        self.emit_task_update(&updated, Some(status_message(&updated).into()));
        true
    }

    pub async fn list_tasks(&self) -> Vec<AgentTask> {
        let sessions = self.session_manager.list().await;
        let mut tasks = self.tasks.lock().await;
        let changed = refresh_task_queue_state(&mut tasks, now_millis(), &sessions);
        let snapshot = tasks.iter().cloned().collect();
        drop(tasks);

        if !changed.is_empty() {
            self.persist_tasks().await;
            for task in changed {
                self.emit_task_update(&task, Some(status_message(&task).into()));
            }
        }

        snapshot
    }

    async fn dispatch_goal_runs(self: Arc<Self>) {
        let goal_run_ids = {
            let goal_runs = self.goal_runs.lock().await;
            goal_runs
                .iter()
                .filter(|goal_run| {
                    !matches!(
                        goal_run.status,
                        GoalRunStatus::Paused
                            | GoalRunStatus::Completed
                            | GoalRunStatus::Failed
                            | GoalRunStatus::Cancelled
                    )
                })
                .map(|goal_run| goal_run.id.clone())
                .collect::<Vec<_>>()
        };

        for goal_run_id in goal_run_ids {
            if !self.try_begin_goal_run_work(&goal_run_id).await {
                continue;
            }

            let engine = self.clone();
            tokio::spawn(async move {
                let result = engine.advance_goal_run(&goal_run_id).await;
                if let Err(error) = result {
                    tracing::error!(goal_run_id = %goal_run_id, error = %error, "goal run advancement failed");
                    engine
                        .fail_goal_run(&goal_run_id, &error.to_string(), "goal-run")
                        .await;
                }
                engine.finish_goal_run_work(&goal_run_id).await;
            });
        }
    }

    async fn try_begin_goal_run_work(&self, goal_run_id: &str) -> bool {
        let mut inflight = self.inflight_goal_runs.lock().await;
        inflight.insert(goal_run_id.to_string())
    }

    async fn finish_goal_run_work(&self, goal_run_id: &str) {
        self.inflight_goal_runs.lock().await.remove(goal_run_id);
    }

    async fn advance_goal_run(&self, goal_run_id: &str) -> Result<()> {
        let goal_run = match self.get_goal_run(goal_run_id).await {
            Some(goal_run) => goal_run,
            None => return Ok(()),
        };

        if goal_run.status == GoalRunStatus::Queued && goal_run.steps.is_empty() {
            self.plan_goal_run(goal_run_id).await?;
            return Ok(());
        }

        if goal_run.current_step_index >= goal_run.steps.len() {
            self.complete_goal_run(goal_run_id).await?;
            return Ok(());
        }

        let current_step = goal_run.steps[goal_run.current_step_index].clone();
        if current_step.task_id.is_none() {
            self.enqueue_goal_run_step(goal_run_id).await?;
            return Ok(());
        }

        let task_id = current_step.task_id.as_deref().unwrap_or_default();
        let task = {
            let tasks = self.tasks.lock().await;
            tasks.iter().find(|task| task.id == task_id).cloned()
        };

        let Some(task) = task else {
            self.requeue_goal_run_step(goal_run_id, &format!("child task {task_id} disappeared"))
                .await;
            return Ok(());
        };

        match task.status {
            TaskStatus::Queued | TaskStatus::InProgress | TaskStatus::Blocked => {
                self.sync_goal_run_with_task(goal_run_id, &task).await;
            }
            TaskStatus::AwaitingApproval => {
                self.sync_goal_run_with_task(goal_run_id, &task).await;
            }
            TaskStatus::Completed => {
                self.handle_goal_run_step_completion(goal_run_id, &task)
                    .await?;
            }
            TaskStatus::Failed | TaskStatus::Cancelled => {
                self.handle_goal_run_step_failure(goal_run_id, &task)
                    .await?;
            }
            TaskStatus::FailedAnalyzing => {
                self.sync_goal_run_with_task(goal_run_id, &task).await;
            }
        }

        Ok(())
    }

    async fn dispatch_ready_tasks(self: Arc<Self>) -> Result<()> {
        let now = now_millis();
        let sessions = self.session_manager.list().await;
        let (changed_before_start, dispatched_tasks) = {
            let mut tasks = self.tasks.lock().await;
            let changed_before_start = refresh_task_queue_state(&mut tasks, now, &sessions);
            let next_indices = select_ready_task_indices(&tasks, &sessions);
            if next_indices.is_empty() {
                drop(tasks);
                if !changed_before_start.is_empty() {
                    self.persist_tasks().await;
                    for task in changed_before_start {
                        self.emit_task_update(&task, Some(status_message(&task).into()));
                    }
                }
                return Ok(());
            }

            let mut dispatched_tasks = Vec::with_capacity(next_indices.len());
            for index in next_indices {
                let task = &mut tasks[index];
                let lane_id = task_lane_key(task);
                task.status = TaskStatus::InProgress;
                task.started_at = Some(now);
                task.completed_at = None;
                task.progress = task.progress.max(5);
                task.blocked_reason = None;
                task.awaiting_approval_id = None;
                task.lane_id = Some(lane_id.clone());
                task.logs.push(make_task_log_entry(
                    task.retry_count,
                    TaskLogLevel::Info,
                    "execution",
                    &format!("task dispatched to {lane_id} lane"),
                    None,
                ));
                dispatched_tasks.push(task.clone());
            }
            (changed_before_start, dispatched_tasks)
        };

        self.persist_tasks().await;
        for changed in changed_before_start {
            self.emit_task_update(&changed, Some(status_message(&changed).into()));
        }
        for task in dispatched_tasks {
            self.emit_task_update(&task, Some(format!("Starting: {}", task.title)));
            let engine = self.clone();
            tokio::spawn(async move {
                if let Err(error) = engine.execute_dispatched_task(task).await {
                    tracing::error!(error = %error, "agent task execution error");
                }
            });
        }

        Ok(())
    }

    async fn execute_dispatched_task(&self, task: AgentTask) -> Result<()> {
        match self
            .send_task_message(
                &task.id,
                task.thread_id.as_deref(),
                task.session_id.as_deref(),
                &build_task_prompt(&task),
            )
            .await
        {
            Ok(outcome) if outcome.interrupted_for_approval => Ok(()),
            Ok(outcome) => {
                let now = now_millis();
                let updated = {
                    let mut tasks = self.tasks.lock().await;
                    if let Some(current) = tasks.iter_mut().find(|entry| entry.id == task.id) {
                        current.status = TaskStatus::Completed;
                        current.progress = 100;
                        current.completed_at = Some(now);
                        current.thread_id = Some(outcome.thread_id);
                        current.lane_id = None;
                        current.blocked_reason = None;
                        current.awaiting_approval_id = None;
                        current.error = None;
                        current.last_error = None;
                        current.next_retry_at = None;
                        current.logs.push(make_task_log_entry(
                            current.retry_count,
                            TaskLogLevel::Info,
                            "execution",
                            if current.retry_count > 0 {
                                "task self-healed and completed"
                            } else {
                                "task completed"
                            },
                            None,
                        ));
                        current.clone()
                    } else {
                        return Ok(());
                    }
                };
                self.persist_tasks().await;
                self.emit_task_update(
                    &updated,
                    Some(if updated.retry_count > 0 {
                        "Task self-healed and completed".into()
                    } else {
                        "Task completed".into()
                    }),
                );
                Ok(())
            }
            Err(error) => {
                let error_text = error.to_string();
                let retry_delay_ms = compute_task_backoff_ms(
                    self.config.read().await.retry_delay_ms,
                    task.retry_count.saturating_add(1),
                );
                let updated = {
                    let mut tasks = self.tasks.lock().await;
                    if let Some(current) = tasks.iter_mut().find(|entry| entry.id == task.id) {
                        current.retry_count = current.retry_count.saturating_add(1);
                        current.error = Some(error_text.clone());
                        current.last_error = Some(error_text.clone());
                        current.progress = 0;
                        current.lane_id = None;
                        current.logs.push(make_task_log_entry(
                            current.retry_count,
                            TaskLogLevel::Error,
                            "execution",
                            "task execution failed",
                            Some(error_text.clone()),
                        ));

                        if current.retry_count <= current.max_retries {
                            current.status = TaskStatus::FailedAnalyzing;
                            current.completed_at = None;
                            current.next_retry_at =
                                Some(now_millis().saturating_add(retry_delay_ms));
                            current.blocked_reason = Some(format!(
                                "retry {} of {} scheduled in {}s",
                                current.retry_count,
                                current.max_retries,
                                ((retry_delay_ms + 999) / 1000).max(1),
                            ));
                            current.logs.push(make_task_log_entry(
                                current.retry_count,
                                TaskLogLevel::Warn,
                                "analysis",
                                "agent queued self-healing retry",
                                current.blocked_reason.clone(),
                            ));
                        } else {
                            current.status = TaskStatus::Failed;
                            current.completed_at = Some(now_millis());
                            current.next_retry_at = None;
                            current.blocked_reason = Some("retry budget exhausted".into());
                            current.logs.push(make_task_log_entry(
                                current.retry_count,
                                TaskLogLevel::Error,
                                "analysis",
                                "task failed permanently after exhausting retry budget",
                                Some(error_text.clone()),
                            ));
                        }
                        current.clone()
                    } else {
                        return Ok(());
                    }
                };

                self.persist_tasks().await;
                self.emit_task_update(
                    &updated,
                    Some(match updated.status {
                        TaskStatus::FailedAnalyzing => {
                            format!("Attempt {} failed; retry scheduled", updated.retry_count)
                        }
                        _ => format!("Failed: {error_text}"),
                    }),
                );
                Ok(())
            }
        }
    }

    fn emit_task_update(&self, task: &AgentTask, message: Option<String>) {
        let _ = self.event_tx.send(AgentEvent::TaskUpdate {
            task_id: task.id.clone(),
            status: task.status,
            progress: task.progress,
            message,
            task: Some(task.clone()),
        });
    }

    fn emit_goal_run_update(&self, goal_run: &GoalRun, message: Option<String>) {
        let _ = self.event_tx.send(AgentEvent::GoalRunUpdate {
            goal_run_id: goal_run.id.clone(),
            status: goal_run.status,
            current_step_index: Some(goal_run.current_step_index),
            message,
            goal_run: Some(goal_run.clone()),
        });
    }

    fn emit_todo_update(
        &self,
        thread_id: &str,
        goal_run_id: Option<String>,
        step_index: Option<usize>,
        items: Vec<TodoItem>,
    ) {
        let _ = self.event_tx.send(AgentEvent::TodoUpdate {
            thread_id: thread_id.to_string(),
            goal_run_id,
            step_index,
            items,
        });
    }

    fn emit_workflow_notice(
        &self,
        thread_id: &str,
        kind: &str,
        message: impl Into<String>,
        details: Option<String>,
    ) {
        let _ = self.event_tx.send(AgentEvent::WorkflowNotice {
            thread_id: thread_id.to_string(),
            kind: kind.to_string(),
            message: message.into(),
            details,
        });
    }

    pub async fn replace_thread_todos(
        &self,
        thread_id: &str,
        mut items: Vec<TodoItem>,
        task_id: Option<&str>,
    ) {
        let now = now_millis();
        for (index, item) in items.iter_mut().enumerate() {
            item.position = index;
            if item.created_at == 0 {
                item.created_at = now;
            }
            item.updated_at = now;
        }

        {
            let mut todos = self.thread_todos.write().await;
            todos.insert(thread_id.to_string(), items.clone());
        }
        self.persist_todos().await;

        let mut goal_run_update = None;
        let mut goal_run_id = None;
        let mut step_index = None;
        if let Some(task_id) = task_id {
            goal_run_update = self.record_goal_run_todo_snapshot(task_id, &items).await;
            if let Some(goal_run) = goal_run_update.as_ref() {
                goal_run_id = Some(goal_run.id.clone());
                step_index = Some(goal_run.current_step_index);
            }
        }

        self.emit_todo_update(thread_id, goal_run_id, step_index, items);

        if let Some(goal_run) = goal_run_update {
            self.persist_goal_runs().await;
            self.emit_goal_run_update(&goal_run, Some("Goal todo updated".into()));
        }
    }

    async fn record_goal_run_todo_snapshot(
        &self,
        task_id: &str,
        items: &[TodoItem],
    ) -> Option<GoalRun> {
        let goal_run_id = {
            let tasks = self.tasks.lock().await;
            tasks
                .iter()
                .find(|task| task.id == task_id)
                .and_then(|task| task.goal_run_id.clone())
        }?;

        let mut goal_runs = self.goal_runs.lock().await;
        let goal_run = goal_runs
            .iter_mut()
            .find(|goal_run| goal_run.id == goal_run_id)?;
        goal_run.updated_at = now_millis();
        goal_run.events.push(make_goal_run_event_with_todos(
            "todo",
            "goal todo updated",
            None,
            Some(goal_run.current_step_index),
            items.to_vec(),
        ));
        Some(goal_run.clone())
    }

    async fn mark_task_awaiting_approval(
        &self,
        task_id: &str,
        thread_id: &str,
        pending_approval: &ToolPendingApproval,
    ) {
        let updated = {
            let mut tasks = self.tasks.lock().await;
            let Some(task) = tasks.iter_mut().find(|entry| entry.id == task_id) else {
                return;
            };

            let reason = format!(
                "waiting for operator approval: {}",
                pending_approval.command
            );
            task.status = TaskStatus::AwaitingApproval;
            task.thread_id = Some(thread_id.to_string());
            if task.session_id.is_none() {
                task.session_id = pending_approval.session_id.clone();
            }
            task.awaiting_approval_id = Some(pending_approval.approval_id.clone());
            task.blocked_reason = Some(reason.clone());
            task.error = None;
            task.last_error = None;
            task.progress = task.progress.max(35);
            task.logs.push(make_task_log_entry(
                task.retry_count,
                TaskLogLevel::Warn,
                "approval",
                "managed command paused for operator approval",
                Some(reason),
            ));
            task.clone()
        };

        self.persist_tasks().await;
        self.emit_task_update(&updated, Some("Task awaiting approval".into()));
    }

    async fn plan_goal_run(&self, goal_run_id: &str) -> Result<()> {
        let goal_run = self
            .get_goal_run(goal_run_id)
            .await
            .context("goal run missing during planning")?;

        let queued = {
            let mut goal_runs = self.goal_runs.lock().await;
            let Some(current) = goal_runs.iter_mut().find(|item| item.id == goal_run_id) else {
                anyhow::bail!("goal run disappeared during planning");
            };
            current.status = GoalRunStatus::Planning;
            current.started_at.get_or_insert(now_millis());
            current.updated_at = now_millis();
            current.events.push(make_goal_run_event(
                "planning",
                "building execution plan",
                None,
            ));
            current.clone()
        };
        self.persist_goal_runs().await;
        self.emit_goal_run_update(&queued, Some("Planning goal".into()));

        let plan = self.request_goal_plan(&goal_run).await?;
        let now = now_millis();
        let updated = {
            let mut goal_runs = self.goal_runs.lock().await;
            let Some(current) = goal_runs.iter_mut().find(|item| item.id == goal_run_id) else {
                anyhow::bail!("goal run disappeared after planning");
            };
            let default_session_id = current.session_id.clone();
            if let Some(title) = plan
                .title
                .as_deref()
                .filter(|value| !value.trim().is_empty())
            {
                current.title = title.trim().to_string();
            }
            current.plan_summary = Some(plan.summary.clone());
            current.steps = plan
                .steps
                .into_iter()
                .enumerate()
                .map(|(position, step)| GoalRunStep {
                    id: format!("goal_step_{}", Uuid::new_v4()),
                    position,
                    title: step.title,
                    instructions: step.instructions,
                    kind: step.kind,
                    success_criteria: step.success_criteria,
                    session_id: step.session_id.or_else(|| default_session_id.clone()),
                    status: GoalRunStepStatus::Pending,
                    task_id: None,
                    summary: None,
                    error: None,
                    started_at: None,
                    completed_at: None,
                })
                .collect();
            current.current_step_index = 0;
            current.current_step_title = current.steps.first().map(|step| step.title.clone());
            current.current_step_kind = current.steps.first().map(|step| step.kind);
            current.status = GoalRunStatus::Running;
            current.updated_at = now;
            current.last_error = None;
            current.failure_cause = None;
            current.awaiting_approval_id = None;
            current.active_task_id = None;
            current.events.push(make_goal_run_event(
                "planning",
                "goal plan generated",
                current.plan_summary.clone(),
            ));
            current.clone()
        };
        self.persist_goal_runs().await;
        self.emit_goal_run_update(&updated, Some("Goal plan ready".into()));
        Ok(())
    }

    async fn enqueue_goal_run_step(&self, goal_run_id: &str) -> Result<()> {
        let snapshot = self
            .get_goal_run(goal_run_id)
            .await
            .context("goal run missing while enqueuing step")?;
        if snapshot.current_step_index >= snapshot.steps.len() {
            return Ok(());
        }

        let step = snapshot.steps[snapshot.current_step_index].clone();
        let task = self
            .enqueue_task(
                format!("{}: {}", snapshot.title, step.title),
                step.instructions.clone(),
                task_priority_to_str(snapshot.priority),
                None,
                step.session_id
                    .clone()
                    .or_else(|| snapshot.session_id.clone()),
                Vec::new(),
                None,
                "goal_run",
                Some(snapshot.id.clone()),
            )
            .await;

        let updated = {
            let mut goal_runs = self.goal_runs.lock().await;
            let Some(goal_run) = goal_runs.iter_mut().find(|item| item.id == goal_run_id) else {
                anyhow::bail!("goal run disappeared after task enqueue");
            };
            if let Some(current_step) = goal_run.steps.get_mut(goal_run.current_step_index) {
                current_step.task_id = Some(task.id.clone());
                current_step.status = GoalRunStepStatus::InProgress;
                current_step.started_at = Some(now_millis());
            }
            if !goal_run.child_task_ids.iter().any(|id| id == &task.id) {
                goal_run.child_task_ids.push(task.id.clone());
            }
            goal_run.child_task_count = goal_run.child_task_ids.len() as u32;
            goal_run.status = GoalRunStatus::Running;
            goal_run.updated_at = now_millis();
            goal_run.current_step_title = Some(step.title.clone());
            goal_run.current_step_kind = Some(step.kind);
            goal_run.active_task_id = Some(task.id.clone());
            goal_run.awaiting_approval_id = None;
            goal_run.events.push(make_goal_run_event(
                "execution",
                "queued child task for goal step",
                Some(format!("{} -> {}", step.title, task.id)),
            ));
            goal_run.clone()
        };

        self.persist_goal_runs().await;
        self.emit_goal_run_update(&updated, Some(format!("Queued step: {}", step.title)));
        Ok(())
    }

    async fn sync_goal_run_with_task(&self, goal_run_id: &str, task: &AgentTask) {
        let mut maybe_updated = None;
        {
            let mut goal_runs = self.goal_runs.lock().await;
            if let Some(goal_run) = goal_runs.iter_mut().find(|item| item.id == goal_run_id) {
                let prior_status = goal_run.status;
                let next_status = if task.status == TaskStatus::AwaitingApproval {
                    GoalRunStatus::AwaitingApproval
                } else {
                    GoalRunStatus::Running
                };
                let mut changed = goal_run.status != next_status;
                goal_run.status = next_status;
                goal_run.updated_at = now_millis();
                goal_run.awaiting_approval_id = task.awaiting_approval_id.clone();
                goal_run.active_task_id = Some(task.id.clone());
                if let Some(step) = goal_run.steps.get_mut(goal_run.current_step_index) {
                    if step.status != GoalRunStepStatus::InProgress {
                        step.status = GoalRunStepStatus::InProgress;
                        step.started_at.get_or_insert(now_millis());
                        changed = true;
                    }
                }
                if next_status == GoalRunStatus::AwaitingApproval
                    && prior_status != GoalRunStatus::AwaitingApproval
                {
                    goal_run.events.push(make_goal_run_event(
                        "approval",
                        "goal step awaiting operator approval",
                        task.awaiting_approval_id.clone(),
                    ));
                    changed = true;
                }
                if changed {
                    maybe_updated = Some(goal_run.clone());
                }
            }
        }

        if let Some(updated) = maybe_updated {
            self.persist_goal_runs().await;
            self.emit_goal_run_update(&updated, Some(goal_run_status_message(&updated).into()));
        }
    }

    async fn handle_goal_run_step_completion(
        &self,
        goal_run_id: &str,
        task: &AgentTask,
    ) -> Result<()> {
        let now = now_millis();
        let thread_summary = match task.thread_id.as_deref() {
            Some(thread_id) => self.goal_thread_summary(thread_id).await,
            None => None,
        };
        let updated = {
            let mut goal_runs = self.goal_runs.lock().await;
            let Some(goal_run) = goal_runs.iter_mut().find(|item| item.id == goal_run_id) else {
                anyhow::bail!("goal run missing after task completion");
            };
            if let Some(thread_id) = task.thread_id.clone() {
                goal_run.thread_id = Some(thread_id);
            }
            if let Some(step) = goal_run.steps.get_mut(goal_run.current_step_index) {
                step.status = GoalRunStepStatus::Completed;
                step.completed_at = Some(now);
                step.summary = thread_summary
                    .clone()
                    .or_else(|| Some("step completed".into()));
            }
            goal_run.current_step_index = goal_run.current_step_index.saturating_add(1);
            let next_step = goal_run.steps.get(goal_run.current_step_index);
            goal_run.current_step_title = next_step.map(|step| step.title.clone());
            goal_run.current_step_kind = next_step.map(|step| step.kind);
            goal_run.status = GoalRunStatus::Running;
            goal_run.updated_at = now;
            goal_run.last_error = None;
            goal_run.failure_cause = None;
            goal_run.awaiting_approval_id = None;
            goal_run.active_task_id = None;
            goal_run.events.push(make_goal_run_event(
                "execution",
                "goal step completed",
                thread_summary.clone(),
            ));
            goal_run.clone()
        };

        self.persist_goal_runs().await;
        self.emit_goal_run_update(&updated, Some("Goal step completed".into()));

        if updated.current_step_index >= updated.steps.len() {
            self.complete_goal_run(goal_run_id).await?;
        }

        Ok(())
    }

    async fn handle_goal_run_step_failure(
        &self,
        goal_run_id: &str,
        task: &AgentTask,
    ) -> Result<()> {
        let snapshot = self
            .get_goal_run(goal_run_id)
            .await
            .context("goal run missing during failure handling")?;
        let failure = task
            .last_error
            .clone()
            .or_else(|| task.error.clone())
            .unwrap_or_else(|| format!("child task {} failed", task.id));

        if snapshot.replan_count < snapshot.max_replans
            && snapshot.current_step_index < snapshot.steps.len()
        {
            let revised = self.request_goal_replan(&snapshot, &failure).await?;
            let updated = {
                let mut goal_runs = self.goal_runs.lock().await;
                let Some(goal_run) = goal_runs.iter_mut().find(|item| item.id == goal_run_id)
                else {
                    anyhow::bail!("goal run disappeared during replan");
                };
                let default_session_id = goal_run.session_id.clone();
                if let Some(step) = goal_run.steps.get_mut(goal_run.current_step_index) {
                    step.status = GoalRunStepStatus::Failed;
                    step.completed_at = Some(now_millis());
                    step.error = Some(failure.clone());
                }
                let insert_at = goal_run.current_step_index.saturating_add(1);
                goal_run.steps.truncate(insert_at);
                for (offset, step) in revised.steps.into_iter().enumerate() {
                    goal_run.steps.push(GoalRunStep {
                        id: format!("goal_step_{}", Uuid::new_v4()),
                        position: insert_at + offset,
                        title: step.title,
                        instructions: step.instructions,
                        kind: step.kind,
                        success_criteria: step.success_criteria,
                        session_id: step.session_id.or_else(|| default_session_id.clone()),
                        status: GoalRunStepStatus::Pending,
                        task_id: None,
                        summary: None,
                        error: None,
                        started_at: None,
                        completed_at: None,
                    });
                }
                for (position, step) in goal_run.steps.iter_mut().enumerate() {
                    step.position = position;
                }
                goal_run.current_step_index = insert_at;
                let next_step = goal_run.steps.get(goal_run.current_step_index);
                goal_run.current_step_title = next_step.map(|step| step.title.clone());
                goal_run.current_step_kind = next_step.map(|step| step.kind);
                goal_run.replan_count = goal_run.replan_count.saturating_add(1);
                goal_run.status = GoalRunStatus::Running;
                goal_run.updated_at = now_millis();
                goal_run.last_error = Some(failure.clone());
                goal_run.failure_cause = Some(failure.clone());
                goal_run.reflection_summary = Some(revised.summary.clone());
                goal_run.awaiting_approval_id = None;
                goal_run.active_task_id = None;
                goal_run.events.push(make_goal_run_event(
                    "replan",
                    "goal plan revised after failed step",
                    Some(failure.clone()),
                ));
                goal_run.clone()
            };
            self.persist_goal_runs().await;
            self.emit_goal_run_update(&updated, Some("Goal replanned after failure".into()));
            return Ok(());
        }

        self.fail_goal_run(goal_run_id, &failure, "execution").await;
        Ok(())
    }

    async fn complete_goal_run(&self, goal_run_id: &str) -> Result<()> {
        let snapshot = self
            .get_goal_run(goal_run_id)
            .await
            .context("goal run missing during completion")?;
        let reflection = self.request_goal_reflection(&snapshot).await?;
        if let Some(update) = reflection.stable_memory_update.clone() {
            self.append_goal_memory_update(&update).await?;
        }
        let generated_skill_path = if reflection.generate_skill {
            let skill_title = reflection
                .skill_title
                .as_deref()
                .filter(|value| !value.trim().is_empty())
                .unwrap_or(snapshot.title.as_str());
            self.history
                .generate_skill(Some(snapshot.goal.as_str()), Some(skill_title))
                .ok()
                .map(|(_, path)| path)
        } else {
            None
        };

        let updated = {
            let mut goal_runs = self.goal_runs.lock().await;
            let Some(goal_run) = goal_runs.iter_mut().find(|item| item.id == goal_run_id) else {
                anyhow::bail!("goal run missing while finalizing");
            };
            goal_run.status = GoalRunStatus::Completed;
            goal_run.completed_at = Some(now_millis());
            goal_run.updated_at = now_millis();
            goal_run.reflection_summary = Some(reflection.summary.clone());
            goal_run.current_step_title = None;
            goal_run.current_step_kind = None;
            goal_run.awaiting_approval_id = None;
            goal_run.active_task_id = None;
            if let Some(update) = reflection.stable_memory_update {
                goal_run.memory_updates.push(update);
            }
            if let Some(path) = generated_skill_path {
                goal_run.generated_skill_path = Some(path);
            }
            goal_run.events.push(make_goal_run_event(
                "reflection",
                "goal run completed",
                goal_run.reflection_summary.clone(),
            ));
            goal_run.clone()
        };

        self.persist_goal_runs().await;
        self.emit_goal_run_update(&updated, Some("Goal completed".into()));
        Ok(())
    }

    async fn fail_goal_run(&self, goal_run_id: &str, error: &str, phase: &str) {
        let mut maybe_updated = None;
        {
            let mut goal_runs = self.goal_runs.lock().await;
            if let Some(goal_run) = goal_runs.iter_mut().find(|item| item.id == goal_run_id) {
                goal_run.status = GoalRunStatus::Failed;
                goal_run.completed_at = Some(now_millis());
                goal_run.updated_at = now_millis();
                goal_run.last_error = Some(error.to_string());
                goal_run.failure_cause = Some(error.to_string());
                goal_run.awaiting_approval_id = None;
                goal_run.active_task_id = None;
                goal_run.events.push(make_goal_run_event(
                    phase,
                    "goal run failed",
                    Some(error.to_string()),
                ));
                maybe_updated = Some(goal_run.clone());
            }
        }
        if let Some(updated) = maybe_updated {
            self.persist_goal_runs().await;
            self.emit_goal_run_update(&updated, Some(format!("Goal failed: {error}")));
        }
    }

    async fn requeue_goal_run_step(&self, goal_run_id: &str, reason: &str) {
        let mut maybe_updated = None;
        {
            let mut goal_runs = self.goal_runs.lock().await;
            if let Some(goal_run) = goal_runs.iter_mut().find(|item| item.id == goal_run_id) {
                if let Some(step) = goal_run.steps.get_mut(goal_run.current_step_index) {
                    step.task_id = None;
                    step.status = GoalRunStepStatus::Pending;
                    step.error = Some(reason.to_string());
                }
                goal_run.status = GoalRunStatus::Running;
                goal_run.updated_at = now_millis();
                goal_run.awaiting_approval_id = None;
                goal_run.active_task_id = None;
                goal_run.events.push(make_goal_run_event(
                    "execution",
                    "goal step returned to pending",
                    Some(reason.to_string()),
                ));
                maybe_updated = Some(goal_run.clone());
            }
        }
        if let Some(updated) = maybe_updated {
            self.persist_goal_runs().await;
            self.emit_goal_run_update(&updated, Some("Goal step re-queued".into()));
        }
    }

    // -----------------------------------------------------------------------
    // Heartbeat
    // -----------------------------------------------------------------------

    pub async fn get_heartbeat_items(&self) -> Vec<HeartbeatItem> {
        self.heartbeat_items.read().await.clone()
    }

    pub async fn set_heartbeat_items(&self, items: Vec<HeartbeatItem>) {
        *self.heartbeat_items.write().await = items;
        self.persist_heartbeat().await;
    }

    async fn run_heartbeat(&self) -> Result<()> {
        let items = self.heartbeat_items.read().await.clone();
        let now = now_millis();

        for item in &items {
            if !item.enabled {
                continue;
            }

            let interval_ms = if item.interval_minutes > 0 {
                item.interval_minutes * 60 * 1000
            } else {
                self.config.read().await.heartbeat_interval_mins * 60 * 1000
            };

            let due = match item.last_run_at {
                Some(last) => now - last >= interval_ms,
                None => true,
            };

            if !due {
                continue;
            }

            let prompt = format!(
                "Heartbeat check: {}\n\n\
                 Respond with HEARTBEAT_OK if everything is normal, \
                 or HEARTBEAT_ALERT: <explanation> if something needs attention.",
                item.prompt
            );

            let result = match self.send_message(None, &prompt).await {
                Ok(thread_id) => {
                    // Check the last assistant message for OK/ALERT
                    let threads = self.threads.read().await;
                    let response = threads
                        .get(&thread_id)
                        .and_then(|t| {
                            t.messages
                                .iter()
                                .rev()
                                .find(|m| m.role == MessageRole::Assistant)
                                .map(|m| m.content.clone())
                        })
                        .unwrap_or_default();

                    if response.contains("HEARTBEAT_OK") {
                        (HeartbeatOutcome::Ok, "OK".into())
                    } else if response.contains("HEARTBEAT_ALERT") {
                        (HeartbeatOutcome::Alert, response)
                    } else {
                        (HeartbeatOutcome::Ok, response)
                    }
                }
                Err(e) => (HeartbeatOutcome::Error, format!("Error: {e}")),
            };

            let _ = self.event_tx.send(AgentEvent::HeartbeatResult {
                item_id: item.id.clone(),
                result: result.0,
                message: result.1.clone(),
            });

            // Update item state
            {
                let mut items = self.heartbeat_items.write().await;
                if let Some(i) = items.iter_mut().find(|i| i.id == item.id) {
                    i.last_run_at = Some(now);
                    i.last_result = Some(result.0);
                    i.last_message = Some(result.1);
                }
            }

            // If alert and notify enabled, send notification
            if result.0 == HeartbeatOutcome::Alert && item.notify_on_alert {
                let _ = self.event_tx.send(AgentEvent::Notification {
                    title: format!("Heartbeat Alert: {}", item.label),
                    body: item.last_message.clone().unwrap_or_default(),
                    severity: NotificationSeverity::Alert,
                    channels: item.notify_channels.clone(),
                });
            }
        }

        self.persist_heartbeat().await;
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Config management
    // -----------------------------------------------------------------------

    pub async fn get_config(&self) -> AgentConfig {
        self.config.read().await.clone()
    }

    pub async fn set_config(&self, config: AgentConfig) {
        *self.config.write().await = config;
        self.persist_config().await;
    }

    // -----------------------------------------------------------------------
    // Thread management
    // -----------------------------------------------------------------------

    pub async fn list_threads(&self) -> Vec<AgentThread> {
        let threads = self.threads.read().await;
        let mut list: Vec<AgentThread> = threads.values().cloned().collect();
        list.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        list
    }

    pub async fn get_thread(&self, thread_id: &str) -> Option<AgentThread> {
        self.threads.read().await.get(thread_id).cloned()
    }

    pub async fn planner_required_for_thread(&self, thread_id: &str) -> bool {
        let threads = self.threads.read().await;
        let Some(thread) = threads.get(thread_id) else {
            return false;
        };
        let latest_user_message = thread
            .messages
            .iter()
            .rev()
            .find(|message| message.role == MessageRole::User)
            .map(|message| message.content.as_str())
            .unwrap_or("");
        planner_required_for_message(latest_user_message)
    }

    pub async fn delete_thread(&self, thread_id: &str) -> bool {
        let removed = self.threads.write().await.remove(thread_id).is_some();
        if removed {
            self.persist_threads().await;
        }
        removed
    }

    // -----------------------------------------------------------------------
    // External agent backends (openclaw / hermes)
    // -----------------------------------------------------------------------

    /// Route a message through an external agent process.
    async fn send_message_external(
        &self,
        config: &AgentConfig,
        thread_id: Option<&str>,
        content: &str,
    ) -> Result<String> {
        let (tid, is_new_thread) = self.get_or_create_thread(thread_id, content).await;

        // Add user message
        {
            let mut threads = self.threads.write().await;
            if let Some(thread) = threads.get_mut(&tid) {
                thread.messages.push(AgentMessage {
                    role: MessageRole::User,
                    content: content.into(),
                    tool_calls: None,
                    tool_call_id: None,
                    tool_name: None,
                    tool_arguments: None,
                    tool_status: None,
                    input_tokens: 0,
                    output_tokens: 0,
                    reasoning: None,
                    timestamp: now_millis(),
                });
                thread.updated_at = now_millis();
            }
        }

        let onecontext_bootstrap = if is_new_thread {
            self.onecontext_bootstrap_for_new_thread(content).await
        } else {
            None
        };

        let (stream_generation, stream_cancel_token) = self.begin_stream_cancellation(&tid).await;

        // Ensure tamux-mcp is configured in the external agent's MCP settings
        {
            let runners = self.external_runners.read().await;
            if let Some(runner) = runners.get(&config.agent_backend) {
                if !runner.has_tamux_mcp() {
                    external_runner::ensure_tamux_mcp_configured(&config.agent_backend);
                }
            }
        }

        // Only inject tamux context on the first message in a thread
        // (subsequent messages in the same thread don't need the preamble
        // repeated — the external agent session carries the context)
        let is_first_message = {
            let threads = self.threads.read().await;
            threads
                .get(&tid)
                .map(|t| t.messages.len() <= 1) // 1 = just the user message we added above
                .unwrap_or(true)
        };

        let mut enriched_prompt = if is_first_message {
            let memory = self.memory.read().await;
            let memory_dir = active_memory_dir(&self.data_dir);
            build_external_agent_prompt(config, &memory, content, &memory_dir)
        } else {
            content.to_string()
        };
        self.emit_workflow_notice(
            &tid,
            "memory-consulted",
            "Loaded persistent memory, user profile, and local skill paths for this turn.",
            Some(format!(
                "memory_dir={}; skills_dir={}",
                active_memory_dir(&self.data_dir).display(),
                skills_dir(&self.data_dir).display()
            )),
        );
        if let Some(recall) = onecontext_bootstrap {
            enriched_prompt.push_str("\n\n[ONECONTEXT RECALL]\n");
            enriched_prompt.push_str(&recall);
        }

        // Run through external agent
        let runners = self.external_runners.read().await;
        let runner = match runners.get(&config.agent_backend) {
            Some(runner) => runner,
            None => {
                self.finish_stream_cancellation(&tid, stream_generation)
                    .await;
                anyhow::bail!(
                    "No external agent runner for backend '{}'",
                    config.agent_backend
                );
            }
        };

        let response = match runner
            .send_message(&tid, &enriched_prompt, Some(stream_cancel_token))
            .await
        {
            Ok(response) => Some(response),
            Err(e) if external_runner::is_stream_cancelled(&e) => None,
            Err(e) => {
                self.finish_stream_cancellation(&tid, stream_generation)
                    .await;
                return Err(e);
            }
        };

        // Store assistant response in thread
        if let Some(response) = response {
            self.add_assistant_message(&tid, &response, 0, 0, None)
                .await;
        }
        self.persist_threads().await;
        self.finish_stream_cancellation(&tid, stream_generation)
            .await;

        Ok(tid)
    }

    /// Get the availability status of an external agent.
    pub async fn external_agent_status(
        &self,
        agent_type: &str,
    ) -> Option<external_runner::ExternalAgentStatus> {
        let runners = self.external_runners.read().await;
        runners.get(agent_type).map(|r| r.status())
    }

    /// Start gateway mode for an external agent.
    pub async fn start_external_gateway(&self) -> Result<()> {
        let config = self.config.read().await.clone();
        let runners = self.external_runners.read().await;
        if let Some(runner) = runners.get(&config.agent_backend) {
            runner.start_gateway().await
        } else {
            Ok(())
        }
    }

    /// Stop any running external agent processes.
    pub async fn stop_external_agents(&self) {
        let runners = self.external_runners.read().await;
        for runner in runners.values() {
            runner.stop().await;
        }
    }

    async fn request_goal_plan(&self, goal_run: &GoalRun) -> Result<GoalPlanResponse> {
        let prompt = format!(
            "You are planning a durable autonomous goal runner inside tamux.\n\
             Produce strict JSON only with the shape:\n\
             {{\"title\":\"...\",\"summary\":\"...\",\"steps\":[{{\"title\":\"...\",\"instructions\":\"...\",\"kind\":\"reason|command|research|memory|skill\",\"success_criteria\":\"...\",\"session_id\":null}}]}}\n\
             Requirements:\n\
             - 2 to 6 steps.\n\
             - Keep each step actionable and narrow.\n\
             - Use kind=command only when the step should execute via the daemon task queue.\n\
             - Use skill only only if a reusable workflow artifact should be generated at the end.\n\
             - Prefer one terminal session unless the goal clearly requires otherwise.\n\
             Goal title: {}\n\
             Goal:\n{}",
            goal_run.title, goal_run.goal
        );
        let raw = self.run_goal_llm_json(&prompt).await?;
        validate_goal_plan_response(parse_goal_llm_json(&raw)?, false)
    }

    async fn request_goal_replan(
        &self,
        goal_run: &GoalRun,
        failure: &str,
    ) -> Result<GoalPlanResponse> {
        let completed = goal_run
            .steps
            .iter()
            .take(goal_run.current_step_index.saturating_add(1))
            .map(|step| {
                format!(
                    "- {} [{}]",
                    step.title,
                    goal_run_step_status_label(step.status)
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        let prompt = format!(
            "You are replanning a tamux goal runner after a failed step.\n\
             Produce strict JSON only with the shape:\n\
             {{\"title\":\"...\",\"summary\":\"...\",\"steps\":[{{\"title\":\"...\",\"instructions\":\"...\",\"kind\":\"reason|command|research|memory|skill\",\"success_criteria\":\"...\",\"session_id\":null}}]}}\n\
             Return only the revised remaining steps, not the full history.\n\
             Goal: {}\n\
             Failure: {}\n\
             Completed / attempted steps:\n{}\n",
            goal_run.goal,
            failure,
            if completed.is_empty() { "- none".into() } else { completed }
        );
        let raw = self.run_goal_llm_json(&prompt).await?;
        validate_goal_plan_response(parse_goal_llm_json(&raw)?, true)
    }

    async fn request_goal_reflection(&self, goal_run: &GoalRun) -> Result<GoalReflectionResponse> {
        let step_summaries = goal_run
            .steps
            .iter()
            .map(|step| {
                format!(
                    "- {} [{}]: {}",
                    step.title,
                    goal_run_step_status_label(step.status),
                    step.summary
                        .as_deref()
                        .or(step.error.as_deref())
                        .unwrap_or("no summary")
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        let prompt = format!(
            "You are reflecting on a completed tamux goal runner.\n\
             Produce strict JSON only with the shape:\n\
             {{\"summary\":\"...\",\"stable_memory_update\":null,\"generate_skill\":false,\"skill_title\":null}}\n\
             `stable_memory_update` must be null unless you learned a durable operator preference or stable workspace fact worth appending to MEMORY.md.\n\
             Goal: {}\n\
             Step outcomes:\n{}\n",
            goal_run.goal,
            if step_summaries.is_empty() {
                "- no steps recorded".into()
            } else {
                step_summaries
            }
        );
        let raw = self.run_goal_llm_json(&prompt).await?;
        parse_goal_reflection_json(&raw)
    }

    async fn run_goal_llm_json(&self, prompt: &str) -> Result<String> {
        let config = self.config.read().await.clone();
        if config.agent_backend != "daemon" {
            anyhow::bail!("goal runs currently require the built-in daemon agent backend");
        }
        let provider_config = self.resolve_provider_config(&config)?;
        let messages = vec![ApiMessage {
            role: "user".into(),
            content: ApiContent::Text(prompt.to_string()),
            tool_call_id: None,
            name: None,
            tool_calls: None,
        }];
        let mut stream = send_chat_completion(
            &self.http_client,
            &config.provider,
            &provider_config,
            "Return strict JSON only. Do not call tools. Do not wrap the answer in markdown.",
            &messages,
            &[],
            config.max_retries,
            config.retry_delay_ms,
        );
        let mut content = String::new();
        while let Some(chunk) = stream.next().await {
            match chunk? {
                CompletionChunk::Delta { content: delta, .. } => content.push_str(&delta),
                CompletionChunk::Done { content: done, .. } => {
                    if done.is_empty() {
                        return Ok(content);
                    }
                    return Ok(done);
                }
                CompletionChunk::Error { message } => anyhow::bail!(message),
                CompletionChunk::Retry { .. } => {}
                CompletionChunk::ToolCalls { .. } => {
                    anyhow::bail!("goal planning unexpectedly returned tool calls")
                }
            }
        }
        if content.trim().is_empty() {
            anyhow::bail!("goal planning returned empty output");
        }
        Ok(content)
    }

    async fn append_goal_memory_update(&self, update: &str) -> Result<()> {
        let trimmed = update.trim();
        if trimmed.is_empty() {
            return Ok(());
        }
        let memory_dir = active_memory_dir(&self.data_dir);
        let memory_path = memory_dir.join("MEMORY.md");
        tokio::fs::create_dir_all(&memory_dir).await?;
        let existing = tokio::fs::read_to_string(&memory_path)
            .await
            .unwrap_or_default();
        if existing.contains(trimmed) {
            return Ok(());
        }

        let mut next = existing.trim_end().to_string();
        if !next.is_empty() {
            next.push_str("\n\n");
        }
        next.push_str("## Learned During Goal Runs\n");
        next.push_str("- ");
        next.push_str(trimmed);
        next.push('\n');
        tokio::fs::write(&memory_path, next).await?;
        self.refresh_memory_cache().await;
        Ok(())
    }

    async fn goal_thread_summary(&self, thread_id: &str) -> Option<String> {
        let threads = self.threads.read().await;
        threads.get(thread_id).and_then(|thread| {
            thread
                .messages
                .iter()
                .rev()
                .find(|message| {
                    message.role == MessageRole::Assistant && !message.content.trim().is_empty()
                })
                .map(|message| summarize_text(&message.content, 320))
        })
    }

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    fn resolve_provider_config(&self, config: &AgentConfig) -> Result<ProviderConfig> {
        // Check named providers first
        if let Some(pc) = config.providers.get(&config.provider) {
            return Ok(pc.clone());
        }

        // Fall back to top-level config
        if config.base_url.is_empty() {
            anyhow::bail!(
                "No base URL configured for provider '{}'. Configure in agent settings.",
                config.provider
            );
        }

        Ok(ProviderConfig {
            base_url: config.base_url.clone(),
            model: config.model.clone(),
            api_key: config.api_key.clone(),
        })
    }

    async fn add_assistant_message(
        &self,
        thread_id: &str,
        content: &str,
        input_tokens: u64,
        output_tokens: u64,
        reasoning: Option<String>,
    ) {
        let mut threads = self.threads.write().await;
        if let Some(thread) = threads.get_mut(thread_id) {
            thread.messages.push(AgentMessage {
                role: MessageRole::Assistant,
                content: content.into(),
                tool_calls: None,
                tool_call_id: None,
                tool_name: None,
                tool_arguments: None,
                tool_status: None,
                input_tokens,
                output_tokens,
                reasoning,
                timestamp: now_millis(),
            });
            thread.total_input_tokens += input_tokens;
            thread.total_output_tokens += output_tokens;
            thread.updated_at = now_millis();
        }
    }

    async fn persist_threads(&self) {
        let threads = self.threads.read().await;
        for thread in threads.values() {
            let thread_row = amux_protocol::AgentDbThread {
                id: thread.id.clone(),
                workspace_id: None,
                surface_id: None,
                pane_id: None,
                agent_name: Some("assistant".to_string()),
                title: thread.title.clone(),
                created_at: thread.created_at as i64,
                updated_at: thread.updated_at as i64,
                message_count: thread.messages.len() as i64,
                total_tokens: (thread.total_input_tokens + thread.total_output_tokens) as i64,
                last_preview: thread
                    .messages
                    .last()
                    .map(|message| message.content.chars().take(100).collect())
                    .unwrap_or_default(),
            };

            if let Err(e) = self.history.delete_thread(&thread.id) {
                tracing::warn!(thread_id = %thread.id, "failed to reset sqlite thread state: {e}");
                continue;
            }
            if let Err(e) = self.history.create_thread(&thread_row) {
                tracing::warn!(thread_id = %thread.id, "failed to persist sqlite thread row: {e}");
                continue;
            }

            for (index, message) in thread.messages.iter().enumerate() {
                let metadata_json = serde_json::to_string(&serde_json::json!({
                    "tool_call_id": message.tool_call_id,
                    "tool_name": message.tool_name,
                    "toolName": message.tool_name,
                    "toolCallId": message.tool_call_id,
                    "toolArguments": message.tool_arguments,
                    "toolStatus": message.tool_status,
                }))
                .ok();
                let row = amux_protocol::AgentDbMessage {
                    id: format!("{}:{}", thread.id, index),
                    thread_id: thread.id.clone(),
                    created_at: message.timestamp as i64,
                    role: match message.role {
                        MessageRole::System => "system",
                        MessageRole::User => "user",
                        MessageRole::Assistant => "assistant",
                        MessageRole::Tool => "tool",
                    }
                    .to_string(),
                    content: message.content.clone(),
                    provider: None,
                    model: None,
                    input_tokens: Some(message.input_tokens as i64),
                    output_tokens: Some(message.output_tokens as i64),
                    total_tokens: Some((message.input_tokens + message.output_tokens) as i64),
                    reasoning: message.reasoning.clone(),
                    tool_calls_json: message
                        .tool_calls
                        .as_ref()
                        .and_then(|calls| serde_json::to_string(calls).ok()),
                    metadata_json,
                };
                if let Err(e) = self.history.add_message(&row) {
                    tracing::warn!(thread_id = %thread.id, message_index = index, "failed to persist sqlite message row: {e}");
                }
            }
        }
    }

    async fn persist_todos(&self) {
        let todos = self.thread_todos.read().await;
        if let Err(e) = persist_json(&self.data_dir.join("todos.json"), &*todos).await {
            tracing::warn!("failed to persist todos: {e}");
        }
    }

    async fn persist_tasks(&self) {
        let tasks = self.tasks.lock().await;
        for task in tasks.iter() {
            if let Err(e) = self.history.upsert_agent_task(task) {
                tracing::warn!(task_id = %task.id, "failed to persist task to sqlite: {e}");
            }
        }
        if let Err(e) = persist_json(&self.data_dir.join("tasks.json"), &*tasks).await {
            tracing::warn!("failed to persist tasks: {e}");
        }
    }

    async fn persist_goal_runs(&self) {
        let goal_runs = self.goal_runs.lock().await;
        for goal_run in goal_runs.iter() {
            if let Err(e) = self.history.upsert_goal_run(goal_run) {
                tracing::warn!(goal_run_id = %goal_run.id, "failed to persist goal run to sqlite: {e}");
            }
        }
        if let Err(e) = persist_json(&self.data_dir.join("goal-runs.json"), &*goal_runs).await {
            tracing::warn!("failed to persist goal runs: {e}");
        }
    }

    async fn persist_heartbeat(&self) {
        let items = self.heartbeat_items.read().await;
        if let Err(e) = persist_json(&self.data_dir.join("heartbeat.json"), &*items).await {
            tracing::warn!("failed to persist heartbeat: {e}");
        }
    }

    async fn persist_config(&self) {
        let config = self.config.read().await;
        if let Err(e) = persist_json(&self.data_dir.join("config.json"), &*config).await {
            tracing::warn!("failed to persist config: {e}");
        }
    }
}

fn make_task_log_entry(
    attempt: u32,
    level: TaskLogLevel,
    phase: &str,
    message: &str,
    details: Option<String>,
) -> AgentTaskLogEntry {
    AgentTaskLogEntry {
        id: format!("tasklog_{}", Uuid::new_v4()),
        timestamp: now_millis(),
        level,
        phase: phase.to_string(),
        message: message.to_string(),
        details,
        attempt,
    }
}

fn refresh_task_queue_state(
    tasks: &mut VecDeque<AgentTask>,
    now: u64,
    sessions: &[amux_protocol::SessionInfo],
) -> Vec<AgentTask> {
    let completed: HashSet<String> = tasks
        .iter()
        .filter(|task| task.status == TaskStatus::Completed)
        .map(|task| task.id.clone())
        .collect();
    let occupied_lanes = tasks
        .iter()
        .filter(|task| {
            matches!(
                task.status,
                TaskStatus::InProgress | TaskStatus::AwaitingApproval
            )
        })
        .map(current_task_lane_key)
        .collect::<HashSet<_>>();
    let occupied_workspaces = tasks
        .iter()
        .filter(|task| {
            matches!(
                task.status,
                TaskStatus::InProgress | TaskStatus::AwaitingApproval
            )
        })
        .filter_map(|task| task_workspace_key(task, sessions))
        .collect::<HashSet<_>>();
    let mut changed = Vec::new();

    for task in tasks.iter_mut() {
        let unresolved = task
            .dependencies
            .iter()
            .filter(|dependency| !completed.contains(*dependency))
            .cloned()
            .collect::<Vec<_>>();

        if matches!(task.status, TaskStatus::Queued | TaskStatus::Blocked) {
            if !unresolved.is_empty() {
                let reason = format!("waiting for dependencies: {}", unresolved.join(", "));
                if task.status != TaskStatus::Blocked
                    || task.blocked_reason.as_deref() != Some(reason.as_str())
                {
                    task.status = TaskStatus::Blocked;
                    task.blocked_reason = Some(reason.clone());
                    task.logs.push(make_task_log_entry(
                        task.retry_count,
                        TaskLogLevel::Info,
                        "queue",
                        "task blocked on dependencies",
                        Some(reason),
                    ));
                    changed.push(task.clone());
                }
                continue;
            }

            if let Some(scheduled_at) = task.scheduled_at.filter(|deadline| *deadline > now) {
                let reason = describe_scheduled_time(scheduled_at);
                if task.status != TaskStatus::Blocked
                    || task.blocked_reason.as_deref() != Some(reason.as_str())
                {
                    task.status = TaskStatus::Blocked;
                    task.blocked_reason = Some(reason.clone());
                    task.logs.push(make_task_log_entry(
                        task.retry_count,
                        TaskLogLevel::Info,
                        "schedule",
                        "task waiting for scheduled time",
                        Some(reason),
                    ));
                    changed.push(task.clone());
                }
                continue;
            }

            let resource_reason = if occupied_lanes.contains(&task_lane_key(task)) {
                Some(format!(
                    "waiting for lane availability: {}",
                    task_lane_key(task)
                ))
            } else if let Some(workspace_key) = task_workspace_key(task, sessions) {
                if occupied_workspaces.contains(&workspace_key) {
                    Some(format!(
                        "waiting for workspace lock: {}",
                        workspace_key.replace("workspace:", "")
                    ))
                } else {
                    None
                }
            } else {
                None
            };

            if let Some(reason) = resource_reason {
                if task.status != TaskStatus::Blocked
                    || task.blocked_reason.as_deref() != Some(reason.as_str())
                {
                    task.status = TaskStatus::Blocked;
                    task.blocked_reason = Some(reason.clone());
                    task.logs.push(make_task_log_entry(
                        task.retry_count,
                        TaskLogLevel::Info,
                        "queue",
                        "task blocked on lane or workspace availability",
                        Some(reason),
                    ));
                    changed.push(task.clone());
                }
                continue;
            }

            if task.status == TaskStatus::Blocked {
                task.status = TaskStatus::Queued;
                task.blocked_reason = None;
                task.logs.push(make_task_log_entry(
                    task.retry_count,
                    TaskLogLevel::Info,
                    "queue",
                    "task gate cleared; task returned to queue",
                    None,
                ));
                changed.push(task.clone());
            }
        }

        if task.status == TaskStatus::FailedAnalyzing
            && task
                .next_retry_at
                .map(|deadline| deadline <= now)
                .unwrap_or(true)
        {
            task.status = TaskStatus::Queued;
            task.next_retry_at = None;
            task.blocked_reason = None;
            task.logs.push(make_task_log_entry(
                task.retry_count,
                TaskLogLevel::Info,
                "analysis",
                "retry backoff elapsed; task returned to queue",
                None,
            ));
            changed.push(task.clone());
        }
    }

    changed
}

fn select_ready_task_indices(
    tasks: &VecDeque<AgentTask>,
    sessions: &[amux_protocol::SessionInfo],
) -> Vec<usize> {
    let mut occupied_lanes = tasks
        .iter()
        .filter(|task| {
            matches!(
                task.status,
                TaskStatus::InProgress | TaskStatus::AwaitingApproval
            )
        })
        .map(current_task_lane_key)
        .collect::<HashSet<_>>();
    let mut occupied_workspaces = tasks
        .iter()
        .filter(|task| {
            matches!(
                task.status,
                TaskStatus::InProgress | TaskStatus::AwaitingApproval
            )
        })
        .filter_map(|task| task_workspace_key(task, sessions))
        .collect::<HashSet<_>>();

    let mut queued = tasks
        .iter()
        .enumerate()
        .filter(|(_, task)| task.status == TaskStatus::Queued)
        .collect::<Vec<_>>();
    queued.sort_by_key(|(_, task)| (task_priority_rank(task.priority), task.created_at));

    let mut selected = Vec::new();
    for (index, task) in queued {
        let lane = task_lane_key(task);
        let workspace = task_workspace_key(task, sessions);
        let lane_available = occupied_lanes.insert(lane);
        let workspace_available = workspace
            .as_ref()
            .map(|key| occupied_workspaces.insert(key.clone()))
            .unwrap_or(true);
        if lane_available && workspace_available {
            selected.push(index);
            continue;
        }

        if lane_available {
            occupied_lanes.remove(current_task_lane_key(task).as_str());
        }
    }

    selected
}

fn task_lane_key(task: &AgentTask) -> String {
    task.session_id
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .map(|value| format!("session:{value}"))
        .unwrap_or_else(|| "daemon-main".to_string())
}

fn current_task_lane_key(task: &AgentTask) -> String {
    task.lane_id.clone().unwrap_or_else(|| task_lane_key(task))
}

fn task_workspace_key(task: &AgentTask, sessions: &[amux_protocol::SessionInfo]) -> Option<String> {
    let session_hint = task.session_id.as_deref()?.trim();
    if session_hint.is_empty() {
        return None;
    }

    sessions
        .iter()
        .find(|session| {
            let session_id = session.id.to_string();
            session_id == session_hint || session_id.contains(session_hint)
        })
        .and_then(|session| session.workspace_id.as_ref())
        .map(|workspace_id| format!("workspace:{workspace_id}"))
}

fn task_priority_rank(priority: TaskPriority) -> u8 {
    match priority {
        TaskPriority::Urgent => 0,
        TaskPriority::High => 1,
        TaskPriority::Normal => 2,
        TaskPriority::Low => 3,
    }
}

fn compute_task_backoff_ms(base_delay_ms: u64, retry_count: u32) -> u64 {
    let multiplier = 2u64.saturating_pow(retry_count.saturating_sub(1));
    base_delay_ms.saturating_mul(multiplier).min(5 * 60 * 1000)
}

fn build_task_prompt(task: &AgentTask) -> String {
    let mut prompt = format!(
        "Execute the following queued task.\n\nTitle: {}\nDescription: {}",
        task.title, task.description
    );

    prompt.push_str(
        "\nUse execute_managed_command when work should run inside a daemon-managed terminal lane, needs a real PTY, or may require operator approval.",
    );
    prompt.push_str(
        "\nIf the task is more than a one-shot action, call update_todo immediately with a concise plan and keep it current as steps advance.",
    );

    if let Some(command) = task.command.as_deref() {
        prompt.push_str(&format!("\nPreferred command or entrypoint: {command}"));
    }

    if let Some(session_id) = task.session_id.as_deref() {
        prompt.push_str(&format!("\nPreferred terminal session: {session_id}"));
    }

    if let Some(goal_run_id) = task.goal_run_id.as_deref() {
        prompt.push_str(&format!("\nGoal run context: {goal_run_id}"));
    }

    if let Some(scheduled_at) = task.scheduled_at {
        prompt.push_str(&format!(
            "\nOriginal schedule: {}",
            describe_scheduled_time(scheduled_at)
        ));
    }

    if !task.dependencies.is_empty() {
        prompt.push_str(&format!(
            "\nResolved dependencies: {}",
            task.dependencies.join(", ")
        ));
    }

    if task.retry_count > 0 {
        prompt.push_str(&format!(
            "\n\nThis is self-healing retry attempt {} of {}.",
            task.retry_count, task.max_retries
        ));
        if let Some(last_error) = task.last_error.as_deref() {
            prompt.push_str(&format!("\nLast failure: {last_error}"));
        }
        let recent_logs = task
            .logs
            .iter()
            .rev()
            .take(4)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .map(|log| format!("- [{}] {}", log.phase, log.message))
            .collect::<Vec<_>>();
        if !recent_logs.is_empty() {
            prompt.push_str("\nRecent task log:\n");
            prompt.push_str(&recent_logs.join("\n"));
        }
        prompt.push_str(
            "\nAnalyze the root cause, adapt the approach, and retry with the smallest viable correction.",
        );
    } else {
        prompt.push_str(
            "\n\nWork through this step by step. Use your tools as needed and report your progress clearly.",
        );
    }

    prompt
}

fn describe_scheduled_time(timestamp_ms: u64) -> String {
    let system_time = UNIX_EPOCH + Duration::from_millis(timestamp_ms);
    format!(
        "scheduled for {}",
        humantime::format_rfc3339_seconds(system_time)
    )
}

fn status_message(task: &AgentTask) -> &'static str {
    match task.status {
        TaskStatus::Queued => "Task queued",
        TaskStatus::InProgress => "Task in progress",
        TaskStatus::AwaitingApproval => "Task awaiting approval",
        TaskStatus::Blocked => "Task blocked",
        TaskStatus::FailedAnalyzing => "Task analyzing failure",
        TaskStatus::Completed => "Task completed",
        TaskStatus::Failed => "Task failed",
        TaskStatus::Cancelled => "Task cancelled",
    }
}

async fn resolve_preferred_session_id(
    session_manager: &Arc<SessionManager>,
    session_hint: Option<&str>,
) -> Option<amux_protocol::SessionId> {
    let hint = session_hint?.trim();
    if hint.is_empty() {
        return None;
    }

    session_manager
        .list()
        .await
        .into_iter()
        .find(|session| {
            let session_id = session.id.to_string();
            session_id == hint || session_id.contains(hint)
        })
        .map(|session| session.id)
}

// ---------------------------------------------------------------------------
// Utilities
// ---------------------------------------------------------------------------

fn agent_data_dir() -> PathBuf {
    let base = if cfg!(windows) {
        std::env::var("LOCALAPPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|_| {
                dirs::home_dir()
                    .unwrap_or_default()
                    .join("AppData")
                    .join("Local")
            })
            .join("tamux")
    } else {
        dirs::home_dir().unwrap_or_default().join(".tamux")
    };
    base.join("agent")
}

fn ordered_memory_dirs(agent_data_dir: &std::path::Path) -> Vec<PathBuf> {
    let root = agent_data_dir.parent().unwrap_or(std::path::Path::new("."));
    let mut dirs = vec![root.join("agent-mission"), agent_data_dir.to_path_buf()];
    dirs.dedup();
    dirs
}

pub(super) fn skills_dir(agent_data_dir: &std::path::Path) -> PathBuf {
    agent_data_dir
        .parent()
        .unwrap_or(std::path::Path::new("."))
        .join("skills")
}

/// Seed built-in skill documents into `~/.tamux/skills/builtin/`.
///
/// Files are written on every startup so that updates to the binary ship new
/// skill versions automatically.  The `builtin/` subdirectory keeps them
/// separate from user-authored and generated skills.
fn seed_builtin_skills(agent_data_dir: &std::path::Path) {
    static BUILTIN_SKILLS: &[(&str, &str)] = &[
        ("builtin/README.md",                       include_str!("../../../../docs/skills/README.md")),
        ("builtin/cheatsheet.md",                   include_str!("../../../../docs/skills/cheatsheet.md")),
        ("builtin/connection/setup.md",             include_str!("../../../../docs/skills/connection/setup.md")),
        ("builtin/operating/terminals.md",          include_str!("../../../../docs/skills/operating/terminals.md")),
        ("builtin/operating/browser.md",            include_str!("../../../../docs/skills/operating/browser.md")),
        ("builtin/operating/tasks.md",              include_str!("../../../../docs/skills/operating/tasks.md")),
        ("builtin/operating/goals.md",              include_str!("../../../../docs/skills/operating/goals.md")),
        ("builtin/operating/memory.md",             include_str!("../../../../docs/skills/operating/memory.md")),
        ("builtin/operating/workspaces.md",         include_str!("../../../../docs/skills/operating/workspaces.md")),
        ("builtin/operating/safety.md",             include_str!("../../../../docs/skills/operating/safety.md")),
        ("builtin/operating/messaging.md",          include_str!("../../../../docs/skills/operating/messaging.md")),
        ("builtin/operating/observability.md",      include_str!("../../../../docs/skills/operating/observability.md")),
        ("builtin/building/plugin-development.md",  include_str!("../../../../docs/skills/building/plugin-development.md")),
    ];

    let root = skills_dir(agent_data_dir);
    for (relative_path, content) in BUILTIN_SKILLS {
        let target = root.join(relative_path);
        if let Some(parent) = target.parent() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                tracing::warn!("failed to create skill dir {}: {e}", parent.display());
                continue;
            }
        }
        if let Err(e) = std::fs::write(&target, content) {
            tracing::warn!("failed to seed skill {}: {e}", target.display());
        }
    }
    tracing::debug!("seeded {} built-in skills into {}", BUILTIN_SKILLS.len(), root.join("builtin").display());
}

fn dir_has_memory_files(dir: &std::path::Path) -> bool {
    ["MEMORY.md", "SOUL.md", "USER.md"]
        .iter()
        .any(|name| dir.join(name).exists())
}

pub(super) fn active_memory_dir(agent_data_dir: &std::path::Path) -> PathBuf {
    let dirs = ordered_memory_dirs(agent_data_dir);
    if let Some(path) = dirs.iter().find(|dir| dir_has_memory_files(dir)) {
        return path.clone();
    }
    if let Some(path) = dirs.iter().find(|dir| dir.exists()) {
        return path.clone();
    }
    // Default to the frontend mission directory for new installs.
    dirs.first()
        .cloned()
        .unwrap_or_else(|| agent_data_dir.to_path_buf())
}

/// Read a setting from a settings.json `Value` using the nested `/settings/<key>`
/// path or a top-level `<key>` fallback.
fn read_setting_str(v: &serde_json::Value, key: &str) -> String {
    let pointer = format!("/settings/{key}");
    v.pointer(&pointer)
        .or_else(|| v.get(key))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string()
}

/// Parse a comma-separated channel filter string into a list.
fn parse_channel_filter(filter: &str) -> Vec<String> {
    filter
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

/// Build an enriched prompt for external agents (hermes/openclaw) that includes
/// tamux context: system identity, available tools, gateway config, and memory.
fn build_external_agent_prompt(
    config: &AgentConfig,
    memory: &AgentMemory,
    user_message: &str,
    memory_dir: &std::path::Path,
) -> String {
    let mut context_parts = Vec::new();
    let memory_root = memory_dir;
    let skills_root = skills_dir(&agent_data_dir());
    let generated_skills_root = skills_root.join("generated");

    // Environment context — do NOT override the agent's own identity
    context_parts.push(
        "[ENVIRONMENT: tamux]\n\
         You are being invoked through tamux — an agentic terminal multiplexer app.\n\
         Keep your own identity and personality. Do NOT call yourself tamux.\n\
         \n\
         About tamux:\n\
         - tamux is a desktop app with workspaces, surfaces (tab groups), and panes (terminals)\n\
         - The user sees your response in tamux's agent chat panel (a sidebar)\n\
         - The user has terminal panes open next to this chat\n\
         - tamux's daemon manages your process lifecycle and relays your responses to the UI\n\
         \n\
         tamux tools via MCP:\n\
         tamux-mcp has been configured in your MCP servers. You should have access to \
         these tools — use them when the user asks about terminals, sessions, or history:\n\
         - list_sessions: list active terminal sessions (IDs, CWD, dimensions)\n\
         - get_terminal_content: read what's displayed in a terminal pane (scrollback)\n\
         - type_in_terminal: send keystrokes/input to a terminal session\n\
         - execute_command: run managed commands inside tamux terminal sessions\n\
         - search_history: full-text search of command/transcript history\n\
         - find_symbol: semantic code symbol search using tree-sitter\n\
         - get_git_status: git status for a working directory\n\
         - list_snapshots / restore_snapshot: workspace checkpoint management\n\
         - scrub_sensitive: redact secrets from text\n"
            .to_string(),
    );

    // Operator's instructions for this session
    if !config.system_prompt.is_empty() {
        context_parts.push(format!("Operator instructions: {}\n", config.system_prompt));
    }

    // Gateway info — the agent can use its own gateway tools if it has them
    let gw = &config.gateway;
    if gw.enabled {
        let mut platforms = Vec::new();
        if !gw.slack_token.is_empty() {
            platforms.push("Slack");
        }
        if !gw.discord_token.is_empty() {
            platforms.push("Discord");
        }
        if !gw.telegram_token.is_empty() {
            platforms.push("Telegram");
        }
        if !platforms.is_empty() {
            context_parts.push(format!(
                "Connected chat platforms: {}. Use your own messaging tools to reach them.\n",
                platforms.join(", ")
            ));
        }
    }

    // Memory context from tamux's persistent files
    if !memory.soul.is_empty() {
        context_parts.push(format!("Operator identity notes:\n{}\n", memory.soul));
    }
    if !memory.memory.is_empty() {
        context_parts.push(format!("Session memory:\n{}\n", memory.memory));
    }
    if !memory.user_profile.is_empty() {
        context_parts.push(format!("Operator profile:\n{}\n", memory.user_profile));
    }

    context_parts.push(format!(
        "tamux persistent memory files on this machine:\n- MEMORY.md: {}\n- SOUL.md: {}\n- USER.md: {}\n",
        memory_root.join("MEMORY.md").display(),
        memory_root.join("SOUL.md").display(),
        memory_root.join("USER.md").display(),
    ));
    context_parts.push(format!(
        "tamux local skills on this machine:\n- Skills root: {}\n- Generated skills: {}\n\
         Before non-trivial work, review relevant skills in that directory and reuse them when possible.\n",
        skills_root.display(),
        generated_skills_root.display(),
    ));

    if context_parts.is_empty() {
        return user_message.to_string();
    }

    format!(
        "{}\n[USER MESSAGE]\n{}",
        context_parts.join(""),
        user_message
    )
}

fn build_system_prompt(base: &str, memory: &AgentMemory, data_dir: &std::path::Path) -> String {
    let mut prompt = String::new();
    let memory_path = data_dir.join("MEMORY.md");
    let soul_path = data_dir.join("SOUL.md");
    let user_path = data_dir.join("USER.md");
    let skills_root = skills_dir(data_dir);
    let generated_skills_root = skills_root.join("generated");

    if !memory.soul.is_empty() {
        prompt.push_str(&memory.soul);
        prompt.push_str("\n\n");
    }

    prompt.push_str(base);

    if !memory.memory.is_empty() {
        prompt.push_str("\n\n## Persistent Memory\n");
        prompt.push_str(&memory.memory);
    }

    if !memory.user_profile.is_empty() {
        prompt.push_str("\n\n## Operator Profile\n");
        prompt.push_str(&memory.user_profile);
    }

    prompt.push_str(
        &format!(
            "\n\n## Persistent Memory File Paths\n\
             - MEMORY.md: {}\n\
             - SOUL.md: {}\n\
             - USER.md: {}\n\
             - Use these exact paths when reading or explaining where tamux agent memory lives on this platform.\n",
            memory_path.display(),
            soul_path.display(),
            user_path.display(),
        ),
    );

    prompt.push_str(
        &format!(
            "\n\n## Local Skills\n\
             - Skills root: {}\n\
             - Generated skills: {}\n\
             - Built-in skills: {}/builtin/ (tamux reference docs for terminals, browser, tasks, goals, memory, safety, etc.)\n\
             - Before non-trivial work, consult MEMORY.md and USER.md, then call `list_skills` to inspect reusable local skills.\n\
             - If a relevant skill exists, call `read_skill` before executing commands or spawning tasks.\n\
             - The `builtin/cheatsheet` skill provides a quick reference for all available MCP tools.\n\
             - Prefer reusing an existing skill over inventing a brand-new workflow.\n",
            skills_root.display(),
            generated_skills_root.display(),
            skills_root.display(),
        ),
    );

    prompt.push_str(
        "\n\n## Recall and Memory Maintenance\n\
         - Use `onecontext_search` when the user asks about prior decisions, existing implementations, or historical debugging context.\n\
         - For any non-trivial or multi-step task, call `update_todo` early to enter plan mode, then keep that todo list current as work progresses.\n\
         - When you learn durable operator preferences or stable project facts, call `update_memory` with a concise update so future sessions start with that context.\n",
    );

    prompt.push_str(
        "\n\n## Terminal Session Discipline\n\
         - Before running file or command actions, call `list_sessions` (or `list_terminals`) to discover current session IDs and CWD.\n\
         - Pick a target session and reuse that `session` value across related tool calls so all actions stay in one terminal context.\n\
         - If the operator asks to use another terminal, call `list_sessions` again and switch explicitly.\n",
    );

    prompt
}

#[derive(Debug, Clone, serde::Deserialize)]
struct GoalPlanResponse {
    #[serde(default)]
    title: Option<String>,
    summary: String,
    steps: Vec<GoalPlanStepResponse>,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct GoalPlanStepResponse {
    title: String,
    instructions: String,
    kind: GoalRunStepKind,
    success_criteria: String,
    #[serde(default)]
    session_id: Option<String>,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct GoalReflectionResponse {
    summary: String,
    #[serde(default)]
    stable_memory_update: Option<String>,
    #[serde(default)]
    generate_skill: bool,
    #[serde(default)]
    skill_title: Option<String>,
}

fn validate_goal_plan_response(
    mut plan: GoalPlanResponse,
    allow_single_step: bool,
) -> Result<GoalPlanResponse> {
    plan.summary = plan.summary.trim().to_string();
    if plan.summary.is_empty() {
        anyhow::bail!("goal plan summary must not be empty");
    }

    let min_steps = if allow_single_step { 1 } else { 2 };
    if plan.steps.len() < min_steps || plan.steps.len() > 6 {
        anyhow::bail!(
            "goal plan must contain between {} and 6 steps, got {}",
            min_steps,
            plan.steps.len()
        );
    }

    let mut seen_skill = false;
    let last_index = plan.steps.len().saturating_sub(1);
    for (index, step) in plan.steps.iter_mut().enumerate() {
        step.title = step.title.trim().to_string();
        step.instructions = step.instructions.trim().to_string();
        step.success_criteria = step.success_criteria.trim().to_string();
        step.session_id = step
            .session_id
            .take()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());

        if step.title.is_empty() || step.instructions.is_empty() || step.success_criteria.is_empty()
        {
            anyhow::bail!("goal plan step {} is missing required fields", index + 1);
        }

        if step.kind == GoalRunStepKind::Skill {
            if index != last_index {
                anyhow::bail!("skill steps must be the final step in the plan");
            }
            seen_skill = true;
        } else if seen_skill {
            anyhow::bail!("no steps may follow a skill-generation step");
        }
    }

    Ok(plan)
}

fn parse_priority_str(value: &str) -> TaskPriority {
    match value {
        "low" => TaskPriority::Low,
        "high" => TaskPriority::High,
        "urgent" => TaskPriority::Urgent,
        _ => TaskPriority::Normal,
    }
}

fn task_priority_to_str(value: TaskPriority) -> &'static str {
    match value {
        TaskPriority::Low => "low",
        TaskPriority::Normal => "normal",
        TaskPriority::High => "high",
        TaskPriority::Urgent => "urgent",
    }
}

fn summarize_goal_title(goal: &str) -> String {
    let trimmed = goal.trim();
    if trimmed.is_empty() {
        return "Untitled Goal".into();
    }
    summarize_text(trimmed, 72)
}

fn summarize_text(value: &str, max_chars: usize) -> String {
    let normalized = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if normalized.chars().count() <= max_chars {
        return normalized;
    }
    let truncated = normalized
        .chars()
        .take(max_chars.saturating_sub(1))
        .collect::<String>();
    format!("{truncated}…")
}

fn resolve_goal_run_control_step(goal_run: &GoalRun, step_index: Option<usize>) -> usize {
    if goal_run.steps.is_empty() {
        return 0;
    }
    step_index
        .unwrap_or(
            goal_run
                .current_step_index
                .min(goal_run.steps.len().saturating_sub(1)),
        )
        .min(goal_run.steps.len().saturating_sub(1))
}

fn reset_goal_run_step(step: &mut GoalRunStep) {
    step.status = GoalRunStepStatus::Pending;
    step.task_id = None;
    step.summary = None;
    step.error = None;
    step.started_at = None;
    step.completed_at = None;
}

fn retry_goal_run_step(goal_run: &mut GoalRun, step_index: Option<usize>) -> Result<()> {
    if goal_run.steps.is_empty() {
        anyhow::bail!("goal run has no steps to retry");
    }

    let target_index = resolve_goal_run_control_step(goal_run, step_index);
    let Some(step) = goal_run.steps.get_mut(target_index) else {
        anyhow::bail!("goal run step index out of range");
    };

    reset_goal_run_step(step);
    goal_run.current_step_index = target_index;
    goal_run.completed_at = None;
    goal_run.status = GoalRunStatus::Running;
    goal_run.last_error = None;
    goal_run.failure_cause = None;
    goal_run.current_step_title = goal_run
        .steps
        .get(target_index)
        .map(|step| step.title.clone());
    goal_run.current_step_kind = goal_run.steps.get(target_index).map(|step| step.kind);
    goal_run.awaiting_approval_id = None;
    goal_run.active_task_id = None;
    Ok(())
}

fn rerun_goal_run_from_step(goal_run: &mut GoalRun, step_index: Option<usize>) -> Result<()> {
    if goal_run.steps.is_empty() {
        anyhow::bail!("goal run has no steps to rerun");
    }

    let target_index = resolve_goal_run_control_step(goal_run, step_index);
    for step in goal_run.steps.iter_mut().skip(target_index) {
        reset_goal_run_step(step);
    }
    goal_run.current_step_index = target_index;
    goal_run.completed_at = None;
    goal_run.status = GoalRunStatus::Running;
    goal_run.last_error = None;
    goal_run.failure_cause = None;
    goal_run.current_step_title = goal_run
        .steps
        .get(target_index)
        .map(|step| step.title.clone());
    goal_run.current_step_kind = goal_run.steps.get(target_index).map(|step| step.kind);
    goal_run.awaiting_approval_id = None;
    goal_run.active_task_id = None;
    goal_run.reflection_summary = None;
    goal_run.generated_skill_path = None;
    Ok(())
}

fn latest_goal_run_failure(goal_run: &GoalRun, tasks: &[AgentTask]) -> Option<String> {
    goal_run
        .last_error
        .clone()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            goal_run
                .steps
                .iter()
                .rev()
                .find_map(|step| step.error.clone().filter(|value| !value.trim().is_empty()))
        })
        .or_else(|| {
            tasks.iter().rev().find_map(|task| {
                task.last_error
                    .clone()
                    .or_else(|| task.error.clone())
                    .filter(|value| !value.trim().is_empty())
            })
        })
}

fn approval_count_for_tasks(tasks: &[AgentTask]) -> u32 {
    tasks
        .iter()
        .flat_map(|task| task.logs.iter())
        .filter(|log| {
            log.phase == "approval"
                && log
                    .message
                    .to_ascii_lowercase()
                    .contains("managed command paused for operator approval")
        })
        .count() as u32
}

fn project_goal_run_snapshot(
    mut goal_run: GoalRun,
    related_tasks: &[AgentTask],
    now: u64,
) -> GoalRun {
    goal_run.current_step_title = goal_run
        .steps
        .get(goal_run.current_step_index)
        .map(|step| step.title.clone());
    goal_run.current_step_kind = goal_run
        .steps
        .get(goal_run.current_step_index)
        .map(|step| step.kind);
    goal_run.active_task_id = goal_run
        .steps
        .get(goal_run.current_step_index)
        .and_then(|step| step.task_id.clone());
    goal_run.awaiting_approval_id = related_tasks
        .iter()
        .find_map(|task| task.awaiting_approval_id.clone());
    goal_run.child_task_count = if goal_run.child_task_ids.is_empty() {
        related_tasks.len() as u32
    } else {
        goal_run.child_task_ids.iter().collect::<HashSet<_>>().len() as u32
    };
    goal_run.approval_count = approval_count_for_tasks(related_tasks);
    goal_run.failure_cause = latest_goal_run_failure(&goal_run, related_tasks);
    goal_run.duration_ms = goal_run.started_at.map(|started_at| {
        goal_run
            .completed_at
            .unwrap_or(now)
            .saturating_sub(started_at)
    });
    goal_run
}

fn make_goal_run_event(phase: &str, message: &str, details: Option<String>) -> GoalRunEvent {
    make_goal_run_event_with_todos(phase, message, details, None, Vec::new())
}

fn make_goal_run_event_with_todos(
    phase: &str,
    message: &str,
    details: Option<String>,
    step_index: Option<usize>,
    todo_snapshot: Vec<TodoItem>,
) -> GoalRunEvent {
    GoalRunEvent {
        id: format!("goal_event_{}", Uuid::new_v4()),
        timestamp: now_millis(),
        phase: phase.to_string(),
        message: message.to_string(),
        details,
        step_index,
        todo_snapshot,
    }
}

fn goal_run_status_message(goal_run: &GoalRun) -> &'static str {
    match goal_run.status {
        GoalRunStatus::Queued => "Goal queued",
        GoalRunStatus::Planning => "Goal planning",
        GoalRunStatus::Running => "Goal running",
        GoalRunStatus::AwaitingApproval => "Goal awaiting approval",
        GoalRunStatus::Paused => "Goal paused",
        GoalRunStatus::Completed => "Goal completed",
        GoalRunStatus::Failed => "Goal failed",
        GoalRunStatus::Cancelled => "Goal cancelled",
    }
}

fn goal_run_step_status_label(value: GoalRunStepStatus) -> &'static str {
    match value {
        GoalRunStepStatus::Pending => "pending",
        GoalRunStepStatus::InProgress => "in_progress",
        GoalRunStepStatus::Completed => "completed",
        GoalRunStepStatus::Failed => "failed",
        GoalRunStepStatus::Skipped => "skipped",
    }
}

fn planner_required_for_message(content: &str) -> bool {
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return false;
    }

    let lower = trimmed.to_lowercase();
    let word_count = lower.split_whitespace().count();
    if word_count >= 24 || trimmed.len() >= 160 {
        return true;
    }

    if trimmed.lines().count() >= 3 {
        return true;
    }

    if trimmed.lines().any(|line| {
        let line = line.trim_start();
        line.starts_with("- ")
            || line.starts_with("* ")
            || (line.len() >= 2
                && line.as_bytes()[0].is_ascii_digit()
                && line.as_bytes()[1] == b'.')
    }) {
        return true;
    }

    [
        " then ",
        " also ",
        " after ",
        " before ",
        " next ",
        " first ",
        " second ",
        " third ",
        " plan ",
        " steps ",
        " todo ",
        " workflow ",
        " investigate ",
        " implement ",
        " migrate ",
        " refactor ",
        " compare ",
        " audit ",
        " analyze ",
        " long-running ",
        " autonomous ",
        " goal ",
    ]
    .iter()
    .any(|needle| lower.contains(needle))
}

fn parse_goal_llm_json(raw: &str) -> Result<GoalPlanResponse> {
    parse_json_block(raw)
}

fn parse_goal_reflection_json(raw: &str) -> Result<GoalReflectionResponse> {
    parse_json_block(raw)
}

fn parse_json_block<T: serde::de::DeserializeOwned>(raw: &str) -> Result<T> {
    let trimmed = raw.trim();
    if let Ok(parsed) = serde_json::from_str::<T>(trimmed) {
        return Ok(parsed);
    }

    let without_fence = trimmed
        .strip_prefix("```json")
        .or_else(|| trimmed.strip_prefix("```"))
        .map(str::trim)
        .and_then(|value| value.strip_suffix("```"))
        .map(str::trim)
        .unwrap_or(trimmed);

    if let Ok(parsed) = serde_json::from_str::<T>(without_fence) {
        return Ok(parsed);
    }

    let object_candidate = without_fence
        .find('{')
        .zip(without_fence.rfind('}'))
        .and_then(|(start, end)| (start < end).then_some(&without_fence[start..=end]));
    if let Some(candidate) = object_candidate {
        if let Ok(parsed) = serde_json::from_str::<T>(candidate) {
            return Ok(parsed);
        }
    }

    anyhow::bail!("failed to parse structured JSON from model output")
}

async fn persist_json<T: serde::Serialize>(path: &std::path::Path, data: &T) -> Result<()> {
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let json = serde_json::to_string_pretty(data)?;
    tokio::fs::write(path, json).await?;
    Ok(())
}

/// Load agent config from disk, returning defaults if not found.
pub fn load_config() -> Result<AgentConfig> {
    let path = agent_data_dir().join("config.json");
    if path.exists() {
        let raw = std::fs::read_to_string(&path)?;
        Ok(serde_json::from_str(&raw).unwrap_or_default())
    } else {
        Ok(AgentConfig::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_goal_run() -> GoalRun {
        GoalRun {
            id: "goal_test".to_string(),
            title: "Test goal".to_string(),
            goal: "Ship something".to_string(),
            status: GoalRunStatus::Failed,
            priority: TaskPriority::Normal,
            created_at: 10,
            updated_at: 30,
            started_at: Some(20),
            completed_at: Some(80),
            thread_id: None,
            session_id: Some("session-1".to_string()),
            current_step_index: 1,
            current_step_title: None,
            current_step_kind: None,
            replan_count: 1,
            max_replans: 2,
            plan_summary: Some("Plan".to_string()),
            reflection_summary: None,
            memory_updates: Vec::new(),
            generated_skill_path: Some("/tmp/skill.md".to_string()),
            last_error: Some("child task failed".to_string()),
            failure_cause: None,
            child_task_ids: vec!["task-a".to_string(), "task-b".to_string()],
            child_task_count: 0,
            approval_count: 0,
            awaiting_approval_id: None,
            active_task_id: None,
            duration_ms: None,
            steps: vec![
                GoalRunStep {
                    id: "step-0".to_string(),
                    position: 0,
                    title: "Inspect".to_string(),
                    instructions: "Inspect state".to_string(),
                    kind: GoalRunStepKind::Research,
                    success_criteria: "Know what failed".to_string(),
                    session_id: None,
                    status: GoalRunStepStatus::Completed,
                    task_id: Some("task-a".to_string()),
                    summary: Some("done".to_string()),
                    error: None,
                    started_at: Some(21),
                    completed_at: Some(30),
                },
                GoalRunStep {
                    id: "step-1".to_string(),
                    position: 1,
                    title: "Fix".to_string(),
                    instructions: "Fix it".to_string(),
                    kind: GoalRunStepKind::Command,
                    success_criteria: "Green".to_string(),
                    session_id: Some("session-1".to_string()),
                    status: GoalRunStepStatus::Failed,
                    task_id: Some("task-b".to_string()),
                    summary: None,
                    error: Some("step failure".to_string()),
                    started_at: Some(31),
                    completed_at: Some(50),
                },
            ],
            events: Vec::new(),
        }
    }

    fn sample_task(id: &str, goal_run_id: &str) -> AgentTask {
        AgentTask {
            id: id.to_string(),
            title: id.to_string(),
            description: id.to_string(),
            status: TaskStatus::Failed,
            priority: TaskPriority::Normal,
            progress: 0,
            created_at: 1,
            started_at: Some(2),
            completed_at: Some(3),
            error: Some("task error".to_string()),
            result: None,
            thread_id: None,
            source: "goal_run".to_string(),
            notify_on_complete: false,
            notify_channels: Vec::new(),
            dependencies: Vec::new(),
            command: None,
            session_id: Some("session-1".to_string()),
            goal_run_id: Some(goal_run_id.to_string()),
            retry_count: 0,
            max_retries: 0,
            next_retry_at: None,
            scheduled_at: None,
            blocked_reason: None,
            awaiting_approval_id: Some("apr-1".to_string()),
            lane_id: None,
            last_error: Some("task error".to_string()),
            logs: vec![AgentTaskLogEntry {
                id: format!("log-{id}"),
                timestamp: 4,
                level: TaskLogLevel::Warn,
                phase: "approval".to_string(),
                message: "managed command paused for operator approval".to_string(),
                details: None,
                attempt: 0,
            }],
        }
    }

    #[test]
    fn validate_goal_plan_rejects_non_terminal_skill_step() {
        let plan = GoalPlanResponse {
            title: Some("Goal".to_string()),
            summary: "Summary".to_string(),
            steps: vec![
                GoalPlanStepResponse {
                    title: "Generate skill".to_string(),
                    instructions: "Do it".to_string(),
                    kind: GoalRunStepKind::Skill,
                    success_criteria: "Artifact exists".to_string(),
                    session_id: None,
                },
                GoalPlanStepResponse {
                    title: "More work".to_string(),
                    instructions: "Continue".to_string(),
                    kind: GoalRunStepKind::Command,
                    success_criteria: "Done".to_string(),
                    session_id: None,
                },
            ],
        };

        assert!(validate_goal_plan_response(plan, false).is_err());
    }

    #[test]
    fn retry_goal_run_step_resets_selected_step() {
        let mut goal_run = sample_goal_run();

        retry_goal_run_step(&mut goal_run, Some(1)).expect("retry should succeed");

        assert_eq!(goal_run.current_step_index, 1);
        assert_eq!(goal_run.status, GoalRunStatus::Running);
        assert!(goal_run.completed_at.is_none());
        assert!(goal_run.last_error.is_none());
        assert_eq!(goal_run.steps[1].status, GoalRunStepStatus::Pending);
        assert!(goal_run.steps[1].task_id.is_none());
        assert!(goal_run.generated_skill_path.is_some());
    }

    #[test]
    fn rerun_goal_run_from_step_resets_following_steps_and_skill_output() {
        let mut goal_run = sample_goal_run();

        rerun_goal_run_from_step(&mut goal_run, Some(0)).expect("rerun should succeed");

        assert_eq!(goal_run.current_step_index, 0);
        assert_eq!(goal_run.status, GoalRunStatus::Running);
        assert!(goal_run.completed_at.is_none());
        assert!(goal_run.generated_skill_path.is_none());
        assert!(goal_run.reflection_summary.is_none());
        assert_eq!(goal_run.steps[0].status, GoalRunStepStatus::Pending);
        assert_eq!(goal_run.steps[1].status, GoalRunStepStatus::Pending);
    }

    #[test]
    fn project_goal_run_snapshot_derives_metrics() {
        let goal_run = sample_goal_run();
        let tasks = vec![sample_task("task-b", "goal_test")];

        let projected = project_goal_run_snapshot(goal_run, &tasks, 100);

        assert_eq!(projected.current_step_title.as_deref(), Some("Fix"));
        assert_eq!(projected.child_task_count, 2);
        assert_eq!(projected.approval_count, 1);
        assert_eq!(projected.awaiting_approval_id.as_deref(), Some("apr-1"));
        assert_eq!(
            projected.failure_cause.as_deref(),
            Some("child task failed")
        );
        assert_eq!(projected.duration_ms, Some(60));
    }

    #[test]
    fn make_goal_run_event_with_todos_preserves_snapshot() {
        let event = make_goal_run_event_with_todos(
            "todo",
            "goal todo updated",
            None,
            Some(1),
            vec![TodoItem {
                id: "todo-1".to_string(),
                content: "Inspect failing test".to_string(),
                status: TodoStatus::InProgress,
                position: 0,
                step_index: Some(1),
                created_at: 10,
                updated_at: 20,
            }],
        );

        assert_eq!(event.phase, "todo");
        assert_eq!(event.step_index, Some(1));
        assert_eq!(event.todo_snapshot.len(), 1);
        assert_eq!(event.todo_snapshot[0].content, "Inspect failing test");
    }

    #[test]
    fn planner_required_for_message_detects_multi_step_requests() {
        assert!(planner_required_for_message(
            "Investigate the failing tests, then update the parser, and finally rerun the suite."
        ));
        assert!(planner_required_for_message(
            "1. Inspect logs\n2. Find the bad config\n3. Patch it"
        ));
    }

    #[test]
    fn planner_required_for_message_skips_simple_requests() {
        assert!(!planner_required_for_message("What port is the daemon listening on?"));
        assert!(!planner_required_for_message("Show me the last error."));
    }
}
