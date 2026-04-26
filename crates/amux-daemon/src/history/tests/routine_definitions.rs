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
                routine_index,
            ))
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    assert!(status.0);
    assert!(status.1);
    assert!(status.2);
    assert!(status.3);
    assert!(status.4);
    assert!(status.5);
    assert!(status.6);
    assert!(status.7);
    assert!(status.8);
    assert_eq!(
        status.9.as_deref(),
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
        next_run_at: Some(1_800),
        last_run_at: Some(900),
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
    assert_eq!(loaded.next_run_at, row.next_run_at);
    assert_eq!(loaded.last_run_at, row.last_run_at);
    assert_eq!(loaded.created_at, row.created_at);
    assert_eq!(loaded.updated_at, row.updated_at);

    std::fs::remove_dir_all(root)?;
    Ok(())
}
