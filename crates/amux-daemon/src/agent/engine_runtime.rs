//! Engine runtime — stream cancellation, repo watchers, and memory cache.

use super::*;

impl AgentEngine {
    pub(super) async fn ensure_subagent_runtime(&self, task: &AgentTask, thread_id: Option<&str>) {
        if !should_track_subagent(task) {
            return;
        }

        let now = now_millis();
        let inserted = {
            let mut runtime = self.subagent_runtime.write().await;
            if runtime.contains_key(&task.id) {
                false
            } else {
                runtime.insert(
                    task.id.clone(),
                    SubagentRuntimeStats {
                        task_id: task.id.clone(),
                        parent_task_id: task.parent_task_id.clone(),
                        thread_id: thread_id
                            .map(ToOwned::to_owned)
                            .or_else(|| task.thread_id.clone()),
                        started_at: task.started_at.unwrap_or(task.created_at),
                        created_at: task.created_at,
                        max_duration_secs: task.max_duration_secs,
                        context_budget_tokens: task.context_budget_tokens,
                        last_tool_call_at: None,
                        last_progress_at: None,
                        tool_calls_total: 0,
                        tool_calls_succeeded: 0,
                        tool_calls_failed: 0,
                        consecutive_errors: 0,
                        recent_tool_names: VecDeque::new(),
                        tokens_consumed: 0,
                        context_utilization_pct: 0,
                        health_state: SubagentHealthState::Healthy,
                        updated_at: now,
                    },
                );
                true
            }
        };
        if inserted {
            self.persist_subagent_runtime_metrics(&task.id).await;
        }
    }

    pub(super) async fn record_subagent_tool_result(
        &self,
        task: &AgentTask,
        thread_id: &str,
        tool_name: &str,
        is_error: bool,
        current_tokens: u32,
    ) {
        if !should_track_subagent(task) {
            return;
        }

        self.ensure_subagent_runtime(task, Some(thread_id)).await;

        let now = now_millis();
        let mut runtime = self.subagent_runtime.write().await;
        let Some(entry) = runtime.get_mut(&task.id) else {
            return;
        };

        entry.thread_id = Some(thread_id.to_string());
        entry.updated_at = now;
        entry.tokens_consumed = current_tokens;
        entry.context_utilization_pct = entry
            .context_budget_tokens
            .map(|budget| {
                if budget == 0 {
                    0
                } else {
                    ((current_tokens as u64 * 100) / budget as u64).min(100) as u32
                }
            })
            .unwrap_or(0);
        entry.last_tool_call_at = Some(now);
        entry.tool_calls_total = entry.tool_calls_total.saturating_add(1);
        entry.recent_tool_names.push_back(tool_name.to_string());
        while entry.recent_tool_names.len() > 8 {
            entry.recent_tool_names.pop_front();
        }

        if is_error {
            entry.tool_calls_failed = entry.tool_calls_failed.saturating_add(1);
            entry.consecutive_errors = entry.consecutive_errors.saturating_add(1);
        } else {
            entry.tool_calls_succeeded = entry.tool_calls_succeeded.saturating_add(1);
            entry.consecutive_errors = 0;
            entry.last_progress_at = Some(now);
        }
    }

    pub(super) async fn update_subagent_health(
        &self,
        task_id: &str,
        health_state: SubagentHealthState,
    ) {
        let mut runtime = self.subagent_runtime.write().await;
        if let Some(entry) = runtime.get_mut(task_id) {
            entry.health_state = health_state;
            entry.updated_at = now_millis();
        }
    }

    pub(super) async fn subagent_snapshot(
        &self,
        task: &AgentTask,
    ) -> Option<crate::agent::subagent::supervisor::SubagentSnapshot> {
        if !should_track_subagent(task) {
            return None;
        }

        self.ensure_subagent_runtime(task, task.thread_id.as_deref())
            .await;
        let runtime = self.subagent_runtime.read().await;
        let stats = runtime.get(&task.id)?;
        Some(crate::agent::subagent::supervisor::SubagentSnapshot {
            task_id: stats.task_id.clone(),
            last_tool_call_at: stats.last_tool_call_at,
            tool_calls_total: stats.tool_calls_total,
            tool_calls_failed: stats.tool_calls_failed,
            consecutive_errors: stats.consecutive_errors,
            recent_tool_names: stats.recent_tool_names.iter().cloned().collect(),
            context_utilization_pct: stats.context_utilization_pct,
            started_at: stats.started_at,
            max_duration_secs: stats.max_duration_secs,
        })
    }

    pub(super) async fn persist_subagent_runtime_metrics(&self, task_id: &str) {
        let stats = {
            let runtime = self.subagent_runtime.read().await;
            runtime.get(task_id).cloned()
        };
        let Some(stats) = stats else {
            return;
        };

        let elapsed_secs =
            now_millis().saturating_sub(stats.started_at).max(1_000) as f64 / 1_000.0;
        let progress_rate = stats.tool_calls_succeeded as f64 / elapsed_secs;
        let failure_ratio = if stats.tool_calls_total == 0 {
            0.0
        } else {
            stats.tool_calls_failed as f64 / stats.tool_calls_total as f64
        };
        let stuck_score = ((stats.context_utilization_pct as f64 / 100.0) * 0.5)
            + failure_ratio * 0.3
            + ((stats.consecutive_errors.min(5) as f64) / 5.0) * 0.2;

        if let Err(e) = self.history.upsert_subagent_metrics(
            &stats.task_id,
            stats.parent_task_id.as_deref(),
            stats.thread_id.as_deref(),
            stats.tool_calls_total as i64,
            stats.tool_calls_succeeded as i64,
            stats.tool_calls_failed as i64,
            stats.tokens_consumed as i64,
            stats.context_budget_tokens.map(|v| v as i64),
            progress_rate,
            stats.last_progress_at,
            stuck_score,
            subagent_health_label(stats.health_state),
            stats.created_at,
            stats.updated_at,
        ).await {
            tracing::warn!(task_id = %stats.task_id, "failed to persist subagent metrics: {e}");
        }
    }

    pub async fn health_status_snapshot(&self) -> serde_json::Value {
        let tasks = self.tasks.lock().await;
        let goal_runs = self.goal_runs.lock().await;
        let active_tasks = tasks
            .iter()
            .filter(|task| {
                matches!(
                    task.status,
                    TaskStatus::Queued
                        | TaskStatus::InProgress
                        | TaskStatus::Blocked
                        | TaskStatus::AwaitingApproval
                        | TaskStatus::FailedAnalyzing
                )
            })
            .count();
        let awaiting_approval_tasks = tasks
            .iter()
            .filter(|task| task.status == TaskStatus::AwaitingApproval)
            .count();
        let active_goal_runs = goal_runs
            .iter()
            .filter(|goal_run| {
                matches!(
                    goal_run.status,
                    GoalRunStatus::Queued
                        | GoalRunStatus::Planning
                        | GoalRunStatus::Running
                        | GoalRunStatus::AwaitingApproval
                        | GoalRunStatus::Paused
                )
            })
            .count();
        drop(goal_runs);
        drop(tasks);

        let latest = self
            .history
            .list_health_log(1)
            .await
            .ok()
            .and_then(|items| items.into_iter().next());

        serde_json::json!({
            "state": latest.as_ref().map(|entry| entry.3.clone()).unwrap_or_else(|| "healthy".to_string()),
            "uptime_secs": now_millis().saturating_sub(self.started_at_ms) / 1000,
            "active_goal_runs": active_goal_runs,
            "active_tasks": active_tasks,
            "awaiting_approval_tasks": awaiting_approval_tasks,
            "latest_health_event_at": latest.as_ref().map(|entry| entry.6),
            "latest_health_entity_type": latest.as_ref().map(|entry| entry.1.clone()),
            "latest_health_entity_id": latest.as_ref().map(|entry| entry.2.clone()),
        })
    }

    pub async fn health_log_entries(&self, limit: u32) -> Result<Vec<serde_json::Value>> {
        let rows = self.history.list_health_log(limit).await?;
        Ok(rows
            .into_iter()
            .map(
                |(id, entity_type, entity_id, health_state, indicators_json, intervention, created_at)| {
                    serde_json::json!({
                        "id": id,
                        "entity_type": entity_type,
                        "entity_id": entity_id,
                        "health_state": health_state,
                        "indicators": indicators_json.and_then(|json| serde_json::from_str::<serde_json::Value>(&json).ok()),
                        "intervention": intervention,
                        "created_at": created_at,
                    })
                },
            )
            .collect())
    }

    pub(super) async fn begin_stream_cancellation(
        &self,
        thread_id: &str,
    ) -> (u64, CancellationToken) {
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

    pub(super) async fn finish_stream_cancellation(&self, thread_id: &str, generation: u64) {
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

    pub(super) async fn ensure_repo_watcher(&self, thread_id: &str, repo_root: &str) {
        let normalized_root = std::fs::canonicalize(repo_root)
            .unwrap_or_else(|_| std::path::PathBuf::from(repo_root))
            .to_string_lossy()
            .to_string();

        let mut watchers = self.repo_watchers.lock().await;
        if watchers
            .get(thread_id)
            .map(|entry| entry.repo_root == normalized_root)
            .unwrap_or(false)
        {
            return;
        }

        watchers.remove(thread_id);

        let refresh_tx = self.watcher_refresh_tx.clone();
        let callback_thread_id = thread_id.to_string();
        let callback_repo_root = normalized_root.clone();
        let mut watcher =
            match notify::recommended_watcher(move |result: notify::Result<Event>| match result {
                Ok(event) => {
                    if file_watch_event_is_relevant(&event) {
                        let _ = refresh_tx.send(callback_thread_id.clone());
                    }
                }
                Err(error) => {
                    tracing::warn!(
                        thread_id = %callback_thread_id,
                        repo_root = %callback_repo_root,
                        "filesystem watcher error: {error}"
                    );
                }
            }) {
                Ok(watcher) => watcher,
                Err(error) => {
                    tracing::warn!(
                        thread_id = %thread_id,
                        repo_root = %normalized_root,
                        "failed to create filesystem watcher: {error}"
                    );
                    return;
                }
            };

        if let Err(error) = watcher.watch(
            std::path::Path::new(&normalized_root),
            RecursiveMode::Recursive,
        ) {
            tracing::warn!(
                thread_id = %thread_id,
                repo_root = %normalized_root,
                "failed to watch repo root: {error}"
            );
            return;
        }

        tracing::info!(
            thread_id = %thread_id,
            repo_root = %normalized_root,
            "filesystem watcher attached"
        );
        watchers.insert(
            thread_id.to_string(),
            ThreadRepoWatcher {
                repo_root: normalized_root,
                watcher,
            },
        );
    }

    pub(super) async fn remove_repo_watcher(&self, thread_id: &str) {
        let removed = self.repo_watchers.lock().await.remove(thread_id);
        if let Some(entry) = removed {
            tracing::info!(
                thread_id = %thread_id,
                repo_root = %entry.repo_root,
                "filesystem watcher removed"
            );
            drop(entry.watcher);
        }
    }

    pub(super) async fn refresh_memory_cache(&self) {
        if let Err(e) = ensure_memory_files(&self.data_dir).await {
            tracing::warn!("failed to ensure persistent memory files: {e}");
        }
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

    pub(super) async fn onecontext_bootstrap_for_new_thread(
        &self,
        initial_message: &str,
    ) -> Option<String> {
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
}

fn should_track_subagent(task: &AgentTask) -> bool {
    task.source == "subagent"
        || task.parent_task_id.is_some()
        || task.supervisor_config.is_some()
        || task.sub_agent_def_id.is_some()
}

fn subagent_health_label(state: SubagentHealthState) -> &'static str {
    match state {
        SubagentHealthState::Healthy => "healthy",
        SubagentHealthState::Degraded => "degraded",
        SubagentHealthState::Stuck => "stuck",
        SubagentHealthState::Crashed => "crashed",
    }
}
