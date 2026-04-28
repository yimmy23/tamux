use super::*;
use crate::history::schema_helpers::table_has_column;
use std::fs;

#[tokio::test]
async fn init_schema_adds_event_log_table() -> Result<()> {
    let (store, root) = make_test_store().await?;

    store
        .conn
        .call(|conn| {
            conn.execute_batch("DROP TABLE IF EXISTS event_log;")?;
            Ok(())
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    store.init_schema().await?;

    let status = store
        .conn
        .call(|conn| {
            let has_event_family = table_has_column(conn, "event_log", "event_family")?;
            let has_payload = table_has_column(conn, "event_log", "payload_json")?;
            let log_index: Option<String> = conn
                .query_row(
                    "SELECT name FROM sqlite_master WHERE type = 'index' AND name = 'idx_event_log_family_kind_ts'",
                    [],
                    |row| row.get(0),
                )
                .optional()?;
            Ok((has_event_family, has_payload, log_index))
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    assert!(status.0);
    assert!(status.1);
    assert_eq!(status.2.as_deref(), Some("idx_event_log_family_kind_ts"));

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn event_log_round_trips_runtime_event_rows() -> Result<()> {
    let (store, root) = make_test_store().await?;

    store
        .insert_event_log(&EventLogRow {
            id: "event-log-1".to_string(),
            event_family: "filesystem".to_string(),
            event_kind: "file_changed".to_string(),
            state: Some("detected".to_string()),
            thread_id: Some("thread-1".to_string()),
            payload_json: serde_json::json!({
                "path": "src/main.rs",
                "agent_id": "weles"
            })
            .to_string(),
            risk_label: "low".to_string(),
            handled_at_ms: 100,
        })
        .await?;

    let rows = store
        .list_event_log(Some("filesystem"), Some("file_changed"), 4)
        .await?;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].id, "event-log-1");
    assert_eq!(rows[0].thread_id.as_deref(), Some("thread-1"));
    assert_eq!(rows[0].risk_label, "low");

    fs::remove_dir_all(root)?;
    Ok(())
}
