use super::workspace_support::*;
use super::*;
use amux_protocol::{
    WorkspaceActor, WorkspaceCompletionSubmission, WorkspaceNotice, WorkspaceOperator,
    WorkspaceReviewSubmission, WorkspaceReviewVerdict, WorkspaceSettings, WorkspaceTask,
    WorkspaceTaskCreate, WorkspaceTaskMove, WorkspaceTaskStatus, WorkspaceTaskType,
    WorkspaceTaskUpdate,
};

#[derive(Clone)]
enum WorkspaceAutoStart {
    Await,
    Defer(Arc<AgentEngine>),
}

impl AgentEngine {
    pub async fn get_or_create_workspace_settings(
        &self,
        workspace_id: &str,
    ) -> Result<WorkspaceSettings> {
        if let Some(settings) = self.history.get_workspace_settings(workspace_id).await? {
            return Ok(settings);
        }

        let now = now_millis();
        let settings = WorkspaceSettings {
            workspace_id: workspace_id.to_string(),
            workspace_root: Some(
                workspace_root(&self.history, workspace_id)
                    .to_string_lossy()
                    .into(),
            ),
            operator: WorkspaceOperator::User,
            created_at: now,
            updated_at: now,
        };
        self.history.upsert_workspace_settings(&settings).await?;
        self.sync_workspace_mirror(workspace_id).await?;
        self.broadcast_workspace_settings(&settings);
        Ok(settings)
    }

    pub async fn set_workspace_operator(
        &self,
        workspace_id: &str,
        operator: WorkspaceOperator,
    ) -> Result<WorkspaceSettings> {
        self.set_workspace_operator_internal(workspace_id, operator, WorkspaceAutoStart::Await)
            .await
    }

    pub async fn set_workspace_operator_deferred_auto_start(
        self: &Arc<Self>,
        workspace_id: &str,
        operator: WorkspaceOperator,
    ) -> Result<WorkspaceSettings> {
        self.set_workspace_operator_internal(
            workspace_id,
            operator,
            WorkspaceAutoStart::Defer(Arc::clone(self)),
        )
        .await
    }

    async fn set_workspace_operator_internal(
        &self,
        workspace_id: &str,
        operator: WorkspaceOperator,
        auto_start: WorkspaceAutoStart,
    ) -> Result<WorkspaceSettings> {
        let mut settings = self.get_or_create_workspace_settings(workspace_id).await?;
        settings.operator = operator;
        settings.updated_at = now_millis();
        self.history.upsert_workspace_settings(&settings).await?;
        self.broadcast_workspace_settings(&settings);
        if settings.operator == WorkspaceOperator::Svarog {
            match auto_start {
                WorkspaceAutoStart::Await => {
                    self.start_svarog_workspace_operator_tasks(workspace_id)
                        .await?;
                }
                WorkspaceAutoStart::Defer(engine) => {
                    let workspace_id = workspace_id.to_string();
                    tokio::spawn(async move {
                        if let Err(error) = engine
                            .start_svarog_workspace_operator_tasks_deferred(&workspace_id)
                            .await
                        {
                            tracing::warn!(
                                workspace_id = %workspace_id,
                                error = %error,
                                "deferred Svarog workspace operator auto-start failed"
                            );
                        }
                    });
                }
            }
        }
        self.sync_workspace_mirror(workspace_id).await?;
        Ok(settings)
    }

    pub(super) async fn start_svarog_workspace_operator_tasks(
        &self,
        workspace_id: &str,
    ) -> Result<()> {
        let tasks = self
            .history
            .list_workspace_tasks(workspace_id, false)
            .await?;
        for task in tasks
            .into_iter()
            .filter(|task| task.status == WorkspaceTaskStatus::Todo)
            .filter(|task| task.assignee.is_some())
        {
            self.run_workspace_task(&task.id).await?;
        }
        Ok(())
    }

    async fn start_svarog_workspace_operator_tasks_deferred(
        self: &Arc<Self>,
        workspace_id: &str,
    ) -> Result<()> {
        let tasks = self
            .history
            .list_workspace_tasks(workspace_id, false)
            .await?;
        for task in tasks
            .into_iter()
            .filter(|task| task.status == WorkspaceTaskStatus::Todo)
            .filter(|task| task.assignee.is_some())
        {
            self.run_workspace_task_deferred(&task.id).await?;
        }
        Ok(())
    }

    pub async fn create_workspace_task(
        &self,
        request: WorkspaceTaskCreate,
        reporter: WorkspaceActor,
    ) -> Result<WorkspaceTask> {
        self.create_workspace_task_internal(request, reporter, WorkspaceAutoStart::Await)
            .await
    }

    pub async fn create_workspace_task_deferred_auto_start(
        self: &Arc<Self>,
        request: WorkspaceTaskCreate,
        reporter: WorkspaceActor,
    ) -> Result<WorkspaceTask> {
        self.create_workspace_task_internal(
            request,
            reporter,
            WorkspaceAutoStart::Defer(Arc::clone(self)),
        )
        .await
    }

    async fn create_workspace_task_internal(
        &self,
        request: WorkspaceTaskCreate,
        reporter: WorkspaceActor,
        auto_start: WorkspaceAutoStart,
    ) -> Result<WorkspaceTask> {
        let title = request.title.trim();
        let description = request.description.trim();
        if title.is_empty() {
            anyhow::bail!("workspace task title is required");
        }
        if description.is_empty() {
            anyhow::bail!("workspace task description is required");
        }

        self.get_or_create_workspace_settings(&request.workspace_id)
            .await?;
        let sort_order = self
            .history
            .max_workspace_task_sort_order(&request.workspace_id, WorkspaceTaskStatus::Todo, None)
            .await?
            .unwrap_or(0)
            + 1;
        let now = now_millis();
        let id = format!("wtask_{}", Uuid::new_v4());
        let task_type = request.task_type;
        let task = WorkspaceTask {
            id: id.clone(),
            workspace_id: request.workspace_id.clone(),
            title: title.to_string(),
            task_type: task_type.clone(),
            description: description.to_string(),
            definition_of_done: request.definition_of_done,
            priority: request.priority.unwrap_or_default(),
            status: WorkspaceTaskStatus::Todo,
            sort_order,
            reporter,
            assignee: request.assignee,
            reviewer: request.reviewer,
            thread_id: (task_type == WorkspaceTaskType::Thread).then(|| reserved_thread_id(&id)),
            goal_run_id: (task_type == WorkspaceTaskType::Goal).then(|| reserved_goal_run_id(&id)),
            runtime_history: Vec::new(),
            created_at: now,
            updated_at: now,
            started_at: None,
            completed_at: None,
            deleted_at: None,
            last_notice_id: None,
        };
        self.history.upsert_workspace_task(&task).await?;
        self.insert_workspace_notice(
            &task.workspace_id,
            &task.id,
            "created",
            "Workspace task created",
            Some(task.reporter.clone()),
        )
        .await?;
        self.broadcast_workspace_task_by_id(&task.id).await?;
        if task.assignee.is_some()
            && self
                .workspace_operator_is_svarog(&task.workspace_id)
                .await?
        {
            match auto_start {
                WorkspaceAutoStart::Await => {
                    self.run_workspace_task(&task.id).await?;
                    return Ok(task);
                }
                WorkspaceAutoStart::Defer(engine) => {
                    let task_id = task.id.clone();
                    tokio::spawn(async move {
                        if let Err(error) = engine.run_workspace_task(&task_id).await {
                            tracing::warn!(
                                task_id = %task_id,
                                error = %error,
                                "deferred Svarog workspace task auto-start failed"
                            );
                        }
                    });
                    return Ok(task);
                }
            }
        }
        self.sync_workspace_mirror(&task.workspace_id).await?;
        Ok(task)
    }

    pub async fn list_workspace_tasks(
        &self,
        workspace_id: &str,
        include_deleted: bool,
    ) -> Result<Vec<WorkspaceTask>> {
        self.get_or_create_workspace_settings(workspace_id).await?;
        let tasks = self
            .history
            .list_workspace_tasks(workspace_id, include_deleted)
            .await?;
        self.sync_workspace_task_runtime_states(tasks).await
    }

    pub async fn get_workspace_task(&self, task_id: &str) -> Result<Option<WorkspaceTask>> {
        let Some(task) = self.history.get_workspace_task(task_id).await? else {
            return Ok(None);
        };
        Ok(Some(self.sync_workspace_task_runtime_state(task).await?))
    }

    pub async fn update_workspace_task(
        &self,
        task_id: &str,
        update: WorkspaceTaskUpdate,
    ) -> Result<Option<WorkspaceTask>> {
        self.update_workspace_task_internal(task_id, update, WorkspaceAutoStart::Await)
            .await
    }

    pub async fn update_workspace_task_deferred_auto_start(
        self: &Arc<Self>,
        task_id: &str,
        update: WorkspaceTaskUpdate,
    ) -> Result<Option<WorkspaceTask>> {
        self.update_workspace_task_internal(
            task_id,
            update,
            WorkspaceAutoStart::Defer(Arc::clone(self)),
        )
        .await
    }

    async fn update_workspace_task_internal(
        &self,
        task_id: &str,
        update: WorkspaceTaskUpdate,
        auto_start: WorkspaceAutoStart,
    ) -> Result<Option<WorkspaceTask>> {
        let Some(mut task) = self.history.get_workspace_task(task_id).await? else {
            return Ok(None);
        };
        if let Some(title) = update.title {
            let title = title.trim();
            if title.is_empty() {
                anyhow::bail!("workspace task title is required");
            }
            task.title = title.to_string();
        }
        if let Some(description) = update.description {
            let description = description.trim();
            if description.is_empty() {
                anyhow::bail!("workspace task description is required");
            }
            task.description = description.to_string();
        }
        if let Some(definition_of_done) = update.definition_of_done {
            task.definition_of_done = definition_of_done;
        }
        if let Some(priority) = update.priority {
            task.priority = priority;
        }
        if let Some(assignee) = update.assignee {
            task.assignee = assignee;
        }
        if let Some(reviewer) = update.reviewer {
            task.reviewer = reviewer;
        }
        let should_auto_start = task.status == WorkspaceTaskStatus::Todo
            && task.assignee.is_some()
            && self
                .workspace_operator_is_svarog(&task.workspace_id)
                .await?;
        task.updated_at = now_millis();
        self.history.upsert_workspace_task(&task).await?;
        self.insert_workspace_notice(
            &task.workspace_id,
            &task.id,
            "updated",
            "Workspace task updated",
            Some(WorkspaceActor::User),
        )
        .await?;
        self.broadcast_workspace_task_by_id(&task.id).await?;
        if should_auto_start {
            return match auto_start {
                WorkspaceAutoStart::Await => Ok(Some(self.run_workspace_task(&task.id).await?)),
                WorkspaceAutoStart::Defer(engine) => {
                    let task_id = task.id.clone();
                    tokio::spawn(async move {
                        if let Err(error) = engine.run_workspace_task(&task_id).await {
                            tracing::warn!(
                                task_id = %task_id,
                                error = %error,
                                "deferred Svarog workspace task update auto-start failed"
                            );
                        }
                    });
                    Ok(Some(task))
                }
            };
        }
        self.sync_workspace_mirror(&task.workspace_id).await?;
        Ok(Some(task))
    }

    async fn workspace_operator_is_svarog(&self, workspace_id: &str) -> Result<bool> {
        Ok(self
            .history
            .get_workspace_settings(workspace_id)
            .await?
            .is_some_and(|settings| settings.operator == WorkspaceOperator::Svarog))
    }

    pub async fn move_workspace_task(
        &self,
        request: WorkspaceTaskMove,
    ) -> Result<Option<WorkspaceTask>> {
        self.move_workspace_task_internal(request, WorkspaceAutoStart::Await)
            .await
    }

    pub async fn move_workspace_task_deferred_auto_start(
        self: &Arc<Self>,
        request: WorkspaceTaskMove,
    ) -> Result<Option<WorkspaceTask>> {
        self.move_workspace_task_internal(request, WorkspaceAutoStart::Defer(Arc::clone(self)))
            .await
    }

    async fn move_workspace_task_internal(
        &self,
        request: WorkspaceTaskMove,
        auto_start: WorkspaceAutoStart,
    ) -> Result<Option<WorkspaceTask>> {
        let Some(mut task) = self.history.get_workspace_task(&request.task_id).await? else {
            return Ok(None);
        };
        if task.status == WorkspaceTaskStatus::Todo
            && request.status == WorkspaceTaskStatus::InProgress
        {
            return match auto_start {
                WorkspaceAutoStart::Await => Ok(Some(self.run_workspace_task(&task.id).await?)),
                WorkspaceAutoStart::Defer(engine) => {
                    Ok(Some(engine.run_workspace_task_deferred(&task.id).await?))
                }
            };
        }
        let next_status = request.status;
        let sort_order = match request.sort_order {
            Some(sort_order) => {
                self.make_workspace_sort_slot(
                    &task.workspace_id,
                    next_status.clone(),
                    &task.id,
                    sort_order,
                )
                .await?;
                sort_order
            }
            None => {
                self.next_workspace_sort_order(
                    &task.workspace_id,
                    next_status.clone(),
                    Some(&task.id),
                )
                .await?
            }
        };
        task.status = next_status;
        task.sort_order = sort_order;
        task.updated_at = now_millis();
        task.completed_at = (task.status == WorkspaceTaskStatus::Done).then_some(task.updated_at);
        self.history.upsert_workspace_task(&task).await?;
        self.insert_workspace_notice(
            &task.workspace_id,
            &task.id,
            "moved",
            &format!("Workspace task moved to {:?}", task.status),
            Some(WorkspaceActor::User),
        )
        .await?;
        self.maybe_request_workspace_review(&task).await?;
        self.broadcast_workspace_task_by_id(&task.id).await?;
        self.sync_workspace_mirror(&task.workspace_id).await?;
        Ok(Some(task))
    }

    pub async fn delete_workspace_task(&self, task_id: &str) -> Result<Option<WorkspaceTask>> {
        let Some(mut task) = self.history.get_workspace_task(task_id).await? else {
            return Ok(None);
        };
        let now = now_millis();
        task.deleted_at = Some(now);
        task.updated_at = now;
        self.history.upsert_workspace_task(&task).await?;
        self.insert_workspace_notice(
            &task.workspace_id,
            &task.id,
            "deleted",
            "Workspace task soft-deleted",
            Some(WorkspaceActor::User),
        )
        .await?;
        self.broadcast_workspace_task_deleted(&task.id, task.deleted_at);
        self.sync_workspace_mirror(&task.workspace_id).await?;
        Ok(Some(task))
    }

    pub async fn submit_workspace_review(
        &self,
        review: WorkspaceReviewSubmission,
    ) -> Result<Option<WorkspaceTask>> {
        let Some(mut task) = self.history.get_workspace_task(&review.task_id).await? else {
            return Ok(None);
        };
        let failed = review.verdict == WorkspaceReviewVerdict::Fail;
        let (status, notice_type, default_message) = match review.verdict {
            WorkspaceReviewVerdict::Pass => (
                WorkspaceTaskStatus::Done,
                "review_passed",
                "Workspace task review passed",
            ),
            WorkspaceReviewVerdict::Fail => (
                WorkspaceTaskStatus::InProgress,
                "review_failed",
                "Workspace task review failed",
            ),
        };
        task.status = status;
        task.updated_at = now_millis();
        task.completed_at = (task.status == WorkspaceTaskStatus::Done).then_some(task.updated_at);
        self.history.upsert_workspace_task(&task).await?;
        self.insert_workspace_notice(
            &task.workspace_id,
            &task.id,
            notice_type,
            review.message.as_deref().unwrap_or(default_message),
            Some(WorkspaceActor::User),
        )
        .await?;
        if failed {
            let restarted = self
                .restart_workspace_task_after_failed_review(
                    task,
                    review.message.as_deref().unwrap_or(default_message),
                )
                .await?;
            self.broadcast_workspace_task_by_id(&restarted.id).await?;
            self.sync_workspace_mirror(&restarted.workspace_id).await?;
            return Ok(Some(restarted));
        }
        self.broadcast_workspace_task_by_id(&task.id).await?;
        self.sync_workspace_mirror(&task.workspace_id).await?;
        Ok(Some(task))
    }

    pub async fn submit_workspace_completion(
        &self,
        completion: WorkspaceCompletionSubmission,
        actor: WorkspaceActor,
    ) -> Result<Option<WorkspaceTask>> {
        let Some(task) = self.history.get_workspace_task(&completion.task_id).await? else {
            return Ok(None);
        };
        if task.deleted_at.is_some() {
            anyhow::bail!("workspace task is deleted");
        }
        if task.status != WorkspaceTaskStatus::InProgress {
            anyhow::bail!("workspace task completion can only be submitted for in-progress tasks");
        }
        let summary = completion.summary.trim();
        if summary.is_empty() {
            anyhow::bail!("workspace task completion summary is required");
        }
        self.insert_workspace_notice(
            &task.workspace_id,
            &task.id,
            "task_completion",
            summary,
            Some(actor),
        )
        .await?;
        Ok(Some(
            self.complete_workspace_task_runtime_success(
                task,
                &format!("Workspace task completion submitted: {summary}"),
            )
            .await?,
        ))
    }

    pub async fn list_workspace_notices(
        &self,
        workspace_id: &str,
        task_id: Option<&str>,
    ) -> Result<Vec<WorkspaceNotice>> {
        self.history
            .list_workspace_notices(workspace_id, task_id)
            .await
    }

    pub(super) async fn insert_workspace_notice(
        &self,
        workspace_id: &str,
        task_id: &str,
        notice_type: &str,
        message: &str,
        actor: Option<WorkspaceActor>,
    ) -> Result<WorkspaceNotice> {
        let notice = WorkspaceNotice {
            id: format!("wnotice_{}", Uuid::new_v4()),
            workspace_id: workspace_id.to_string(),
            task_id: task_id.to_string(),
            notice_type: notice_type.to_string(),
            message: message.to_string(),
            actor,
            created_at: now_millis(),
        };
        self.history.insert_workspace_notice(&notice).await?;
        if let Some(mut task) = self.history.get_workspace_task(task_id).await? {
            task.last_notice_id = Some(notice.id.clone());
            task.updated_at = notice.created_at;
            self.history.upsert_workspace_task(&task).await?;
        }
        self.broadcast_workspace_notice_update(&notice);
        Ok(notice)
    }

    pub(super) async fn sync_workspace_mirror(&self, workspace_id: &str) -> Result<()> {
        let settings = self
            .get_or_create_workspace_settings_no_sync(workspace_id)
            .await?;
        let tasks = self
            .history
            .list_workspace_tasks(workspace_id, true)
            .await?;
        let notices = self
            .history
            .list_workspace_notices(workspace_id, None)
            .await?;
        let root = workspace_root(&self.history, workspace_id);
        tokio::fs::create_dir_all(&root).await?;
        let mirror = WorkspaceMirror {
            schema_version: WORKSPACE_MIRROR_SCHEMA_VERSION,
            generated_at: now_millis(),
            settings: &settings,
            tasks: &tasks,
            notices: &notices,
        };
        let json = serde_json::to_string_pretty(&mirror)?;
        tokio::fs::write(root.join("workspace.json"), json).await?;
        Ok(())
    }

    async fn get_or_create_workspace_settings_no_sync(
        &self,
        workspace_id: &str,
    ) -> Result<WorkspaceSettings> {
        if let Some(settings) = self.history.get_workspace_settings(workspace_id).await? {
            return Ok(settings);
        }
        let now = now_millis();
        let settings = WorkspaceSettings {
            workspace_id: workspace_id.to_string(),
            workspace_root: Some(
                workspace_root(&self.history, workspace_id)
                    .to_string_lossy()
                    .into(),
            ),
            operator: WorkspaceOperator::User,
            created_at: now,
            updated_at: now,
        };
        self.history.upsert_workspace_settings(&settings).await?;
        Ok(settings)
    }
}
