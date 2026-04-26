use super::workspace_edit_modal::*;
use crate::state::{modal, DaemonCommand};
use amux_protocol::{
    WorkspaceActor, WorkspacePriority, WorkspaceTask, WorkspaceTaskStatus, WorkspaceTaskType,
};
use tokio::sync::mpsc::unbounded_channel;

fn task() -> WorkspaceTask {
    WorkspaceTask {
        id: "task-1".to_string(),
        workspace_id: "main".to_string(),
        title: "Old title".to_string(),
        task_type: WorkspaceTaskType::Thread,
        description: "Old description".to_string(),
        definition_of_done: Some("Old dod".to_string()),
        priority: WorkspacePriority::Low,
        status: WorkspaceTaskStatus::Todo,
        sort_order: 1,
        reporter: WorkspaceActor::User,
        assignee: None,
        reviewer: Some(WorkspaceActor::User),
        thread_id: Some("workspace-thread:task-1".to_string()),
        goal_run_id: None,
        runtime_history: Vec::new(),
        created_at: 1,
        updated_at: 1,
        started_at: None,
        completed_at: None,
        deleted_at: None,
        last_notice_id: None,
    }
}

#[test]
fn edit_form_loads_task_and_builds_update() {
    let mut form = WorkspaceEditForm::from_task(&task());
    form.title = "New title".to_string();
    form.description = "New description".to_string();
    form.definition_of_done.clear();
    form.priority = WorkspacePriority::Urgent;

    let update = form.to_update().expect("valid update");

    assert_eq!(form.task_id, "task-1");
    assert_eq!(update.title, Some("New title".to_string()));
    assert_eq!(update.description, Some("New description".to_string()));
    assert_eq!(update.definition_of_done, Some(None));
    assert_eq!(update.priority, Some(WorkspacePriority::Urgent));
}

#[test]
fn edit_form_save_preserves_assignee_and_reviewer() {
    let mut task = task();
    task.assignee = Some(WorkspaceActor::Agent("svarog".to_string()));
    task.reviewer = Some(WorkspaceActor::Subagent("qa".to_string()));

    let update = WorkspaceEditForm::from_task(&task)
        .to_update()
        .expect("valid update");

    assert_eq!(
        update.assignee,
        Some(Some(WorkspaceActor::Agent("svarog".to_string())))
    );
    assert_eq!(
        update.reviewer,
        Some(Some(WorkspaceActor::Subagent("qa".to_string())))
    );
}

#[test]
fn edit_form_requires_title_and_description() {
    let mut form = WorkspaceEditForm::from_task(&task());

    form.title.clear();
    assert_eq!(form.to_update().unwrap_err(), "Title is required");

    form.title = "Title".to_string();
    form.description.clear();
    assert_eq!(form.to_update().unwrap_err(), "Description is required");
}

#[test]
fn model_submits_edit_modal_as_update_command() {
    let (_event_tx, event_rx) = std::sync::mpsc::channel();
    let (daemon_tx, mut daemon_rx) = unbounded_channel();
    let mut model = crate::app::TuiModel::new(event_rx, daemon_tx);

    model.open_workspace_edit_modal_for_task(task());
    assert_eq!(model.modal.top(), Some(modal::ModalKind::WorkspaceEditTask));
    let form = model.pending_workspace_edit_form.as_mut().expect("form");
    form.title = "Edited".to_string();
    form.priority = WorkspacePriority::High;

    model.submit_workspace_edit_modal();

    match daemon_rx.try_recv().expect("update command") {
        DaemonCommand::UpdateWorkspaceTask { task_id, update } => {
            assert_eq!(task_id, "task-1");
            assert_eq!(update.title, Some("Edited".to_string()));
            assert_eq!(update.priority, Some(WorkspacePriority::High));
        }
        other => panic!("unexpected command: {other:?}"),
    }
    assert_eq!(model.modal.top(), None);
}

#[test]
fn edit_modal_reviewer_picker_selection_is_saved() {
    let (_event_tx, event_rx) = std::sync::mpsc::channel();
    let (daemon_tx, mut daemon_rx) = unbounded_channel();
    let mut model = crate::app::TuiModel::new(event_rx, daemon_tx);

    model.open_workspace_edit_modal_for_task(task());
    model
        .pending_workspace_edit_form
        .as_mut()
        .expect("form")
        .field = WorkspaceEditField::Reviewer;

    model.handle_workspace_edit_modal_key(
        crossterm::event::KeyCode::Enter,
        crossterm::event::KeyModifiers::NONE,
    );
    assert_eq!(
        model.modal.top(),
        Some(modal::ModalKind::WorkspaceActorPicker)
    );

    let options = crate::app::workspace_actor_picker::workspace_actor_picker_options(
        crate::app::workspace_actor_picker::WorkspaceActorPickerMode::Reviewer,
        &model.subagents,
    );
    let svarog_index = options
        .iter()
        .position(|option| option.label == "svarog")
        .expect("svarog reviewer option");
    model
        .modal
        .reduce(modal::ModalAction::Navigate(svarog_index as i32));
    model.submit_workspace_actor_picker();

    assert_eq!(model.modal.top(), Some(modal::ModalKind::WorkspaceEditTask));
    assert_eq!(
        model
            .pending_workspace_edit_form
            .as_ref()
            .expect("form")
            .reviewer,
        Some(WorkspaceActor::Agent(
            amux_protocol::AGENT_ID_SWAROG.to_string()
        ))
    );

    model
        .pending_workspace_edit_form
        .as_mut()
        .expect("form")
        .field = WorkspaceEditField::Submit;
    model.submit_workspace_edit_modal();

    match daemon_rx.try_recv().expect("update command") {
        DaemonCommand::UpdateWorkspaceTask { task_id, update } => {
            assert_eq!(task_id, "task-1");
            assert_eq!(
                update.reviewer,
                Some(Some(WorkspaceActor::Agent(
                    amux_protocol::AGENT_ID_SWAROG.to_string()
                )))
            );
        }
        other => panic!("unexpected command: {other:?}"),
    }
}
