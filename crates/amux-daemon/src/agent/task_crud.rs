//! Task and goal run CRUD — create, list, cancel, control operations.

use super::*;

impl AgentEngine {
    pub async fn restore_checkpoint(
        &self,
        checkpoint_id: &str,
    ) -> Result<crate::agent::liveness::state_layers::RestoreOutcome> {
        let Some(state_json) = self.history.get_checkpoint(checkpoint_id).await? else {
            anyhow::bail!("checkpoint not found");
        };
        let checkpoint = crate::agent::liveness::checkpoint::checkpoint_load(&state_json)?;
        let goal_run_id = checkpoint.goal_run_id.clone();
        let restored_step_index = checkpoint.goal_run.current_step_index;
        let tasks_restored = checkpoint.tasks_snapshot.len();

        {
            let mut goal_runs = self.goal_runs.lock().await;
            if let Some(existing) = goal_runs
                .iter_mut()
                .find(|goal_run| goal_run.id == goal_run_id)
            {
                *existing = checkpoint.goal_run.clone();
            } else {
                goal_runs.push_back(checkpoint.goal_run.clone());
            }
        }

        {
            let mut tasks = self.tasks.lock().await;
            let retained = tasks
                .drain(..)
                .filter(|task| task.goal_run_id.as_deref() != Some(goal_run_id.as_str()))
                .collect::<VecDeque<_>>();
            *tasks = retained;
            tasks.extend(checkpoint.tasks_snapshot.clone());
        }

        if let Some(thread_id) = checkpoint.thread_id.clone() {
            self.thread_todos
                .write()
                .await
                .insert(thread_id.clone(), checkpoint.todos.clone());
            if let Some(work_context) = checkpoint.work_context.clone() {
                self.thread_work_contexts
                    .write()
                    .await
                    .insert(thread_id.clone(), work_context);
            }
            let mut threads = self.threads.write().await;
            if let Some(thread) = threads.get_mut(&thread_id) {
                thread.updated_at = now_millis();
                thread.messages.push(AgentMessage {
                    id: generate_message_id(),
                    role: MessageRole::System,
                    content: format!(
                        "Restored goal run {} from checkpoint {} at step {}.",
                        goal_run_id, checkpoint.id, restored_step_index
                    ),
                    tool_calls: None,
                    tool_call_id: None,
                    tool_name: None,
                    tool_arguments: None,
                    tool_status: None,
                    input_tokens: 0,
                    output_tokens: 0,
                    provider: None,
                    model: None,
                    api_transport: None,
                    response_id: None,
                    reasoning: None,
                    timestamp: now_millis(),
                });
            }
        }

        self.persist_goal_runs().await;
        self.persist_tasks().await;
        self.persist_todos().await;
        self.persist_work_context().await;
        self.persist_threads().await;

        let outcome = crate::agent::liveness::state_layers::RestoreOutcome {
            checkpoint_id: checkpoint.id,
            goal_run_id,
            restored_step_index,
            tasks_restored,
            context_restored: checkpoint.thread_id.is_some(),
        };

        let indicators_json = serde_json::json!({
            "restored_step_index": outcome.restored_step_index,
            "tasks_restored": outcome.tasks_restored,
            "context_restored": outcome.context_restored,
        })
        .to_string();
        let _ = self
            .history
            .insert_health_log(
                &format!("health_{}", Uuid::new_v4()),
                "goal_run",
                &outcome.goal_run_id,
                "healthy",
                Some(&indicators_json),
                Some("restore_checkpoint"),
                now_millis(),
            )
            .await;

        Ok(outcome)
    }

    pub async fn start_goal_run(
        &self,
        goal: String,
        title: Option<String>,
        thread_id: Option<String>,
        session_id: Option<String>,
        priority: Option<&str>,
        client_request_id: Option<String>,
        autonomy_level: Option<String>,
    ) -> GoalRun {
        let normalized_goal_key = normalize_goal_key(&goal);
        let normalized_request_id = client_request_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned);
        {
            let goal_runs = self.goal_runs.lock().await;
            if let Some(existing) = goal_runs
                .iter()
                .rev()
                .find(|existing| {
                    if matches!(
                        existing.status,
                        GoalRunStatus::Completed | GoalRunStatus::Failed | GoalRunStatus::Cancelled
                    ) {
                        return false;
                    }
                    if existing.thread_id != thread_id || existing.session_id != session_id {
                        return false;
                    }
                    if normalize_goal_key(&existing.goal) != normalized_goal_key {
                        return false;
                    }
                    match (&normalized_request_id, &existing.client_request_id) {
                        (Some(request_id), Some(existing_request_id)) => {
                            existing_request_id == request_id
                        }
                        (Some(_), None) => false,
                        _ => true,
                    }
                })
                .cloned()
            {
                return self.project_goal_run(existing).await;
            }
        }

        let now = now_millis();
        let normalized_title = title
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| summarize_goal_title(&goal));
        let goal_run = GoalRun {
            id: format!("goal_{}", Uuid::new_v4()),
            title: normalized_title,
            goal,
            client_request_id: normalized_request_id,
            status: GoalRunStatus::Queued,
            priority: parse_priority_str(priority.unwrap_or("normal")),
            created_at: now,
            updated_at: now,
            started_at: None,
            completed_at: None,
            thread_id,
            session_id,
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
            awaiting_approval_id: None,
            active_task_id: None,
            duration_ms: None,
            child_task_ids: Vec::new(),
            child_task_count: 0,
            approval_count: 0,
            steps: Vec::new(),
            events: vec![make_goal_run_event("queue", "goal run created", None)],
            total_prompt_tokens: 0,
            total_completion_tokens: 0,
            estimated_cost_usd: None,
            autonomy_level: autonomy_level
                .as_deref()
                .map(super::autonomy::AutonomyLevel::from_str_or_default)
                .unwrap_or_default(),
            authorship_tag: None,
        };

        self.goal_runs.lock().await.push_back(goal_run.clone());
        self.persist_goal_runs().await;
        self.emit_goal_run_update(&goal_run, Some("Goal queued".into()));
        self.record_provenance_event(
            "goal_created",
            "goal run created",
            serde_json::json!({
                "goal_run_id": goal_run.id.clone(),
                "title": goal_run.title.clone(),
                "goal": goal_run.goal.clone(),
                "priority": format!("{:?}", goal_run.priority).to_lowercase(),
            }),
            Some(goal_run.id.as_str()),
            None,
            goal_run.thread_id.as_deref(),
            None,
            None,
        )
        .await;
        if let Err(error) = self.record_goal_start_episode(&goal_run).await {
            tracing::warn!(
                goal_run_id = %goal_run.id,
                error = %error,
                "failed to record goal start episode"
            );
        }
        self.project_goal_run(goal_run).await
    }

    pub async fn list_goal_runs(&self) -> Vec<GoalRun> {
        let goal_runs = self.goal_runs.lock().await;
        let mut items: Vec<GoalRun> = goal_runs.iter().cloned().collect();
        drop(goal_runs);
        let mut projected = Vec::with_capacity(items.len());
        for goal_run in items.drain(..) {
            projected.push(self.project_goal_run(goal_run).await);
        }
        projected.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        projected
    }

    pub async fn get_goal_run(&self, goal_run_id: &str) -> Option<GoalRun> {
        let goal_run = self
            .goal_runs
            .lock()
            .await
            .iter()
            .find(|goal_run| goal_run.id == goal_run_id)
            .cloned()?;
        Some(self.project_goal_run(goal_run).await)
    }

    pub async fn list_todos(&self) -> HashMap<String, Vec<TodoItem>> {
        self.thread_todos.read().await.clone()
    }

    pub async fn get_todos(&self, thread_id: &str) -> Vec<TodoItem> {
        self.thread_todos
            .read()
            .await
            .get(thread_id)
            .cloned()
            .unwrap_or_default()
    }

    pub async fn get_work_context(&self, thread_id: &str) -> ThreadWorkContext {
        self.refresh_thread_repo_context(thread_id).await;
        self.thread_work_contexts
            .read()
            .await
            .get(thread_id)
            .cloned()
            .unwrap_or_else(|| ThreadWorkContext {
                thread_id: thread_id.to_string(),
                entries: Vec::new(),
            })
    }

    pub(super) async fn project_goal_run(&self, goal_run: GoalRun) -> GoalRun {
        let tasks = self.tasks.lock().await;
        let related_tasks = tasks
            .iter()
            .filter(|task| task.goal_run_id.as_deref() == Some(goal_run.id.as_str()))
            .cloned()
            .collect::<Vec<_>>();
        project_goal_run_snapshot(goal_run, &related_tasks, now_millis())
    }

    pub(super) async fn goal_run_has_active_tasks(&self, goal_run_id: &str) -> bool {
        let tasks = self.tasks.lock().await;
        tasks.iter().any(|task| {
            task.goal_run_id.as_deref() == Some(goal_run_id)
                && matches!(
                    task.status,
                    TaskStatus::Queued
                        | TaskStatus::InProgress
                        | TaskStatus::Blocked
                        | TaskStatus::FailedAnalyzing
                        | TaskStatus::AwaitingApproval
                )
        })
    }

    pub async fn control_goal_run(
        &self,
        goal_run_id: &str,
        action: &str,
        step_index: Option<usize>,
    ) -> bool {
        let mut changed_goal: Option<GoalRun> = None;
        let mut task_to_cancel: Option<String> = None;
        let mut task_to_release: Option<(String, Option<String>)> = None;
        {
            let mut goal_runs = self.goal_runs.lock().await;
            let Some(goal_run) = goal_runs.iter_mut().find(|item| item.id == goal_run_id) else {
                return false;
            };

            match action {
                "pause" => {
                    if matches!(
                        goal_run.status,
                        GoalRunStatus::Queued
                            | GoalRunStatus::Planning
                            | GoalRunStatus::Running
                            | GoalRunStatus::AwaitingApproval
                    ) {
                        goal_run.status = GoalRunStatus::Paused;
                        goal_run.updated_at = now_millis();
                        goal_run.events.push(make_goal_run_event(
                            "control",
                            "goal run paused",
                            None,
                        ));
                        changed_goal = Some(goal_run.clone());
                    }
                }
                "resume" => {
                    if goal_run.status == GoalRunStatus::Paused {
                        goal_run.status = if goal_run.steps.is_empty() {
                            GoalRunStatus::Queued
                        } else {
                            GoalRunStatus::Running
                        };
                        goal_run.updated_at = now_millis();
                        goal_run.events.push(make_goal_run_event(
                            "control",
                            "goal run resumed",
                            None,
                        ));
                        changed_goal = Some(goal_run.clone());
                    }
                }
                "acknowledge" | "ack" => {
                    if goal_run.status == GoalRunStatus::AwaitingApproval {
                        let current_task_id = goal_run
                            .steps
                            .get(goal_run.current_step_index)
                            .and_then(|step| step.task_id.clone());
                        let current_ack = goal_run.awaiting_approval_id.clone();
                        let has_steps = !goal_run.steps.is_empty();
                        goal_run.status = if has_steps {
                            GoalRunStatus::Running
                        } else {
                            GoalRunStatus::Queued
                        };
                        goal_run.updated_at = now_millis();
                        goal_run.awaiting_approval_id = None;
                        goal_run.events.push(make_goal_run_event(
                            "autonomy_acknowledgment",
                            "supervised acknowledgment received; approval gate cleared",
                            current_ack.clone(),
                        ));
                        changed_goal = Some(goal_run.clone());
                        task_to_release = current_task_id.map(|task_id| (task_id, current_ack));
                    }
                }
                "retry_step" | "retry-step" => {
                    let target_index = resolve_goal_run_control_step(goal_run, step_index);
                    task_to_cancel = goal_run
                        .steps
                        .get(target_index)
                        .and_then(|step| step.task_id.clone());
                    if retry_goal_run_step(goal_run, step_index).is_ok() {
                        goal_run.updated_at = now_millis();
                        goal_run.awaiting_approval_id = None;
                        goal_run.active_task_id = None;
                        goal_run.events.push(make_goal_run_event(
                            "control",
                            "goal run step retry requested",
                            step_index.map(|value| format!("step {value}")),
                        ));
                        changed_goal = Some(goal_run.clone());
                    }
                }
                "rerun_from_step" | "rerun-from-step" => {
                    let target_index = resolve_goal_run_control_step(goal_run, step_index);
                    task_to_cancel = goal_run
                        .steps
                        .get(target_index)
                        .and_then(|step| step.task_id.clone());
                    if rerun_goal_run_from_step(goal_run, step_index).is_ok() {
                        goal_run.updated_at = now_millis();
                        goal_run.awaiting_approval_id = None;
                        goal_run.active_task_id = None;
                        goal_run.events.push(make_goal_run_event(
                            "control",
                            "goal run rerun requested",
                            step_index.map(|value| format!("step {value}")),
                        ));
                        changed_goal = Some(goal_run.clone());
                    }
                }
                "cancel" => {
                    if !matches!(
                        goal_run.status,
                        GoalRunStatus::Completed | GoalRunStatus::Failed | GoalRunStatus::Cancelled
                    ) {
                        goal_run.status = GoalRunStatus::Cancelled;
                        goal_run.completed_at = Some(now_millis());
                        goal_run.updated_at = now_millis();
                        goal_run.events.push(make_goal_run_event(
                            "control",
                            "goal run cancelled",
                            None,
                        ));
                        goal_run.awaiting_approval_id = None;
                        goal_run.active_task_id = None;
                        task_to_cancel = goal_run
                            .steps
                            .get(goal_run.current_step_index)
                            .and_then(|step| step.task_id.clone());
                        changed_goal = Some(goal_run.clone());
                    }
                }
                _ => {}
            }
        }

        if let Some((task_id, ack_id)) = task_to_release {
            let released_task = {
                let mut tasks = self.tasks.lock().await;
                if let Some(task) = tasks.iter_mut().find(|task| task.id == task_id) {
                    if task.status == TaskStatus::AwaitingApproval {
                        task.status = TaskStatus::Queued;
                        task.started_at = None;
                        task.awaiting_approval_id = None;
                        task.blocked_reason = None;
                        task.logs.push(make_task_log_entry(
                            task.retry_count,
                            TaskLogLevel::Info,
                            "autonomy_acknowledgment",
                            "supervised acknowledgment received; task released to queue",
                            ack_id,
                        ));
                        task.progress = task.progress.max(5);
                        Some(task.clone())
                    } else {
                        None
                    }
                } else {
                    None
                }
            };

            if let Some(task) = released_task {
                self.persist_tasks().await;
                self.emit_task_update(&task, Some(status_message(&task).into()));
            }
        }

        if let Some(task_id) = task_to_cancel {
            let _ = self.cancel_task(&task_id).await;
        }

        if let Some(goal_run) = changed_goal {
            self.persist_goal_runs().await;
            if goal_run.status == GoalRunStatus::Cancelled {
                self.settle_goal_skill_consultations(&goal_run, "cancelled")
                    .await;
            }
            self.emit_goal_run_update(&goal_run, Some(goal_run_status_message(&goal_run).into()));
            return true;
        }

        false
    }

    pub async fn add_task(
        &self,
        title: String,
        description: String,
        priority: &str,
        command: Option<String>,
        session_id: Option<String>,
        dependencies: Vec<String>,
    ) -> String {
        self.enqueue_task(
            title,
            description,
            priority,
            command,
            session_id,
            dependencies,
            None,
            "user",
            None,
            None,
            None,
            None,
        )
        .await
        .id
    }

    pub async fn enqueue_task(
        &self,
        title: String,
        description: String,
        priority: &str,
        command: Option<String>,
        session_id: Option<String>,
        dependencies: Vec<String>,
        scheduled_at: Option<u64>,
        source: &str,
        goal_run_id: Option<String>,
        parent_task_id: Option<String>,
        parent_thread_id: Option<String>,
        runtime: Option<String>,
    ) -> AgentTask {
        let id = format!("task_{}", Uuid::new_v4());
        let now = now_millis();
        let initial_schedule_reason = scheduled_at
            .filter(|deadline| *deadline > now)
            .map(describe_scheduled_time);
        let task = AgentTask {
            id: id.clone(),
            title,
            description,
            status: if initial_schedule_reason.is_some() {
                TaskStatus::Blocked
            } else {
                TaskStatus::Queued
            },
            priority: parse_priority_str(priority),
            progress: 0,
            created_at: now,
            started_at: None,
            completed_at: None,
            error: None,
            result: None,
            thread_id: None,
            source: source.into(),
            notify_on_complete: true,
            notify_channels: vec!["in-app".into()],
            dependencies,
            command,
            session_id,
            goal_run_id,
            goal_run_title: None,
            goal_step_id: None,
            goal_step_title: None,
            parent_task_id,
            parent_thread_id,
            runtime: runtime.unwrap_or_else(|| "daemon".to_string()),
            retry_count: 0,
            max_retries: self.config.read().await.max_retries.max(1),
            next_retry_at: None,
            scheduled_at,
            blocked_reason: initial_schedule_reason.clone(),
            awaiting_approval_id: None,
            lane_id: None,
            last_error: None,
            tool_whitelist: None,
            tool_blacklist: None,
            context_budget_tokens: None,
            context_overflow_action: None,
            termination_conditions: None,
            success_criteria: None,
            max_duration_secs: None,
            supervisor_config: None,
            override_provider: None,
            override_model: None,
            override_system_prompt: None,
            sub_agent_def_id: None,
            logs: vec![make_task_log_entry(
                0,
                TaskLogLevel::Info,
                "queue",
                if initial_schedule_reason.is_some() {
                    "task scheduled"
                } else {
                    "task enqueued"
                },
                initial_schedule_reason,
            )],
        };

        self.tasks.lock().await.push_back(task);
        self.persist_tasks().await;

        let task = self
            .tasks
            .lock()
            .await
            .iter()
            .find(|task| task.id == id)
            .cloned()
            .expect("enqueued task missing from queue");
        self.emit_task_update(&task, Some(status_message(&task).into()));

        task
    }

    pub async fn cancel_task(&self, task_id: &str) -> bool {
        let mut tasks = self.tasks.lock().await;
        if let Some(task) = tasks.iter_mut().find(|t| t.id == task_id) {
            if matches!(
                task.status,
                TaskStatus::Queued
                    | TaskStatus::InProgress
                    | TaskStatus::Blocked
                    | TaskStatus::FailedAnalyzing
                    | TaskStatus::AwaitingApproval
            ) {
                let thread_to_stop = task.thread_id.clone();
                let session_to_interrupt = task.session_id.clone();
                task.status = TaskStatus::Cancelled;
                task.completed_at = Some(now_millis());
                task.lane_id = None;
                task.blocked_reason = None;
                task.awaiting_approval_id = None;
                task.logs.push(make_task_log_entry(
                    task.retry_count,
                    TaskLogLevel::Warn,
                    "queue",
                    "task cancelled by user",
                    None,
                ));
                let updated = task.clone();
                drop(tasks);
                self.persist_tasks().await;
                if let Some(thread_id) = thread_to_stop {
                    let _ = self.stop_stream(&thread_id).await;
                }
                if let Some(session_id) =
                    session_to_interrupt.and_then(|value| Uuid::parse_str(&value).ok())
                {
                    let _ = self.session_manager.write_input(session_id, &[3]).await;
                }
                self.emit_task_update(&updated, Some("Cancelled by user".into()));
                self.settle_task_skill_consultations(&updated, "cancelled")
                    .await;
                self.record_collaboration_outcome(&updated, "cancelled")
                    .await;
                self.record_provenance_event(
                    "step_cancelled",
                    "task cancelled by operator",
                    serde_json::json!({
                        "task_id": updated.id,
                        "title": updated.title,
                        "source": updated.source,
                    }),
                    updated.goal_run_id.as_deref(),
                    Some(updated.id.as_str()),
                    updated.thread_id.as_deref(),
                    None,
                    None,
                )
                .await;
                return true;
            }
        }
        false
    }

    pub async fn handle_task_approval_resolution(
        &self,
        approval_id: &str,
        decision: amux_protocol::ApprovalDecision,
    ) -> bool {
        let updated = {
            let mut tasks = self.tasks.lock().await;
            let Some(task) = tasks
                .iter_mut()
                .find(|task| task.awaiting_approval_id.as_deref() == Some(approval_id))
            else {
                return false;
            };

            match decision {
                amux_protocol::ApprovalDecision::ApproveOnce
                | amux_protocol::ApprovalDecision::ApproveSession => {
                    task.status = TaskStatus::Queued;
                    task.started_at = None;
                    task.awaiting_approval_id = None;
                    task.blocked_reason = None;
                    task.logs.push(make_task_log_entry(
                        task.retry_count,
                        TaskLogLevel::Info,
                        "approval",
                        "operator approved managed command; task re-queued",
                        None,
                    ));
                }
                amux_protocol::ApprovalDecision::Deny => {
                    let reason = "operator denied managed command approval".to_string();
                    task.status = TaskStatus::Failed;
                    task.started_at = None;
                    task.completed_at = Some(now_millis());
                    task.awaiting_approval_id = None;
                    task.blocked_reason = Some(reason.clone());
                    task.error = Some(reason.clone());
                    task.last_error = Some(reason.clone());
                    task.logs.push(make_task_log_entry(
                        task.retry_count,
                        TaskLogLevel::Error,
                        "approval",
                        "operator denied managed command; task failed",
                        Some(reason),
                    ));
                }
            }

            task.clone()
        };

        self.persist_tasks().await;
        self.emit_task_update(&updated, Some(status_message(&updated).into()));
        self.record_provenance_event(
            match decision {
                amux_protocol::ApprovalDecision::ApproveOnce
                | amux_protocol::ApprovalDecision::ApproveSession => "approval_granted",
                amux_protocol::ApprovalDecision::Deny => "approval_denied",
            },
            match decision {
                amux_protocol::ApprovalDecision::ApproveOnce
                | amux_protocol::ApprovalDecision::ApproveSession => {
                    "operator approved managed command"
                }
                amux_protocol::ApprovalDecision::Deny => "operator denied managed command",
            },
            serde_json::json!({
                "approval_id": approval_id,
                "task_id": updated.id,
                "title": updated.title,
                "decision": format!("{decision:?}").to_lowercase(),
            }),
            updated.goal_run_id.as_deref(),
            Some(updated.id.as_str()),
            updated.thread_id.as_deref(),
            Some(approval_id),
            None,
        )
        .await;

        // Track operator correction in counter-who (Phase 1: Memory Foundation - CWHO-03)
        if matches!(decision, amux_protocol::ApprovalDecision::Deny) {
            let correction_desc = format!("Denied approval for task: {}", updated.title);
            let thread_id = updated.thread_id.clone().unwrap_or_default();
            self.update_counter_who_on_correction(&thread_id, &correction_desc)
                .await;
        }

        true
    }

    pub(super) async fn snapshot_tasks(&self) -> Vec<AgentTask> {
        let sessions = self.session_manager.list().await;
        let mut tasks = self.tasks.lock().await;
        let changed = refresh_task_queue_state(&mut tasks, now_millis(), &sessions);
        let snapshot = tasks.iter().cloned().collect();
        drop(tasks);

        if !changed.is_empty() {
            self.persist_tasks().await;
            for task in changed {
                self.emit_task_update(&task, Some(status_message(&task).into()));
            }
        }

        snapshot
    }

    pub async fn list_tasks(&self) -> Vec<AgentTask> {
        self.snapshot_tasks().await
    }

    pub async fn list_runs(&self) -> Vec<AgentRun> {
        let tasks = self.snapshot_tasks().await;
        let sessions = self.session_manager.list().await;
        let mut runs = project_task_runs(&tasks, &sessions);
        runs.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        runs
    }

    pub async fn get_run(&self, run_id: &str) -> Option<AgentRun> {
        let tasks = self.snapshot_tasks().await;
        let sessions = self.session_manager.list().await;
        project_task_runs(&tasks, &sessions)
            .into_iter()
            .find(|run| run.id == run_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::episodic::{EpisodeType, LinkType};
    use crate::session_manager::SessionManager;
    use tempfile::tempdir;

    fn sample_supervised_goal_run(goal_run_id: &str, task_id: &str, approval_id: &str) -> GoalRun {
        GoalRun {
            id: goal_run_id.to_string(),
            title: "supervised goal".to_string(),
            goal: "verify explicit acknowledgment".to_string(),
            client_request_id: None,
            status: GoalRunStatus::AwaitingApproval,
            priority: TaskPriority::Normal,
            created_at: now_millis(),
            updated_at: now_millis(),
            started_at: Some(now_millis()),
            completed_at: None,
            thread_id: None,
            session_id: None,
            current_step_index: 0,
            current_step_title: Some("step-1".to_string()),
            current_step_kind: Some(GoalRunStepKind::Command),
            replan_count: 0,
            max_replans: 2,
            plan_summary: Some("plan".to_string()),
            reflection_summary: None,
            memory_updates: Vec::new(),
            generated_skill_path: None,
            last_error: None,
            failure_cause: None,
            child_task_ids: vec![task_id.to_string()],
            child_task_count: 1,
            approval_count: 0,
            awaiting_approval_id: Some(approval_id.to_string()),
            active_task_id: Some(task_id.to_string()),
            duration_ms: None,
            steps: vec![GoalRunStep {
                id: "step-1".to_string(),
                position: 0,
                title: "step-1".to_string(),
                instructions: "do supervised work".to_string(),
                kind: GoalRunStepKind::Command,
                success_criteria: "done".to_string(),
                session_id: None,
                status: GoalRunStepStatus::InProgress,
                task_id: Some(task_id.to_string()),
                summary: None,
                error: None,
                started_at: Some(now_millis()),
                completed_at: None,
            }],
            events: Vec::new(),
            total_prompt_tokens: 0,
            total_completion_tokens: 0,
            estimated_cost_usd: None,
            autonomy_level: super::autonomy::AutonomyLevel::Supervised,
            authorship_tag: None,
        }
    }

    async fn sample_awaiting_task(
        engine: &AgentEngine,
        goal_run_id: &str,
        task_id: &str,
        approval_id: &str,
    ) {
        engine.tasks.lock().await.push_back(AgentTask {
            id: task_id.to_string(),
            title: "step task".to_string(),
            description: "goal step work".to_string(),
            status: TaskStatus::AwaitingApproval,
            priority: TaskPriority::Normal,
            progress: 30,
            created_at: now_millis(),
            started_at: Some(now_millis()),
            completed_at: None,
            error: None,
            result: None,
            thread_id: None,
            source: "goal_run".to_string(),
            notify_on_complete: false,
            notify_channels: Vec::new(),
            dependencies: Vec::new(),
            command: None,
            session_id: None,
            goal_run_id: Some(goal_run_id.to_string()),
            goal_run_title: Some("supervised goal".to_string()),
            goal_step_id: Some("step-1".to_string()),
            goal_step_title: Some("step-1".to_string()),
            parent_task_id: None,
            parent_thread_id: None,
            runtime: "daemon".to_string(),
            retry_count: 0,
            max_retries: 3,
            next_retry_at: None,
            scheduled_at: None,
            blocked_reason: Some("awaiting supervised acknowledgment".to_string()),
            awaiting_approval_id: Some(approval_id.to_string()),
            lane_id: None,
            last_error: None,
            logs: Vec::new(),
            tool_whitelist: None,
            tool_blacklist: None,
            context_budget_tokens: None,
            context_overflow_action: None,
            termination_conditions: None,
            success_criteria: None,
            max_duration_secs: None,
            supervisor_config: None,
            override_provider: None,
            override_model: None,
            override_system_prompt: None,
            sub_agent_def_id: None,
        });
    }

    #[tokio::test]
    async fn resume_does_not_clear_supervised_awaiting_approval_gate() {
        let root = tempdir().expect("temp dir");
        let manager = SessionManager::new_test(root.path()).await;
        let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
        let goal_run_id = "goal-supervised";
        let task_id = "task-supervised";
        let approval_id = "autonomy-ack-1";

        engine
            .goal_runs
            .lock()
            .await
            .push_back(sample_supervised_goal_run(
                goal_run_id,
                task_id,
                approval_id,
            ));
        sample_awaiting_task(&engine, goal_run_id, task_id, approval_id).await;

        let changed = engine.control_goal_run(goal_run_id, "resume", None).await;
        assert!(
            !changed,
            "resume should not mutate awaiting-approval supervised runs"
        );

        let goal = engine
            .get_goal_run(goal_run_id)
            .await
            .expect("goal should exist");
        assert_eq!(goal.status, GoalRunStatus::AwaitingApproval);
        assert_eq!(goal.awaiting_approval_id.as_deref(), Some(approval_id));

        let task = engine
            .tasks
            .lock()
            .await
            .iter()
            .find(|task| task.id == task_id)
            .cloned()
            .expect("task should exist");
        assert_eq!(task.status, TaskStatus::AwaitingApproval);
        assert_eq!(task.awaiting_approval_id.as_deref(), Some(approval_id));
    }

    #[tokio::test]
    async fn explicit_acknowledgment_unblocks_goal_and_current_step_task() {
        let root = tempdir().expect("temp dir");
        let manager = SessionManager::new_test(root.path()).await;
        let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
        let goal_run_id = "goal-supervised";
        let task_id = "task-supervised";
        let approval_id = "autonomy-ack-2";

        engine
            .goal_runs
            .lock()
            .await
            .push_back(sample_supervised_goal_run(
                goal_run_id,
                task_id,
                approval_id,
            ));
        sample_awaiting_task(&engine, goal_run_id, task_id, approval_id).await;

        let changed = engine
            .control_goal_run(goal_run_id, "acknowledge", None)
            .await;
        assert!(changed, "acknowledge should clear supervised gate");

        let goal = engine
            .get_goal_run(goal_run_id)
            .await
            .expect("goal should exist");
        assert_eq!(goal.status, GoalRunStatus::Running);
        assert!(goal.awaiting_approval_id.is_none());

        let task = engine
            .tasks
            .lock()
            .await
            .iter()
            .find(|task| task.id == task_id)
            .cloned()
            .expect("task should exist");
        assert_eq!(task.status, TaskStatus::Queued);
        assert!(task.awaiting_approval_id.is_none());
    }

    #[tokio::test]
    async fn start_goal_run_records_goal_start_episode_with_archived_fields() {
        let root = tempdir().expect("temp dir");
        let manager = SessionManager::new_test(root.path()).await;
        let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

        let goal = engine
            .start_goal_run(
                "repair archived parity gaps".to_string(),
                Some("Repair parity".to_string()),
                Some("thread-epis-1".to_string()),
                Some("session-epis-1".to_string()),
                None,
                None,
                None,
            )
            .await;

        let episodes = engine
            .list_episodes_for_goal_run(&goal.id)
            .await
            .expect("episodes should load");
        assert_eq!(
            episodes.len(),
            1,
            "goal start should immediately record one episode"
        );

        let episode_json =
            serde_json::to_value(&episodes[0]).expect("episode should serialize to json");
        assert_eq!(episode_json["episode_type"], "goal_start");
        assert_eq!(episode_json["goal_text"], "repair archived parity gaps");
        assert_eq!(episode_json["goal_type"], "goal_run");
        assert_eq!(
            episode_json["summary"],
            "Repair parity: repair archived parity gaps"
        );
        assert!(
            episode_json.get("confidence_before").is_some(),
            "goal-start episodes should carry explicit confidence_before field"
        );
        assert!(
            episode_json.get("confidence_after").is_some(),
            "goal-start episodes should carry explicit confidence_after field"
        );
    }

    #[tokio::test]
    async fn repeated_goal_start_creates_retry_link_to_previous_related_episode() {
        let root = tempdir().expect("temp dir");
        let manager = SessionManager::new_test(root.path()).await;
        let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

        let first = engine
            .start_goal_run(
                "repair archived parity gaps".to_string(),
                Some("Repair parity".to_string()),
                Some("thread-epis-2".to_string()),
                Some("session-epis-2".to_string()),
                None,
                None,
                None,
            )
            .await;
        engine
            .record_goal_episode(&first, crate::agent::episodic::EpisodeOutcome::Failure)
            .await
            .expect("first goal failure episode should record");

        let second = engine
            .start_goal_run(
                "repair archived parity gaps".to_string(),
                Some("Repair parity again".to_string()),
                Some("thread-epis-3".to_string()),
                Some("session-epis-3".to_string()),
                None,
                Some("req-2".to_string()),
                None,
            )
            .await;

        let episodes = engine
            .list_episodes_for_goal_run(&second.id)
            .await
            .expect("episodes should load");
        let start_episode = episodes
            .iter()
            .find(|episode| episode.episode_type == EpisodeType::GoalStart)
            .expect("second goal should have a goal_start episode");

        let links = engine
            .get_episode_links(&start_episode.id)
            .await
            .expect("links should load");
        assert!(
            links.iter().any(|link| link.link_type == LinkType::RetryOf),
            "repeated goal should link to the prior related episode as retry_of"
        );
    }

    #[tokio::test]
    async fn suppressed_session_id_skips_goal_start_episode_recording() {
        let root = tempdir().expect("temp dir");
        let manager = SessionManager::new_test(root.path()).await;
        let mut config = AgentConfig::default();
        config.episodic.suppressed_session_ids = vec!["session-suppressed".to_string()];
        let engine = AgentEngine::new_test(manager, config, root.path()).await;

        let goal = engine
            .start_goal_run(
                "do not persist this goal".to_string(),
                Some("Suppressed goal".to_string()),
                Some("thread-suppressed".to_string()),
                Some("session-suppressed".to_string()),
                None,
                None,
                None,
            )
            .await;

        let episodes = engine
            .list_episodes_for_goal_run(&goal.id)
            .await
            .expect("episodes should load");
        assert!(
            episodes.is_empty(),
            "suppressed session ids should prevent per-session episodic recording"
        );
    }
}
