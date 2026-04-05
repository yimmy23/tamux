//! Task and goal run CRUD — create, list, cancel, control operations.

use super::*;

#[path = "task_crud/tasks.rs"]
mod tasks;

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
                    weles_review: None,
                    input_tokens: 0,
                    output_tokens: 0,
                    provider: None,
                    model: None,
                    api_transport: None,
                    response_id: None,
                    upstream_message: None,
                    provider_final_result: None,
                    reasoning: None,
                    message_kind: AgentMessageKind::Normal,
                    compaction_strategy: None,
                    compaction_payload: None,
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
        self.start_goal_run_with_surface(
            goal,
            title,
            thread_id,
            session_id,
            priority,
            client_request_id,
            autonomy_level,
            None,
        )
        .await
    }

    pub async fn start_goal_run_with_surface(
        &self,
        goal: String,
        title: Option<String>,
        thread_id: Option<String>,
        session_id: Option<String>,
        priority: Option<&str>,
        client_request_id: Option<String>,
        autonomy_level: Option<String>,
        client_surface: Option<amux_protocol::ClientSurface>,
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
        if let (Some(thread_id), Some(client_surface)) = (thread_id.as_deref(), client_surface) {
            self.set_thread_client_surface(thread_id, client_surface)
                .await;
        }
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
        if let Some(client_surface) = client_surface {
            self.set_goal_run_client_surface(&goal_run.id, client_surface)
                .await;
        }
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
                self.settle_goal_plan_causal_traces(&goal_run.id, "cancelled", None)
                    .await;
            }
            self.emit_goal_run_update(&goal_run, Some(goal_run_status_message(&goal_run).into()));
            return true;
        }

        false
    }
}

#[cfg(test)]
#[path = "tests/task_crud.rs"]
mod tests;
