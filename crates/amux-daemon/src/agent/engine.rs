//! AgentEngine struct definition and constructor.

use super::circuit_breaker::CircuitBreakerRegistry;
use super::concierge::ConciergeEngine;
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

#[derive(Clone, Debug)]
pub(super) struct SubagentRuntimeStats {
    pub task_id: String,
    pub parent_task_id: Option<String>,
    pub thread_id: Option<String>,
    pub started_at: u64,
    pub created_at: u64,
    pub max_duration_secs: Option<u64>,
    pub context_budget_tokens: Option<u32>,
    pub last_tool_call_at: Option<u64>,
    pub last_progress_at: Option<u64>,
    pub tool_calls_total: u32,
    pub tool_calls_succeeded: u32,
    pub tool_calls_failed: u32,
    pub consecutive_errors: u32,
    pub recent_tool_names: VecDeque<String>,
    pub tokens_consumed: u32,
    pub context_utilization_pct: u32,
    pub health_state: SubagentHealthState,
    pub updated_at: u64,
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
    pub started_at_ms: u64,
    pub config: Arc<RwLock<AgentConfig>>,
    pub http_client: reqwest::Client,
    pub concierge: Arc<ConciergeEngine>,
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
    pub(super) operator_model: RwLock<OperatorModel>,
    pub(super) anticipatory: RwLock<AnticipatoryRuntime>,
    pub(super) collaboration: RwLock<HashMap<String, collaboration::CollaborationSession>>,
    pub data_dir: PathBuf,
    pub gateway_process: Mutex<Option<tokio::process::Child>>,
    pub gateway_state: Mutex<Option<gateway::GatewayState>>,
    /// Discord channel IDs to poll (parsed from config).
    pub gateway_discord_channels: RwLock<Vec<String>>,
    /// Slack channel IDs to poll (parsed from config).
    pub gateway_slack_channels: RwLock<Vec<String>>,
    /// Maps gateway channel IDs to daemon thread IDs for conversation continuity.
    pub gateway_threads: RwLock<HashMap<String, String>>,
    /// Recently-seen gateway message IDs for deduplication (capped ring buffer).
    pub gateway_seen_ids: Mutex<Vec<String>>,
    /// Channels currently being processed — prevents concurrent dispatch to the same channel.
    pub gateway_inflight_channels: Mutex<HashSet<String>>,
    /// External agent runners for openclaw/hermes backends.
    pub external_runners: RwLock<HashMap<String, external_runner::ExternalAgentRunner>>,
    pub(super) subagent_runtime: RwLock<HashMap<String, SubagentRuntimeStats>>,
    /// Active cancellation tokens per thread for stop-stream behavior.
    pub stream_cancellations: Mutex<HashMap<String, StreamCancellationEntry>>,
    pub stream_generation: AtomicU64,
    pub(super) active_operator_sessions: RwLock<HashMap<String, u64>>,
    pub(super) pending_operator_approvals: RwLock<HashMap<String, PendingApprovalObservation>>,
    pub(super) honcho_sync: Mutex<HonchoSyncState>,
    pub repo_watchers: Mutex<HashMap<String, ThreadRepoWatcher>>,
    pub watcher_refresh_tx: mpsc::UnboundedSender<String>,
    pub watcher_refresh_rx: Mutex<Option<mpsc::UnboundedReceiver<String>>>,
    /// Per-provider circuit breakers for LLM call path gating.
    pub circuit_breakers: Arc<CircuitBreakerRegistry>,
    /// Notifies the run_loop when config changes so heartbeat schedule can be recomputed.
    pub config_notify: tokio::sync::Notify,
    /// Learned priority weights per check type, updated from feedback signals (D-04).
    /// When present, these override the config `priority_weight` fields.
    /// Falls back to config weights when a check type has no learned weight.
    pub(crate) learned_check_weights: RwLock<HashMap<HeartbeatCheckType, f64>>,
    /// Aggregated learned heuristics persisted across restarts (Phase 5).
    pub(super) heuristic_store: RwLock<super::learning::heuristics::HeuristicStore>,
    /// Mined tool-usage patterns persisted across restarts (Phase 5).
    pub(super) pattern_store: RwLock<super::learning::patterns::PatternStore>,
    /// Feature disclosure queue for progressive tier-based feature revelation (D-13).
    pub(super) disclosure_queue: RwLock<super::capability_tier::DisclosureQueue>,
    /// Plugin manager for API proxy tool executor access (Phase 17).
    /// Set after both AgentEngine and PluginManager are constructed in server.rs.
    pub plugin_manager: std::sync::OnceLock<Arc<crate::plugin::PluginManager>>,
}

impl AgentEngine {
    pub fn new_with_shared_history(
        session_manager: Arc<SessionManager>,
        config: AgentConfig,
        history: Arc<HistoryStore>,
    ) -> Arc<Self> {
        Self::new_with_storage(
            session_manager,
            config,
            (*history).clone(),
            agent_data_dir(),
        )
    }

    fn new_with_storage(
        session_manager: Arc<SessionManager>,
        config: AgentConfig,
        history: HistoryStore,
        data_dir: PathBuf,
    ) -> Arc<Self> {
        let (event_tx, _) = broadcast::channel(config.agent_event_channel_capacity);
        let (watcher_refresh_tx, watcher_refresh_rx) = mpsc::unbounded_channel();

        // Pre-initialize external agent runners for discovery
        let mut runners = HashMap::new();
        for agent_type in &["openclaw", "hermes"] {
            runners.insert(
                agent_type.to_string(),
                external_runner::ExternalAgentRunner::new(agent_type, event_tx.clone()),
            );
        }

        let http_client = reqwest::Client::new();

        // Pre-initialize per-provider circuit breakers from configured providers.
        let circuit_breakers = Arc::new(CircuitBreakerRegistry::from_provider_keys(
            config
                .providers
                .keys()
                .cloned()
                .chain(std::iter::once(config.provider.clone())),
        ));

        let config = Arc::new(RwLock::new(config));
        let concierge = Arc::new(ConciergeEngine::new(
            config.clone(),
            event_tx.clone(),
            http_client.clone(),
            circuit_breakers.clone(),
        ));

        Arc::new(Self {
            started_at_ms: now_millis(),
            config,
            http_client,
            concierge,
            session_manager,
            history,
            threads: RwLock::new(HashMap::new()),
            thread_todos: RwLock::new(HashMap::new()),
            thread_work_contexts: RwLock::new(HashMap::new()),
            tasks: Mutex::new(VecDeque::new()),
            goal_runs: Mutex::new(VecDeque::new()),
            inflight_goal_runs: Mutex::new(HashSet::new()),
            heartbeat_items: RwLock::new(Vec::new()),
            event_tx,
            memory: RwLock::new(AgentMemory::default()),
            operator_model: RwLock::new(OperatorModel::default()),
            anticipatory: RwLock::new(AnticipatoryRuntime::default()),
            collaboration: RwLock::new(HashMap::new()),
            data_dir,
            gateway_process: Mutex::new(None),
            gateway_state: Mutex::new(None),
            gateway_discord_channels: RwLock::new(Vec::new()),
            gateway_slack_channels: RwLock::new(Vec::new()),
            gateway_threads: RwLock::new(HashMap::new()),
            gateway_seen_ids: Mutex::new(Vec::new()),
            gateway_inflight_channels: Mutex::new(HashSet::new()),
            external_runners: RwLock::new(runners),
            subagent_runtime: RwLock::new(HashMap::new()),
            stream_cancellations: Mutex::new(HashMap::new()),
            stream_generation: AtomicU64::new(1),
            active_operator_sessions: RwLock::new(HashMap::new()),
            pending_operator_approvals: RwLock::new(HashMap::new()),
            honcho_sync: Mutex::new(HonchoSyncState::default()),
            repo_watchers: Mutex::new(HashMap::new()),
            watcher_refresh_tx,
            watcher_refresh_rx: Mutex::new(Some(watcher_refresh_rx)),
            circuit_breakers,
            config_notify: tokio::sync::Notify::new(),
            learned_check_weights: RwLock::new(HashMap::new()),
            heuristic_store: RwLock::new(super::learning::heuristics::HeuristicStore::default()),
            pattern_store: RwLock::new(super::learning::patterns::PatternStore::default()),
            disclosure_queue: RwLock::new(super::capability_tier::DisclosureQueue::default()),
            plugin_manager: std::sync::OnceLock::new(),
        })
    }

    // ── Circuit breaker helpers ──────────────────────────────────────────

    /// Check the circuit breaker before an LLM call. Returns `Err` if the
    /// breaker is open (provider is unhealthy). Callers must invoke
    /// [`record_llm_outcome`] after the call completes.
    pub async fn check_circuit_breaker(&self, provider: &str) -> Result<()> {
        use super::circuit_breaker::CircuitState;

        let breaker_arc = self.circuit_breakers.get(provider).await;
        let mut breaker = breaker_arc.lock().await;
        let now = now_millis();

        if !breaker.can_execute(now) {
            let _ = self.event_tx.send(AgentEvent::ProviderCircuitOpen {
                provider: provider.to_string(),
                trip_count: breaker.trip_count(),
            });
            anyhow::bail!(
                "Circuit breaker open for provider '{}' — {} consecutive failures. \
                 Requests are blocked for ~30s to allow recovery.",
                provider,
                breaker.trip_count()
            );
        }
        Ok(())
    }

    /// Record the outcome of an LLM call for circuit breaker tracking.
    /// Emits [`AgentEvent::ProviderCircuitOpen`] on trip and
    /// [`AgentEvent::ProviderCircuitRecovered`] on recovery.
    pub async fn record_llm_outcome(&self, provider: &str, success: bool) {
        use super::circuit_breaker::CircuitState;

        let breaker_arc = self.circuit_breakers.get(provider).await;
        let mut breaker = breaker_arc.lock().await;
        let now = now_millis();

        if success {
            let was_half_open = breaker.state() == CircuitState::HalfOpen;
            breaker.record_success(now);
            if was_half_open && breaker.state() == CircuitState::Closed {
                tracing::info!(provider, "circuit breaker recovered — provider is healthy again");
                let _ = self.event_tx.send(AgentEvent::ProviderCircuitRecovered {
                    provider: provider.to_string(),
                });
            }
        } else {
            let was_closed_or_half = breaker.state() != CircuitState::Open;
            breaker.record_failure(now);
            if was_closed_or_half && breaker.state() == CircuitState::Open {
                tracing::warn!(
                    provider,
                    trips = breaker.trip_count(),
                    "circuit breaker tripped — provider marked unhealthy"
                );
                let _ = self.event_tx.send(AgentEvent::ProviderCircuitOpen {
                    provider: provider.to_string(),
                    trip_count: breaker.trip_count(),
                });
            }
        }
    }

    /// Suggest an alternative healthy provider when the requested one is unavailable.
    pub(super) async fn suggest_alternative_provider(&self, failed_provider: &str) -> Option<String> {
        let config = self.config.read().await;
        for (name, _pconfig) in &config.providers {
            if name != failed_provider {
                let breaker_arc = self.circuit_breakers.get(name).await;
                let mut breaker = breaker_arc.lock().await;
                let now = now_millis();
                if breaker.can_execute(now) {
                    return Some(format!(
                        "Consider switching to provider '{}' which is currently healthy.",
                        name
                    ));
                }
            }
        }
        None
    }

    #[cfg(test)]
    pub(crate) async fn new_test(
        session_manager: Arc<SessionManager>,
        config: AgentConfig,
        root: &std::path::Path,
    ) -> Arc<Self> {
        let history = HistoryStore::new_test_store(root)
            .await
            .expect("test history store initialization failed");
        let data_dir = root.join("agent");
        std::fs::create_dir_all(&data_dir).expect("failed to create test agent data dir");
        Self::new_with_storage(session_manager, config, history, data_dir)
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
