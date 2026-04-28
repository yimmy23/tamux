use crossterm::event::{KeyCode, KeyModifiers};

use crate::state::modal;

impl super::TuiModel {
    pub(super) fn open_workspace_detail_modal(&mut self, task_id: String) {
        if self.workspace.task_by_id(&task_id).is_none() {
            self.status_line = "Workspace task not found".to_string();
            return;
        }
        self.pending_workspace_detail_task_id = Some(task_id);
        self.modal.reduce(modal::ModalAction::Push(
            modal::ModalKind::WorkspaceTaskDetail,
        ));
        self.status_line = "Workspace task detail".to_string();
    }

    pub(super) fn workspace_detail_modal_body(&self) -> String {
        let Some(task_id) = self.pending_workspace_detail_task_id.as_deref() else {
            return "No workspace task selected".to_string();
        };
        self.workspace
            .task_detail_body(task_id)
            .unwrap_or_else(|| "Workspace task not found".to_string())
    }

    pub(super) fn handle_workspace_detail_modal_key(
        &mut self,
        code: KeyCode,
        _modifiers: KeyModifiers,
    ) -> bool {
        match code {
            KeyCode::Esc | KeyCode::Enter => self.close_top_modal(),
            _ => {}
        }
        false
    }
}
