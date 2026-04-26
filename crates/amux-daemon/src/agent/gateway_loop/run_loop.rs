use super::*;

const BACKGROUND_SHUTDOWN_JOIN_TIMEOUT_SECS: u64 = 2;
const SUPERVISOR_TICK_SECS: u64 = 30;

impl AgentEngine {
    pub async fn run_loop(self: Arc<Self>, mut shutdown: tokio::sync::watch::Receiver<bool>) {
        let config = self.config.read().await.clone();
        let heartbeat_cron = super::heartbeat::resolve_cron_from_config(&config);

        tracing::info!(
            task_poll_secs = config.task_poll_interval_secs,
            heartbeat_cron = %heartbeat_cron,
            "agent background runtime started"
        );

        if let Err(error) = self.ensure_default_event_triggers().await {
            tracing::warn!(error = %error, "failed to seed default event triggers");
        }

        let workers = vec![
            tokio::spawn({
                let engine = self.clone();
                let rx = shutdown.clone();
                async move { engine.run_task_dispatch_loop(rx).await }
            }),
            tokio::spawn({
                let engine = self.clone();
                let rx = shutdown.clone();
                async move { engine.run_gateway_event_drain_loop(rx).await }
            }),
            tokio::spawn({
                let engine = self.clone();
                let rx = shutdown.clone();
                async move { engine.run_heartbeat_loop(rx).await }
            }),
            tokio::spawn({
                let engine = self.clone();
                let rx = shutdown.clone();
                async move { engine.run_anticipatory_loop(rx).await }
            }),
            tokio::spawn({
                let engine = self.clone();
                let rx = shutdown.clone();
                async move { engine.run_watcher_refresh_loop(rx).await }
            }),
            tokio::spawn({
                let engine = self.clone();
                let rx = shutdown.clone();
                async move { engine.run_gateway_supervision_loop(rx).await }
            }),
            tokio::spawn({
                let engine = self.clone();
                let rx = shutdown.clone();
                async move { engine.run_stalled_turn_supervision_loop(rx).await }
            }),
            tokio::spawn({
                let engine = self.clone();
                let rx = shutdown.clone();
                async move { engine.run_quiet_goal_supervision_loop(rx).await }
            }),
            tokio::spawn({
                let engine = self.clone();
                let rx = shutdown.clone();
                async move { engine.run_webhook_listener(rx).await }
            }),
            tokio::spawn({
                let engine = self.clone();
                let rx = shutdown.clone();
                async move { engine.run_subagent_supervision_loop(rx).await }
            }),
        ];

        let _ = shutdown.changed().await;

        tracing::info!("agent background runtime shutting down");
        self.stop_gateway().await;
        self.stop_external_agents().await;

        for worker in workers {
            let _ = tokio::time::timeout(
                std::time::Duration::from_secs(BACKGROUND_SHUTDOWN_JOIN_TIMEOUT_SECS),
                worker,
            )
            .await;
        }
    }

    async fn run_task_dispatch_loop(
        self: Arc<Self>,
        mut shutdown: tokio::sync::watch::Receiver<bool>,
    ) {
        let config = self.config.read().await.clone();
        let mut task_tick = tokio::time::interval(std::time::Duration::from_secs(
            config.task_poll_interval_secs,
        ));

        loop {
            tokio::select! {
                _ = task_tick.tick() => {
                    if let Err(error) = self.materialize_due_routines().await {
                        tracing::error!("agent routine materialization error: {error}");
                    }
                    self.clone().dispatch_goal_runs().await;
                    if let Err(error) = self.clone().dispatch_ready_tasks().await {
                        tracing::error!("agent task error: {error}");
                    }
                }
                _ = shutdown.changed() => break,
            }
        }
    }

    async fn run_gateway_event_drain_loop(
        self: Arc<Self>,
        mut shutdown: tokio::sync::watch::Receiver<bool>,
    ) {
        let mut gateway_event_tick = tokio::time::interval(std::time::Duration::from_millis(
            GATEWAY_EVENT_DRAIN_INTERVAL_MS,
        ));

        loop {
            tokio::select! {
                _ = gateway_event_tick.tick() => {
                    Box::pin(self.process_gateway_messages()).await;
                }
                _ = shutdown.changed() => break,
            }
        }
    }

    async fn run_heartbeat_loop(self: Arc<Self>, mut shutdown: tokio::sync::watch::Receiver<bool>) {
        let config = self.config.read().await.clone();
        let heartbeat_cron_expr = super::heartbeat::resolve_cron_from_config(&config);
        let mut heartbeat_cron: croner::Cron = heartbeat_cron_expr
            .parse()
            .unwrap_or_else(|_| "*/15 * * * *".parse().unwrap());
        let mut next_heartbeat = next_heartbeat_deadline(&heartbeat_cron);
        let mut heartbeat_cycle_count: u64 = 0;

        loop {
            tokio::select! {
                _ = tokio::time::sleep_until(next_heartbeat) => {
                    heartbeat_cycle_count += 1;
                    let is_quiet = self.is_quiet_hours().await;
                    if !is_quiet {
                        let config_snap = self.config.read().await.clone();
                        let model = self.operator_model.read().await;
                        let current_hour = chrono::Utc::now().hour() as u8;
                        let session_count = model.session_rhythm.session_count;
                        let in_peak = if session_count < 5 {
                            true
                        } else {
                            is_peak_activity_hour(
                                current_hour,
                                &model.session_rhythm.peak_activity_hours_utc,
                                &model.session_rhythm.smoothed_activity_histogram,
                                config_snap.ema_activity_threshold,
                            )
                        };
                        drop(model);

                        if in_peak {
                            if let Err(error) = self.run_structured_heartbeat_adaptive(heartbeat_cycle_count).await {
                                tracing::error!("agent heartbeat error: {error}");
                            }
                        } else {
                            let skip_factor = config_snap.low_activity_frequency_factor;
                            if skip_factor == 0 || heartbeat_cycle_count % skip_factor == 0 {
                                if let Err(error) = self.run_structured_heartbeat_adaptive(heartbeat_cycle_count).await {
                                    tracing::error!("agent heartbeat error: {error}");
                                }
                            } else {
                                tracing::debug!(
                                    cycle = heartbeat_cycle_count,
                                    skip_factor = skip_factor,
                                    "heartbeat skipped (low-activity period)"
                                );
                            }
                        }
                    } else {
                        tracing::debug!("heartbeat suppressed (quiet hours/DND)");
                    }

                    if let Err(error) = self.check_tier_change().await {
                        tracing::warn!(error = %error, "tier change check failed");
                    }

                    {
                        let session_count = self.operator_model.read().await.session_count;
                        let mut queue = self.disclosure_queue.write().await;
                        if let Err(error) = self.concierge.deliver_next_disclosure(&mut queue, session_count).await {
                            tracing::warn!(error = %error, "feature disclosure delivery failed");
                        }
                    }

                    next_heartbeat = next_heartbeat_deadline(&heartbeat_cron);
                }
                _ = self.config_notify.notified() => {
                    let new_cron_expr = self.resolve_heartbeat_cron().await;
                    if let Ok(new_cron) = new_cron_expr.parse::<croner::Cron>() {
                        heartbeat_cron = new_cron;
                        next_heartbeat = next_heartbeat_deadline(&heartbeat_cron);
                        tracing::info!(cron = %new_cron_expr, "heartbeat schedule updated");
                    }
                }
                _ = shutdown.changed() => break,
            }
        }
    }

    async fn run_anticipatory_loop(
        self: Arc<Self>,
        mut shutdown: tokio::sync::watch::Receiver<bool>,
    ) {
        let mut anticipatory_tick =
            tokio::time::interval(std::time::Duration::from_secs(ANTICIPATORY_TICK_SECS));

        loop {
            tokio::select! {
                _ = anticipatory_tick.tick() => {
                    self.run_anticipatory_tick().await;
                }
                _ = shutdown.changed() => break,
            }
        }
    }

    async fn run_watcher_refresh_loop(
        self: Arc<Self>,
        mut shutdown: tokio::sync::watch::Receiver<bool>,
    ) {
        let mut watcher_refresh_rx = self.watcher_refresh_rx.lock().await.take();
        if watcher_refresh_rx.is_none() {
            return;
        }

        let mut watcher_tick =
            tokio::time::interval(std::time::Duration::from_millis(FILE_WATCH_TICK_MS));
        let mut pending_watcher_refreshes: HashMap<String, Instant> = HashMap::new();

        loop {
            tokio::select! {
                maybe_thread_id = async {
                    match watcher_refresh_rx.as_mut() {
                        Some(rx) => rx.recv().await,
                        None => std::future::pending::<Option<String>>().await,
                    }
                } => {
                    if let Some(thread_id) = maybe_thread_id {
                        pending_watcher_refreshes.insert(
                            thread_id,
                            Instant::now() + Duration::from_millis(FILE_WATCH_DEBOUNCE_MS),
                        );
                    }
                }
                _ = watcher_tick.tick() => {
                    if pending_watcher_refreshes.is_empty() {
                        continue;
                    }

                    let now = Instant::now();
                    let due_threads = pending_watcher_refreshes
                        .iter()
                        .filter_map(|(thread_id, deadline)| {
                            if *deadline <= now {
                                Some(thread_id.clone())
                            } else {
                                None
                            }
                        })
                        .collect::<Vec<_>>();

                    for thread_id in due_threads {
                        pending_watcher_refreshes.remove(&thread_id);
                        self.refresh_thread_repo_context(&thread_id).await;
                        let repo_root = self
                            .resolve_thread_repo_root(&thread_id)
                            .await
                            .map(|(repo_root, _, _, _)| repo_root)
                            .unwrap_or_default();
                        if let Err(error) = self
                            .maybe_fire_event_trigger(
                                "filesystem",
                                "file_changed",
                                Some("detected"),
                                Some(&thread_id),
                                serde_json::json!({
                                    "path": if repo_root.is_empty() { "." } else { repo_root.as_str() },
                                    "repo_root": repo_root,
                                    "source": "watcher_refresh",
                                }),
                            )
                            .await
                        {
                            tracing::warn!(thread_id = %thread_id, error = %error, "failed to fire filesystem event trigger");
                        }
                    }
                }
                _ = shutdown.changed() => break,
            }
        }
    }

    async fn run_gateway_supervision_loop(
        self: Arc<Self>,
        mut shutdown: tokio::sync::watch::Receiver<bool>,
    ) {
        let mut tick = tokio::time::interval(std::time::Duration::from_secs(SUPERVISOR_TICK_SECS));

        loop {
            tokio::select! {
                _ = tick.tick() => {
                    if let Err(error) = self.supervise_gateway_runtime().await {
                        tracing::warn!(error = %error, "gateway supervision tick failed");
                    }
                }
                _ = shutdown.changed() => break,
            }
        }
    }

    async fn run_stalled_turn_supervision_loop(
        self: Arc<Self>,
        mut shutdown: tokio::sync::watch::Receiver<bool>,
    ) {
        let mut tick = tokio::time::interval(std::time::Duration::from_secs(SUPERVISOR_TICK_SECS));

        loop {
            tokio::select! {
                _ = tick.tick() => {
                    if let Err(error) = self.supervise_stalled_turns().await {
                        tracing::warn!(error = %error, "stalled-turn supervision tick failed");
                    }
                }
                _ = shutdown.changed() => break,
            }
        }
    }

    async fn run_subagent_supervision_loop(
        self: Arc<Self>,
        mut shutdown: tokio::sync::watch::Receiver<bool>,
    ) {
        let mut tick = tokio::time::interval(std::time::Duration::from_secs(SUPERVISOR_TICK_SECS));

        loop {
            tokio::select! {
                _ = tick.tick() => {
                    self.run_subagent_supervision_tick().await;
                }
                _ = shutdown.changed() => break,
            }
        }
    }

    async fn run_quiet_goal_supervision_loop(
        self: Arc<Self>,
        mut shutdown: tokio::sync::watch::Receiver<bool>,
    ) {
        let mut tick = tokio::time::interval(std::time::Duration::from_secs(SUPERVISOR_TICK_SECS));

        loop {
            tokio::select! {
                _ = tick.tick() => {
                    if let Err(error) = self.supervise_quiet_goal_runs().await {
                        tracing::warn!(error = %error, "quiet-goal supervision tick failed");
                    }
                }
                _ = shutdown.changed() => break,
            }
        }
    }

    async fn run_subagent_supervision_tick(&self) {
        let supervised: Vec<_> = {
            let tasks = self.tasks.lock().await;
            tasks
                .iter()
                .filter(|t| t.status == TaskStatus::InProgress && t.supervisor_config.is_some())
                .cloned()
                .collect()
        };

        let now_secs = now_millis() / 1000;
        for task in supervised {
            self.ensure_subagent_runtime(&task, task.thread_id.as_deref())
                .await;
            let Some(snapshot) = self.subagent_snapshot(&task).await else {
                continue;
            };
            let action = crate::agent::subagent::supervisor::check_health(
                &snapshot,
                task.supervisor_config
                    .as_ref()
                    .expect("supervised task must have config"),
                now_secs,
            );
            let new_state = action
                .as_ref()
                .map(|value| value.health_state)
                .unwrap_or(SubagentHealthState::Healthy);
            let previous_state = {
                let runtime = self.subagent_runtime.read().await;
                runtime
                    .get(&task.id)
                    .map(|entry| entry.health_state)
                    .unwrap_or(SubagentHealthState::Healthy)
            };

            if previous_state != new_state {
                self.update_subagent_health(&task.id, new_state).await;
                let runtime = {
                    let runtime = self.subagent_runtime.read().await;
                    runtime.get(&task.id).cloned()
                };
                let indicators_json = runtime.as_ref().map(|entry| {
                    serde_json::json!({
                        "last_progress_at": entry.last_progress_at,
                        "tool_call_frequency": if now_secs > entry.started_at / 1000 {
                            entry.tool_calls_total as f64 / ((now_secs - entry.started_at / 1000) as f64 / 60.0).max(1.0)
                        } else {
                            0.0
                        },
                        "error_rate": if entry.tool_calls_total == 0 {
                            0.0
                        } else {
                            entry.tool_calls_failed as f64 / entry.tool_calls_total as f64
                        },
                        "context_growth_rate": 0.0,
                        "context_utilization_pct": entry.context_utilization_pct,
                        "consecutive_errors": entry.consecutive_errors,
                        "total_tool_calls": entry.tool_calls_total,
                        "successful_tool_calls": entry.tool_calls_succeeded,
                    })
                    .to_string()
                });
                if let Err(error) = self
                    .history
                    .insert_health_log(
                        &format!("health_{}", Uuid::new_v4()),
                        "task",
                        &task.id,
                        match new_state {
                            SubagentHealthState::Healthy => "healthy",
                            SubagentHealthState::Degraded => "degraded",
                            SubagentHealthState::Stuck => "stuck",
                            SubagentHealthState::Crashed => "crashed",
                        },
                        indicators_json.as_deref(),
                        action
                            .as_ref()
                            .map(|value| format!("{:?}", value.action))
                            .as_deref(),
                        now_millis(),
                    )
                    .await
                {
                    tracing::warn!(task_id = %task.id, "failed to persist health log: {error}");
                }
                let _ = self.event_tx.send(AgentEvent::SubagentHealthChange {
                    task_id: task.id.clone(),
                    previous_state,
                    new_state,
                    reason: action.as_ref().and_then(|value| value.reason),
                    intervention: action.as_ref().map(|value| value.action),
                });
                let _ = self
                    .maybe_fire_event_trigger(
                        "health",
                        "subagent_health",
                        Some(match new_state {
                            SubagentHealthState::Healthy => "healthy",
                            SubagentHealthState::Degraded => "degraded",
                            SubagentHealthState::Stuck => "stuck",
                            SubagentHealthState::Crashed => "crashed",
                        }),
                        task.thread_id.as_deref(),
                        serde_json::json!({
                            "task_id": task.id,
                            "previous_state": format!("{:?}", previous_state).to_ascii_lowercase(),
                            "new_state": format!("{:?}", new_state).to_ascii_lowercase(),
                            "reason": action
                                .as_ref()
                                .and_then(|value| value.reason)
                                .map(|reason| format!("{:?}", reason).to_ascii_lowercase())
                                .unwrap_or_else(|| "unknown".to_string()),
                        }),
                    )
                    .await;
            }
            self.persist_subagent_runtime_metrics(&task.id).await;
        }
    }

    pub(crate) async fn supervise_gateway_runtime(&self) -> Result<()> {
        let config = self.config.read().await.clone();
        let gw = &config.gateway;
        if !gw.enabled {
            return Ok(());
        }

        let restart_deadline = gateway_runtime_control().lock().await.restart_not_before_ms;
        if let Some(deadline_ms) = restart_deadline {
            if now_millis() < deadline_ms {
                return Ok(());
            }
        }

        let child_exited = {
            let mut proc = self.gateway_process.lock().await;
            if let Some(child) = proc.as_mut() {
                match child.try_wait() {
                    Ok(Some(status)) => {
                        tracing::warn!(?status, "gateway child exited");
                        *proc = None;
                        true
                    }
                    Ok(None) => false,
                    Err(error) => {
                        tracing::warn!(error = %error, "gateway child status check failed");
                        *proc = None;
                        true
                    }
                }
            } else {
                false
            }
        };

        if child_exited {
            self.clear_gateway_ipc_sender().await;
            self.schedule_gateway_restart_backoff("gateway child exited")
                .await;
            return Ok(());
        }

        let process_running = self.gateway_process.lock().await.is_some();
        if process_running {
            return Ok(());
        }

        {
            let mut control = gateway_runtime_control().lock().await;
            control.restart_not_before_ms = None;
        }
        self.maybe_spawn_gateway().await;
        Ok(())
    }
}

fn next_heartbeat_deadline(heartbeat_cron: &croner::Cron) -> tokio::time::Instant {
    let now_local = chrono::Local::now();
    heartbeat_cron
        .find_next_occurrence(&now_local, false)
        .map(|dt| {
            let dur = (dt - now_local)
                .to_std()
                .unwrap_or(std::time::Duration::from_secs(900));
            tokio::time::Instant::now() + dur
        })
        .unwrap_or_else(|_| tokio::time::Instant::now() + std::time::Duration::from_secs(900))
}
