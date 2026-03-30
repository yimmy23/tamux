//! AgentEngine struct definition and constructor.

use super::circuit_breaker::CircuitBreakerRegistry;
use super::concierge::ConciergeEngine;
use super::*;
use std::time::Duration;

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
const AGENT_HTTP_CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
const AGENT_HTTP_READ_TIMEOUT: Duration = Duration::from_secs(125);

fn build_agent_http_client(read_timeout: Duration) -> reqwest::Client {
    reqwest::Client::builder()
        .connect_timeout(AGENT_HTTP_CONNECT_TIMEOUT)
        .read_timeout(read_timeout)
        .build()
        .expect("agent HTTP client configuration should be valid")
}

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

pub(super) async fn provider_is_eligible_for_alternative(
    config: &Arc<RwLock<AgentConfig>>,
    circuit_breakers: &Arc<CircuitBreakerRegistry>,
    failed_provider: &str,
    provider_id: &str,
) -> bool {
    if provider_id == failed_provider {
        return false;
    }

    let config_guard = config.read().await;
    let Ok(resolved) = resolve_candidate_provider_config(&config_guard, provider_id) else {
        return false;
    };
    drop(config_guard);

    if resolved.model.trim().is_empty() || resolved.base_url.trim().is_empty() {
        return false;
    }

    match resolved.auth_source {
        AuthSource::ApiKey => {
            if resolved.api_key.trim().is_empty() {
                return false;
            }
        }
        AuthSource::ChatgptSubscription => {
            if provider_id != "openai" || !super::llm_client::has_openai_chatgpt_subscription_auth()
            {
                return false;
            }
        }
    }

    let breaker_arc = circuit_breakers.get(provider_id).await;
    let mut breaker = breaker_arc.lock().await;
    breaker.can_execute(now_millis())
}

async fn collect_provider_alternatives(
    config: &Arc<RwLock<AgentConfig>>,
    circuit_breakers: &Arc<CircuitBreakerRegistry>,
    failed_provider: &str,
) -> Vec<ProviderAlternativeSuggestion> {
    let provider_ids: Vec<String> = config.read().await.providers.keys().cloned().collect();
    let mut alternatives = Vec::new();

    for provider_id in provider_ids {
        if !provider_is_eligible_for_alternative(
            config,
            circuit_breakers,
            failed_provider,
            provider_id.as_str(),
        )
        .await
        {
            continue;
        }

        let config_guard = config.read().await;
        let Ok(resolved) = resolve_candidate_provider_config(&config_guard, &provider_id) else {
            continue;
        };

        alternatives.push(ProviderAlternativeSuggestion {
            provider_id,
            model: Some(resolved.model),
            reason: "configured and healthy".to_string(),
        });
    }

    alternatives
}

pub(super) async fn collect_provider_outage_metadata(
    config: &Arc<RwLock<AgentConfig>>,
    circuit_breakers: &Arc<CircuitBreakerRegistry>,
    failed_provider: &str,
    trip_count: u32,
    reason: impl Into<String>,
) -> ProviderCircuitOpenDetails {
    let failed_model = {
        let config_guard = config.read().await;
        resolve_candidate_provider_config(&config_guard, failed_provider)
            .ok()
            .and_then(|resolved| (!resolved.model.trim().is_empty()).then_some(resolved.model))
    };

    ProviderCircuitOpenDetails {
        provider: failed_provider.to_string(),
        failed_model,
        trip_count,
        reason: reason.into(),
        suggested_alternatives: collect_provider_alternatives(
            config,
            circuit_breakers,
            failed_provider,
        )
        .await,
    }
}

pub(super) async fn collect_provider_health_snapshot(
    config: &Arc<RwLock<AgentConfig>>,
    circuit_breakers: &Arc<CircuitBreakerRegistry>,
) -> Vec<ProviderHealthSnapshot> {
    let provider_ids: Vec<String> = config.read().await.providers.keys().cloned().collect();
    let mut snapshots = Vec::new();

    for provider_id in provider_ids {
        let breaker_arc = circuit_breakers.get(&provider_id).await;
        let mut breaker = breaker_arc.lock().await;
        let can_execute = breaker.can_execute(now_millis());
        let trip_count = breaker.trip_count();
        drop(breaker);

        if can_execute {
            snapshots.push(ProviderHealthSnapshot {
                provider_id,
                can_execute,
                trip_count,
                failed_model: None,
                reason: None,
                suggested_alternatives: Vec::new(),
            });
            continue;
        }

        let outage = collect_provider_outage_metadata(
            config,
            circuit_breakers,
            &provider_id,
            trip_count,
            "circuit breaker open",
        )
        .await;
        snapshots.push(ProviderHealthSnapshot {
            provider_id: outage.provider,
            can_execute,
            trip_count: outage.trip_count,
            failed_model: outage.failed_model,
            reason: Some(outage.reason),
            suggested_alternatives: outage.suggested_alternatives,
        });
    }

    snapshots
}

pub(super) fn format_provider_outage_message(
    outage: &ProviderCircuitOpenDetails,
) -> Option<String> {
    if outage.suggested_alternatives.is_empty() {
        return None;
    }

    let alternatives = outage
        .suggested_alternatives
        .iter()
        .map(|alt| match &alt.model {
            Some(model) => format!("{} ({})", alt.provider_id, model),
            None => alt.provider_id.clone(),
        })
        .collect::<Vec<_>>()
        .join(", ");

    let model = outage
        .failed_model
        .as_ref()
        .map(|m| format!(" model '{}'", m))
        .unwrap_or_default();

    Some(format!(
        "Provider '{}'{} is temporarily unavailable ({}). Alternatives: {}.",
        outage.provider, model, outage.reason, alternatives
    ))
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
    pub memory: RwLock<HashMap<String, AgentMemory>>,
    pub(super) operator_model: RwLock<OperatorModel>,
    pub(super) anticipatory: RwLock<AnticipatoryRuntime>,
    pub(super) collaboration: RwLock<HashMap<String, collaboration::CollaborationSession>>,
    pub data_dir: PathBuf,
    pub gateway_process: Mutex<Option<tokio::process::Child>>,
    pub gateway_state: Mutex<Option<gateway::GatewayState>>,
    pub gateway_ipc_sender: Mutex<Option<mpsc::UnboundedSender<amux_protocol::DaemonMessage>>>,
    pub gateway_pending_send_results:
        Mutex<HashMap<String, tokio::sync::oneshot::Sender<amux_protocol::GatewaySendResult>>>,
    pub gateway_restart_attempts: Mutex<u32>,
    pub gateway_restart_not_before_ms: Mutex<Option<u64>>,
    pub whatsapp_link: Arc<whatsapp_link::WhatsAppLinkRuntime>,
    /// Discord channel IDs to poll (parsed from config).
    pub gateway_discord_channels: RwLock<Vec<String>>,
    /// Slack channel IDs to poll (parsed from config).
    pub gateway_slack_channels: RwLock<Vec<String>>,
    /// Maps gateway channel IDs to daemon thread IDs for conversation continuity.
    pub gateway_threads: RwLock<HashMap<String, String>>,
    /// Sticky agent route per gateway channel.
    pub gateway_route_modes: RwLock<HashMap<String, gateway::GatewayRouteMode>>,
    /// Recently-seen gateway message IDs for deduplication (capped ring buffer).
    pub gateway_seen_ids: Mutex<Vec<String>>,
    /// Channels currently being processed — prevents concurrent dispatch to the same channel.
    pub gateway_inflight_channels: Mutex<HashSet<String>>,
    /// Queue of externally injected gateway messages (e.g. linked WhatsApp sidecar).
    pub gateway_injected_messages: Mutex<VecDeque<gateway::IncomingMessage>>,
    /// External agent runners for openclaw/hermes backends.
    pub external_runners: RwLock<HashMap<String, external_runner::ExternalAgentRunner>>,
    pub(super) subagent_runtime: RwLock<HashMap<String, SubagentRuntimeStats>>,
    /// Active cancellation tokens per thread for stop-stream behavior.
    pub stream_cancellations: Mutex<HashMap<String, StreamCancellationEntry>>,
    pub stream_generation: AtomicU64,
    pub(super) active_operator_sessions: RwLock<HashMap<String, u64>>,
    pub(super) pending_operator_approvals: RwLock<HashMap<String, PendingApprovalObservation>>,
    pub(super) operator_profile_sessions: RwLock<HashMap<String, OperatorProfileSessionState>>,
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
    /// Episodic memory subsystem state (Phase v3.0).
    pub(super) episodic_store: RwLock<HashMap<String, super::episodic::EpisodicStore>>,
    /// Situational awareness monitor (Phase v3.0: AWAR-01).
    pub(super) awareness: RwLock<super::awareness::AwarenessMonitor>,
    /// Calibration tracker for uncertainty quantification (Phase v3.0: UNCR-07).
    pub(super) calibration_tracker: RwLock<super::uncertainty::calibration::CalibrationTracker>,
    /// Handoff broker for multi-agent task delegation (Phase v3.0: HAND-01).
    pub(super) handoff_broker: RwLock<super::handoff::HandoffBroker>,
    /// Active divergent sessions for parallel framing mode (Phase v3.0: DIVR-01).
    pub(super) divergent_sessions:
        RwLock<HashMap<String, super::handoff::divergent::DivergentSession>>,
    /// Per-goal-run cost trackers, keyed by goal_run_id (Phase v3.0: COST-01).
    pub(super) cost_trackers: Mutex<HashMap<String, super::cost::CostTracker>>,
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
        Self::new_with_storage_and_http_client(
            session_manager,
            config,
            history,
            data_dir,
            build_agent_http_client(AGENT_HTTP_READ_TIMEOUT),
        )
    }

    fn new_with_storage_and_http_client(
        session_manager: Arc<SessionManager>,
        config: AgentConfig,
        history: HistoryStore,
        data_dir: PathBuf,
        http_client: reqwest::Client,
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
            memory: RwLock::new(HashMap::new()),
            operator_model: RwLock::new(OperatorModel::default()),
            anticipatory: RwLock::new(AnticipatoryRuntime::default()),
            collaboration: RwLock::new(HashMap::new()),
            data_dir,
            gateway_process: Mutex::new(None),
            gateway_state: Mutex::new(None),
            gateway_ipc_sender: Mutex::new(None),
            gateway_pending_send_results: Mutex::new(HashMap::new()),
            gateway_restart_attempts: Mutex::new(0),
            gateway_restart_not_before_ms: Mutex::new(None),
            whatsapp_link: Arc::new(whatsapp_link::WhatsAppLinkRuntime::new()),
            gateway_discord_channels: RwLock::new(Vec::new()),
            gateway_slack_channels: RwLock::new(Vec::new()),
            gateway_threads: RwLock::new(HashMap::new()),
            gateway_route_modes: RwLock::new(HashMap::new()),
            gateway_seen_ids: Mutex::new(Vec::new()),
            gateway_inflight_channels: Mutex::new(HashSet::new()),
            gateway_injected_messages: Mutex::new(VecDeque::new()),
            external_runners: RwLock::new(runners),
            subagent_runtime: RwLock::new(HashMap::new()),
            stream_cancellations: Mutex::new(HashMap::new()),
            stream_generation: AtomicU64::new(1),
            active_operator_sessions: RwLock::new(HashMap::new()),
            pending_operator_approvals: RwLock::new(HashMap::new()),
            operator_profile_sessions: RwLock::new(HashMap::new()),
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
            episodic_store: RwLock::new(HashMap::new()),
            awareness: RwLock::new(super::awareness::AwarenessMonitor::new()),
            calibration_tracker: RwLock::new(
                super::uncertainty::calibration::CalibrationTracker::default(),
            ),
            handoff_broker: RwLock::new(super::handoff::HandoffBroker::default()),
            divergent_sessions: RwLock::new(HashMap::new()),
            cost_trackers: Mutex::new(HashMap::new()),
        })
    }

    // ── Circuit breaker helpers ──────────────────────────────────────────

    /// Check the circuit breaker before an LLM call. Returns `Err` if the
    /// breaker is open (provider is unhealthy). Callers must invoke
    /// [`record_llm_outcome`] after the call completes.
    pub async fn check_circuit_breaker(&self, provider: &str) -> Result<()> {
        let breaker_arc = self.circuit_breakers.get(provider).await;
        let mut breaker = breaker_arc.lock().await;
        let now = now_millis();

        if !breaker.can_execute(now) {
            let trip_count = breaker.trip_count();
            drop(breaker);
            let outage = collect_provider_outage_metadata(
                &self.config,
                &self.circuit_breakers,
                provider,
                trip_count,
                "circuit breaker open",
            )
            .await;
            let _ = self.event_tx.send(AgentEvent::ProviderCircuitOpen {
                provider: outage.provider,
                failed_model: outage.failed_model,
                trip_count: outage.trip_count,
                reason: outage.reason,
                suggested_alternatives: outage.suggested_alternatives,
            });
            anyhow::bail!(
                "Circuit breaker open for provider '{}' — {} consecutive failures. \
                 Requests are blocked for ~30s to allow recovery.",
                provider,
                trip_count
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
                tracing::info!(
                    provider,
                    "circuit breaker recovered — provider is healthy again"
                );
                let _ = self.event_tx.send(AgentEvent::ProviderCircuitRecovered {
                    provider: provider.to_string(),
                });
            }
        } else {
            let was_closed_or_half = breaker.state() != CircuitState::Open;
            breaker.record_failure(now);
            if was_closed_or_half && breaker.state() == CircuitState::Open {
                let trip_count = breaker.trip_count();
                drop(breaker);
                let outage = collect_provider_outage_metadata(
                    &self.config,
                    &self.circuit_breakers,
                    provider,
                    trip_count,
                    "circuit breaker tripped",
                )
                .await;
                tracing::warn!(
                    provider,
                    trips = trip_count,
                    "circuit breaker tripped — provider marked unhealthy"
                );
                let _ = self.event_tx.send(AgentEvent::ProviderCircuitOpen {
                    provider: outage.provider,
                    failed_model: outage.failed_model,
                    trip_count: outage.trip_count,
                    reason: outage.reason,
                    suggested_alternatives: outage.suggested_alternatives,
                });
            }
        }
    }

    async fn provider_is_eligible_for_alternative(
        &self,
        failed_provider: &str,
        provider_id: &str,
    ) -> bool {
        provider_is_eligible_for_alternative(
            &self.config,
            &self.circuit_breakers,
            failed_provider,
            provider_id,
        )
        .await
    }

    /// Suggest an alternative healthy provider when the requested one is unavailable.
    pub(super) async fn suggest_alternative_provider(
        &self,
        failed_provider: &str,
    ) -> Option<String> {
        let outage = collect_provider_outage_metadata(
            &self.config,
            &self.circuit_breakers,
            failed_provider,
            0,
            "circuit breaker open",
        )
        .await;
        format_provider_outage_message(&outage)
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use std::sync::{Mutex, OnceLock};
    use tempfile::TempDir;
    use tokio::io::AsyncReadExt;
    use tokio::net::TcpListener;

    async fn make_test_engine(config: AgentConfig) -> (Arc<AgentEngine>, TempDir) {
        let temp_dir = TempDir::new().expect("temp dir");
        let session_manager = SessionManager::new_test(temp_dir.path()).await;
        let history = HistoryStore::new_test_store(temp_dir.path())
            .await
            .expect("history store");
        let data_dir = temp_dir.path().join("agent");
        std::fs::create_dir_all(&data_dir).expect("create agent data dir");
        let engine = AgentEngine::new_with_storage_and_http_client(
            session_manager,
            config,
            history,
            data_dir,
            build_agent_http_client(Duration::from_millis(75)),
        );
        (engine, temp_dir)
    }

    fn provider_config(
        base_url: &str,
        model: &str,
        api_key: &str,
        auth_source: AuthSource,
    ) -> ProviderConfig {
        ProviderConfig {
            base_url: base_url.to_string(),
            model: model.to_string(),
            api_key: api_key.to_string(),
            assistant_id: String::new(),
            auth_source,
            api_transport: ApiTransport::ChatCompletions,
            reasoning_effort: String::new(),
            context_window_tokens: 0,
            response_schema: None,
        }
    }

    fn openai_auth_env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    fn write_openai_subscription_auth(path: &Path) {
        let auth = serde_json::json!({
            "provider": "openai-codex",
            "auth_mode": "chatgpt_subscription",
            "access_token": "header.eyJodHRwczovL2FwaS5vcGVuYWkuY29tL2F1dGgiOnsiY2hhdGdwdF9hY2NvdW50X2lkIjoiacctMSJ9LCJleHAiOjQxMDI0NDQ4MDB9.signature",
            "refresh_token": "refresh-token",
            "account_id": "acct-1",
            "expires_at": 4_102_444_800_000i64,
            "source": "test",
            "updated_at": 4_102_444_800_000i64,
            "created_at": 4_102_444_800_000i64
        });
        std::fs::write(
            path,
            serde_json::to_vec(&auth).expect("serialize auth fixture"),
        )
        .expect("write auth fixture");
    }

    #[tokio::test]
    async fn provider_alternative_excludes_placeholder_provider_row() {
        let mut config = AgentConfig::default();
        config.provider = "openai".to_string();
        config.providers.insert(
            "custom".to_string(),
            provider_config("", "", "", AuthSource::ApiKey),
        );
        let (engine, _temp_dir) = make_test_engine(config).await;

        let suggestion = engine.suggest_alternative_provider("openai").await;

        assert!(
            suggestion.is_none(),
            "placeholder provider rows must not be suggested"
        );
    }

    #[tokio::test]
    async fn provider_alternative_excludes_failed_provider_itself() {
        let mut config = AgentConfig::default();
        config.provider = "openai".to_string();
        config.providers.insert(
            "openai".to_string(),
            provider_config(
                "https://api.openai.com/v1",
                "gpt-4o",
                "valid-key",
                AuthSource::ApiKey,
            ),
        );
        let (engine, _temp_dir) = make_test_engine(config).await;

        let suggestion = engine.suggest_alternative_provider("openai").await;

        assert!(
            suggestion.is_none(),
            "the failed provider itself must not be suggested as an alternative"
        );
    }

    #[tokio::test]
    async fn provider_alternative_excludes_open_breaker_provider() {
        let mut config = AgentConfig::default();
        config.provider = "openai".to_string();
        config.providers.insert(
            "custom".to_string(),
            provider_config(
                "https://example.invalid/v1",
                "model-a",
                "valid-key",
                AuthSource::ApiKey,
            ),
        );
        let (engine, _temp_dir) = make_test_engine(config).await;
        {
            let breaker = engine.circuit_breakers.get("custom").await;
            let mut breaker = breaker.lock().await;
            let now = now_millis();
            for offset in 0..5 {
                breaker.record_failure(now + offset);
            }
        }

        let suggestion = engine.suggest_alternative_provider("openai").await;

        assert!(
            suggestion.is_none(),
            "providers with open circuit breakers must not be suggested"
        );
    }

    #[tokio::test]
    async fn provider_alternative_includes_configured_healthy_provider() {
        let mut config = AgentConfig::default();
        config.provider = "openai".to_string();
        config.providers.insert(
            "custom".to_string(),
            provider_config(
                "https://example.invalid/v1",
                "model-a",
                "valid-key",
                AuthSource::ApiKey,
            ),
        );
        let (engine, _temp_dir) = make_test_engine(config).await;

        let suggestion = engine.suggest_alternative_provider("openai").await;

        let suggestion = suggestion.expect("healthy provider should be suggested");
        assert!(
            suggestion.contains("custom"),
            "expected healthy configured provider to be suggested, got: {suggestion}"
        );
    }

    #[tokio::test]
    async fn provider_alternative_excludes_openai_subscription_without_auth() {
        let _env_guard = openai_auth_env_lock().lock().expect("lock auth env");
        let temp_dir = TempDir::new().expect("temp dir");
        let missing_auth_path = temp_dir.path().join("missing-openai-auth.json");
        std::env::set_var("TAMUX_OPENAI_CODEX_AUTH_PATH", &missing_auth_path);
        std::env::set_var("TAMUX_CODEX_CLI_AUTH_PATH", &missing_auth_path);

        let mut config = AgentConfig::default();
        config.provider = "groq".to_string();
        config.providers.insert(
            "openai".to_string(),
            provider_config(
                "https://api.openai.com/v1",
                "gpt-5.4",
                "",
                AuthSource::ChatgptSubscription,
            ),
        );
        let (engine, _temp_dir) = make_test_engine(config).await;

        let suggestion = engine.suggest_alternative_provider("groq").await;

        std::env::remove_var("TAMUX_OPENAI_CODEX_AUTH_PATH");
        std::env::remove_var("TAMUX_CODEX_CLI_AUTH_PATH");
        assert!(
            suggestion.is_none(),
            "OpenAI subscription auth must be present before suggesting it as an alternative"
        );
    }

    #[tokio::test]
    async fn provider_alternative_uses_candidate_default_model_for_empty_named_model() {
        let _env_guard = openai_auth_env_lock().lock().expect("lock auth env");
        let temp_dir = TempDir::new().expect("temp dir");
        let auth_path = temp_dir.path().join("openai-auth.json");
        write_openai_subscription_auth(&auth_path);
        std::env::set_var("TAMUX_OPENAI_CODEX_AUTH_PATH", &auth_path);
        std::env::set_var(
            "TAMUX_CODEX_CLI_AUTH_PATH",
            temp_dir.path().join("missing-codex-auth.json"),
        );

        let mut config = AgentConfig::default();
        config.provider = "openai".to_string();
        config.model = "gpt-5.4".to_string();
        config.providers.insert(
            "groq".to_string(),
            provider_config("", "", "groq-key", AuthSource::ApiKey),
        );
        let (engine, _temp_dir) = make_test_engine(config).await;

        let resolved = {
            let config = engine.config.read().await;
            resolve_candidate_provider_config(&config, "groq")
                .expect("candidate provider should resolve with its default model")
        };
        let suggestion = engine.suggest_alternative_provider("openai").await;

        std::env::remove_var("TAMUX_OPENAI_CODEX_AUTH_PATH");
        std::env::remove_var("TAMUX_CODEX_CLI_AUTH_PATH");
        assert_eq!(resolved.model, "llama-3.3-70b-versatile");
        assert!(
            suggestion.as_deref().unwrap_or_default().contains("groq"),
            "expected groq to remain eligible using its own default model"
        );
    }

    async fn spawn_hung_http_server() -> String {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind hung http server");
        let addr = listener.local_addr().expect("hung server local addr");
        tokio::spawn(async move {
            loop {
                let Ok((mut socket, _)) = listener.accept().await else {
                    break;
                };
                tokio::spawn(async move {
                    let mut buffer = [0u8; 1024];
                    let _ = socket.read(&mut buffer).await;
                    tokio::time::sleep(Duration::from_secs(5)).await;
                });
            }
        });
        format!("http://{addr}/v1")
    }

    #[tokio::test]
    async fn send_message_times_out_hung_provider_request() {
        let server_url = spawn_hung_http_server().await;
        let temp_dir = TempDir::new().expect("temp dir");
        let session_manager = SessionManager::new_test(temp_dir.path()).await;
        let history = HistoryStore::new_test_store(temp_dir.path())
            .await
            .expect("history store");
        let data_dir = temp_dir.path().join("agent");
        std::fs::create_dir_all(&data_dir).expect("create agent data dir");

        let mut config = AgentConfig::default();
        config.provider = "openai".to_string();
        config.base_url = server_url;
        config.model = "gpt-4o-mini".to_string();
        config.api_transport = ApiTransport::ChatCompletions;
        config.max_retries = 0;
        config.auto_retry = false;

        let engine = AgentEngine::new_with_storage_and_http_client(
            session_manager,
            config,
            history,
            data_dir,
            build_agent_http_client(Duration::from_millis(75)),
        );

        let result = tokio::time::timeout(
            Duration::from_secs(2),
            engine.send_message_inner(
                None,
                "What model are you?",
                None,
                None,
                None,
                None,
                None,
                true,
            ),
        )
        .await
        .expect("hung provider request should time out at the HTTP layer, not the test harness");

        let error = match result {
            Ok(_) => panic!("hung provider should surface as an error"),
            Err(error) => error,
        };
        let error_text = error.to_string().to_lowercase();
        assert!(
            error_text.contains("timed out"),
            "expected timeout error, got: {error}"
        );
    }
}
