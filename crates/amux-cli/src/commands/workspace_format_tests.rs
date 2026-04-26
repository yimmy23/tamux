use super::*;

fn task(id: &str, status: WorkspaceTaskStatus, title: &str) -> WorkspaceTask {
    WorkspaceTask {
        id: id.to_string(),
        workspace_id: "main".to_string(),
        title: title.to_string(),
        task_type: WorkspaceTaskType::Thread,
        description: "Description".to_string(),
        definition_of_done: None,
        priority: WorkspacePriority::Low,
        status,
        sort_order: 1,
        reporter: WorkspaceActor::User,
        assignee: Some(WorkspaceActor::Agent("svarog".to_string())),
        reviewer: Some(WorkspaceActor::User),
        thread_id: Some(format!("workspace-thread:{id}")),
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
fn workspace_task_list_formats_as_board_columns() {
    let output = format_workspace_task_list(
        &[
            task("todo-1", WorkspaceTaskStatus::Todo, "Plan work"),
            task("done-1", WorkspaceTaskStatus::Done, "Ship work"),
        ],
        false,
    )
    .expect("format workspace list");

    assert!(output.contains("TODO (1)"));
    assert!(output.contains("IN PROGRESS (0)"));
    assert!(output.contains("IN REVIEW (0)"));
    assert!(output.contains("DONE (1)"));
    assert!(output.contains("todo-1 low Plan work"));
    assert!(output.contains("done-1 low Ship work"));
}
