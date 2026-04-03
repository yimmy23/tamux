use super::*;

impl AgentEngine {
    pub async fn run_loop(self: Arc<Self>, mut shutdown: tokio::sync::watch::Receiver<bool>) {
        let config = self.config.read().await.clone();

        let task_interval = std::time::Duration::from_secs(config.task_poll_interval_secs);
        let mut watcher_refresh_rx = self.watcher_refresh_rx.lock().await.take();

        let mut task_tick = tokio::time::interval(task_interval);
        let mut watcher_tick =
            tokio::time::interval(std::time::Duration::from_millis(FILE_WATCH_TICK_MS));
        let mut supervisor_tick = tokio::time::interval(std::time::Duration::from_secs(30));
        let mut anticipatory_tick =
            tokio::time::interval(std::time::Duration::from_secs(ANTICIPATORY_TICK_SECS));
        let mut gateway_event_tick = tokio::time::interval(std::time::Duration::from_millis(
            GATEWAY_EVENT_DRAIN_INTERVAL_MS,
        ));
        let mut pending_watcher_refreshes: HashMap<String, Instant> = HashMap::new();

        let heartbeat_cron_expr = super::heartbeat::resolve_cron_from_config(&config);
        let mut heartbeat_cron: croner::Cron = heartbeat_cron_expr
            .parse()
            .unwrap_or_else(|_| "*/15 * * * *".parse().unwrap());
        let mut next_heartbeat = {
            let now_local = chrono::Local::now();
            heartbeat_cron
                .find_next_occurrence(&now_local, false)
                .map(|dt| {
                    let dur = (dt - now_local)
                        .to_std()
                        .unwrap_or(std::time::Duration::from_secs(900));
                    tokio::time::Instant::now() + dur
                })
                .unwrap_or_else(|_| {
                    tokio::time::Instant::now() + std::time::Duration::from_secs(900)
                })
        };

        let mut heartbeat_cycle_count: u64 = 0;

        tracing::info!(
            task_poll_secs = config.task_poll_interval_secs,
            heartbeat_cron = %heartbeat_cron_expr,
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
                _ = gateway_event_tick.tick() => {
                    self.process_gateway_messages().await;
                }
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
                            if let Err(e) = self.run_structured_heartbeat_adaptive(heartbeat_cycle_count).await {
                                tracing::error!("agent heartbeat error: {e}");
                            }
                        } else {
                            let skip_factor = config_snap.low_activity_frequency_factor;
                            if skip_factor == 0 || heartbeat_cycle_count % skip_factor == 0 {
                                if let Err(e) = self.run_structured_heartbeat_adaptive(heartbeat_cycle_count).await {
                                    tracing::error!("agent heartbeat error: {e}");
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

                    if let Err(e) = self.check_tier_change().await {
                        tracing::warn!(error = %e, "tier change check failed");
                    }

                    {
                        let session_count = self.operator_model.read().await.session_count;
                        let mut queue = self.disclosure_queue.write().await;
                        if let Err(e) = self.concierge.deliver_next_disclosure(&mut queue, session_count).await {
                            tracing::warn!(error = %e, "feature disclosure delivery failed");
                        }
                    }

                    let now_local = chrono::Local::now();
                    next_heartbeat = heartbeat_cron
                        .find_next_occurrence(&now_local, false)
                        .map(|dt| {
                            let dur = (dt - now_local)
                                .to_std()
                                .unwrap_or(std::time::Duration::from_secs(900));
                            tokio::time::Instant::now() + dur
                        })
                        .unwrap_or_else(|_| {
                            tokio::time::Instant::now() + std::time::Duration::from_secs(900)
                        });
                }
                _ = self.config_notify.notified() => {
                    let new_cron_expr = self.resolve_heartbeat_cron().await;
                    if let Ok(new_cron) = new_cron_expr.parse::<croner::Cron>() {
                        heartbeat_cron = new_cron;
                        let now_local = chrono::Local::now();
                        next_heartbeat = heartbeat_cron
                            .find_next_occurrence(&now_local, false)
                            .map(|dt| {
                                let dur = (dt - now_local)
                                    .to_std()
                                    .unwrap_or(std::time::Duration::from_secs(900));
                                tokio::time::Instant::now() + dur
                            })
                            .unwrap_or_else(|_| {
                                tokio::time::Instant::now() + std::time::Duration::from_secs(900)
                            });
                        tracing::info!(cron = %new_cron_expr, "heartbeat schedule updated");
                    }
                }
                _ = anticipatory_tick.tick() => {
                    self.run_anticipatory_tick().await;
                }
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
                    }
                }
                _ = supervisor_tick.tick() => {
                    if let Err(error) = self.supervise_gateway_runtime().await {
                        tracing::warn!(error = %error, "gateway supervision tick failed");
                    }
                    if let Err(error) = self.supervise_stalled_turns().await {
                        tracing::warn!(error = %error, "stalled-turn supervision tick failed");
                    }

                    let supervised: Vec<_> = {
                        let tasks = self.tasks.lock().await;
                        tasks.iter()
                            .filter(|t| t.status == TaskStatus::InProgress && t.supervisor_config.is_some())
                            .cloned()
                            .collect()
                    };

                    let now_secs = now_millis() / 1000;
                    for task in supervised {
                        self.ensure_subagent_runtime(&task, task.thread_id.as_deref()).await;
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
                            if let Err(e) = self.history.insert_health_log(
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
                            ).await {
                                tracing::warn!(task_id = %task.id, "failed to persist health log: {e}");
                            }
                            let _ = self.event_tx.send(AgentEvent::SubagentHealthChange {
                                task_id: task.id.clone(),
                                previous_state,
                                new_state,
                                reason: action.as_ref().and_then(|value| value.reason),
                                intervention: action.as_ref().map(|value| value.action),
                            });
                        }
                        self.persist_subagent_runtime_metrics(&task.id).await;
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

        self.maybe_spawn_gateway().await;
        Ok(())
    }

    #[cfg(test)]
    pub(crate) async fn gateway_restart_attempts(&self) -> u32 {
        gateway_runtime_control().lock().await.restart_attempts
    }

    #[cfg(test)]
    pub(crate) async fn gateway_restart_not_before_ms(&self) -> Option<u64> {
        gateway_runtime_control().lock().await.restart_not_before_ms
    }
}
