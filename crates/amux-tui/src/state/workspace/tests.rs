use super::*;
use amux_protocol::{WorkspaceActor, WorkspaceTaskRuntimeHistoryEntry, WorkspaceTaskType};

fn task(id: &str, status: WorkspaceTaskStatus, priority: WorkspacePriority) -> WorkspaceTask {
    WorkspaceTask {
        id: id.to_string(),
        workspace_id: "main".to_string(),
        title: id.to_string(),
        task_type: WorkspaceTaskType::Thread,
        description: "Description".to_string(),
        definition_of_done: None,
        priority,
        status,
        sort_order: 1,
        reporter: WorkspaceActor::User,
        assignee: Some(WorkspaceActor::Agent("swarog".to_string())),
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
fn workspace_projection_filters_cached_tasks() {
    let mut state = WorkspaceState::new();
    state.set_tasks(
        "main".to_string(),
        vec![
            task(
                "todo-low",
                WorkspaceTaskStatus::Todo,
                WorkspacePriority::Low,
            ),
            task(
                "done-high",
                WorkspaceTaskStatus::Done,
                WorkspacePriority::High,
            ),
        ],
    );

    state.set_filter(WorkspaceFilter {
        status: Some(WorkspaceTaskStatus::Done),
        priority: Some(WorkspacePriority::High),
        ..WorkspaceFilter::default()
    });

    assert_eq!(state.projection().columns[0].tasks.len(), 0);
    assert_eq!(state.projection().columns[3].tasks.len(), 1);
    assert!(state
        .projection()
        .filter_summary
        .as_deref()
        .is_some_and(|summary| summary.contains("status:Done")));
}

#[test]
fn append_sort_order_uses_cached_tasks_for_target_status() {
    let mut state = WorkspaceState::new();
    let mut first = task(
        "first",
        WorkspaceTaskStatus::InProgress,
        WorkspacePriority::Low,
    );
    first.sort_order = 4;
    let mut second = task(
        "second",
        WorkspaceTaskStatus::InProgress,
        WorkspacePriority::Low,
    );
    second.sort_order = 7;
    state.set_tasks("main".to_string(), vec![first, second]);

    assert_eq!(state.append_sort_order(&WorkspaceTaskStatus::InProgress), 8);
    assert_eq!(state.append_sort_order(&WorkspaceTaskStatus::Done), 1);
}

#[test]
fn sort_order_for_drop_uses_target_task_order_or_appends() {
    let mut state = WorkspaceState::new();
    let mut first = task("first", WorkspaceTaskStatus::Todo, WorkspacePriority::Low);
    first.sort_order = 1;
    let mut second = task("second", WorkspaceTaskStatus::Todo, WorkspacePriority::Low);
    second.sort_order = 2;
    let mut third = task("third", WorkspaceTaskStatus::Todo, WorkspacePriority::Low);
    third.sort_order = 3;
    state.set_tasks("main".to_string(), vec![first, second, third]);

    assert_eq!(
        state.sort_order_for_drop("third", &WorkspaceTaskStatus::Todo, Some("second")),
        2
    );
    assert_eq!(
        state.sort_order_for_drop("third", &WorkspaceTaskStatus::Todo, None),
        4
    );
    assert!(state.drop_targets_same_task("third", &WorkspaceTaskStatus::Todo, Some("third")));
}

#[test]
fn drop_to_in_progress_requires_assignee_for_todo_task() {
    let mut state = WorkspaceState::new();
    let mut unassigned = task(
        "unassigned",
        WorkspaceTaskStatus::Todo,
        WorkspacePriority::Low,
    );
    unassigned.assignee = None;
    state.set_tasks(
        "main".to_string(),
        vec![
            unassigned,
            task(
                "assigned",
                WorkspaceTaskStatus::Todo,
                WorkspacePriority::Low,
            ),
        ],
    );

    assert!(state.drop_to_in_progress_run_blocked(
        "unassigned",
        Some(&WorkspaceTaskStatus::Todo),
        &WorkspaceTaskStatus::InProgress,
    ));
    assert!(!state.drop_to_in_progress_run_blocked(
        "assigned",
        Some(&WorkspaceTaskStatus::Todo),
        &WorkspaceTaskStatus::InProgress,
    ));
    assert!(!state.drop_to_in_progress_run_blocked(
        "unassigned",
        Some(&WorkspaceTaskStatus::Todo),
        &WorkspaceTaskStatus::InReview,
    ));
}

#[test]
fn workspace_detail_body_includes_task_fields_and_recent_notices() {
    let mut state = WorkspaceState::new();
    let mut task = task(
        "task-1",
        WorkspaceTaskStatus::InReview,
        WorkspacePriority::High,
    );
    task.title = "Ship workspace".to_string();
    task.description = "Build the board".to_string();
    task.definition_of_done = Some("Tests pass".to_string());
    state.set_tasks("main".to_string(), vec![task]);
    state.set_notices(vec![WorkspaceNotice {
        id: "notice-1".to_string(),
        workspace_id: "main".to_string(),
        task_id: "task-1".to_string(),
        notice_type: "review_failed".to_string(),
        message: "Needs tighter tests".to_string(),
        actor: Some(WorkspaceActor::User),
        created_at: 4,
    }]);

    let body = state.task_detail_body("task-1").expect("detail body");

    assert!(body.contains("Ship workspace"));
    assert!(body.contains("Description: Build the board"));
    assert!(body.contains("Definition of done: Tests pass"));
    assert!(body.contains("review_failed: Needs tighter tests"));
}

#[test]
fn upsert_notice_updates_cached_projection_notice_summary() {
    let mut state = WorkspaceState::new();
    state.set_tasks(
        "main".to_string(),
        vec![task(
            "task-1",
            WorkspaceTaskStatus::InProgress,
            WorkspacePriority::Low,
        )],
    );

    state.upsert_notice(WorkspaceNotice {
        id: "notice-1".to_string(),
        workspace_id: "main".to_string(),
        task_id: "task-1".to_string(),
        notice_type: "run_started".to_string(),
        message: "Run started".to_string(),
        actor: Some(WorkspaceActor::Agent("svarog".to_string())),
        created_at: 5,
    });

    assert_eq!(
        state
            .projection()
            .notice_summaries
            .get("task-1")
            .map(String::as_str),
        Some("run_started: Run started")
    );
}

#[test]
fn switch_workspace_resets_cached_tasks_notices_and_projection() {
    let mut state = WorkspaceState::new();
    state.set_tasks(
        "main".to_string(),
        vec![task(
            "task-1",
            WorkspaceTaskStatus::Todo,
            WorkspacePriority::Low,
        )],
    );
    state.set_notices(vec![WorkspaceNotice {
        id: "notice-1".to_string(),
        workspace_id: "main".to_string(),
        task_id: "task-1".to_string(),
        notice_type: "created".to_string(),
        message: "Created".to_string(),
        actor: Some(WorkspaceActor::User),
        created_at: 1,
    }]);

    state.switch_workspace("client-a");

    assert_eq!(state.workspace_id(), "client-a");
    assert_eq!(state.projection().workspace_id, "client-a");
    assert!(state
        .projection()
        .columns
        .iter()
        .all(|column| column.tasks.is_empty()));
    assert!(state.projection().notice_summaries.is_empty());
}

#[test]
fn review_task_id_is_derived_from_latest_review_notice() {
    let mut state = WorkspaceState::new();
    state.set_tasks(
        "main".to_string(),
        vec![task(
            "task-1",
            WorkspaceTaskStatus::InReview,
            WorkspacePriority::Low,
        )],
    );
    state.set_notices(vec![
        WorkspaceNotice {
            id: "notice-1".to_string(),
            workspace_id: "main".to_string(),
            task_id: "task-1".to_string(),
            notice_type: "review_requested".to_string(),
            message: "Workspace task review requested from subagent:qa; queued review task old"
                .to_string(),
            actor: Some(WorkspaceActor::Subagent("qa".to_string())),
            created_at: 1,
        },
        WorkspaceNotice {
            id: "notice-2".to_string(),
            workspace_id: "main".to_string(),
            task_id: "task-1".to_string(),
            notice_type: "review_requested".to_string(),
            message:
                "Workspace task review requested from subagent:qa; queued review task task-review-2"
                    .to_string(),
            actor: Some(WorkspaceActor::Subagent("qa".to_string())),
            created_at: 2,
        },
    ]);

    assert_eq!(
        state.review_task_id_for("task-1").as_deref(),
        Some("task-review-2")
    );
}

#[test]
fn review_task_id_falls_back_to_runtime_history_reviewer_task() {
    let mut state = WorkspaceState::new();
    let mut task = task(
        "task-1",
        WorkspaceTaskStatus::InReview,
        WorkspacePriority::Low,
    );
    task.runtime_history.push(WorkspaceTaskRuntimeHistoryEntry {
        task_type: WorkspaceTaskType::Thread,
        thread_id: None,
        goal_run_id: None,
        agent_task_id: Some("review-task-runtime".to_string()),
        source: Some("workspace_review".to_string()),
        title: Some("Review workspace task".to_string()),
        review_path: None,
        review_feedback: None,
        archived_at: 10,
    });
    state.set_tasks("main".to_string(), vec![task]);

    assert_eq!(
        state.review_task_id_for("task-1").as_deref(),
        Some("review-task-runtime")
    );
}
