//! Task and goal dispatching — background execution scheduling.

use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RecoverableGoalPauseReason {
    ProviderCredits,
    RateLimit,
    TemporaryProvider,
    Transport,
}

impl RecoverableGoalPauseReason {
    fn notification_kind(self) -> &'static str {
        match self {
            Self::ProviderCredits => "goal_provider_credits",
            Self::RateLimit => "goal_provider_rate_limit",
            Self::TemporaryProvider => "goal_provider_temporary",
            Self::Transport => "goal_provider_transport",
        }
    }

    fn operator_label(self) -> &'static str {
        match self {
            Self::ProviderCredits => "provider credits or billing need attention",
            Self::RateLimit => "provider rate limit",
            Self::TemporaryProvider => "temporary provider outage",
            Self::Transport => "network transport problem",
        }
    }
}

fn recoverable_goal_pause_reason(message: &str) -> Option<RecoverableGoalPauseReason> {
    if let Some(structured) = crate::agent::llm_client::parse_structured_upstream_failure(message) {
        let diagnostic_text = structured.diagnostics.to_string().to_ascii_lowercase();
        let summary = structured.summary.to_ascii_lowercase();
        let combined = format!("{summary}\n{diagnostic_text}");
        if mentions_provider_credit_problem(&combined) {
            return Some(RecoverableGoalPauseReason::ProviderCredits);
        }
        return match structured.class.as_str() {
            "rate_limit" => Some(RecoverableGoalPauseReason::RateLimit),
            "temporary_upstream" => Some(RecoverableGoalPauseReason::TemporaryProvider),
            "transient_transport" => Some(RecoverableGoalPauseReason::Transport),
            "auth_configuration" if mentions_provider_credit_problem(&combined) => {
                Some(RecoverableGoalPauseReason::ProviderCredits)
            }
            _ => None,
        };
    }

    let lower = message.to_ascii_lowercase();
    if mentions_provider_credit_problem(&lower) {
        return Some(RecoverableGoalPauseReason::ProviderCredits);
    }
    if lower.contains("429") || lower.contains("rate limit") || lower.contains("too many requests")
    {
        return Some(RecoverableGoalPauseReason::RateLimit);
    }
    if lower.contains("timed out")
        || lower.contains("timeout")
        || lower.contains("transport error")
        || lower.contains("error sending request")
        || lower.contains("connection refused")
        || lower.contains("connection reset")
        || lower.contains("broken pipe")
        || lower.contains("unexpected eof")
        || lower.contains("network")
    {
        return Some(RecoverableGoalPauseReason::Transport);
    }
    if lower.contains("overloaded")
        || lower.contains("temporarily unavailable")
        || lower.contains("try again later")
        || lower.contains("503")
        || lower.contains("502")
    {
        return Some(RecoverableGoalPauseReason::TemporaryProvider);
    }
    None
}

fn mentions_provider_credit_problem(lower: &str) -> bool {
    (lower.contains("openrouter")
        && (lower.contains("credit")
            || lower.contains("credits")
            || lower.contains("billing")
            || lower.contains("payment")))
        || lower.contains("insufficient credits")
        || lower.contains("insufficient credit")
        || lower.contains("quota exceeded")
        || lower.contains("payment required")
        || lower.contains("billing hard limit")
}

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
                    if !engine
                        .pause_goal_run_for_recoverable_provider_error(&goal_run_id, &error)
                        .await
                    {
                        engine
                            .fail_goal_run(&goal_run_id, &error.to_string(), "goal-run", None)
                            .await;
                    }
                }
                engine.finish_goal_run_work(&goal_run_id).await;
            });
        }
    }

    async fn pause_goal_run_for_recoverable_provider_error(
        &self,
        goal_run_id: &str,
        error: &anyhow::Error,
    ) -> bool {
        let Some(reason) = recoverable_goal_pause_reason(&error.to_string()) else {
            return false;
        };

        let message = format!(
            "Goal paused after a recoverable provider issue: {}. Resolve it, then resume the goal.",
            reason.operator_label()
        );
        let error_text = error.to_string();
        let mut notification = None;
        let updated = {
            let mut goal_runs = self.goal_runs.lock().await;
            let Some(goal_run) = goal_runs.iter_mut().find(|item| item.id == goal_run_id) else {
                return false;
            };
            if matches!(
                goal_run.status,
                GoalRunStatus::Completed | GoalRunStatus::Failed | GoalRunStatus::Cancelled
            ) {
                return false;
            }
            goal_run.status = GoalRunStatus::Paused;
            goal_run.updated_at = now_millis();
            goal_run.completed_at = None;
            goal_run.last_error = Some(error_text.clone());
            goal_run.failure_cause = None;
            goal_run.events.push(make_goal_run_event(
                "provider_recovery",
                "goal run paused after recoverable provider issue",
                Some(message.clone()),
            ));
            if let Some(thread_id) = goal_run.thread_id.as_deref() {
                let now = now_millis() as i64;
                notification = Some(amux_protocol::InboxNotification {
                    id: format!("goal-provider-recovery:{goal_run_id}"),
                    source: "goal_runner".to_string(),
                    kind: reason.notification_kind().to_string(),
                    title: "Goal paused".to_string(),
                    body: format!(
                        "{}\n\nGoal: {}\n\nAfter fixing the provider or network issue, resume this goal.",
                        message, goal_run.title
                    ),
                    subtitle: Some(reason.operator_label().to_string()),
                    severity: "warning".to_string(),
                    created_at: now,
                    updated_at: now,
                    read_at: None,
                    archived_at: None,
                    deleted_at: None,
                    actions: vec![crate::notifications::open_thread_action(thread_id)],
                    metadata_json: Some(
                        serde_json::json!({
                            "goal_run_id": goal_run_id,
                            "reason": reason.notification_kind(),
                        })
                        .to_string(),
                    ),
                });
            }
            goal_run.clone()
        };

        self.persist_goal_runs().await;
        self.emit_goal_run_update(&updated, Some(message.clone()));
        if let Some(notification) = notification {
            if let Err(error) = self.upsert_inbox_notification(notification).await {
                tracing::warn!(goal_run_id, %error, "failed to upsert goal pause notification");
            }
        }
        let _ = self.event_tx.send(AgentEvent::WorkflowNotice {
            thread_id: updated.thread_id.clone().unwrap_or_default(),
            kind: reason.notification_kind().to_string(),
            message,
            details: Some(
                serde_json::json!({
                    "goal_run_id": goal_run_id,
                    "error": error_text,
                })
                .to_string(),
            ),
        });
        true
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

        if goal_run.current_step_index >= goal_run.steps.len() {
            if let Some(task_id) = goal_run.active_task_id.clone() {
                let task = {
                    let tasks = self.tasks.lock().await;
                    tasks.iter().find(|task| task.id == task_id).cloned()
                };
                if let Some(task) = task {
                    match task.status {
                        TaskStatus::Queued
                        | TaskStatus::InProgress
                        | TaskStatus::Blocked
                        | TaskStatus::AwaitingApproval => {
                            self.sync_goal_run_with_task(goal_run_id, &task).await;
                        }
                        TaskStatus::Completed | TaskStatus::BudgetExceeded => {
                            self.handle_goal_run_final_review_completion(goal_run_id, &task)
                                .await?;
                        }
                        TaskStatus::Failed
                        | TaskStatus::Cancelled
                        | TaskStatus::FailedAnalyzing => {
                            self.handle_goal_run_final_review_failure(goal_run_id, &task)
                                .await?;
                        }
                    }
                    return Ok(());
                }
            }

            if !self.goal_run_has_active_tasks(goal_run_id).await {
                self.complete_goal_run(goal_run_id).await?;
            }
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
        task_prompt::append_goal_run_context(self, &mut prompt, &task).await;
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
                        let budget_exceeded_reason =
                            "execution budget exceeded for this thread".to_string();
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
                        current.blocked_reason = if outcome.terminated_for_budget {
                            Some(budget_exceeded_reason.clone())
                        } else if waiting_for_subagents {
                            Some(format!(
                                "waiting for subagents: {}",
                                active_child_ids.join(", ")
                            ))
                        } else {
                            None
                        };
                        current.awaiting_approval_id = None;
                        current.error = if outcome.terminated_for_budget {
                            Some(budget_exceeded_reason.clone())
                        } else {
                            None
                        };
                        current.last_error = if outcome.terminated_for_budget {
                            Some(budget_exceeded_reason.clone())
                        } else {
                            None
                        };
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
                            } else if outcome.terminated_for_budget {
                                "task stopped after exhausting execution budget"
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
                match updated.status {
                    TaskStatus::Completed => {
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
                    }
                    TaskStatus::BudgetExceeded => {
                        self.handle_budget_exceeded_task_terminal_state(&updated)
                            .await;
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
                        if updated.source == "subagent" {
                            self.record_collaboration_outcome(&updated, "failure").await;
                            self.record_subagent_outcome_on_parent(
                                &updated,
                                TaskLogLevel::Warn,
                                "subagent budget exceeded",
                                updated.blocked_reason.clone(),
                            )
                            .await;
                        }
                    }
                    _ => {}
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

    async fn handle_budget_exceeded_task_terminal_state(&self, task: &AgentTask) {
        let Some(thread_id) = task.thread_id.as_deref() else {
            return;
        };

        let message = format!(
            "Task budget exceeded for this thread.\n\nThread `{thread_id}` exhausted its execution budget and is now locked for further operator messages. Review the completed work in this thread. If more work is needed, continue from the parent thread and respawn from the last completed point with a larger child budget."
        );
        if self.append_system_thread_message(thread_id, message).await {
            self.emit_workflow_notice(
                thread_id,
                "thread-budget-exceeded",
                "Thread budget exceeded. Further sends are blocked for this thread.",
                Some(
                    serde_json::json!({
                        "task_id": task.id,
                        "thread_id": thread_id,
                        "parent_task_id": task.parent_task_id,
                        "parent_thread_id": task.parent_thread_id,
                    })
                    .to_string(),
                ),
            );
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
            if child_task.status == TaskStatus::BudgetExceeded {
                let parent_thread_id = parent
                    .thread_id
                    .clone()
                    .or_else(|| child_task.parent_thread_id.clone());
                if let Some(parent_thread_id) = parent_thread_id {
                    let child_thread_id = child_task
                        .thread_id
                        .as_deref()
                        .unwrap_or("<unknown-child-thread>");
                    let parent_message = format!(
                        "Spawned thread `{child_thread_id}` exhausted its execution budget.\n\nReview what was completed in that child thread. If the result is sufficient, keep it. Otherwise respawn from the last completed point with a larger budget."
                    );
                    if self
                        .append_system_thread_message(&parent_thread_id, parent_message)
                        .await
                    {
                        self.emit_workflow_notice(
                            &parent_thread_id,
                            "child-thread-budget-exceeded",
                            format!("Spawned thread {child_thread_id} exhausted its budget."),
                            Some(
                                serde_json::json!({
                                    "child_task_id": child_task.id,
                                    "child_thread_id": child_task.thread_id,
                                    "parent_task_id": parent.id,
                                })
                                .to_string(),
                            ),
                        );
                    }
                }
            }
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

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn budget_exceeded_subagent_notifies_child_and_parent_threads() {
        let root = tempdir().expect("tempdir");
        let manager = SessionManager::new_test(root.path()).await;
        let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

        let parent_thread_id = "thread-parent";
        let child_thread_id = "thread-child";
        let parent_task_id = "task-parent";
        let child_task_id = "task-child";

        {
            let mut threads = engine.threads.write().await;
            threads.insert(
                parent_thread_id.to_string(),
                AgentThread {
                    id: parent_thread_id.to_string(),
                    agent_name: Some("Svarog".to_string()),
                    title: "Parent".to_string(),
                    messages: vec![AgentMessage::user("Continue until done", 1)],
                    pinned: false,
                    upstream_thread_id: None,
                    upstream_transport: None,
                    upstream_provider: None,
                    upstream_model: None,
                    upstream_assistant_id: None,
                    total_input_tokens: 0,
                    total_output_tokens: 0,
                    created_at: 1,
                    updated_at: 1,
                },
            );
            threads.insert(
                child_thread_id.to_string(),
                AgentThread {
                    id: child_thread_id.to_string(),
                    agent_name: Some("Dazhbog".to_string()),
                    title: "Child".to_string(),
                    messages: vec![AgentMessage::user("Do the refactor", 2)],
                    pinned: false,
                    upstream_thread_id: None,
                    upstream_transport: None,
                    upstream_provider: None,
                    upstream_model: None,
                    upstream_assistant_id: None,
                    total_input_tokens: 0,
                    total_output_tokens: 0,
                    created_at: 2,
                    updated_at: 2,
                },
            );
        }

        let parent_task = AgentTask {
            id: parent_task_id.to_string(),
            title: "Parent".to_string(),
            description: "Wait for child".to_string(),
            status: TaskStatus::Blocked,
            priority: TaskPriority::Normal,
            progress: 90,
            created_at: 1,
            started_at: Some(1),
            completed_at: None,
            error: None,
            result: None,
            thread_id: Some(parent_thread_id.to_string()),
            source: "user".to_string(),
            notify_on_complete: false,
            notify_channels: Vec::new(),
            dependencies: Vec::new(),
            command: None,
            session_id: None,
            goal_run_id: None,
            goal_run_title: None,
            goal_step_id: None,
            goal_step_title: None,
            parent_task_id: None,
            parent_thread_id: None,
            runtime: "daemon".to_string(),
            retry_count: 0,
            max_retries: 0,
            next_retry_at: None,
            scheduled_at: None,
            blocked_reason: Some(format!("waiting for subagents: {child_task_id}")),
            awaiting_approval_id: None,
            policy_fingerprint: None,
            approval_expires_at: None,
            containment_scope: None,
            compensation_status: None,
            compensation_summary: None,
            lane_id: None,
            last_error: None,
            logs: Vec::new(),
            tool_whitelist: None,
            tool_blacklist: None,
            override_provider: None,
            override_model: None,
            override_system_prompt: None,
            context_budget_tokens: None,
            context_overflow_action: None,
            termination_conditions: None,
            success_criteria: None,
            max_duration_secs: None,
            supervisor_config: None,
            sub_agent_def_id: None,
        };
        let child_task = AgentTask {
            id: child_task_id.to_string(),
            title: "Child".to_string(),
            description: "Refactor everything".to_string(),
            status: TaskStatus::BudgetExceeded,
            priority: TaskPriority::Normal,
            progress: 100,
            created_at: 2,
            started_at: Some(2),
            completed_at: Some(3),
            error: Some("execution budget exceeded for this thread".to_string()),
            result: None,
            thread_id: Some(child_thread_id.to_string()),
            source: "subagent".to_string(),
            notify_on_complete: false,
            notify_channels: Vec::new(),
            dependencies: Vec::new(),
            command: None,
            session_id: None,
            goal_run_id: None,
            goal_run_title: None,
            goal_step_id: None,
            goal_step_title: None,
            parent_task_id: Some(parent_task_id.to_string()),
            parent_thread_id: Some(parent_thread_id.to_string()),
            runtime: "daemon".to_string(),
            retry_count: 0,
            max_retries: 0,
            next_retry_at: None,
            scheduled_at: None,
            blocked_reason: Some("execution budget exceeded for this thread".to_string()),
            awaiting_approval_id: None,
            policy_fingerprint: None,
            approval_expires_at: None,
            containment_scope: None,
            compensation_status: None,
            compensation_summary: None,
            lane_id: None,
            last_error: Some("execution budget exceeded for this thread".to_string()),
            logs: Vec::new(),
            tool_whitelist: None,
            tool_blacklist: None,
            override_provider: None,
            override_model: None,
            override_system_prompt: None,
            context_budget_tokens: None,
            context_overflow_action: None,
            termination_conditions: None,
            success_criteria: None,
            max_duration_secs: None,
            supervisor_config: None,
            sub_agent_def_id: None,
        };

        {
            let mut tasks = engine.tasks.lock().await;
            tasks.push_back(parent_task);
            tasks.push_back(child_task.clone());
        }

        engine
            .handle_budget_exceeded_task_terminal_state(&child_task)
            .await;
        engine
            .record_subagent_outcome_on_parent(
                &child_task,
                TaskLogLevel::Warn,
                "subagent budget exceeded",
                child_task.blocked_reason.clone(),
            )
            .await;

        let threads = engine.threads.read().await;
        let child_thread = threads
            .get(child_thread_id)
            .expect("child thread should exist");
        assert!(child_thread.messages.iter().any(|message| {
            message.role == MessageRole::System
                && message
                    .content
                    .contains("Task budget exceeded for this thread")
                && message
                    .content
                    .contains("locked for further operator messages")
        }));

        let parent_thread = threads
            .get(parent_thread_id)
            .expect("parent thread should exist");
        assert!(parent_thread.messages.iter().any(|message| {
            message.role == MessageRole::System
                && message.content.contains(child_thread_id)
                && message
                    .content
                    .contains("respawn from the last completed point")
        }));
    }
}
