use super::workspace_create_workspace_modal::*;
use crate::state::{modal, DaemonCommand};
use tokio::sync::mpsc::unbounded_channel;
use zorai_protocol::WorkspaceOperator;

#[test]
fn new_workspace_form_defaults_to_user_operator() {
    let form = WorkspaceCreateForm::new();

    assert_eq!(form.workspace_id, "");
    assert_eq!(form.operator, WorkspaceOperator::User);
    assert_eq!(form.field, WorkspaceCreateField::WorkspaceId);
}

#[test]
fn command_palette_new_workspace_opens_workspace_creator_without_seeding_input() {
    let (_event_tx, event_rx) = std::sync::mpsc::channel();
    let (daemon_tx, mut daemon_rx) = unbounded_channel();
    let mut model = crate::app::TuiModel::new(event_rx, daemon_tx);

    model.execute_command("new-workspace");

    assert_eq!(model.modal.top(), Some(modal::ModalKind::WorkspaceCreate));
    let form = model.pending_workspace_create_workspace_form.as_ref().unwrap();
    assert_eq!(form.workspace_id, "");
    assert_eq!(form.operator, WorkspaceOperator::User);
    assert_eq!(model.input.buffer(), "");
    assert!(daemon_rx.try_recv().is_err());
}

#[test]
fn new_workspace_modal_submit_creates_workspace_with_operator_and_switches_to_it() {
    let (_event_tx, event_rx) = std::sync::mpsc::channel();
    let (daemon_tx, mut daemon_rx) = unbounded_channel();
    let mut model = crate::app::TuiModel::new(event_rx, daemon_tx);

    model.open_workspace_create_workspace_modal();
    let form = model
        .pending_workspace_create_workspace_form
        .as_mut()
        .unwrap();
    form.workspace_id = "client-a".to_string();
    form.operator = WorkspaceOperator::Svarog;

    model.submit_workspace_create_workspace_modal();

    assert_eq!(model.workspace.workspace_id(), "client-a");
    assert_eq!(model.modal.top(), None);
    match daemon_rx.try_recv().expect("set workspace operator command") {
        DaemonCommand::SetWorkspaceOperator {
            workspace_id,
            operator,
        } => {
            assert_eq!(workspace_id, "client-a");
            assert_eq!(operator, WorkspaceOperator::Svarog);
        }
        other => panic!("expected SetWorkspaceOperator, got {other:?}"),
    }
    match daemon_rx.try_recv().expect("workspace task refresh") {
        DaemonCommand::ListWorkspaceTasks { workspace_id, .. } => {
            assert_eq!(workspace_id, "client-a");
        }
        other => panic!("expected ListWorkspaceTasks, got {other:?}"),
    }
}
