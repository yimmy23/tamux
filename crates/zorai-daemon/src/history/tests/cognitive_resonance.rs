use super::*;
use crate::history::schema_helpers::table_has_column;
use std::fs;

#[tokio::test]
async fn init_schema_adds_cognitive_resonance_tables() -> Result<()> {
    let (store, root) = make_test_store().await?;

    store
        .conn
        .call(|conn| {
            conn.execute_batch(
                "
                DROP TABLE IF EXISTS behavior_adjustments_log;
                DROP TABLE IF EXISTS cognitive_resonance_samples;
                ",
            )?;
            Ok(())
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    store.init_schema().await?;

    async fn index_name(
        conn: &dyn crate::history::db::DbConn,
        name: &str,
    ) -> Result<Option<String>> {
        Ok(conn
            .query_opt(
                &format!("SELECT name FROM sqlite_master WHERE type = 'index' AND name = '{name}'"),
                crate::history::db::Params::None,
            )
            .await?
            .map(|row| row.get::<String>(0))
            .transpose()?)
    }
    let mut exec = crate::history::db::ConnExecutor(&*store.read_db);
    let has_state =
        table_has_column(&mut exec, "cognitive_resonance_samples", "cognitive_state").await?;
    let has_score =
        table_has_column(&mut exec, "cognitive_resonance_samples", "resonance_score").await?;
    let has_parameter =
        table_has_column(&mut exec, "behavior_adjustments_log", "parameter").await?;
    let status = (
        has_state,
        has_score,
        index_name(&*store.read_db, "idx_cognitive_resonance_samples_sampled").await?,
        has_parameter,
        index_name(&*store.read_db, "idx_behavior_adjustments_log_adjusted").await?,
    );

    assert!(status.0);
    assert!(status.1);
    assert_eq!(
        status.2.as_deref(),
        Some("idx_cognitive_resonance_samples_sampled")
    );
    assert!(status.3);
    assert_eq!(
        status.4.as_deref(),
        Some("idx_behavior_adjustments_log_adjusted")
    );

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn cognitive_resonance_round_trips_samples_and_adjustments() -> Result<()> {
    let (store, root) = make_test_store().await?;

    store
        .insert_cognitive_resonance_sample(&CognitiveResonanceSampleRow {
            id: None,
            sampled_at_ms: 100,
            revision_velocity_ms: Some(800),
            session_entropy: Some(0.42),
            approval_latency_ms: Some(4_000),
            tool_hesitation_count: 2,
            cognitive_state: "frustrated".to_string(),
            state_confidence: 0.74,
            resonance_score: 0.28,
            verbosity_adjustment: 0.22,
            risk_adjustment: 0.18,
            proactiveness_adjustment: 0.15,
            memory_urgency_adjustment: 0.81,
        })
        .await?;
    store
        .insert_behavior_adjustment_log(&BehaviorAdjustmentLogRow {
            id: None,
            adjusted_at_ms: 101,
            parameter: "verbosity".to_string(),
            old_value: 0.62,
            new_value: 0.22,
            trigger_reason: "frustrated".to_string(),
            resonance_score: 0.28,
        })
        .await?;

    let samples = store.list_cognitive_resonance_samples(4).await?;
    assert_eq!(samples.len(), 1);
    assert_eq!(samples[0].cognitive_state, "frustrated");
    assert_eq!(samples[0].tool_hesitation_count, 2);

    let logs = store.list_behavior_adjustment_log(4).await?;
    assert_eq!(logs.len(), 1);
    assert_eq!(logs[0].parameter, "verbosity");
    assert_eq!(logs[0].new_value, 0.22);

    fs::remove_dir_all(root)?;
    Ok(())
}
