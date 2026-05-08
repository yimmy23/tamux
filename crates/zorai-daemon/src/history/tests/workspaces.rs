use super::*;
use zorai_protocol::{
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
            zorai_protocol::AGENT_ID_SWAROG.to_string(),
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
        repo_monitor_enabled: true,
        repo_monitor_include_dirs: vec![
            "frontend/src".to_string(),
            "crates/zorai-daemon".to_string(),
        ],
        repo_monitor_exclude_dirs: vec!["target".to_string()],
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
async fn workspace_settings_operator_filter_ignores_unrelated_malformed_rows() -> Result<()> {
    let (store, root) = make_test_store().await?;

    store
        .conn
        .call(|conn| {
            conn.execute(
                "INSERT INTO workspace_settings \
                 (workspace_id, workspace_root, operator, repo_monitor_enabled, repo_monitor_include_dirs_json, repo_monitor_exclude_dirs_json, created_at, updated_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                rusqlite::params![
                    "workspace-svarog",
                    "/tmp/workspace-svarog",
                    "svarog",
                    1i64,
                    "[\"frontend\"]",
                    "[\"target\"]",
                    1i64,
                    2i64
                ],
            )?;
            conn.execute(
                "INSERT INTO workspace_settings \
                 (workspace_id, workspace_root, operator, repo_monitor_enabled, repo_monitor_include_dirs_json, repo_monitor_exclude_dirs_json, created_at, updated_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                rusqlite::params![
                    "workspace-user",
                    "/tmp/workspace-user",
                    "user",
                    0i64,
                    "[]",
                    "[]",
                    1i64,
                    "not-an-integer"
                ],
            )?;
            Ok(())
        })
        .await
        .map_err(|error| anyhow::anyhow!("{error}"))?;

    let settings = store
        .list_workspace_settings_by_operator(WorkspaceOperator::Svarog)
        .await?;

    assert_eq!(settings.len(), 1);
    assert_eq!(settings[0].workspace_id, "workspace-svarog");

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
async fn list_assigned_workspace_tasks_by_status_filters_in_sql() -> Result<()> {
    let (store, root) = make_test_store().await?;

    let assigned_todo = sample_task("wtask_assigned_todo", WorkspaceTaskStatus::Todo, 20);
    let mut unassigned_todo = sample_task("wtask_unassigned_todo", WorkspaceTaskStatus::Todo, 10);
    unassigned_todo.assignee = None;
    let assigned_running = sample_task(
        "wtask_assigned_running",
        WorkspaceTaskStatus::InProgress,
        30,
    );
    let mut deleted_assigned_todo =
        sample_task("wtask_deleted_assigned_todo", WorkspaceTaskStatus::Todo, 40);
    deleted_assigned_todo.deleted_at = Some(99);

    store.upsert_workspace_task(&unassigned_todo).await?;
    store.upsert_workspace_task(&assigned_todo).await?;
    store.upsert_workspace_task(&assigned_running).await?;
    store.upsert_workspace_task(&deleted_assigned_todo).await?;

    let tasks = store
        .list_assigned_workspace_tasks_by_status("workspace-main", WorkspaceTaskStatus::Todo)
        .await?;

    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0].id, "wtask_assigned_todo");

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn list_workspace_tasks_for_sort_shift_filters_status_and_sort_order_in_sql() -> Result<()> {
    let (store, root) = make_test_store().await?;

    store
        .upsert_workspace_task(&sample_task(
            "wtask_todo_before",
            WorkspaceTaskStatus::Todo,
            10,
        ))
        .await?;
    store
        .upsert_workspace_task(&sample_task(
            "wtask_todo_moving",
            WorkspaceTaskStatus::Todo,
            20,
        ))
        .await?;
    store
        .upsert_workspace_task(&sample_task(
            "wtask_todo_shift",
            WorkspaceTaskStatus::Todo,
            30,
        ))
        .await?;
    store
        .upsert_workspace_task(&sample_task(
            "wtask_progress_after",
            WorkspaceTaskStatus::InProgress,
            40,
        ))
        .await?;
    let mut deleted_after = sample_task("wtask_deleted_after", WorkspaceTaskStatus::Todo, 50);
    deleted_after.deleted_at = Some(99);
    store.upsert_workspace_task(&deleted_after).await?;

    let tasks = store
        .list_workspace_tasks_for_sort_shift(
            "workspace-main",
            WorkspaceTaskStatus::Todo,
            "wtask_todo_moving",
            20,
        )
        .await?;

    assert_eq!(
        tasks.into_iter().map(|task| task.id).collect::<Vec<_>>(),
        vec!["wtask_todo_shift"]
    );

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

#[tokio::test]
async fn list_workspace_notices_limited_applies_task_filter_and_limit_in_sql() -> Result<()> {
    let (store, root) = make_test_store().await?;
    for index in 0..3 {
        store
            .insert_workspace_notice(&WorkspaceNotice {
                id: format!("wnotice_{index}"),
                workspace_id: "workspace-main".to_string(),
                task_id: "wtask_1".to_string(),
                notice_type: "status".to_string(),
                message: format!("Notice {index}"),
                actor: Some(WorkspaceActor::User),
                created_at: 40 + index,
            })
            .await?;
    }
    store
        .insert_workspace_notice(&WorkspaceNotice {
            id: "wnotice_other_task".to_string(),
            workspace_id: "workspace-main".to_string(),
            task_id: "wtask_2".to_string(),
            notice_type: "status".to_string(),
            message: "Other task".to_string(),
            actor: Some(WorkspaceActor::User),
            created_at: 39,
        })
        .await?;

    let notices = store
        .list_workspace_notices_limited("workspace-main", Some("wtask_1"), 2)
        .await?;

    assert_eq!(
        notices
            .into_iter()
            .map(|notice| notice.id)
            .collect::<Vec<_>>(),
        vec!["wnotice_0", "wnotice_1"]
    );

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn workspace_notice_exists_filters_task_and_type_in_sql() -> Result<()> {
    let (store, root) = make_test_store().await?;
    let completion = WorkspaceNotice {
        id: "wnotice_completion".to_string(),
        workspace_id: "workspace-main".to_string(),
        task_id: "wtask_1".to_string(),
        notice_type: "task_completion".to_string(),
        message: "Done".to_string(),
        actor: Some(WorkspaceActor::User),
        created_at: 42,
    };
    let review = WorkspaceNotice {
        id: "wnotice_review".to_string(),
        workspace_id: "workspace-main".to_string(),
        task_id: "wtask_1".to_string(),
        notice_type: "review_failed".to_string(),
        message: "Add tests".to_string(),
        actor: Some(WorkspaceActor::User),
        created_at: 43,
    };
    store.insert_workspace_notice(&review).await?;
    store.insert_workspace_notice(&completion).await?;

    assert!(
        store
            .workspace_notice_exists("workspace-main", "wtask_1", "task_completion")
            .await?
    );
    assert!(
        !store
            .workspace_notice_exists("workspace-main", "wtask_other", "task_completion")
            .await?
    );
    assert!(
        !store
            .workspace_notice_exists("workspace-main", "wtask_1", "handoff")
            .await?
    );

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn workspace_notice_with_message_exists_filters_exact_message_in_sql() -> Result<()> {
    let (store, root) = make_test_store().await?;
    let notice = WorkspaceNotice {
        id: "wnotice_runtime_failed".to_string(),
        workspace_id: "workspace-main".to_string(),
        task_id: "wtask_1".to_string(),
        notice_type: "runtime_failed".to_string(),
        message: "Workspace goal failed".to_string(),
        actor: Some(WorkspaceActor::User),
        created_at: 42,
    };
    store.insert_workspace_notice(&notice).await?;

    assert!(
        store
            .workspace_notice_with_message_exists(
                "workspace-main",
                "wtask_1",
                "runtime_failed",
                "Workspace goal failed",
            )
            .await?
    );
    assert!(
        !store
            .workspace_notice_with_message_exists(
                "workspace-main",
                "wtask_1",
                "runtime_failed",
                "Different message",
            )
            .await?
    );

    fs::remove_dir_all(root)?;
    Ok(())
}
