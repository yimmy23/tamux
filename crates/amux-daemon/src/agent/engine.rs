//! AgentEngine struct definition and constructor.

use super::*;

pub(super) struct SendMessageOutcome {
    pub thread_id: String,
    pub interrupted_for_approval: bool,
}

#[derive(Clone)]
pub struct StreamCancellationEntry {
    pub generation: u64,
    pub token: CancellationToken,
}

pub struct ThreadRepoWatcher {
    pub repo_root: String,
    pub watcher: RecommendedWatcher,
}

pub(super) const ONECONTEXT_BOOTSTRAP_QUERY_MAX_CHARS: usize = 180;
pub(super) const ONECONTEXT_BOOTSTRAP_OUTPUT_MAX_CHARS: usize = 5000;
pub(super) const MIN_CONTEXT_TARGET_TOKENS: usize = 1024;
pub(in crate::agent) const APPROX_CHARS_PER_TOKEN: usize = 4;
pub(super) const FILE_WATCH_DEBOUNCE_MS: u64 = 700;
pub(super) const FILE_WATCH_TICK_MS: u64 = 250;

pub(super) fn file_watch_event_is_relevant(event: &Event) -> bool {
    matches!(
        event.kind,
        EventKind::Any | EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_)
    )
}

/// Cached check for `aline` CLI availability (checked once per process).
pub(crate) fn aline_available() -> bool {
    static AVAILABLE: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *AVAILABLE.get_or_init(|| which::which("aline").is_ok())
}

// ---------------------------------------------------------------------------
// AgentEngine
// ---------------------------------------------------------------------------

pub struct AgentEngine {
    pub config: RwLock<AgentConfig>,
    pub http_client: reqwest::Client,
    pub session_manager: Arc<SessionManager>,
    pub history: HistoryStore,
    pub threads: RwLock<HashMap<String, AgentThread>>,
    pub thread_todos: RwLock<HashMap<String, Vec<TodoItem>>>,
    pub thread_work_contexts: RwLock<HashMap<String, ThreadWorkContext>>,
    pub tasks: Mutex<VecDeque<AgentTask>>,
    pub goal_runs: Mutex<VecDeque<GoalRun>>,
    pub inflight_goal_runs: Mutex<HashSet<String>>,
    pub heartbeat_items: RwLock<Vec<HeartbeatItem>>,
    pub event_tx: broadcast::Sender<AgentEvent>,
    pub memory: RwLock<AgentMemory>,
    pub data_dir: PathBuf,
    pub gateway_process: Mutex<Option<tokio::process::Child>>,
    pub gateway_state: Mutex<Option<gateway::GatewayState>>,
    /// Discord channel IDs to poll (parsed from config).
    pub gateway_discord_channels: RwLock<Vec<String>>,
    /// Slack channel IDs to poll (parsed from config).
    pub gateway_slack_channels: RwLock<Vec<String>>,
    /// Maps gateway channel IDs to daemon thread IDs for conversation continuity.
    pub gateway_threads: RwLock<HashMap<String, String>>,
    /// External agent runners for openclaw/hermes backends.
    pub external_runners: RwLock<HashMap<String, external_runner::ExternalAgentRunner>>,
    /// Active cancellation tokens per thread for stop-stream behavior.
    pub stream_cancellations: Mutex<HashMap<String, StreamCancellationEntry>>,
    pub stream_generation: AtomicU64,
    pub repo_watchers: Mutex<HashMap<String, ThreadRepoWatcher>>,
    pub watcher_refresh_tx: mpsc::UnboundedSender<String>,
    pub watcher_refresh_rx: Mutex<Option<mpsc::UnboundedReceiver<String>>>,
}

impl AgentEngine {
    pub fn new(session_manager: Arc<SessionManager>, config: AgentConfig) -> Arc<Self> {
        let (event_tx, _) = broadcast::channel(256);
        let (watcher_refresh_tx, watcher_refresh_rx) = mpsc::unbounded_channel();
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
            thread_work_contexts: RwLock::new(HashMap::new()),
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
            repo_watchers: Mutex::new(HashMap::new()),
            watcher_refresh_tx,
            watcher_refresh_rx: Mutex::new(Some(watcher_refresh_rx)),
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
}
