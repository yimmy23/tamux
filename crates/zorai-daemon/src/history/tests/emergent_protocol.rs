use super::*;
use crate::history::schema_helpers::table_has_column;
use std::fs;

#[tokio::test]
async fn init_schema_adds_emergent_protocol_registry_tables() -> Result<()> {
    let (store, root) = make_test_store().await?;

    store
        .conn
        .call(|conn| {
            conn.execute_batch(
                "
                DROP TABLE IF EXISTS protocol_usage_log;
                DROP TABLE IF EXISTS protocol_steps;
                DROP TABLE IF EXISTS emergent_protocols;
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
            let has_token = table_has_column(conn, "emergent_protocols", "token")?;
            let has_pattern = table_has_column(conn, "emergent_protocols", "normalized_pattern")?;
            let has_signal_kind = table_has_column(conn, "emergent_protocols", "signal_kind")?;
            let protocol_index: Option<String> = conn
                .query_row(
                    "SELECT name FROM sqlite_master WHERE type = 'index' AND name = 'idx_emergent_protocols_thread_activated'",
                    [],
                    |row| row.get(0),
                )
                .optional()?;
            let has_step_intent = table_has_column(conn, "protocol_steps", "intent")?;
            let step_index: Option<String> = conn
                .query_row(
                    "SELECT name FROM sqlite_master WHERE type = 'index' AND name = 'idx_protocol_steps_protocol'",
                    [],
                    |row| row.get(0),
                )
                .optional()?;
            let has_usage_success = table_has_column(conn, "protocol_usage_log", "success")?;
            let usage_index: Option<String> = conn
                .query_row(
                    "SELECT name FROM sqlite_master WHERE type = 'index' AND name = 'idx_protocol_usage_log_protocol_used'",
                    [],
                    |row| row.get(0),
                )
                .optional()?;
            Ok((
                has_token,
                has_pattern,
                has_signal_kind,
                protocol_index,
                has_step_intent,
                step_index,
                has_usage_success,
                usage_index,
            ))
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    assert!(status.0);
    assert!(status.1);
    assert!(status.2);
    assert_eq!(
        status.3.as_deref(),
        Some("idx_emergent_protocols_thread_activated")
    );
    assert!(status.4);
    assert_eq!(status.5.as_deref(), Some("idx_protocol_steps_protocol"));
    assert!(status.6);
    assert_eq!(
        status.7.as_deref(),
        Some("idx_protocol_usage_log_protocol_used")
    );

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn emergent_protocol_registry_round_trips_steps_and_usage() -> Result<()> {
    let (store, root) = make_test_store().await?;

    let row = EmergentProtocolRow {
        protocol_id: "proto_reg_1".to_string(),
        token: "@proto_deadbeef".to_string(),
        description: "Accepted emergent protocol".to_string(),
        agent_a: "user".to_string(),
        agent_b: "assistant".to_string(),
        thread_id: "thread-ep-1".to_string(),
        normalized_pattern: "continue".to_string(),
        signal_kind: "repeated_continuation_cue".to_string(),
        context_signature_json: serde_json::json!({
            "thread_id": "thread-ep-1",
            "normalized_pattern": "continue",
            "trigger_phrase": "continue",
            "signal_kind": "repeated_continuation_cue",
            "source_role": "user",
            "target_role": "assistant"
        })
        .to_string(),
        created_at: 100,
        activated_at: 200,
        last_used_at: Some(250),
        usage_count: 2,
        success_rate: 0.75,
        source_candidate_id: Some("cand-1".to_string()),
    };
    store.upsert_emergent_protocol(&row).await?;
    store
        .replace_protocol_steps(
            "proto_reg_1",
            &[
                ProtocolStepRow {
                    protocol_id: "proto_reg_1".to_string(),
                    step_index: 0,
                    intent: "expand token".to_string(),
                    tool_name: None,
                    args_template_json: serde_json::json!({"normalized_pattern": "continue"})
                        .to_string(),
                },
                ProtocolStepRow {
                    protocol_id: "proto_reg_1".to_string(),
                    step_index: 1,
                    intent: "continue execution".to_string(),
                    tool_name: Some("message_agent".to_string()),
                    args_template_json: serde_json::json!({"target": "svarog"}).to_string(),
                },
            ],
        )
        .await?;
    store
        .insert_protocol_usage_log(&ProtocolUsageLogRow {
            id: "usage-1".to_string(),
            protocol_id: "proto_reg_1".to_string(),
            used_at: 300,
            execution_time_ms: Some(25),
            success: true,
            fallback_reason: None,
        })
        .await?;

    let loaded = store
        .get_emergent_protocol_by_token("@proto_deadbeef")
        .await?
        .expect("protocol row should load");
    assert_eq!(loaded.thread_id, "thread-ep-1");
    assert_eq!(loaded.normalized_pattern, "continue");
    assert_eq!(loaded.source_candidate_id.as_deref(), Some("cand-1"));

    let steps = store.list_protocol_steps("proto_reg_1").await?;
    assert_eq!(steps.len(), 2);
    assert_eq!(steps[0].step_index, 0);
    assert_eq!(steps[1].tool_name.as_deref(), Some("message_agent"));

    let usage = store.list_protocol_usage_log("proto_reg_1").await?;
    assert_eq!(usage.len(), 1);
    assert!(usage[0].success);
    assert_eq!(usage[0].execution_time_ms, Some(25));

    fs::remove_dir_all(root)?;
    Ok(())
}
