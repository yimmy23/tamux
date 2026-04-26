use super::*;
use amux_protocol::{WorkspaceActor, WorkspaceTask, WorkspaceTaskStatus, WorkspaceTaskType};

impl AgentEngine {
    pub async fn pause_workspace_task(&self, task_id: &str) -> Result<Option<WorkspaceTask>> {
        self.control_workspace_task(task_id, "pause").await
    }

    pub async fn stop_workspace_task(&self, task_id: &str) -> Result<Option<WorkspaceTask>> {
        self.control_workspace_task(task_id, "stop").await
    }

    async fn control_workspace_task(
        &self,
        task_id: &str,
        action: &str,
    ) -> Result<Option<WorkspaceTask>> {
        let Some(mut task) = self.history.get_workspace_task(task_id).await? else {
            return Ok(None);
        };
        let mut notice_type = action.to_string();
        let mut notice_message = format!("Workspace task {action} requested");
        match (task.task_type.clone(), action) {
            (WorkspaceTaskType::Goal, "pause") => {
                if let Some(goal_run_id) = task.goal_run_id.as_deref() {
                    self.control_goal_run(goal_run_id, "pause", None).await;
                }
            }
            (WorkspaceTaskType::Thread, "pause") => {
                notice_type = "pause_unsupported".to_string();
                notice_message =
                    "Workspace cannot pause thread tasks; stop it or let the thread finish."
                        .to_string();
            }
            (WorkspaceTaskType::Goal, "stop") => {
                if let Some(goal_run_id) = task.goal_run_id.as_deref() {
                    self.control_goal_run(goal_run_id, "cancel", None).await;
                }
                self.mark_workspace_task_stopped(&mut task).await?;
            }
            (WorkspaceTaskType::Thread, "stop") => {
                if let Some(thread_id) = task.thread_id.as_deref() {
                    self.stop_stream(thread_id).await;
                }
                self.mark_workspace_task_stopped(&mut task).await?;
            }
            _ => {}
        }
        task.updated_at = now_millis();
        self.history.upsert_workspace_task(&task).await?;
        self.insert_workspace_notice(
            &task.workspace_id,
            &task.id,
            &notice_type,
            &notice_message,
            Some(WorkspaceActor::User),
        )
        .await?;
        self.broadcast_workspace_task_by_id(&task.id).await?;
        self.sync_workspace_mirror(&task.workspace_id).await?;
        Ok(Some(task))
    }

    async fn mark_workspace_task_stopped(&self, task: &mut WorkspaceTask) -> Result<()> {
        task.status = WorkspaceTaskStatus::Todo;
        task.sort_order = self
            .next_workspace_sort_order(
                &task.workspace_id,
                WorkspaceTaskStatus::Todo,
                Some(&task.id),
            )
            .await?;
        task.started_at = None;
        task.completed_at = None;
        Ok(())
    }
}
