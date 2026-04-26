use super::workspace_support::*;
use super::*;
use amux_protocol::{
    WorkspaceActor, WorkspaceTask, WorkspaceTaskRuntimeHistoryEntry, WorkspaceTaskStatus,
    WorkspaceTaskType, AGENT_ID_SWAROG,
};

struct WorkspaceTaskRunStart {
    task: WorkspaceTask,
    assignee: WorkspaceActor,
    previous_status: WorkspaceTaskStatus,
    previous_sort_order: i64,
    previous_started_at: Option<u64>,
}

impl AgentEngine {
    pub async fn run_workspace_task(&self, task_id: &str) -> Result<WorkspaceTask> {
        let start = self.begin_workspace_task_run(task_id).await?;
        self.finish_workspace_task_run(start).await
    }

    pub async fn run_workspace_task_deferred(
        self: &Arc<Self>,
        task_id: &str,
    ) -> Result<WorkspaceTask> {
        let start = self.begin_workspace_task_run(task_id).await?;
        let started_task = start.task.clone();
        let started_task_id = started_task.id.clone();
        let engine = Arc::clone(self);
        tokio::spawn(async move {
            if let Err(error) = engine.finish_workspace_task_run(start).await {
                tracing::warn!(
                    task_id = %started_task_id,
                    error = %error,
                    "deferred workspace task launch failed"
                );
            }
        });
        Ok(started_task)
    }

    async fn begin_workspace_task_run(&self, task_id: &str) -> Result<WorkspaceTaskRunStart> {
        let Some(mut task) = self.history.get_workspace_task(task_id).await? else {
            anyhow::bail!("workspace task not found");
        };
        let Some(assignee) = task.assignee.clone() else {
            anyhow::bail!("workspace task cannot run without an assignee");
        };
        let now = now_millis();
        let previous_status = task.status;
        let previous_sort_order = task.sort_order;
        let previous_started_at = task.started_at;
        task.status = WorkspaceTaskStatus::InProgress;
        if previous_status != WorkspaceTaskStatus::InProgress {
            task.sort_order = self
                .next_workspace_sort_order(
                    &task.workspace_id,
                    WorkspaceTaskStatus::InProgress,
                    Some(&task.id),
                )
                .await?;
        }
        task.started_at = task.started_at.or(Some(now));
        task.updated_at = now;
        self.history.upsert_workspace_task(&task).await?;
        self.insert_workspace_notice(
            &task.workspace_id,
            &task.id,
            "run_started",
            &format!("Workspace task run started for {}", actor_label(&assignee)),
            Some(WorkspaceActor::Agent(AGENT_ID_SWAROG.to_string())),
        )
        .await?;
        self.broadcast_workspace_task_by_id(&task.id).await?;
        self.sync_workspace_mirror(&task.workspace_id).await?;

        Ok(WorkspaceTaskRunStart {
            task,
            assignee,
            previous_status,
            previous_sort_order,
            previous_started_at,
        })
    }

    async fn finish_workspace_task_run(
        &self,
        start: WorkspaceTaskRunStart,
    ) -> Result<WorkspaceTask> {
        let WorkspaceTaskRunStart {
            task,
            assignee,
            previous_status,
            previous_sort_order,
            previous_started_at,
        } = start;
        let started_task = task.clone();

        let run_result = match task.task_type {
            WorkspaceTaskType::Thread => self.start_workspace_thread_task(task, &assignee).await,
            WorkspaceTaskType::Goal => {
                let started_at = task.started_at.unwrap_or_else(now_millis);
                self.start_workspace_goal_task(task, &assignee, started_at)
                    .await
            }
        };
        let mut task = match run_result {
            Ok(task) => task,
            Err(error) => {
                let message = error.to_string();
                self.fail_workspace_task_run_start(
                    started_task,
                    previous_status,
                    previous_sort_order,
                    previous_started_at,
                    &message,
                )
                .await?;
                return Err(error);
            }
        };

        task.updated_at = now_millis();
        let entry = workspace_runtime_history_entry(&task, task.updated_at);
        upsert_workspace_runtime_history_entry(&mut task, entry);
        self.history.upsert_workspace_task(&task).await?;
        self.broadcast_workspace_task_update(&task);
        self.sync_workspace_mirror(&task.workspace_id).await?;
        Ok(task)
    }

    async fn start_workspace_thread_task(
        &self,
        mut task: WorkspaceTask,
        assignee: &WorkspaceActor,
    ) -> Result<WorkspaceTask> {
        let thread_id = task
            .thread_id
            .clone()
            .unwrap_or_else(|| reserved_thread_id(&task.id));
        let target = actor_target(assignee);
        let prompt = task_run_prompt(&task);
        let thread_id = self
            .send_message_with_session_surface_and_target(
                Some(&thread_id),
                None,
                &prompt,
                None,
                None,
                target.as_deref(),
            )
            .await?;
        task.thread_id = Some(thread_id);
        Ok(task)
    }

    async fn start_workspace_goal_task(
        &self,
        mut task: WorkspaceTask,
        assignee: &WorkspaceActor,
        now: u64,
    ) -> Result<WorkspaceTask> {
        let goal_run_id = self.queue_workspace_goal_run(&task, assignee, now).await?;
        task.goal_run_id = Some(goal_run_id);
        Ok(task)
    }

    pub(super) async fn next_workspace_sort_order(
        &self,
        workspace_id: &str,
        status: WorkspaceTaskStatus,
        exclude_task_id: Option<&str>,
    ) -> Result<i64> {
        Ok(self
            .history
            .max_workspace_task_sort_order(workspace_id, status, exclude_task_id)
            .await?
            .unwrap_or(0)
            + 1)
    }

    pub(super) async fn make_workspace_sort_slot(
        &self,
        workspace_id: &str,
        status: WorkspaceTaskStatus,
        moving_task_id: &str,
        sort_order: i64,
    ) -> Result<()> {
        let mut tasks = self
            .history
            .list_workspace_tasks(workspace_id, false)
            .await?;
        tasks.sort_by_key(|task| (task.sort_order, task.created_at));
        for mut task in tasks {
            if task.id == moving_task_id || task.status != status || task.sort_order < sort_order {
                continue;
            }
            task.sort_order += 1;
            task.updated_at = now_millis();
            self.history.upsert_workspace_task(&task).await?;
            self.broadcast_workspace_task_update(&task);
        }
        Ok(())
    }

    pub(super) async fn sync_workspace_task_runtime_states(
        &self,
        tasks: Vec<WorkspaceTask>,
    ) -> Result<Vec<WorkspaceTask>> {
        let mut synced = Vec::with_capacity(tasks.len());
        for task in tasks {
            synced.push(self.sync_workspace_task_runtime_state(task).await?);
        }
        Ok(synced)
    }

    pub(super) async fn sync_workspace_task_runtime_state(
        &self,
        mut task: WorkspaceTask,
    ) -> Result<WorkspaceTask> {
        task = self.backfill_workspace_runtime_history(task).await?;
        if task.deleted_at.is_some()
            || task.task_type != WorkspaceTaskType::Goal
            || task.status != WorkspaceTaskStatus::InProgress
        {
            return Ok(task);
        }
        let Some(goal_run_id) = task.goal_run_id.clone() else {
            return Ok(task);
        };
        let Some(goal_run) = self.history.get_goal_run(&goal_run_id).await? else {
            return Ok(task);
        };
        let (next_status, notice_type, notice_message) = match goal_run.status {
            GoalRunStatus::Completed => {
                self.ensure_workspace_goal_completion_notice(&task, &goal_run)
                    .await?;
                return self
                    .complete_workspace_task_runtime_success(task, "Workspace goal completed")
                    .await;
            }
            GoalRunStatus::Failed => (
                WorkspaceTaskStatus::InProgress,
                "runtime_failed",
                goal_run
                    .last_error
                    .clone()
                    .unwrap_or_else(|| "Workspace goal failed".to_string()),
            ),
            GoalRunStatus::Cancelled => (
                WorkspaceTaskStatus::InProgress,
                "runtime_cancelled",
                "Workspace goal was cancelled".to_string(),
            ),
            _ => return Ok(task),
        };
        if task.status == next_status && notice_type != "runtime_completed" {
            let notices = self
                .history
                .list_workspace_notices(&task.workspace_id, Some(&task.id))
                .await?;
            if notices
                .iter()
                .any(|notice| notice.notice_type == notice_type && notice.message == notice_message)
            {
                return Ok(task);
            }
        }
        task.status = next_status;
        task.updated_at = now_millis();
        if task.status == WorkspaceTaskStatus::Done {
            task.completed_at = Some(task.updated_at);
        }
        if matches!(
            task.status,
            WorkspaceTaskStatus::InReview | WorkspaceTaskStatus::Done
        ) {
            task.sort_order = self
                .next_workspace_sort_order(&task.workspace_id, task.status.clone(), Some(&task.id))
                .await?;
        }
        self.history.upsert_workspace_task(&task).await?;
        self.insert_workspace_notice(
            &task.workspace_id,
            &task.id,
            notice_type,
            &notice_message,
            Some(WorkspaceActor::Agent(AGENT_ID_SWAROG.to_string())),
        )
        .await?;
        self.maybe_request_workspace_review(&task).await?;
        self.broadcast_workspace_task_by_id(&task.id).await?;
        self.sync_workspace_mirror(&task.workspace_id).await?;
        Ok(task)
    }

    async fn backfill_workspace_runtime_history(
        &self,
        mut task: WorkspaceTask,
    ) -> Result<WorkspaceTask> {
        if task.deleted_at.is_some() || task.status == WorkspaceTaskStatus::Todo {
            return Ok(task);
        }
        if task.thread_id.is_none() && task.goal_run_id.is_none() {
            return Ok(task);
        }
        let has_active_runtime = task.runtime_history.iter().any(|entry| {
            (task.thread_id.is_some() && entry.thread_id == task.thread_id)
                || (task.goal_run_id.is_some() && entry.goal_run_id == task.goal_run_id)
        });
        if has_active_runtime {
            return Ok(task);
        }
        let recorded_at = task.started_at.unwrap_or(task.updated_at);
        let entry = workspace_runtime_history_entry(&task, recorded_at);
        upsert_workspace_runtime_history_entry(&mut task, entry);
        task.updated_at = now_millis();
        self.history.upsert_workspace_task(&task).await?;
        Ok(task)
    }

    async fn ensure_workspace_goal_completion_notice(
        &self,
        task: &WorkspaceTask,
        goal_run: &GoalRun,
    ) -> Result<()> {
        let notices = self
            .history
            .list_workspace_notices(&task.workspace_id, Some(&task.id))
            .await?;
        if notices
            .iter()
            .any(|notice| notice.notice_type == "task_completion")
        {
            return Ok(());
        }
        let mut summary = format!(
            "Workspace goal completed automatically for goal run {}.",
            goal_run.id
        );
        if let Some(reflection) = goal_run
            .reflection_summary
            .as_deref()
            .or(goal_run.plan_summary.as_deref())
            .map(str::trim)
            .filter(|summary| !summary.is_empty())
        {
            summary.push_str("\n\nGoal summary:\n");
            summary.push_str(reflection);
        }
        self.insert_workspace_notice(
            &task.workspace_id,
            &task.id,
            "task_completion",
            &summary,
            Some(
                task.assignee
                    .clone()
                    .unwrap_or_else(|| WorkspaceActor::Agent(AGENT_ID_SWAROG.to_string())),
            ),
        )
        .await?;
        Ok(())
    }

    pub(super) async fn complete_workspace_task_runtime_success(
        &self,
        mut task: WorkspaceTask,
        message: &str,
    ) -> Result<WorkspaceTask> {
        task.status = if task.reviewer.is_some() {
            WorkspaceTaskStatus::InReview
        } else {
            WorkspaceTaskStatus::Done
        };
        task.updated_at = now_millis();
        task.completed_at = (task.status == WorkspaceTaskStatus::Done).then_some(task.updated_at);
        task.sort_order = self
            .next_workspace_sort_order(&task.workspace_id, task.status.clone(), Some(&task.id))
            .await?;
        self.history.upsert_workspace_task(&task).await?;
        let review_suffix = if task.status == WorkspaceTaskStatus::InReview {
            "; review is pending"
        } else {
            ""
        };
        self.insert_workspace_notice(
            &task.workspace_id,
            &task.id,
            "runtime_completed",
            &format!("{message}{review_suffix}"),
            Some(WorkspaceActor::Agent(AGENT_ID_SWAROG.to_string())),
        )
        .await?;
        self.maybe_request_workspace_review(&task).await?;
        self.broadcast_workspace_task_by_id(&task.id).await?;
        self.sync_workspace_mirror(&task.workspace_id).await?;
        Ok(task)
    }

    pub(super) async fn fail_workspace_task_run_start(
        &self,
        mut task: WorkspaceTask,
        previous_status: WorkspaceTaskStatus,
        previous_sort_order: i64,
        previous_started_at: Option<u64>,
        message: &str,
    ) -> Result<WorkspaceTask> {
        task.status = previous_status;
        task.sort_order = previous_sort_order;
        task.started_at = previous_started_at;
        task.completed_at = None;
        task.updated_at = now_millis();
        self.history.upsert_workspace_task(&task).await?;
        self.insert_workspace_notice(
            &task.workspace_id,
            &task.id,
            "run_failed",
            message,
            Some(WorkspaceActor::Agent(AGENT_ID_SWAROG.to_string())),
        )
        .await?;
        self.broadcast_workspace_task_by_id(&task.id).await?;
        self.sync_workspace_mirror(&task.workspace_id).await?;
        Ok(task)
    }

    pub(super) async fn maybe_request_workspace_review(&self, task: &WorkspaceTask) -> Result<()> {
        if task.status != WorkspaceTaskStatus::InReview {
            return Ok(());
        }
        let Some(reviewer) = task.reviewer.clone() else {
            return Ok(());
        };
        let review_task_id = match reviewer {
            WorkspaceActor::Agent(_) | WorkspaceActor::Subagent(_) => {
                let delivery_summary = workspace_delivery_summary(task);
                let review_task = self
                    .enqueue_task(
                        format!("Review workspace task: {}", task.title),
                        format!(
                            "Your job is to review completion of workspace task {task_id}.\n\nWorkspace task id: {task_id}\nTitle: {title}\nAssignee: {assignee}\n\nOriginal description:\n{description}\n\nDefinition of done:\n{dod}\n\nAssignee delivery summary:\n{delivery_summary}\n\nReview the delivered thread/goal against the description and definition of done. Complete this review task by calling workspace_submit_review with task_id={task_id} and verdict pass or fail. That review submission is the workspace event handler: pass moves the original workspace task to done; fail moves it back to in-progress and records your concrete notices for the assignee. Do not call workspace_submit_completion for this review task.",
                            task_id = task.id,
                            title = task.title,
                            assignee = task
                                .assignee
                                .as_ref()
                                .map(actor_label)
                                .unwrap_or_else(|| "unassigned".to_string()),
                            description = task.description,
                            dod = task.definition_of_done.as_deref().unwrap_or("Not provided"),
                            delivery_summary = delivery_summary,
                        ),
                        "normal",
                        None,
                        None,
                        Vec::new(),
                        None,
                        "workspace_review",
                        None,
                        None,
                        task.thread_id.clone(),
                        actor_target(&reviewer),
                    )
                    .await;
                self.record_workspace_review_task_history(task, &review_task)
                    .await?;
                Some(review_task.id)
            }
            WorkspaceActor::User => None,
        };
        let suffix = review_task_id
            .as_deref()
            .map(|task_id| format!("; queued review task {task_id}"))
            .unwrap_or_default();
        self.insert_workspace_notice(
            &task.workspace_id,
            &task.id,
            "review_requested",
            &format!(
                "Workspace task review requested from {}{}",
                actor_label(&reviewer),
                suffix
            ),
            Some(reviewer),
        )
        .await?;
        self.broadcast_workspace_task_by_id(&task.id).await?;
        Ok(())
    }

    async fn record_workspace_review_task_history(
        &self,
        task: &WorkspaceTask,
        review_task: &AgentTask,
    ) -> Result<()> {
        let Some(mut stored) = self.history.get_workspace_task(&task.id).await? else {
            return Ok(());
        };
        upsert_workspace_runtime_history_entry(
            &mut stored,
            WorkspaceTaskRuntimeHistoryEntry {
                task_type: WorkspaceTaskType::Thread,
                thread_id: review_task.thread_id.clone(),
                goal_run_id: review_task.goal_run_id.clone(),
                agent_task_id: Some(review_task.id.clone()),
                source: Some("workspace_review".to_string()),
                title: Some(review_task.title.clone()),
                review_path: None,
                review_feedback: None,
                archived_at: now_millis(),
            },
        );
        stored.updated_at = now_millis();
        self.history.upsert_workspace_task(&stored).await?;
        Ok(())
    }
}

fn workspace_runtime_history_entry(
    task: &WorkspaceTask,
    recorded_at: u64,
) -> WorkspaceTaskRuntimeHistoryEntry {
    WorkspaceTaskRuntimeHistoryEntry {
        task_type: task.task_type.clone(),
        thread_id: task.thread_id.clone(),
        goal_run_id: task.goal_run_id.clone(),
        agent_task_id: None,
        source: Some("workspace_runtime".to_string()),
        title: Some(task.title.clone()),
        review_path: None,
        review_feedback: None,
        archived_at: recorded_at,
    }
}

fn workspace_delivery_summary(task: &WorkspaceTask) -> String {
    let mut parts = Vec::new();
    match task.task_type {
        WorkspaceTaskType::Thread => {
            if let Some(thread_id) = task.thread_id.as_deref() {
                parts.push(format!(
                    "The assignee completed the workspace thread {thread_id}."
                ));
            } else {
                parts.push("The assignee completed the workspace thread.".to_string());
            }
        }
        WorkspaceTaskType::Goal => {
            if let Some(goal_run_id) = task.goal_run_id.as_deref() {
                parts.push(format!(
                    "The assignee completed the workspace goal run {goal_run_id}."
                ));
            } else {
                parts.push("The assignee completed the workspace goal run.".to_string());
            }
        }
    }
    if let Some(thread_id) = task.thread_id.as_deref() {
        parts.push(format!("Review thread: {thread_id}."));
    }
    if let Some(goal_run_id) = task.goal_run_id.as_deref() {
        parts.push(format!("Review goal run: {goal_run_id}."));
    }
    parts.join("\n")
}
