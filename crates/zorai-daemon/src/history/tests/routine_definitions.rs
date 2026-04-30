use super::*;
use crate::history::schema_helpers::table_has_column;

#[tokio::test]
async fn init_schema_adds_routine_definitions_table() -> Result<()> {
    let (store, root) = make_test_store().await?;

    store
        .conn
        .call(|conn| {
            conn.execute_batch(
                "
                DROP TABLE IF EXISTS routine_runs;
                DROP TABLE IF EXISTS routine_definitions;
                ",
            )?;
            Ok(())
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    store.init_schema().await?;

    let status = store
        .conn
        .call(|conn| {
            let has_title = table_has_column(conn, "routine_definitions", "title")?;
            let has_description = table_has_column(conn, "routine_definitions", "description")?;
            let has_enabled = table_has_column(conn, "routine_definitions", "enabled")?;
            let has_paused_at = table_has_column(conn, "routine_definitions", "paused_at")?;
            let has_schedule_expression =
                table_has_column(conn, "routine_definitions", "schedule_expression")?;
            let has_target_kind = table_has_column(conn, "routine_definitions", "target_kind")?;
            let has_target_payload_json =
                table_has_column(conn, "routine_definitions", "target_payload_json")?;
            let has_next_run_at = table_has_column(conn, "routine_definitions", "next_run_at")?;
            let has_last_run_at = table_has_column(conn, "routine_definitions", "last_run_at")?;
            let has_schema_version = table_has_column(conn, "routine_definitions", "schema_version")?;
            let has_last_result = table_has_column(conn, "routine_definitions", "last_result")?;
            let has_last_error = table_has_column(conn, "routine_definitions", "last_error")?;
            let has_last_success_summary =
                table_has_column(conn, "routine_definitions", "last_success_summary")?;
            let has_routine_runs = table_has_column(conn, "routine_runs", "routine_id")?;
            let routine_index: Option<String> = conn
                .query_row(
                    "SELECT name FROM sqlite_master WHERE type = 'index' AND name = 'idx_routine_definitions_enabled_next_run'",
                    [],
                    |row| row.get(0),
                )
                .optional()?;
            Ok((
                has_title,
                has_description,
                has_enabled,
                has_paused_at,
                has_schedule_expression,
                has_target_kind,
                has_target_payload_json,
                has_next_run_at,
                has_last_run_at,
                has_schema_version,
                has_last_result,
                has_last_error,
                has_last_success_summary,
                has_routine_runs,
                routine_index,
            ))
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    assert_eq!(status.9, true);
    assert_eq!(status.10, true);
    assert_eq!(status.11, true);
    assert_eq!(status.12, true);
    assert_eq!(status.13, true);
    assert_eq!(
        status.14.as_deref(),
        Some("idx_routine_definitions_enabled_next_run")
    );

    std::fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn routine_definitions_round_trip_create_list_read() -> Result<()> {
    let (store, root) = make_test_store().await?;

    let row = RoutineDefinitionRow {
        id: "routine-daily-brief".to_string(),
        title: "Daily brief".to_string(),
        description: "Send a daily project brief".to_string(),
        enabled: true,
        paused_at: None,
        schedule_expression: "0 9 * * *".to_string(),
        target_kind: "task".to_string(),
        target_payload_json: serde_json::json!({
            "description": "Prepare daily brief",
            "priority": "normal"
        })
        .to_string(),
        schema_version: 1,
        next_run_at: Some(1_800),
        last_run_at: Some(900),
        last_result: Some("success".to_string()),
        last_error: None,
        last_success_summary: Some("Enqueued task task_123 (Daily brief)".to_string()),
        created_at: 100,
        updated_at: 120,
    };

    store.upsert_routine_definition(&row).await?;

    let rows = store.list_routine_definitions().await?;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].id, "routine-daily-brief");
    assert_eq!(rows[0].title, "Daily brief");
    assert_eq!(rows[0].description, "Send a daily project brief");
    assert!(rows[0].enabled);
    assert_eq!(rows[0].paused_at, None);
    assert_eq!(rows[0].schedule_expression, "0 9 * * *");
    assert_eq!(rows[0].target_kind, "task");
    assert!(rows[0].target_payload_json.contains("Prepare daily brief"));
    assert_eq!(rows[0].next_run_at, Some(1_800));
    assert_eq!(rows[0].last_run_at, Some(900));
    assert_eq!(rows[0].schema_version, 1);
    assert_eq!(rows[0].last_result.as_deref(), Some("success"));
    assert_eq!(
        rows[0].last_success_summary.as_deref(),
        Some("Enqueued task task_123 (Daily brief)")
    );

    let loaded = store
        .get_routine_definition("routine-daily-brief")
        .await?
        .expect("routine should exist after upsert");
    assert_eq!(loaded.id, row.id);
    assert_eq!(loaded.title, row.title);
    assert_eq!(loaded.description, row.description);
    assert_eq!(loaded.enabled, row.enabled);
    assert_eq!(loaded.paused_at, row.paused_at);
    assert_eq!(loaded.schedule_expression, row.schedule_expression);
    assert_eq!(loaded.target_kind, row.target_kind);
    assert_eq!(loaded.target_payload_json, row.target_payload_json);
    assert_eq!(loaded.schema_version, row.schema_version);
    assert_eq!(loaded.next_run_at, row.next_run_at);
    assert_eq!(loaded.last_run_at, row.last_run_at);
    assert_eq!(loaded.last_result, row.last_result);
    assert_eq!(loaded.last_error, row.last_error);
    assert_eq!(loaded.last_success_summary, row.last_success_summary);
    assert_eq!(loaded.created_at, row.created_at);
    assert_eq!(loaded.updated_at, row.updated_at);

    std::fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn routine_runs_round_trip_append_and_list() -> Result<()> {
    let (store, root) = make_test_store().await?;

    let run = RoutineRunRow {
        id: "routine-run-1".to_string(),
        routine_id: "routine-daily-brief".to_string(),
        trigger_kind: "run_now".to_string(),
        status: "success".to_string(),
        started_at: 1_000,
        finished_at: Some(1_050),
        created_task_id: Some("task-1".to_string()),
        created_goal_run_id: None,
        payload_json: serde_json::json!({
            "target_kind": "task",
            "materialized_payload": {
                "title": "Daily brief",
                "description": "Prepare the daily brief"
            }
        })
        .to_string(),
        result_summary: Some("Enqueued task task-1 (Daily brief)".to_string()),
        error: None,
        rerun_of_run_id: None,
    };

    store.append_routine_run(&run).await?;

    let listed = store.list_routine_runs("routine-daily-brief", 10).await?;
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].id, run.id);
    assert_eq!(listed[0].trigger_kind, "run_now");
    assert_eq!(listed[0].created_task_id.as_deref(), Some("task-1"));

    let loaded = store
        .get_routine_run("routine-run-1")
        .await?
        .expect("routine run should exist");
    assert_eq!(loaded.id, run.id);
    assert_eq!(loaded.status, run.status);
    assert_eq!(loaded.result_summary, run.result_summary);

    std::fs::remove_dir_all(root)?;
    Ok(())
}
