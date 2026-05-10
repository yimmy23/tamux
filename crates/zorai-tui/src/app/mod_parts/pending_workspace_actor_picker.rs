use super::*;
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PendingWorkspaceActorPicker {
    pub(crate) target: PendingWorkspaceActorPickerTarget,
    pub(crate) task_id: String,
    pub(crate) mode: workspace_actor_picker::WorkspaceActorPickerMode,
}
