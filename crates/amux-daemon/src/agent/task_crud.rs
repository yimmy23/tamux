//! Task and goal run CRUD — create, list, cancel, control operations.

use super::*;

#[path = "task_crud/tasks.rs"]
mod tasks;

fn goal_run_detail_frame_fits_ipc(goal_run: &Option<GoalRun>) -> bool {
    let Ok(goal_run_json) = serde_json::to_string(goal_run) else {
        return false;
    };

    amux_protocol::daemon_message_fits_ipc(&amux_protocol::DaemonMessage::AgentGoalRunDetail {
        goal_run_json,
    })
}

fn task_detail_frame_fits_ipc(task: &AgentTask) -> bool {
    let Ok(task_json) = serde_json::to_string(task) else {
        return false;
    };

    amux_protocol::daemon_message_fits_ipc(&amux_protocol::DaemonMessage::AgentTaskEnqueued {
        task_json,
    })
}

fn task_list_frame_fits_ipc(tasks: &[AgentTask]) -> bool {
    let Ok(tasks_json) = serde_json::to_string(tasks) else {
        return false;
    };

    amux_protocol::daemon_message_fits_ipc(&amux_protocol::DaemonMessage::AgentTaskList {
        tasks_json,
    })
}

fn goal_run_list_frame_fits_ipc(goal_runs: &[GoalRun]) -> bool {
    let Ok(goal_runs_json) = serde_json::to_string(goal_runs) else {
        return false;
    };

    amux_protocol::daemon_message_fits_ipc(&amux_protocol::DaemonMessage::AgentGoalRunList {
        goal_runs_json,
    })
}

fn todo_detail_frame_fits_ipc(thread_id: &str, todos: &[TodoItem]) -> bool {
    let Ok(todos_json) = serde_json::to_string(todos) else {
        return false;
    };

    amux_protocol::daemon_message_fits_ipc(&amux_protocol::DaemonMessage::AgentTodoDetail {
        thread_id: thread_id.to_string(),
        todos_json,
    })
}

fn todo_list_frame_fits_ipc(todos_by_thread: &HashMap<String, Vec<TodoItem>>) -> bool {
    let Ok(todos_json) = serde_json::to_string(todos_by_thread) else {
        return false;
    };

    amux_protocol::daemon_message_fits_ipc(&amux_protocol::DaemonMessage::AgentTodoList {
        todos_json,
    })
}

fn work_context_detail_frame_fits_ipc(thread_id: &str, context: &ThreadWorkContext) -> bool {
    let Ok(context_json) = serde_json::to_string(context) else {
        return false;
    };

    amux_protocol::daemon_message_fits_ipc(&amux_protocol::DaemonMessage::AgentWorkContextDetail {
        thread_id: thread_id.to_string(),
        context_json,
    })
}

fn cap_todos_for_ipc(thread_id: &str, todos: Vec<TodoItem>) -> (Vec<TodoItem>, bool) {
    if todo_detail_frame_fits_ipc(thread_id, &todos) {
        return (todos, false);
    }

    let mut low = 0usize;
    let mut high = todos.len();
    while low < high {
        let mid = low + (high - low) / 2;
        if todo_detail_frame_fits_ipc(thread_id, &todos[mid..]) {
            high = mid;
        } else {
            low = mid + 1;
        }
    }

    (todos.into_iter().skip(low).collect(), true)
}

fn cap_todo_list_for_ipc(
    todos_by_thread: HashMap<String, Vec<TodoItem>>,
) -> (HashMap<String, Vec<TodoItem>>, bool) {
    let mut truncated = false;
    let mut entries = Vec::with_capacity(todos_by_thread.len());
    for (thread_id, todos) in todos_by_thread {
        let (todos, todos_truncated) = cap_todos_for_ipc(&thread_id, todos);
        truncated |= todos_truncated;
        entries.push((thread_id, todos));
    }

    entries.sort_by(|(a_thread_id, a_todos), (b_thread_id, b_todos)| {
        let a_updated = a_todos
            .iter()
            .map(|todo| todo.updated_at.max(todo.created_at))
            .max()
            .unwrap_or(0);
        let b_updated = b_todos
            .iter()
            .map(|todo| todo.updated_at.max(todo.created_at))
            .max()
            .unwrap_or(0);
        b_updated
            .cmp(&a_updated)
            .then_with(|| a_thread_id.cmp(b_thread_id))
    });

    let to_map = |slice: &[(String, Vec<TodoItem>)]| {
        slice
            .iter()
            .cloned()
            .collect::<HashMap<String, Vec<TodoItem>>>()
    };

    let candidate = to_map(&entries);
    if todo_list_frame_fits_ipc(&candidate) {
        return (candidate, truncated);
    }

    let mut low = 0usize;
    let mut high = entries.len();
    while low < high {
        let mid = low + (high - low).div_ceil(2);
        let candidate = to_map(&entries[..mid]);
        if todo_list_frame_fits_ipc(&candidate) {
            low = mid;
        } else {
            high = mid - 1;
        }
    }

    (to_map(&entries[..low]), true)
}

fn cap_work_context_for_ipc(
    thread_id: &str,
    context: ThreadWorkContext,
) -> (ThreadWorkContext, bool) {
    if work_context_detail_frame_fits_ipc(thread_id, &context) {
        return (context, false);
    }

    let entries = context.entries;
    let mut low = 0usize;
    let mut high = entries.len();
    while low < high {
        let mid = low + (high - low) / 2;
        let candidate = ThreadWorkContext {
            thread_id: thread_id.to_string(),
            entries: entries[mid..].to_vec(),
        };
        if work_context_detail_frame_fits_ipc(thread_id, &candidate) {
            high = mid;
        } else {
            low = mid + 1;
        }
    }

    (
        ThreadWorkContext {
            thread_id: thread_id.to_string(),
            entries: entries.into_iter().skip(low).collect(),
        },
        true,
    )
}

fn task_with_log_slice(task: &AgentTask, start_idx: usize) -> AgentTask {
    let mut candidate = task.clone();
    if start_idx >= candidate.logs.len() {
        candidate.logs.clear();
        return candidate;
    }
    candidate.logs = candidate.logs[start_idx..].to_vec();
    candidate
}

fn cap_task_for_ipc(task: AgentTask) -> Option<(AgentTask, bool)> {
    if task_detail_frame_fits_ipc(&task) {
        return Some((task, false));
    }

    let mut low = 0usize;
    let mut high = task.logs.len();
    while low < high {
        let mid = low + (high - low) / 2;
        let candidate = task_with_log_slice(&task, mid);
        if task_detail_frame_fits_ipc(&candidate) {
            high = mid;
        } else {
            low = mid + 1;
        }
    }

    let candidate = task_with_log_slice(&task, low);
    if task_detail_frame_fits_ipc(&candidate) {
        return Some((candidate, low > 0));
    }

    None
}

fn cap_task_list_for_ipc(tasks: Vec<AgentTask>) -> (Vec<AgentTask>, bool) {
    if task_list_frame_fits_ipc(&tasks) {
        return (tasks, false);
    }

    let mut truncated = false;
    let mut capped = Vec::with_capacity(tasks.len());
    for task in tasks {
        if let Some((task, task_truncated)) = cap_task_for_ipc(task) {
            truncated |= task_truncated;
            capped.push(task);
        } else {
            truncated = true;
        }
    }

    if task_list_frame_fits_ipc(&capped) {
        return (capped, truncated);
    }

    let mut low = 0usize;
    let mut high = capped.len();
    while low < high {
        let mid = low + (high - low + 1) / 2;
        if task_list_frame_fits_ipc(&capped[..mid]) {
            low = mid;
        } else {
            high = mid - 1;
        }
    }

    (capped.into_iter().take(low).collect(), true)
}

fn goal_run_with_step_slice(goal_run: &GoalRun, start_idx: usize) -> GoalRun {
    let mut candidate = goal_run.clone();
    if start_idx >= candidate.steps.len() {
        candidate.steps.clear();
        candidate.current_step_index = 0;
        candidate.current_step_title = None;
        candidate.current_step_kind = None;
        candidate.active_task_id = None;
        return candidate;
    }

    candidate.steps = candidate.steps[start_idx..].to_vec();
    let current_idx = goal_run
        .current_step_index
        .saturating_sub(start_idx)
        .min(candidate.steps.len().saturating_sub(1));
    candidate.current_step_index = current_idx;
    candidate.current_step_title = candidate
        .steps
        .get(current_idx)
        .map(|step| step.title.clone());
    candidate.current_step_kind = candidate
        .steps
        .get(current_idx)
        .map(|step| step.kind.clone());
    candidate.active_task_id = candidate
        .steps
        .get(current_idx)
        .and_then(|step| step.task_id.clone());
    candidate
}

fn goal_run_stripped_summary(goal_run: &GoalRun) -> GoalRun {
    let mut candidate = goal_run.clone();
    candidate.events.clear();
    candidate.steps.clear();
    candidate.current_step_index = 0;
    candidate.current_step_title = None;
    candidate.current_step_kind = None;
    candidate.active_task_id = None;
    candidate.plan_summary = None;
    candidate.reflection_summary = None;
    candidate.memory_updates.clear();
    candidate.last_error = None;
    candidate.failure_cause = None;
    candidate
}

fn cap_goal_run_for_ipc(goal_run: GoalRun) -> Option<(GoalRun, bool)> {
    if goal_run_detail_frame_fits_ipc(&Some(goal_run.clone())) {
        return Some((goal_run, false));
    }

    let mut candidate = goal_run.clone();
    if !candidate.events.is_empty() {
        let mut low = 0usize;
        let mut high = candidate.events.len();
        while low < high {
            let mid = low + (high - low) / 2;
            let mut trial = candidate.clone();
            trial.events = trial.events[mid..].to_vec();
            if goal_run_detail_frame_fits_ipc(&Some(trial)) {
                high = mid;
            } else {
                low = mid + 1;
            }
        }
        candidate.events = candidate.events[low..].to_vec();
    }
    if goal_run_detail_frame_fits_ipc(&Some(candidate.clone())) {
        return Some((candidate, true));
    }

    if !candidate.steps.is_empty() {
        let max_prefix_drop = goal_run
            .current_step_index
            .min(goal_run.steps.len().saturating_sub(1));
        let mut low = 0usize;
        let mut high = max_prefix_drop;
        while low < high {
            let mid = low + (high - low) / 2;
            let trial = goal_run_with_step_slice(&candidate, mid);
            if goal_run_detail_frame_fits_ipc(&Some(trial)) {
                high = mid;
            } else {
                low = mid + 1;
            }
        }

        candidate = goal_run_with_step_slice(&candidate, low);
        if goal_run_detail_frame_fits_ipc(&Some(candidate.clone())) {
            return Some((candidate, true));
        }

        candidate = goal_run_with_step_slice(
            &candidate,
            candidate
                .current_step_index
                .min(candidate.steps.len().saturating_sub(1)),
        );
        if goal_run_detail_frame_fits_ipc(&Some(candidate.clone())) {
            return Some((candidate, true));
        }
    }

    candidate = goal_run_stripped_summary(&candidate);
    if goal_run_detail_frame_fits_ipc(&Some(candidate.clone())) {
        return Some((candidate, true));
    }

    None
}

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
                    cost: None,
                    provider: None,
                    model: None,
                    api_transport: None,
                    response_id: None,
                    upstream_message: None,
                    provider_final_result: None,
                    author_agent_id: None,
                    author_agent_name: None,
                    reasoning: None,
                    message_kind: AgentMessageKind::Normal,
                    compaction_strategy: None,
                    compaction_payload: None,
                    offloaded_payload_id: None,
                    structural_refs: Vec::new(),
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
        let adaptation_mode = {
            let model = self.operator_model.read().await;
            SatisfactionAdaptationMode::from_label(&model.operator_satisfaction.label)
        };
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
            max_replans: adaptation_mode.max_goal_replans(2),
            plan_summary: None,
            reflection_summary: None,
            memory_updates: Vec::new(),
            generated_skill_path: None,
            last_error: None,
            failure_cause: None,
            awaiting_approval_id: None,
            policy_fingerprint: None,
            approval_expires_at: None,
            containment_scope: None,
            compensation_status: None,
            compensation_summary: None,
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
        self.list_goal_runs_capped_for_ipc().await.0
    }

    pub(crate) async fn list_goal_runs_capped_for_ipc(&self) -> (Vec<GoalRun>, bool) {
        let goal_runs = self.goal_runs.lock().await;
        let mut items: Vec<GoalRun> = goal_runs.iter().cloned().collect();
        drop(goal_runs);
        let mut projected = Vec::with_capacity(items.len());
        for goal_run in items.drain(..) {
            projected.push(self.project_goal_run(goal_run).await);
        }
        projected.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));

        if goal_run_list_frame_fits_ipc(&projected) {
            return (projected, false);
        }

        let mut capped = Vec::with_capacity(projected.len());
        for goal_run in projected {
            if let Some((goal_run, _goal_truncated)) = cap_goal_run_for_ipc(goal_run) {
                capped.push(goal_run);
            }
        }

        if goal_run_list_frame_fits_ipc(&capped) {
            return (capped, true);
        }

        let mut low = 0usize;
        let mut high = capped.len();
        while low < high {
            let mid = low + (high - low + 1) / 2;
            if goal_run_list_frame_fits_ipc(&capped[..mid]) {
                low = mid;
            } else {
                high = mid - 1;
            }
        }

        (capped.into_iter().take(low).collect(), true)
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

    pub(crate) async fn get_goal_run_capped_for_ipc(
        &self,
        goal_run_id: &str,
    ) -> Option<(GoalRun, bool)> {
        let goal_run = self.get_goal_run(goal_run_id).await?;
        cap_goal_run_for_ipc(goal_run)
    }

    pub async fn list_todos(&self) -> HashMap<String, Vec<TodoItem>> {
        self.thread_todos.read().await.clone()
    }

    pub(crate) async fn list_tasks_capped_for_ipc(&self) -> (Vec<AgentTask>, bool) {
        cap_task_list_for_ipc(self.snapshot_tasks().await)
    }

    pub(crate) async fn list_todos_capped_for_ipc(&self) -> (HashMap<String, Vec<TodoItem>>, bool) {
        cap_todo_list_for_ipc(self.list_todos().await)
    }

    pub async fn get_todos(&self, thread_id: &str) -> Vec<TodoItem> {
        self.thread_todos
            .read()
            .await
            .get(thread_id)
            .cloned()
            .unwrap_or_default()
    }

    pub(crate) async fn get_todos_capped_for_ipc(&self, thread_id: &str) -> (Vec<TodoItem>, bool) {
        cap_todos_for_ipc(thread_id, self.get_todos(thread_id).await)
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

    pub(crate) async fn get_work_context_capped_for_ipc(
        &self,
        thread_id: &str,
    ) -> (ThreadWorkContext, bool) {
        cap_work_context_for_ipc(thread_id, self.get_work_context(thread_id).await)
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
