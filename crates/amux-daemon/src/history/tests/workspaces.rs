use super::*;
use amux_protocol::{
    WorkspaceActor, WorkspaceNotice, WorkspaceOperator, WorkspacePriority, WorkspaceSettings,
    WorkspaceTask, WorkspaceTaskRuntimeHistoryEntry, WorkspaceTaskStatus, WorkspaceTaskType,
};

fn sample_task(id: &str, status: WorkspaceTaskStatus, sort_order: i64) -> WorkspaceTask {
    WorkspaceTask {
        id: id.to_string(),
        workspace_id: "workspace-main".to_string(),
        title: format!("Task {id}"),
        task_type: WorkspaceTaskType::Goal,
        description: "Deliver the workspace board".to_string(),
        definition_of_done: Some("Board state persists".to_string()),
        priority: WorkspacePriority::Low,
        status,
        sort_order,
        reporter: WorkspaceActor::User,
        assignee: Some(WorkspaceActor::Agent(
            amux_protocol::AGENT_ID_SWAROG.to_string(),
        )),
        reviewer: Some(WorkspaceActor::User),
        thread_id: None,
        goal_run_id: None,
        runtime_history: Vec::new(),
        created_at: 10,
        updated_at: 20,
        started_at: None,
        completed_at: None,
        deleted_at: None,
        last_notice_id: None,
    }
}

#[tokio::test]
async fn init_schema_adds_workspace_tables() -> Result<()> {
    let (store, root) = make_test_store().await?;

    let has_tables = store
        .conn
        .call(|conn| {
            let table_exists = |name: &str| -> rusqlite::Result<bool> {
                conn.query_row(
                    "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1)",
                    [name],
                    |row| row.get::<_, i64>(0),
                )
                .map(|value| value == 1)
            };

            Ok((
                table_exists("workspace_settings")?,
                table_exists("workspace_tasks")?,
                table_exists("workspace_notices")?,
            ))
        })
        .await
        .map_err(|error| anyhow::anyhow!("{error}"))?;

    assert_eq!(has_tables, (true, true, true));

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn workspace_settings_round_trip() -> Result<()> {
    let (store, root) = make_test_store().await?;
    let settings = WorkspaceSettings {
        workspace_id: "workspace-main".to_string(),
        workspace_root: Some("/tmp/workspace-main".to_string()),
        operator: WorkspaceOperator::Svarog,
        created_at: 1,
        updated_at: 2,
    };

    store.upsert_workspace_settings(&settings).await?;
    let loaded = store
        .get_workspace_settings("workspace-main")
        .await?
        .expect("settings should be stored");

    assert_eq!(loaded, settings);

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn workspace_tasks_round_trip_and_hide_deleted_by_default() -> Result<()> {
    let (store, root) = make_test_store().await?;
    let mut deleted = sample_task("wtask_deleted", WorkspaceTaskStatus::Todo, 20);
    deleted.deleted_at = Some(99);
    let mut visible = sample_task("wtask_visible", WorkspaceTaskStatus::Todo, 10);
    visible
        .runtime_history
        .push(WorkspaceTaskRuntimeHistoryEntry {
            task_type: WorkspaceTaskType::Thread,
            thread_id: Some("workspace-thread:old".to_string()),
            goal_run_id: None,
            agent_task_id: None,
            source: Some("workspace_runtime".to_string()),
            title: Some("Task wtask_visible".to_string()),
            review_path: Some("task-wtask_visible/failed-review.md".to_string()),
            review_feedback: Some("Needs tests".to_string()),
            archived_at: 30,
        });

    store.upsert_workspace_task(&visible).await?;
    store.upsert_workspace_task(&deleted).await?;

    let visible = store.list_workspace_tasks("workspace-main", false).await?;
    assert_eq!(visible.len(), 1);
    assert_eq!(visible[0].id, "wtask_visible");
    assert_eq!(visible[0].runtime_history.len(), 1);
    assert_eq!(
        visible[0].runtime_history[0].review_path.as_deref(),
        Some("task-wtask_visible/failed-review.md")
    );

    let all = store.list_workspace_tasks("workspace-main", true).await?;
    assert_eq!(all.len(), 2);
    assert_eq!(all[0].id, "wtask_visible");
    assert_eq!(all[1].id, "wtask_deleted");

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn workspace_tasks_order_by_status_then_sort_order() -> Result<()> {
    let (store, root) = make_test_store().await?;

    store
        .upsert_workspace_task(&sample_task(
            "wtask_review",
            WorkspaceTaskStatus::InReview,
            1,
        ))
        .await?;
    store
        .upsert_workspace_task(&sample_task("wtask_todo_2", WorkspaceTaskStatus::Todo, 20))
        .await?;
    store
        .upsert_workspace_task(&sample_task("wtask_todo_1", WorkspaceTaskStatus::Todo, 10))
        .await?;

    let tasks = store.list_workspace_tasks("workspace-main", false).await?;
    let ids = tasks.into_iter().map(|task| task.id).collect::<Vec<_>>();

    assert_eq!(ids, vec!["wtask_todo_1", "wtask_todo_2", "wtask_review"]);

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn workspace_task_max_sort_order_reads_only_target_status() -> Result<()> {
    let (store, root) = make_test_store().await?;
    let mut deleted_todo = sample_task("wtask_deleted", WorkspaceTaskStatus::Todo, 80);
    deleted_todo.deleted_at = Some(123);

    store.upsert_workspace_task(&deleted_todo).await?;
    store
        .upsert_workspace_task(&sample_task("wtask_todo_1", WorkspaceTaskStatus::Todo, 10))
        .await?;
    store
        .upsert_workspace_task(&sample_task("wtask_todo_2", WorkspaceTaskStatus::Todo, 40))
        .await?;
    store
        .upsert_workspace_task(&sample_task(
            "wtask_progress",
            WorkspaceTaskStatus::InProgress,
            90,
        ))
        .await?;

    assert_eq!(
        store
            .max_workspace_task_sort_order("workspace-main", WorkspaceTaskStatus::Todo, None)
            .await?,
        Some(40)
    );
    assert_eq!(
        store
            .max_workspace_task_sort_order(
                "workspace-main",
                WorkspaceTaskStatus::Todo,
                Some("wtask_todo_2"),
            )
            .await?,
        Some(10)
    );
    assert_eq!(
        store
            .max_workspace_task_sort_order("workspace-main", WorkspaceTaskStatus::InProgress, None)
            .await?,
        Some(90)
    );

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn workspace_notices_round_trip_for_task() -> Result<()> {
    let (store, root) = make_test_store().await?;
    let notice = WorkspaceNotice {
        id: "wnotice_1".to_string(),
        workspace_id: "workspace-main".to_string(),
        task_id: "wtask_1".to_string(),
        notice_type: "review_failed".to_string(),
        message: "Add tests for the board projection".to_string(),
        actor: Some(WorkspaceActor::User),
        created_at: 42,
    };

    store.insert_workspace_notice(&notice).await?;

    let loaded = store
        .list_workspace_notices("workspace-main", Some("wtask_1"))
        .await?;

    assert_eq!(loaded, vec![notice]);

    fs::remove_dir_all(root)?;
    Ok(())
}
