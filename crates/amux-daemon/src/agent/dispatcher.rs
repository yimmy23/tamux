//! Task and goal dispatching — background execution scheduling.

use super::*;

impl AgentEngine {
    pub(super) async fn dispatch_goal_runs(self: Arc<Self>) {
        let goal_run_ids = {
            let goal_runs = self.goal_runs.lock().await;
            goal_runs
                .iter()
                .filter(|goal_run| {
                    !matches!(
                        goal_run.status,
                        GoalRunStatus::AwaitingApproval
                            | GoalRunStatus::Planning
                            | GoalRunStatus::Paused
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
                        .fail_goal_run(&goal_run_id, &error.to_string(), "goal-run", None)
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

        if matches!(
            goal_run.status,
            GoalRunStatus::AwaitingApproval | GoalRunStatus::Planning
        ) {
            return Ok(());
        }

        if goal_run.status == GoalRunStatus::Queued && goal_run.steps.is_empty() {
            self.plan_goal_run(goal_run_id).await?;
            return Ok(());
        }

        if goal_run.current_step_index >= goal_run.steps.len()
            && !self.goal_run_has_active_tasks(goal_run_id).await
        {
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
            TaskStatus::Completed | TaskStatus::BudgetExceeded => {
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

    pub(super) async fn dispatch_ready_tasks(self: Arc<Self>) -> Result<()> {
        let now = now_millis();
        let sessions = self.session_manager.list().await;
        let config = self.config.read().await.clone();
        let goal_run_statuses = {
            let goal_runs = self.goal_runs.lock().await;
            goal_runs
                .iter()
                .map(|goal_run| (goal_run.id.clone(), goal_run.status))
                .collect::<HashMap<_, _>>()
        };
        let (changed_before_start, dispatched_tasks) = {
            let mut tasks = self.tasks.lock().await;
            let changed_before_start =
                refresh_task_queue_state(&mut tasks, now, &sessions, &config);
            let next_dispatches =
                select_ready_task_indices(&tasks, &sessions, &goal_run_statuses, &config);
            if next_dispatches.is_empty() {
                drop(tasks);
                if !changed_before_start.is_empty() {
                    self.persist_tasks().await;
                    for task in &changed_before_start {
                        if let Some(goal_run_id) = task.goal_run_id.as_deref() {
                            self.sync_goal_run_with_task(goal_run_id, task).await;
                        }
                    }
                    for task in changed_before_start {
                        self.emit_task_update(&task, Some(status_message(&task).into()));
                    }
                }
                return Ok(());
            }

            let mut dispatched_tasks = Vec::with_capacity(next_dispatches.len());
            for (index, lane_id) in next_dispatches {
                let task = &mut tasks[index];
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
        for changed in &changed_before_start {
            if let Some(goal_run_id) = changed.goal_run_id.as_deref() {
                self.sync_goal_run_with_task(goal_run_id, changed).await;
            }
        }
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
        let mut prompt = build_task_prompt(&task);
        task_prompt::append_effective_sub_agent_registry(self, &mut prompt).await;
        let weles_sender_scope = if task.sub_agent_def_id.as_deref()
            == Some(crate::agent::agent_identity::WELES_BUILTIN_SUBAGENT_ID)
        {
            let tasks = self.tasks.lock().await;
            task.parent_task_id
                .as_deref()
                .and_then(|parent_task_id| tasks.iter().find(|entry| entry.id == parent_task_id))
                .map(|parent| crate::agent::agent_identity::agent_scope_id_for_task(Some(parent)))
                .unwrap_or_else(|| crate::agent::agent_identity::MAIN_AGENT_ID.to_string())
        } else {
            String::new()
        };
        let outcome = if task.sub_agent_def_id.as_deref()
            == Some(crate::agent::agent_identity::WELES_BUILTIN_SUBAGENT_ID)
        {
            self.send_internal_task_message(
                &weles_sender_scope,
                crate::agent::agent_identity::WELES_AGENT_ID,
                &task.id,
                task.session_id.as_deref(),
                Some(task.runtime.as_str()),
                &prompt,
            )
            .await
        } else {
            self.send_task_message(
                &task.id,
                task.thread_id.as_deref(),
                task.session_id.as_deref(),
                Some(task.runtime.as_str()),
                &prompt,
            )
            .await
        };
        match outcome {
            Ok(outcome) if outcome.interrupted_for_approval => Ok(()),
            Ok(outcome) => {
                let now = now_millis();
                let updated = {
                    let mut tasks = self.tasks.lock().await;
                    let active_child_ids = tasks
                        .iter()
                        .filter(|entry| {
                            entry.source == "subagent"
                                && entry.parent_task_id.as_deref() == Some(task.id.as_str())
                                && !is_task_terminal_status(entry.status)
                        })
                        .map(|entry| entry.id.clone())
                        .collect::<Vec<_>>();
                    if let Some(current) = tasks.iter_mut().find(|entry| entry.id == task.id) {
                        let waiting_for_subagents = !active_child_ids.is_empty();
                        current.status = if outcome.terminated_for_budget {
                            TaskStatus::BudgetExceeded
                        } else if waiting_for_subagents {
                            TaskStatus::Blocked
                        } else {
                            TaskStatus::Completed
                        };
                        current.progress = if waiting_for_subagents {
                            current.progress.max(90)
                        } else {
                            100
                        };
                        current.completed_at = if waiting_for_subagents {
                            None
                        } else {
                            Some(now)
                        };
                        current.thread_id = Some(outcome.thread_id);
                        current.lane_id = None;
                        current.blocked_reason = if waiting_for_subagents {
                            Some(format!(
                                "waiting for subagents: {}",
                                active_child_ids.join(", ")
                            ))
                        } else {
                            None
                        };
                        current.awaiting_approval_id = None;
                        current.error = None;
                        current.last_error = None;
                        current.next_retry_at = None;
                        current.logs.push(make_task_log_entry(
                            current.retry_count,
                            TaskLogLevel::Info,
                            if waiting_for_subagents {
                                "subagent"
                            } else {
                                "execution"
                            },
                            if waiting_for_subagents {
                                "task waiting for spawned subagents to finish"
                            } else if current.retry_count > 0 {
                                "task self-healed and completed"
                            } else {
                                "task completed"
                            },
                            if waiting_for_subagents {
                                current.blocked_reason.clone()
                            } else {
                                None
                            },
                        ));
                        current.clone()
                    } else {
                        return Ok(());
                    }
                };
                self.persist_tasks().await;
                self.emit_task_update(
                    &updated,
                    Some(if updated.status == TaskStatus::Blocked {
                        format!(
                            "Waiting for {} subagent(s)",
                            updated
                                .blocked_reason
                                .as_deref()
                                .map(|reason| reason.split(',').count())
                                .unwrap_or(0)
                        )
                    } else if updated.status == TaskStatus::BudgetExceeded {
                        "Task stopped after exhausting its execution budget".into()
                    } else if updated.retry_count > 0 {
                        "Task self-healed and completed".into()
                    } else {
                        "Task completed".into()
                    }),
                );
                if matches!(
                    updated.status,
                    TaskStatus::Completed | TaskStatus::BudgetExceeded
                ) {
                    self.settle_task_skill_consultations(&updated, "success")
                        .await;
                    if updated.source == "divergent" {
                        if let Err(error) = self
                            .record_divergent_contribution_on_task_completion(&updated)
                            .await
                        {
                            tracing::warn!(
                                task_id = %updated.id,
                                "failed to process divergent contribution completion hook: {error}"
                            );
                        }
                    }
                    if updated.source == "handoff" {
                        if let Err(error) =
                            self.record_handoff_task_outcome(&updated, "success").await
                        {
                            tracing::warn!(
                                task_id = %updated.id,
                                "failed to record handoff success outcome: {error}"
                            );
                        }
                    }
                }
                if updated.source == "subagent" {
                    self.record_collaboration_outcome(&updated, "success").await;
                    self.record_subagent_outcome_on_parent(
                        &updated,
                        TaskLogLevel::Info,
                        "subagent completed",
                        updated.blocked_reason.clone(),
                    )
                    .await;
                }
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
                                retry_delay_ms.div_ceil(1000).max(1),
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
                if updated.status == TaskStatus::Failed {
                    self.settle_task_skill_consultations(&updated, "failure")
                        .await;
                    if updated.source == "handoff" {
                        if let Err(error) =
                            self.record_handoff_task_outcome(&updated, "failure").await
                        {
                            tracing::warn!(
                                task_id = %updated.id,
                                "failed to record handoff failure outcome: {error}"
                            );
                        }
                    }
                }
                if updated.source == "subagent"
                    && matches!(updated.status, TaskStatus::Failed | TaskStatus::Cancelled)
                {
                    self.record_collaboration_outcome(&updated, "failure").await;
                    self.record_subagent_outcome_on_parent(
                        &updated,
                        TaskLogLevel::Error,
                        "subagent failed",
                        updated.last_error.clone(),
                    )
                    .await;
                }
                Ok(())
            }
        }
    }

    async fn record_subagent_outcome_on_parent(
        &self,
        child_task: &AgentTask,
        level: TaskLogLevel,
        message: &str,
        details: Option<String>,
    ) {
        let Some(parent_task_id) = child_task.parent_task_id.as_deref() else {
            return;
        };

        let updated_parent = {
            let mut tasks = self.tasks.lock().await;
            let Some(parent) = tasks.iter_mut().find(|entry| entry.id == parent_task_id) else {
                return;
            };
            let detail_suffix = details
                .as_deref()
                .map(|value| format!("; {value}"))
                .unwrap_or_default();
            parent.logs.push(make_task_log_entry(
                child_task.retry_count,
                level,
                "subagent",
                &format!(
                    "{}: {} ({}){}",
                    message, child_task.title, child_task.id, detail_suffix
                ),
                Some(format!(
                    "runtime={} status={} thread_id={} session_id={}",
                    child_task.runtime,
                    serde_json::to_string(&child_task.status)
                        .unwrap_or_else(|_| "unknown".to_string()),
                    child_task.thread_id.as_deref().unwrap_or("-"),
                    child_task.session_id.as_deref().unwrap_or("-"),
                )),
            ));
            Some(parent.clone())
        };

        if let Some(parent) = updated_parent {
            self.persist_tasks().await;
            self.emit_task_update(&parent, Some("Subagent update received".into()));
        }
    }

    async fn record_handoff_task_outcome(&self, task: &AgentTask, outcome: &str) -> Result<()> {
        let Some(context) = self
            .get_handoff_learning_context_by_task_id(&task.id)
            .await?
        else {
            return Ok(());
        };

        let duration_ms = task
            .started_at
            .zip(task.completed_at)
            .map(|(started, completed)| completed.saturating_sub(started));
        let error_message = if matches!(outcome, "failure") {
            task.last_error.as_deref().or(task.error.as_deref())
        } else {
            None
        };

        let thread_tokens = if let Some(thread_id) = task.thread_id.as_deref() {
            let threads = self.threads.read().await;
            threads
                .get(thread_id)
                .map(|thread| thread.total_input_tokens + thread.total_output_tokens)
                .unwrap_or(0)
        } else {
            0
        };

        let ema_alpha = self.config.read().await.routing.confidence_ema_alpha;

        self.update_handoff_outcome(
            &context.handoff_log_id,
            if matches!(outcome, "success") {
                "completed"
            } else {
                "failed"
            },
            duration_ms,
            error_message,
        )
        .await?;

        self.record_capability_outcome(
            &context.to_specialist_id,
            &context.capability_tags,
            outcome,
            context.routing_score,
            thread_tokens,
            ema_alpha,
        )
        .await?;

        Ok(())
    }
}
