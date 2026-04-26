use super::*;
use amux_protocol::{WorkspaceTaskStatus, WorkspaceTaskType};

impl AgentEngine {
    pub async fn complete_workspace_thread_task_by_thread_id(&self, thread_id: &str) -> Result<()> {
        let Some(task) = self
            .history
            .get_workspace_task_by_thread_id(thread_id)
            .await?
        else {
            return Ok(());
        };
        if task.task_type != WorkspaceTaskType::Thread
            || task.status != WorkspaceTaskStatus::InProgress
        {
            return Ok(());
        }
        self.complete_workspace_task_runtime_success(task, "Workspace thread completed")
            .await?;
        Ok(())
    }
}
