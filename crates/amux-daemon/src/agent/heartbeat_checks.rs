//! Built-in heartbeat check functions — structured data gathering (no LLM calls).
//!
//! Per D-01: Each check is a standalone async method on AgentEngine that reads
//! in-memory state and returns a HeartbeatCheckResult. Per D-02: four checks
//! ship in Phase 2.

use super::*;

impl AgentEngine {
    /// Check for TODOs that have been pending/in-progress longer than threshold. Per D-02/BEAT-02.
    pub(super) async fn check_stale_todos(&self, threshold_hours: u64) -> HeartbeatCheckResult {
        let now = now_millis();
        let threshold_ms = threshold_hours * 3600 * 1000;
        let todos = self.thread_todos.read().await;
        let stale: Vec<CheckDetail> = todos
            .values()
            .flat_map(|items| items.iter())
            .filter(|t| matches!(t.status, TodoStatus::Pending | TodoStatus::InProgress))
            .filter(|t| now.saturating_sub(t.updated_at) >= threshold_ms)
            .map(|t| {
                let age_h = (now.saturating_sub(t.updated_at)) as f64 / 3_600_000.0;
                CheckDetail {
                    id: t.id.clone(),
                    label: t.content.clone(),
                    age_hours: age_h,
                    severity: if age_h > (threshold_hours as f64 * 3.0) {
                        CheckSeverity::High
                    } else if age_h > (threshold_hours as f64 * 1.5) {
                        CheckSeverity::Medium
                    } else {
                        CheckSeverity::Low
                    },
                    context: format!(
                        "TODO '{}' ({:?}) last updated {:.1}h ago",
                        t.content, t.status, age_h
                    ),
                }
            })
            .collect();

        HeartbeatCheckResult {
            check_type: HeartbeatCheckType::StaleTodos,
            items_found: stale.len(),
            summary: if stale.is_empty() {
                "No stale TODOs.".into()
            } else {
                format!("{} TODO(s) older than {}h", stale.len(), threshold_hours)
            },
            details: stale,
        }
    }

    /// Check for goal runs stuck in Running/Planning/AwaitingApproval longer than threshold. Per D-02/BEAT-02.
    pub(super) async fn check_stuck_goal_runs(&self, threshold_hours: u64) -> HeartbeatCheckResult {
        let now = now_millis();
        let threshold_ms = threshold_hours * 3600 * 1000;
        let goal_runs = self.goal_runs.lock().await;
        let stuck: Vec<CheckDetail> = goal_runs
            .iter()
            .filter(|g| {
                matches!(
                    g.status,
                    GoalRunStatus::Running
                        | GoalRunStatus::Planning
                        | GoalRunStatus::AwaitingApproval
                )
            })
            .filter(|g| now.saturating_sub(g.updated_at) >= threshold_ms)
            .map(|g| {
                let age_h = (now.saturating_sub(g.updated_at)) as f64 / 3_600_000.0;
                CheckDetail {
                    id: g.id.clone(),
                    label: g.title.clone(),
                    age_hours: age_h,
                    severity: if age_h > (threshold_hours as f64 * 4.0) {
                        CheckSeverity::Critical
                    } else if age_h > (threshold_hours as f64 * 2.0) {
                        CheckSeverity::High
                    } else {
                        CheckSeverity::Medium
                    },
                    context: format!(
                        "Goal '{}' status {:?}, last update {:.1}h ago{}",
                        g.title,
                        g.status,
                        age_h,
                        g.last_error
                            .as_ref()
                            .map(|e| format!(", error: {}", e))
                            .unwrap_or_default()
                    ),
                }
            })
            .collect();

        HeartbeatCheckResult {
            check_type: HeartbeatCheckType::StuckGoalRuns,
            items_found: stuck.len(),
            summary: if stuck.is_empty() {
                "No stuck goal runs.".into()
            } else {
                format!(
                    "{} goal run(s) stuck for >{}h",
                    stuck.len(),
                    threshold_hours
                )
            },
            details: stuck,
        }
    }

    /// Check for unreplied gateway messages. Per D-02/BEAT-02/GATE-06.
    ///
    /// Compares `last_incoming_at` vs `last_response_at` per channel in GatewayState.
    /// A channel is considered "unreplied" when:
    /// 1. It has an incoming message timestamp newer than the last response timestamp
    ///    (or no response at all), AND
    /// 2. The incoming message is older than `threshold_hours` (prevents flagging
    ///    messages that just arrived — gives the agent time to respond).
    pub(super) async fn check_unreplied_messages(
        &self,
        threshold_hours: u64,
    ) -> HeartbeatCheckResult {
        let now = now_millis();
        let threshold_ms = threshold_hours * 3600 * 1000;

        // Read gateway_threads for sender context (maps thread_id -> gateway channel key)
        let gateway_threads = self.gateway_threads.read().await;

        // Read gateway_state for last_incoming_at and last_response_at
        let gw_lock = self.gateway_state.lock().await;

        let mut unreplied: Vec<CheckDetail> = Vec::new();

        if let Some(gw) = gw_lock.as_ref() {
            for (channel_key, &incoming_at) in &gw.last_incoming_at {
                // Check if we've responded after the incoming message
                let responded = gw
                    .last_response_at
                    .get(channel_key)
                    .map(|&resp_at| resp_at >= incoming_at)
                    .unwrap_or(false);

                if responded {
                    continue;
                }

                // Check if the incoming message is old enough to flag
                // (prevents flagging messages that just arrived)
                let elapsed_ms = now.saturating_sub(incoming_at);
                if elapsed_ms < threshold_ms {
                    continue;
                }

                let age_h = elapsed_ms as f64 / 3_600_000.0;

                // Try to find sender info from gateway_threads
                let sender = gateway_threads
                    .iter()
                    .find(|(_, v)| v.as_str() == channel_key)
                    .map(|(k, _)| k.clone())
                    .unwrap_or_else(|| "unknown".to_string());

                let severity = if age_h > (threshold_hours as f64 * 4.0) {
                    CheckSeverity::High
                } else if age_h > (threshold_hours as f64 * 2.0) {
                    CheckSeverity::Medium
                } else {
                    CheckSeverity::Low
                };

                unreplied.push(CheckDetail {
                    id: channel_key.clone(),
                    label: format!("Unreplied message on {channel_key}"),
                    age_hours: age_h,
                    severity,
                    context: format!(
                        "Message from '{}' on {} unreplied for {:.1}h",
                        sender, channel_key, age_h
                    ),
                });
            }
        }

        drop(gw_lock);

        HeartbeatCheckResult {
            check_type: HeartbeatCheckType::UnrepliedGatewayMessages,
            items_found: unreplied.len(),
            summary: if unreplied.is_empty() {
                "No unreplied gateway messages.".into()
            } else {
                format!(
                    "{} unreplied gateway conversation(s) for >{}h",
                    unreplied.len(),
                    threshold_hours
                )
            },
            details: unreplied,
        }
    }

    /// Check for repo changes using git status. Per D-05/BEAT-02.
    /// Uses spawn_blocking to avoid blocking the tokio reactor.
    pub(super) async fn check_repo_changes(&self) -> HeartbeatCheckResult {
        let data_dir = self.data_dir.clone();
        // Find the parent of data_dir as the likely project root
        let repo_path = data_dir
            .parent()
            .and_then(|p| p.parent())
            .map(|p| p.to_string_lossy().to_string());

        let repo_path = match repo_path {
            Some(p) => p,
            None => {
                return HeartbeatCheckResult {
                    check_type: HeartbeatCheckType::RepoChanges,
                    items_found: 0,
                    summary: "No repo path available.".into(),
                    details: vec![],
                };
            }
        };

        // Check if git is available on PATH
        let has_git = which::which("git").is_ok();
        if !has_git {
            return HeartbeatCheckResult {
                check_type: HeartbeatCheckType::RepoChanges,
                items_found: 0,
                summary: "Git not available on PATH.".into(),
                details: vec![],
            };
        }

        // Run git status in spawn_blocking to avoid blocking the reactor (Pitfall 2)
        let path_clone = repo_path.clone();
        let git_info = match tokio::task::spawn_blocking(move || {
            crate::git::get_git_status(&path_clone)
        })
        .await
        {
            Ok(info) => info,
            Err(e) => {
                tracing::warn!("git status check failed: {e}");
                return HeartbeatCheckResult {
                    check_type: HeartbeatCheckType::RepoChanges,
                    items_found: 0,
                    summary: format!("Git check failed: {e}"),
                    details: vec![],
                };
            }
        };

        let total_changes = git_info.modified + git_info.staged + git_info.untracked;
        let mut details = Vec::new();

        if git_info.modified > 0 {
            details.push(CheckDetail {
                id: "repo_modified".into(),
                label: format!("{} modified file(s)", git_info.modified),
                age_hours: 0.0,
                severity: CheckSeverity::Low,
                context: format!("{} modified file(s) in {}", git_info.modified, repo_path),
            });
        }
        if git_info.staged > 0 {
            details.push(CheckDetail {
                id: "repo_staged".into(),
                label: format!("{} staged file(s)", git_info.staged),
                age_hours: 0.0,
                severity: CheckSeverity::Low,
                context: format!("{} staged file(s) ready to commit", git_info.staged),
            });
        }
        if git_info.untracked > 0 {
            details.push(CheckDetail {
                id: "repo_untracked".into(),
                label: format!("{} untracked file(s)", git_info.untracked),
                age_hours: 0.0,
                severity: CheckSeverity::Low,
                context: format!("{} untracked file(s)", git_info.untracked),
            });
        }

        HeartbeatCheckResult {
            check_type: HeartbeatCheckType::RepoChanges,
            items_found: total_changes as usize,
            summary: if total_changes == 0 {
                format!(
                    "Repo clean on branch {}",
                    git_info.branch.as_deref().unwrap_or("unknown")
                )
            } else {
                format!(
                    "{} change(s) on branch {} ({} modified, {} staged, {} untracked)",
                    total_changes,
                    git_info.branch.as_deref().unwrap_or("unknown"),
                    git_info.modified,
                    git_info.staged,
                    git_info.untracked
                )
            },
            details,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;

    fn make_todo(id: &str, content: &str, status: TodoStatus, updated_at: u64) -> TodoItem {
        TodoItem {
            id: id.to_string(),
            content: content.to_string(),
            status,
            position: 0,
            step_index: None,
            created_at: updated_at,
            updated_at,
        }
    }

    fn make_goal_run(id: &str, title: &str, status: GoalRunStatus, updated_at: u64) -> GoalRun {
        GoalRun {
            id: id.to_string(),
            title: title.to_string(),
            goal: title.to_string(),
            client_request_id: None,
            status,
            priority: TaskPriority::Normal,
            created_at: updated_at,
            updated_at,
            started_at: Some(updated_at),
            completed_at: None,
            thread_id: None,
            session_id: None,
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
            child_task_ids: Vec::new(),
            child_task_count: 0,
            approval_count: 0,
            awaiting_approval_id: None,
            active_task_id: None,
            duration_ms: None,
            steps: Vec::new(),
            events: Vec::new(),
            total_prompt_tokens: 0,
            total_completion_tokens: 0,
            estimated_cost_usd: None,
            autonomy_level: Default::default(),
            authorship_tag: None,
        }
    }

    /// Helper to build a minimal AgentEngine for testing check functions.
    /// Only populates the fields needed by check functions.
    async fn make_test_engine(
        todos: HashMap<String, Vec<TodoItem>>,
        goal_runs: VecDeque<GoalRun>,
        gateway_threads: HashMap<String, String>,
    ) -> Arc<AgentEngine> {
        use crate::agent::circuit_breaker::CircuitBreakerRegistry;
        use crate::agent::concierge::ConciergeEngine;

        let config = AgentConfig::default();
        let (event_tx, _) = broadcast::channel(16);
        let (watcher_refresh_tx, watcher_refresh_rx) = mpsc::unbounded_channel();
        let http_client = reqwest::Client::new();
        let circuit_breakers = Arc::new(CircuitBreakerRegistry::from_provider_keys(
            std::iter::empty::<String>(),
        ));
        let config = Arc::new(RwLock::new(config));
        let concierge = Arc::new(ConciergeEngine::new(
            config.clone(),
            event_tx.clone(),
            http_client.clone(),
            circuit_breakers.clone(),
        ));

        // Create a minimal HistoryStore using a temp path
        let data_dir = std::env::temp_dir().join("tamux-test-heartbeat-checks");
        let _ = std::fs::create_dir_all(&data_dir);

        let history = crate::history::HistoryStore::new()
            .await
            .expect("test history store");
        let sm = crate::session_manager::SessionManager::new_with_history(
            Arc::new(
                crate::history::HistoryStore::new()
                    .await
                    .expect("test history store for session manager"),
            ),
            1024,
        );

        Arc::new(AgentEngine {
            started_at_ms: now_millis(),
            config,
            http_client,
            concierge,
            session_manager: sm,
            history,
            threads: RwLock::new(HashMap::new()),
            thread_todos: RwLock::new(todos),
            thread_work_contexts: RwLock::new(HashMap::new()),
            tasks: Mutex::new(VecDeque::new()),
            goal_runs: Mutex::new(goal_runs),
            inflight_goal_runs: Mutex::new(HashSet::new()),
            heartbeat_items: RwLock::new(Vec::new()),
            event_tx,
            memory: RwLock::new(HashMap::new()),
            recent_policy_decisions: RwLock::new(HashMap::new()),
            retry_guards: RwLock::new(HashMap::new()),
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
            gateway_discord_channels: RwLock::new(Vec::new()),
            gateway_slack_channels: RwLock::new(Vec::new()),
            gateway_threads: RwLock::new(gateway_threads),
            gateway_route_modes: RwLock::new(HashMap::new()),
            gateway_seen_ids: Mutex::new(Vec::new()),
            gateway_inflight_channels: Mutex::new(HashSet::new()),
            gateway_injected_messages: Mutex::new(VecDeque::new()),
            whatsapp_link: Arc::new(super::whatsapp_link::WhatsAppLinkRuntime::new()),
            external_runners: RwLock::new(HashMap::new()),
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

    #[tokio::test]
    async fn heartbeat_checks_stale_todos_detects_old_pending() {
        let now = now_millis();
        let old = now - (25 * 3600 * 1000); // 25 hours ago

        let mut todos = HashMap::new();
        todos.insert(
            "thread-1".to_string(),
            vec![
                make_todo("todo-1", "Fix bug", TodoStatus::Pending, old),
                make_todo("todo-2", "Write docs", TodoStatus::InProgress, old),
                make_todo("todo-3", "Ship it", TodoStatus::Completed, old), // completed should be skipped
            ],
        );

        let engine = make_test_engine(todos, VecDeque::new(), HashMap::new()).await;
        let result = engine.check_stale_todos(24).await;

        assert_eq!(result.check_type, HeartbeatCheckType::StaleTodos);
        assert_eq!(result.items_found, 2);
        assert_eq!(result.details.len(), 2);
    }

    #[tokio::test]
    async fn heartbeat_checks_stale_todos_none_stale() {
        let now = now_millis();
        let recent = now - (1 * 3600 * 1000); // 1 hour ago

        let mut todos = HashMap::new();
        todos.insert(
            "thread-1".to_string(),
            vec![make_todo("todo-1", "New task", TodoStatus::Pending, recent)],
        );

        let engine = make_test_engine(todos, VecDeque::new(), HashMap::new()).await;
        let result = engine.check_stale_todos(24).await;

        assert_eq!(result.items_found, 0);
        assert!(result.summary.contains("No stale TODOs"));
    }

    #[tokio::test]
    async fn heartbeat_checks_stuck_goals_detects_old_running() {
        let now = now_millis();
        let old = now - (3 * 3600 * 1000); // 3 hours ago

        let goal_runs = VecDeque::from(vec![
            make_goal_run("goal-1", "Deploy feature", GoalRunStatus::Running, old),
            make_goal_run(
                "goal-2",
                "Complete migration",
                GoalRunStatus::Completed,
                old,
            ), // completed should be skipped
        ]);

        let engine = make_test_engine(HashMap::new(), goal_runs, HashMap::new()).await;
        let result = engine.check_stuck_goal_runs(2).await;

        assert_eq!(result.check_type, HeartbeatCheckType::StuckGoalRuns);
        assert_eq!(result.items_found, 1);
        assert_eq!(result.details[0].id, "goal-1");
    }

    #[tokio::test]
    async fn heartbeat_checks_stuck_goals_none_stuck() {
        let now = now_millis();
        let recent = now - (30 * 60 * 1000); // 30 minutes ago

        let goal_runs = VecDeque::from(vec![make_goal_run(
            "goal-1",
            "Active goal",
            GoalRunStatus::Running,
            recent,
        )]);

        let engine = make_test_engine(HashMap::new(), goal_runs, HashMap::new()).await;
        let result = engine.check_stuck_goal_runs(2).await;

        assert_eq!(result.items_found, 0);
        assert!(result.summary.contains("No stuck"));
    }

    #[tokio::test]
    async fn heartbeat_checks_unreplied_empty_gateway() {
        let engine = make_test_engine(HashMap::new(), VecDeque::new(), HashMap::new()).await;
        let result = engine.check_unreplied_messages(1).await;

        assert_eq!(
            result.check_type,
            HeartbeatCheckType::UnrepliedGatewayMessages
        );
        assert_eq!(result.items_found, 0);
        assert!(result.summary.contains("No unreplied gateway"));
    }

    #[tokio::test]
    async fn heartbeat_checks_ignore_channels_with_newer_response_timestamps() {
        let engine = make_test_engine(HashMap::new(), VecDeque::new(), HashMap::new()).await;
        let mut gateway_state =
            crate::agent::gateway::GatewayState::new(AgentConfig::default().gateway, reqwest::Client::new());
        let now = now_millis();
        gateway_state
            .last_incoming_at
            .insert("Slack:C123".to_string(), now.saturating_sub(3_600_000));
        gateway_state
            .last_response_at
            .insert("Slack:C123".to_string(), now.saturating_sub(60_000));
        *engine.gateway_state.lock().await = Some(gateway_state);

        let result = engine.check_unreplied_messages(1).await;
        assert_eq!(result.items_found, 0);
        assert!(result.summary.contains("No unreplied gateway"));
    }

    #[tokio::test]
    async fn heartbeat_checks_read_gateway_health_from_ipc_updates() {
        let engine = make_test_engine(HashMap::new(), VecDeque::new(), HashMap::new()).await;
        let mut gateway_state = crate::agent::gateway::GatewayState::new(
            AgentConfig::default().gateway,
            reqwest::Client::new(),
        );
        crate::agent::gateway_loop::apply_health_snapshot(
            &mut gateway_state,
            &amux_protocol::GatewayHealthState {
                platform: "slack".to_string(),
                status: amux_protocol::GatewayConnectionStatus::Error,
                last_success_at_ms: Some(100),
                last_error_at_ms: Some(200),
                consecutive_failure_count: 3,
                last_error: Some("api timeout".to_string()),
                current_backoff_secs: 30,
            },
        );
        *engine.gateway_state.lock().await = Some(gateway_state);

        let snapshots = engine.gateway_health_snapshots().await;
        let slack = snapshots
            .iter()
            .find(|snapshot| snapshot.platform == "slack")
            .expect("slack snapshot should exist");
        assert_eq!(slack.status, amux_protocol::GatewayConnectionStatus::Error);
        assert_eq!(slack.consecutive_failure_count, 3);
        assert_eq!(slack.last_error.as_deref(), Some("api timeout"));
        assert_eq!(slack.current_backoff_secs, 30);
    }

    #[tokio::test]
    async fn heartbeat_checks_repo_changes_graceful_on_no_data_dir() {
        // Engine with a non-existent data_dir parent chain
        let engine = make_test_engine(HashMap::new(), VecDeque::new(), HashMap::new()).await;
        let result = engine.check_repo_changes().await;

        assert_eq!(result.check_type, HeartbeatCheckType::RepoChanges);
        // Should not panic, should return gracefully
        // Items may be 0 or non-zero depending on actual git state
    }

    #[tokio::test]
    async fn heartbeat_checks_independent() {
        // All checks should work independently even when data is empty
        let engine = make_test_engine(HashMap::new(), VecDeque::new(), HashMap::new()).await;

        let stale = engine.check_stale_todos(24).await;
        let stuck = engine.check_stuck_goal_runs(2).await;
        let unreplied = engine.check_unreplied_messages(1).await;
        let repo = engine.check_repo_changes().await;

        // Each should return a valid result without affecting others
        assert_eq!(stale.check_type, HeartbeatCheckType::StaleTodos);
        assert_eq!(stuck.check_type, HeartbeatCheckType::StuckGoalRuns);
        assert_eq!(
            unreplied.check_type,
            HeartbeatCheckType::UnrepliedGatewayMessages
        );
        assert_eq!(repo.check_type, HeartbeatCheckType::RepoChanges);
    }
}
