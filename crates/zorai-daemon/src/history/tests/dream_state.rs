use super::*;
use crate::history::schema_helpers::table_has_column;
use std::fs;

#[tokio::test]
async fn init_schema_adds_dream_state_tables() -> Result<()> {
    let (store, root) = make_test_store().await?;

    store
        .conn
        .call(|conn| {
            conn.execute_batch(
                "
                DROP TABLE IF EXISTS counterfactual_evaluations;
                DROP TABLE IF EXISTS dream_cycles;
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
            let has_idle_duration = table_has_column(conn, "dream_cycles", "idle_duration_ms")?;
            let has_status = table_has_column(conn, "dream_cycles", "status")?;
            let cycle_index: Option<String> = conn
                .query_row(
                    "SELECT name FROM sqlite_master WHERE type = 'index' AND name = 'idx_dream_cycles_started'",
                    [],
                    |row| row.get(0),
                )
                .optional()?;
            let has_variation_type =
                table_has_column(conn, "counterfactual_evaluations", "variation_type")?;
            let eval_index: Option<String> = conn
                .query_row(
                    "SELECT name FROM sqlite_master WHERE type = 'index' AND name = 'idx_counterfactual_evaluations_cycle'",
                    [],
                    |row| row.get(0),
                )
                .optional()?;
            Ok((
                has_idle_duration,
                has_status,
                cycle_index,
                has_variation_type,
                eval_index,
            ))
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    assert!(status.0);
    assert!(status.1);
    assert_eq!(status.2.as_deref(), Some("idx_dream_cycles_started"));
    assert!(status.3);
    assert_eq!(
        status.4.as_deref(),
        Some("idx_counterfactual_evaluations_cycle")
    );

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn dream_state_round_trips_cycle_and_counterfactual_evaluations() -> Result<()> {
    let (store, root) = make_test_store().await?;

    let cycle_id = store
        .insert_dream_cycle(&DreamCycleRow {
            id: None,
            started_at_ms: 100,
            completed_at_ms: Some(180),
            idle_duration_ms: 30_000,
            tasks_analyzed: 4,
            counterfactuals_generated: 7,
            counterfactuals_successful: 2,
            status: "completed".to_string(),
        })
        .await?;

    store
        .insert_counterfactual_evaluation(&CounterfactualEvaluationRow {
            id: None,
            dream_cycle_id: cycle_id,
            source_task_id: "task-1".to_string(),
            variation_type: "tool_substitution".to_string(),
            counterfactual_description: "Use read_file before bash cat".to_string(),
            estimated_token_saving: Some(42.0),
            estimated_time_saving_ms: Some(900),
            estimated_revision_reduction: Some(1),
            score: 0.87,
            threshold_met: true,
            created_at_ms: 140,
        })
        .await?;

    let cycles = store.list_dream_cycles(8).await?;
    assert_eq!(cycles.len(), 1);
    assert_eq!(cycles[0].id, Some(cycle_id));
    assert_eq!(cycles[0].status, "completed");

    let evaluations = store.list_counterfactual_evaluations(cycle_id).await?;
    assert_eq!(evaluations.len(), 1);
    assert_eq!(evaluations[0].source_task_id, "task-1");
    assert!(evaluations[0].threshold_met);
    assert_eq!(evaluations[0].estimated_revision_reduction, Some(1));

    fs::remove_dir_all(root)?;
    Ok(())
}
