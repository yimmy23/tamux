use crate::state::modal;
use tokio::sync::mpsc::unbounded_channel;
use zorai_protocol::{
    WorkspaceActor, WorkspaceNotice, WorkspacePriority, WorkspaceTask, WorkspaceTaskStatus,
    WorkspaceTaskType,
};

fn task() -> WorkspaceTask {
    WorkspaceTask {
        id: "task-1".to_string(),
        workspace_id: "main".to_string(),
        title: "Ship workspace".to_string(),
        task_type: WorkspaceTaskType::Thread,
        description: "Build the board".to_string(),
        definition_of_done: Some("Tests pass".to_string()),
        priority: WorkspacePriority::High,
        status: WorkspaceTaskStatus::InReview,
        sort_order: 1,
        reporter: WorkspaceActor::User,
        assignee: Some(WorkspaceActor::Agent("swarog".to_string())),
        reviewer: Some(WorkspaceActor::User),
        thread_id: Some("workspace-thread:task-1".to_string()),
        goal_run_id: None,
        runtime_history: Vec::new(),
        created_at: 1,
        updated_at: 2,
        started_at: Some(3),
        completed_at: None,
        deleted_at: None,
        last_notice_id: Some("notice-1".to_string()),
    }
}

fn notice() -> WorkspaceNotice {
    WorkspaceNotice {
        id: "notice-1".to_string(),
        workspace_id: "main".to_string(),
        task_id: "task-1".to_string(),
        notice_type: "review_failed".to_string(),
        message: "Needs tighter tests".to_string(),
        actor: Some(WorkspaceActor::User),
        created_at: 4,
    }
}

#[test]
fn model_opens_workspace_detail_modal_with_task_and_notices() {
    let (_event_tx, event_rx) = std::sync::mpsc::channel();
    let (daemon_tx, _daemon_rx) = unbounded_channel();
    let mut model = crate::app::TuiModel::new(event_rx, daemon_tx);
    model.workspace.set_tasks("main".to_string(), vec![task()]);
    model.workspace.set_notices(vec![notice()]);

    model.open_workspace_detail_modal("task-1".to_string());

    assert_eq!(
        model.modal.top(),
        Some(modal::ModalKind::WorkspaceTaskDetail)
    );
    let body = model.workspace_detail_modal_body();
    assert!(body.contains("Ship workspace"));
    assert!(body.contains("Definition of done: Tests pass"));
    assert!(body.contains("review_failed: Needs tighter tests"));
}
