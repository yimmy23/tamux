use super::*;
use amux_protocol::{WorkspaceNotice, WorkspaceSettings, WorkspaceTask};

impl AgentEngine {
    pub(super) fn broadcast_workspace_settings(&self, settings: &WorkspaceSettings) {
        let _ = self.event_tx.send(AgentEvent::WorkspaceSettingsUpdate {
            settings: settings.clone(),
        });
    }

    pub(super) fn broadcast_workspace_task_update(&self, task: &WorkspaceTask) {
        let _ = self
            .event_tx
            .send(AgentEvent::WorkspaceTaskUpdate { task: task.clone() });
    }

    pub(super) async fn broadcast_workspace_task_by_id(&self, task_id: &str) -> Result<()> {
        if let Some(task) = self.history.get_workspace_task(task_id).await? {
            self.broadcast_workspace_task_update(&task);
        }
        Ok(())
    }

    pub(super) fn broadcast_workspace_task_deleted(&self, task_id: &str, deleted_at: Option<u64>) {
        let _ = self.event_tx.send(AgentEvent::WorkspaceTaskDeleted {
            task_id: task_id.to_string(),
            deleted_at,
        });
    }

    pub(super) fn broadcast_workspace_notice_update(&self, notice: &WorkspaceNotice) {
        let _ = self.event_tx.send(AgentEvent::WorkspaceNoticeUpdate {
            notice: notice.clone(),
        });
    }
}
