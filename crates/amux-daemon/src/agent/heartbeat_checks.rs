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
    pub(super) async fn check_stuck_goal_runs(
        &self,
        threshold_hours: u64,
    ) -> HeartbeatCheckResult {
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

    /// Check for unreplied gateway messages. Per D-02/BEAT-02.
    /// Phase 2 scope: check if gateway_threads exist with no recent agent response.
    /// Full unreplied tracking deferred to Phase 8.
    pub(super) async fn check_unreplied_messages(
        &self,
        _threshold_hours: u64,
    ) -> HeartbeatCheckResult {
        let gateway_threads = self.gateway_threads.read().await;
        // Phase 2: report active gateway threads count as awareness item.
        // True unreplied detection requires response tracking (Phase 8).
        let active_count = gateway_threads.len();
        HeartbeatCheckResult {
            check_type: HeartbeatCheckType::UnrepliedGatewayMessages,
            items_found: 0, // Conservative: no false positives until Phase 8
            summary: if active_count == 0 {
                "No active gateway conversations.".into()
            } else {
                format!("{} active gateway conversation(s)", active_count)
            },
            details: vec![],
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

    fn make_goal_run(
        id: &str,
        title: &str,
        status: GoalRunStatus,
        updated_at: u64,
    ) -> GoalRun {
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
            memory: RwLock::new(AgentMemory::default()),
            operator_model: RwLock::new(OperatorModel::default()),
            anticipatory: RwLock::new(AnticipatoryRuntime::default()),
            collaboration: RwLock::new(HashMap::new()),
            data_dir,
            gateway_process: Mutex::new(None),
            gateway_state: Mutex::new(None),
            gateway_discord_channels: RwLock::new(Vec::new()),
            gateway_slack_channels: RwLock::new(Vec::new()),
            gateway_threads: RwLock::new(gateway_threads),
            gateway_seen_ids: Mutex::new(Vec::new()),
            gateway_inflight_channels: Mutex::new(HashSet::new()),
            external_runners: RwLock::new(HashMap::new()),
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
        assert!(result.summary.contains("No active gateway"));
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
