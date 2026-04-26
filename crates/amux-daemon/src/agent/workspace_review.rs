use super::workspace_support::*;
use super::*;
use amux_protocol::{
    WorkspaceActor, WorkspaceTask, WorkspaceTaskRuntimeHistoryEntry, WorkspaceTaskStatus,
    WorkspaceTaskType, AGENT_ID_SWAROG,
};

impl AgentEngine {
    pub(super) async fn restart_workspace_task_after_failed_review(
        &self,
        task: WorkspaceTask,
        feedback: &str,
    ) -> Result<WorkspaceTask> {
        let mut task = self
            .prepare_failed_review_rerun_task(task, feedback)
            .await?;
        self.history.upsert_workspace_task(&task).await?;
        self.insert_workspace_notice(
            &task.workspace_id,
            &task.id,
            "assignee_notified",
            &format!(
                "Failed review follow-up prepared for {}",
                task.assignee
                    .as_ref()
                    .map(actor_label)
                    .unwrap_or_else(|| "unassigned".to_string())
            ),
            Some(WorkspaceActor::Agent(AGENT_ID_SWAROG.to_string())),
        )
        .await?;
        task = self.run_workspace_task(&task.id).await?;
        Ok(task)
    }

    async fn prepare_failed_review_rerun_task(
        &self,
        mut task: WorkspaceTask,
        feedback: &str,
    ) -> Result<WorkspaceTask> {
        let now = now_millis();
        let review_path = self
            .write_failed_review_document(&task, feedback, now)
            .await?;
        if task.thread_id.is_some() || task.goal_run_id.is_some() {
            let entry = WorkspaceTaskRuntimeHistoryEntry {
                task_type: task.task_type.clone(),
                thread_id: task.thread_id.clone(),
                goal_run_id: task.goal_run_id.clone(),
                agent_task_id: None,
                source: Some("workspace_runtime".to_string()),
                title: Some(task.title.clone()),
                review_path: Some(review_path.clone()),
                review_feedback: Some(feedback.to_string()),
                archived_at: now,
            };
            upsert_workspace_runtime_history_entry(&mut task, entry);
        }
        let runtime_suffix = Uuid::new_v4().simple().to_string();
        match task.task_type {
            WorkspaceTaskType::Thread => {
                task.thread_id = Some(format!("workspace-thread:{}:{runtime_suffix}", task.id));
                task.goal_run_id = None;
            }
            WorkspaceTaskType::Goal => {
                task.thread_id = None;
                task.goal_run_id = Some(format!("workspace-goal:{}:{runtime_suffix}", task.id));
            }
        }
        task.status = WorkspaceTaskStatus::InProgress;
        task.completed_at = None;
        task.started_at = None;
        task.sort_order = self
            .next_workspace_sort_order(
                &task.workspace_id,
                WorkspaceTaskStatus::InProgress,
                Some(&task.id),
            )
            .await?;
        task.updated_at = now;
        Ok(task)
    }

    async fn write_failed_review_document(
        &self,
        task: &WorkspaceTask,
        feedback: &str,
        now: u64,
    ) -> Result<String> {
        let relative_path = format!("task-{}/failed-review.md", task.id);
        let path = workspace_root(&self.history, &task.workspace_id).join(&relative_path);
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        let runtime = match task.task_type {
            WorkspaceTaskType::Thread => task
                .thread_id
                .as_deref()
                .map(|thread_id| format!("thread_id: {thread_id}"))
                .unwrap_or_else(|| "thread_id: not recorded".to_string()),
            WorkspaceTaskType::Goal => task
                .goal_run_id
                .as_deref()
                .map(|goal_run_id| format!("goal_run_id: {goal_run_id}"))
                .unwrap_or_else(|| "goal_run_id: not recorded".to_string()),
        };
        let contents = format!(
            "# Failed Review\n\n- task_id: {}\n- title: {}\n- archived_at: {}\n- {}\n\n## Reviewer Feedback\n\n{}\n",
            task.id, task.title, now, runtime, feedback
        );
        tokio::fs::write(path, contents).await?;
        Ok(relative_path)
    }
}
